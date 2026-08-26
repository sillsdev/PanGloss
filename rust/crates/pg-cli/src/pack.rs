use std::fs;

use pg_foma::health::{
    FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase, Severity, ValueProvenance,
};
use pg_foma::health_evaluator::evaluate_health;

/// Applies readiness independently of capability trust; raw NotProductionReady/MachineLimit/CannotRepresent findings never admit.
pub(crate) fn validate_health_readiness(
    report: &HealthReport,
    worker_containment: bool,
) -> Result<(), String> {
    let admission = report.admission();
    let by_class = report.admission_by_class().render();
    if worker_containment {
        return Err(format!(
            "FST health is a worker containment failure; it cannot be overridden and no .pgpack was written ({by_class})"
        ));
    }
    if report
        .findings
        .iter()
        .any(|finding| finding.phase == Phase::Apply && finding.severity >= Severity::NotProductionReady)
    {
        return Err(format!(
            "FST health is an apply containment failure; it cannot be overridden and no .pgpack was written ({by_class})"
        ));
    }
    if report
        .findings
        .iter()
        .any(|finding| finding.severity >= Severity::NotProductionReady)
    {
        return Err(format!(
            "FST health is {admission:?}; no .pgpack was written. A correctness override cannot admit an oversized artifact, a contained attempt, or an unrepresentable feature. ({by_class})"
        ));
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn synthetic_health(severity: Severity) -> HealthReport {
        HealthReport::new(vec![HealthFinding {
            code: FindingCode::ResourceBudgetReached,
            severity,
            phase: Phase::Compile,
            affected: vec!["synthetic composite route".to_string()],
            metric: Metric::UnknownUnboundedWork,
            value: MetricValue::Count(101),
            provenance: ValueProvenance::Observed,
            threshold: Some(MetricValue::Count(100)),
            explanation: "synthetic health gate".to_string(),
            remedies: Vec::new(),
        }])
    }

    #[test]
    fn health_large_multiplier_publishes_without_override() {
        let report = synthetic_health(Severity::LargeMultiplier);
        assert!(validate_health_readiness(&report, false).is_ok());
        assert_eq!(report.admission(), Severity::LargeMultiplier);
    }

    #[test]
    fn health_not_production_ready_refuses_publication_without_override() {
        let report = synthetic_health(Severity::NotProductionReady);
        let error = validate_health_readiness(&report, false).unwrap_err();
        assert!(error.contains("no .pgpack was written"));
        assert_eq!(report.admission(), Severity::NotProductionReady);
    }

    /// The refusal message must name the failing axis, not just the collapsed severity band.
    #[test]
    fn readiness_refusal_message_names_the_failing_axis() {
        let report = synthetic_health(Severity::NotProductionReady);
        let error = validate_health_readiness(&report, false).unwrap_err();
        assert!(
            error.contains("containment=NotProductionReady"),
            "expected the per-axis breakdown in the refusal message: {error}"
        );
        assert!(error.contains("representability=WithinLimits"));
        assert!(error.contains("readiness=WithinLimits"));
        assert!(error.contains("process=WithinLimits"));
    }

    /// Regression guard: the richer refusal message must not move which reports get refused.
    #[test]
    fn validate_health_readiness_decision_matrix_is_unchanged() {
        let severities = [
            Severity::WithinLimits,
            Severity::Elevated,
            Severity::LargeMultiplier,
            Severity::NotProductionReady,
            Severity::MachineLimit,
            Severity::CannotRepresent,
        ];
        let phases = [Phase::Characterization, Phase::Compile, Phase::Apply];
        for &severity in &severities {
            for &phase in &phases {
                for &worker_containment in &[false, true] {
                    let mut report = synthetic_health(severity);
                    report.findings[0].phase = phase;

                    let expected_ok = !worker_containment
                        && !(phase == Phase::Apply && severity >= Severity::NotProductionReady)
                        && severity < Severity::NotProductionReady;

                    let actual_ok = validate_health_readiness(&report, worker_containment).is_ok();
                    assert_eq!(
                        actual_ok, expected_ok,
                        "severity={severity:?} phase={phase:?} worker_containment={worker_containment} \
                         must decide ok={expected_ok}"
                    );
                }
            }
        }
    }

    #[test]
    fn health_machine_limit_refuses_publication_without_override() {
        let report = synthetic_health(Severity::MachineLimit);
        let error = validate_health_readiness(&report, false).unwrap_err();
        assert!(error.contains("no .pgpack was written"));
        assert_eq!(report.admission(), Severity::MachineLimit);
    }

    #[cfg(feature = "developer-tools")]
    #[test]
    fn correctness_override_does_not_override_health_not_production_ready() {
        let report = synthetic_health(Severity::NotProductionReady);
        assert!(validate_health_readiness(&report, false).is_err());
        assert_eq!(report.admission(), Severity::NotProductionReady);
    }

    #[cfg(feature = "developer-tools")]
    #[test]
    fn capability_override_does_not_admit_health_cannot_represent() {
        let mut report = synthetic_health(Severity::CannotRepresent);
        report.findings[0].code = FindingCode::BackendCoverageIncomplete;
        assert!(validate_health_readiness(&report, false).is_err());
        assert_eq!(
            report.admission(),
            Severity::CannotRepresent
        );
        assert_eq!(report.admission(), Severity::CannotRepresent);
    }

    #[cfg(feature = "developer-tools")]
    #[test]
    fn health_apply_containment_cannot_be_overridden() {
        let mut report = synthetic_health(Severity::MachineLimit);
        report.findings[0].phase = Phase::Apply;
        let error = validate_health_readiness(&report, false).unwrap_err();
        assert!(error.contains("apply containment"));
    }

    /// A fresh, collision-free scratch directory per test.
    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "pangloss-cli-pack-test-{tag}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// An oversized payload byte count must produce a `PayloadSizeBand` finding at `NotProductionReady`.
    #[test]
    fn an_oversized_payload_is_labelled_not_production_ready() {
        let oversized = pg_foma::health::IDEAL_MAX_BYTES + 1;
        let health = evaluate_health(Some(oversized), None, &[], &[], None);
        let finding = health
            .findings
            .iter()
            .find(|finding| finding.code == FindingCode::PayloadSizeBand)
            .expect("an oversized payload must produce a PayloadSizeBand finding");
        assert_eq!(finding.severity, Severity::NotProductionReady);
    }

    /// Injects the byte count at the `evaluate_health` seam rather than compiling a genuine >100MB network.
    #[test]
    fn an_oversized_pack_is_refused_publication() {
        let oversized = pg_foma::health::IDEAL_MAX_BYTES + 1;
        let health = evaluate_health(Some(oversized), None, &[], &[], None);
        assert!(
            validate_health_readiness(&health, false).is_err(),
            "an oversized payload must be refused publication"
        );

        let dir = scratch_dir("oversized-refusal");
        let out_path = dir.join("out.pgpack");
        let attempt: Result<(), String> = (|| {
            validate_health_readiness(&health, false)?;
            fs::write(&out_path, b"unused").map_err(|e| e.to_string())?;
            Ok(())
        })();
        assert!(attempt.is_err());
        assert!(
            !out_path.exists(),
            "no .pgpack may be written for a refused oversized payload"
        );
    }

}
