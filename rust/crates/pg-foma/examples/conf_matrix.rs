//! Full backend scoreboard (`pg.ps1 -Mode run -Example conf_matrix`): every discovered conformance
//! fixture x every `EmissionStrategy`, measured directly against the same run-scoped
//! `RunEvaluationCache` per fixture. Per `docs/research/backend-measurement-instruments.md`:
//! constructing a `LoweredCandidate` with `LoweringAdapter::for_strategy(...)` and calling
//! `evaluate_plans_observed_with_cache` BYPASSES `select_backends`, so a capability-refused backend
//! still compiles and runs here rather than reading as a pre-filtered refusal.
//!
//! Measurement only -- no production behaviour changes. Run in the foreground or background per
//! `pg.ps1 -Mode run -Example conf_matrix`; never as a `#[test]` (nextest's 10-minute kill would
//! truncate a full sweep silently).

use std::collections::BTreeMap;

use pg_conformance_fixtures::{discover, FixtureRef};
use pg_foma::backend_optimizer::Certification;
use pg_foma::backend_runtime::{
    evaluate_plans_observed_with_cache, RunEvaluationCache, RuntimeBudget,
};
use pg_foma::enumerate::{enumerate_default, CandidateRole, EmissionStrategy, LoweredCandidate};
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::lowering_adapter::LoweringAdapter;
use pg_foma::parity::IdentityDivergence;
use pg_foma::strategy_coverage::ALL_STRATEGIES;
use pg_grammar::model::Grammar;

/// Per-fixture word cap. Measured 2026-08-29 (see report): the largest fixture on disk today is 63
/// words (`machine/languages/fusional-realizational-morphology`), well under this. It exists for a
/// future larger corpus, not because one was observed at this size -- and any fixture it actually
/// truncates is labeled in that fixture's own output, never silently.
const MAX_WORDS_PER_FIXTURE: usize = 200;

/// One strategy's measured outcome for one fixture.
struct StrategyRow {
    strategy: EmissionStrategy,
    /// `Ok(())` = the network built; `Err(reason)` names the typed refusal/failure.
    compiles: Result<(), String>,
    certification_debug: String,
    /// `Certification::FullHcConfirmed` -- every comparable word's confirmed output matched the
    /// oracle's identity set exactly.
    exact: bool,
    /// ADR-0001 defect: identities the oracle found that the CONFIRMED output missed.
    recall_oracle_only: u64,
    /// Real defect regardless of ADR-0001: identities the CONFIRMED output has that the oracle does not.
    soundness_candidate_only: u64,
    /// Informational, legal under ADR-0001: raw pre-confirm proposals (by admission key) that did not
    /// survive into the confirmed output.
    legal_overgeneration: u64,
    /// Words evidence reached (`Some`) vs never reached (`None` -> `could_not_measure` names why).
    words_measured: Option<usize>,
    could_not_measure: Option<String>,
}

struct FixtureRow {
    label: String,
    total_words: usize,
    measured_words: usize,
    subsampled: bool,
    excluded: Vec<(String, String)>,
    expect_fail_count: usize,
    expect_skip_count: usize,
    strategies: Vec<StrategyRow>,
    /// `None` = the fixture itself could not be measured at all (load failure, empty corpus, oracle
    /// preparation fault) -- distinct from every strategy individually failing.
    exact_count: Option<usize>,
}

