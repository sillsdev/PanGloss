use super::*;

// The single threshold, pinned once.

#[test]
fn fst_health_size_threshold_value_is_the_declared_target() {
    // Changing this changes a stated target: say so in `IDEAL_MAX_BYTES`'s doc.
    assert_eq!(IDEAL_MAX_BYTES, 100_000_000);
}

#[test]
fn fst_health_size_bands_zero_is_within_limits() {
    assert_eq!(severity_for_size_bytes(0), Severity::WithinLimits);
}

#[test]
fn fst_health_size_bands_within_limits_upper_edge_inclusive() {
    assert_eq!(
        severity_for_size_bytes(IDEAL_MAX_BYTES),
        Severity::WithinLimits
    );
}

#[test]
fn fst_health_size_bands_not_production_ready_lower_edge_exclusive_of_within_limits() {
    assert_eq!(
        severity_for_size_bytes(IDEAL_MAX_BYTES + 1),
        Severity::NotProductionReady
    );
}

#[test]
fn fst_health_size_bands_far_above_floor_remains_not_production_ready() {
    assert_eq!(
        severity_for_size_bytes(u64::MAX),
        Severity::NotProductionReady
    );
}

/// The pin for the whole category-leak fix: a compiled-artifact size measurement must never surface as a pre-compile static-analysis or containment verdict, at any size.
#[test]
fn size_never_reports_an_analysis_verdict() {
    let sizes = [
        0,
        IDEAL_MAX_BYTES,
        IDEAL_MAX_BYTES + 1,
        150_000_000,
        200_000_000,
        250_000_000,
        1_000_000_000,
        3_000_000_000,
        5_000_000_000,
        6_000_000_000,
        u64::MAX,
    ];
    for bytes in sizes {
        let severity = severity_for_size_bytes(bytes);
        assert_ne!(
            severity,
            Severity::Elevated,
            "{bytes} bytes must never report the pre-compile Elevated verdict"
        );
        assert_ne!(
            severity,
            Severity::LargeMultiplier,
            "{bytes} bytes must never report the pre-compile LargeMultiplier verdict"
        );
        assert_ne!(
            severity,
            Severity::MachineLimit,
            "{bytes} bytes must never report the containment MachineLimit verdict"
        );
        assert_ne!(
            severity,
            Severity::CannotRepresent,
            "{bytes} bytes must never report the pre-compile CannotRepresent verdict"
        );
    }
}

fn synthetic_finding(severity: Severity) -> HealthFinding {
    HealthFinding::new(
        FindingCode::PayloadSizeBand,
        severity,
        Phase::Compile,
        Metric::PayloadBytes,
        MetricValue::Bytes(1),
        ValueProvenance::Observed,
        "synthetic test finding".to_string(),
    )
    .affecting(vec!["synthetic-construct".to_string()])
}

#[test]
fn an_empty_report_admits_within_limits() {
    let report = HealthReport::new(Vec::new());
    assert_eq!(report.admission(), Severity::WithinLimits);
}

#[test]
fn the_worst_severity_wins_among_several_findings() {
    let report = HealthReport::new(vec![
        synthetic_finding(Severity::Elevated),
        synthetic_finding(Severity::NotProductionReady),
        synthetic_finding(Severity::LargeMultiplier),
    ]);
    assert_eq!(report.admission(), Severity::NotProductionReady);
}

// fst_health_schema: code registry, golden JSON, round trip, closed-enum exhaustiveness.

#[test]
fn fst_health_schema_codes_are_unique_and_well_formed() {
    let mut seen = std::collections::HashSet::new();
    for code in FindingCode::ALL {
        let wire = code.code();
        assert!(wire.starts_with("PGF"), "{wire} must start with PGF");
        let digits = &wire[3..];
        assert_eq!(digits.len(), 4, "{wire} must have exactly 4 digits");
        assert!(
            digits.chars().all(|c| c.is_ascii_digit()),
            "{wire} digits must be numeric"
        );
        assert!(seen.insert(wire), "duplicate finding code {wire}");
        assert!(
            !code.meaning().is_empty(),
            "{wire} must document its meaning"
        );
    }
}

#[test]
fn fst_health_schema_from_code_round_trips_every_registered_code() {
    for code in FindingCode::ALL {
        assert_eq!(FindingCode::from_code(code.code()), Some(*code));
    }
}

#[test]
fn fst_health_schema_from_code_rejects_unknown_code() {
    assert_eq!(FindingCode::from_code("PGF9999"), None);
}

