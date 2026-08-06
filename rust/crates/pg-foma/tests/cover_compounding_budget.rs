//! The compound HEAD x NON-HEAD root-pair budget must trip before any lexc text is emitted for an oversized cross product; kept in its own test file/process since it mutates the process-global `HC_COMPOUND_PAIR_BUDGET` env var and `cargo test` files run as separate processes.

use pg_foma::emit;
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

fn overbudget_recipe() -> Recipe {
    Recipe {
        name: "cover-compounding-budget",
        seed: 20260725,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            compounding_rule_count: 3,
            ..Default::default()
        },
    }
}

/// `HC_COMPOUND_PAIR_BUDGET=20` against a 6-root (36-pair) grammar must trip `EnumBudgetExceeded`, reported `Unsupported`, with an empty `lexc_source` (never a partial/unsound network).
#[test]
fn compound_pair_budget_trips_before_any_lexc_emitted() {
    // This is the only test in this file/process, so no other test can observe this env var mutation racily.
    std::env::set_var("HC_COMPOUND_PAIR_BUDGET", "20");

    let recipe = overbudget_recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated compounding-budget XML failed to load: {e}\n{}",
            rendered.xml
        )
    });
    assert_eq!(
        g.entries.len(),
        6,
        "3 independent compounding rules must realize 3 head + 3 non-head = 6 entries, matching \
         tests/phase_c_compounding.rs's own overbudget_recipe precedent"
    );

    let result = emit::emit(&g);
    std::env::remove_var("HC_COMPOUND_PAIR_BUDGET");

    assert!(
        result.lexc_source.is_empty(),
        "an over-budget compound grammar must never emit a partial/unsound network"
    );
    match result.report.tier {
        emit::FomaTier::Unsupported { ref reason } => {
            assert!(
                reason.contains("compound"),
                "the refusal reason must name the compound cross product: {reason:?}"
            );
        }
        other => panic!("expected FomaTier::Unsupported, got {other:?}"),
    }
    let exceeded = result
        .report
        .enum_budget_exceeded
        .expect("must report an EnumBudgetExceeded for the compound-pair measure");
    assert_eq!(exceeded.measure, "compound head x non-head root pairs");
    assert_eq!(exceeded.limit, 20);
    assert!(
        exceeded.value > 20,
        "reported value {} must exceed the limit it tripped",
        exceeded.value
    );
    assert_eq!(
        exceeded.value, 36,
        "6 unrestricted roots on both sides must yield the full 6x6 cross product"
    );
}