fn main() {
    // `discover` panics unless a run claims a scope; `all` reaches both fixture roots.
    std::env::set_var("PANGLOSS_CONFORMANCE_SCOPE", "all");

    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "<git rev-parse HEAD failed -- read it separately>".to_string());
    println!("commit under measurement: {commit}");
    println!(
        "scope: PANGLOSS_CONFORMANCE_SCOPE=all (conformance-staging/** + machine/conformance/**)"
    );
    println!(
        "strategies measured: {:?} -- capability selector BYPASSED (LoweredCandidate constructed \
         directly, evaluate_plans_observed_with_cache never calls select_backends), so a backend \
         `select_backends` would refuse ahead-of-time is still compiled and measured here",
        ALL_STRATEGIES
    );
    println!(
        "per-fixture word cap: {MAX_WORDS_PER_FIXTURE} (first-N subsample if a fixture exceeds \
         this; labeled on that fixture's own row when it applies -- no fixture on disk triggers it \
         today)"
    );
    println!(
        "\"compiles\" below means the candidate's network was actually built: Certification::\
         {{CapabilityRejected,BuildFailed,Unsupported}} = no; every other certification variant \
         (Truncated/ResourceBreach/IdentityMismatch/FullHcConfirmed/EstimateOnly) = yes, since the \
         network was built even if the corpus-level verdict is not FullHcConfirmed.\n"
    );

    let fixtures = discover();
    println!("discovered {} fixtures under scope=all\n", fixtures.len());

    let mut rows: Vec<FixtureRow> = Vec::with_capacity(fixtures.len());
    let mut dist = [0usize; 4]; // index = count of strategies FullHcConfirmed (0..=3)
    let mut unmeasurable = 0usize;

    for fixture in &fixtures {
        let row = run_fixture(fixture);
        match row.exact_count {
            Some(n) => dist[n] += 1,
            None => unmeasurable += 1,
        }
        // Running tally after every fixture, so a killed run still leaves a legible partial headline.
        println!(
            "  [running tally after {} fixture(s): 3/3={} 2/3={} 1/3={} 0/3={} unmeasurable={}]\n",
            rows.len() + 1,
            dist[3],
            dist[2],
            dist[1],
            dist[0],
            unmeasurable
        );
        rows.push(row);
    }

    print_headline(&rows, &dist, unmeasurable);
    print_full_table(&rows);
    print_could_not_measure(&rows);

    println!("\ncommit under measurement: {commit}");
    println!(
        "reproduce: rust/tools/pg.ps1 -Mode run -Example conf_matrix   (from the repo root of the \
         worktree this was measured in)"
    );
}