#[test]
fn characterization_phase_has_product_vocabulary_on_the_wire() {
    assert_eq!(
        serde_json::to_string(&Phase::Characterization).unwrap(),
        "\"characterization\""
    );
}

#[test]
fn dead_health_labels_bump_health_schema_version() {
    assert_eq!(HEALTH_SCHEMA_VERSION, 8);
}

/// An exhaustive `match` with no catch-all arm over every `Severity` variant, so adding a variant stops this from compiling until every exhaustive match in this file is updated.
#[test]
fn fst_health_schema_severity_is_closed_and_exhaustive() {
    const fn label(severity: Severity) -> &'static str {
        match severity {
            Severity::WithinLimits => "within_limits",
            Severity::Elevated => "elevated",
            Severity::LargeMultiplier => "large_multiplier",
            Severity::NotProductionReady => "not_production_ready",
            Severity::MachineLimit => "machine_limit",
            Severity::CannotRepresent => "cannot_represent",
        }
    }
    assert_eq!(label(Severity::WithinLimits), "within_limits");
    assert_eq!(label(Severity::Elevated), "elevated");
    assert_eq!(label(Severity::LargeMultiplier), "large_multiplier");
    assert_eq!(label(Severity::NotProductionReady), "not_production_ready");
    assert_eq!(label(Severity::MachineLimit), "machine_limit");
    assert_eq!(label(Severity::CannotRepresent), "cannot_represent");
}

/// One NotProductionReady payload-size label.
fn representative_report() -> HealthReport {
    HealthReport::new(vec![HealthFinding::new(
        FindingCode::PayloadSizeBand,
        Severity::NotProductionReady,
        Phase::Compile,
        Metric::PayloadBytes,
        MetricValue::Bytes(1_500_000_000),
        ValueProvenance::Observed,
        "Final FST payload is 1,500,000,000 bytes, over the 100,000,000-byte \
                    NotProductionReady threshold."
            .to_string(),
    )
    .affecting(vec!["synthetic-stress-grammar".to_string()])
    .against_threshold(MetricValue::Bytes(IDEAL_MAX_BYTES))
    .with_remedies(vec![Remedy {
        rank: 1,
        description:
            "Review the measured compile cost and simplify the grammar before publication."
                .to_string(),
        requires_linguistic_equivalence: false,
        caveat: None,
    }])])
}

const GOLDEN_JSON: &str = r#"{
  "schema_version": 8,
  "findings": [
    {
      "code": "PGF0001",
      "severity": "not_production_ready",
      "phase": "compile",
      "affected": [
        "synthetic-stress-grammar"
      ],
      "metric": "payload_bytes",
      "value": {
        "kind": "bytes",
        "value": 1500000000
      },
      "provenance": "observed",
      "threshold": {
        "kind": "bytes",
        "value": 100000000
      },
      "explanation": "Final FST payload is 1,500,000,000 bytes, over the 100,000,000-byte NotProductionReady threshold.",
      "remedies": [
        {
          "rank": 1,
          "description": "Review the measured compile cost and simplify the grammar before publication.",
          "requires_linguistic_equivalence": false
        }
      ]
    }
  ]
}"#;

#[test]
fn fst_health_schema_golden_json() {
    let report = representative_report();
    let json = report.to_json().expect("serialization must succeed");
    assert_eq!(
        json, GOLDEN_JSON,
        "canonical JSON drifted from the committed golden"
    );
}

#[test]
fn fst_health_schema_round_trip() {
    let report = representative_report();
    let json = report.to_json().expect("serialization must succeed");
    let parsed = HealthReport::from_json(&json).expect("deserialization must succeed");
    assert_eq!(
        parsed, report,
        "round trip through canonical JSON must be lossless"
    );
    assert_eq!(parsed.admission(), Severity::NotProductionReady);
}

#[test]
fn fst_health_schema_rejects_stale_v6_reports() {
    let stale = GOLDEN_JSON.replacen("\"schema_version\": 8", "\"schema_version\": 6", 1);
    let error = HealthReport::from_json(&stale).expect_err("schema v6 must be rejected");
    assert!(error.to_string().contains("schema version 6"));
    assert!(error.to_string().contains("expected 8"));
}

// fst_health_finding_class: FindingCode -> FindingClass, the four-question vocabulary.

#[test]
fn every_finding_code_has_a_class() {
    let classified: Vec<FindingClass> = FindingCode::ALL.iter().map(|code| code.class()).collect();
    assert_eq!(classified.len(), FindingCode::ALL.len());
}

