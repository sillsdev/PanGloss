//! One backend's place in a compile's selection, and the whole selection over every backend —
//! plain data plus the constructors that need nothing but a `HealthFinding`/`CompileDecision`
//! already in hand. `pg_foma::backend_selection` owns the rest: turning a capability envelope
//! into a `BackendSelection` (`select_backends`), and attaching the advice catalog's shapes/
//! remedies to a refusal (its own free `refused` function) — both need the compiler's capability
//! registry and embedded advice catalog, which this crate does not and must not depend on. That
//! module re-exports every type here at its historical path.
//!
//! # Why `refused` is a free function here, not `BackendReport::refused`
//! An inherent impl on a type must live in the type's own defining crate, and populating a
//! refusal's `shapes`/`advice_references` needs `pg_foma`'s embedded advice catalog and
//! grammar-wide-check registry. So this module builds the refusal's `HealthFinding` alone
//! (`BackendReport::refused`, below — that much needs only data already in hand) and exposes
//! `BackendReport::with_capability_advice` as the seam `pg_foma::backend_selection::refused`
//! uses to attach the catalog-derived shapes/remedies afterward.

use crate::capability::{CapabilityDiagnostic, CompileDecision};
use crate::health::{
    FindingCode, HealthFinding, Metric, MetricValue, Phase, Severity, ValueProvenance,
};
use crate::strategy::EmissionStrategy;

/// Why a backend report exists even when no artifact can be produced from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendStatus {
    /// Capability admission succeeded and the backend is eligible for normal generation, subject
    /// to its health severity.
    Accepted,
    /// Capability admission refused this grammar. The refusal and its diagnostics remain in the
    /// report for explanation and advice.
    Refused,
    /// The backend was requested but was not available in this run.
    Missing,
    /// The backend was admitted, but its construction attempt failed.
    Failed,
}

/// A typed link from a backend finding to one cataloged remedy for one observed shape.
///
/// Effort belongs to the pair, not the remedy key: a shared remedy can be easy for one shape and
/// hard for another. Equality and ordering include all three fields so stable de-duplication never
/// erases that distinction.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AdviceReference {
    pub shape_key: String,
    pub remedy_key: String,
    pub effort: crate::advice::RemedyEffort,
}

impl AdviceReference {
    pub fn new(
        shape_key: impl Into<String>,
        remedy_key: impl Into<String>,
        effort: crate::advice::RemedyEffort,
    ) -> Self {
        Self {
            shape_key: shape_key.into(),
            remedy_key: remedy_key.into(),
            effort,
        }
    }
}

fn dedup_advice_references(mut references: Vec<AdviceReference>) -> Vec<AdviceReference> {
    references.sort();
    references.dedup();
    references
}

/// One backend's place in the selection: its own compatibility report, plus whether that report
/// admits it as a path for this grammar.
#[derive(Debug, Clone, PartialEq)]
pub struct BackendReport {
    strategy: EmissionStrategy,
    decision: CompileDecision,
    status: BackendStatus,
    findings: Vec<HealthFinding>,
    failed_predicates: Vec<String>,
    shapes: Vec<String>,
    advice_references: Vec<AdviceReference>,
    status_detail: Option<String>,
}

impl BackendReport {
    /// Which backend this report is about.
    pub fn strategy(&self) -> EmissionStrategy {
        self.strategy
    }

    /// The backend's own `CompileDecision` — kept whole, so a caller can tell an `Admit` path
    /// from a `ConfirmOnly` one rather than only "selected or not".
    pub fn decision(&self) -> &CompileDecision {
        &self.decision
    }

    /// Lifecycle status for this backend. Unlike `decision`, this distinguishes refusal, absence,
    /// and a failed construction attempt, all of which must remain visible in a full report.
    pub fn status(&self) -> BackendStatus {
        self.status
    }

    pub fn findings(&self) -> &[HealthFinding] {
        &self.findings
    }

    pub fn failed_predicates(&self) -> &[String] {
        &self.failed_predicates
    }

    pub fn shapes(&self) -> &[String] {
        &self.shapes
    }

    pub fn advice_references(&self) -> &[AdviceReference] {
        &self.advice_references
    }

    /// Whether this backend can represent the grammar: true for `Admit` and `ConfirmOnly`,
    /// false only for a refusal.
    ///
    /// A Compatibility report fact about one backend, never a Selector decision (ADR-0001).
    /// It answers the representability axis and says nothing about readiness or containment,
    /// which is why it is not named for whether the backend was picked.
    pub fn can_represent(&self) -> bool {
        !matches!(self.decision, CompileDecision::Refuse(_))
    }

    pub fn status_detail(&self) -> Option<&str> {
        self.status_detail.as_deref()
    }

    /// The worst health finding for this backend.
    pub fn worst_severity(&self) -> Severity {
        self.findings
            .iter()
            .map(|finding| finding.severity)
            .max()
            .unwrap_or(Severity::WithinLimits)
    }

    fn base(strategy: EmissionStrategy, decision: CompileDecision, status: BackendStatus) -> Self {
        Self {
            strategy,
            decision,
            status,
            findings: Vec::new(),
            failed_predicates: Vec::new(),
            shapes: Vec::new(),
            advice_references: Vec::new(),
            status_detail: None,
        }
    }

