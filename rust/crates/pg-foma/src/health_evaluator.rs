//! The Rust health **evaluator**: reads `crate::compose_budget` measurements and produces
//! `crate::health::HealthFinding`s from them. `crate::health` owns the finding schema itself; this
//! module is the one place that reads real compile measurements and turns them into findings.
//!
//! # Scope: consume, never remeasure
//! Measurements come from the admission walker and budget tracker once; the health evaluator
//! consumes them without recomputation. This module reads only the measurement sources that exist
//! in this crate **today** — nothing here calls `foma`, walks a grammar, or measures anything
//! itself:
//! - **Payload size**: a plain `u64` byte count the caller already has (the emitted network /
//!   `pg-pack` payload), scored by `crate::health::severity_for_size_bytes`; oversized payloads
//!   remain readiness `NotProductionReady`, never `MachineLimit`/`CannotRepresent`.
//! - **`crate::emit::EmitReport`**: `tier`/`uncovered`, already produced by
//!   `crate::emit::emit`/`emit_with_budget`.
//! - **`crate::compose_budget::ComposeError`** (compile-time composition budget trips) and
//!   `ApplyBudgetTrip` (this module's own lightweight distillation of a per-word
//!   `crate::compose_budget::ApplyOutcome::Incomplete` — see that type's own doc for why it exists
//!   instead of taking `ApplyOutcome<T>` generically).
//!
//! # Two distinct axes, again (see `crate::health`'s own doc first)
//! Every `HealthFinding` this module builds carries `severity` on the cost/health axis only
//! (never a capability admission decision). This evaluator only reads compiler measurements.
//! `HealthReport::admission`
//! (unmodified, called as-is — never re-derived here) is what turns this report's findings into
//! the "FST admission result".
//!
//! # Judgment calls flagged for review
//! 1. **`crate::compose_budget::ComposeError::ChainDepthExceeded` maps to
//!    `FindingCode::ResourceBudgetReached` with `ValueProvenance::Observed`** because it is
//!    detected after a real recursion/unapplication step count is measured.
//! 2. **`crate::emit::FomaTier::Partial`'s `uncovered` count maps to
//!    `FindingCode::BackendCoverageIncomplete` at `Severity::CannotRepresent`**. This is observed
//!    semantic under-proposal, not uncertain cost: confirmation cannot manufacture a candidate
//!    that the proposer omitted. `ValueProvenance::Observed` (not `Predicted`) is used throughout
//!    this module's `FomaTier`-derived findings because the uncovered count is an exact, already-
//!    counted value, never a heuristic guess.
//! 4. **`crate::emit::FomaTier::Unsupported` maps to `FindingCode::UnknownUnboundedConstruct`
//!    at `Severity::NotProductionReady`**: this backend produced no usable network, while another
//!    backend may succeed. This is "any uncertainty that could omit an analysis fails closed" for
//!    one route, (total, not partial, coverage loss), not the ordinary bounded-cost-uncertainty
//!    shape the code otherwise names. `MetricValue::Unbounded` is used here (this compile's
//!    residual coverage is definitionally unknown, not a countable partial gap).
//! 5. **`ApplyBudgetTrip` is this module's own type, not `crate::compose_budget::ApplyOutcome<T>`
//!    directly**: `ApplyOutcome<T>`'s `Complete(T)` payload type varies by caller (e.g.
//!    `Vec<Candidate>`) and carries nothing this evaluator needs; making `evaluate_health` generic
//!    over `T` just to ignore `Complete`'s payload would cost every caller a type parameter for no
//!    benefit. Callers extract each `ApplyOutcome::Incomplete { dimension, value, limit }` into an
//!    `ApplyBudgetTrip` themselves — a direct field-for-field copy, not a recomputation.
//! 7. **All findings this module builds set `affected` from whatever stable identifier the source
//!    measurement already carries** (a compose-budget `site` label, an `UncoveredItem::id`, a rule
//!    XML id, an apply-time word) — never inventing a new identifier scheme; grammar-level findings
//!    with no specific construct identifier (e.g. a payload-size finding) leave `affected` empty.

use crate::analyzer::FomaError;
use crate::compose_budget::{ApplyDimension, ComposeError};
use crate::emit::{ClosureRefusalCode, EmitReport, FomaTier};
use crate::health::{
    severity_for_size_bytes, FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase,
    Remedy, Severity, ValueProvenance,
};
/// This evaluator's own distillation of one `crate::compose_budget::ApplyOutcome::Incomplete` —
/// see this module's "Judgment calls" item 6 for why `evaluate_health` takes this instead of the
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

