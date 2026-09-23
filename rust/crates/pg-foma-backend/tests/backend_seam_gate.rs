//! Differential measurement for the backend-seam consolidation: what `backend_selection::capability_shape_key` resolves for every fixture-observed diagnostic, and which dispatcher realizes each `EmissionStrategy`, pinned so a later change to either is checked against it inside one build cycle.

use std::collections::BTreeSet;

use pg_conformance_fixtures::{discover_scoped, ConformanceScope};
use pg_foma::capability::CompileDecision;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::strategy_coverage::ALL_STRATEGIES;
use pg_foma_backend::backend_selection::{capability_shape_key_for_test, select_backends};

/// `(predicate id, shape key)`, deduplicated since construct text is fixture-specific free text a pinned literal should not depend on.
type ShapeRow = (&'static str, &'static str);

fn sweep_shape_keys() -> (BTreeSet<ShapeRow>, BTreeSet<&'static str>) {
    let mut rows = BTreeSet::new();
    let mut predicate_ids = BTreeSet::new();
    for fixture in discover_scoped(ConformanceScope::All) {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        if grammar.char_tables.is_empty() {
            continue;
        }
        let semantics = GrammarSemantics::derive(&grammar);
        let selection = select_backends(&semantics);
        for &strategy in ALL_STRATEGIES {
            let Some(report) = selection.report_for(strategy) else {
                continue;
            };
            if let CompileDecision::Refuse(diagnostics) = report.decision() {
                for diagnostic in diagnostics {
                    let shape_key = capability_shape_key_for_test(diagnostic);
                    predicate_ids.insert(diagnostic.predicate);
                    rows.insert((diagnostic.predicate, shape_key));
                }
            }
        }
    }
    (rows, predicate_ids)
}

/// Which of the three dispatchers `EmissionStrategy` names today, pinned so the seam consolidation is checked against a recorded fact rather than an assumption re-derived after the change.
fn dispatcher_for(strategy: EmissionStrategy) -> &'static str {
    match strategy {
        EmissionStrategy::TunedSurfaceProbed => {
            "emit::emit_tuned_surface_for_request -> analyzer::FomaProposer::new"
        }
        EmissionStrategy::TemplatedUnderlyingTokens => {
            "emit::emit_underlying_templated -> templated_compile.rs"
        }
        EmissionStrategy::PlanComposed => "enumerate -> build::build_controllable",
    }
}

/// The pinned (predicate, shape key) table and predicate-id set this fixture set observes through every `EmissionStrategy`'s `Refuse` diagnostics.
#[test]
fn backend_seam_shape_key_table_is_pinned() {
    let (rows, predicate_ids) = sweep_shape_keys();

    eprintln!(
        "backend-seam-gate: {} distinct (predicate, shape key) row(s)",
        rows.len()
    );
    for (predicate, shape_key) in &rows {
        eprintln!("  {predicate} -> {shape_key}");
    }
    eprintln!(
        "backend-seam-gate: {} distinct predicate id(s) ever reached a Refuse diagnostic",
        predicate_ids.len()
    );
    for id in &predicate_ids {
        eprintln!("  {id}");
    }

    // Row provenance: seven ids resolve via `GrammarWideCheck` field lookup, `simultaneous.subrule-overlap` via the match's explicit arm, and `strategy-coverage.construct-not-representable` via the sniffing fallback's final default arm alone.
    let expected: BTreeSet<ShapeRow> = [
        ("simultaneous.subrule-overlap", "wide-phonology"),
        (
            "strategy-coverage.construct-not-representable",
            "nonregular-process-morphology",
        ),
        (
            "strategy-coverage.templated-unsupported-shape",
            "nonregular-process-morphology",
        ),
        (
            "strategy-materializer.marker-subtree-not-buildable",
            "plan-composed-missing-subtrees",
        ),
        (
            "strategy-materializer.tokenizable-root-required",
            "nonregular-process-morphology",
        ),
        ("surface-probe.root-spelling-cap", "repeated-application"),
        (
            "templated-route.emission-uncovered",
            "nonregular-process-morphology",
        ),
        (
            "templated-route.rule-cascade-uncompilable",
            "nonregular-process-morphology",
        ),
        (
            "templated-route.tokenizable-root-shape",
            "nonregular-process-morphology",
        ),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        rows, expected,
        "capability_shape_key's observed (predicate, shape key) table changed -- see the printed \
         rows above for what this fixture set now produces"
    );

    let expected_ids: BTreeSet<&'static str> = expected.iter().map(|&(id, _)| id).collect();
    assert_eq!(
        predicate_ids, expected_ids,
        "the set of predicate ids that ever reach a Refuse diagnostic changed"
    );
}

/// Pins today's strategy-to-dispatcher table against the same static fact `dispatcher_for` names.
#[test]
fn backend_seam_dispatcher_table_is_pinned() {
    assert_eq!(
        dispatcher_for(EmissionStrategy::TunedSurfaceProbed),
        "emit::emit_tuned_surface_for_request -> analyzer::FomaProposer::new"
    );
    assert_eq!(
        dispatcher_for(EmissionStrategy::TemplatedUnderlyingTokens),
        "emit::emit_underlying_templated -> templated_compile.rs"
    );
    assert_eq!(
        dispatcher_for(EmissionStrategy::PlanComposed),
        "enumerate -> build::build_controllable"
    );
}
