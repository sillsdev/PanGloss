#[cfg(test)]
mod tests {
    use pg_foma::health::{FindingCode, Phase, Severity};
    use pg_foma::health_evaluator::{evaluate, AttemptedPhases, CompileMeasurements};

    /// An oversized payload byte count must produce a `PayloadSizeBand` finding at `NotProductionReady`.
    #[test]
    fn an_oversized_payload_is_labelled_not_production_ready() {
        let oversized = pg_foma::health::IDEAL_MAX_BYTES + 1;
        let health = evaluate(CompileMeasurements {
            phases: AttemptedPhases::starting_with(Phase::Compile),
            payload_bytes: Some(oversized),
            emit_report: None,
            compose_errors: &[],
            apply_budget_trips: &[],
        });
        let finding = health
            .findings
            .iter()
            .find(|finding| finding.code == FindingCode::PayloadSizeBand)
            .expect("an oversized payload must produce a PayloadSizeBand finding");
        assert_eq!(finding.severity, Severity::NotProductionReady);
    }
}
