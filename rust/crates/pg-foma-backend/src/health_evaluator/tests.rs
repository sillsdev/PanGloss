use super::*;
use crate::emit::{
    ClosureFallbackBackend, ClosureRefusal, ClosureRefusalCode, EmitCounts, UncoveredItem,
};
use crate::health::FindingClass;

/// `CompileMeasurements` for a compile-phase attempt -- the shape every test below already had under the four-parameter `evaluate_health` this module used to expose, now behind a named phase.
fn compile_measurements<'a>(
    payload_bytes: Option<u64>,
    emit_report: Option<&'a EmitReport>,
    compose_errors: &'a [ComposeError],
    apply_budget_trips: &'a [ApplyBudgetTrip],
) -> CompileMeasurements<'a> {
    CompileMeasurements {
        phases: AttemptedPhases::starting_with(Phase::Compile),
        payload_bytes,
        emit_report,
        compose_errors,
        apply_budget_trips,
    }
}

fn synthetic_full_emit_report() -> EmitReport {
    EmitReport {
        uncovered: Vec::new(),
        counts: EmitCounts::default(),
        tier: FomaTier::Full,
        enum_budget_exceeded: None,
        closure_refusal: None,
        closure_evidence: None,
    }
}

#[test]
fn fst_health_evaluator_every_foma_error_is_nonempty_backend_error() {
    let cases = vec![
        FomaError::LexcCompileFailed(Box::new(synthetic_full_emit_report())),
        FomaError::Unsupported(Box::new(EmitReport {
            tier: FomaTier::Unsupported {
                reason: "synthetic unsupported route".to_string(),
            },
            ..synthetic_full_emit_report()
        })),
    ];

    // Every error must BLOCK; the exact band is per-cause, pinned by the split tests below.
    for error in cases {
        let health = evaluate_foma_error(&error);
        assert!(!health.findings.is_empty(), "empty health for {error}");
        assert!(
            health.admission() >= Severity::NotProductionReady,
            "health for {error} must block publication, got {:?}",
            health.admission()
        );
    }
}

#[test]
fn fst_health_evaluator_lexc_failure_is_explicit_even_with_full_emit_report() {
    let health = evaluate_foma_error(&FomaError::LexcCompileFailed(Box::new(
        synthetic_full_emit_report(),
    )));
    assert!(health
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::BackendCompilationFailed));
}

#[test]
fn fst_health_evaluator_backend_local_budget_failures_are_errors() {
    let compose_errors = vec![ComposeError::ChainDepthExceeded {
        depth: 2,
        limit: 1,
        site: "apply",
    }];
    for error in compose_errors {
        let health = evaluate(compile_measurements(None, None, &[error], &[]));
        assert_eq!(health.admission(), Severity::NotProductionReady);
    }

    let trip = ApplyBudgetTrip {
        dimension: ApplyDimension::Candidates,
        value: 2,
        limit: 1,
        word: Some("word".to_string()),
    };
    assert_eq!(
        evaluate(compile_measurements(None, None, &[], &[trip])).admission(),
        Severity::NotProductionReady
    );
}

// fst_health_evaluator_size_bands: payload-size-only inputs, the single threshold.

#[test]
fn fst_health_evaluator_within_limits_payload_produces_no_finding() {
    let report = evaluate(compile_measurements(
        Some(crate::health::IDEAL_MAX_BYTES),
        None,
        &[],
        &[],
    ));
    assert!(report.findings.is_empty());
    assert_eq!(report.admission(), Severity::WithinLimits);
}

#[test]
fn fst_health_evaluator_over_ideal_payload_produces_not_production_ready_payload_size_band_finding()
{
    let bytes = 500_000_000u64;
    let report = evaluate(compile_measurements(Some(bytes), None, &[], &[]));
    assert_eq!(report.findings.len(), 1);
    let finding = &report.findings[0];
    assert_eq!(finding.code, FindingCode::PayloadSizeBand);
    assert_eq!(finding.severity, Severity::NotProductionReady);
    assert_eq!(finding.metric, Metric::PayloadBytes);
    assert_eq!(finding.value, MetricValue::Bytes(bytes));
    assert_eq!(
        finding.threshold,
        Some(MetricValue::Bytes(crate::health::IDEAL_MAX_BYTES))
    );
    assert_eq!(finding.provenance, ValueProvenance::Observed);
    assert_eq!(report.admission(), Severity::NotProductionReady);
}

