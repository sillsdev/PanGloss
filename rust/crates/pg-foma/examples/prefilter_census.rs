//! Rejection census for the deterministic candidate pre-filter design. Measures, per grammar,
//! what fraction of FAILING-candidate confirm time is spent on candidates that a cheap
//! deterministic predicate (env/co-occurrence/stem-name/bound-root — `pg_rules::validity`'s gate)
//! could have rejected BEFORE the engine ever ran (category a), vs. candidates where the
//! unapply/synthesis cascade never produced a derivation at all (category b), vs. everything else
//! (category c) — the go/no-go gate for building the filter: only if (a) is NOT under ~10% of
//! failing time on every grammar.
//!
//! ## Methodology (why this isn't just "sum per-candidate confirm_all times")
//!
//! `pg_foma::confirm::confirm_batch`'s whole point is that batching/fusion (root-set grouping,
//! `RULE_UNION_SLACK` sub-chunking, cross-root-set fusion) makes REAL production confirm time
//! much less than the sum of per-candidate reparses — confirm.rs's own doc measures cross-
//! root-set fusion alone at ~54% of batched Sena confirm. Timing per-candidate unbatched calls
//! would measure a workload the real filter never runs in, and the bias direction (inflating
//! category (b), which repeats cascade work every unbatched call, OR inflating category (a) via
//! repeated synthesis) can't be signed — exactly the failure mode that makes a census built that
//! way untrustworthy near the plan's ~10% gate.
//!
//! So TIME comes from a **counterfactual under the real batched/fused confirm**, per word:
//!   - `baseline`       = `confirm_batch(all candidates)`
//!   - `keep_confirming` = `confirm_batch(only candidates that end up confirming)`
//!   - `minus_x`        = `confirm_batch(all candidates EXCEPT the ones classified category x)`
//!     for each of x in {a, b, c}
//!   - category x's share of FAILING time = `(baseline - minus_x) / (baseline - keep_confirming)`
//!
//! This is literally "how much of the failing-candidate time would a perfect category-x
//! predicate have saved, run through the real fused confirm path" — the actual go/no-go
//! quantity. It is not required to be perfectly additive across a+b+c (removing candidates
//! changes which chunks fuse, so marginals overlap or leave gaps); the RATIO is the decision
//! signal, not the sum — reported as such.
//!
//! CLASSIFICATION (which category a failing candidate belongs to) is a SEPARATE, untimed pass,
//! using `pg_foma::confirm::confirm_one_traced` (census-only, additive — never on a production
//! path): first a cheap untraced call reads `ParseOutcome::candidates_generated` (no tracing
//! overhead) — `== 0` is category (b) by construction (the synthesis cascade produced not one
//! candidate word to test). Only if `candidates_generated > 0` does a second, TRACED call run
//! (tracing forces `merge_equivalent = false` and disables the analysis memo — see
//! `pg-parse/src/morpher.rs`'s own doc on why — so this expensive path is scoped to the smaller
//! subset that needs it). The resulting trace tree's `Failed` nodes are walked for
//! `FailureReason`s; precedence is (a) validity-gate reasons > (b) cascade/surface reasons > (c)
//! everything else (see `classify_reasons` below) — a candidate reaching a validity-gate `Failed`
//! node on ANY explored branch demonstrates a deterministic predicate over its own fixed
//! morpheme/allomorph set could have rejected it (the plan's "does ANY allomorph of this morpheme
//! survive" upward-safe framing), independent of which cascade branch happened to reach the gate.
//!
//! ## Usage
//!
//!   cargo run -p pg-foma --release --example prefilter_census -- <sena|indonesian|amharic> [word_cap]
//!
//! Prints a markdown report: counts per category, the raw `FailureReason` histogram within (a),
//! and the time-share ratios above. Not part of `cargo test` — a measurement tool, like
//! `precision_bench`/`knob_probe`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use foma::apply::apply_init;
use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use foma::types::Fsm;

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::confirm::{self, MorphemeOwner};
use pg_foma::emit;
use pg_foma::peel::ReduplicationPeeler;
use pg_foma::tags::{self, Candidate};
use pg_grammar::model::Grammar;
use pg_parse::Morpher;
use pg_rules::trace::{FailureReason, TraceType, TreeTraceSink};

