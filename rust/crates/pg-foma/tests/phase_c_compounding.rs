//! GATE (`docs/fst-plan/phase-c-generator-design.md` §6, priority (6)): compounding-rule scale --
//! recall-parity + `_overbudget` (`EmitLineBudgetExceeded`, the first EMIT-scale exerciser in this
//! suite -- every earlier stage-2 gate exercised a COMPOSE-path budget: V6 group cap for
//! partition-k, V3 tuple cap for alpha-scale).
//!
//! See `pg_grammar_gen::build::compounding`'s own module doc for why recall-parity and the
//! overbudget variant deliberately use TWO DIFFERENT emitters: `pg_foma::emit::emit` (production
//! path, does support compounding, GATE 2's own precedent) for recall; `pg_foma::uflexc::
//! emit_underlying_filtered_with_budget` (does NOT see compounding rules at all, by that module's
//! own doc, but DOES incrementally count every root-entry line it writes) for the overbudget check
//! -- an honest, deliberate choice: the vector that actually trips first here is plain root-entry
//! COUNT (V4, synthetic-stress-grammar-plan.md §3), and compounding is simply the construct that
//! motivated giving this vector its first gate.

mod common;

use std::time::Duration;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;

use pg_foma::compose_budget::{ComposeBudget, ComposeError};
use pg_foma::emit;
use pg_foma::replace::SegAlphabet;
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying_filtered_with_budget;
use pg_grammar::model::MorphemeId;
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};
use pg_parse::morpher::GenMorpheme;
use pg_parse::{Morpher, ParseOptions};

use common::gate_template::{assert_net_size_within, entry_id_of, per_word_p99, recall_reachable};

fn recipe() -> Recipe {
    Recipe {
        name: "phase-c-compounding",
        seed: 20260720,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            compounding_rule_count: 1,
            ..Default::default()
        },
    }
}

/// A grammar with several INDEPENDENT compounding rules (module doc): `uflexc` never sees
/// compounding content, so this is really just several MORE root entries -- large enough (6, module
/// doc's mirrored precedent: `uflexc.rs`'s own `line_budget_trips_incrementally` trips a `line_cap`
/// of 5 on 20 entries after 6 lines) for a tiny test `line_cap` to trip almost immediately.
fn overbudget_recipe() -> Recipe {
    Recipe {
        name: "phase-c-compounding-overbudget",
        seed: 20260720,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            compounding_rule_count: 3,
            ..Default::default()
        },
    }
}

