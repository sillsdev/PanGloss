//! Gate for `pg_grammar_gen::build::chain` (deep standalone-affix chains): a synthetic deep chain does NOT reproduce the real Aweti `apply_up`-explosion/OOM even at N=24, most likely because foma's minimization collapses bisimilar "which level fired" derivation-order variants for this pure, content-free chain — a property Aweti's real, phonologically-conditioned rules are not guaranteed to share. Pins the actual (small) measured envelope as a regression guard rather than claiming a cliff this construct doesn't reproduce.

mod common;

use std::time::{Duration, Instant};

use pg_foma::analyzer::FomaProposer;
use pg_foma::emit::FomaTier;
use pg_foma::health::{Phase, Severity};
use pg_foma::health_evaluator::{evaluate, AttemptedPhases, CompileMeasurements};
use pg_grammar_gen::oracle::{sweep, OracleOpts};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};
use pg_parse::{Morpher, ParseOptions};

use common::gate_template::{assert_net_size_within, entry_id_of, per_word_p99, recall_reachable};

fn recipe(n: usize) -> Recipe {
    Recipe {
        name: "phase-c-chain",
        seed: 20260725,
        scale: ScaleKnobs {
            segment_inventory: n + 1,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 1,
            chain_rule_count: n,
            ..Default::default()
        },
    }
}

#[test]
fn ordinary_affix_depth_five_and_ten_are_not_health_violations() {
    for depth in [5, 10] {
        let rendered = pg_grammar_gen::render_indexed(&recipe(depth));
        let grammar = pg_grammar::load(&rendered.xml).unwrap_or_else(|error| {
            panic!("generated depth-{depth} chain XML failed to load: {error}")
        });

        let emitted = pg_foma::emit::emit(&grammar);
        assert!(
            matches!(emitted.report.tier, FomaTier::Full),
            "ordinary depth {depth} must stay fully represented: {:?}",
            emitted.report.tier
        );
        assert!(
            emitted.report.uncovered.is_empty(),
            "ordinary depth {depth} must not leave uncovered constructs: {:?}",
            emitted.report.uncovered
        );
        let health = evaluate(CompileMeasurements {
            phases: AttemptedPhases::starting_with(Phase::Compile),
            payload_bytes: None,
            emit_report: Some(&emitted.report),
            compose_errors: &[],
            apply_budget_trips: &[],
        });
        assert_eq!(
            health.admission(),
            Severity::WithinLimits,
            "ordinary depth {depth} alone must produce no health complaint: {:?}",
            health.findings
        );
    }
}

