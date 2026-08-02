//! `openspec/changes/add-fst-compilation-health-audit`: the Rust health **evaluator** — the first
//! real (non-test) producer of `crate::health::HealthFinding`s. `crate::health` (Stage 0D of
//! `openspec/changes/define-fst-compilation-health`) defined and unit-tested the finding schema
//! only ("purely additive... does not instrument any compiler pass"); this module is "a later
//! change [that] wires a real evaluator that reads `crate::compose_budget`/
//! `crate::morphotactics::EnumerationBudget` measurements and produces `HealthFinding`s from them"
//! — that module's own doc, quoted verbatim, naming this file's job.
//!
//! # Scope: consume, never remeasure (R6)
//! `openspec/changes/IMPLEMENTATION-READINESS.md` §R6: "Measurements come from the admission
//! walker, budget tracker, and compile profile once; the health evaluator consumes them without
//! recomputation." This module reads exactly four measurement sources that exist in this crate
//! **today** — nothing here calls `foma`, walks a grammar, or measures anything itself:
//! - **Payload size**: a plain `u64` byte count the caller already has (the emitted network /
//!   `pg-pack` payload), scored by [`crate::health::severity_for_size_bytes`] (unchanged, reused).
//! - **[`crate::emit::EmitReport`]**: `tier`/`uncovered`/`enum_budget_exceeded`, already produced
//!   by `crate::emit::emit`/`emit_with_budget`.
//! - **[`crate::compose_budget::ComposeError`]** (compile-time composition budget trips) and
//!   [`ApplyBudgetTrip`] (this module's own lightweight distillation of a per-word
//!   `crate::compose_budget::ApplyOutcome::Incomplete` — see that type's own doc for why it exists
//!   instead of taking `ApplyOutcome<T>` generically).
//! - **[`crate::profile::CompileProfile`]** (`openspec/changes/profile-fst-compilation`): [`profile_findings`]
//!   reads its final compiled-network state/arc counts and total emitted-line count to produce the
//!   two *approaching-but-not-yet-tripped* finding kinds this module's own doc used to list as
//!   deferred (see immediately below) — the compile-time-series instrumentation R6 asked for.
//!
//! **Previously deferred, now populated by [`profile_findings`]** (`profile-fst-compilation`):
//! [`crate::health::FindingCode::IntermediateNetworkGrowth`] (the production network's own final
//! state/arc count approaching, but not tripping, `crate::compose_budget::DEFAULT_STATE_BUDGET`/
//! `DEFAULT_ARC_BUDGET` — reused as the closest existing calibrated size dimension; see
//! [`profile_findings`]'s own doc for why Phase A's production path has no earlier "intermediate"
//! composition product to measure instead) and [`crate::health::FindingCode::CompileWorkBudget`]
//! (total emitted lexc lines approaching, but not tripping, `crate::compose_budget::
//! DEFAULT_LINE_BUDGET` — a dimension the production path does not even check today, unlike the
//! experimental `emit_underlying_templated`/`crate::uflexc` paths' own incremental `line_cap`
//! check).
//!
//! **Still explicitly deferred, not populated here** (R6: "FST health policy/schema may land before
//! instrumentation; observed audit fields populate as their owning profile/budget changes merge
//! and are never independently remeasured" — `openspec/changes/IMPLEMENTATION-READINESS.md`
//! "Conditional/later work"): [`crate::health::FindingCode::ApplicationTimeWork`]'s
//! [`crate::health::Metric::ElapsedMillis`]/[`crate::health::Metric::ApplyAllocationBytes`]
//! dimensions (no per-word wall-clock/allocation instrumentation exists yet, only the two
//! magnitude caps [`ApplyBudgetTrip`] already covers — `profile-fst-compilation` is a COMPILE-time
//! profile, this dimension is per-word APPLY-time, a different measurement surface entirely);
//! [`crate::health::FindingCode::DuplicateAnalysisOverlap`] (needs `crate::confirm`'s pre-dedup
//! counts, not produced anywhere yet); and [`crate::health::FindingCode::ProposalVolume`]/
//! [`crate::health::FindingCode::ConfirmationWork`] for *large-but-not-tripped* candidate/
//! confirmation volume (only the tripped case, via [`ApplyBudgetTrip`], is evaluated here — see
//! this module's "Judgment calls" section, item 6; also apply-time, not compile-time). Every one of
//! these finding kinds is fully *producible* by this evaluator's own shape (the `match` arms below
//! are exhaustive over [`crate::compose_budget::ComposeError`]/[`crate::emit::FomaTier`]) but stays
//! unpopulated until its owning profile/budget change lands real values to read.
//!
//! # Two distinct axes, again (see `crate::health`'s own doc first)
//! Every [`HealthFinding`] this module builds carries `severity` on the cost/health axis only
//! (never the ADR 0001/0005 capability-trust axis) and always `override_record: None` — attaching
//! an [`crate::health::OverrideRecord`] to a finding is a separate, later, explicitly-authorized
//! caller action (`tasks.md` section 4, "Admission and packages"), not something this evaluator
//! (which only reads compiler measurements) can decide on its own. [`HealthReport::admission`]
//! (unmodified, called as-is — never re-derived here) is what turns this report's findings into
//! CONTEXT.md's `FST admission result`.
//!
//! # Judgment calls flagged for review
//! 1. **[`crate::compose_budget::ComposeError`] variants split into two [`crate::health::
//!    FindingCode`]s by *when* the check runs, not by variant name alone**: `AlphaTupleBudgetExceeded`/
//!    `GroupBudgetExceeded`/`OrderingMultiplicityExceeded` are checked BEFORE the expensive
//!    operation they would gate even starts (`compose_budget.rs`'s own doc, verbatim, for all
//!    three: "checked BEFORE..."), on an exact, already-known count — CONTEXT.md's `Proven work
//!    bound` — so they map to [`FindingCode::ProvenBoundExceedsBudget`] with
//!    [`ValueProvenance::ProvenBound`]. `NetSizeExceeded`/`EmitLineBudgetExceeded`/
//!    `ComposeStepTimedOut`/`ChainDepthExceeded` are only detected AFTER the checked operation (an
//!    actual compose/union/minimize call, an actual emission run, an actual wall-clock wait, an
//!    actual recursion) already executed and produced/consumed a measured value, so they map to
//!    [`FindingCode::ResourceBudgetReached`] with [`ValueProvenance::Observed`].
//! 2. **[`crate::health::Metric::OrderingRuleCount`] is a new variant this change appends** to
//!    `crate::health`'s `Metric` enum (see that enum's own doc on the variant) — the only schema
//!    edit this evaluator makes, and purely additive (no renumbering, no removal, no change to any
//!    existing golden JSON).
//! 3. **`crate::emit::FomaTier::Partial`'s `uncovered` count maps to
//!    [`FindingCode::UnknownUnboundedConstruct`] at [`Severity::Warning`]**, not
//!    [`Severity::Critical`]: `FomaTier::Partial`'s own doc is explicit that this is "still safe to
//!    use — those constructs simply cannot contribute candidates; nothing was emitted incorrectly"
//!    — the same shape R6 names for cost uncertainty ("not itself Critical"), even though
//!    CONTEXT.md's `Cost uncertainty` glossary entry literally describes *unknown* cost under a
//!    recall-preserving disposition, and an uncovered construct is instead a *confirmed*, exactly-
//!    counted zero-candidate gap for those specific occurrences. [`FindingCode::UnknownUnboundedConstruct`]
//!    is nonetheless the closest of this schema's ten registered codes for a per-construct coverage
//!    fact today; a dedicated coverage-gap code is a candidate follow-on if this reuse proves
//!    confusing in practice. [`ValueProvenance::Observed`] (not `Predicted`) is used throughout
//!    this module's `FomaTier`-derived findings because the uncovered count is an exact, already-
//!    counted value, never a heuristic guess.
//! 4. **`crate::emit::FomaTier::Unsupported` maps to the SAME [`FindingCode::UnknownUnboundedConstruct`]
//!    but at [`Severity::Critical`]**, deliberately diverging from that code's general "not itself
//!    Critical" framing: `Unsupported` means this compile path produced no usable network at all —
//!    R6's "any uncertainty that could omit an analysis fails closed" taken to its maximal case
//!    (total, not partial, coverage loss), not the ordinary bounded-cost-uncertainty shape the code
//!    otherwise names. [`MetricValue::Unbounded`] is used here (this compile's residual
//!    coverage is definitionally unknown, not a countable partial gap).
//! 5. **`crate::emit::EnumBudgetExceeded`'s free-form `measure: &'static str` label has no
//!    dedicated [`Metric`]** (it names one of several different eager-enumeration measures --
//!    `crate::morphotactics::EnumMeasure`'s own label set -- not one fixed quantity); this evaluator
//!    reuses [`Metric::UnknownUnboundedWork`] (the closest existing "unbounded compile-time-work"
//!    slot) and folds the exact label into the finding's `explanation` text, since `Metric` itself
//!    cannot carry a free-form label.
//! 6. **[`ApplyBudgetTrip`] is this module's own type, not `crate::compose_budget::ApplyOutcome<T>`
//!    directly**: `ApplyOutcome<T>`'s `Complete(T)` payload type varies by caller (e.g.
//!    `Vec<Candidate>`) and carries nothing this evaluator needs; making [`evaluate_health`] generic
//!    over `T` just to ignore `Complete`'s payload would cost every caller a type parameter for no
//!    benefit. Callers extract each `ApplyOutcome::Incomplete { dimension, value, limit }` into an
//!    [`ApplyBudgetTrip`] themselves — a direct field-for-field copy, not a recomputation.
//! 7. **All findings this module builds set `affected` from whatever stable identifier the source
//!    measurement already carries** (a compose-budget `site` label, an `UncoveredItem::id`, a rule
//!    XML id, an apply-time word) — never inventing a new identifier scheme; grammar-level findings
//!    with no specific construct identifier (e.g. a payload-size finding) leave `affected` empty.
//! 8. **[`profile_findings`] reuses [`Metric::IntermediateStateCount`]/[`Metric::IntermediateArcCount`]
//!    for the PRODUCTION path's own FINAL compiled network**, not a mid-cascade intermediate
//!    composition product — `crate::emit::emit_with_budget`'s Phase A production path (proposal.md's
//!    own "Context": "pre-bakes phonology into emitted surface forms, so replacement-rule nets ...
//!    do not exist there") performs no separate `fsm_compose`/`fsm_union`/`fsm_minimize` call at all;
//!    the single `foma::lexcread::fsm_lexc_parse_string` call IS the only network-construction step,
//!    so its own state/arc count is simultaneously the "final" and the only "intermediate" product
//!    available pre-Stage-2. This reuses the task brief's required existing `Metric`/`MetricValue`
//!    vocabulary rather than inventing a parallel one; the finding's own `explanation` text says so
//!    explicitly, so a report reader is never misled into expecting Phase B's future per-rule
//!    cascade curve from a Phase A report.
//! 9. **A single flat 80%-of-budget threshold, not a banded severity scale**
//!    ([`APPROACHING_BUDGET_WARNING_FRACTION`]) — no real large-grammar measurement of a legitimate
//!    "approaching" curve exists yet for either dimension (mirrors `crate::compose_budget`'s own
//!    "conservative placeholder pending real-grammar measurement" convention for its calibrated
//!    defaults); always [`Severity::Warning`], never escalated further by this evaluator, because an
//!    ACTUAL trip of the same dimension is a completely different, already-handled code path
//!    ([`compose_error_finding`]'s [`FindingCode::ResourceBudgetReached`]/
//!    [`FindingCode::ProvenBoundExceedsBudget`] arms) that this function never reaches (Phase A's
//!    production path has no compose-budget-checked call site at all, module doc).
//! 10. **[`profile_findings`] refuses a non-[`crate::profile::ProfileLabel::Production`] profile
//!     outright** (empty `Vec`, never a partial fold) — design.md's Phase B gate/spec.md
//!     "Experimental cascade is profiled early": an `ExperimentalComposition`-labeled profile "cannot
//!     satisfy production-profile gates." [`evaluate_health`] never even needs to check this itself;
//!     [`profile_findings`] is the one and only place this gate is enforced.

