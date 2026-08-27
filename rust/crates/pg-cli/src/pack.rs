#[cfg(test)]
mod tests {
    use pg_foma::health::{FindingCode, Severity};
    use pg_foma::health_evaluator::evaluate_health;

    /// An oversized payload byte count must produce a `PayloadSizeBand` finding at `NotProductionReady`.
    #[test]
    fn an_oversized_payload_is_labelled_not_production_ready() {
        let oversized = pg_foma::health::IDEAL_MAX_BYTES + 1;
        let health = evaluate_health(Some(oversized), None, &[], &[]);
        let finding = health
            .findings
            .iter()
            .find(|finding| finding.code == FindingCode::PayloadSizeBand)
            .expect("an oversized payload must produce a PayloadSizeBand finding");
        assert_eq!(finding.severity, Severity::NotProductionReady);
    }
}