#[test]
fn fst_health_evaluator_not_production_ready_payload_matches_health_schema_worked_scenario() {
    // `IDEAL_MAX_BYTES` is the only size threshold; one byte more crosses into NotProductionReady.
    let report = evaluate(compile_measurements(
        Some(crate::health::IDEAL_MAX_BYTES + 1),
        None,
        &[],
        &[],
    ));
    assert_eq!(report.findings[0].severity, Severity::NotProductionReady);
    assert_eq!(
        report.findings[0].threshold,
        Some(MetricValue::Bytes(crate::health::IDEAL_MAX_BYTES))
    );
}

#[test]
fn fst_health_evaluator_oversized_payload_remains_not_production_ready_readiness() {
    let report = evaluate(compile_measurements(
        Some(10_000_000_000u64),
        None,
        &[],
        &[],
    ));
    assert_eq!(report.findings[0].severity, Severity::NotProductionReady);
    assert_eq!(report.admission(), Severity::NotProductionReady);
}

// fst_health_evaluator_emit_report: FomaTier + enum-budget-exceeded mapping.

#[test]
fn fst_health_evaluator_full_tier_produces_no_finding() {
    let report = EmitReport {
        uncovered: Vec::new(),
        counts: EmitCounts::default(),
        tier: FomaTier::Full,
        enum_budget_exceeded: None,
        closure_refusal: None,
        closure_evidence: None,
    };
    let health = evaluate(compile_measurements(None, Some(&report), &[], &[]));
    assert!(health.findings.is_empty());
    assert_eq!(health.admission(), Severity::WithinLimits);
}

#[test]
fn fst_health_evaluator_partial_tier_is_cannot_represent_coverage_gap() {
    let uncovered = vec![
        UncoveredItem {
            kind: "infix".to_string(),
            id: "mrule12#allo0".to_string(),
            reason: "synthetic interdigitation not representable".to_string(),
        },
        UncoveredItem {
            kind: "process-morph".to_string(),
            id: "mrule13#allo0".to_string(),
            reason: "synthetic process morph not representable".to_string(),
        },
    ];
    let report = EmitReport {
        uncovered: uncovered.clone(),
        counts: EmitCounts::default(),
        tier: FomaTier::Partial { uncovered: 2 },
        enum_budget_exceeded: None,
        closure_refusal: None,
        closure_evidence: None,
    };
    let health = evaluate(compile_measurements(None, Some(&report), &[], &[]));
    assert_eq!(health.findings.len(), 1);
    let finding = &health.findings[0];
    assert_eq!(finding.code, FindingCode::BackendCoverageIncomplete);
    assert_eq!(finding.severity, Severity::CannotRepresent);
    assert_eq!(finding.metric, Metric::BackendCoverageGapCount);
    assert_eq!(finding.value, MetricValue::Count(2));
    assert_eq!(
        finding.affected,
        vec!["mrule12#allo0".to_string(), "mrule13#allo0".to_string()]
    );
    assert_eq!(health.admission(), Severity::CannotRepresent);
}

#[test]
fn fst_health_evaluator_unsupported_tier_with_no_closure_refusal_is_cannot_represent() {
    let report = EmitReport {
        uncovered: Vec::new(),
        counts: EmitCounts::default(),
        tier: FomaTier::Unsupported {
            reason: "zero root allomorphs survived synthetic pre-filtering".to_string(),
        },
        enum_budget_exceeded: None,
        closure_refusal: None,
        closure_evidence: None,
    };
    let health = evaluate(compile_measurements(None, Some(&report), &[], &[]));
    assert_eq!(health.findings.len(), 1);
    let finding = &health.findings[0];
    assert_eq!(finding.code, FindingCode::BackendCoverageIncomplete);
    assert_eq!(finding.severity, Severity::CannotRepresent);
    assert_eq!(finding.value, MetricValue::Unbounded);
    assert_eq!(health.admission(), Severity::CannotRepresent);
}

