use pg_foma::capability::{CapabilityDiagnostic, CompileDecision};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma_backend::backend_selection::{BackendReport, BackendStatus};
use pg_foma_backend::health::{FindingCode, Severity};

fn refused(strategy: EmissionStrategy) -> BackendReport {
    // Exercise the compiler-facing constructor; author advice is owned by grammar-health.
    pg_foma_backend::backend_selection::refused(
        strategy,
        CompileDecision::Refuse(vec![CapabilityDiagnostic {
            refusal: pg_health::capability::CapabilityRefusal::StrategyCoverage,
            predicate: "surface-probe.root-spelling-cap",
            construct: "unsupported".to_string(),
            witness: "synthetic".to_string(),
        }]),
    )
    .expect("a refusal with one registered predicate is a valid refused report")
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
fn capability_refusal_is_a_typed_cannot_represent_with_grammar_health_guidance() {
    let report = refused(EmissionStrategy::TunedSurfaceProbed);
    assert_eq!(report.status(), BackendStatus::Refused);
    assert_eq!(report.worst_severity(), Severity::CannotRepresent);
    assert_eq!(report.findings().len(), 1);
    assert_eq!(
        report.findings()[0].code,
        FindingCode::BackendCoverageIncomplete
    );
    assert!(!report.failed_predicates().is_empty());
    assert!(report.shapes().is_empty());
    assert!(report.advice_references().is_empty());
    assert!(report.findings()[0]
        .explanation
        .contains("see grammar-health"));
}

#[test]
fn missing_and_failed_backends_are_typed_errors_carrying_no_grammar_advice() {
    for (report, expected_code) in [
        (
            BackendReport::missing(
                EmissionStrategy::TunedSurfaceProbed,
                "backend executable is unavailable",
            ),
            FindingCode::BuildProcessFailed,
        ),
        (
            BackendReport::failed(
                EmissionStrategy::TunedSurfaceProbed,
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
