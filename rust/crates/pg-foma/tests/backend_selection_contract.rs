use pg_foma::backend_selection::{
    BackendReport, BackendSelection, BackendStatus, BACKEND_PREFERENCE,
};
use pg_foma::capability::{CapabilityDiagnostic, CompileDecision};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::health::{
    FindingCode, HealthFinding, Metric, MetricValue, Phase, Severity, ValueProvenance,
};

fn finding(severity: Severity, code: FindingCode) -> HealthFinding {
    HealthFinding {
        code,
        severity,
        phase: Phase::Compile,
        affected: vec!["synthetic-rule".to_string()],
        metric: Metric::EmittedLineCount,
        value: MetricValue::Count(1),
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation: "synthetic finding".to_string(),
        remedies: Vec::new(),
        override_record: None,
    }
}

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
fn reports_retain_every_backend_and_rank_only_normal_candidates() {
    let selection = BackendSelection::from_reports(vec![
        BackendReport::accepted(
            EmissionStrategy::TunedSurfaceProbed,
            CompileDecision::Admit,
            vec![finding(Severity::Warning, FindingCode::PayloadSizeBand)],
        ),
        BackendReport::accepted(
            EmissionStrategy::TemplatedUnderlyingTokens,
            CompileDecision::Admit,
            vec![],
        ),
        refused(EmissionStrategy::PlanComposed),
    ]);

    assert_eq!(selection.reports().len(), BACKEND_PREFERENCE.len());
    assert_eq!(
        selection.selected(),
        vec![
            EmissionStrategy::TemplatedUnderlyingTokens,
            EmissionStrategy::TunedSurfaceProbed,
        ]
    );
    assert_eq!(
        selection.preferred(),
        Some(EmissionStrategy::TemplatedUnderlyingTokens)
    );
    assert_eq!(
        selection
            .report_for(EmissionStrategy::PlanComposed)
            .unwrap()
            .status(),
        BackendStatus::Refused
    );
    assert_eq!(
        selection
            .report_for(EmissionStrategy::PlanComposed)
            .unwrap()
            .failed_predicates(),
        &["synthetic.test-only".to_string()]
    );
}

#[test]
fn error_and_critical_reports_are_retained_but_not_selected() {
    let selection = BackendSelection::from_reports(vec![
        BackendReport::accepted(
            EmissionStrategy::TunedSurfaceProbed,
            CompileDecision::Admit,
            vec![finding(Severity::Error, FindingCode::PayloadSizeBand)],
        ),
        BackendReport::accepted(
            EmissionStrategy::TemplatedUnderlyingTokens,
            CompileDecision::Admit,
            vec![finding(
                Severity::Critical,
                FindingCode::UnknownUnboundedConstruct,
            )],
        ),
    ]);

    assert!(selection.selected().is_empty());
    assert_eq!(selection.reports().len(), BACKEND_PREFERENCE.len());
    assert_eq!(
        selection
            .report_for(EmissionStrategy::TunedSurfaceProbed)
            .unwrap()
            .worst_severity(),
        Severity::Error
    );
    assert_eq!(
        selection
            .report_for(EmissionStrategy::PlanComposed)
            .unwrap()
            .status(),
        BackendStatus::Missing
    );
}

#[test]
fn selected_candidates_can_be_limited_to_two() {
    let selection = BackendSelection::from_reports(
        BACKEND_PREFERENCE
            .iter()
            .enumerate()
            .map(|(index, &strategy)| {
                BackendReport::accepted(
                    strategy,
                    CompileDecision::Admit,
                    if index == 0 {
                        vec![finding(Severity::Info, FindingCode::PayloadSizeBand)]
                    } else {
                        vec![]
                    },
                )
            })
            .collect(),
    );

    assert_eq!(selection.select_up_to(2).len(), 2);
    assert_eq!(
        selection.select_up_to(2)[0],
        EmissionStrategy::TemplatedUnderlyingTokens
    );
    assert_eq!(selection.select_up_to(2)[1], EmissionStrategy::PlanComposed);
}
