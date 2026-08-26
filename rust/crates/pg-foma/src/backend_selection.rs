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
//! # Correctness and buildability select; readiness labels
//! A backend is a normal-generation candidate when its own report is correctness-admitted (`Admit`
//! or `ConfirmOnly` — `ConfirmOnly` is a recall-preserving mode, not a defect) AND none of its
//! findings show it cannot produce a usable artifact: a `crate::health::FindingClass::
//! Representability` finding (the feature cannot be faithfully proposed) or a `FindingClass::
//! Containment` finding (this attempt was itself stopped, internally or by the host watchdog,
//! before finishing) both mean there is nothing built to select. A `FindingClass::Readiness`
//! finding — including `Severity::NotProductionReady`, e.g. an oversized compiled payload — is a
//! label on an artifact that DID get built, so it never excludes a backend here; see
//! `BackendReport::is_normal_candidate` for the exact predicate. Refusals and every excluded
//! report remain visible in `reports`/`excluded`, never silently dropped.
//!
//! Candidate ranking is deterministic and deliberately modest: clean reports first, then worst
//! severity, then finding count, with `BACKEND_PREFERENCE` only as the final tie-break. Cost
//! evidence is retained for callers and explanations; it never overrides correctness.

use pg_grammar::model::Grammar;

use crate::advice_catalog::{
    builtin_catalog, RemedyEffort, BACKEND_BUILD_UNAVAILABLE_SHAPE_KEY,
    PLAN_COMPOSED_MISSING_SUBTREES_SHAPE_KEY, TUNED_SURFACE_CLOSURE_BUDGET_SHAPE_KEY,
};
use crate::capability::{
    compose_envelope_across_strategies, default_registry, meet, CapabilityDiagnostic,
    CompileDecision, StrategyEnvelope,
};
use crate::emit::surface_table;
use crate::enumerate::{enumerate_default, EmissionStrategy};
use crate::grammar_semantics::GrammarSemantics;
use crate::health::{
    FindingClass, FindingCode, HealthFinding, Metric, MetricValue, Phase, Severity,
    ValueProvenance,
};
use crate::junctions::PhonologyProbe;
use crate::plan::FragmentSpec;
use crate::replace::SegAlphabet;

/// The order `BackendSelection::preferred` breaks a tie between viable backends in.
///
/// `crate::enumerate::EmissionStrategy::TunedSurfaceProbed` leads because it is the backend this
/// crate's shipping analyzer realizes (`crate::analyzer::FomaProposer::EMISSION_STRATEGY`), so
/// "the preferred backend" and "the backend a `pangloss` invocation actually runs" name the same
/// thing unless a caller deliberately says otherwise. The remaining two are ordered whole-grammar
/// first, since `EmissionStrategy::is_whole_grammar` is the difference between compiling the
/// grammar and compiling its controllable subtree.
///
/// This is a policy constant, not a derived fact: see this module's own doc.
pub const BACKEND_PREFERENCE: &[EmissionStrategy] = &[
    EmissionStrategy::TunedSurfaceProbed,
    EmissionStrategy::TemplatedUnderlyingTokens,
    EmissionStrategy::PlanComposed,
];

fn preference_index(strategy: EmissionStrategy) -> usize {
    BACKEND_PREFERENCE
        .iter()
        .position(|candidate| *candidate == strategy)
        .unwrap_or(usize::MAX)
}

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

