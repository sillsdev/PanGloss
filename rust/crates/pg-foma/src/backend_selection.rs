//! The SELECTOR: which backend(s) compile a given grammar, and — for each one that does not — the
//! named construct it declined on.
//!
//! # The gap this fills
//! `crate::capability::StrategyEnvelope` already holds every backend's own compatibility report,
//! and `crate::capability::StrategyEnvelope::global` joins them into one whole-grammar answer. The
//! join is the right shape for "is this grammar compilable AT ALL" and the wrong shape for every
//! caller that is about to run ONE backend: a non-refusing join can mean "some other backend can do
//! this", which is no licence for the backend actually in hand. Nothing in this workspace turned
//! the envelope into a choice, so callers reached for the join and inherited that ambiguity.
//!
use pg_grammar::model::Grammar;

use crate::advice_catalog::{
    builtin_catalog, RemedyEffort, PLAN_COMPOSED_MISSING_SUBTREES_SHAPE_KEY,
};
use crate::capability::{
    compose_envelope_across_strategies, default_registry, meet, CapabilityDiagnostic,
    CompileDecision, StrategyEnvelope,
};
use crate::enumerate::{enumerate_default, EmissionStrategy};
use crate::grammar_semantics::GrammarSemantics;
use crate::health::{
    FindingCode, HealthFinding, Metric, MetricValue, Phase, Severity, ValueProvenance,
};
use crate::junctions::PhonologyProbe;
use crate::plan::FragmentSpec;
use crate::strategy_coverage::ALL_STRATEGIES;

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
    pub effort: RemedyEffort,
}

