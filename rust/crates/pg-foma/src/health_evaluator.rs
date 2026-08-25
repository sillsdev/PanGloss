//! The Rust health **evaluator**: reads `crate::compose_budget`/
//! `crate::morphotactics::EnumerationBudget` measurements and produces
//! `crate::health::HealthFinding`s from them. `crate::health` owns the finding schema itself; this
//! module is the one place that reads real compile measurements and turns them into findings.
//!
//! # Scope: consume, never remeasure
//! Measurements come from the admission
//! walker, budget tracker, and compile profile once; the health evaluator consumes them without
//! recomputation. This module reads exactly four measurement sources that exist in this crate
//! **today** — nothing here calls `foma`, walks a grammar, or measures anything itself:
//! - **Payload size**: a plain `u64` byte count the caller already has (the emitted network /
//!   `pg-pack` payload), scored by `crate::health::severity_for_size_bytes`; oversized payloads
//!   remain readiness `NotProductionReady`, never `MachineLimit`/`CannotRepresent`.
//! - **`crate::emit::EmitReport`**: `tier`/`uncovered`/`enum_budget_exceeded`, already produced
//!   by `crate::emit::emit`/`emit_with_budget`.
//! - **`crate::compose_budget::ComposeError`** (compile-time composition budget trips) and
//!   `ApplyBudgetTrip` (this module's own lightweight distillation of a per-word
//!   `crate::compose_budget::ApplyOutcome::Incomplete` — see that type's own doc for why it exists
//!   instead of taking `ApplyOutcome<T>` generically).
//! - **`crate::profile::CompileProfile`**: `profile_findings`
//!   reads its final compiled-network state/arc counts and total emitted-line count to produce the
//!   two *approaching-but-not-yet-tripped* finding kinds this crate's compile-time-series
//!   instrumentation supports.
//!
//! `profile_findings` populates `crate::health::FindingCode::IntermediateNetworkGrowth` (the
//! production network's own final state/arc count approaching, but not tripping,
//! `crate::compose_budget::DEFAULT_STATE_BUDGET`/`DEFAULT_ARC_BUDGET` — reused as the closest
//! existing calibrated size dimension; see `profile_findings`'s own doc for why the production
//! path has no earlier "intermediate" composition product to measure instead) and
//! `crate::health::FindingCode::CompileWorkBudget` (total emitted lexc lines approaching, but not
//! tripping, `crate::compose_budget::DEFAULT_LINE_BUDGET` — a dimension the production path does
//! not even check today, unlike the experimental `emit_underlying_templated`/`crate::uflexc`
//! paths' own incremental `line_cap` check).
//!
//! Not populated here (observed audit fields populate only as their owning profile/budget
//! instrumentation exists, and are never independently remeasured):
//! `crate::health::FindingCode::ApplicationTimeWork`'s
//! `crate::health::Metric::ElapsedMillis`/`crate::health::Metric::ApplyAllocationBytes`
//! dimensions (no per-word wall-clock/allocation instrumentation exists yet, only the two
//! magnitude caps `ApplyBudgetTrip` already covers — `profile-fst-compilation` is a COMPILE-time
//! profile, this dimension is per-word APPLY-time, a different measurement surface entirely);
//! `crate::health::FindingCode::DuplicateAnalysisOverlap` (needs `crate::confirm`'s pre-dedup
//! counts, not produced anywhere yet); and `crate::health::FindingCode::ProposalVolume`/
//! `crate::health::FindingCode::ConfirmationWork` for *large-but-not-tripped* candidate/
//! confirmation volume (only the tripped case, via `ApplyBudgetTrip`, is evaluated here — see
//! this module's "Judgment calls" section, item 6; also apply-time, not compile-time). Every one of
//! these finding kinds is fully *producible* by this evaluator's own shape (the `match` arms below
//! are exhaustive over `crate::compose_budget::ComposeError`/`crate::emit::FomaTier`) but stays
//! unpopulated until its owning profile/budget change lands real values to read.
//!
//! # Two distinct axes, again (see `crate::health`'s own doc first)
//! Every `HealthFinding` this module builds carries `severity` on the cost/health axis only
//! (never the capability-trust axis). Capability trust is recorded separately at pack-manifest
//! level; this evaluator only reads compiler measurements. `HealthReport::admission`
//! (unmodified, called as-is — never re-derived here) is what turns this report's findings into
//! the "FST admission result".
//!
//! # Judgment calls flagged for review
//! 1. **`crate::compose_budget::ComposeError` variants split into two [`crate::health::
//!    FindingCode`]s by *when* the check runs, not by variant name alone**: `AlphaTupleBudgetExceeded`/
//!    `GroupBudgetExceeded`/`OrderingMultiplicityExceeded` are checked BEFORE the expensive
//!    operation they would gate even starts (`compose_budget.rs`'s own doc, verbatim, for all
//!    three: "checked BEFORE..."), on an exact, already-known count — a proven work bound — so
//!    they map to `FindingCode::ProvenBoundExceedsBudget` with
//!    `ValueProvenance::ProvenBound`. `NetSizeExceeded`/`EmitLineBudgetExceeded`/
//!    `ComposeStepTimedOut`/`ChainDepthExceeded` are only detected AFTER the checked operation (an
//!    actual compose/union/minimize call, an actual emission run, an actual wall-clock wait, an
//!    actual recursion) already executed and produced/consumed a measured value, so they map to
//!    `FindingCode::ResourceBudgetReached` with `ValueProvenance::Observed`.
//! 2. **`crate::health::Metric::OrderingRuleCount` is a new variant this change appends** to
//!    `crate::health`'s `Metric` enum (see that enum's own doc on the variant) — the only schema
//!    edit this evaluator makes: an appended variant, with no renumbering, no removal, no change to
//!    any existing golden JSON.
//! 3. **`crate::emit::FomaTier::Partial`'s `uncovered` count maps to
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
//! 5. **`crate::emit::EnumBudgetExceeded`'s free-form `measure: &'static str` label has no
//!    dedicated `Metric`** (it names one of several different eager-enumeration measures --
//!    `crate::morphotactics::EnumMeasure`'s own label set -- not one fixed quantity); this evaluator
//!    reuses `Metric::UnknownUnboundedWork` (the closest existing "unbounded compile-time-work"
//!    slot) and folds the exact label into the finding's `explanation` text, since `Metric` itself
//!    cannot carry a free-form label.
//! 6. **`ApplyBudgetTrip` is this module's own type, not `crate::compose_budget::ApplyOutcome<T>`
//!    directly**: `ApplyOutcome<T>`'s `Complete(T)` payload type varies by caller (e.g.
//!    `Vec<Candidate>`) and carries nothing this evaluator needs; making `evaluate_health` generic
//!    over `T` just to ignore `Complete`'s payload would cost every caller a type parameter for no
//!    benefit. Callers extract each `ApplyOutcome::Incomplete { dimension, value, limit }` into an
//!    `ApplyBudgetTrip` themselves — a direct field-for-field copy, not a recomputation.
//! 7. **All findings this module builds set `affected` from whatever stable identifier the source
//!    measurement already carries** (a compose-budget `site` label, an `UncoveredItem::id`, a rule
//!    XML id, an apply-time word) — never inventing a new identifier scheme; grammar-level findings
//!    with no specific construct identifier (e.g. a payload-size finding) leave `affected` empty.
//! 8. **`profile_findings` reuses `Metric::IntermediateStateCount`/`Metric::IntermediateArcCount`
//!    for the PRODUCTION path's own FINAL compiled network**, not a mid-cascade intermediate
//!    composition product — `crate::emit::emit_with_budget`'s production path pre-bakes
//!    phonology into emitted surface forms, so replacement-rule nets do not exist there: it
//!    performs no separate `fsm_compose`/`fsm_union`/`fsm_minimize` call at all;
//!    the single `foma::lexcread::fsm_lexc_parse_string` call IS the only network-construction step,
//!    so its own state/arc count is simultaneously the "final" and the only "intermediate" product
//!    available. This reuses the existing `Metric`/`MetricValue`
//!    vocabulary rather than inventing a parallel one; the finding's own `explanation` text says so
//!    explicitly, so a report reader is never misled into expecting a future per-rule
//!    cascade curve from this kind of report.
//! 9. **A single flat 80%-of-budget threshold, not a banded severity scale**
//!    (`APPROACHING_BUDGET_WARNING_FRACTION`) — no real large-grammar measurement of a legitimate
//!    "approaching" curve exists yet for either dimension (mirrors `crate::compose_budget`'s own
//!    "conservative placeholder pending real-grammar measurement" convention for its calibrated
//!    defaults); always `Severity::LargeMultiplier`, never escalated further by this evaluator, because an
//!    ACTUAL trip of the same dimension is a completely different, already-handled code path
//!    (`compose_error_finding`'s `FindingCode::ResourceBudgetReached`/
//!    `FindingCode::ProvenBoundExceedsBudget` arms) that this function never reaches (the
//!    production path has no compose-budget-checked call site at all, module doc).
//! 10. **A non-`crate::profile::ProfileLabel::Production` profile is refused outright**
//!     (empty `Vec`, never a partial fold), pinned by
//!     `fst_health_evaluator_experimental_composition_profile_is_refused`. `evaluate_health`
//!     never even needs to check this itself; `profile_findings` is the one and only place this
//!     gate is enforced.