/// One measured or predicted cost datum carried beside a backend's stable findings.
#[derive(Debug, Clone, PartialEq)]
pub struct CostEvidence {
    pub metric: Metric,
    pub value: MetricValue,
    pub threshold: Option<MetricValue>,
    pub provenance: ValueProvenance,
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

fn remedy_set_key(set: &[AdviceReference]) -> (usize, usize, usize, Vec<AdviceReference>) {
    let set = dedup_advice_references(set.to_vec());
    let mut hard = 0;
    let mut medium = 0;
    let mut easy = 0;
    for reference in &set {
        match reference.effort {
            RemedyEffort::Hard => hard += 1,
            RemedyEffort::Medium => medium += 1,
            RemedyEffort::Easy => easy += 1,
        }
    }
    (hard, medium, easy, set)
}

/// Deterministically orders alternative blocking remedy sets by hard, medium, and easy effort.
///
/// Correctness admission is decided before this ordering is consulted. This function only helps
/// explain which cataloged remedy set would be least work for a backend that is currently blocked.
pub fn sort_blocking_remedy_sets(sets: Vec<Vec<AdviceReference>>) -> Vec<Vec<AdviceReference>> {
    let mut keyed: Vec<_> = sets.into_iter().map(|set| remedy_set_key(&set)).collect();
    keyed.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| left.3.cmp(&right.3))
    });
    keyed.into_iter().map(|(_, _, _, set)| set).collect()
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
    cost_evidence: Vec<CostEvidence>,
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

    pub fn cost_evidence(&self) -> &[CostEvidence] {
        &self.cost_evidence
    }

    pub fn advice_references(&self) -> &[AdviceReference] {
        &self.advice_references
    }

    pub fn status_detail(&self) -> Option<&str> {
        self.status_detail.as_deref()
    }

    /// The worst health finding for this backend. Overrides are intentionally retained in this
    /// aggregate: an explicit development override does not silently turn a
    /// NotProductionReady/MachineLimit/CannotRepresent backend into a normal-generation candidate.
    pub fn worst_severity(&self) -> Severity {
        self.findings
            .iter()
            .map(|finding| finding.severity)
            .max()
            .unwrap_or(Severity::WithinLimits)
    }

    /// A backend is a normal-generation candidate when correctness admits it AND no finding shows
    /// it cannot produce a usable artifact. That second test asks "which question failed", never
    /// "how high on the severity scale" — a `FindingClass::Representability` finding means the
    /// feature cannot be faithfully proposed, a `FindingClass::Containment` finding means this
    /// attempt itself was stopped (self-imposed budget or the external host watchdog) before
    /// producing one, and a `FindingClass::Process` finding means the attempt failed for a reason
    /// unrelated to the grammar (bad input, worker/protocol failure); all three leave nothing to
    /// build. The `status == Accepted` check already makes the `Process` exclusion redundant for
    /// every producer that exists today (`BackendReport::missing`/`failed` both attach a Process
    /// finding and also set a non-`Accepted` status), pinned by
    /// `a_process_finding_excludes_even_if_status_were_accepted`; listing it here too makes the
    /// predicate correct on its own terms rather than by that coincidence. A `FindingClass::
    /// Readiness` finding — including one at `Severity::NotProductionReady`, e.g. an oversized
    /// payload — labels an artifact that DID get built, so it never excludes here; publication
    /// gating for it lives in `pg_cli::pack::validate_health_readiness`, a separate gate this
    /// predicate does not reach.
    pub fn is_normal_candidate(&self) -> bool {
        self.status == BackendStatus::Accepted
            && !matches!(self.decision, CompileDecision::Refuse(_))
            && !self.findings.iter().any(|finding| {
                matches!(
                    finding.code.class(),
                    FindingClass::Representability | FindingClass::Containment | FindingClass::Process
                )
            })
    }

    fn rank_key(&self) -> (bool, Severity, usize) {
        (
            // `false` sorts before `true`: a zero-finding report wins before severity is consulted.
            !self.findings.is_empty(),
            self.worst_severity(),
            self.findings.len(),
        )
    }

    fn base(strategy: EmissionStrategy, decision: CompileDecision, status: BackendStatus) -> Self {
        Self {
            strategy,
            decision,
            status,
            findings: Vec::new(),
            failed_predicates: Vec::new(),
            shapes: Vec::new(),
            cost_evidence: Vec::new(),
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

    pub fn with_diagnostics(
        mut self,
        failed_predicates: Vec<String>,
        shapes: Vec<String>,
        cost_evidence: Vec<CostEvidence>,
        advice_references: Vec<AdviceReference>,
    ) -> Self {
        self.failed_predicates = failed_predicates;
        self.shapes = shapes;
        self.cost_evidence = cost_evidence;
        self.advice_references = dedup_advice_references(advice_references);
        self
    }

    /// Whether this backend is a normal-generation path for the grammar. Refused, missing, and
    /// failed reports are retained but not selected, as is any report with a Representability or
    /// Containment finding; a Readiness-only report (e.g. NotProductionReady payload size) IS
    /// selected — see `is_normal_candidate`.
    pub fn is_selected(&self) -> bool {
        self.is_normal_candidate()
    }

    /// Why this backend was not selected: the diagnostics naming the construct it declined on, or
    /// an empty slice when it WAS selected. Non-empty exactly when `is_selected` is false, so the
    /// reason for an exclusion is never absent from a report that has one.
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
    report.findings.push(HealthFinding {
        code: FindingCode::BackendCoverageIncomplete,
        severity: Severity::CannotRepresent,
        phase: Phase::Characterization,
        affected: diagnostics
            .iter()
            .map(|diagnostic| diagnostic.construct.clone())
            .collect(),
        metric: Metric::BackendCoverageGapCount,
        value: MetricValue::Count(diagnostics.len() as u64),
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation: format!(
            "{:?} cannot prove a complete FST relation for {} characterized construct(s): \
             {explanation}",
            report.strategy,
            diagnostics.len()
        ),
        remedies: Vec::new(),
    });

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
    report.findings.push(HealthFinding {
        code,
        severity: Severity::NotProductionReady,
        phase: Phase::Compile,
        affected: vec![format!("{:?}", report.strategy)],
        metric: Metric::UnknownUnboundedWork,
        value: MetricValue::Count(1),
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation: format!("{:?} is not buildable: {detail}", report.strategy),
        remedies: Vec::new(),
    });

    let catalog = builtin_catalog().expect("the embedded backend advice catalog must validate");
    let entry = catalog
        .entry_for(BACKEND_BUILD_UNAVAILABLE_SHAPE_KEY)
        .expect("backend build failures must have a catalog entry");
    report.shapes.push(entry.shape_key.clone());
    report
        .advice_references
        .extend(entry.remedies.iter().map(|remedy| {
            AdviceReference::new(
                entry.shape_key.clone(),
                remedy.remedy_key.clone(),
                remedy.effort,
            )
        }));
    report.advice_references =
        dedup_advice_references(std::mem::take(&mut report.advice_references));
}

/// Attaches the backend-native TunedSurface resource finding and its catalogued advice.
fn attach_tuned_surface_resource_finding(report: &mut BackendReport, finding: HealthFinding) {
    let catalog = builtin_catalog().expect("the embedded backend advice catalog must validate");
    let entry = catalog
        .entry_for(TUNED_SURFACE_CLOSURE_BUDGET_SHAPE_KEY)
        .expect("the TunedSurface resource finding must have a catalog entry");

    report.findings.push(finding.clone());
    if !report
        .failed_predicates
        .iter()
        .any(|predicate| predicate == &entry.failed_predicate)
    {
        report
            .failed_predicates
            .push(entry.failed_predicate.clone());
    }
    if !report.shapes.iter().any(|shape| shape == &entry.shape_key) {
        report.shapes.push(entry.shape_key.clone());
    }
    report.cost_evidence.push(CostEvidence {
        metric: finding.metric,
        value: finding.value,
        threshold: finding.threshold,
        provenance: finding.provenance,
    });
    report
        .advice_references
        .extend(entry.remedies.iter().map(|remedy| {
            AdviceReference::new(
                entry.shape_key.clone(),
                remedy.remedy_key.clone(),
                remedy.effort,
            )
        }));
    report.advice_references =
        dedup_advice_references(std::mem::take(&mut report.advice_references));
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

/// The selector's answer for one grammar: every backend's report, in `BACKEND_PREFERENCE` order.
///
/// No path, one path and several paths are all ordinary states of this type — `selected` returns
/// an empty, one-element or many-element list respectively, and `reports` still carries every
/// declining backend's named construct in the empty case, which is the case a caller most needs to
/// explain.
#[derive(Debug, Clone, PartialEq)]
pub struct BackendSelection {
    reports: Vec<BackendReport>,
}

impl BackendSelection {
    /// Normalizes a caller-supplied set into one report for every committed backend. Missing
    /// entries become explicit `Missing` reports rather than disappearing from the explanation.
    pub fn from_reports(reports: Vec<BackendReport>) -> Self {
        let mut reports = reports;
        let normalized = BACKEND_PREFERENCE
            .iter()
            .map(|&strategy| {
                reports
                    .iter()
                    .position(|report| report.strategy == strategy)
                    .map(|index| reports.remove(index))
                    .unwrap_or_else(|| {
                        BackendReport::missing(strategy, "backend was not available")
                    })
            })
            .collect();
        Self {
            reports: normalized,
        }
    }

    /// Builds reports from an envelope and optionally attaches the backend-native TunedSurface
    /// resource characterization.  Every committed backend still receives exactly one report,
    /// including when TunedSurface is refused by capability admission.
    pub fn from_envelope_with_tuned_surface_resource_finding(
        envelope: &StrategyEnvelope,
        finding: Option<HealthFinding>,
    ) -> Self {
        Self::from_envelope_with_backend_findings(envelope, finding, &[])
    }

    fn from_envelope_with_backend_findings(
        envelope: &StrategyEnvelope,
        tuned_surface_finding: Option<HealthFinding>,
        plan_composed_markers: &[FragmentSpec],
    ) -> Self {
        let reports = BACKEND_PREFERENCE
            .iter()
            .filter_map(|&strategy| {
                envelope.decision_for(strategy).map(|decision| {
                    let decision = if strategy == EmissionStrategy::PlanComposed
                        && !plan_composed_markers.is_empty()
                    {
                        meet(
                            decision.clone(),
                            plan_composed_marker_refusal(plan_composed_markers),
                        )
                    } else {
                        decision.clone()
                    };
                    let mut report = if matches!(decision, CompileDecision::Refuse(_)) {
                        BackendReport::refused(strategy, decision)
                    } else {
                        BackendReport::accepted(strategy, decision, Vec::new())
                            .expect("non-refusing decision must be accepted")
                    };
                    if strategy == EmissionStrategy::TunedSurfaceProbed {
                        if let Some(finding) = tuned_surface_finding.clone() {
                            attach_tuned_surface_resource_finding(&mut report, finding);
                        }
                    }
                    report
                })
            })
            .collect();
        Self::from_reports(reports)
    }

    /// Every backend's report, in `BACKEND_PREFERENCE` order — selected and excluded alike, since
    /// "why not that one" is answerable only from the excluded ones.
    pub fn reports(&self) -> &[BackendReport] {
        &self.reports
    }

    /// One named backend's report, or `None` if it was not composed.
    pub fn report_for(&self, strategy: EmissionStrategy) -> Option<&BackendReport> {
        self.reports.iter().find(|r| r.strategy == strategy)
    }

    /// Normal-generation candidates, ranked by report quality. Empty is the "no path" answer.
    pub fn selected(&self) -> Vec<EmissionStrategy> {
        self.ranked_candidates()
            .into_iter()
            .map(|r| r.strategy)
            .collect()
    }

    /// The single highest-ranked normal-generation backend, or `None` when no backend is
    /// correctness- and health-admitted.
    pub fn preferred(&self) -> Option<EmissionStrategy> {
        self.ranked_candidates().first().map(|r| r.strategy)
    }

    fn ranked_candidates(&self) -> Vec<&BackendReport> {
        let mut candidates: Vec<_> = self.reports.iter().filter(|r| r.is_selected()).collect();
        candidates.sort_by(|a, b| {
            a.rank_key()
                .cmp(&b.rank_key())
                .then_with(|| preference_index(a.strategy).cmp(&preference_index(b.strategy)))
        });
        candidates
    }

    /// Every backend that declined, with the constructs it declined on — the per-backend
    /// attribution a single whole-grammar verdict cannot carry.
    pub fn excluded(&self) -> Vec<(EmissionStrategy, &[CapabilityDiagnostic])> {
        self.reports
            .iter()
            .filter(|r| !r.is_selected())
            .map(|r| (r.strategy, r.declined_on()))
            .collect()
    }

    /// Whether no backend at all can compile this grammar. Distinct from an empty `excluded`, which
    /// says the opposite. Pinned by `no_path_is_representable_and_carries_every_reason`.
    pub fn is_no_path(&self) -> bool {
        !self.reports.is_empty() && self.reports.iter().all(|r| !r.is_selected())
    }
}

/// Selects over an already-derived `crate::grammar_semantics::GrammarSemantics` — the primary form,
/// since deriving one runs the whole `crate::capability::characterize` walk and a caller that
/// already holds a semantics should never pay for a second.
pub fn select_backends(semantics: &GrammarSemantics<'_>) -> BackendSelection {
    let g = semantics.grammar();
    let alphabet = SegAlphabet::new(surface_table(g));
    let phon = PhonologyProbe::new_with_semantics(semantics);
    let plan = enumerate_default(g, &alphabet, semantics.prules_in_order(), phon.as_ref());
    let envelope = compose_envelope_across_strategies(semantics, &plan, &default_registry());
    let plan_composed_markers = crate::build::unbuildable_markers(&plan);
    BackendSelection::from_envelope_with_backend_findings(
        &envelope,
        crate::characterization::tuned_surface_resource_finding_with_limit(
            g,
            crate::characterization::DEFAULT_TUNED_CLOSURE_WORK_LIMIT,
        ),
        &plan_composed_markers,
    )
}

/// `select_backends` from a bare `&Grammar`, deriving the semantics itself. **Check-only**: nothing
/// here builds a `foma::types::Fsm`, runs foma, or alters any compile path.
pub fn select_backends_for_grammar(g: &Grammar) -> BackendSelection {
    select_backends(&GrammarSemantics::derive(g))
}

#[cfg(test)]
mod tests {
    //! Synthetic, delanguaged fixtures only, in this crate's established test-module style: XML
    //! through `pg_grammar::load` rather than a hand-built `Grammar`.

    use super::*;
    use crate::capability::CapabilityDiagnostic;

    fn diagnostic(construct: &str) -> CapabilityDiagnostic {
        CapabilityDiagnostic {
            predicate: "synthetic.test-only",
            construct: construct.to_string(),
            witness: "synthetic".to_string(),
        }
    }

    fn finding(
        severity: crate::health::Severity,
        code: crate::health::FindingCode,
    ) -> crate::health::HealthFinding {
        crate::health::HealthFinding {
            code,
            severity,
            phase: crate::health::Phase::Compile,
            affected: vec!["synthetic-rule".to_string()],
            metric: crate::health::Metric::EmittedLineCount,
            value: crate::health::MetricValue::Count(1),
            provenance: crate::health::ValueProvenance::Observed,
            threshold: None,
            explanation: "synthetic finding".to_string(),
            remedies: Vec::new(),
        }
    }

    fn envelope_of(rows: &[(EmissionStrategy, CompileDecision)]) -> BackendSelection {
        BackendSelection::from_reports(
            BACKEND_PREFERENCE
                .iter()
                .filter_map(|&strategy| {
                    rows.iter()
                        .find(|(s, _)| *s == strategy)
                        .map(|(_, decision)| match decision {
                            CompileDecision::Refuse(_) => {
                                BackendReport::refused(strategy, decision.clone())
                            }
                            CompileDecision::Admit | CompileDecision::ConfirmOnly => {
                                BackendReport::accepted(strategy, decision.clone(), Vec::new())
                                    .expect("a non-refusing decision is accepted")
                            }
                        })
                })
                .collect(),
        )
    }

    /// A refusing backend is excluded and carries its diagnostics; a `ConfirmOnly` one is selected, since `ConfirmOnly` is recall-preserving rather than a defect.
    #[test]
    fn a_refusing_backend_is_never_selected() {
        let selection = envelope_of(&[
            (
                EmissionStrategy::TunedSurfaceProbed,
                CompileDecision::Refuse(vec![diagnostic("stratum 0 (Unordered)")]),
            ),
            (
                EmissionStrategy::TemplatedUnderlyingTokens,
                CompileDecision::ConfirmOnly,
            ),
            (EmissionStrategy::PlanComposed, CompileDecision::Admit),
        ]);

        assert_eq!(
            selection.selected(),
            vec![
                EmissionStrategy::TemplatedUnderlyingTokens,
                EmissionStrategy::PlanComposed
            ]
        );
        assert_eq!(
            selection.preferred(),
            Some(EmissionStrategy::TemplatedUnderlyingTokens),
            "preference order decides among viable backends, and the refused one is not viable"
        );
        let excluded = selection.excluded();
        assert_eq!(excluded.len(), 1);
        assert_eq!(excluded[0].0, EmissionStrategy::TunedSurfaceProbed);
        assert_eq!(excluded[0].1[0].construct, "stratum 0 (Unordered)");
        let report = selection
            .report_for(EmissionStrategy::TunedSurfaceProbed)
            .expect("the refusal remains reportable");
        assert_eq!(report.worst_severity(), Severity::CannotRepresent);
        assert_eq!(
            report.findings()[0].code,
            FindingCode::BackendCoverageIncomplete
        );
        assert!(!report.advice_references().is_empty());
        assert!(!selection.is_no_path());
    }

    /// Every backend refusing is a first-class answer, and every backend's own reason survives into it.
    #[test]
    fn no_path_is_representable_and_carries_every_reason() {
        let selection = envelope_of(&[
            (
                EmissionStrategy::TunedSurfaceProbed,
                CompileDecision::Refuse(vec![diagnostic("tuned construct")]),
            ),
            (
                EmissionStrategy::TemplatedUnderlyingTokens,
                CompileDecision::Refuse(vec![diagnostic("templated construct")]),
            ),
            (
                EmissionStrategy::PlanComposed,
                CompileDecision::Refuse(vec![diagnostic("plan construct")]),
            ),
        ]);

        assert!(selection.is_no_path());
        assert!(selection.selected().is_empty());
        assert_eq!(selection.preferred(), None);
        let constructs: Vec<&str> = selection
            .excluded()
            .iter()
            .map(|(_, diags)| diags[0].construct.as_str())
            .collect();
        assert_eq!(
            constructs,
            vec!["tuned construct", "templated construct", "plan construct"],
            "no path must still name what each backend declined on"
        );
    }

    /// The selector reads the SAME per-backend verdicts the envelope holds, so an envelope whose backends disagree is not collapsed to its join.
    #[test]
    fn the_selection_is_per_backend_not_the_envelope_join() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>SelectorFixture</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <RealizationalRule id="rr1">
                  <Name>Realiz</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput><PhoneticSequence id="s0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
                      <MorphologicalOutput><CopyFromInput index="s0" /></MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </RealizationalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = pg_grammar::load(XML).expect("fixture must load");
        let selection = select_backends_for_grammar(&g);

        let plan_composed = selection
            .report_for(EmissionStrategy::PlanComposed)
            .expect("every backend must be reported");
        assert!(
            !plan_composed.is_selected(),
            "the plan-composed backend has no lexicon emitter for a realizational rule, so it must \
             be excluded here: {:?}",
            plan_composed.decision()
        );
        assert!(
            selection
                .report_for(EmissionStrategy::TunedSurfaceProbed)
                .expect("every backend must be reported")
                .is_selected(),
            "the same grammar must stay a path for the backend that can represent it"
        );
        assert_eq!(
            selection.preferred(),
            Some(EmissionStrategy::TunedSurfaceProbed)
        );
    }

    #[test]
    fn reports_retain_every_backend_and_rank_only_normal_candidates() {
        let reports = vec![
            BackendReport::accepted(
                EmissionStrategy::TunedSurfaceProbed,
                CompileDecision::Admit,
                vec![finding(
                    crate::health::Severity::LargeMultiplier,
                    crate::health::FindingCode::PayloadSizeBand,
                )],
            )
            .unwrap(),
            BackendReport::accepted(
                EmissionStrategy::TemplatedUnderlyingTokens,
                CompileDecision::Admit,
                vec![],
            )
            .unwrap(),
            BackendReport::refused(
                EmissionStrategy::PlanComposed,
                CompileDecision::Refuse(vec![diagnostic("unsupported")]),
            ),
        ];
        let selection = BackendSelection::from_reports(reports);

        assert_eq!(selection.reports().len(), BACKEND_PREFERENCE.len());
        assert_eq!(
            selection.selected(),
            vec![
                EmissionStrategy::TemplatedUnderlyingTokens,
                EmissionStrategy::TunedSurfaceProbed,
            ],
            "clean reports rank ahead of warning reports; refused reports remain retained but are not candidates"
        );
        assert_eq!(
            selection.preferred(),
            Some(EmissionStrategy::TemplatedUnderlyingTokens)
        );
        assert_eq!(
            selection
                .report_for(EmissionStrategy::PlanComposed)
                .expect("refused backend remains reportable")
                .status(),
            BackendStatus::Refused
        );
    }

    /// An oversized payload is a label on something that got built, so it stays selectable.
    #[test]
    fn an_oversized_payload_labels_a_backend_without_excluding_it() {
        let reports = vec![BackendReport::accepted(
            EmissionStrategy::TunedSurfaceProbed,
            CompileDecision::Admit,
            vec![finding(
                crate::health::Severity::NotProductionReady,
                crate::health::FindingCode::PayloadSizeBand,
            )],
        )
        .unwrap()];
        let selection = BackendSelection::from_reports(reports);

        assert!(
            selection
                .selected()
                .contains(&EmissionStrategy::TunedSurfaceProbed),
            "a readiness label must never cost a backend its candidacy"
        );
        assert_eq!(
            selection
                .report_for(EmissionStrategy::TunedSurfaceProbed)
                .expect("the report is retained")
                .worst_severity(),
            crate::health::Severity::NotProductionReady,
            "and the label itself must survive selection"
        );
    }

    /// An approaching-but-not-tripped budget observation must never cost a backend its candidacy.
    #[test]
    fn an_approaching_compile_work_budget_labels_a_backend_without_excluding_it() {
        let reports = vec![BackendReport::accepted(
            EmissionStrategy::TunedSurfaceProbed,
            CompileDecision::Admit,
            vec![finding(
                crate::health::Severity::LargeMultiplier,
                crate::health::FindingCode::CompileWorkBudget,
            )],
        )
        .unwrap()];
        let selection = BackendSelection::from_reports(reports);

        assert!(
            selection
                .selected()
                .contains(&EmissionStrategy::TunedSurfaceProbed),
            "an approaching-budget magnitude finding must never cost a backend its candidacy"
        );
        assert_eq!(
            crate::health::FindingCode::CompileWorkBudget.class(),
            FindingClass::Readiness
        );
    }

    /// The predicate must exclude on `Process` even where `status == Accepted` would already do so.
    #[test]
    fn a_process_finding_excludes_even_if_status_were_accepted() {
        let report = BackendReport::accepted(
            EmissionStrategy::TunedSurfaceProbed,
            CompileDecision::Admit,
            vec![finding(
                crate::health::Severity::NotProductionReady,
                crate::health::FindingCode::BuildProcessFailed,
            )],
        )
        .unwrap();

        assert!(!report.is_normal_candidate());
    }

    /// Nothing was built in either case, so neither can be selected.
    #[test]
    fn containment_and_representability_findings_exclude_a_backend() {
        let reports = vec![
            BackendReport::accepted(
                EmissionStrategy::TunedSurfaceProbed,
                CompileDecision::Admit,
                vec![finding(
                    crate::health::Severity::MachineLimit,
                    crate::health::FindingCode::HostContainmentFired,
                )],
            )
            .unwrap(),
            BackendReport::accepted(
                EmissionStrategy::TemplatedUnderlyingTokens,
                CompileDecision::Admit,
                vec![finding(
                    crate::health::Severity::CannotRepresent,
                    crate::health::FindingCode::BackendCoverageIncomplete,
                )],
            )
            .unwrap(),
        ];
        let selection = BackendSelection::from_reports(reports);

        assert!(selection.selected().is_empty());
        assert_eq!(selection.reports().len(), BACKEND_PREFERENCE.len());
        assert_eq!(
            selection
                .report_for(EmissionStrategy::PlanComposed)
                .expect("missing backend remains retained")
                .status(),
            BackendStatus::Missing
        );
    }

}