    fn predicates_from_decision(decision: &CompileDecision) -> Vec<String> {
        match decision {
            CompileDecision::Refuse(diagnostics) => diagnostics
                .iter()
                .map(|diagnostic| diagnostic.predicate.to_string())
                .fold(Vec::new(), |mut predicates, predicate| {
                    if !predicates.contains(&predicate) {
                        predicates.push(predicate);
                    }
                    predicates
                }),
            CompileDecision::Admit | CompileDecision::ConfirmOnly => Vec::new(),
        }
    }

    pub fn accepted(
        strategy: EmissionStrategy,
        decision: CompileDecision,
        findings: Vec<HealthFinding>,
    ) -> Result<Self, &'static str> {
        if matches!(decision, CompileDecision::Refuse(_)) {
            return Err("an accepted backend report cannot carry a refusal");
        }
        let mut report = Self::base(strategy, decision, BackendStatus::Accepted);
        report.findings = findings;
        Ok(report)
    }

    /// A refusal's `HealthFinding` and failed-predicate list, from data alone — no advice
    /// catalog. `pg_foma::backend_selection::refused` is the production entry point: it calls
    /// this, then attaches catalog-derived shapes/remedies via `Self::with_capability_advice`.
    pub fn refused(strategy: EmissionStrategy, decision: CompileDecision) -> Self {
        let mut report = Self::base(strategy, decision, BackendStatus::Refused);
        report.failed_predicates = Self::predicates_from_decision(&report.decision);
        if let CompileDecision::Refuse(diagnostics) = &report.decision {
            let explanation = diagnostics
                .iter()
                .map(|diagnostic| {
                    format!(
                        "predicate={} construct={} witness={}",
                        diagnostic.predicate, diagnostic.construct, diagnostic.witness
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            report.findings.push(
                HealthFinding::new(
                    FindingCode::BackendCoverageIncomplete,
                    Severity::CannotRepresent,
                    Phase::Characterization,
                    Metric::BackendCoverageGapCount,
                    MetricValue::Count(diagnostics.len() as u64),
                    ValueProvenance::Observed,
                    format!(
                        "{:?} cannot prove a complete FST relation for {} characterized \
                         construct(s): {explanation}",
                        report.strategy,
                        diagnostics.len()
                    ),
                )
                .affecting(
                    diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.construct.clone())
                        .collect(),
                ),
            );
        }
        report
    }

    /// Attaches catalog-derived shapes and deduplicated remedy references to a refusal. A no-op
    /// builder call for any other status; the advice catalog only ever explains a refusal.
    #[must_use]
    pub fn with_capability_advice(
        mut self,
        shapes: impl IntoIterator<Item = String>,
        advice_references: impl IntoIterator<Item = AdviceReference>,
    ) -> Self {
        for shape in shapes {
            if !self.shapes.contains(&shape) {
                self.shapes.push(shape);
            }
        }
        self.advice_references.extend(advice_references);
        self.advice_references = dedup_advice_references(std::mem::take(&mut self.advice_references));
        self
    }

    pub fn missing(strategy: EmissionStrategy, detail: impl Into<String>) -> Self {
        let mut report = Self::base(
            strategy,
            CompileDecision::Refuse(Vec::new()),
            BackendStatus::Missing,
        );
        report.status_detail = Some(detail.into());
        // Nothing attempted to compile, so this is a build-process fault, not a compile failure.
        attach_operational_failure(&mut report, FindingCode::BuildProcessFailed);
        report
    }

    pub fn failed(strategy: EmissionStrategy, detail: impl Into<String>) -> Self {
        let mut report = Self::base(
            strategy,
            CompileDecision::Refuse(Vec::new()),
            BackendStatus::Failed,
        );
        report.status_detail = Some(detail.into());
        // A compile attempt ran and failed, matching BackendCompilationFailed's own doc.
        attach_operational_failure(&mut report, FindingCode::BackendCompilationFailed);
        report
    }

    pub fn declined_on(&self) -> &[CapabilityDiagnostic] {
        match &self.decision {
            CompileDecision::Refuse(diagnostics) => diagnostics,
            CompileDecision::Admit | CompileDecision::ConfirmOnly => &[],
        }
    }
}

fn attach_operational_failure(report: &mut BackendReport, code: FindingCode) {
    let detail = report
        .status_detail
        .as_deref()
        .unwrap_or("backend construction did not complete");
    report.findings.push(
        HealthFinding::new(
            code,
            Severity::NotProductionReady,
            Phase::Compile,
            Metric::UnknownUnboundedWork,
            MetricValue::Count(1),
            ValueProvenance::Observed,
            format!("{:?} is not buildable: {detail}", report.strategy),
        )
        .affecting(vec![format!("{:?}", report.strategy)]),
    );
    // No advice: the catalog advises grammar changes, and no grammar change starts a compiler.
}

/// Every backend's report for one grammar compile.
#[derive(Debug, Clone, PartialEq)]
pub struct BackendSelection {
    reports: Vec<BackendReport>,
}

impl BackendSelection {
    /// Builds a selection from already-decided reports, in the order given. The decision of WHICH
    /// backends to report on and in what order is `pg_foma::backend_selection::select_backends`'s
    /// job, not this constructor's.
    pub fn from_reports(reports: Vec<BackendReport>) -> Self {
        Self { reports }
    }

    pub fn reports(&self) -> &[BackendReport] {
        &self.reports
    }

    /// One named backend's report, or `None` if it was not composed.
    pub fn report_for(&self, strategy: EmissionStrategy) -> Option<&BackendReport> {
        self.reports.iter().find(|r| r.strategy == strategy)
    }
}
