//! Publication admission from grammar-health labels after a completed FST build.

use crate::enumerate::EmissionStrategy;
use crate::health::{
    FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase, Severity, ValueProvenance,
};
use pg_grammar::grammar_health::check_grammar_health_diagnostics;
use pg_grammar::model::Grammar;
use pg_grammar::GrammarError;

/// One strategy's production verdict for one completed FST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FstProductionAdmission {
    strategy: EmissionStrategy,
    health: HealthReport,
}

impl FstProductionAdmission {
    /// Which compiler's completed result this verdict is about.
    pub fn strategy(&self) -> EmissionStrategy {
        self.strategy
    }

    pub fn health(&self) -> &HealthReport {
        &self.health
    }

    /// The publication gate: true from [`Severity::NotProductionReady`] upward. The severity names
    /// the blocking tier and the finding's `FindingClass` names which question it answers, so
    /// callers gate on this rather than comparing severities themselves.
    pub fn blocks_publication(&self) -> bool {
        health_blocks_publication(&self.health)
    }
}

/// The publication gate over ANY health report, so a caller holding only a merged
/// [`HealthReport`] shares this comparison instead of re-deriving it against
/// [`Severity::NotProductionReady`] by hand.
///
/// [`crate::backend_runtime::RuntimeEvaluation::production_blocks_publication`] is the other caller;
/// keeping both on this function is what stops the artifact boundary and the selection boundary
/// from drifting to different thresholds.
pub fn health_blocks_publication(health: &HealthReport) -> bool {
    health.admission() >= Severity::NotProductionReady
}

/// Reads grammar-health's labels; completion evidence already establishes FST representability.
/// This decision is applied after construction so a labelled grammar can still be measured.
pub fn assess_completed_fst(
    grammar: &Grammar,
    strategy: EmissionStrategy,
) -> Result<FstProductionAdmission, GrammarError> {
    let diagnostics = check_grammar_health_diagnostics(grammar)?;
    let blocking: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.blocks_fst)
        .collect();
    let findings = if blocking.is_empty() {
        Vec::new()
    } else {
        let mut codes: Vec<String> = blocking
            .iter()
            .map(|diagnostic| diagnostic.code.wire().to_string())
            .collect();
        codes.sort();
        codes.dedup();
        vec![HealthFinding::new(
            FindingCode::GrammarHealthBlocksFst,
            Severity::NotProductionReady,
            Phase::Compile,
            Metric::GrammarHealthBlockingCount,
            MetricValue::Count(blocking.len() as u64),
            ValueProvenance::Observed,
            format!("FST publication blocked by {} labelled grammar-health finding(s); see grammar-health for the author findings.", blocking.len()),
        ).affecting(codes)]
    };
    Ok(FstProductionAdmission {
        strategy,
        health: HealthReport::new(findings),
    })
}

#[cfg(test)]
mod tests;