/// (a) Recall parity at a small `N`: `sweep`'s depth-1 bound (bare root + each rule individually) suits this construct exactly, since every chain rule is independently optional.
#[test]
fn chain_recall_parity_via_generator_and_oracle() {
    let recipe = recipe(5);
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml)
        .unwrap_or_else(|e| panic!("generated chain XML failed to load: {e}\n{}", rendered.xml));

    let chain = rendered.chain.as_ref().expect("chain_rule_count > 0");
    assert_eq!(chain.rule_xml_ids.len(), 5);
    assert_eq!(
        g.templates.len(),
        0,
        "chain rules are standalone, never template-wrapped"
    );
    let affix_process_rules = g
        .mrules
        .iter()
        .filter(|r| matches!(r, pg_grammar::model::MorphRuleDef::AffixProcess(_)))
        .count();
    assert_eq!(affix_process_rules, 5);

    let root_id = entry_id_of(&g, &chain.root_entry_xml_id);
    let mrules: Vec<pg_grammar::model::MRuleId> = chain
        .rule_xml_ids
        .iter()
        .map(|id| common::gate_template::mrule_id_of(&g, id))
        .collect();

    let oracle_opts = OracleOpts {
        step_cap: 20_000,
        word_timeout: Some(Duration::from_millis(500)),
        max_rules_per_root: 8,
        max_total_words: 100,
    };
    let words = sweep(&g, &[root_id], &mrules, &oracle_opts);
    assert!(!words.is_empty(), "oracle sweep produced zero words");
    assert!(
        words.iter().any(|w| w.mrule.is_none()),
        "no bare-root oracle word (every chain rule is optional)"
    );
    assert!(
        words.iter().any(|w| w.mrule.is_some()),
        "no single-suffix oracle word"
    );

    let emit_result = pg_foma::emit::emit(&g);
    assert!(
        emit_result.report.uncovered.is_empty(),
        "standalone suffix rules must be fully covered: {:?}",
        emit_result.report.uncovered
    );
    let opts = foma::options::FomaOptions::default();
    let net = foma::lexcread::fsm_lexc_parse_string(&opts, None, &emit_result.lexc_source)
        .unwrap_or_else(|| panic!("emitted lexc must compile:\n{}", emit_result.lexc_source));
    assert_net_size_within(&net, 500, 2_000);

    let morpher =
        Morpher::new(&g, oracle_opts.step_cap).with_word_timeout(oracle_opts.word_timeout);
    let popts = ParseOptions::default();
    let width = pg_foma::tags::tag_width(g.morphemes.len());
    let tag_sequences_for = |surface: &str| -> Vec<Vec<String>> {
        let outcome = morpher.parse_word_opts(surface, &popts);
        outcome
            .structured
            .iter()
            .map(|a| {
                a.morpheme_ids
                    .iter()
                    .enumerate()
                    .map(|(i, &m)| {
                        let mid = pg_grammar::model::MorphemeId(m);
                        if i as i32 == a.root_morpheme_index {
                            pg_foma::tags::root_tag_text(mid, width)
                        } else {
                            pg_foma::tags::morph_tag_text(mid, width)
                        }
                    })
                    .collect()
            })
            .collect()
    };

    let mut missed = Vec::new();
    for w in &words {
        let normalized = pg_grammar::nfd::nfd(&w.surface);
        let analyses = tag_sequences_for(&w.surface);
        assert!(
            !analyses.is_empty(),
            "oracle word {:?} has no analysis from its own grammar's own Morpher",
            w.surface
        );
        let any_reachable = analyses
            .iter()
            .any(|tags| recall_reachable(&net, &normalized, tags));
        if !any_reachable {
            missed.push(w.surface.clone());
        }
    }
    assert!(
        missed.is_empty(),
        "100% recall required; missed: {missed:?}"
    );

    let p99 = per_word_p99(&words, |w| {
        let normalized = pg_grammar::nfd::nfd(&w.surface);
        for tags in tag_sequences_for(&w.surface) {
            let _ = recall_reachable(&net, &normalized, &tags);
        }
    });
    assert!(
        p99 < Duration::from_millis(50),
        "per-word p99 {p99:?} exceeds the trip-wire"
    );
}

/// (b) Resource envelope and apply-time behavior at Aweti's real per-zone scale (N=24, this construct's own ceiling): pins the module doc's measured negative finding — net stays small, `propose()` stays fast even on a maximally-path-ambiguous query — as a regression guard.
#[test]
fn chain_stays_small_and_fast_at_full_scale() {
    let n = 24;
    let recipe = recipe(n);
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml)
        .unwrap_or_else(|e| panic!("generated chain XML failed to load: {e}"));
    let chain = rendered.chain.as_ref().expect("chain_rule_count > 0");
    assert_eq!(chain.rule_xml_ids.len(), n);

    let (result, profile) = FomaProposer::new_with_profile(&g);
    let mut proposer = result.unwrap_or_else(|e| panic!("N={n} chain grammar must compile: {e}"));

    // Small margin above the actual measured size at N=24 — a real regression would blow far past this, never land just over it.
    assert!(
        profile.final_state_count.unwrap_or(i64::MAX) < 5_000,
        "N={n} chain net has {:?} states -- expected to stay small (module doc's own negative \
         finding); if this now explodes, the finding above needs revisiting",
        profile.final_state_count
    );
    assert!(
        profile.final_arc_count.unwrap_or(i64::MAX) < 10_000,
        "N={n} chain net has {:?} arcs",
        profile.final_arc_count
    );

    // Deliberately maximally-ambiguous query word: root + every-other-rule's own suffix, chosen to maximize raw apply_up placements if the legacy chain's path-multiplicity mechanism were actually explosive here.
    let table = &g.char_tables[0];
    let root_ch = chain.root_shape.chars().next().unwrap();
    let mut suffix_chars: Vec<char> = Vec::new();
    for (_, cd) in table.iter() {
        for rep in cd.representations() {
            if let Some(c) = rep.chars().next() {
                if c != root_ch {
                    suffix_chars.push(c);
                }
            }
        }
    }
    suffix_chars.sort_unstable();
    let mut word = chain.root_shape.clone();
    for &c in suffix_chars.iter().take(n / 2) {
        word.push(c);
    }

    let t0 = Instant::now();
    let candidates = proposer.propose(&word);
    let elapsed = t0.elapsed();
    assert!(
        !candidates.is_empty(),
        "the maximally-ambiguous query word must still be a real, analyzable word"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "propose({word:?}) took {elapsed:?} at N={n} -- module doc's own measured finding is \
         microseconds; a real regression would land far past this generous trip-wire, never just \
         over it"
    );
}
