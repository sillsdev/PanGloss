use pg_foma::health::{HealthReport, Phase, Severity};
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

}