/// `crate::emit::FomaTier::Partial`'s observed coverage gaps, which refuse normal generation.
fn partial_tier_finding(report: &EmitReport, uncovered_count: usize) -> HealthFinding {
    let affected: Vec<String> = report
        .uncovered
        .iter()
        .map(|item| item.id.clone())
        .collect();
    HealthFinding::new(
        FindingCode::BackendCoverageIncomplete,
        Severity::CannotRepresent,
        Phase::Compile,
        Metric::BackendCoverageGapCount,
        MetricValue::Count(uncovered_count as u64),
        ValueProvenance::Observed,
        format!(
            "{uncovered_count} construct occurrence(s) could not be represented in this \
             FST-propose network and contribute no candidates for it. Confirmation cannot restore \
             omitted candidates, so normal generation fails closed."
        ),
    )
    .affecting(affected)
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
    let value = report
        .closure_refusal
        .as_ref()
        .and_then(|refusal| refusal.pending_successors)
        .map(|pending| MetricValue::Count(pending as u64))
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
        .unwrap_or_default();
    // A depth-budget stop halted THIS attempt; every other cause is a coverage gap in the grammar.
    let depth_budget_stop = matches!(
        report.closure_refusal.as_ref().map(|refusal| refusal.code),
        Some(ClosureRefusalCode::DepthBudgetExceeded)
    );
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
        FomaTier::Partial { uncovered } => {
            findings.push(partial_tier_finding(report, *uncovered));
        }
        FomaTier::Unsupported { reason } => {
            findings.push(unsupported_tier_finding(report, reason));
        }
    }
    findings
}

