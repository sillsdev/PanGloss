//! Part C (delanguaging) gate for `pg_grammar_gen::build::chain` (the deep standalone-affix-chain
//! builder — that module's own doc). This is the MEASURED answer to Part C's own open risk: does a
//! synthetic deep-chain grammar reproduce the real Aweti `apply_up`-explosion/OOM anchor
//! without the gitignored corpus?
//!
//! **Measured finding (`examples/deep_chain_scale_probe.rs`/`examples/deep_chain_compose_probe.rs`,
//! run manually — see this crate's own delanguaging report for the full numbers): NO, not at this
//! construct's own scale ceiling (N=24 standalone suffix rules, matching Aweti's real per-zone
//! rule counts of 11/24).** Both the bare lexc net (`FomaProposer::new_with_profile`) and the SAME
//! net composed against a trivial identity phonological rule (`fsm_compose` + `fsm_minimize`) stay
//! in the microsecond-to-low-millisecond range at every `N` tried, with state/arc counts growing
//! only near-linearly in `N` (not exponentially), and an unbounded `propose()` call on a
//! deliberately maximally-path-ambiguous query word (root + every-other-rule's own suffix, `k =
//! N/2` of `N`, chosen to maximize `C(N, k)` — up to 2,704,156 at N=24) completes in
//! low-microseconds, not the expected combinatorial blowup. The most likely explanation (not
//! independently verified further — flagged, not asserted, as this construct's own honest limit):
//! foma's minimization collapses every "which of the N levels fired" derivation-order variant into
//! ONE state whenever they are bisimilar (same future language) — true for this construct's PURE,
//! content-free suffix chain, but NOT necessarily true for Aweti's real rules, whose real
//! phonological conditioning environments and TWO independent per-zone chain instances break that
//! bisimulation. This gate pins the ACTUAL (small) measured envelope as a regression guard, and
//! documents the gap rather than claiming a cliff this construct does not, in fact, reproduce.
//!
//! The OTHER required Part C shape (a large-cascade grammar) DOES reproduce a real cliff and its
//! guard: `examples/large_cascade_scale_probe.rs` (roots x circumfix-rule composite-pre-expansion
//! scale, `pg_grammar_gen::build::circumfix` at growing `entries_per_stratum x circumfix_count`)
//! measured compile time growing from ~1ms (product=3) to ~8.6s (product=384), then the PRODUCTION
//! default `EnumerationBudget` (Fix 1 — the SAME guard `pg_foma::analyzer::FomaError::
//! EnumerationBudgetExceeded`'s own doc names as built for exactly Aweti's real 855-root/123-rule
//! case) correctly tripped at product=576 (200,038 composite entries against the 200,000 default
//! cap) after ~18s, honestly, with no OOM and no hang. That test is not duplicated here as a fast
//! gate: `EnumerationBudget::with_caps` is `pub(crate)` (only `crate::morphotactics` itself and
//! `crate::analyzer` can construct a tiny explicit cap the way `phase_c_compounding.rs`'s own
//! `ComposeBudget::with_caps` overbudget variant does for its OWN, public budget type) --
//! triggering the REAL default cap deterministically fast, from an external test crate, would
//! require the `HC_ENUM_ENTRY_BUDGET` env var, which this crate's own tests deliberately never
//! touch (parallel-test-process env races, `EnumerationBudget::with_caps`'s own doc). The 18s
//! real-cap trip is recorded as a manually-run measurement instead (module doc above), matching
//! `examples/p6_aweti_q1_cycle_check.rs`'s own precedent for a durable, not-every-`cargo-test`-run
//! measurement tool.

mod common;

use std::time::{Duration, Instant};

use pg_foma::analyzer::FomaProposer;
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

/// (a) Recall parity at a small `N` -- mirrors `phase_c_compounding.rs`/`phase_c_circumfix.rs`'s
/// own oracle-sweep-then-compose-recall shape. `sweep`'s own depth-1 bound (bare root + each rule
/// individually) is exactly what this construct's own oracle needs: every chain rule is
/// independently optional (module doc of `build::chain`), so a bare root and each single-rule
/// application are all individually valid, real words.
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

/// (b) Resource envelope AND apply-time behavior at Aweti's REAL per-zone scale (`N = 24`, this
/// construct's own ceiling — `build::chain`'s doc: `build::tables`' 26-ASCII-letter limit). Pins
/// the module doc's own measured negative finding as a regression guard: this shape's compiled net
/// stays small and `propose()` stays fast, even on a deliberately maximally-path-ambiguous query.
/// If a future change to `build_deriv_chain`'s legacy `TextMode::SurfaceProbed` strategy (or to
/// `foma`'s own minimization) ever makes this explode, this test is what will catch it.
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

    // Module doc's own measured envelope (small margin above the actual ~1150 states/1700 arcs at
    // N=24) -- a real regression (this construct starting to explode) would blow far past this,
    // never land just over it.
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

    // Deliberately maximally-ambiguous query word (module doc): root + every-other-rule's own
    // suffix (k = n/2 of n, in order) -- C(24, 12) = 2,704,156 raw apply_up placements if the
    // legacy chain's own path-multiplicity mechanism were actually explosive here.
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
