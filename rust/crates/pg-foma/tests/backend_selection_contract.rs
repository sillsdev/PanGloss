use pg_foma::backend_selection::{BackendReport, BackendStatus};
use pg_foma::capability::{CapabilityDiagnostic, CompileDecision};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::health::{FindingCode, Severity};

fn refused(strategy: EmissionStrategy) -> BackendReport {
    BackendReport::refused(
        strategy,
        CompileDecision::Refuse(vec![CapabilityDiagnostic {
            predicate: "synthetic.test-only",
            construct: "unsupported".to_string(),
            witness: "synthetic".to_string(),
        }]),
    )
}

#[test]
fn accepted_constructor_rejects_refusal() {
    let result = BackendReport::accepted(
        EmissionStrategy::TunedSurfaceProbed,
        CompileDecision::Refuse(Vec::new()),
        vec![],
    );
    assert_eq!(
        result,
        Err("an accepted backend report cannot carry a refusal")
    );
}

#[test]
fn capability_refusal_is_a_typed_cannot_represent_with_actionable_advice() {
    let report = refused(EmissionStrategy::TunedSurfaceProbed);
    assert_eq!(report.status(), BackendStatus::Refused);
    assert_eq!(report.worst_severity(), Severity::CannotRepresent);
    assert_eq!(report.findings().len(), 1);
    assert_eq!(
        report.findings()[0].code,
        FindingCode::BackendCoverageIncomplete
    );
    assert!(!report.failed_predicates().is_empty());
    assert!(!report.shapes().is_empty());
    assert!(!report.advice_references().is_empty());
}

#[test]
fn missing_and_failed_backends_are_typed_errors_carrying_no_grammar_advice() {
    for (report, expected_code) in [
        (
            BackendReport::missing(
                EmissionStrategy::TemplatedUnderlyingTokens,
                "backend executable is unavailable",
            ),
            FindingCode::BuildProcessFailed,
        ),
        (
            BackendReport::failed(
                EmissionStrategy::PlanComposed,
                "compiler process exited unsuccessfully",
            ),
            FindingCode::BackendCompilationFailed,
        ),
    ] {
        assert_eq!(report.worst_severity(), Severity::NotProductionReady);
        assert_eq!(report.findings().len(), 1);
        assert_eq!(
            report.findings()[0].code,
            expected_code,
            "missing() (no tool to run) must emit BuildProcessFailed, and failed() (a compile \
             attempt that ran and failed) must emit BackendCompilationFailed -- each matching its \
             own documented meaning, not the other's"
        );
        assert!(
            report.shapes().is_empty() && report.advice_references().is_empty(),
            "a backend whose compiler is absent or died is a PanGloss defect, and the advice \
             catalog advises GRAMMAR changes -- reporting a remedy here would tell a language \
             owner to edit a grammar that is not what went wrong: {report:?}"
        );
    }
}

#[test]
fn plan_composed_required_subtrees_are_a_typed_cannot_represent_refusal() {
    // truncate-morphotactic is now admitted (non-empty, complete material); mpr-gated-exception's empty material is never admitted, a stable witness.
    let grammar_xml = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../machine/conformance/edge-cases/mpr-gated-exception/grammar.xml"
    ));
    let grammar = pg_grammar::load(grammar_xml).expect("marker fixture must load");
    let selection = pg_foma::backend_selection::select_backends_for_grammar(&grammar);
    let composed = selection
        .report_for(EmissionStrategy::PlanComposed)
        .expect("PlanComposed must always have one report");

    assert_eq!(composed.status(), BackendStatus::Refused);
    assert_eq!(composed.worst_severity(), Severity::CannotRepresent);
    assert!(composed.findings().iter().any(|finding| {
        finding.code == FindingCode::BackendCoverageIncomplete
            && finding
                .affected
                .iter()
                .any(|affected| affected.contains("Composite") && affected.contains("Marker"))
    }));
    assert!(composed
        .shapes()
        .contains(&"plan-composed-missing-subtrees".to_string()));
    assert!(composed.declined_on().iter().any(|diagnostic| {
        diagnostic.predicate == "strategy-materializer.marker-subtree-not-buildable"
            && diagnostic.witness.contains("build_controllable")
    }));
    assert!(!composed.failed_predicates().is_empty());
    assert!(!composed.advice_references().is_empty());
}
