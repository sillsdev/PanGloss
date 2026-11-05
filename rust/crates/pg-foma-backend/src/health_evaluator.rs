//! Scores compiler observations and containment events without re-detecting author findings.

use crate::analyzer::FomaError;
use crate::compose_budget::{ApplyDimension, ComposeError};
use crate::emit::{ClosureRefusalCode, EmitReport, FomaTier};
use crate::health::{
    severity_for_size_bytes, FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase,
    Remedy, Severity, ValueProvenance,
};
/// This evaluator's own distillation of one `crate::compose_budget::ApplyOutcome::Incomplete` —
/// see this module's "Judgment calls" item 6 for why `evaluate` takes this instead of the
/// generic `ApplyOutcome<T>` directly. Callers build one of these per incomplete per-word apply
/// result they want reflected as health evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyBudgetTrip {
    /// Which apply-time magnitude dimension tripped
    /// (`crate::compose_budget::ApplyOutcome::Incomplete`'s own field, copied unchanged).
    pub dimension: ApplyDimension,
    /// The count at the moment of the trip (copied unchanged from `ApplyOutcome::Incomplete`).
    pub value: usize,
    /// The limit that was exceeded (copied unchanged from `ApplyOutcome::Incomplete`).
    pub limit: usize,
    /// The word this trip was observed for, if the caller has it. `None` produces a finding with
    /// an empty `affected` list rather than a fabricated identifier.
    pub word: Option<String>,
}

/// The threshold a non-`Severity::WithinLimits` size finding crossed -- read from the shared `IDEAL_MAX_BYTES` constant so a threshold change cannot desync a second copy.
fn size_band_crossed_threshold(severity: Severity) -> MetricValue {
    match severity {
        Severity::WithinLimits => {
            unreachable!("payload_size_finding filters Severity::WithinLimits before calling this")
        }
        Severity::NotProductionReady => MetricValue::Bytes(crate::health::IDEAL_MAX_BYTES),
        Severity::Elevated
        | Severity::LargeMultiplier
        | Severity::MachineLimit
        | Severity::CannotRepresent => {
            unreachable!(
                "severity_for_size_bytes produces only WithinLimits/NotProductionReady; every other severity is reserved for non-size producers"
            )
        }
    }
}

/// Maps a final FST payload byte count to a `HealthFinding` via `severity_for_size_bytes`; `None` when the payload is within limits.
fn payload_size_finding(bytes: u64) -> Option<HealthFinding> {
    let severity = severity_for_size_bytes(bytes);
    if severity == Severity::WithinLimits {
        return None;
    }
    Some(
        HealthFinding::new(
            FindingCode::PayloadSizeBand,
            severity,
            Phase::Compile,
            Metric::PayloadBytes,
            MetricValue::Bytes(bytes),
            ValueProvenance::Observed,
            format!(
            "Final FST payload is {bytes} bytes, in the {severity:?} band (R6 decimal-byte size \
             thresholds)."
        ),
        )
        .against_threshold(size_band_crossed_threshold(severity)),
    )
}