/// Every `crate::compose_budget::ComposeError` variant, exhaustively.
fn compose_error_finding(err: &ComposeError) -> HealthFinding {
    match err {
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

/// The evaluator: turns every
/// available compile measurement into `HealthFinding`s and returns the aggregated
/// `HealthReport` — call `HealthReport::admission` on the result for the `FST
/// admission result` (unmodified, never re-derived here).
///
/// Every parameter is optional/empty-by-default so a caller with only some measurements (e.g. just
/// a payload size, no compose-budget trips) still gets a valid report — this module's own
/// `fst_health_evaluator_empty_report_is_ideal` test pins the all-`None`/all-empty case.
///
/// - `payload_bytes`: the final FST payload's byte count, if known.
/// - `emit_report`: `crate::emit::emit`/`emit_with_budget`'s own `EmitReport`, if this
///   compilation went through that path.
/// - `compose_errors`: every `ComposeError` this compilation's checked compose/union/minimize/
///   chain-depth calls raised (typically zero or one per grammar, but a
///   caller collecting evidence across a batch or a diagnostic sweep may pass more than one).
/// - `apply_budget_trips`: every per-word `ApplyBudgetTrip` this compilation's callers observed.
pub fn evaluate_health(
    payload_bytes: Option<u64>,
    emit_report: Option<&EmitReport>,
    compose_errors: &[ComposeError],
    apply_budget_trips: &[ApplyBudgetTrip],
) -> HealthReport {
    let mut findings = Vec::new();

    if let Some(bytes) = payload_bytes {
        findings.extend(payload_size_finding(bytes));
    }
    if let Some(report) = emit_report {
        findings.extend(emit_report_findings(report));
    }
    for err in compose_errors {
        findings.push(compose_error_finding(err));
    }
    for trip in apply_budget_trips {
        findings.push(apply_budget_trip_finding(trip));
    }
    HealthReport::new(findings)
}

/// Converts every typed Foma construction failure into nonempty backend-local health evidence.
pub fn evaluate_foma_error(error: &FomaError) -> HealthReport {
    match error {
        FomaError::LexcCompileFailed(report) => {
            let mut health = evaluate_health(None, Some(report), &[], &[]);
            health
                .findings
                .push(backend_compilation_failed_finding(format!(
                "The Foma backend could not compile the emitted lexc representation; no usable \
                 network was produced. Compiler detail: {error}"
            )));
            HealthReport::new(health.findings)
        }
        FomaError::Unsupported(report) | FomaError::Incomplete(report) => {
            let mut health = evaluate_health(None, Some(report), &[], &[]);
            if health.findings.is_empty() {
                health
                    .findings
                    .push(backend_compilation_failed_finding(error.to_string()));
            }
            HealthReport::new(health.findings)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::{
        ClosureFallbackBackend, ClosureRefusal, ClosureRefusalCode, EmitCounts, UncoveredItem,
    };
    use crate::health::FindingClass;
    use std::time::Duration;

    fn synthetic_full_emit_report() -> EmitReport {
        EmitReport {
            uncovered: Vec::new(),
            counts: EmitCounts::default(),
            tier: FomaTier::Full,
            enum_budget_exceeded: None,
            closure_refusal: None,
            closure_evidence: None,
        }
    }

    #[test]
    fn fst_health_evaluator_every_foma_error_is_nonempty_backend_error() {
        let cases = vec![
            FomaError::LexcCompileFailed(synthetic_full_emit_report()),
            FomaError::Unsupported(EmitReport {
                tier: FomaTier::Unsupported {
                    reason: "synthetic unsupported route".to_string(),
                },
                ..synthetic_full_emit_report()
            }),
        ];

        // Every error must BLOCK; the exact band is per-cause, pinned by the split tests below.
        for error in cases {
            let health = evaluate_foma_error(&error);
            assert!(!health.findings.is_empty(), "empty health for {error}");
            assert!(
                health.admission() >= Severity::NotProductionReady,
                "health for {error} must block publication, got {:?}",
                health.admission()
            );
        }
    }

    #[test]
    fn fst_health_evaluator_lexc_failure_is_explicit_even_with_full_emit_report() {
        let health =
            evaluate_foma_error(&FomaError::LexcCompileFailed(synthetic_full_emit_report()));
        assert!(health
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::BackendCompilationFailed));
    }

    #[test]
    fn fst_health_evaluator_backend_local_budget_failures_are_errors() {
        let compose_errors = vec![ComposeError::ChainDepthExceeded {
            depth: 2,
            limit: 1,
            site: "apply",
        }];
        for error in compose_errors {
            let health = evaluate_health(None, None, &[error], &[]);
            assert_eq!(health.admission(), Severity::NotProductionReady);
        }

        let trip = ApplyBudgetTrip {
            dimension: ApplyDimension::Candidates,
            value: 2,
            limit: 1,
            word: Some("word".to_string()),
        };
        assert_eq!(
            evaluate_health(None, None, &[], &[trip]).admission(),
            Severity::NotProductionReady
        );
    }

    // fst_health_evaluator_size_bands: payload-size-only inputs, the single threshold.

    #[test]
    fn fst_health_evaluator_within_limits_payload_produces_no_finding() {
        let report = evaluate_health(Some(crate::health::IDEAL_MAX_BYTES), None, &[], &[]);
        assert!(report.findings.is_empty());
        assert_eq!(report.admission(), Severity::WithinLimits);
    }

    #[test]
    fn fst_health_evaluator_over_ideal_payload_produces_not_production_ready_payload_size_band_finding(
    ) {
        let bytes = 500_000_000u64;
        let report = evaluate_health(Some(bytes), None, &[], &[]);
        assert_eq!(report.findings.len(), 1);
        let finding = &report.findings[0];
        assert_eq!(finding.code, FindingCode::PayloadSizeBand);
        assert_eq!(finding.severity, Severity::NotProductionReady);
        assert_eq!(finding.metric, Metric::PayloadBytes);
        assert_eq!(finding.value, MetricValue::Bytes(bytes));
        assert_eq!(
            finding.threshold,
            Some(MetricValue::Bytes(crate::health::IDEAL_MAX_BYTES))
        );
        assert_eq!(finding.provenance, ValueProvenance::Observed);
        assert_eq!(report.admission(), Severity::NotProductionReady);
    }

    #[test]
    fn fst_health_evaluator_not_production_ready_payload_matches_health_schema_worked_scenario() {
        // `IDEAL_MAX_BYTES` is the only size threshold; one byte more crosses into NotProductionReady.
        let report = evaluate_health(Some(crate::health::IDEAL_MAX_BYTES + 1), None, &[], &[]);
        assert_eq!(report.findings[0].severity, Severity::NotProductionReady);
        assert_eq!(
            report.findings[0].threshold,
            Some(MetricValue::Bytes(crate::health::IDEAL_MAX_BYTES))
        );
    }

    #[test]
    fn fst_health_evaluator_oversized_payload_remains_not_production_ready_readiness() {
        let report = evaluate_health(Some(10_000_000_000u64), None, &[], &[]);
        assert_eq!(report.findings[0].severity, Severity::NotProductionReady);
        assert_eq!(report.admission(), Severity::NotProductionReady);
    }

    // fst_health_evaluator_emit_report: FomaTier + enum-budget-exceeded mapping.

    #[test]
    fn fst_health_evaluator_full_tier_produces_no_finding() {
        let report = EmitReport {
            uncovered: Vec::new(),
            counts: EmitCounts::default(),
            tier: FomaTier::Full,
            enum_budget_exceeded: None,
            closure_refusal: None,
            closure_evidence: None,
        };
        let health = evaluate_health(None, Some(&report), &[], &[]);
        assert!(health.findings.is_empty());
        assert_eq!(health.admission(), Severity::WithinLimits);
    }

    #[test]
    fn fst_health_evaluator_partial_tier_is_cannot_represent_coverage_gap() {
        let uncovered = vec![
            UncoveredItem {
                kind: "infix".to_string(),
                id: "mrule12#allo0".to_string(),
                reason: "synthetic interdigitation not representable".to_string(),
            },
            UncoveredItem {
                kind: "process-morph".to_string(),
                id: "mrule13#allo0".to_string(),
                reason: "synthetic process morph not representable".to_string(),
            },
        ];
        let report = EmitReport {
            uncovered: uncovered.clone(),
            counts: EmitCounts::default(),
            tier: FomaTier::Partial { uncovered: 2 },
            enum_budget_exceeded: None,
            closure_refusal: None,
            closure_evidence: None,
        };
        let health = evaluate_health(None, Some(&report), &[], &[]);
        assert_eq!(health.findings.len(), 1);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::BackendCoverageIncomplete);
        assert_eq!(finding.severity, Severity::CannotRepresent);
        assert_eq!(finding.metric, Metric::BackendCoverageGapCount);
        assert_eq!(finding.value, MetricValue::Count(2));
        assert_eq!(
            finding.affected,
            vec!["mrule12#allo0".to_string(), "mrule13#allo0".to_string()]
        );
        assert_eq!(health.admission(), Severity::CannotRepresent);
    }

    #[test]
    fn fst_health_evaluator_unsupported_tier_with_no_closure_refusal_is_cannot_represent() {
        let report = EmitReport {
            uncovered: Vec::new(),
            counts: EmitCounts::default(),
            tier: FomaTier::Unsupported {
                reason: "zero root allomorphs survived synthetic pre-filtering".to_string(),
            },
            enum_budget_exceeded: None,
            closure_refusal: None,
            closure_evidence: None,
        };
        let health = evaluate_health(None, Some(&report), &[], &[]);
        assert_eq!(health.findings.len(), 1);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::BackendCoverageIncomplete);
        assert_eq!(finding.severity, Severity::CannotRepresent);
        assert_eq!(finding.value, MetricValue::Unbounded);
        assert_eq!(health.admission(), Severity::CannotRepresent);
    }

    /// A `BackendCoverageIncomplete` finding is representability evidence, so it must always carry `CannotRepresent`, never `MachineLimit`.
    #[test]
    fn representability_never_reports_as_machine_limit() {
        let partial = EmitReport {
            uncovered: vec![UncoveredItem {
                kind: "infix".to_string(),
                id: "mrule1#allo0".to_string(),
                reason: "synthetic".to_string(),
            }],
            counts: EmitCounts::default(),
            tier: FomaTier::Partial { uncovered: 1 },
            enum_budget_exceeded: None,
            closure_refusal: None,
            closure_evidence: None,
        };
        let unsupported = EmitReport {
            uncovered: Vec::new(),
            counts: EmitCounts::default(),
            tier: FomaTier::Unsupported {
                reason: "synthetic total coverage loss".to_string(),
            },
            enum_budget_exceeded: None,
            closure_refusal: None,
            closure_evidence: None,
        };

        for report in [&partial, &unsupported] {
            let health = evaluate_health(None, Some(report), &[], &[]);
            let coverage_findings: Vec<_> = health
                .findings
                .iter()
                .filter(|f| f.code == FindingCode::BackendCoverageIncomplete)
                .collect();
            assert!(
                !coverage_findings.is_empty(),
                "expected a BackendCoverageIncomplete finding for {report:?}"
            );
            for finding in coverage_findings {
                assert_eq!(
                    finding.severity,
                    Severity::CannotRepresent,
                    "a BackendCoverageIncomplete finding must be CannotRepresent: {finding:?}"
                );
                assert_ne!(
                    finding.severity,
                    Severity::MachineLimit,
                    "representability must never report as MachineLimit: {finding:?}"
                );
            }
        }
    }

    #[test]
    fn fst_health_evaluator_unbounded_rule_application_is_cannot_represent() {
        let report = EmitReport {
            uncovered: Vec::new(),
            counts: EmitCounts::default(),
            tier: FomaTier::Unsupported {
                reason: "a participating rule has no authored finite bound".to_string(),
            },
            enum_budget_exceeded: None,
            closure_refusal: Some(ClosureRefusal {
                code: ClosureRefusalCode::UnboundedRuleApplication,
                affected_rule_ordinals: vec![7],
                depth_limit: None,
                pending_successors: None,
                remedy_backend: ClosureFallbackBackend::FullMorphologicalParser,
            }),
            closure_evidence: None,
        };
        let health = evaluate_health(None, Some(&report), &[], &[]);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::BackendCoverageIncomplete);
        assert_eq!(finding.severity, Severity::CannotRepresent);
        assert_eq!(finding.class(), FindingClass::Representability);
    }

    /// An artificial cap is never a representability verdict, however deep it stopped.
    #[test]
    fn fst_health_evaluator_depth_budget_stop_is_containment_not_cannot_represent() {
        let report = EmitReport {
            uncovered: Vec::new(),
            counts: EmitCounts::default(),
            tier: FomaTier::Unsupported {
                reason: "closure depth budget exceeded".to_string(),
            },
            enum_budget_exceeded: None,
            closure_refusal: Some(ClosureRefusal {
                code: ClosureRefusalCode::DepthBudgetExceeded,
                affected_rule_ordinals: vec![3],
                depth_limit: Some(16),
                pending_successors: Some(5),
                remedy_backend: ClosureFallbackBackend::FullMorphologicalParser,
            }),
            closure_evidence: None,
        };
        let health = evaluate_health(None, Some(&report), &[], &[]);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::ResourceBudgetReached);
        assert_eq!(finding.severity, Severity::NotProductionReady);
        assert_eq!(finding.class(), FindingClass::Containment);
        assert_ne!(
            finding.severity,
            Severity::MachineLimit,
            "a depth-budget stop halted one attempt; it must never condemn the grammar"
        );
    }

    #[test]
    fn fst_health_evaluator_preserves_closure_refusal_cause() {
        let report = EmitReport {
            uncovered: Vec::new(),
            counts: EmitCounts::default(),
            tier: FomaTier::Unsupported {
                reason: "synthetic incomplete closure".to_string(),
            },
            enum_budget_exceeded: None,
            closure_refusal: Some(ClosureRefusal {
                code: ClosureRefusalCode::DepthBudgetExceeded,
                affected_rule_ordinals: vec![3, 7],
                depth_limit: Some(64),
                pending_successors: Some(11),
                remedy_backend: ClosureFallbackBackend::FullMorphologicalParser,
            }),
            closure_evidence: None,
        };
        let health = evaluate_health(None, Some(&report), &[], &[]);
        let finding = &health.findings[0];
        assert_eq!(finding.affected, vec!["mrule3", "mrule7"]);
        assert_eq!(finding.value, MetricValue::Count(11));
        assert!(finding.explanation.contains("closure-depth limit was 64"));
    }

    // fst_health_evaluator_compose_errors: every ComposeError variant maps to a finding.

    #[test]
    fn fst_health_evaluator_chain_depth_exceeded_is_apply_phase() {
        let err = ComposeError::ChainDepthExceeded {
            depth: 30,
            limit: 24,
            site: "synthetic-peel-site",
        };
        let health = evaluate_health(None, None, std::slice::from_ref(&err), &[]);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::ResourceBudgetReached);
        assert_eq!(finding.metric, Metric::ApplyChainDepth);
        assert_eq!(finding.phase, Phase::Apply);
    }

    // fst_health_evaluator_apply_budget_trips

    #[test]
    fn fst_health_evaluator_apply_budget_trip_decoded_paths() {
        let trip = ApplyBudgetTrip {
            dimension: ApplyDimension::DecodedPaths,
            value: 10_001,
            limit: 10_000,
            word: Some("synthetic-word".to_string()),
        };
        let health = evaluate_health(None, None, &[], std::slice::from_ref(&trip));
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::ResourceBudgetReached);
        assert_eq!(finding.metric, Metric::ProposalPathCount);
        assert_eq!(finding.phase, Phase::Apply);
        assert_eq!(finding.affected, vec!["synthetic-word".to_string()]);
    }

    #[test]
    fn fst_health_evaluator_apply_budget_trip_candidates() {
        let trip = ApplyBudgetTrip {
            dimension: ApplyDimension::Candidates,
            value: 501,
            limit: 500,
            word: None,
        };
        let health = evaluate_health(None, None, &[], std::slice::from_ref(&trip));
        let finding = &health.findings[0];
        assert_eq!(finding.metric, Metric::ProposalCandidateCount);
        assert!(finding.affected.is_empty());
    }

    // fst_health_evaluator_empty_report_is_within_limits

    #[test]
    fn fst_health_evaluator_empty_report_is_within_limits() {
        let health = evaluate_health(None, None, &[], &[]);
        assert!(health.findings.is_empty());
        assert_eq!(health.admission(), Severity::WithinLimits);
        assert_eq!(health.schema_version, crate::health::HEALTH_SCHEMA_VERSION);
    }

    // fst_health_evaluator_golden: a representative multi-source compile, byte-for-byte golden.

    /// Two distinct measurement sources (payload size and an emit report) feeding one report, the shape a real caller assembles.
    fn representative_inputs() -> (u64, EmitReport) {
        let payload_bytes = 250_000_000u64; // NotProductionReady: over IDEAL_MAX_BYTES
        let emit_report = EmitReport {
            uncovered: vec![UncoveredItem {
                kind: "process-morph".to_string(),
                id: "mrule0007#allo0".to_string(),
                reason: "synthetic non-concatenative process morph".to_string(),
            }],
            counts: EmitCounts::default(),
            tier: FomaTier::Partial { uncovered: 1 },
            enum_budget_exceeded: None,
            closure_refusal: None,
            closure_evidence: None,
        };
        (payload_bytes, emit_report)
    }

    const GOLDEN_JSON: &str = r#"{
  "schema_version": 7,
  "findings": [
    {
      "code": "PGF0001",
      "severity": "not_production_ready",
      "phase": "compile",
      "affected": [],
      "metric": "payload_bytes",
      "value": {
        "kind": "bytes",
        "value": 250000000
      },
      "provenance": "observed",
      "threshold": {
        "kind": "bytes",
        "value": 100000000
      },
      "explanation": "Final FST payload is 250000000 bytes, in the NotProductionReady band (R6 decimal-byte size thresholds).",
      "remedies": []
    },
    {
      "code": "PGF0013",
      "severity": "cannot_represent",
      "phase": "compile",
      "affected": [
        "mrule0007#allo0"
      ],
      "metric": "backend_coverage_gap_count",
      "value": {
        "kind": "count",
        "value": 1
      },
      "provenance": "observed",
      "explanation": "1 construct occurrence(s) could not be represented in this FST-propose network and contribute no candidates for it. Confirmation cannot restore omitted candidates, so normal generation fails closed.",
      "remedies": []
    }
  ]
}"#;

    #[test]
    fn fst_health_evaluator_golden_json() {
        let (payload_bytes, emit_report) = representative_inputs();
        let health = evaluate_health(Some(payload_bytes), Some(&emit_report), &[], &[]);
        let json = health.to_json().expect("serialization must succeed");
        assert_eq!(
            json, GOLDEN_JSON,
            "canonical JSON drifted from the committed golden"
        );
    }

    #[test]
    fn fst_health_evaluator_golden_admission_is_cannot_represent() {
        let (payload_bytes, emit_report) = representative_inputs();
        let health = evaluate_health(Some(payload_bytes), Some(&emit_report), &[], &[]);
        // An uncovered construct is CannotRepresent even when resource findings have lower severity.
        assert_eq!(health.admission(), Severity::CannotRepresent);
    }

    #[test]
    fn fst_health_evaluator_golden_round_trips() {
        let (payload_bytes, emit_report) = representative_inputs();
        let health = evaluate_health(Some(payload_bytes), Some(&emit_report), &[], &[]);
        let json = health.to_json().expect("serialization must succeed");
        let parsed = HealthReport::from_json(&json).expect("deserialization must succeed");
        assert_eq!(
            parsed, health,
            "round trip through canonical JSON must be lossless"
        );
    }
}
