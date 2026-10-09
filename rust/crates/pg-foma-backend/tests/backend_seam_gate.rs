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
type PartialFixtureRow = (&'static str, &'static str, &'static str, &'static str);
type PartialAdmissionRow = (&'static str, &'static str);

const PARTIAL_CLASS_DISAGREEMENT_FIXTURES: &[&str] = &[
    "staging:edge-cases/partial-class-disagree-bounded-rtl-right",
    "staging:edge-cases/partial-class-disagree-unbounded-rtl-left",
    "staging:edge-cases/partial-class-disagree-bounded-ltr-left",
    "staging:edge-cases/partial-class-disagree-bounded-rtl-left",
    "staging:edge-cases/partial-class-disagree-unbounded-ltr-right",
    "staging:edge-cases/partial-class-disagree-unbounded-rtl-right",
];

fn sweep_shape_keys() -> (
    BTreeSet<ShapeRow>,
    BTreeSet<&'static str>,
    BTreeSet<PartialFixtureRow>,
    BTreeSet<PartialAdmissionRow>,
) {
    let mut rows = BTreeSet::new();
    let mut predicate_ids = BTreeSet::new();
    let mut partial_fixture_rows = BTreeSet::new();
    let mut partial_fixture_admissions = BTreeSet::new();
    for fixture in discover_scoped(ConformanceScope::All) {
        let label = fixture.label();
        let partial_label = PARTIAL_CLASS_DISAGREEMENT_FIXTURES
            .iter()
            .copied()
            .find(|partial| *partial == label);
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
            if let Some(partial_label) = partial_label {
                if report.can_represent() {
                    partial_fixture_admissions.insert((partial_label, strategy.label()));
                }
            }
            if let CompileDecision::Refuse(diagnostics) = report.decision() {
                for diagnostic in diagnostics {
                    let shape_key = capability_shape_key_for_test(diagnostic);
                    eprintln!(
                        "SEAM_ROW\t{}\t{strategy:?}\t{}\t{shape_key}",
                        fixture.label(),
                        diagnostic.predicate
                    );
                    predicate_ids.insert(diagnostic.predicate);
                    rows.insert((diagnostic.predicate, shape_key));
                    if let Some(partial_label) = partial_label {
                        partial_fixture_rows.insert((
                            partial_label,
                            strategy.label(),
                            diagnostic.predicate,
                            shape_key,
                        ));
                    }
                }
            }
        }
    }
    (
        rows,
        predicate_ids,
        partial_fixture_rows,
        partial_fixture_admissions,
    )
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
    let (rows, predicate_ids, partial_fixture_rows, partial_fixture_admissions) =
        sweep_shape_keys();

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

    let expected: BTreeSet<ShapeRow> = [
        ("quantifier.bounded-expansion", "repeated-application"),
        (
            "reduplication.peel-eligible-rule-kind",
            "nonregular-process-morphology",
        ),
        (
            "right-to-left-rewrite.faithful-reversal-construction",
            "wide-phonology",
        ),
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

    let expected_partial_admissions: BTreeSet<PartialAdmissionRow> =
        PARTIAL_CLASS_DISAGREEMENT_FIXTURES
            .iter()
            .flat_map(|&fixture| {
                [
                    EmissionStrategy::TunedSurfaceProbed,
                    EmissionStrategy::TemplatedUnderlyingTokens,
                ]
                .into_iter()
                .map(move |strategy| (fixture, strategy.label()))
            })
            .collect();
    assert_eq!(
        partial_fixture_admissions, expected_partial_admissions,
        "partial-class fixture admission changed outside the approved TSP/TUT lowering"
    );

    let expected_partial_refusals: BTreeSet<PartialFixtureRow> =
        PARTIAL_CLASS_DISAGREEMENT_FIXTURES
            .iter()
            .map(|&fixture| {
                (
                    fixture,
                    EmissionStrategy::PlanComposed.label(),
                    "strategy-materializer.marker-subtree-not-buildable",
                    "plan-composed-missing-subtrees",
                )
            })
            .collect();
    assert_eq!(
        partial_fixture_rows, expected_partial_refusals,
        "partial-class fixtures must retain only their pinned PlanComposed refusal row"
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
