use pg_foma::advice_catalog::RemedyEffort;
use pg_foma::backend_selection::{
    sort_blocking_remedy_sets, AdviceReference, BackendReport, BackendSelection, BackendStatus,
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

fn default_budget_exceeding_grammar() -> pg_grammar::model::Grammar {
    let base = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../machine/conformance/edge-cases/truncate-morphotactic/grammar.xml"
    ));
    let mut entries = String::new();
    for index in 0..1_100 {
        entries.push_str(&format!(
            r#"          <LexicalEntry id="syntheticRoot{index}" partOfSpeech="posV">
            <Allomorphs>
              <Allomorph id="syntheticRoot{index}_1">
                <PhoneticShape>sag</PhoneticShape>
              </Allomorph>
            </Allomorphs>
            <Gloss>synthetic-{index}</Gloss>
          </LexicalEntry>
"#
        ));
    }
    let marker = "        </LexicalEntries>";
    let expanded = base.replacen(marker, &format!("{entries}{marker}"), 1);
    pg_grammar::load(&expanded).expect("default-budget fixture must load")
}

/// An oversized payload labels an artifact that exists; only an absent artifact excludes.
#[test]
fn readiness_labels_stay_selectable_while_containment_and_representability_do_not() {
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
                FindingCode::HostContainmentFired,
            )],
        )
        .unwrap(),
    ]);

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
        assert_eq!(report.shapes(), &["backend-build-unavailable".to_string()]);
        assert!(!report.advice_references().is_empty());
    }
}

#[test]
fn tuned_surface_closure_budget_finding_is_reported_and_not_production_ready_is_not_selected() {
    let grammar = default_budget_exceeding_grammar();
    let selection = pg_foma::backend_selection::select_backends_for_grammar(&grammar);
    let tuned = selection
        .report_for(EmissionStrategy::TunedSurfaceProbed)
        .expect("TunedSurface must always have one report");

    assert_eq!(tuned.worst_severity(), Severity::NotProductionReady);
    assert_eq!(tuned.findings().len(), 1);
    assert_eq!(
        tuned.findings()[0].code,
        FindingCode::ProvenBoundExceedsBudget
    );
    assert_eq!(tuned.findings()[0].metric, Metric::CompositeRulePairCount);
    assert_eq!(
        tuned.shapes(),
        &["tuned-surface-closure-budget".to_string()]
    );
    assert!(tuned
        .cost_evidence()
        .iter()
        .any(|evidence| evidence.metric == Metric::CompositeRulePairCount));
    assert!(!tuned.advice_references().is_empty());
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
    assert!(composed.findings().is_empty());
    assert!(composed.shapes().is_empty());
}