#[test]
fn representability_is_the_only_class_that_denies_the_grammar() {
    assert_eq!(
        FindingCode::BackendCoverageIncomplete.class(),
        FindingClass::Representability
    );
    for code in FindingCode::ALL {
        if *code == FindingCode::BackendCoverageIncomplete {
            continue;
        }
        assert_ne!(
            code.class(),
            FindingClass::Representability,
            "{code:?} must not claim to deny the grammar's representability"
        );
    }
}

#[test]
fn containment_codes_are_about_the_attempt_not_the_language() {
    let containment: Vec<FindingCode> = FindingCode::ALL
        .iter()
        .copied()
        .filter(|code| code.class() == FindingClass::Containment)
        .collect();
    assert_eq!(containment, vec![FindingCode::ResourceBudgetReached]);
}

#[test]
fn unknown_unbounded_construct_is_not_representability() {
    // Its own doc calls the construct recall-preserving: cost uncertainty, not a denial.
    assert_eq!(
        FindingCode::UnknownUnboundedConstruct.class(),
        FindingClass::Readiness
    );
}

// fst_health_admission_by_class: the per-class view, additive alongside `admission`.

fn class_finding(code: FindingCode, severity: Severity) -> HealthFinding {
    HealthFinding::new(
        code,
        severity,
        Phase::Compile,
        Metric::PayloadBytes,
        MetricValue::Bytes(1),
        ValueProvenance::Observed,
        "synthetic per-class test finding".to_string(),
    )
    .affecting(vec!["synthetic-construct".to_string()])
}

#[test]
fn admission_by_class_separates_a_resource_stop_from_a_representability_gap() {
    // Demonstrates the blur is gone: one severity used to hide which question was failing.
    let report = HealthReport::new(vec![
        class_finding(
            FindingCode::ResourceBudgetReached,
            Severity::NotProductionReady,
        ), // Containment
        class_finding(
            FindingCode::BackendCoverageIncomplete,
            Severity::CannotRepresent,
        ), // Representability
    ]);

    // The existing publish-gating value is untouched: still the plain max over everything.
    assert_eq!(report.admission(), Severity::CannotRepresent);

    let by_class = report.admission_by_class();
    assert_eq!(
        by_class.containment,
        Severity::NotProductionReady,
        "the resource stop must be visible on its own axis"
    );
    assert_eq!(
        by_class.representability,
        Severity::CannotRepresent,
        "the representability gap must be visible on its own axis"
    );
    assert_eq!(by_class.readiness, Severity::WithinLimits);
    assert_eq!(by_class.process, Severity::WithinLimits);
}

#[test]
fn admission_by_class_render_names_all_four_axes() {
    let report = HealthReport::new(vec![
        class_finding(
            FindingCode::ResourceBudgetReached,
            Severity::NotProductionReady,
        ), // Containment
        class_finding(
            FindingCode::BackendCoverageIncomplete,
            Severity::CannotRepresent,
        ), // Representability
    ]);
    assert_eq!(
        report.admission_by_class().render(),
        "representability=CannotRepresent, readiness=WithinLimits, \
             containment=NotProductionReady, process=WithinLimits"
    );
}

#[test]
fn worst_by_class_is_within_limits_for_an_absent_class() {
    let report = HealthReport::new(vec![
        class_finding(FindingCode::PayloadSizeBand, Severity::LargeMultiplier), // Readiness
    ]);
    assert_eq!(
        report.worst_by_class(FindingClass::Representability),
        Severity::WithinLimits
    );
    assert_eq!(
        report.worst_by_class(FindingClass::Containment),
        Severity::WithinLimits
    );
}

#[test]
fn admission_is_unchanged_by_the_per_class_view() {
    // Each code is chosen so its class agrees with the severity beside it; the assertion reads severities only.
    let reports = vec![
        HealthReport::new(Vec::new()),
        HealthReport::new(vec![class_finding(
            FindingCode::PayloadSizeBand,
            Severity::Elevated,
        )]),
        HealthReport::new(vec![
            class_finding(
                FindingCode::UnknownUnboundedConstruct,
                Severity::NotProductionReady,
            ),
            class_finding(
                FindingCode::RuleInteractionProduct,
                Severity::LargeMultiplier,
            ),
        ]),
        HealthReport::new(vec![class_finding(
            FindingCode::ResourceBudgetReached,
            Severity::MachineLimit,
        )]),
    ];

    for report in reports {
        let plain_max = report
            .findings
            .iter()
            .map(|finding| finding.severity)
            .max()
            .unwrap_or(Severity::WithinLimits);
        assert_eq!(
            report.admission(),
            plain_max,
            "admission() must still equal the plain max over all severities"
        );
    }
}