/// A `BackendCoverageIncomplete` finding is representability evidence, so it must always carry `CannotRepresent`, never `MachineLimit`.
#[test]
fn representability_never_reports_as_machine_limit() {
    let partial = EmitReport {
        uncovered: vec![UncoveredItem {
            kind: "infix".to_string(),
            id: "mrule1#allo0".to_string(),
            reason: "synthetic".to_string(),
        }],
        counts: EmitCounts::default(),
        tier: FomaTier::Partial { uncovered: 1 },
        enum_budget_exceeded: None,
        closure_refusal: None,
        closure_evidence: None,
    };
    let unsupported = EmitReport {
        uncovered: Vec::new(),
        counts: EmitCounts::default(),
        tier: FomaTier::Unsupported {
            reason: "synthetic total coverage loss".to_string(),
        },
        enum_budget_exceeded: None,
        closure_refusal: None,
        closure_evidence: None,
    };

    for report in [&partial, &unsupported] {
        let health = evaluate(compile_measurements(None, Some(report), &[], &[]));
        let coverage_findings: Vec<_> = health
            .findings
            .iter()
            .filter(|f| f.code == FindingCode::BackendCoverageIncomplete)
            .collect();
        assert!(
            !coverage_findings.is_empty(),
            "expected a BackendCoverageIncomplete finding for {report:?}"
        );
        for finding in coverage_findings {
            assert_eq!(
                finding.severity,
                Severity::CannotRepresent,
                "a BackendCoverageIncomplete finding must be CannotRepresent: {finding:?}"
            );
            assert_ne!(
                finding.severity,
                Severity::MachineLimit,
                "representability must never report as MachineLimit: {finding:?}"
            );
        }
    }
}

#[test]
fn fst_health_evaluator_unbounded_rule_application_is_cannot_represent() {
    let report = EmitReport {
        uncovered: Vec::new(),
        counts: EmitCounts::default(),
        tier: FomaTier::Unsupported {
            reason: "a participating rule has no authored finite bound".to_string(),
        },
        enum_budget_exceeded: None,
        closure_refusal: Some(ClosureRefusal {
            code: ClosureRefusalCode::UnboundedRuleApplication,
            affected_rule_ordinals: vec![7],
            depth_limit: None,
            pending_successors: None,
            remedy_backend: ClosureFallbackBackend::FullMorphologicalParser,
        }),
        closure_evidence: None,
    };
    let health = evaluate(compile_measurements(None, Some(&report), &[], &[]));
    let finding = &health.findings[0];
    assert_eq!(finding.code, FindingCode::BackendCoverageIncomplete);
    assert_eq!(finding.severity, Severity::CannotRepresent);
    assert_eq!(finding.class(), FindingClass::Representability);
}

/// An artificial cap is never a representability verdict, however deep it stopped.
#[test]
fn fst_health_evaluator_depth_budget_stop_is_containment_not_cannot_represent() {
    let report = EmitReport {
        uncovered: Vec::new(),
        counts: EmitCounts::default(),
        tier: FomaTier::Unsupported {
            reason: "closure depth budget exceeded".to_string(),
        },
        enum_budget_exceeded: None,
        closure_refusal: Some(ClosureRefusal {
            code: ClosureRefusalCode::DepthBudgetExceeded,
            affected_rule_ordinals: vec![3],
            depth_limit: Some(16),
            pending_successors: Some(5),
            remedy_backend: ClosureFallbackBackend::FullMorphologicalParser,
        }),
        closure_evidence: None,
    };
    let health = evaluate(compile_measurements(None, Some(&report), &[], &[]));
    let finding = &health.findings[0];
    assert_eq!(finding.code, FindingCode::ResourceBudgetReached);
    assert_eq!(finding.severity, Severity::NotProductionReady);
    assert_eq!(finding.class(), FindingClass::Containment);
    assert_ne!(
        finding.severity,
        Severity::MachineLimit,
        "a depth-budget stop halted one attempt; it must never condemn the grammar"
    );
}

#[test]
fn fst_health_evaluator_preserves_closure_refusal_cause() {
    let report = EmitReport {
        uncovered: Vec::new(),
        counts: EmitCounts::default(),
        tier: FomaTier::Unsupported {
            reason: "synthetic incomplete closure".to_string(),
        },
        enum_budget_exceeded: None,
        closure_refusal: Some(ClosureRefusal {
            code: ClosureRefusalCode::DepthBudgetExceeded,
            affected_rule_ordinals: vec![3, 7],
            depth_limit: Some(64),
            pending_successors: Some(11),
            remedy_backend: ClosureFallbackBackend::FullMorphologicalParser,
        }),
        closure_evidence: None,
    };
    let health = evaluate(compile_measurements(None, Some(&report), &[], &[]));
    let finding = &health.findings[0];
    assert_eq!(finding.affected, vec!["mrule3", "mrule7"]);
    assert_eq!(finding.value, MetricValue::Count(11));
    assert!(finding.explanation.contains("closure-depth limit was 64"));
}

