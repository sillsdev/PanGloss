//! GATE (`docs/fst-plan/phase-c-generator-design.md` §6, priority (3)): partition-k / MPR-POS
//! subrule gating -- recall-parity + `_overbudget` (`GroupBudgetExceeded`).
//!
//! ## Why `pg_foma::gate::compile_gated_grammar_with_budget`, not a hand-assembled compose
//! Unlike GATE 1/GATE 2 (which build their own net by hand from `uflexc`/`emit`), this gate calls
//! the PRODUCTION gating entry point directly: [`pg_foma::gate::compile_gated_grammar_with_budget`]
//! already does everything (`find_gated_subrules` -> `partition_entries` -> per-group
//! lexc+rules compile -> disjoint union) -- exactly the mechanism `pg_grammar_gen::build::gating`
//! generalizes `pg-foma/src/gate.rs`'s own `sixteen_group_fixture_xml` test to exercise, generated
//! instead of hand-authored.
//!
//! ## Recall technique
//! Each of the `2^k` generated entries is a BARE root (no morphological rules at all) whose only
//! grammar content is the `k` gated phonological rules on its own stratum -- so
//! `pg_grammar_gen::oracle::sweep`'s bare-root generation (`GenMorpheme` list empty) already runs
//! the REAL per-stratum phonological cascade (`pg_rules::rewrite::subrule_applicable`, the exact
//! predicate `crate::gate::entry_gate_key` also calls -- `pg-foma/src/gate.rs`'s own module doc),
//! giving ground truth for which marker positions this specific entry's own gating key flips. The
//! compose-recall check (`tests/common/gate_template.rs`) then verifies the `compile_gated_grammar`
//! net relates that SAME surface string to the SAME root tag, in [`SegAlphabet`] token space
//! (mirrors `tests/phase_c_multi_table.rs`'s own technique).

mod common;

use std::time::{Duration, Instant};

use foma::options::FomaOptions;

use pg_foma::compose_budget::{ComposeBudget, ComposeError};
use pg_foma::gate::compile_gated_grammar_with_budget;
use pg_foma::replace::SegAlphabet;
use pg_foma::tags;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_grammar_gen::oracle::{sweep, OracleOpts};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

use common::gate_template::{assert_net_size_within, entry_id_of, p99, recall_reachable};

fn recipe() -> Recipe {
    Recipe {
        name: "phase-c-partition-k",
        seed: 20260720,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            gated_subrule_count: 2,
            ..Default::default()
        },
    }
}

fn overbudget_recipe() -> Recipe {
    Recipe {
        name: "phase-c-partition-k-overbudget",
        seed: 20260720,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            gated_subrule_count: 4,
            ..Default::default()
        },
    }
}

/// Same convention as `p6_gate_parity.rs`'s own `rules_in_order`: stratum 0's `phonologicalRules`
/// id-list, resolved to `&PhonRuleDef`s, document order.
fn rules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
    g.strata[0]
        .prules
        .iter()
        .map(|&id| &g.prules[id.0 as usize])
        .collect()
}

