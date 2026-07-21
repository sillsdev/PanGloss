//! GATE (`docs/fst-plan/phase-c-generator-design.md` §6, priority (7)): quantifier /
//! `OptionalSegmentSequence` HONEST-SKIP bail gate -- pure test-writing, the loader/compiler already
//! reports this construct as `skipped`; this gate pins that it stays that way (never silently
//! mis-compiled).
//!
//! `pg_foma::replace::pattern_slots` returns `None` on any `PatternNode::Quantifier` it meets in a
//! REWRITE rule's own LHS/RHS/environment, which `compile_rewrite_rule_subset` turns into `Ok(None)`
//! for the whole rule; `compile_and_compose_rules_with_budget` reports that via `skipped.push(rule.
//! xml_id.clone())` (design doc §5's "Honest skip now" list).

mod common;

use foma::options::FomaOptions;

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::replace::{compile_and_compose_rules_with_budget, SegAlphabet};
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

fn recipe() -> Recipe {
    Recipe {
        name: "phase-c-quantifier",
        seed: 20260720,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            quantifier_bound: Some((1, 3)),
            ..Default::default()
        },
    }
}

fn rules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
    g.strata[0]
        .prules
        .iter()
        .map(|&id| &g.prules[id.0 as usize])
        .collect()
}

#[test]
fn quantifier_rule_is_honestly_reported_skipped() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated quantifier XML failed to load: {e}\n{}",
            rendered.xml
        )
    });

    let quantifier = rendered
        .quantifier
        .as_ref()
        .expect("recipe declared quantifier_bound.is_some()");
    assert_eq!(g.prules.len(), 1);
    assert_eq!(g.entries.len(), 1);

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro = rules_in_order(&g);
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );

    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let composed = compile_and_compose_rules_with_budget(
        &opts,
        &g,
        &alphabet,
        &ro,
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("compile must not hit any budget: {e}"));

    // The construct's own generator-doc contract: the quantifier-bearing rule is DETECTED and
    // reported -- not silently mis-compiled into a wrong network (design doc §5).
    assert_eq!(
        skipped,
        vec![quantifier.rule_xml_id.clone()],
        "the quantifier rule must be the ONLY skipped rule, and must be reported (never silent)"
    );
    assert!(
        composed.is_none(),
        "zero compilable rules -- the cascade must be a no-op, never a wrong network"
    );
    assert!(
        tuple_reports.is_empty(),
        "a skipped rule contributes no alpha-tuple report"
    );
}
