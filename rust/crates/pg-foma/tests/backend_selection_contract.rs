use pg_foma::advice_catalog::RemedyEffort;
use pg_foma::backend_selection::{
    sort_blocking_remedy_sets, AdviceReference, BackendReport, BackendSelection, BackendStatus,
    BACKEND_PREFERENCE,
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

fn advice(shape: &str, remedy: &str, effort: RemedyEffort) -> AdviceReference {
    AdviceReference::new(shape, remedy, effort)
}

#[test]
fn reports_retain_every_backend_and_rank_only_normal_candidates() {
    let selection = BackendSelection::from_reports(vec![
        BackendReport::accepted(
            EmissionStrategy::TunedSurfaceProbed,
            CompileDecision::Admit,
            vec![finding(Severity::LargeMultiplier, FindingCode::PayloadSizeBand)],
        )
        .unwrap(),
        BackendReport::accepted(
            EmissionStrategy::TemplatedUnderlyingTokens,
            CompileDecision::Admit,
            vec![],
        )
        .unwrap(),
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
fn not_production_ready_and_machine_limit_reports_are_retained_but_not_selected() {
    let selection = BackendSelection::from_reports(vec![
        BackendReport::accepted(
            EmissionStrategy::TunedSurfaceProbed,
            CompileDecision::Admit,
            vec![finding(Severity::NotProductionReady, FindingCode::PayloadSizeBand)],
        )
        .unwrap(),
        BackendReport::accepted(
            EmissionStrategy::TemplatedUnderlyingTokens,
            CompileDecision::Admit,
            vec![finding(
                Severity::MachineLimit,
                FindingCode::UnknownUnboundedConstruct,
            )],
        )
        .unwrap(),
    ]);

    assert!(selection.selected().is_empty());
    assert_eq!(selection.reports().len(), BACKEND_PREFERENCE.len());
    assert_eq!(
        selection
            .report_for(EmissionStrategy::TunedSurfaceProbed)
            .unwrap()
            .worst_severity(),
        Severity::NotProductionReady
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
                        vec![finding(Severity::Elevated, FindingCode::PayloadSizeBand)]
                    } else {
                        vec![]
                    },
                )
                .unwrap()
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

#[test]
fn blocking_remedy_sets_sort_hard_then_medium_then_easy_and_deduplicate() {
    let ordered = sort_blocking_remedy_sets(vec![
        vec![advice("shape-hard", "shared", RemedyEffort::Hard)],
        vec![advice("shape-medium", "shared", RemedyEffort::Medium)],
        vec![
            advice("shape-easy", "shared", RemedyEffort::Easy),
            advice("shape-easy", "shared", RemedyEffort::Easy),
        ],
        vec![
            advice("shape-medium-easy", "shared", RemedyEffort::Medium),
            advice("shape-medium-easy", "other", RemedyEffort::Easy),
        ],
    ]);

    assert_eq!(
        ordered,
        vec![
            vec![advice("shape-easy", "shared", RemedyEffort::Easy)],
            vec![advice("shape-medium", "shared", RemedyEffort::Medium)],
            vec![
                advice("shape-medium-easy", "other", RemedyEffort::Easy),
                advice("shape-medium-easy", "shared", RemedyEffort::Medium),
            ],
            vec![advice("shape-hard", "shared", RemedyEffort::Hard)],
        ]
    );
}

#[test]
fn shared_remedies_keep_shape_specific_effort_and_report_refs_are_stable() {
    let report = BackendReport::accepted(
        EmissionStrategy::TunedSurfaceProbed,
        CompileDecision::Admit,
        vec![],
    )
    .unwrap()
    .with_diagnostics(
        vec![],
        vec![],
        vec![],
        vec![
            advice("shape-b", "shared-order", RemedyEffort::Hard),
            advice("shape-a", "shared-order", RemedyEffort::Easy),
            advice("shape-a", "shared-order", RemedyEffort::Easy),
        ],
    );

    assert_eq!(
        report.advice_references(),
        &[
            advice("shape-a", "shared-order", RemedyEffort::Easy),
            advice("shape-b", "shared-order", RemedyEffort::Hard),
        ]
    );
    assert!(
        report.is_selected(),
        "remedy effort must not override correctness/health"
    );
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
fn missing_and_failed_backends_are_typed_errors_with_shared_advice() {
    for report in [
        BackendReport::missing(
            EmissionStrategy::TemplatedUnderlyingTokens,
            "backend executable is unavailable",
        ),
        BackendReport::failed(
            EmissionStrategy::PlanComposed,
            "compiler process exited unsuccessfully",
        ),
    ] {
        assert_eq!(report.worst_severity(), Severity::NotProductionReady);
        assert_eq!(report.findings().len(), 1);
        assert!(matches!(
            report.findings()[0].code,
            FindingCode::BackendCompilationFailed | FindingCode::BuildProcessFailed
        ));
        assert_eq!(report.shapes(), &["backend-build-unavailable".to_string()]);
        assert!(!report.advice_references().is_empty());
    }
}

#[test]
fn tuned_surface_resource_finding_is_reported_and_not_production_ready_is_not_selected() {
    let grammar_xml = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../machine/conformance/edge-cases/truncate-morphotactic/grammar.xml"
    ));
    let grammar = pg_grammar::load(grammar_xml).expect("resource fixture must load");
    let selection =
        pg_foma::backend_selection::select_backends_for_grammar_with_tuned_closure_work_limit(
            &grammar, 1,
        );
    let tuned = selection
        .report_for(EmissionStrategy::TunedSurfaceProbed)
        .expect("TunedSurface must always have one report");

    assert_eq!(selection.reports().len(), BACKEND_PREFERENCE.len());
    assert_eq!(tuned.worst_severity(), Severity::NotProductionReady);
    assert_eq!(tuned.findings().len(), 1);
    assert_eq!(
        tuned.findings()[0].code,
        FindingCode::ProvenBoundExceedsBudget
    );
    assert_eq!(tuned.findings()[0].metric, Metric::CompositeRulePairCount);
    assert_eq!(
        tuned.shapes(),
        &["tuned-surface-resource-envelope".to_string()]
    );
    assert!(tuned
        .cost_evidence()
        .iter()
        .any(|evidence| evidence.metric == Metric::CompositeRulePairCount));
    assert!(!tuned.advice_references().is_empty());
    assert!(
        !selection
            .selected()
            .contains(&EmissionStrategy::TunedSurfaceProbed),
        "a proven resource NotProductionReady finding remains reportable but cannot receive an implicit override"
    );
    assert!(
        selection.is_no_path(),
        "the fixture has no complete route: TunedSurface exceeds the named envelope, Templated \
         refuses its unordered rules, and PlanComposed cannot build its required structural \
         subtree: {selection:?}"
    );

    let retried =
        pg_foma::backend_selection::select_backends_for_grammar_with_tuned_closure_work_limit(
            &grammar,
            usize::MAX,
        );
    assert_ne!(
        retried
            .report_for(EmissionStrategy::TunedSurfaceProbed)
            .expect("the retry must retain the TunedSurface report")
            .worst_severity(),
        Severity::NotProductionReady,
        "a larger named envelope must rerun characterization instead of preserving the NotProductionReady finding"
    );
}

#[test]
fn plan_composed_required_subtrees_are_a_typed_cannot_represent_refusal() {
    let grammar_xml = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../machine/conformance/edge-cases/truncate-morphotactic/grammar.xml"
    ));
    let grammar = pg_grammar::load(grammar_xml).expect("marker fixture must load");
    let selection = pg_foma::backend_selection::select_backends_for_grammar(&grammar);
    let composed = selection
        .report_for(EmissionStrategy::PlanComposed)
        .expect("PlanComposed must always have one report");

    assert_eq!(selection.reports().len(), BACKEND_PREFERENCE.len());
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
    assert!(
        !selection
            .selected()
            .contains(&EmissionStrategy::PlanComposed),
        "the selector must not advertise a net that runtime already knows is incomplete"
    );
}

#[test]
fn marker_free_plan_composed_remains_selectable() {
    let grammar_xml = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../machine/conformance/edge-cases/loader-isactive/grammar.xml"
    ));
    let grammar = pg_grammar::load(grammar_xml).expect("marker-free fixture must load");
    let selection = pg_foma::backend_selection::select_backends_for_grammar(&grammar);
    let composed = selection
        .report_for(EmissionStrategy::PlanComposed)
        .expect("PlanComposed must always have one report");

    assert_eq!(composed.status(), BackendStatus::Accepted);
    assert!(composed.is_selected());
    assert!(composed.findings().is_empty());
    assert!(composed.shapes().is_empty());
}