use crate::compose_budget::{
    ApplyDimension, ComposeError, NetSizeMeasure, DEFAULT_ARC_BUDGET, DEFAULT_LINE_BUDGET,
    DEFAULT_STATE_BUDGET,
};
use crate::emit::{EmitReport, EnumBudgetExceeded, FomaTier};
use crate::health::{
    severity_for_size_bytes, FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase,
    Remedy, Severity, ValueProvenance,
};
use crate::profile::{CompileProfile, ProfileLabel};

/// `openspec/changes/profile-fst-compilation`: the fraction of a calibrated compose-budget
/// dimension ([`DEFAULT_STATE_BUDGET`]/[`DEFAULT_ARC_BUDGET`]/[`DEFAULT_LINE_BUDGET`]) at or above
/// which [`profile_findings`] raises an "approaching, not yet tripped" [`Severity::Warning`]
/// finding — this module's own doc's previously-deferred "continuous '80% of budget' measurement."
/// A single flat threshold, not a banded severity scale — see this module's "Judgment calls" item
/// 9 for why.
const APPROACHING_BUDGET_WARNING_FRACTION: f64 = 0.8;

/// One "approaching, not yet tripped" [`Severity::Warning`] finding, or `None` when `value` is
/// below [`APPROACHING_BUDGET_WARNING_FRACTION`] of `limit` (this module's own "Ideal: nothing to
/// report" convention, mirroring [`payload_size_finding`]). Shared by every [`profile_findings`]
/// dimension so the threshold/severity policy lives in exactly one place.
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
        severity: Severity::Warning,
        phase: Phase::Compile,
        affected: Vec::new(),
        metric,
        value: MetricValue::Count(value),
        provenance: ValueProvenance::Observed,
        threshold: Some(MetricValue::Count(limit)),
        explanation,
        remedies: Vec::new(),
        override_record: None,
    })
}