const ENGINE_TIMEOUT: Duration = Duration::from_secs(10);

// -------------------------------------------------------------------------------------------
// Sample loading + propose/peel plumbing — copied (not imported: examples can't depend on test
// code, and this crate's own analyzer hardcodes `emit::emit(g)`/Strip already, matching this
// census's use of the SAME production network) from `examples/precision_bench.rs`, which itself
// copied it from `tests/pk1_precision_recall_invariance.rs`. See that file for the property-level
// justification of each step.
// -------------------------------------------------------------------------------------------

fn sample_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/data")
        .join(name)
}

fn load_grammar(name: &str) -> Option<Grammar> {
    let path = sample_path(name);
    let xml = std::fs::read_to_string(&path).ok()?;
    Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {name}: {e}")))
}

fn read_words(name: &str) -> Option<Vec<String>> {
    let path = sample_path(name);
    let text = std::fs::read_to_string(&path).ok()?;
    Some(
        text.lines()
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

fn propose(net: &Fsm, word: &str) -> Vec<Candidate> {
    let normalized = pg_grammar::nfd::nfd(word);
    let mut handle = apply_init(net);
    let mut seen: HashSet<(Vec<u32>, i32)> = HashSet::new();
    let mut out = Vec::new();
    for s in handle.up(&normalized) {
        let Some(path) = tags::decode_path(&s) else {
            continue;
        };
        for c in tags::to_candidates(&path) {
            let key: (Vec<u32>, i32) = (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
            if seen.insert(key) {
                out.push(c);
            }
        }
    }
    out
}

fn propose_and_peel(
    net: &Fsm,
    g: &Grammar,
    peeler: &ReduplicationPeeler,
    word: &str,
) -> Vec<Candidate> {
    let mut candidates = propose(net, word);
    // Real corpus/census words, never an adversarial synthetic stress string -- an unbounded
    // chain-depth budget is safe here (`pg_foma::peel`'s own module doc, "Chain depth and nested
    // reduplication", ADR 0003).
    let budget = ComposeBudget::from_env();
    let peeled = peeler
        .peel_candidates(g, word, &budget, &mut |r: &str| propose(net, r))
        .unwrap_or_else(|e| {
            eprintln!("[prefilter_census] reduplication peel refused for {word:?}: {e}");
            Vec::new()
        });
    for c in peeled {
        let already = candidates.iter().any(|existing| {
            existing.root_index == c.root_index && existing.morphemes == c.morphemes
        });
        if !already {
            candidates.push(c);
        }
    }
    candidates
}

// -------------------------------------------------------------------------------------------
// Category model.
// -------------------------------------------------------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum Category {
    /// Derives at least one candidate word but is rejected by `pg_rules::validity`'s gate — the
    /// deterministically pre-checkable rejection the plan's Phase 2 filters would target.
    A,
    /// Either `candidates_generated == 0` (the cascade never produced a derivation to test), or
    /// the only `Failed` reasons hit are `PartialParse`/`SurfaceFormMismatch` (cascade-completion/
    /// surface-match dead ends, not a pre-checkable morpheme-level gate).
    B,
    /// Everything else: `ObligatorySyntacticFeatures`, pin-resolution `None`, or a candidate whose
    /// restricted reparse generated candidates and even matched SOMETHING but never routed to
    /// this candidate's own key (positional-routing mismatch).
    C,
}

/// The validity-gate `FailureReason`s `pg_rules::validity::allomorphs_valid_impl` actually emits
/// (confirmed by reading that function's body — NOT all 23 `FailureReason` variants exist there;
/// see this file's own doc / the census report's caveat). `DisjunctiveAllomorph` and
/// `RequiredSyntacticFeatureStruct` are flagged separately below as edge cases (still counted in
/// (a) — the plan's Phase 2 order already lists them separately from the flagship env/
/// co-occurrence checks).
const VALIDITY_GATE_REASONS: &[FailureReason] = &[
    FailureReason::Environments,
    FailureReason::AllomorphCoOccurrenceRules,
    FailureReason::MorphemeCoOccurrenceRules,
    FailureReason::BoundRoot,
    FailureReason::RequiredStemName,
    FailureReason::ExcludedStemName,
    FailureReason::DisjunctiveAllomorph,
    FailureReason::RequiredSyntacticFeatureStruct,
];

const CASCADE_REASONS: &[FailureReason] = &[
    FailureReason::PartialParse,
    FailureReason::SurfaceFormMismatch,
];

fn reason_name(r: FailureReason) -> &'static str {
    match r {
        FailureReason::ObligatorySyntacticFeatures => "ObligatorySyntacticFeatures",
        FailureReason::AllomorphCoOccurrenceRules => "AllomorphCoOccurrenceRules",
        FailureReason::Environments => "Environments",
        FailureReason::MorphemeCoOccurrenceRules => "MorphemeCoOccurrenceRules",
        FailureReason::DisjunctiveAllomorph => "DisjunctiveAllomorph",
        FailureReason::SurfaceFormMismatch => "SurfaceFormMismatch",
        FailureReason::Pattern => "Pattern",
        FailureReason::HeadPattern => "HeadPattern",
        FailureReason::NonHeadPattern => "NonHeadPattern",
        FailureReason::RequiredSyntacticFeatureStruct => "RequiredSyntacticFeatureStruct",
        FailureReason::HeadRequiredSyntacticFeatureStruct => "HeadRequiredSyntacticFeatureStruct",
        FailureReason::NonHeadRequiredSyntacticFeatureStruct => {
            "NonHeadRequiredSyntacticFeatureStruct"
        }
        FailureReason::HeadProdRestrictMprFeatures => "HeadProdRestrictMprFeatures",
        FailureReason::NonHeadProdRestrictMprFeatures => "NonHeadProdRestrictMprFeatures",
        FailureReason::RequiredMprFeatures => "RequiredMprFeatures",
        FailureReason::ExcludedMprFeatures => "ExcludedMprFeatures",
        FailureReason::RequiredStemName => "RequiredStemName",
        FailureReason::ExcludedStemName => "ExcludedStemName",
        FailureReason::PartialParse => "PartialParse",
        FailureReason::BoundRoot => "BoundRoot",
        FailureReason::NonPartialRuleProhibitedAfterFinalTemplate => {
            "NonPartialRuleProhibitedAfterFinalTemplate"
        }
        FailureReason::NonPartialRuleRequiredAfterNonFinalTemplate => {
            "NonPartialRuleRequiredAfterNonFinalTemplate"
        }
        FailureReason::MaxApplicationCount => "MaxApplicationCount",
    }
}

/// Walk every `Failed`-type node reachable from `sink`'s root, collecting `failure_reason`s.
/// `pg_rules::trace::TraceSink::failed` is the only method that sets `TraceNode::failure_reason`
/// on a `TraceType::Failed` node (the other `_not_applied` sites use `TraceType::
/// {Phonological,Morphological}RuleSynthesis` with their own reason, included too — same
/// "this branch dead-ended with reason X" semantics for this census's purposes).
fn collect_failure_reasons(sink: &TreeTraceSink) -> Vec<FailureReason> {
    let mut out = Vec::new();
    let Some(root) = sink.root() else { return out };
    let mut stack = vec![root];
    while let Some(h) = stack.pop() {
        let n = sink.node(h);
        if let Some(r) = n.failure_reason {
            if matches!(n.type_, TraceType::Failed) {
                out.push(r);
            }
        }
        stack.extend(n.children.iter().copied());
    }
    out
}

fn classify_reasons(reasons: &[FailureReason]) -> Category {
    if reasons.iter().any(|r| VALIDITY_GATE_REASONS.contains(r)) {
        Category::A
    } else if reasons.iter().any(|r| CASCADE_REASONS.contains(r)) {
        Category::B
    } else {
        Category::C
    }
}

// -------------------------------------------------------------------------------------------
// Per-word measurement.
// -------------------------------------------------------------------------------------------

struct WordCensus {
    total_candidates: usize,
    confirming: usize,
    failing: usize,
    cat_counts: [usize; 3], // A, B, C
    reason_hist: rustc_hash::FxHashMap<&'static str, usize>,
    /// Numerators/denominator for this word's contribution to the aggregate time-share ratios
    /// (see module doc). `None` when there were no failing candidates (nothing to attribute) or
    /// the denominator was non-positive (measurement noise on a near-zero-cost word).
    time_shares: Option<[f64; 3]>, // (baseline-minus_x) for x in A,B,C
    denom: f64, // baseline - keep_confirming
}

fn measure_word(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    word: &str,
    candidates: &[Candidate],
) -> WordCensus {
    let mut cat_counts = [0usize; 3];
    let mut reason_hist: rustc_hash::FxHashMap<&'static str, usize> =
        rustc_hash::FxHashMap::default();

    let baseline_start = Instant::now();
    let baseline_buckets = confirm::confirm_batch(g, owners, morpher, candidates, word);
    let baseline_ms = baseline_start.elapsed().as_secs_f64() * 1000.0;

    let mut failing_idx = Vec::new();
    let mut confirming_idx = Vec::new();
    for (i, bucket) in baseline_buckets.iter().enumerate() {
        if bucket.is_empty() {
            failing_idx.push(i);
        } else {
            confirming_idx.push(i);
        }
    }

    let total_candidates = candidates.len();
    let confirming = confirming_idx.len();
    let failing = failing_idx.len();

    if failing_idx.is_empty() {
        return WordCensus {
            total_candidates,
            confirming,
            failing,
            cat_counts,
            reason_hist,
            time_shares: None,
            denom: 0.0,
        };
    }

    // Classify every failing candidate (untimed pass).
    let mut cat_of: Vec<Category> = Vec::with_capacity(failing_idx.len());
    for &i in &failing_idx {
        let noop = pg_rules::trace::NoopSink;
        let cheap = confirm::confirm_one_traced(g, owners, morpher, &candidates[i], word, &noop);
        let cat = match cheap {
            None => Category::C, // pin resolution failed — plan's "pin-resolution None" bucket.
            Some(outcome) if outcome.candidates_generated == 0 => Category::B,
            Some(_) => {
                let sink = TreeTraceSink::new();
                let _ =
                    confirm::confirm_one_traced(g, owners, morpher, &candidates[i], word, &sink);
                let reasons = collect_failure_reasons(&sink);
                for &r in &reasons {
                    if VALIDITY_GATE_REASONS.contains(&r) {
                        *reason_hist.entry(reason_name(r)).or_insert(0) += 1;
                    }
                }
                classify_reasons(&reasons)
            }
        };
        cat_counts[match cat {
            Category::A => 0,
            Category::B => 1,
            Category::C => 2,
        }] += 1;
        cat_of.push(cat);
    }

    // Counterfactual timing: keep_confirming, and minus_{a,b,c}.
    let confirming_only: Vec<Candidate> = confirming_idx
        .iter()
        .map(|&i| candidates[i].clone())
        .collect();
    let keep_start = Instant::now();
    let _ = confirm::confirm_batch(g, owners, morpher, &confirming_only, word);
    let keep_ms = keep_start.elapsed().as_secs_f64() * 1000.0;

    let mut minus_ms = [0.0f64; 3];
    for (slot, target) in [Category::A, Category::B, Category::C]
        .into_iter()
        .enumerate()
    {
        let minus: Vec<Candidate> = candidates
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                // Keep every confirming candidate untouched, plus every failing candidate NOT of
                // this category.
                match failing_idx.iter().position(|&fi| fi == *i) {
                    Some(pos) => cat_of[pos] != target,
                    None => true,
                }
            })
            .map(|(_, c)| c.clone())
            .collect();
        let start = Instant::now();
        let _ = confirm::confirm_batch(g, owners, morpher, &minus, word);
        minus_ms[slot] = start.elapsed().as_secs_f64() * 1000.0;
    }

    let denom = baseline_ms - keep_ms;
    let time_shares = if denom > 1e-6 {
        Some([
            baseline_ms - minus_ms[0],
            baseline_ms - minus_ms[1],
            baseline_ms - minus_ms[2],
        ])
    } else {
        None
    };

    WordCensus {
        total_candidates,
        confirming,
        failing,
        cat_counts,
        reason_hist,
        time_shares,
        denom: denom.max(0.0),
    }
}