/// A backend-local unsupported result: no artifact for this route, but not a whole-build invariant failure.
fn unsupported_tier_finding(report: &EmitReport, reason: &str) -> HealthFinding {
    let affected = report
        .closure_refusal
        .as_ref()
        .map(|refusal| {
            refusal
                .affected_rule_ordinals
                .iter()
                .map(|ordinal| format!("mrule{ordinal}"))
                .collect()
        })
        .unwrap_or_default();
    // `enum_budget_exceeded` carries the measured value a budget stop tripped on; without it the finding reported `Unbounded` while the number sat unread on the report.
    let value = report
        .closure_refusal
        .as_ref()
        .and_then(|refusal| refusal.pending_successors)
        .map(|pending| MetricValue::Count(pending as u64))
        .or_else(|| {
            report
                .enum_budget_exceeded
                .as_ref()
                .map(|budget| MetricValue::Count(budget.value as u64))
        })
        .unwrap_or(MetricValue::Unbounded);
    let closure_detail = report
        .closure_refusal
        .as_ref()
        .map(|refusal| match refusal.code {
            ClosureRefusalCode::UnboundedRuleApplication => {
                " The affected rules have no authored finite application bound.".to_string()
            }
            ClosureRefusalCode::DepthBudgetExceeded => format!(
                " The closure-depth limit was {} and {} legal successor(s) remained.",
                refusal.depth_limit.unwrap_or_default(),
                refusal.pending_successors.unwrap_or_default()
            ),
        })
        .or_else(|| {
            report.enum_budget_exceeded.as_ref().map(|budget| {
                format!(
                    " The {} limit was {} and this grammar reached {}.",
                    budget.measure, budget.limit, budget.value
                )
            })
        })
        .unwrap_or_default();
    // Both stop THIS attempt at a raisable cap; reading either as a coverage gap reports a tunable limit as CannotRepresent, a permanent verdict about the grammar.
    let depth_budget_stop = matches!(
        report.closure_refusal.as_ref().map(|refusal| refusal.code),
        Some(ClosureRefusalCode::DepthBudgetExceeded)
    ) || report.enum_budget_exceeded.is_some();
    let (code, severity, explanation) = if depth_budget_stop {
        (
            FindingCode::ResourceBudgetReached,
            Severity::NotProductionReady,
            format!(
                "This grammar's FST-propose path stopped at an internal closure-depth cap before \
                 it finished ({reason}); the attempt is incomplete and its partial output is \
                 unusable, but no fixed affix depth is a language boundary and nothing here shows \
                 the grammar is unrepresentable.{closure_detail}"
            ),
        )
    } else {
        (
            FindingCode::BackendCoverageIncomplete,
            Severity::CannotRepresent,
            format!(
                "This grammar's FST-propose path produced no usable network at all ({reason}); \
                 this compile path's coverage is entirely unknown, the maximal case of R6's \"any \
                 uncertainty that could omit an analysis fails closed\".{closure_detail}"
            ),
        )
    };
    HealthFinding::new(
        code,
        severity,
        Phase::Compile,
        Metric::UnknownUnboundedWork,
        value,
        ValueProvenance::Observed,
        explanation,
    )
    .affecting(affected)
}

fn backend_compilation_failed_finding(detail: String) -> HealthFinding {
    HealthFinding::new(
        FindingCode::BackendCompilationFailed,
        Severity::NotProductionReady,
        Phase::Compile,
        Metric::UnknownUnboundedWork,
        MetricValue::Unbounded,
        ValueProvenance::Observed,
        detail,
    )
}

/// Every `crate::emit::EmitReport`-sourced finding: the tier disposition.
fn emit_report_findings(report: &EmitReport) -> Vec<HealthFinding> {
    let mut findings = Vec::new();
    match &report.tier {
        FomaTier::Full => {}
        FomaTier::Unsupported { reason } => {
            findings.push(unsupported_tier_finding(report, reason));
        }
        FomaTier::CannotRepresent { reason } => {
            findings.push(HealthFinding::new(
                FindingCode::BackendCoverageIncomplete,
                Severity::CannotRepresent,
                Phase::Compile,
                Metric::BackendCoverageGapCount,
                MetricValue::Count(
                    report
                        .uncovered
                        .iter()
                        .filter(|item| {
                            matches!(item.grammar_health, crate::emit::EmissionHealth::Grammar(_))
                        })
                        .count()
                        .max(1) as u64,
                ),
                ValueProvenance::Observed,
                reason.clone(),
            ));
        }
    }
    findings
}