fn run_fixture(fixture: &FixtureRef) -> FixtureRow {
    let label = fixture.label();
    println!("=== {label} ===");

    let grammar_xml = fixture.load_grammar_xml();
    let grammar: Grammar = match pg_grammar::load(&grammar_xml) {
        Ok(g) => g,
        Err(e) => {
            println!("  COULD NOT MEASURE (any strategy): grammar failed to load: {e}\n");
            return unmeasurable_row(&label, format!("grammar failed to load: {e}"));
        }
    };
    if grammar.char_tables.is_empty() {
        println!("  COULD NOT MEASURE (any strategy): grammar has no character table\n");
        return unmeasurable_row(&label, "grammar has no character table".to_string());
    }

    let words_yaml = fixture.load_words_yaml();
    let total_words = words_yaml.words.len();
    if total_words == 0 {
        println!("  COULD NOT MEASURE (any strategy): words.yaml has no words\n");
        return unmeasurable_row(&label, "words.yaml has no words".to_string());
    }

    let expect_fail_count = words_yaml.words.iter().filter(|w| w.expect_fail).count();
    let expect_skip_count = words_yaml.words.iter().filter(|w| w.expect_skip).count();
    println!(
        "  words.yaml: {total_words} word(s) declared ({expect_fail_count} expect_fail, \
         {expect_skip_count} expect_skip -- negative controls the oracle itself resolves to zero/\
         invalid-shape, not measurement gaps)"
    );

    let all_words: Vec<String> = words_yaml.words.iter().map(|w| w.word.clone()).collect();
    let subsampled = all_words.len() > MAX_WORDS_PER_FIXTURE;
    let words: Vec<String> = if subsampled {
        println!(
            "  SUBSAMPLED: measuring the first {MAX_WORDS_PER_FIXTURE} of {total_words} words \
             (bound = MAX_WORDS_PER_FIXTURE)"
        );
        all_words[..MAX_WORDS_PER_FIXTURE].to_vec()
    } else {
        all_words
    };
    let measured_words = words.len();

    let semantics = GrammarSemantics::derive(&grammar);
    let phonology = PhonologyProbe::new_with_semantics(&semantics);
    let baseline_plan =
        enumerate_default(&grammar, semantics.prules_in_order(), phonology.as_ref());

    let mut cache = match RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default()) {
        Ok(cache) => cache,
        Err(fault) => {
            println!("  COULD NOT MEASURE (any strategy): oracle preparation faulted: {fault}\n");
            return unmeasurable_row(&label, format!("oracle preparation faulted: {fault}"));
        }
    };

    let corpus_evidence = cache.corpus_evidence(&words);
    let excluded: Vec<(String, String)> = corpus_evidence
        .exclusions
        .iter()
        .map(|e| (e.word.clone(), e.reason.clone()))
        .collect();
    if excluded.is_empty() {
        println!("  words excluded by the oracle: 0/{measured_words}");
    } else {
        println!(
            "  words excluded by the oracle: {}/{measured_words} (named, never dropped silently):",
            excluded.len()
        );
        for (word, reason) in &excluded {
            println!("    excluded {word:?}: {reason}");
        }
    }

    let mut prev_divergence = cache.identity_divergence();
    let mut strategy_rows = Vec::with_capacity(ALL_STRATEGIES.len());
    let mut exact_count = 0usize;

    for &strategy in ALL_STRATEGIES {
        let candidate = LoweredCandidate {
            label: "conf-matrix",
            plan: baseline_plan.clone(),
            adapter: LoweringAdapter::for_strategy(strategy),
            // Only the plan-composing adapter reads this shared baseline plan, so it alone is baseline.
            role: if strategy == EmissionStrategy::PlanComposed {
                CandidateRole::Baseline
            } else {
                CandidateRole::Alternative
            },
        };

        let observed = evaluate_plans_observed_with_cache(
            &grammar,
            std::slice::from_ref(&candidate),
            &words,
            RuntimeBudget::default(),
            &mut cache,
        );
        let obs = &observed[0];

        let now_divergence = cache.identity_divergence();
        let delta = subtract_divergence(now_divergence, prev_divergence);
        prev_divergence = now_divergence;

        let certification = &obs.evaluation.certification;
        let certification_debug = format!("{certification:?}");
        let compiles = compile_verdict(certification);
        let exact = matches!(certification, Certification::FullHcConfirmed { .. });
        if exact {
            exact_count += 1;
        }

        print!(
            "  [{:?}] compiles={} certification={certification_debug}",
            strategy,
            if compiles.is_ok() { "yes" } else { "no" }
        );
        if let Err(reason) = &compiles {
            println!(" -- REFUSED/FAILED: {reason}");
            strategy_rows.push(StrategyRow {
                strategy,
                compiles: Err(reason.clone()),
                certification_debug,
                exact,
                recall_oracle_only: 0,
                soundness_candidate_only: 0,
                legal_overgeneration: 0,
                words_measured: None,
                could_not_measure: Some(reason.clone()),
            });
            continue;
        }
        println!();

        let Some(evidence) = &obs.words else {
            let reason = format!(
                "evaluation did not reach comparable per-word evidence (certification={certification_debug})"
            );
            println!("    COULD NOT MEASURE per-word evidence: {reason}");
            strategy_rows.push(StrategyRow {
                strategy,
                compiles: Ok(()),
                certification_debug,
                exact,
                recall_oracle_only: 0,
                soundness_candidate_only: 0,
                legal_overgeneration: 0,
                words_measured: None,
                could_not_measure: Some(reason),
            });
            continue;
        };

        let legal_overgeneration: u64 = evidence
            .iter()
            .map(|we| proposals_pruned_by_confirm(we))
            .sum();

        println!(
            "    recall(oracle_only,DEFECT)={} soundness(candidate_only post-confirm,DEFECT)={} \
             legal_overgeneration(pre-confirm pruned by confirm,INFORMATIONAL per ADR-0001)={} \
             words_measured={}/{measured_words} exact={exact}",
            delta.oracle_only_identities,
            delta.candidate_only_identities,
            legal_overgeneration,
            evidence.len(),
        );

        strategy_rows.push(StrategyRow {
            strategy,
            compiles: Ok(()),
            certification_debug,
            exact,
            recall_oracle_only: delta.oracle_only_identities,
            soundness_candidate_only: delta.candidate_only_identities,
            legal_overgeneration,
            words_measured: Some(evidence.len()),
            could_not_measure: None,
        });
    }

    println!(
        "  HEADLINE: {exact_count}/{} backends produce oracle-exact confirmed output for this \
         fixture\n",
        ALL_STRATEGIES.len()
    );

    FixtureRow {
        label,
        total_words,
        measured_words,
        subsampled,
        excluded,
        expect_fail_count,
        expect_skip_count,
        strategies: strategy_rows,
        exact_count: Some(exact_count),
    }
}