use crate::analyzer::FomaError;
use crate::compose_budget::{
    ApplyDimension, ComposeError, NetSizeMeasure, DEFAULT_ARC_BUDGET, DEFAULT_LINE_BUDGET,
    DEFAULT_STATE_BUDGET,
};
use crate::emit::{ClosureRefusalCode, EmitReport, EnumBudgetExceeded, FomaTier};
use crate::health::{
    severity_for_size_bytes, FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase,
    Remedy, Severity, ValueProvenance,
};
use crate::profile::{CompileProfile, ProfileLabel};

/// Fraction of a calibrated compose-budget dimension at or above which `profile_findings` raises an "approaching, not yet tripped" `Severity::LargeMultiplier`; a flat threshold, not a banded scale.
const APPROACHING_BUDGET_WARNING_FRACTION: f64 = 0.8;

/// One "approaching, not yet tripped" `Severity::LargeMultiplier` finding, or `None` below threshold; shared by every `profile_findings` dimension so the policy lives in one place.
fn approaching_budget_finding(
    code: FindingCode,
    metric: Metric,
    value: u64,
    limit: u64,
    explanation: String,
) -> Option<HealthFinding> {
    if limit == 0 || (value as f64) < APPROACHING_BUDGET_WARNING_FRACTION * (limit as f64) {
        return None;
    }
    Some(HealthFinding {
        code,
        severity: Severity::LargeMultiplier,
        phase: Phase::Compile,
        affected: Vec::new(),
        metric,
        value: MetricValue::Count(value),
        provenance: ValueProvenance::Observed,
        threshold: Some(MetricValue::Count(limit)),
        explanation,
        remedies: Vec::new(),
    })
}