/// Every `crate::compose_budget::ComposeError` variant, exhaustively.
fn compose_error_finding(err: &ComposeError) -> HealthFinding {
    match err {
        ComposeError::CannotRepresent { reason } => HealthFinding::new(
            FindingCode::BackendCoverageIncomplete,
            Severity::CannotRepresent,
            Phase::Compile,
            Metric::BackendCoverageGapCount,
            MetricValue::Count(1),
            ValueProvenance::Observed,
            (*reason).to_owned(),
        ),
        ComposeError::ChainDepthExceeded { depth, limit, site } => HealthFinding::new(
            FindingCode::ResourceBudgetReached,
            Severity::NotProductionReady,
            Phase::Apply,
            Metric::ApplyChainDepth,
            MetricValue::Count(*depth as u64),
            ValueProvenance::Observed,
            format!(
                "Derivation/unapplication chain depth at {site:?} reached {depth} nested steps \
                 (limit {limit}); this deterministically closes the stack-overflow failure class \
                 (ADR 0003) instead of relying on a larger call stack."
            ),
        )
        .affecting(vec![(*site).to_string()])
        .against_threshold(MetricValue::Count(*limit as u64)),
    }
}

/// One `ApplyBudgetTrip` — see this module's "Judgment calls" item 6.
fn apply_budget_trip_finding(trip: &ApplyBudgetTrip) -> HealthFinding {
    let metric = match trip.dimension {
        ApplyDimension::DecodedPaths => Metric::ProposalPathCount,
        ApplyDimension::Candidates => Metric::ProposalCandidateCount,
    };
    HealthFinding::new(
        FindingCode::ResourceBudgetReached,
        Severity::NotProductionReady,
        Phase::Apply,
        metric,
        MetricValue::Count(trip.value as u64),
        ValueProvenance::Observed,
        format!(
            "Apply-time {label} reached {value} (limit {limit}) before this word completed; the \
             word is incomplete, never a definitive partial analysis -- other words in the same \
             batch remain valid and this word may be explicitly resubmitted with a larger apply \
             budget.",
            label = trip.dimension.label(),
            value = trip.value,
            limit = trip.limit,
        ),
    )
    .affecting(trip.word.iter().cloned().collect())
    .against_threshold(MetricValue::Count(trip.limit as u64))
    .with_remedies(vec![Remedy {
        rank: 1,
        description: "Explicitly retry this word alone with a larger caller-selected apply-time \
                budget."
            .to_string(),
        requires_linguistic_equivalence: false,
        caveat: None,
    }])
}

/// The non-empty set of phases one compile attempt actually reached. `starting_with` is the only
/// public constructor, so a caller must always name the first phase before it can build one at
/// all -- there is no `Default` and no empty constructor. The field stays private to this module
/// (not `pub(crate)`) so nothing outside `health_evaluator` can bypass `starting_with` either;
/// `evaluate`'s own doc explains why this is the fix for this module's historical defect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptedPhases(Vec<Phase>);

impl AttemptedPhases {
    /// The only way to start one: an attempt must name the first phase it reached.
    pub fn starting_with(first: Phase) -> Self {
        Self(vec![first])
    }