/// A raisable compound-depth budget is a stop on THIS attempt, never a verdict that the grammar cannot be represented.
#[test]
fn an_enum_budget_stop_is_readiness_not_representability() {
    let report = EmitReport {
        uncovered: Vec::new(),
        counts: EmitCounts::default(),
        tier: FomaTier::Unsupported {
            reason: "compound chain depth budget".to_string(),
        },
        // The shape emit.rs produces for HC_COMPOUND_CHAIN_DEPTH_BUDGET: a budget, and no closure refusal.
        enum_budget_exceeded: Some(crate::emit::EnumBudgetExceeded {
            measure: "compound chain depth (extra non-head root levels)",
            value: 37,
            limit: 24,
        }),
        closure_refusal: None,
        closure_evidence: None,
    };
    let health = evaluate(compile_measurements(None, Some(&report), &[], &[]));
    let finding = &health.findings[0];
    assert_eq!(
        finding.code,
        FindingCode::ResourceBudgetReached,
        "a configured, raisable cap is a budget stop; reporting it as BackendCoverageIncomplete \
         tells the reader this language cannot be represented, which is not what happened"
    );
    assert_eq!(finding.severity, Severity::NotProductionReady);
    assert_eq!(
        finding.value,
        MetricValue::Count(37),
        "the measured value was on the report all along and read as Unbounded"
    );
    assert!(finding.explanation.contains("limit was 24"));
    assert!(finding.explanation.contains("reached 37"));
}

// fst_health_evaluator_compose_errors: every ComposeError variant maps to a finding.

#[test]
fn fst_health_evaluator_chain_depth_exceeded_is_apply_phase() {
    let err = ComposeError::ChainDepthExceeded {
        depth: 30,
        limit: 24,
        site: "synthetic-peel-site",
    };
    let health = evaluate(compile_measurements(
        None,
        None,
        std::slice::from_ref(&err),
        &[],
    ));
    let finding = &health.findings[0];
    assert_eq!(finding.code, FindingCode::ResourceBudgetReached);
    assert_eq!(finding.metric, Metric::ApplyChainDepth);
    assert_eq!(finding.phase, Phase::Apply);
}

// fst_health_evaluator_apply_budget_trips

#[test]
fn fst_health_evaluator_apply_budget_trip_decoded_paths() {
    let trip = ApplyBudgetTrip {
        dimension: ApplyDimension::DecodedPaths,
        value: 10_001,
        limit: 10_000,
        word: Some("synthetic-word".to_string()),
    };
    let health = evaluate(compile_measurements(
        None,
        None,
        &[],
        std::slice::from_ref(&trip),
    ));
    let finding = &health.findings[0];
    assert_eq!(finding.code, FindingCode::ResourceBudgetReached);
    assert_eq!(finding.metric, Metric::ProposalPathCount);
    assert_eq!(finding.phase, Phase::Apply);
    assert_eq!(finding.affected, vec!["synthetic-word".to_string()]);
}

#[test]
fn fst_health_evaluator_apply_budget_trip_candidates() {
    let trip = ApplyBudgetTrip {
        dimension: ApplyDimension::Candidates,
        value: 501,
        limit: 500,
        word: None,
    };
    let health = evaluate(compile_measurements(
        None,
        None,
        &[],
        std::slice::from_ref(&trip),
    ));
    let finding = &health.findings[0];
    assert_eq!(finding.metric, Metric::ProposalCandidateCount);
    assert!(finding.affected.is_empty());
}

// fst_health_evaluator_attempted_but_clean: a real attempt that measured nothing wrong.

/// The legitimate case this migration keeps: an attempt that reached `Phase::Compile` and found nothing to report is still `Ideal` -- naming the phase does not change what counts as clean.
#[test]
fn fst_health_evaluator_attempted_but_clean_is_within_limits() {
    let health = evaluate(compile_measurements(None, None, &[], &[]));
    assert!(health.findings.is_empty());
    assert_eq!(health.admission(), Severity::WithinLimits);
    assert_eq!(health.schema_version, crate::health::HEALTH_SCHEMA_VERSION);
}

