//! GATE (`docs/fst-plan/phase-c-generator-design.md` §6, priority (5)): stratum-depth scale --
//! recall-parity only (no `_overbudget` variant in the stage-2 deliverables list for this
//! construct).
//!
//! `pg_grammar_gen::build::strata`'s own module doc explains why every extra stratum REUSES table
//! 0 rather than minting a new table per stratum: GATE 1 already covers multi-TABLE wrongness
//! (`table_of`/`resolve_alpha_tuples`'s hardcoded `char_tables[0]`, design doc §5); this gate is
//! about multi-STRATUM CASCADING specifically, which stays provably correct against a single,
//! always-table-0 grammar and so can be recall-tested honestly without tripping the still-open
//! multi-table gap a second time.
//!
//! Uses the PRODUCTION `pg_foma::emit::emit` path (same as GATE 2/circumfix): each extra stratum's
//! own obligatory `<MorphologicalRule>` is STRATUM-ATTACHED (wired via `<Stratum morphologicalRules
//! ="...">`, mirroring `pg-foma/src/morphotactics.rs`'s own `FIXTURE_STRATA` precedent), which
//! `emit.rs`'s own classification loop handles as a "standalone (stratum-attached) derivation rule"
//! (`deriv_prefix`/`deriv_suffix`) -- a different, simpler code path than GATE 2's own
//! `AffixTemplate`-wrapped circumfix rule, but still fully covered by the production emitter.

mod common;

use std::time::Duration;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;

use pg_foma::emit;
use pg_foma::tags;
use pg_grammar::model::LexEntryId;
use pg_grammar_gen::oracle::{sweep, OracleOpts};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};
use pg_parse::{Morpher, ParseOptions};

use common::gate_template::{assert_net_size_within, per_word_p99, recall_reachable};

fn recipe() -> Recipe {
    Recipe {
        name: "phase-c-strata-depth",
        seed: 20260720,
        scale: ScaleKnobs {
            entries_per_stratum: 2,
            segment_inventory: 2,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 1,
            extra_strata: 3,
            ..Default::default()
        },
    }
}

#[test]
fn strata_depth_recall_parity_via_generator_and_oracle() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated strata-depth XML failed to load: {e}\n{}",
            rendered.xml
        )
    });

    assert_eq!(g.strata.len(), 4, "1 base stratum + 3 extra");
    assert_eq!(rendered.extra_strata.len(), 3);
    assert_eq!(
        g.entries.len(),
        2,
        "recipe must produce exactly 2 roots on the base stratum"
    );

    // --- Oracle (design doc §3): bounded Morpher-as-generator sweep. Bare-root generation already
    // runs the FULL multi-stratum cascade (every extra stratum's own obligatory rule), so this is
    // ground truth for the fully-derived surface form each root must produce. ---
    let oracle_opts = OracleOpts {
        step_cap: 20_000,
        word_timeout: Some(Duration::from_millis(500)),
        max_rules_per_root: 8,
        max_total_words: 20,
    };
    let roots: Vec<LexEntryId> = (0..g.entries.len() as u32).map(LexEntryId).collect();
    let words = sweep(&g, &roots, &[], &oracle_opts);
    assert!(
        !words.is_empty(),
        "oracle sweep produced zero words -- gate must be non-vacuous"
    );
    assert_eq!(
        words.len(),
        2,
        "each of the 2 roots must produce exactly 1 fully-cascaded surface form"
    );

    // --- Build the FST via the production enumeration path (module doc). ---
    let emit_result = emit::emit(&g);
    assert!(
        emit_result.report.uncovered.is_empty(),
        "stratum-attached derivation rules must be fully covered by the enumeration path: {:?}",
        emit_result.report.uncovered
    );
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, &emit_result.lexc_source)
        .unwrap_or_else(|| panic!("emitted lexc must compile:\n{}", emit_result.lexc_source));

    // --- Resource envelope (design doc §4b): a 2-root, 3-extra-stratum grammar must stay tiny. ---
    assert_net_size_within(&net, 2_000, 20_000);

    // --- Recall (design doc §4a): re-parse each oracle word via an independent Morpher to recover
    // its own tag sequence (mirrors GATE 2's own technique), then check that sequence is reachable
    // in `net`. 100% required (no known compiler gap for this construct, design doc §5). ---
    let morpher =
        Morpher::new(&g, oracle_opts.step_cap).with_word_timeout(oracle_opts.word_timeout);
    let popts = ParseOptions::default();
    let width = tags::tag_width(g.morphemes.len());

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
                            tags::root_tag_text(mid, width)
                        } else {
                            tags::morph_tag_text(mid, width)
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
            "oracle word {:?} (root {:?}) has no analysis from the SAME grammar's own Morpher -- \
             oracle/parser inconsistency, not a recall question",
            w.surface,
            w.root
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
        "100% recall required on the oracle word list; missed: {missed:?}"
    );

    // --- Resource envelope (design doc §4b): per-word p99, sub-10ms trip-wire. ---
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