fn unmeasurable_row(label: &str, reason: String) -> FixtureRow {
    let strategies = ALL_STRATEGIES
        .iter()
        .map(|&strategy| StrategyRow {
            strategy,
            compiles: Err(reason.clone()),
            certification_debug: "n/a".to_string(),
            exact: false,
            recall_oracle_only: 0,
            soundness_candidate_only: 0,
            legal_overgeneration: 0,
            words_measured: None,
            could_not_measure: Some(reason.clone()),
        })
        .collect();
    FixtureRow {
        label: label.to_string(),
        total_words: 0,
        measured_words: 0,
        subsampled: false,
        excluded: Vec::new(),
        expect_fail_count: 0,
        expect_skip_count: 0,
        strategies,
        exact_count: None,
    }
}

/// Whether `certification` means the candidate's network was actually BUILT. See this file's own
/// module doc for the exact rule and why `Truncated`/`ResourceBreach`/`IdentityMismatch` all count
/// as "compiles=yes" (the network built; the corpus-level verdict is a separate question).
fn compile_verdict(certification: &Certification) -> Result<(), String> {
    match certification {
        Certification::CapabilityRejected { reason }
        | Certification::BuildFailed { reason }
        | Certification::Unsupported { reason } => Err(reason.clone()),
        _ => Ok(()),
    }
}

fn subtract_divergence(
    after: IdentityDivergence,
    before: IdentityDivergence,
) -> IdentityDivergence {
    IdentityDivergence {
        occurrences_compared: after
            .occurrences_compared
            .saturating_sub(before.occurrences_compared),
        occurrences_not_compared: after
            .occurrences_not_compared
            .saturating_sub(before.occurrences_not_compared),
        oracle_identities: after
            .oracle_identities
            .saturating_sub(before.oracle_identities),
        candidate_identities: after
            .candidate_identities
            .saturating_sub(before.candidate_identities),
        oracle_only_identities: after
            .oracle_only_identities
            .saturating_sub(before.oracle_only_identities),
        candidate_only_identities: after
            .candidate_only_identities
            .saturating_sub(before.candidate_only_identities),
        occurrences_with_candidate_only: after
            .occurrences_with_candidate_only
            .saturating_sub(before.occurrences_with_candidate_only),
        oracle_admission_key_collisions: after
            .oracle_admission_key_collisions
            .saturating_sub(before.oracle_admission_key_collisions),
        candidate_admission_key_collisions: after
            .candidate_admission_key_collisions
            .saturating_sub(before.candidate_admission_key_collisions),
    }
}