/// The historical defect: `evaluate_health(None, None, &[], &[])` used to compile and return the same empty `WithinLimits` report as the clean attempt above, with no way to tell "nothing ran" apart from "everything ran and was clean". Reaches the private `AttemptedPhases` field directly (the one place in the crate still able to build the all-absent shape) to prove `evaluate` refuses it at runtime too, not just through `AttemptedPhases`'s missing empty constructor; delete the `assert!` in `evaluate` and this test fails instead of panicking.
#[test]
#[should_panic(expected = "at least one attempted phase")]
fn evaluate_panics_when_no_phase_was_ever_attempted() {
    let _ = evaluate(CompileMeasurements {
        phases: AttemptedPhases(Vec::new()),
        payload_bytes: None,
        emit_report: None,
        compose_errors: &[],
        apply_budget_trips: &[],
    });
}

// fst_health_evaluator_golden: a representative multi-source compile, byte-for-byte golden.

/// Two distinct measurement sources (payload size and an emit report) feeding one report, the shape a real caller assembles.
fn representative_inputs() -> (u64, EmitReport) {
    let payload_bytes = 250_000_000u64; // NotProductionReady: over IDEAL_MAX_BYTES
    let emit_report = EmitReport {
        uncovered: vec![UncoveredItem {
            kind: "process-morph".to_string(),
            id: "mrule0007#allo0".to_string(),
            reason: "synthetic non-concatenative process morph".to_string(),
        }],
        counts: EmitCounts::default(),
        tier: FomaTier::Partial { uncovered: 1 },
        enum_budget_exceeded: None,
        closure_refusal: None,
        closure_evidence: None,
    };
    (payload_bytes, emit_report)
}

const GOLDEN_JSON: &str = r#"{
  "schema_version": 8,
  "findings": [
    {
      "code": "PGF0001",
      "severity": "not_production_ready",
      "phase": "compile",
      "affected": [],
      "metric": "payload_bytes",
      "value": {
        "kind": "bytes",
        "value": 250000000
      },
      "provenance": "observed",
      "threshold": {
        "kind": "bytes",
        "value": 100000000
      },
      "explanation": "Final FST payload is 250000000 bytes, in the NotProductionReady band (R6 decimal-byte size thresholds).",
      "remedies": []
    },
    {
      "code": "PGF0013",
      "severity": "cannot_represent",
      "phase": "compile",
      "affected": [
        "mrule0007#allo0"
      ],
      "metric": "backend_coverage_gap_count",
      "value": {
        "kind": "count",
        "value": 1
      },
      "provenance": "observed",
      "explanation": "1 construct occurrence(s) could not be represented in this FST-propose network and contribute no candidates for it. Confirmation cannot restore omitted candidates, so normal generation fails closed.",
      "remedies": []
    }
  ]
}"#;

#[test]
fn fst_health_evaluator_golden_json() {
    let (payload_bytes, emit_report) = representative_inputs();
    let health = evaluate(compile_measurements(
        Some(payload_bytes),
        Some(&emit_report),
        &[],
        &[],
    ));
    let json = health.to_json().expect("serialization must succeed");
    assert_eq!(
        json, GOLDEN_JSON,
        "canonical JSON drifted from the committed golden"
    );
}

#[test]
fn fst_health_evaluator_golden_admission_is_cannot_represent() {
    let (payload_bytes, emit_report) = representative_inputs();
    let health = evaluate(compile_measurements(
        Some(payload_bytes),
        Some(&emit_report),
        &[],
        &[],
    ));
    // An uncovered construct is CannotRepresent even when resource findings have lower severity.
    assert_eq!(health.admission(), Severity::CannotRepresent);
}

#[test]
fn fst_health_evaluator_golden_round_trips() {
    let (payload_bytes, emit_report) = representative_inputs();
    let health = evaluate(compile_measurements(
        Some(payload_bytes),
        Some(&emit_report),
        &[],
        &[],
    ));
    let json = health.to_json().expect("serialization must succeed");
    let parsed = HealthReport::from_json(&json).expect("deserialization must succeed");
    assert_eq!(
        parsed, health,
        "round trip through canonical JSON must be lossless"
    );
}