/// `crate::profile::CompileProfile`-sourced findings: `FindingCode::IntermediateNetworkGrowth`
/// from the production network's final state/arc count approaching (but not tripping)
/// `DEFAULT_STATE_BUDGET`/`DEFAULT_ARC_BUDGET`, and `FindingCode::CompileWorkBudget` from the
/// total emitted lexc line count approaching (but not tripping) `DEFAULT_LINE_BUDGET` — see this
/// module's "Judgment calls" items 8/9 for the reuse/threshold rationale.
///
/// The production-only gate (this module's "Judgment calls" item 10): a `profile.label !=
/// crate::profile::ProfileLabel::Production` is refused outright, returning an empty `Vec` — never
/// partially folded in as production evidence.
pub fn profile_findings(profile: &CompileProfile) -> Vec<HealthFinding> {
    if profile.label != ProfileLabel::Production {
        return Vec::new();
    }
    let mut findings = Vec::new();

    if let Some(states) = profile.final_state_count.filter(|&v| v >= 0) {
        let states = states as u64;
        let limit = DEFAULT_STATE_BUDGET as u64;
        findings.extend(approaching_budget_finding(
            FindingCode::IntermediateNetworkGrowth,
            Metric::IntermediateStateCount,
            states,
            limit,
            format!(
                "This grammar's compiled production network has {states} states, at or above \
                 {pct:.0}% of the {limit}-state compose-budget reference band \
                 (crate::compose_budget::DEFAULT_STATE_BUDGET). Phase A's production path \
                 (surface-prebaked emit_with_budget -> a single fsm_lexc_parse_string call) \
                 performs no separate composition/union/minimize fold of its own -- this is the \
                 compiled network's own final size, reused against the closest existing calibrated \
                 size dimension rather than a mid-cascade intermediate product (this evaluator's own \
                 \"Judgment calls\" item 8).",
                pct = APPROACHING_BUDGET_WARNING_FRACTION * 100.0,
            ),
        ));
    }
    if let Some(arcs) = profile.final_arc_count.filter(|&v| v >= 0) {
        let arcs = arcs as u64;
        let limit = DEFAULT_ARC_BUDGET as u64;
        findings.extend(approaching_budget_finding(
            FindingCode::IntermediateNetworkGrowth,
            Metric::IntermediateArcCount,
            arcs,
            limit,
            format!(
                "This grammar's compiled production network has {arcs} arcs, at or above \
                 {pct:.0}% of the {limit}-arc compose-budget reference band \
                 (crate::compose_budget::DEFAULT_ARC_BUDGET). Same Phase A caveat as the \
                 state-count finding.",
                pct = APPROACHING_BUDGET_WARNING_FRACTION * 100.0,
            ),
        ));
    }
    if let Some(lines) = profile.total_lexc_lines {
        let limit = DEFAULT_LINE_BUDGET as u64;
        findings.extend(approaching_budget_finding(
            FindingCode::CompileWorkBudget,
            Metric::EmittedLineCount,
            lines,
            limit,
            format!(
                "This grammar's production emission wrote {lines} lexc lines, at or above \
                 {pct:.0}% of the {limit}-line compose-budget reference band \
                 (crate::compose_budget::DEFAULT_LINE_BUDGET) -- a dimension Phase A's production \
                 path does not itself check (only the experimental emit_underlying_templated/\
                 crate::uflexc paths do), so this is diagnostic evidence, never a resource-budget \
                 trip.",
                pct = APPROACHING_BUDGET_WARNING_FRACTION * 100.0,
            ),
        ));
    }

    findings
}

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

/// Shared remedy for every budget-tripped finding: fall back to the full engine, or raise the tripped budget's env var. Never a grammar edit (`requires_linguistic_equivalence: false`).
fn retry_full_engine_remedy() -> Remedy {
    Remedy {
        rank: 1,
        description: "Use the default (full) morphological-parser engine for this grammar \
            instead of the FST-propose/composition path, or raise the specific tripped budget's \
            own env var only if you understand why this grammar's composition is this large, and \
            re-run."
            .to_string(),
        requires_linguistic_equivalence: false,
        caveat: None,
    }
}

/// Remedy for an artificial-cap stop: remove the internal caps rather than raise them, since a bigger arbitrary number is still arbitrary.
fn retry_with_internal_caps_removed_remedy() -> Remedy {
    Remedy {
        rank: 0,
        description: "Re-run with the internal size/work caps removed so the only remaining \
            bound is machine containment. The attempt stopped at an artificial cap, not a proven \
            limit, so a fresh characterization with the caps removed may complete; the result is \
            developer evidence and is not production-publishable."
            .to_string(),
        requires_linguistic_equivalence: false,
        caveat: None,
    }
}

