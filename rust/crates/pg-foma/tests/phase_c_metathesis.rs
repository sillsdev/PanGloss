//! GATE (`docs/fst-plan/phase-c-generator-design.md` §6, priority (7)): `MetathesisRuleDef`
//! HONEST-SKIP bail gate -- pure test-writing, `compile_and_compose_rules_with_budget`'s own match
//! on [`PhonRuleDef`] already routes every `PhonRuleDef::Metathesis` straight to
//! `skipped.push(format!("{} (metathesis, unhandled)", m.xml_id))`, with no compile attempt at all
//! (design doc §5's "Honest skip now" list).

mod common;

use foma::options::FomaOptions;

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::replace::{compile_and_compose_rules_with_budget, SegAlphabet};
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

fn recipe() -> Recipe {
    Recipe {
        name: "phase-c-metathesis",
        seed: 20260720,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            metathesis_rule_count: 1,
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
fn metathesis_rule_is_honestly_reported_skipped() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated metathesis XML failed to load: {e}\n{}",
            rendered.xml
        )
    });

    let metathesis = rendered
        .metathesis
        .as_ref()
        .expect("recipe declared metathesis_rule_count > 0");
    assert_eq!(metathesis.rule_xml_ids.len(), 1);
    let metathesis_rules = g
        .prules
        .iter()
        .filter(|p| matches!(p, PhonRuleDef::Metathesis(_)))
        .count();
    assert_eq!(metathesis_rules, 1);
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

    let expected_skip = format!("{} (metathesis, unhandled)", metathesis.rule_xml_ids[0]);
    assert_eq!(
        skipped,
        vec![expected_skip],
        "the metathesis rule must be reported skipped with its own documented annotation"
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