/// `crate::profile::CompileProfile`-sourced findings (`openspec/changes/profile-fst-compilation`;
/// this module's own doc, "Previously deferred, now populated"): [`FindingCode::IntermediateNetworkGrowth`]
/// from the production network's final state/arc count approaching (but not tripping)
/// [`DEFAULT_STATE_BUDGET`]/[`DEFAULT_ARC_BUDGET`], and [`FindingCode::CompileWorkBudget`] from the
/// total emitted lexc line count approaching (but not tripping) [`DEFAULT_LINE_BUDGET`] — see this
/// module's "Judgment calls" items 8/9 for the reuse/threshold rationale.
///
/// Phase B gate (this module's "Judgment calls" item 10): a `profile.label !=
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
/// see this module's "Judgment calls" item 6 for why [`evaluate_health`] takes this instead of the
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

/// A short, stable, applicable remedy shared by every budget-tripped compile-time finding this
/// module builds: fall back to the full morphological-parser engine, or (only for a caller who
/// understands why this grammar's composition is this large) raise the specific tripped budget's
/// own env var and re-run. Never a grammar edit (`requires_linguistic_equivalence: false`) — this
/// is host/engine-selection and resource-envelope advice, not a source-grammar change.
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

/// [`severity_for_size_bytes`]'s own band boundaries (R6), returned as the crossed threshold for
/// every non-[`Severity::Ideal`] band — e.g. an [`Severity::Error`] finding's threshold is
/// `100_000_000` (the [`Severity::Warning`] band's own ceiling, the boundary this payload crossed
/// to become Error). Mirrors `crate::health`'s own golden test's worked Error finding
/// (`threshold: Some(MetricValue::Bytes(100_000_000))` for a 150,000,000-byte payload).
fn size_band_crossed_threshold(severity: Severity) -> MetricValue {
    match severity {
        Severity::Ideal => {
            unreachable!("payload_size_finding filters Severity::Ideal before calling this")
        }
        Severity::Info => MetricValue::Bytes(10_000_000),
        Severity::Warning => MetricValue::Bytes(20_000_000),
        Severity::Error => MetricValue::Bytes(100_000_000),
        Severity::Critical => MetricValue::Bytes(500_000_000),
    }
}