// -------------------------------------------------------------------------------------------
// Aggregation + report.
// -------------------------------------------------------------------------------------------

struct GrammarReport {
    name: String,
    words_scanned: usize,
    total_candidates: usize,
    total_confirming: usize,
    total_failing: usize,
    cat_counts: [usize; 3],
    reason_hist: rustc_hash::FxHashMap<&'static str, usize>,
    time_numer: [f64; 3],
    time_denom: f64,
    wall_ms: f64,
}

fn run_grammar(
    name: &str,
    xml_file: &str,
    words_file: &str,
    word_cap: usize,
) -> Option<GrammarReport> {
    let start = Instant::now();
    let g = load_grammar(xml_file)?;
    let all_words = read_words(words_file)?;
    let words: Vec<String> = all_words.into_iter().take(word_cap).collect();
    if words.is_empty() {
        return None;
    }

    let emit::EmitResult { lexc_source, .. } = emit::emit(&g);
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, &lexc_source)
        .unwrap_or_else(|| panic!("foma failed to compile the emitted lexc source for {name}"));

    let peeler = ReduplicationPeeler::new(&g);
    let owners = confirm::build_morpheme_owners(&g);
    let morpher = Morpher::new(&g, usize::MAX).with_word_timeout(Some(ENGINE_TIMEOUT));

    let mut total_candidates = 0usize;
    let mut total_confirming = 0usize;
    let mut total_failing = 0usize;
    let mut cat_counts = [0usize; 3];
    let mut reason_hist: rustc_hash::FxHashMap<&'static str, usize> =
        rustc_hash::FxHashMap::default();
    let mut time_numer = [0.0f64; 3];
    let mut time_denom = 0.0f64;
    let mut words_scanned = 0usize;

    for word in &words {
        let candidates = propose_and_peel(&net, &g, &peeler, word);
        if candidates.is_empty() {
            continue;
        }
        words_scanned += 1;
        let wc = measure_word(&g, &owners, &morpher, word, &candidates);
        total_candidates += wc.total_candidates;
        total_confirming += wc.confirming;
        total_failing += wc.failing;
        for (acc, v) in cat_counts.iter_mut().zip(wc.cat_counts.iter()) {
            *acc += v;
        }
        for (r, n) in wc.reason_hist {
            *reason_hist.entry(r).or_insert(0) += n;
        }
        if let Some(shares) = wc.time_shares {
            for i in 0..3 {
                time_numer[i] += shares[i];
            }
            time_denom += wc.denom;
        }
        println!(
            "  ... {word}: {} candidates, {} confirming, {} failing (running totals: scanned={words_scanned})",
            wc.total_candidates, wc.confirming, wc.failing
        );
    }

    Some(GrammarReport {
        name: name.to_string(),
        words_scanned,
        total_candidates,
        total_confirming,
        total_failing,
        cat_counts,
        reason_hist,
        time_numer,
        time_denom,
        wall_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

fn print_report(r: &GrammarReport) {
    println!();
    println!("## {}", r.name);
    println!(
        "words_scanned={} total_candidates={} total_confirming={} total_failing={} candidate_precision={:.4}",
        r.words_scanned,
        r.total_candidates,
        r.total_confirming,
        r.total_failing,
        if r.total_candidates > 0 {
            r.total_confirming as f64 / r.total_candidates as f64
        } else {
            1.0
        }
    );
    println!(
        "failing-candidate counts by category: A(validity-gate)={} B(cascade-dead-end)={} C(other)={}",
        r.cat_counts[0], r.cat_counts[1], r.cat_counts[2]
    );
    if r.total_failing > 0 {
        println!(
            "  as %% of failing candidates: A={:.1}%% B={:.1}%% C={:.1}%%",
            100.0 * r.cat_counts[0] as f64 / r.total_failing as f64,
            100.0 * r.cat_counts[1] as f64 / r.total_failing as f64,
            100.0 * r.cat_counts[2] as f64 / r.total_failing as f64,
        );
    }
    println!();
    println!("FailureReason histogram within category (a) (validity-gate rejections):");
    let mut reasons: Vec<(&&'static str, &usize)> = r.reason_hist.iter().collect();
    reasons.sort_by(|a, b| b.1.cmp(a.1));
    for (reason, count) in reasons {
        println!("  {:32} {}", reason, count);
    }
    println!();
    println!(
        "time-share denominator (sum of baseline-minus-keep_confirming, ms) = {:.1}",
        r.time_denom
    );
    if r.time_denom > 1e-6 {
        println!(
            "GO/NO-GO metric — category (a)'s share of FAILING-candidate confirm time: {:.1}%%",
            100.0 * r.time_numer[0] / r.time_denom
        );
        println!(
            "  (for reference) B share: {:.1}%%  C share: {:.1}%%  (ratios need not sum to 100%% \
             — see module doc's non-additivity note)",
            100.0 * r.time_numer[1] / r.time_denom,
            100.0 * r.time_numer[2] / r.time_denom,
        );
    } else {
        println!(
            "GO/NO-GO metric: n/a (no measurable failing-candidate time on this corpus slice)"
        );
    }
    println!();
    println!("grammar wall time: {:.1}ms", r.wall_ms);
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn run() {
    let args: Vec<String> = std::env::args().collect();
    let grammars: Vec<(&str, &str, &str, usize)> = if args.len() >= 2 {
        let cap_override = args.get(2).and_then(|s| s.parse().ok());
        match args[1].as_str() {
            "sena" => vec![(
                "Sena",
                "sena-hc.xml",
                "sena-words.txt",
                cap_override.unwrap_or(env_usize("CENSUS_SENA_CAP", 300)),
            )],
            "indonesian" => vec![(
                "Indonesian",
                "indonesian-hc.xml",
                "indonesian-words.txt",
                cap_override.unwrap_or(env_usize("CENSUS_INDONESIAN_CAP", 121)),
            )],
            "amharic" => vec![(
                "Amharic",
                "amharic-hc.xml",
                "amharic-words.txt",
                cap_override.unwrap_or(env_usize("CENSUS_AMHARIC_CAP", 40)),
            )],
            other => {
                eprintln!("unknown grammar {other:?} (want sena|indonesian|amharic)");
                std::process::exit(2);
            }
        }
    } else {
        vec![
            (
                "Sena",
                "sena-hc.xml",
                "sena-words.txt",
                env_usize("CENSUS_SENA_CAP", 300),
            ),
            (
                "Indonesian",
                "indonesian-hc.xml",
                "indonesian-words.txt",
                env_usize("CENSUS_INDONESIAN_CAP", 121),
            ),
            (
                "Amharic",
                "amharic-hc.xml",
                "amharic-words.txt",
                env_usize("CENSUS_AMHARIC_CAP", 40),
            ),
        ]
    };

    println!("# Phase 0 candidate pre-filter rejection census");
    println!();
    println!(
        "See `docs/superpowers/specs/2026-07-16-candidate-prefilter-plan.md` Phase 0 for the \
         go/no-go criterion: category (a) under ~10%% of failing-candidate time on every grammar \
         => do not build the filter."
    );

    for (name, xml, words, cap) in grammars {
        println!();
        println!("Running {name} (cap={cap})...");
        match run_grammar(name, xml, words, cap) {
            Some(report) => print_report(&report),
            None => {
                println!("## {name}");
                println!("skipped: {xml} or {words} not present on disk");
            }
        }
    }
}

/// Amharic's deep composite/rule-chain recursion needs a big stack under release inlining (same
/// trick `precision_bench`/`knob_probe` use) — the tracing pass here adds MORE recursion depth
/// than either of those (full `TreeTraceSink` cascades, memo off), so this budget is generous.
fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(run)
        .expect("failed to spawn census thread");
    handle.join().expect("census thread panicked");
}