/// The threshold a non-`Severity::WithinLimits` size finding crossed -- read from the shared `IDEAL_MAX_BYTES` constant so a threshold change cannot desync a second copy.
fn size_band_crossed_threshold(severity: Severity) -> MetricValue {
    match severity {
        Severity::WithinLimits => {
            unreachable!("payload_size_finding filters Severity::WithinLimits before calling this")
        }
        Severity::NotProductionReady => MetricValue::Bytes(crate::health::IDEAL_MAX_BYTES),
        Severity::Elevated | Severity::LargeMultiplier | Severity::MachineLimit | Severity::CannotRepresent => {
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
    Some(HealthFinding {
        code: FindingCode::PayloadSizeBand,
        severity,
        phase: Phase::Compile,
        affected: Vec::new(),
        metric: Metric::PayloadBytes,
        value: MetricValue::Bytes(bytes),
        provenance: ValueProvenance::Observed,
        threshold: Some(size_band_crossed_threshold(severity)),
        explanation: format!(
            "Final FST payload is {bytes} bytes, in the {severity:?} band (R6 decimal-byte size \
             thresholds)."
        ),
        remedies: Vec::new(),
    })
}

/// `crate::emit::FomaTier::Partial`'s observed coverage gaps, which refuse normal generation.
fn partial_tier_finding(report: &EmitReport, uncovered_count: usize) -> HealthFinding {
    let affected: Vec<String> = report
        .uncovered
        .iter()
        .map(|item| item.id.clone())
        .collect();
    HealthFinding {
        code: FindingCode::BackendCoverageIncomplete,
        severity: Severity::CannotRepresent,
        phase: Phase::Compile,
        affected,
        metric: Metric::BackendCoverageGapCount,
        value: MetricValue::Count(uncovered_count as u64),
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation: format!(
            "{uncovered_count} construct occurrence(s) could not be represented in this \
             FST-propose network and contribute no candidates for it. Confirmation cannot restore \
             omitted candidates, so normal generation fails closed."
        ),
        remedies: Vec::new(),
    }
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
    let (code, severity, explanation, remedies) = if depth_budget_stop {
        (
            FindingCode::ResourceBudgetReached,
            Severity::NotProductionReady,
            format!(
                "This grammar's FST-propose path stopped at an internal closure-depth cap before \
                 it finished ({reason}); the attempt is incomplete and its partial output is \
                 unusable, but no fixed affix depth is a language boundary and nothing here shows \
                 the grammar is unrepresentable.{closure_detail}"
            ),
            vec![
                retry_with_internal_caps_removed_remedy(),
                retry_full_engine_remedy(),
            ],
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
            vec![retry_full_engine_remedy()],
        )
    };
    HealthFinding {
        code,
        severity,
        phase: Phase::Compile,
        affected,
        metric: Metric::UnknownUnboundedWork,
        value,
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation,
        remedies,
    }
}

/// The fail-fast eager-enumeration budget's trip, reusing `Metric::UnknownUnboundedWork`.
fn enum_budget_finding(exceeded: &EnumBudgetExceeded) -> HealthFinding {
    HealthFinding {
        code: FindingCode::ResourceBudgetReached,
        severity: Severity::NotProductionReady,
        phase: Phase::Compile,
        affected: Vec::new(),
        metric: Metric::UnknownUnboundedWork,
        value: MetricValue::Count(exceeded.value as u64),
        provenance: ValueProvenance::Observed,
        threshold: Some(MetricValue::Count(exceeded.limit as u64)),
        explanation: format!(
            "The eager-enumeration lexc-emission budget for {measure} reached {value} (limit \
             {limit}); this build stopped before allocating further.",
            measure = exceeded.measure,
            value = exceeded.value,
            limit = exceeded.limit,
        ),
        remedies: vec![retry_full_engine_remedy()],
    }
}

fn backend_compilation_failed_finding(detail: String) -> HealthFinding {
    HealthFinding {
        code: FindingCode::BackendCompilationFailed,
        severity: Severity::NotProductionReady,
        phase: Phase::Compile,
        affected: Vec::new(),
        metric: Metric::UnknownUnboundedWork,
        value: MetricValue::Unbounded,
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation: detail,
        remedies: vec![retry_full_engine_remedy()],
    }
}

/// Every `crate::emit::EmitReport`-sourced finding: the tier disposition plus the enumeration-budget finding when present.
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
    if let Some(exceeded) = &report.enum_budget_exceeded {
        findings.push(enum_budget_finding(exceeded));
    }
    findings
}

/// Every `crate::compose_budget::ComposeError` variant, exhaustively.
fn compose_error_finding(err: &ComposeError) -> HealthFinding {
    match err {
        ComposeError::NetSizeExceeded {
            measure,
            value,
            limit,
            site,
        } => HealthFinding {
            code: FindingCode::ResourceBudgetReached,
            severity: Severity::NotProductionReady,
            phase: Phase::Compile,
            affected: vec![(*site).to_string()],
            metric: match measure {
                NetSizeMeasure::States => Metric::IntermediateStateCount,
                NetSizeMeasure::Arcs => Metric::IntermediateArcCount,
            },
            value: MetricValue::Count((*value).max(0) as u64),
            provenance: ValueProvenance::Observed,
            threshold: Some(MetricValue::Count(*limit as u64)),
            explanation: format!(
                "Composition at {site:?} produced a network of {value} {measure} (limit {limit}); \
                 this compilation stopped rather than continue.",
                measure = measure.label(),
            ),
            remedies: vec![retry_full_engine_remedy()],
        },
        ComposeError::AlphaTupleBudgetExceeded {
            surviving,
            limit,
            rule_xml_id,
        } => HealthFinding {
            code: FindingCode::ProvenBoundExceedsBudget,
            severity: Severity::NotProductionReady,
            phase: Phase::Compile,
            affected: vec![rule_xml_id.clone()],
            metric: Metric::AlphaTupleCount,
            value: MetricValue::Count(*surviving as u64),
            provenance: ValueProvenance::ProvenBound,
            threshold: Some(MetricValue::Count(*limit as u64)),
            explanation: format!(
                "Rule {rule_xml_id:?}'s alpha-variable joint-agreement constraint admits \
                 {surviving} surviving tuple assignments (limit {limit}), an exact count proven \
                 to exceed the remaining budget before the per-tuple compile loop began."
            ),
            remedies: vec![retry_full_engine_remedy()],
        },
        ComposeError::GroupBudgetExceeded {
            groups,
            limit,
            gated_subrules,
        } => HealthFinding {
            code: FindingCode::ProvenBoundExceedsBudget,
            severity: Severity::NotProductionReady,
            phase: Phase::Compile,
            affected: Vec::new(),
            metric: Metric::GateGroupCount,
            value: MetricValue::Count(*groups as u64),
            provenance: ValueProvenance::ProvenBound,
            threshold: Some(MetricValue::Count(*limit as u64)),
            explanation: format!(
                "Partitioning {gated_subrules} gated subrule(s) produced {groups} distinct gating \
                 groups (limit {limit}), an exact count proven to exceed the remaining budget \
                 before any per-group compile work began."
            ),
            remedies: vec![retry_full_engine_remedy()],
        },
        ComposeError::EmitLineBudgetExceeded { lines, limit } => HealthFinding {
            code: FindingCode::ResourceBudgetReached,
            severity: Severity::NotProductionReady,
            phase: Phase::Compile,
            affected: Vec::new(),
            metric: Metric::EmittedLineCount,
            value: MetricValue::Count(*lines as u64),
            provenance: ValueProvenance::Observed,
            threshold: Some(MetricValue::Count(*limit as u64)),
            explanation: format!(
                "Templated/underlying-form lexc emission wrote {lines} lines (limit {limit}) \
                 before this compilation stopped."
            ),
            remedies: vec![retry_full_engine_remedy()],
        },
        ComposeError::ComposeStepTimedOut {
            elapsed,
            limit,
            site,
        } => HealthFinding {
            code: FindingCode::ResourceBudgetReached,
            severity: Severity::NotProductionReady,
            phase: Phase::Compile,
            affected: vec![(*site).to_string()],
            metric: Metric::ElapsedMillis,
            value: MetricValue::Millis(elapsed.as_millis() as u64),
            provenance: ValueProvenance::Observed,
            threshold: Some(MetricValue::Millis(limit.as_millis() as u64)),
            explanation: format!(
                "Composition step at {site:?} exceeded its wall-clock deadline ({elapsed:?} \
                 elapsed, limit {limit:?}); the worker thread was abandoned (not killed) and this \
                 attempt is terminal for this grammar -- never retry the identical call."
            ),
            remedies: vec![retry_full_engine_remedy()],
        },
        ComposeError::ChainDepthExceeded { depth, limit, site } => HealthFinding {
            code: FindingCode::ResourceBudgetReached,
            severity: Severity::NotProductionReady,
            phase: Phase::Apply,
            affected: vec![(*site).to_string()],
            metric: Metric::ApplyChainDepth,
            value: MetricValue::Count(*depth as u64),
            provenance: ValueProvenance::Observed,
            threshold: Some(MetricValue::Count(*limit as u64)),
            explanation: format!(
                "Derivation/unapplication chain depth at {site:?} reached {depth} nested steps \
                 (limit {limit}); this deterministically closes the stack-overflow failure class \
                 (ADR 0003) instead of relying on a larger call stack."
            ),
            remedies: Vec::new(),
        },
        ComposeError::OrderingMultiplicityExceeded {
            rule_count,
            limit,
            site,
        } => HealthFinding {
            code: FindingCode::ProvenBoundExceedsBudget,
            severity: Severity::NotProductionReady,
            phase: Phase::Compile,
            affected: vec![(*site).to_string()],
            metric: Metric::OrderingRuleCount,
            value: MetricValue::Count(*rule_count as u64),
            provenance: ValueProvenance::ProvenBound,
            threshold: Some(MetricValue::Count(*limit as u64)),
            explanation: format!(
                "An Unordered stratum at {site:?} has {rule_count} loose rules (limit {limit}), an \
                 exact count proven to admit more admissible rule orderings than this grammar's \
                 ordering-multiplicity budget allows before any combinatorial walk began."
            ),
            remedies: vec![retry_full_engine_remedy()],
        },
        ComposeError::CompoundPairBudgetExceeded {
            heads,
            non_heads,
            pairs,
            limit,
        } => HealthFinding {
            code: FindingCode::ProvenBoundExceedsBudget,
            severity: Severity::NotProductionReady,
            phase: Phase::Compile,
            affected: Vec::new(),
            metric: Metric::CompoundRootPairCount,
            value: MetricValue::Count(*pairs as u64),
            provenance: ValueProvenance::ProvenBound,
            threshold: Some(MetricValue::Count(*limit as u64)),
            explanation: format!(
                "This grammar's compounding rule(s) license {pairs} head x non-head root-allomorph \
                 pairs ({heads} heads x {non_heads} licensed non-heads, limit {limit}) -- an exact \
                 count proven to exceed the compound-pair budget before any compound lexc text was \
                 written."
            ),
            remedies: vec![retry_full_engine_remedy()],
        },
    }
}

/// One `ApplyBudgetTrip` — see this module's "Judgment calls" item 6.
fn apply_budget_trip_finding(trip: &ApplyBudgetTrip) -> HealthFinding {
    let metric = match trip.dimension {
        ApplyDimension::DecodedPaths => Metric::ProposalPathCount,
        ApplyDimension::Candidates => Metric::ProposalCandidateCount,
    };
    HealthFinding {
        code: FindingCode::ResourceBudgetReached,
        severity: Severity::NotProductionReady,
        phase: Phase::Apply,
        affected: trip.word.iter().cloned().collect(),
        metric,
        value: MetricValue::Count(trip.value as u64),
        provenance: ValueProvenance::Observed,
        threshold: Some(MetricValue::Count(trip.limit as u64)),
        explanation: format!(
            "Apply-time {label} reached {value} (limit {limit}) before this word completed; the \
             word is incomplete, never a definitive partial analysis -- other words in the same \
             batch remain valid and this word may be explicitly resubmitted with a larger apply \
             budget.",
            label = trip.dimension.label(),
            value = trip.value,
            limit = trip.limit,
        ),
        remedies: vec![Remedy {
            rank: 1,
            description:
                "Explicitly retry this word alone with a larger caller-selected apply-time \
                budget."
                    .to_string(),
            requires_linguistic_equivalence: false,
            caveat: None,
        }],
    }
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
///   chain-depth/ordering-multiplicity calls raised (typically zero or one per grammar, but a
///   caller collecting evidence across a batch or a diagnostic sweep may pass more than one).
/// - `apply_budget_trips`: every per-word `ApplyBudgetTrip` this compilation's callers observed.
/// - `compile_profile`: this crate's own `CompileProfile`, if this
///   compilation collected one (`crate::analyzer::FomaProposer::new_with_profile`) — see
///   `profile_findings`'s own doc for exactly which finding kinds this populates, and the
///   production-only gate it enforces on a non-production-labeled profile.
pub fn evaluate_health(
    payload_bytes: Option<u64>,
    emit_report: Option<&EmitReport>,
    compose_errors: &[ComposeError],
    apply_budget_trips: &[ApplyBudgetTrip],
    compile_profile: Option<&CompileProfile>,
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
    if let Some(profile) = compile_profile {
        findings.extend(profile_findings(profile));
    }

    HealthReport::new(findings)
}

/// Converts every typed Foma construction failure into nonempty backend-local health evidence.
pub fn evaluate_foma_error(
    error: &FomaError,
    compile_profile: Option<&CompileProfile>,
) -> HealthReport {
    match error {
        FomaError::LexcCompileFailed(report) => {
            let mut health = evaluate_health(None, Some(report), &[], &[], compile_profile);
            health
                .findings
                .push(backend_compilation_failed_finding(format!(
                "The Foma backend could not compile the emitted lexc representation; no usable \
                 network was produced. Compiler detail: {error}"
            )));
            HealthReport::new(health.findings)
        }
        FomaError::Unsupported(report)
        | FomaError::Incomplete(report)
        | FomaError::EnumerationBudgetExceeded { report, .. } => {
            let mut health = evaluate_health(None, Some(report), &[], &[], compile_profile);
            if health.findings.is_empty() {
                health
                    .findings
                    .push(backend_compilation_failed_finding(error.to_string()));
            }
            HealthReport::new(health.findings)
        }
        FomaError::UnorderedOrderingMultiplicityExceeded { rule_count, limit } => {
            let compose_error = ComposeError::OrderingMultiplicityExceeded {
                rule_count: *rule_count,
                limit: *limit,
                site: "foma backend unordered-stratum characterization",
            };
            evaluate_health(
                None,
                None,
                std::slice::from_ref(&compose_error),
                &[],
                compile_profile,
            )
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
        let enum_report = EmitReport {
            tier: FomaTier::Unsupported {
                reason: "synthetic enumeration refusal".to_string(),
            },
            enum_budget_exceeded: Some(EnumBudgetExceeded {
                measure: "synthetic composite work",
                value: 101,
                limit: 100,
            }),
            ..synthetic_full_emit_report()
        };
        let cases = vec![
            FomaError::LexcCompileFailed(synthetic_full_emit_report()),
            FomaError::Unsupported(EmitReport {
                tier: FomaTier::Unsupported {
                    reason: "synthetic unsupported route".to_string(),
                },
                ..synthetic_full_emit_report()
            }),
            FomaError::EnumerationBudgetExceeded {
                measure: "synthetic composite work",
                value: 101,
                limit: 100,
                report: enum_report,
            },
            FomaError::UnorderedOrderingMultiplicityExceeded {
                rule_count: 11,
                limit: 10,
            },
        ];

        // Every error must BLOCK; the exact band is per-cause, pinned by the split tests below.
        for error in cases {
            let health = evaluate_foma_error(&error, None);
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
        let health = evaluate_foma_error(
            &FomaError::LexcCompileFailed(synthetic_full_emit_report()),
            None,
        );
        assert!(health
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::BackendCompilationFailed));
    }

    #[test]
    fn fst_health_evaluator_backend_local_budget_failures_are_errors() {
        let compose_errors = vec![
            ComposeError::NetSizeExceeded {
                measure: NetSizeMeasure::States,
                value: 2,
                limit: 1,
                site: "net",
            },
            ComposeError::AlphaTupleBudgetExceeded {
                surviving: 2,
                limit: 1,
                rule_xml_id: "mr1".to_string(),
            },
            ComposeError::GroupBudgetExceeded {
                groups: 2,
                limit: 1,
                gated_subrules: 1,
            },
            ComposeError::EmitLineBudgetExceeded { lines: 2, limit: 1 },
            ComposeError::ComposeStepTimedOut {
                elapsed: Duration::from_millis(2),
                limit: Duration::from_millis(1),
                site: "compose",
            },
            ComposeError::ChainDepthExceeded {
                depth: 2,
                limit: 1,
                site: "apply",
            },
            ComposeError::OrderingMultiplicityExceeded {
                rule_count: 2,
                limit: 1,
                site: "ordering",
            },
            ComposeError::CompoundPairBudgetExceeded {
                heads: 1,
                non_heads: 2,
                pairs: 2,
                limit: 1,
            },
        ];
        for error in compose_errors {
            let health = evaluate_health(None, None, &[error], &[], None);
            assert_eq!(health.admission(), Severity::NotProductionReady);
        }

        let trip = ApplyBudgetTrip {
            dimension: ApplyDimension::Candidates,
            value: 2,
            limit: 1,
            word: Some("word".to_string()),
        };
        assert_eq!(
            evaluate_health(None, None, &[], &[trip], None).admission(),
            Severity::NotProductionReady
        );
    }

    // fst_health_evaluator_size_bands: payload-size-only inputs, the single threshold.

    #[test]
    fn fst_health_evaluator_within_limits_payload_produces_no_finding() {
        let report = evaluate_health(Some(crate::health::IDEAL_MAX_BYTES), None, &[], &[], None);
        assert!(report.findings.is_empty());
        assert_eq!(report.admission(), Severity::WithinLimits);
    }

    #[test]
    fn fst_health_evaluator_over_ideal_payload_produces_not_production_ready_payload_size_band_finding() {
        let bytes = 500_000_000u64;
        let report = evaluate_health(Some(bytes), None, &[], &[], None);
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
        let report = evaluate_health(
            Some(crate::health::IDEAL_MAX_BYTES + 1),
            None,
            &[],
            &[],
            None,
        );
        assert_eq!(report.findings[0].severity, Severity::NotProductionReady);
        assert_eq!(
            report.findings[0].threshold,
            Some(MetricValue::Bytes(crate::health::IDEAL_MAX_BYTES))
        );
    }

    #[test]
    fn fst_health_evaluator_oversized_payload_remains_not_production_ready_readiness() {
        let report = evaluate_health(Some(10_000_000_000u64), None, &[], &[], None);
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
        let health = evaluate_health(None, Some(&report), &[], &[], None);
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
        let health = evaluate_health(None, Some(&report), &[], &[], None);
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
        let health = evaluate_health(None, Some(&report), &[], &[], None);
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
            let health = evaluate_health(None, Some(report), &[], &[], None);
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
        let health = evaluate_health(None, Some(&report), &[], &[], None);
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
        let health = evaluate_health(None, Some(&report), &[], &[], None);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::ResourceBudgetReached);
        assert_eq!(finding.severity, Severity::NotProductionReady);
        assert_eq!(finding.class(), FindingClass::Containment);
        assert_ne!(
            finding.severity,
            Severity::MachineLimit,
            "a depth-budget stop halted one attempt; it must never condemn the grammar"
        );
        assert!(
            finding
                .remedies
                .iter()
                .any(|remedy| remedy.description.contains("internal size/work caps removed")),
            "a containment stop must name the caps-removed retry route: {:?}",
            finding.remedies
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
        let health = evaluate_health(None, Some(&report), &[], &[], None);
        let finding = &health.findings[0];
        assert_eq!(finding.affected, vec!["mrule3", "mrule7"]);
        assert_eq!(finding.value, MetricValue::Count(11));
        assert!(finding.explanation.contains("closure-depth limit was 64"));
    }

    #[test]
    fn fst_health_evaluator_enum_budget_exceeded_is_resource_budget_reached() {
        let report = EmitReport {
            uncovered: Vec::new(),
            counts: EmitCounts::default(),
            tier: FomaTier::Unsupported {
                reason: "synthetic eager-enumeration budget exceeded".to_string(),
            },
            enum_budget_exceeded: Some(EnumBudgetExceeded {
                measure: "synthetic composite lexc entries",
                value: 5_001,
                limit: 5_000,
            }),
            closure_refusal: None,
            closure_evidence: None,
        };
        let health = evaluate_health(None, Some(&report), &[], &[], None);
        // One finding for the Unsupported tier, one for the specific tripped budget.
        assert_eq!(health.findings.len(), 2);
        let budget_finding = health
            .findings
            .iter()
            .find(|f| f.code == FindingCode::ResourceBudgetReached)
            .expect("enum budget finding present");
        assert_eq!(budget_finding.severity, Severity::NotProductionReady);
        assert_eq!(budget_finding.value, MetricValue::Count(5_001));
        assert_eq!(budget_finding.threshold, Some(MetricValue::Count(5_000)));
        // The co-occurring Unsupported-tier CannotRepresent dominates this NotProductionReady under admission()'s max.
        assert_eq!(health.admission(), Severity::CannotRepresent);
    }

    // fst_health_evaluator_compose_errors: every ComposeError variant maps to a finding.

    #[test]
    fn fst_health_evaluator_net_size_exceeded_is_resource_budget_reached_observed() {
        let err = ComposeError::NetSizeExceeded {
            measure: NetSizeMeasure::States,
            value: 3_000_000,
            limit: 2_000_000,
            site: "synthetic-test-site",
        };
        let health = evaluate_health(None, None, std::slice::from_ref(&err), &[], None);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::ResourceBudgetReached);
        assert_eq!(finding.metric, Metric::IntermediateStateCount);
        assert_eq!(finding.provenance, ValueProvenance::Observed);
        assert_eq!(finding.value, MetricValue::Count(3_000_000));
        assert_eq!(finding.threshold, Some(MetricValue::Count(2_000_000)));
        assert_eq!(finding.affected, vec!["synthetic-test-site".to_string()]);
        assert_eq!(health.admission(), Severity::NotProductionReady);
    }

    #[test]
    fn fst_health_evaluator_alpha_tuple_exceeded_is_proven_bound() {
        let err = ComposeError::AlphaTupleBudgetExceeded {
            surviving: 6_000,
            limit: 5_000,
            rule_xml_id: "synthetic-mrule-0099".to_string(),
        };
        let health = evaluate_health(None, None, std::slice::from_ref(&err), &[], None);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::ProvenBoundExceedsBudget);
        assert_eq!(finding.metric, Metric::AlphaTupleCount);
        assert_eq!(finding.provenance, ValueProvenance::ProvenBound);
        assert_eq!(finding.affected, vec!["synthetic-mrule-0099".to_string()]);
    }

    #[test]
    fn fst_health_evaluator_group_budget_exceeded_is_proven_bound() {
        let err = ComposeError::GroupBudgetExceeded {
            groups: 100,
            limit: 64,
            gated_subrules: 7,
        };
        let health = evaluate_health(None, None, std::slice::from_ref(&err), &[], None);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::ProvenBoundExceedsBudget);
        assert_eq!(finding.metric, Metric::GateGroupCount);
        assert_eq!(finding.provenance, ValueProvenance::ProvenBound);
    }

    #[test]
    fn fst_health_evaluator_emit_line_budget_exceeded_is_resource_budget_reached() {
        let err = ComposeError::EmitLineBudgetExceeded {
            lines: 1_000_001,
            limit: 1_000_000,
        };
        let health = evaluate_health(None, None, std::slice::from_ref(&err), &[], None);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::ResourceBudgetReached);
        assert_eq!(finding.metric, Metric::EmittedLineCount);
        assert_eq!(finding.provenance, ValueProvenance::Observed);
    }

    #[test]
    fn fst_health_evaluator_compose_step_timed_out_is_resource_budget_reached_millis() {
        let err = ComposeError::ComposeStepTimedOut {
            elapsed: Duration::from_millis(5_500),
            limit: Duration::from_millis(5_000),
            site: "synthetic-timeout-site",
        };
        let health = evaluate_health(None, None, std::slice::from_ref(&err), &[], None);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::ResourceBudgetReached);
        assert_eq!(finding.metric, Metric::ElapsedMillis);
        assert_eq!(finding.value, MetricValue::Millis(5_500));
        assert_eq!(finding.threshold, Some(MetricValue::Millis(5_000)));
    }

    #[test]
    fn fst_health_evaluator_chain_depth_exceeded_is_apply_phase() {
        let err = ComposeError::ChainDepthExceeded {
            depth: 30,
            limit: 24,
            site: "synthetic-peel-site",
        };
        let health = evaluate_health(None, None, std::slice::from_ref(&err), &[], None);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::ResourceBudgetReached);
        assert_eq!(finding.metric, Metric::ApplyChainDepth);
        assert_eq!(finding.phase, Phase::Apply);
    }

    #[test]
    fn fst_health_evaluator_ordering_multiplicity_exceeded_uses_new_metric() {
        let err = ComposeError::OrderingMultiplicityExceeded {
            rule_count: 120,
            limit: 100,
            site: "synthetic-unordered-stratum",
        };
        let health = evaluate_health(None, None, std::slice::from_ref(&err), &[], None);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::ProvenBoundExceedsBudget);
        assert_eq!(finding.metric, Metric::OrderingRuleCount);
        assert_eq!(finding.provenance, ValueProvenance::ProvenBound);
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
        let health = evaluate_health(None, None, &[], std::slice::from_ref(&trip), None);
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
        let health = evaluate_health(None, None, &[], std::slice::from_ref(&trip), None);
        let finding = &health.findings[0];
        assert_eq!(finding.metric, Metric::ProposalCandidateCount);
        assert!(finding.affected.is_empty());
    }

    // fst_health_evaluator_empty_report_is_within_limits

    #[test]
    fn fst_health_evaluator_empty_report_is_within_limits() {
        let health = evaluate_health(None, None, &[], &[], None);
        assert!(health.findings.is_empty());
        assert_eq!(health.admission(), Severity::WithinLimits);
        assert_eq!(health.schema_version, crate::health::HEALTH_SCHEMA_VERSION);
    }

    // fst_health_evaluator_profile: IntermediateNetworkGrowth/CompileWorkBudget populate from `crate::profile::CompileProfile`, and the production-only `ProfileLabel` gate.

    fn synthetic_profile(
        label: ProfileLabel,
        final_state_count: Option<i64>,
        final_arc_count: Option<i64>,
        total_lexc_lines: Option<u64>,
    ) -> CompileProfile {
        CompileProfile {
            label,
            pipeline: "synthetic-test-pipeline".to_string(),
            total_elapsed_millis: 5,
            stages: Vec::new(),
            group_lines: Vec::new(),
            total_lexc_lines,
            final_state_count,
            final_arc_count,
        }
    }

    /// Comfortably-below-threshold values must produce nothing (WithinLimits), proving the approaching-budget path is real gating, not unconditional.
    #[test]
    fn fst_health_evaluator_profile_below_threshold_produces_no_finding() {
        // 50% of DEFAULT_STATE_BUDGET/DEFAULT_ARC_BUDGET/DEFAULT_LINE_BUDGET.
        let profile = synthetic_profile(
            ProfileLabel::Production,
            Some((DEFAULT_STATE_BUDGET / 2) as i64),
            Some((DEFAULT_ARC_BUDGET / 2) as i64),
            Some((DEFAULT_LINE_BUDGET / 2) as u64),
        );
        let health = evaluate_health(None, None, &[], &[], Some(&profile));
        assert!(
            health.findings.is_empty(),
            "a comfortably-below-threshold profile must produce no finding, got {:?}",
            health.findings
        );
        assert_eq!(health.admission(), Severity::WithinLimits);
    }

    /// 90% of `DEFAULT_STATE_BUDGET` produces an Observed `IntermediateNetworkGrowth` LargeMultiplier finding with the exact measured value and the calibrated budget as its threshold.
    #[test]
    fn fst_health_evaluator_profile_network_growth_approaching_budget_produces_large_multiplier() {
        let states = (DEFAULT_STATE_BUDGET as f64 * 0.9) as i64;
        let profile = synthetic_profile(ProfileLabel::Production, Some(states), None, None);
        let health = evaluate_health(None, None, &[], &[], Some(&profile));
        assert_eq!(health.findings.len(), 1);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::IntermediateNetworkGrowth);
        assert_eq!(finding.metric, Metric::IntermediateStateCount);
        assert_eq!(finding.severity, Severity::LargeMultiplier);
        assert_eq!(finding.provenance, ValueProvenance::Observed);
        assert_eq!(finding.value, MetricValue::Count(states as u64));
        assert_eq!(
            finding.threshold,
            Some(MetricValue::Count(DEFAULT_STATE_BUDGET as u64))
        );
        assert_eq!(health.admission(), Severity::LargeMultiplier);
    }

    /// Same shape, the arc-count dimension.
    #[test]
    fn fst_health_evaluator_profile_arc_growth_approaching_budget_produces_large_multiplier() {
        let arcs = (DEFAULT_ARC_BUDGET as f64 * 0.85) as i64;
        let profile = synthetic_profile(ProfileLabel::Production, None, Some(arcs), None);
        let health = evaluate_health(None, None, &[], &[], Some(&profile));
        assert_eq!(health.findings.len(), 1);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::IntermediateNetworkGrowth);
        assert_eq!(finding.metric, Metric::IntermediateArcCount);
        assert_eq!(finding.severity, Severity::LargeMultiplier);
    }

    /// The total-emitted-lexc-lines dimension -- `CompileWorkBudget`.
    #[test]
    fn fst_health_evaluator_profile_compile_work_lines_approaching_budget_produces_large_multiplier() {
        let lines = (DEFAULT_LINE_BUDGET as f64 * 0.95) as u64;
        let profile = synthetic_profile(ProfileLabel::Production, None, None, Some(lines));
        let health = evaluate_health(None, None, &[], &[], Some(&profile));
        assert_eq!(health.findings.len(), 1);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::CompileWorkBudget);
        assert_eq!(finding.metric, Metric::EmittedLineCount);
        assert_eq!(finding.severity, Severity::LargeMultiplier);
        assert_eq!(finding.value, MetricValue::Count(lines));
        assert_eq!(
            finding.threshold,
            Some(MetricValue::Count(DEFAULT_LINE_BUDGET as u64))
        );
    }

    /// A profile labeled `ExperimentalComposition` must be refused outright even when its values would otherwise trip every dimension at once.
    #[test]
    fn fst_health_evaluator_experimental_composition_profile_is_refused() {
        let profile = synthetic_profile(
            ProfileLabel::ExperimentalComposition,
            Some(DEFAULT_STATE_BUDGET as i64 * 10),
            Some(DEFAULT_ARC_BUDGET as i64 * 10),
            Some(DEFAULT_LINE_BUDGET as u64 * 10),
        );
        assert!(
            profile_findings(&profile).is_empty(),
            "an experimental_composition-labeled profile must never satisfy production-profile gates"
        );
        let health = evaluate_health(None, None, &[], &[], Some(&profile));
        assert!(health.findings.is_empty());
        assert_eq!(health.admission(), Severity::WithinLimits);
    }

    // fst_health_evaluator_golden: a representative multi-source compile, byte-for-byte golden.

    /// Three distinct measurement sources (payload size, an emit report, a compose error) feeding one report, the shape a real caller assembles.
    fn representative_inputs() -> (u64, EmitReport, ComposeError) {
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
        let compose_error = ComposeError::NetSizeExceeded {
            measure: NetSizeMeasure::Arcs,
            value: 21_000_000,
            limit: 20_000_000,
            site: "synthetic-gate-union-fold",
        };
        (payload_bytes, emit_report, compose_error)
    }

    const GOLDEN_JSON: &str = r#"{
  "schema_version": 3,
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
    },
    {
      "code": "PGF0008",
      "severity": "not_production_ready",
      "phase": "compile",
      "affected": [
        "synthetic-gate-union-fold"
      ],
      "metric": "intermediate_arc_count",
      "value": {
        "kind": "count",
        "value": 21000000
      },
      "provenance": "observed",
      "threshold": {
        "kind": "count",
        "value": 20000000
      },
      "explanation": "Composition at \"synthetic-gate-union-fold\" produced a network of 21000000 arcs (limit 20000000); this compilation stopped rather than continue.",
      "remedies": [
        {
          "rank": 1,
          "description": "Use the default (full) morphological-parser engine for this grammar instead of the FST-propose/composition path, or raise the specific tripped budget's own env var only if you understand why this grammar's composition is this large, and re-run.",
          "requires_linguistic_equivalence": false
        }
      ]
    }
  ]
}"#;

    #[test]
    fn fst_health_evaluator_golden_json() {
        let (payload_bytes, emit_report, compose_error) = representative_inputs();
        let health = evaluate_health(
            Some(payload_bytes),
            Some(&emit_report),
            std::slice::from_ref(&compose_error),
            &[],
            None,
        );
        let json = health.to_json().expect("serialization must succeed");
        assert_eq!(
            json, GOLDEN_JSON,
            "canonical JSON drifted from the committed golden"
        );
    }

    #[test]
    fn fst_health_evaluator_golden_admission_is_cannot_represent() {
        let (payload_bytes, emit_report, compose_error) = representative_inputs();
        let health = evaluate_health(
            Some(payload_bytes),
            Some(&emit_report),
            std::slice::from_ref(&compose_error),
            &[],
            None,
        );
        // An uncovered construct is CannotRepresent even when resource findings have lower severity.
        assert_eq!(health.admission(), Severity::CannotRepresent);
    }

    #[test]
    fn fst_health_evaluator_golden_round_trips() {
        let (payload_bytes, emit_report, compose_error) = representative_inputs();
        let health = evaluate_health(
            Some(payload_bytes),
            Some(&emit_report),
            std::slice::from_ref(&compose_error),
            &[],
            None,
        );
        let json = health.to_json().expect("serialization must succeed");
        let parsed = HealthReport::from_json(&json).expect("deserialization must succeed");
        assert_eq!(
            parsed, health,
            "round trip through canonical JSON must be lossless"
        );
    }
}