#[test]
fn compounding_recall_parity_via_generator_and_oracle() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated compounding XML failed to load: {e}\n{}",
            rendered.xml
        )
    });

    let compounding = rendered
        .compounding
        .as_ref()
        .expect("recipe declared compounding_rule_count > 0");
    assert_eq!(compounding.rule_xml_ids.len(), 1);
    assert_eq!(g.entries.len(), 2, "1 head root + 1 non-head root");
    let compounding_rules = g
        .mrules
        .iter()
        .filter(|r| matches!(r, pg_grammar::model::MorphRuleDef::Compounding(_)))
        .count();
    assert_eq!(compounding_rules, 1);

    // --- Oracle (design doc §3): `Morpher::generate_words` with a `GenMorpheme::NonHead` morpheme
    // -- `pg_grammar_gen::oracle`'s own module doc names this as the mechanism for a compounding
    // non-head root (its `sweep` helper doesn't cover it -- `candidate_rules` explicitly excludes
    // `Compounding`-kind rules, since they own no `MRuleId` a `GenMorpheme::Rule` could name). ---
    let head_id = entry_id_of(&g, &compounding.head_entry_xml_ids[0]);
    let nonhead_id = entry_id_of(&g, &compounding.nonhead_entry_xml_ids[0]);
    let morpher = Morpher::new(&g, 20_000).with_word_timeout(Some(Duration::from_millis(500)));
    let words = morpher.generate_words(
        head_id,
        &[GenMorpheme::NonHead(nonhead_id)],
        pg_featstruct::FeatureStruct::EMPTY,
    );
    assert!(
        !words.is_empty(),
        "oracle must produce at least 1 compound word"
    );

    // --- Build the FST via the production enumeration path (module doc). ---
    let emit_result = emit::emit(&g);
    assert!(
        emit_result.report.uncovered.is_empty(),
        "compounding must be fully covered by the enumeration path: {:?}",
        emit_result.report.uncovered
    );
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, &emit_result.lexc_source)
        .unwrap_or_else(|| panic!("emitted lexc must compile:\n{}", emit_result.lexc_source));

    // --- Resource envelope (design doc §4b): a 2-root, 1-compounding-rule grammar must stay tiny. ---
    assert_net_size_within(&net, 2_000, 20_000);

    // --- Recall (design doc §4a): re-parse each oracle word via an independent Morpher to recover
    // its own tag sequence (GATE 2's own technique), then check that sequence is reachable in
    // `net`. 100% required (compounding has no known compiler gap on the enumeration path). ---
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
                    .map(|&m| {
                        let mid = MorphemeId(m);
                        // A compound word has TWO root morphemes, but `WordAnalysis::
                        // root_morpheme_index` only ever names ONE of them (the "designated"
                        // root position `pg-parse` picks) -- found empirically while building this
                        // gate: `emit.rs`'s own compiled lexc tags EVERY root entry (head AND
                        // non-head) with the SAME `root_tag_lexc` convention (`<R:N>`), never
                        // `<M:N>`, regardless of which one `root_morpheme_index` designates. Check
                        // ROOT-ness directly (is `mid` some entry's own morpheme?) rather than
                        // trusting `root_morpheme_index` alone, so this gate's own tag derivation
                        // matches what the compiled net ACTUALLY emits for every root position.
                        if g.entries.iter().any(|e| e.morpheme == mid) {
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
    for surface in &words {
        let normalized = pg_grammar::nfd::nfd(surface);
        let analyses = tag_sequences_for(surface);
        assert!(!analyses.is_empty(), "oracle word {surface:?} has no analysis from the SAME grammar's own Morpher -- oracle/parser inconsistency");
        let any_reachable = analyses
            .iter()
            .any(|tags| recall_reachable(&net, &normalized, tags));
        if !any_reachable {
            missed.push(surface.clone());
        }
    }
    assert!(
        missed.is_empty(),
        "100% recall required on the oracle word list; missed: {missed:?}"
    );

    let p99 = per_word_p99(&words, |surface| {
        let normalized = pg_grammar::nfd::nfd(surface);
        for tags in tag_sequences_for(surface) {
            let _ = recall_reachable(&net, &normalized, &tags);
        }
    });
    assert!(
        p99 < Duration::from_millis(50),
        "per-word p99 {p99:?} exceeds the trip-wire"
    );
}

/// Honest failure (design doc §4c): the FIRST emit-scale (not compose-scale) budget exercised in
/// this suite. `uflexc::emit_underlying_filtered_with_budget` never sees compounding content
/// (module doc), so this is really a plain root-entry line-count check -- a `line_cap` of 3 must
/// trip `EmitLineBudgetExceeded` on this 6-root-entry grammar, mirroring `uflexc.rs`'s own
/// `line_budget_trips_incrementally` precedent exactly (generated instead of hand-authored).
#[test]
fn compounding_overbudget_trips_emit_line_budget() {
    let recipe = overbudget_recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated compounding overbudget XML failed to load: {e}\n{}",
            rendered.xml
        )
    });
    assert_eq!(
        g.entries.len(),
        6,
        "3 independent compounding rules must realize 3 head + 3 non-head = 6 entries"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let budget = ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, usize::MAX, 3, None);

    let err = emit_underlying_filtered_with_budget(&g, &alphabet, None, &budget)
        .expect_err("6 root-entry lines must exceed a line_cap of 3");
    match err {
        ComposeError::EmitLineBudgetExceeded { lines, limit } => {
            assert!(
                lines > 3,
                "must report the line count that actually crossed the cap: {lines}"
            );
            assert_eq!(limit, 3);
        }
        other => panic!("expected EmitLineBudgetExceeded, got {other:?}"),
    }
}