/// Maps a final FST payload byte count to a [`HealthFinding`] via
/// [`severity_for_size_bytes`] (reused unchanged, never re-derived). `None` when the payload is
/// within the Ideal band — R6/`crate::health`'s own convention: "Ideal: Within every band; nothing
/// to report."
fn payload_size_finding(bytes: u64) -> Option<HealthFinding> {
    let severity = severity_for_size_bytes(bytes);
    if severity == Severity::Ideal {
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
        override_record: None,
    })
}

/// `crate::emit::FomaTier::Partial`'s `uncovered` construct occurrences — see this module's
/// "Judgment calls" item 3 for the [`Severity::Warning`]/[`FindingCode::UnknownUnboundedConstruct`]
/// choice.
fn partial_tier_finding(report: &EmitReport, uncovered_count: usize) -> HealthFinding {
    let affected: Vec<String> = report
        .uncovered
        .iter()
        .map(|item| item.id.clone())
        .collect();
    HealthFinding {
        code: FindingCode::UnknownUnboundedConstruct,
        severity: Severity::Warning,
        phase: Phase::Compile,
        affected,
        metric: Metric::UnknownUnboundedWork,
        value: MetricValue::Count(uncovered_count as u64),
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation: format!(
            "{uncovered_count} construct occurrence(s) could not be represented in this \
             FST-propose network and contribute no candidates for it; this build emitted no \
             incorrect entries, but recall for analyses that depend on these occurrences relies on \
             this grammar's other analysis path(s)."
        ),
        remedies: Vec::new(),
        override_record: None,
    }
}