#[test]
fn partition_k_recall_parity_via_generator_and_oracle() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated partition-k XML failed to load: {e}\n{}",
            rendered.xml
        )
    });

    let gating = rendered
        .gating
        .as_ref()
        .expect("recipe declared gated_subrule_count > 0");
    assert_eq!(gating.rule_xml_ids.len(), 2);
    assert_eq!(
        gating.entry_xml_ids.len(),
        4,
        "2 independent gated subrules must realize 2^2 = 4 entries"
    );
    assert_eq!(g.entries.len(), 4);
    assert_eq!(g.prules.len(), 2);

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro = rules_in_order(&g);
    // Never usize::MAX-with-a-purpose here -- this test is about recall, not the budget mechanism
    // (that's the overbudget test's job); an effectively-unbounded budget just proves the compile
    // itself doesn't spuriously trip on a grammar this tiny.
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );

    let result = compile_gated_grammar_with_budget(&opts, &g, &alphabet, &ro, &budget)
        .unwrap_or_else(|e| panic!("gated compile must not hit any budget: {e}"));
    assert_eq!(
        result.groups, 4,
        "2 independent gated subrules must realize 2^2 = 4 groups"
    );
    // NOTE: `result.skipped_rules` is NOT expected to be empty here -- `compile_and_compose_rules_
    // gated_with_budget`'s own doc records that a gated rule whose ONLY subrule is excluded for a
    // given group (i.e. simply not applicable to that group, the ordinary/expected case) is
    // reported via the SAME per-group `None` path a genuinely-unsupported construct would use ("no
    // NEW branch is introduced at any call site"); `p6_gate_parity.rs`'s own gated-grammar tests
    // (`indonesian_mpr_exclusion_matches_oracle`) likewise never assert this list is empty. The
    // real correctness signal is the recall check below, not this diagnostic list.
    println!(
        "partition-k: skipped_rules across all groups = {:?}",
        result.skipped_rules
    );
    let net = result.net.expect("gated compile must produce a network");

    // Resource envelope (design doc §4b): a 4-entry, 2-rule gated grammar must stay tiny.
    assert_net_size_within(&net, 500, 5_000);

    // Oracle (design doc §3): bare-root generation per entry -- module doc.
    let oracle_opts = OracleOpts {
        step_cap: 20_000,
        word_timeout: Some(Duration::from_millis(500)),
        max_rules_per_root: 0,
        max_total_words: 10,
    };
    let width = tags::tag_width(g.morphemes.len());

    let mut missed = Vec::new();
    let mut samples = Vec::new();
    for xml_id in &gating.entry_xml_ids {
        let entry_id = entry_id_of(&g, xml_id);
        let words = sweep(&g, &[entry_id], &[], &oracle_opts);
        assert_eq!(words.len(), 1, "bare-root generation must produce exactly 1 surface form for entry {xml_id:?}, got {words:?}");
        let surface = &words[0].surface;
        let tag = tags::root_tag_text(g.entries[entry_id.0 as usize].morpheme, width);
        let encoded = alphabet.encode_query(surface).unwrap_or_else(|| {
            panic!("entry {xml_id:?}'s oracle surface {surface:?} must segment against table 0")
        });

        let t0 = Instant::now();
        let ok = recall_reachable(&net, &encoded, &[tag]);
        samples.push(t0.elapsed());
        if !ok {
            missed.push((xml_id.clone(), surface.clone()));
        }
    }
    assert!(
        missed.is_empty(),
        "100% recall required on the 4 gating-key entries; missed: {missed:?}"
    );

    let per_word_p99 = p99(samples);
    assert!(
        per_word_p99 < Duration::from_millis(50),
        "per-word p99 {per_word_p99:?} exceeds the trip-wire"
    );
}

/// Honest failure (design doc §4c): 4 independent gated subrules realize `2^4 = 16` distinct
/// gating groups (mirrors `pg-foma/src/gate.rs`'s own `sixteen_group_fixture_xml` precedent
/// exactly, generated instead of hand-authored) -- a `group_cap` of 8 must trip
/// `GroupBudgetExceeded` BEFORE any per-group compile work runs (V6, fail-fast, well under 200ms).
#[test]
fn partition_k_overbudget_trips_group_budget_fast() {
    let recipe = overbudget_recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated partition-k overbudget XML failed to load: {e}\n{}",
            rendered.xml
        )
    });
    assert_eq!(
        g.entries.len(),
        16,
        "4 independent gated subrules must realize 2^4 = 16 entries"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro = rules_in_order(&g);
    let budget = ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, 8, usize::MAX, None);

    let start = Instant::now();
    let err = compile_gated_grammar_with_budget(&opts, &g, &alphabet, &ro, &budget)
        .expect_err("16 groups must exceed a group_cap of 8");
    let elapsed = start.elapsed();

    match err {
        ComposeError::GroupBudgetExceeded {
            groups,
            limit,
            gated_subrules,
        } => {
            assert_eq!(groups, 16);
            assert_eq!(limit, 8);
            assert_eq!(gated_subrules, 4);
        }
        other => panic!("expected GroupBudgetExceeded, got {other:?}"),
    }
    assert!(
        elapsed < Duration::from_millis(200),
        "group budget must trip BEFORE any per-group compile work runs (took {elapsed:?})"
    );
}