/// Raw pre-confirm proposals (by admission key: morpheme ids + root index, the same key
/// `crate::confirm::confirm_batch` routes on) that did not survive into the post-confirm `actual`
/// result for this one word occurrence. Informational under ADR-0001, never a defect on its own --
/// see this file's module doc and `docs/research/backend-measurement-instruments.md`.
fn proposals_pruned_by_confirm(evidence: &pg_foma::backend_runtime::WordEvidence) -> u64 {
    let mut actual_keys: BTreeMap<(Vec<u32>, i32), usize> = BTreeMap::new();
    for a in &evidence.actual {
        *actual_keys
            .entry((a.morpheme_ids.clone(), a.root_morpheme_index))
            .or_default() += 1;
    }
    let mut proposed_keys: BTreeMap<(Vec<u32>, i32), usize> = BTreeMap::new();
    for c in &evidence.proposals {
        let key = (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
        *proposed_keys.entry(key).or_default() += 1;
    }
    let mut pruned = 0u64;
    for (key, proposed_count) in &proposed_keys {
        let actual_count = actual_keys.get(key).copied().unwrap_or(0);
        pruned = pruned.saturating_add((*proposed_count).saturating_sub(actual_count) as u64);
    }
    pruned
}

fn print_headline(rows: &[FixtureRow], dist: &[usize; 4], unmeasurable: usize) {
    println!(
        "\n================ HEADLINE: oracle-exact confirmed output, per fixture ================"
    );
    println!("fixtures discovered: {}", rows.len());
    println!("fixtures measurable: {}", rows.len() - unmeasurable);
    for k in (0..=3).rev() {
        println!("  supported by {k}/3 backends: {} fixture(s)", dist[k]);
    }
    println!("  unmeasurable (see COULD NOT MEASURE section below): {unmeasurable} fixture(s)");
    if dist[0] > 0 {
        println!("\n  fixtures supported by 0/3 backends:");
        for row in rows.iter().filter(|r| r.exact_count == Some(0)) {
            println!("    {}", row.label);
        }
    }
}

fn print_full_table(rows: &[FixtureRow]) {
    println!("\n================ FULL TABLE ================");
    println!(
        "columns per strategy: compiles(y/n) / recall(oracle-only,DEFECT) / \
         soundness(candidate-only,DEFECT) / legal-overgen(informational) / exact(y/n)"
    );
    for row in rows {
        println!(
            "\n{} [{} words measured of {} total{}, {} excluded, {} expect_fail, {} expect_skip]",
            row.label,
            row.measured_words,
            row.total_words,
            if row.subsampled { ", SUBSAMPLED" } else { "" },
            row.excluded.len(),
            row.expect_fail_count,
            row.expect_skip_count
        );
        for s in &row.strategies {
            let compiles = if s.compiles.is_ok() { "y" } else { "n" };
            match &s.could_not_measure {
                Some(reason) => {
                    println!(
                        "  {:?}: compiles={compiles} certification={} COULD NOT MEASURE -- {reason}",
                        s.strategy, s.certification_debug
                    );
                }
                None => {
                    println!(
                        "  {:?}: compiles={compiles} certification={} recall={} soundness={} \
                         legal_overgen={} words_measured={} exact={}",
                        s.strategy,
                        s.certification_debug,
                        s.recall_oracle_only,
                        s.soundness_candidate_only,
                        s.legal_overgeneration,
                        s.words_measured.unwrap_or(0),
                        s.exact
                    );
                }
            }
        }
    }
}

fn print_could_not_measure(rows: &[FixtureRow]) {
    let mut any = false;
    println!("\n================ CELLS THAT COULD NOT BE MEASURED ================");
    for row in rows {
        for s in &row.strategies {
            if let Some(reason) = &s.could_not_measure {
                any = true;
                println!("{} [{:?}]: {reason}", row.label, s.strategy);
            }
        }
    }
    if !any {
        println!("none -- every (fixture, backend) cell was measured");
    }
}
