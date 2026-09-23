use super::*;

#[test]
fn policy_v1_is_stamped_with_the_current_schema_version() {
    assert_eq!(policy_v1().schema_version, THRESHOLD_POLICY_SCHEMA_VERSION);
}

#[test]
fn policy_v1_names_a_device_class() {
    let policy = policy_v1();
    assert!(!policy.device_class.trim().is_empty());
}

#[test]
fn policy_v1_is_deterministic() {
    assert_eq!(policy_v1(), policy_v1());
}

#[test]
fn policy_v1_round_trips_through_canonical_json() {
    let policy = policy_v1();
    let json = policy.to_canonical_json();
    let parsed = ThresholdPolicy::from_json(&json).expect("valid policy JSON must parse");
    assert_eq!(parsed, policy);
}

#[test]
fn to_canonical_json_is_deterministic() {
    let policy = policy_v1();
    assert_eq!(policy.to_canonical_json(), policy.to_canonical_json());
}

/// The type system can't stop a caller writing `rationale: ""`, so this pins that today's seed values actually carry a non-empty citation/rationale.
#[test]
fn every_seeded_threshold_names_its_calibration_honestly() {
    let policy = policy_v1();
    let calibrations: Vec<(&str, &Calibration)> = vec![
        (
            "pack_size_max_bytes",
            &policy.pack_size_max_bytes.calibration,
        ),
        (
            "lexicon_min_entries",
            &policy.lexicon_min_entries.calibration,
        ),
        (
            "coverage_min_analysis_rate",
            &policy.coverage_min_analysis_rate.calibration,
        ),
        ("latency_p50_max_ms", &policy.latency_p50_max_ms.calibration),
        ("latency_p90_max_ms", &policy.latency_p90_max_ms.calibration),
        ("latency_p99_max_ms", &policy.latency_p99_max_ms.calibration),
    ];
    for (name, calibration) in calibrations {
        match calibration {
            Calibration::Measured { citation } => assert!(
                !citation.trim().is_empty(),
                "{name}'s Measured calibration must cite real evidence, not an empty string"
            ),
            Calibration::Placeholder { rationale } => assert!(
                !rationale.trim().is_empty(),
                "{name}'s Placeholder calibration must name a rationale, not an empty string"
            ),
        }
    }
}

/// Pack size, lexicon scale, and coverage rate have no measured evidence yet, so they must be `Placeholder`, not accidentally `Measured`.
#[test]
fn unmeasured_dimensions_are_placeholders_not_measured() {
    let policy = policy_v1();
    assert!(policy.pack_size_max_bytes.calibration.is_placeholder());
    assert!(policy.lexicon_min_entries.calibration.is_placeholder());
    assert!(policy
        .coverage_min_analysis_rate
        .calibration
        .is_placeholder());
}

/// The latency thresholds have real cited evidence, so they must be `Measured`, not a default placeholder.
#[test]
fn latency_dimensions_are_measured_not_placeholders() {
    let policy = policy_v1();
    assert!(!policy.latency_p50_max_ms.calibration.is_placeholder());
    assert!(!policy.latency_p90_max_ms.calibration.is_placeholder());
    assert!(!policy.latency_p99_max_ms.calibration.is_placeholder());
}
