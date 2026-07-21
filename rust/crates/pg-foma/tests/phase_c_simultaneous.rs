//! GATE (`docs/fst-plan/phase-c-generator-design.md` §6, priority (7)): `RewriteMode::Simultaneous`
//! HONEST-SKIP bail gate -- needs the detection wiring this same Phase C change adds
//! (`pg_foma::replace::is_fully_supported_shape`, wired into `compile_rewrite_rule_subset`; see that
//! function's own doc). BEFORE this change, a `multipleApplicationOrder="simultaneous"` rule was
//! silently compiled as if `Iterative` -- a WRONG network with no signal (design doc §5's "SILENT
//! MIS-MAP" row). This gate pins the fix: the rule is now DETECTED and reported `skipped`, exactly
//! like metathesis, never silently mis-compiled.

mod common;

use foma::options::FomaOptions;

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::replace::{
    compile_and_compose_rules_with_budget, is_fully_supported_shape, SegAlphabet,
};
use pg_grammar::model::{Grammar, PhonRuleDef, RewriteMode};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

fn recipe() -> Recipe {
    Recipe {
        name: "phase-c-simultaneous",
        seed: 20260720,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            simultaneous_rule_count: 1,
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
fn simultaneous_rule_is_detected_and_honestly_reported_skipped() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated simultaneous XML failed to load: {e}\n{}",
            rendered.xml
        )
    });

    let simultaneous = rendered
        .simultaneous
        .as_ref()
        .expect("recipe declared simultaneous_rule_count > 0");
    assert_eq!(simultaneous.rule_xml_ids.len(), 1);
    assert_eq!(g.prules.len(), 1);
    assert_eq!(g.entries.len(), 1);

    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule at prules[0]");
    };
    assert_eq!(rule.mode, RewriteMode::Simultaneous, "recipe's own multipleApplicationOrder=\"simultaneous\" must round-trip to RewriteMode::Simultaneous");
    assert!(
        !is_fully_supported_shape(rule),
        "a Simultaneous-mode rule must NOT be reported fully-supported"
    );

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

    assert_eq!(skipped, vec![simultaneous.rule_xml_ids[0].clone()], "the Simultaneous-mode rule must be the ONLY skipped rule, and must be reported (never silently mis-compiled)");
    assert!(
        composed.is_none(),
        "zero compilable rules -- the cascade must be a no-op, never a wrong network"
    );
    assert!(
        tuple_reports.is_empty(),
        "a skipped rule contributes no alpha-tuple report"
    );
}