    /// Records one more phase this same attempt went on to reach.
    #[must_use]
    pub fn and(mut self, phase: Phase) -> Self {
        self.0.push(phase);
        self
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// What one compile attempt measured, assembled once and evaluated once by [`evaluate`]. Every
/// field but `phases` keeps the exact optional/empty-by-default shape the former four-parameter
/// `evaluate_health` always had; `phases` is the new, non-empty-by-construction fix.
pub struct CompileMeasurements<'a> {
    /// The phases this attempt actually reached (see [`AttemptedPhases`]).
    pub phases: AttemptedPhases,
    /// The final FST payload's byte count, if known.
    pub payload_bytes: Option<u64>,
    /// `crate::emit::emit`/`emit_with_budget`'s own `EmitReport`, if this compilation went
    /// through that path.
    pub emit_report: Option<&'a EmitReport>,
    /// Every `ComposeError` this compilation's checked compose/union/minimize/chain-depth calls
    /// raised (typically zero or one per grammar, but a caller collecting evidence across a batch
    /// or a diagnostic sweep may pass more than one).
    pub compose_errors: &'a [ComposeError],
    /// Every per-word `ApplyBudgetTrip` this compilation's callers observed.
    pub apply_budget_trips: &'a [ApplyBudgetTrip],
}

/// The evaluator: turns every available compile measurement into `HealthFinding`s and returns the
/// aggregated `HealthReport` — call `HealthReport::admission` on the result for the `FST
/// admission result` (unmodified, never re-derived here).
///
/// Replaces the former `evaluate_health(Option<u64>, Option<&EmitReport>, &[ComposeError],
/// &[ApplyBudgetTrip])`, callable with every argument absent to silently return an empty,
/// `WithinLimits` report indistinguishable from a genuinely clean compile.
/// [`CompileMeasurements`] makes that call unrepresentable outside this module: [`AttemptedPhases`]
/// has no empty/`Default` constructor. The `assert!` below is defense in depth against a
/// same-module bypass of that constructor (a private tuple field stays visible within its own
/// defining module) — pinned by `evaluate_panics_when_no_phase_was_ever_attempted`.
///
/// # Panics
/// If `measurements.phases` is empty, which only a same-module caller could even construct.
pub fn evaluate(measurements: CompileMeasurements) -> HealthReport {
    assert!(
        !measurements.phases.is_empty(),
        "CompileMeasurements must name at least one attempted phase; an all-absent report is \
         exactly the defect AttemptedPhases exists to make unrepresentable"
    );

    let mut findings = Vec::new();

    if let Some(bytes) = measurements.payload_bytes {
        findings.extend(payload_size_finding(bytes));
    }
    if let Some(report) = measurements.emit_report {
        findings.extend(emit_report_findings(report));
    }
    for err in measurements.compose_errors {
        findings.push(compose_error_finding(err));
    }
    for trip in measurements.apply_budget_trips {
        findings.push(apply_budget_trip_finding(trip));
    }
    HealthReport::new(findings)
}

/// Converts every typed Foma construction failure into nonempty backend-local health evidence.
pub fn evaluate_foma_error(error: &FomaError) -> HealthReport {
    match error {
        FomaError::ConstructionRefused(_) | FomaError::StrategyNotImplemented(_) => {
            HealthReport::new(vec![backend_compilation_failed_finding(error.to_string())])
        }
        FomaError::LexcCompileFailed(report) => {
            let mut health = evaluate(CompileMeasurements {
                phases: AttemptedPhases::starting_with(Phase::Compile),
                payload_bytes: None,
                emit_report: Some(report.as_ref()),
                compose_errors: &[],
                apply_budget_trips: &[],
            });
            health
                .findings
                .push(backend_compilation_failed_finding(format!(
                "The Foma backend could not compile the emitted lexc representation; no usable \
                 network was produced. Compiler detail: {error}"
            )));
            HealthReport::new(health.findings)
        }
        FomaError::Unsupported(report) => {
            let mut health = evaluate(CompileMeasurements {
                phases: AttemptedPhases::starting_with(Phase::Compile),
                payload_bytes: None,
                emit_report: Some(report.as_ref()),
                compose_errors: &[],
                apply_budget_trips: &[],
            });
            if health.findings.is_empty() {
                health
                    .findings
                    .push(backend_compilation_failed_finding(error.to_string()));
            }
            HealthReport::new(health.findings)
        }
        // No emission ran, so there is no `EmitReport` to evaluate; the refusal is already typed.
        FomaError::CapabilityRefused(diagnostics) => {
            let report = crate::backend_selection::BackendReport::refused(
                crate::enumerate::EmissionStrategy::TunedSurfaceProbed,
                crate::capability::CompileDecision::Refuse(diagnostics.clone()),
            )
            .expect("a refusing decision has a backend report");
            HealthReport::new(report.findings().to_vec())
        }
    }
}

#[cfg(test)]
mod tests;