/// `crate::emit::FomaTier::Unsupported` — see this module's "Judgment calls" item 4 for the
/// [`Severity::Critical`] choice (deliberately diverging from
/// [`FindingCode::UnknownUnboundedConstruct`]'s general "not itself Critical" framing).
fn unsupported_tier_finding(reason: &str) -> HealthFinding {
    HealthFinding {
        code: FindingCode::UnknownUnboundedConstruct,
        severity: Severity::Critical,
        phase: Phase::Compile,
        affected: Vec::new(),
        metric: Metric::UnknownUnboundedWork,
        value: MetricValue::Unbounded,
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation: format!(
            "This grammar's FST-propose path produced no usable network at all ({reason}); this \
             compile path's coverage is entirely unknown, the maximal case of R6's \"any \
             uncertainty that could omit an analysis fails closed\"."
        ),
        remedies: vec![retry_full_engine_remedy()],
        override_record: None,
    }
}

/// `crate::emit::EnumBudgetExceeded` (Fix 1, the fail-fast eager-enumeration budget) — see this
/// module's "Judgment calls" item 5 for the [`Metric::UnknownUnboundedWork`] reuse.
fn enum_budget_finding(exceeded: &EnumBudgetExceeded) -> HealthFinding {
    HealthFinding {
        code: FindingCode::ResourceBudgetReached,
        severity: Severity::Critical,
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
        override_record: None,
    }
}

/// Every `crate::emit::EmitReport`-sourced finding: the tier disposition (`Full` produces none,
/// `Partial`/`Unsupported` each produce one) plus the enumeration-budget finding when present.
fn emit_report_findings(report: &EmitReport) -> Vec<HealthFinding> {
    let mut findings = Vec::new();
    match &report.tier {
        FomaTier::Full => {}
        FomaTier::Partial { uncovered } => {
            findings.push(partial_tier_finding(report, *uncovered));
        }
        FomaTier::Unsupported { reason } => {
            findings.push(unsupported_tier_finding(reason));
        }
    }
    if let Some(exceeded) = &report.enum_budget_exceeded {
        findings.push(enum_budget_finding(exceeded));
    }
    findings
}

/// Every `crate::compose_budget::ComposeError` variant, exhaustively — see this module's
/// "Judgment calls" item 1 for the `ResourceBudgetReached`/`ProvenBoundExceedsBudget` split.
fn compose_error_finding(err: &ComposeError) -> HealthFinding {
    match err {
        ComposeError::NetSizeExceeded {
            measure,
            value,
            limit,
            site,
        } => HealthFinding {
            code: FindingCode::ResourceBudgetReached,
            severity: Severity::Critical,
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
            override_record: None,
        },
        ComposeError::AlphaTupleBudgetExceeded {
            surviving,
            limit,
            rule_xml_id,
        } => HealthFinding {
            code: FindingCode::ProvenBoundExceedsBudget,
            severity: Severity::Critical,
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
            override_record: None,
        },
        ComposeError::GroupBudgetExceeded {
            groups,
            limit,
            gated_subrules,
        } => HealthFinding {
            code: FindingCode::ProvenBoundExceedsBudget,
            severity: Severity::Critical,
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
            override_record: None,
        },
        ComposeError::EmitLineBudgetExceeded { lines, limit } => HealthFinding {
            code: FindingCode::ResourceBudgetReached,
            severity: Severity::Critical,
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
            override_record: None,
        },
        ComposeError::ComposeStepTimedOut {
            elapsed,
            limit,
            site,
        } => HealthFinding {
            code: FindingCode::ResourceBudgetReached,
            severity: Severity::Critical,
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
            override_record: None,
        },
        ComposeError::ChainDepthExceeded { depth, limit, site } => HealthFinding {
            code: FindingCode::ResourceBudgetReached,
            severity: Severity::Critical,
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
            override_record: None,
        },
        ComposeError::OrderingMultiplicityExceeded {
            rule_count,
            limit,
            site,
        } => HealthFinding {
            code: FindingCode::ProvenBoundExceedsBudget,
            severity: Severity::Critical,
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
            override_record: None,
        },
        ComposeError::CompoundPairBudgetExceeded {
            heads,
            non_heads,
            pairs,
            limit,
        } => HealthFinding {
            code: FindingCode::ProvenBoundExceedsBudget,
            severity: Severity::Critical,
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
            override_record: None,
        },
    }
}

/// One [`ApplyBudgetTrip`] — see this module's "Judgment calls" item 6.
fn apply_budget_trip_finding(trip: &ApplyBudgetTrip) -> HealthFinding {
    let metric = match trip.dimension {
        ApplyDimension::DecodedPaths => Metric::ProposalPathCount,
        ApplyDimension::Candidates => Metric::ProposalCandidateCount,
    };
    HealthFinding {
        code: FindingCode::ResourceBudgetReached,
        severity: Severity::Critical,
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
        override_record: None,
    }
}