impl AdviceReference {
    pub fn new(
        shape_key: impl Into<String>,
        remedy_key: impl Into<String>,
        effort: RemedyEffort,
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

    /// The backend's own `crate::capability::CompileDecision` — kept whole, so a caller can tell an
    /// `Admit` path from a `ConfirmOnly` one rather than only "selected or not".
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

    pub fn refused(strategy: EmissionStrategy, decision: CompileDecision) -> Self {
        let mut report = Self::base(strategy, decision, BackendStatus::Refused);
        report.failed_predicates = Self::predicates_from_decision(&report.decision);
        attach_capability_refusal(&mut report);
        report
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

fn capability_shape_key(diagnostic: &CapabilityDiagnostic) -> &'static str {
    match diagnostic.predicate {
        "strategy-materializer.marker-subtree-not-buildable" => {
            PLAN_COMPOSED_MISSING_SUBTREES_SHAPE_KEY
        }
        "circumfix-output-action.faithful-structural-composite" => "late-structural-reachability",
        "reduplication.peel-eligible-rule-kind" => "nonregular-process-morphology",
        "compounding.non-recursive" | "quantifier.bounded-expansion" => "repeated-application",
        "unordered-application.chain-depth-bounded" => "unordered-interactions",
        "multi-table.faithful-table-threading"
        | "right-to-left-rewrite.faithful-reversal-construction"
        | "metathesis.faithful-swap-construction"
        | "simultaneous.subrule-overlap"
        | "epenthesis.structural-composite-route" => "wide-phonology",
        _ if diagnostic
            .construct
            .to_ascii_lowercase()
            .contains("truncat")
            || diagnostic.construct.to_ascii_lowercase().contains("delet") =>
        {
            "structural-deletion-or-truncation"
        }
        _ if diagnostic.construct.to_ascii_lowercase().contains("slot") => {
            "optional-slot-branching"
        }
        _ if diagnostic.construct.to_ascii_lowercase().contains("null")
            || diagnostic
                .construct
                .to_ascii_lowercase()
                .contains("zero-surface") =>
        {
            "null-cycle"
        }
        _ => "nonregular-process-morphology",
    }
}

fn attach_capability_refusal(report: &mut BackendReport) {
    let CompileDecision::Refuse(diagnostics) = &report.decision else {
        return;
    };
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
                "{:?} cannot prove a complete FST relation for {} characterized construct(s): \
             {explanation}",
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

    let catalog = builtin_catalog().expect("the embedded backend advice catalog must validate");
    for diagnostic in diagnostics {
        let shape_key = capability_shape_key(diagnostic);
        let entry = catalog
            .entry_for(shape_key)
            .expect("every capability-refusal shape must exist in the advice catalog");
        if !report.shapes.contains(&entry.shape_key) {
            report.shapes.push(entry.shape_key.clone());
        }
        report
            .advice_references
            .extend(entry.remedies.iter().map(|remedy| {
                AdviceReference::new(
                    entry.shape_key.clone(),
                    remedy.remedy_key.clone(),
                    remedy.effort,
                )
            }));
    }
    report.advice_references =
        dedup_advice_references(std::mem::take(&mut report.advice_references));
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

/// The surface probe's own structural refusal, in the same shape as the plan-composed marker one.
///
/// Not a capability predicate, and deliberately so. A predicate answers a `CharacteristicKind`
/// question, and "this route cannot bound a realizational rule's closure" is not one — the kinds it
/// would fall under are `Proven` or carry ratified `ConfirmOnly` verdicts this must not overturn.
/// The marker refusal beside it is the precedent: a backend-specific structural fact met into one
/// strategy's decision, leaving `compose_envelope`'s per-kind answers untouched.
fn tuned_surface_closure_refusal() -> CompileDecision {
    CompileDecision::Refuse(vec![CapabilityDiagnostic {
        predicate: "surface-probe.finite-closure-bound",
        construct: "realizational rule with no authored application bound".to_string(),
        witness: "EmissionStrategy::TunedSurfaceProbed's eager route cannot prove finite closure \
                  for this grammar and generates no partial FST, so selecting it would promise a \
                  proposer the compiler does not produce"
            .to_string(),
    }])
}

fn plan_composed_marker_refusal(markers: &[FragmentSpec]) -> CompileDecision {
    CompileDecision::Refuse(
        markers
            .iter()
            .map(|marker| {
                let marker = format!("{marker:?}");
                CapabilityDiagnostic {
                    predicate: "strategy-materializer.marker-subtree-not-buildable",
                    construct: marker.clone(),
                    witness: format!(
                        "EmissionStrategy::PlanComposed uses build_controllable, which skips the \
                         required {marker} subtree; selecting PlanComposed would silently omit its \
                         material"
                    ),
                }
            })
            .collect(),
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackendSelection {
    reports: Vec<BackendReport>,
}

impl BackendSelection {
    pub fn reports(&self) -> &[BackendReport] {
        &self.reports
    }

    /// One named backend's report, or `None` if it was not composed.
    pub fn report_for(&self, strategy: EmissionStrategy) -> Option<&BackendReport> {
        self.reports.iter().find(|r| r.strategy == strategy)
    }

    /// One report per backend in [`crate::strategy_coverage::ALL_STRATEGIES`] declaration order.
    fn from_envelope_with_backend_findings(
        envelope: &StrategyEnvelope,
        plan_composed_markers: &[FragmentSpec],
        tuned_surface_closure_unbounded: bool,
    ) -> Self {
        let reports = ALL_STRATEGIES
            .iter()
            .map(|&strategy| {
                let Some(decision) = envelope.decision_for(strategy) else {
                    return BackendReport::missing(strategy, "backend was not available");
                };
                // Marker leaves name subtrees build_controllable cannot build.
                let decision = if strategy == EmissionStrategy::PlanComposed
                    && !plan_composed_markers.is_empty()
                {
                    meet(
                        decision.clone(),
                        plan_composed_marker_refusal(plan_composed_markers),
                    )
                } else if strategy == EmissionStrategy::TunedSurfaceProbed
                    && tuned_surface_closure_unbounded
                {
                    meet(decision.clone(), tuned_surface_closure_refusal())
                } else {
                    decision.clone()
                };
                if matches!(decision, CompileDecision::Refuse(_)) {
                    BackendReport::refused(strategy, decision)
                } else {
                    BackendReport::accepted(strategy, decision, Vec::new())
                        .expect("a non-refusing decision is always accepted")
                }
            })
            .collect();
        Self { reports }
    }
}

/// Selects over an already-derived `crate::grammar_semantics::GrammarSemantics` — the primary form,
/// since deriving one runs the whole `crate::capability::characterize` walk and a caller that
/// already holds a semantics should never pay for a second.
pub fn select_backends(semantics: &GrammarSemantics<'_>) -> BackendSelection {
    let g = semantics.grammar();
    let phon = PhonologyProbe::new_with_semantics(semantics);
    let plan = enumerate_default(g, semantics.prules_in_order(), phon.as_ref());
    let envelope = compose_envelope_across_strategies(semantics, &plan, &default_registry());
    let plan_composed_markers = crate::build::unbuildable_markers(&plan);
    BackendSelection::from_envelope_with_backend_findings(
        &envelope,
        &plan_composed_markers,
        crate::emit::eager_route_refuses_unbounded_closure(g),
    )
}

/// `select_backends` from a bare `&Grammar`, deriving the semantics itself. **Check-only**: nothing
/// here builds a `foma::types::Fsm`, runs foma, or alters any compile path.
pub fn select_backends_for_grammar(g: &Grammar) -> BackendSelection {
    select_backends(&GrammarSemantics::derive(g))
}