/// The evaluator (design.md's own brief: "`fn evaluate_health(...) -> HealthReport`"): turns every
/// available compile measurement into [`HealthFinding`]s and returns the aggregated
/// [`HealthReport`] — call [`HealthReport::admission`] on the result for CONTEXT.md's `FST
/// admission result` (unmodified, never re-derived here).
///
/// Every parameter is optional/empty-by-default so a caller with only some measurements (e.g. just
/// a payload size, no compose-budget trips) still gets a valid report — this module's own
/// `fst_health_evaluator_empty_report_is_ideal` test pins the all-`None`/all-empty case.
///
/// - `payload_bytes`: the final FST payload's byte count, if known.
/// - `emit_report`: `crate::emit::emit`/`emit_with_budget`'s own [`EmitReport`], if this
///   compilation went through that path.
/// - `compose_errors`: every [`ComposeError`] this compilation's checked compose/union/minimize/
///   chain-depth/ordering-multiplicity calls raised (typically zero or one per grammar, but a
///   caller collecting evidence across a batch or a diagnostic sweep may pass more than one).
/// - `apply_budget_trips`: every per-word [`ApplyBudgetTrip`] this compilation's callers observed.
/// - `compile_profile`: `openspec/changes/profile-fst-compilation`'s own [`CompileProfile`], if this
///   compilation collected one (`crate::analyzer::FomaProposer::new_with_profile`) — see
///   [`profile_findings`]'s own doc for exactly which finding kinds this populates, and the Phase B
///   gate it enforces on a non-production-labeled profile.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::{EmitCounts, UncoveredItem};
    use std::time::Duration;

    // ---------------------------------------------------------------------------------------
    // fst_health_evaluator_size_bands: payload-size-only inputs, every severity band.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn fst_health_evaluator_ideal_payload_produces_no_finding() {
        let report = evaluate_health(Some(10_000_000), None, &[], &[], None);
        assert!(report.findings.is_empty());
        assert_eq!(report.admission(), Severity::Ideal);
    }

    #[test]
    fn fst_health_evaluator_warning_payload_produces_payload_size_band_finding() {
        let report = evaluate_health(Some(50_000_000), None, &[], &[], None);
        assert_eq!(report.findings.len(), 1);
        let finding = &report.findings[0];
        assert_eq!(finding.code, FindingCode::PayloadSizeBand);
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.metric, Metric::PayloadBytes);
        assert_eq!(finding.value, MetricValue::Bytes(50_000_000));
        assert_eq!(finding.threshold, Some(MetricValue::Bytes(20_000_000)));
        assert_eq!(finding.provenance, ValueProvenance::Observed);
        assert_eq!(report.admission(), Severity::Warning);
    }

    #[test]
    fn fst_health_evaluator_error_payload_matches_health_schema_worked_scenario() {
        // crate::health's own worked scenario: "FST payload is exactly 100,000,000 bytes" -> Warning
        // is the UPPER edge of Warning; one byte more crosses into Error.
        let report = evaluate_health(Some(100_000_001), None, &[], &[], None);
        assert_eq!(report.findings[0].severity, Severity::Error);
        assert_eq!(
            report.findings[0].threshold,
            Some(MetricValue::Bytes(100_000_000))
        );
    }

    #[test]
    fn fst_health_evaluator_critical_payload_is_critical_and_overridable() {
        let report = evaluate_health(Some(500_000_001), None, &[], &[], None);
        assert_eq!(report.findings[0].severity, Severity::Critical);
        assert!(report.findings[0].severity.overridable());
        assert_eq!(report.admission(), Severity::Critical);
    }

    // ---------------------------------------------------------------------------------------
    // fst_health_evaluator_emit_report: FomaTier + enum-budget-exceeded mapping.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn fst_health_evaluator_full_tier_produces_no_finding() {
        let report = EmitReport {
            uncovered: Vec::new(),
            counts: EmitCounts::default(),
            tier: FomaTier::Full,
            enum_budget_exceeded: None,
        };
        let health = evaluate_health(None, Some(&report), &[], &[], None);
        assert!(health.findings.is_empty());
        assert_eq!(health.admission(), Severity::Ideal);
    }

    #[test]
    fn fst_health_evaluator_partial_tier_is_warning_not_critical() {
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
        };
        let health = evaluate_health(None, Some(&report), &[], &[], None);
        assert_eq!(health.findings.len(), 1);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::UnknownUnboundedConstruct);
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.value, MetricValue::Count(2));
        assert_eq!(
            finding.affected,
            vec!["mrule12#allo0".to_string(), "mrule13#allo0".to_string()]
        );
        assert_eq!(health.admission(), Severity::Warning);
    }

    #[test]
    fn fst_health_evaluator_unsupported_tier_is_critical() {
        let report = EmitReport {
            uncovered: Vec::new(),
            counts: EmitCounts::default(),
            tier: FomaTier::Unsupported {
                reason: "zero root allomorphs survived synthetic pre-filtering".to_string(),
            },
            enum_budget_exceeded: None,
        };
        let health = evaluate_health(None, Some(&report), &[], &[], None);
        assert_eq!(health.findings.len(), 1);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::UnknownUnboundedConstruct);
        assert_eq!(finding.severity, Severity::Critical);
        assert_eq!(finding.value, MetricValue::Unbounded);
        assert_eq!(health.admission(), Severity::Critical);
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
        };
        let health = evaluate_health(None, Some(&report), &[], &[], None);
        // One finding for the Unsupported tier, one for the specific tripped budget.
        assert_eq!(health.findings.len(), 2);
        let budget_finding = health
            .findings
            .iter()
            .find(|f| f.code == FindingCode::ResourceBudgetReached)
            .expect("enum budget finding present");
        assert_eq!(budget_finding.severity, Severity::Critical);
        assert_eq!(budget_finding.value, MetricValue::Count(5_001));
        assert_eq!(budget_finding.threshold, Some(MetricValue::Count(5_000)));
        assert_eq!(health.admission(), Severity::Critical);
    }

    // ---------------------------------------------------------------------------------------
    // fst_health_evaluator_compose_errors: every ComposeError variant maps to a finding.
    // ---------------------------------------------------------------------------------------

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
        assert_eq!(health.admission(), Severity::Critical);
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

    // ---------------------------------------------------------------------------------------
    // fst_health_evaluator_apply_budget_trips
    // ---------------------------------------------------------------------------------------

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

    // ---------------------------------------------------------------------------------------
    // fst_health_evaluator_empty_report_is_ideal
    // ---------------------------------------------------------------------------------------

    #[test]
    fn fst_health_evaluator_empty_report_is_ideal() {
        let health = evaluate_health(None, None, &[], &[], None);
        assert!(health.findings.is_empty());
        assert_eq!(health.admission(), Severity::Ideal);
        assert_eq!(health.schema_version, crate::health::HEALTH_SCHEMA_VERSION);
    }

    // ---------------------------------------------------------------------------------------
    // fst_health_evaluator_profile: `openspec/changes/profile-fst-compilation` -- the two
    // previously-unpopulated finding kinds (IntermediateNetworkGrowth, CompileWorkBudget) now
    // populate from `crate::profile::CompileProfile`, and the Phase B `ProfileLabel` gate.
    // ---------------------------------------------------------------------------------------

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

    /// Before this change, NOTHING could ever produce an `IntermediateNetworkGrowth`/
    /// `CompileWorkBudget` finding for an "approaching, not yet tripped" grammar -- this evaluator's
    /// own module doc used to list both as deferred. A profile whose values sit comfortably below
    /// the 80% threshold must still produce nothing (Ideal), proving the new code path is real
    /// gating, not an unconditional finding.
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
        assert_eq!(health.admission(), Severity::Ideal);
    }

    /// The real case this change exists for: a profile whose final network state count sits at 90%
    /// of `DEFAULT_STATE_BUDGET` -- a case that produced NO finding before this change (no code path
    /// could see this measurement at all) now produces a real, Observed
    /// `IntermediateNetworkGrowth`/`IntermediateStateCount` Warning finding with the exact measured
    /// value and the calibrated budget as its threshold.
    #[test]
    fn fst_health_evaluator_profile_network_growth_approaching_budget_produces_warning() {
        let states = (DEFAULT_STATE_BUDGET as f64 * 0.9) as i64;
        let profile = synthetic_profile(ProfileLabel::Production, Some(states), None, None);
        let health = evaluate_health(None, None, &[], &[], Some(&profile));
        assert_eq!(health.findings.len(), 1);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::IntermediateNetworkGrowth);
        assert_eq!(finding.metric, Metric::IntermediateStateCount);
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.provenance, ValueProvenance::Observed);
        assert_eq!(finding.value, MetricValue::Count(states as u64));
        assert_eq!(
            finding.threshold,
            Some(MetricValue::Count(DEFAULT_STATE_BUDGET as u64))
        );
        assert_eq!(health.admission(), Severity::Warning);
    }

    /// Same shape, the arc-count dimension.
    #[test]
    fn fst_health_evaluator_profile_arc_growth_approaching_budget_produces_warning() {
        let arcs = (DEFAULT_ARC_BUDGET as f64 * 0.85) as i64;
        let profile = synthetic_profile(ProfileLabel::Production, None, Some(arcs), None);
        let health = evaluate_health(None, None, &[], &[], Some(&profile));
        assert_eq!(health.findings.len(), 1);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::IntermediateNetworkGrowth);
        assert_eq!(finding.metric, Metric::IntermediateArcCount);
        assert_eq!(finding.severity, Severity::Warning);
    }

    /// The total-emitted-lexc-lines dimension -- `CompileWorkBudget`, the OTHER
    /// previously-unpopulated finding kind.
    #[test]
    fn fst_health_evaluator_profile_compile_work_lines_approaching_budget_produces_warning() {
        let lines = (DEFAULT_LINE_BUDGET as f64 * 0.95) as u64;
        let profile = synthetic_profile(ProfileLabel::Production, None, None, Some(lines));
        let health = evaluate_health(None, None, &[], &[], Some(&profile));
        assert_eq!(health.findings.len(), 1);
        let finding = &health.findings[0];
        assert_eq!(finding.code, FindingCode::CompileWorkBudget);
        assert_eq!(finding.metric, Metric::EmittedLineCount);
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.value, MetricValue::Count(lines));
        assert_eq!(
            finding.threshold,
            Some(MetricValue::Count(DEFAULT_LINE_BUDGET as u64))
        );
    }

    /// Phase B gate (design.md D1; spec.md "Experimental cascade is profiled early"): a profile
    /// labeled `ExperimentalComposition` must be refused OUTRIGHT by `profile_findings`/
    /// `evaluate_health`, even when its own values would otherwise trip every dimension at once --
    /// proving this is a hard label check, not merely "these particular synthetic values happen not
    /// to cross the threshold."
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
        assert_eq!(health.admission(), Severity::Ideal);
    }

    // ---------------------------------------------------------------------------------------
    // fst_health_evaluator_golden: a representative multi-source compile, byte-for-byte golden.
    // ---------------------------------------------------------------------------------------

    /// One representative compile's measurements: a Warning-band payload, a Partial-tier emit
    /// report with one uncovered construct, and one compile-time net-size budget trip -- three
    /// distinct measurement sources feeding one report, the shape a real caller (e.g.
    /// `pangloss fst-health`, once task 3.1 lands) assembles.
    fn representative_inputs() -> (u64, EmitReport, ComposeError) {
        let payload_bytes = 25_000_000u64; // Warning band
        let emit_report = EmitReport {
            uncovered: vec![UncoveredItem {
                kind: "process-morph".to_string(),
                id: "mrule0007#allo0".to_string(),
                reason: "synthetic non-concatenative process morph".to_string(),
            }],
            counts: EmitCounts::default(),
            tier: FomaTier::Partial { uncovered: 1 },
            enum_budget_exceeded: None,
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
  "schema_version": 1,
  "findings": [
    {
      "code": "PGF0001",
      "severity": "warning",
      "phase": "compile",
      "affected": [],
      "metric": "payload_bytes",
      "value": {
        "kind": "bytes",
        "value": 25000000
      },
      "provenance": "observed",
      "threshold": {
        "kind": "bytes",
        "value": 20000000
      },
      "explanation": "Final FST payload is 25000000 bytes, in the Warning band (R6 decimal-byte size thresholds).",
      "remedies": []
    },
    {
      "code": "PGF0007",
      "severity": "warning",
      "phase": "compile",
      "affected": [
        "mrule0007#allo0"
      ],
      "metric": "unknown_unbounded_work",
      "value": {
        "kind": "count",
        "value": 1
      },
      "provenance": "observed",
      "explanation": "1 construct occurrence(s) could not be represented in this FST-propose network and contribute no candidates for it; this build emitted no incorrect entries, but recall for analyses that depend on these occurrences relies on this grammar's other analysis path(s).",
      "remedies": []
    },
    {
      "code": "PGF0008",
      "severity": "critical",
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
    fn fst_health_evaluator_golden_admission_is_critical() {
        let (payload_bytes, emit_report, compose_error) = representative_inputs();
        let health = evaluate_health(
            Some(payload_bytes),
            Some(&emit_report),
            std::slice::from_ref(&compose_error),
            &[],
            None,
        );
        // Two Warning findings + one Critical (non-overridden) -> admission is Critical.
        assert_eq!(health.admission(), Severity::Critical);
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
