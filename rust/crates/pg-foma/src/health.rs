//! Stage 0D of `openspec/changes/define-fst-compilation-health` (design.md, R6 —
//! `openspec/changes/IMPLEMENTATION-READINESS.md` §R6): the Rust-owned FST compilation-health
//! **finding schema** — types, stable codes, severity/override semantics, size bands, and
//! canonical JSON.
//!
//! **Purely additive.** This module defines and unit-tests the schema only. It does **not**
//! instrument any compiler pass and is not consulted by `emit.rs`/`gate.rs`/`replace.rs`/
//! `preexpand.rs`/`compose_budget.rs` — the same "define the data type, wire it up later" shape
//! `crate::plan`/`crate::capability` use for their own Step 1s. Per
//! `IMPLEMENTATION-READINESS.md`: "FST health policy/schema may land before instrumentation;
//! observed audit fields populate as their owning profile/budget changes merge and are never
//! independently remeasured." A later change wires a real evaluator that reads
//! `crate::compose_budget`/`crate::morphotactics::EnumerationBudget` measurements and produces
//! [`HealthFinding`]s from them.
//!
//! # Two distinct axes (do not conflate)
//! CONTEXT.md's `FST compilation health` / `FST admission result` glossary entries and ADR 0005
//! ("Distinct axis from cost/health") are explicit that this module's severity axis
//! ([`Severity`]: Ideal/Info/Warning/Error/Critical — a **cost/size** axis) is a *different*
//! dimension from the capability-trust axis (ADR 0001's characteristics-check hard-fail / ADR
//! 0005's capability override, binary proven-vs-unproven). A pack can be cost-healthy yet
//! capability-unproven, or vice versa — this module models only the cost/health axis. The
//! [`OverrideRecord`] on a [`HealthFinding`] is the ADR-0005 capability override *as it bears on
//! one health finding* (a severity that would otherwise gate publication was force-compiled
//! through), not a re-implementation of the capability registry itself.
//!
//! # Severity and size bands (R6)
//! [`severity_for_size_bytes`] implements R6's exact decimal-byte FST-payload bands: Ideal
//! `<=10_000_000`; Info `>10_000_000..=20_000_000`; Warning `>20_000_000..=100_000_000`; Error
//! `>100_000_000..=500_000_000`; Critical `>500_000_000`. Size is one dimension among several —
//! see [`Metric`] for the others R6 names (compile work, intermediate nets, candidates, paths,
//! application time, unknown/unbounded constructs) — and [`HealthReport::admission`] aggregates
//! across all of them, not size alone.
//!
//! # Override policy (R6's corrected architecture — read this before trusting spec.md's prose)
//! `specs/fst-compilation-health-contract/spec.md`'s "Overrides are explicit and bounded"
//! requirement and this change's own `design.md` predate a since-corrected architecture decision:
//! both prose sources say Critical "SHALL NOT be overridable". **R6 and CONTEXT.md's `FST
//! admission result` entry supersede that**: "Both Error and Critical are overridable via the
//! ADR 0005 capability override ... the trust axis is binary and the only non-overridable floor
//! is ADR 0003 apply-time execution containment, never a predicted size verdict." This module
//! follows R6/CONTEXT: [`Severity::overridable`] returns `true` for *both* [`Severity::Error`]
//! and [`Severity::Critical`]. Nothing here re-opens `specs/**/spec.md` — that prose reconciliation
//! is out of this change's scope — but the *types* implement the corrected rule, per this
//! change's own task brief.
//!
//! # Worst non-overridden severity ("FST admission result")
//! [`HealthReport::admission`] is CONTEXT.md's `FST admission result`: the worst severity among
//! findings that do **not** carry an [`OverrideRecord`]. An overridden Critical finding is
//! permanently recorded (the [`OverrideRecord`] itself, forever attached to that finding) but does
//! **not** dominate a lower, non-overridden severity elsewhere in the same report — the loud
//! safety signal for an override is the separate ADR-0005 degraded-trust broadcast (pack-level
//! `unproven` + per-result flag), not this report's admission severity.
//!
//! # Cost uncertainty is not itself Critical
//! [`ValueProvenance`] and [`MetricValue::Unbounded`] encode CONTEXT.md's `Cost uncertainty` /
//! R6's "Unknown cost is not itself Critical when construction is recall-preserving" directly:
//! an `Unbounded` value with [`ValueProvenance::Predicted`] is diagnostic evidence only and cannot
//! by itself justify [`Severity::Critical`] — only an actual observed [`Metric::ResourceBudget`]-
//! style outcome (a [`FindingCode::ResourceBudgetReached`] finding, [`ValueProvenance::Observed`])
//! or a [`ValueProvenance::ProvenBound`] that cannot fit the remaining budget
//! ([`FindingCode::ProvenBoundExceedsBudget`]) does. This module records the distinction; it does
//! not enforce it at construction time (no compiler pass populates these types yet — see the
//! module doc's "purely additive" note), so a caller-supplied `HealthFinding` is still free-form
//! data. A later evaluator is where this policy becomes load-bearing.
//!
//! # Finding codes
//! [`FindingCode`] is the immutable `PGFdddd` registry design.md requires ("codes never renumber
//! after publication"). [`FindingCode::ALL`] plus [`FindingCode::code`]/[`FindingCode::meaning`]
//! are the registry; [`FindingCode::from_code`] is the reverse lookup used by
//! `Deserialize`. Every `match` over [`FindingCode`]/[`Severity`]/[`Phase`]/[`Metric`]/
//! [`ValueProvenance`] in this file has **no catch-all arm** — the same closed-enum discipline
//! `crate::plan`/`crate::capability` document for their own enums — so adding a variant breaks
//! this module's build until every site is updated.
//!
//! # Canonical JSON
//! [`HealthReport::to_json`]/[`HealthReport::from_json`] are this schema's canonical
//! machine-readable form (design.md: "Canonical JSON is the source artifact; Markdown is a
//! rendering of the same findings" — no Markdown renderer exists in this purely-additive step).
//! Pretty-printed with two-space indentation and struct fields in Rust declaration order (serde's
//! default, unmodified), mirroring `pg-snapshot`'s own determinism convention.
//!
//! # Judgment calls flagged for review
//! - **[`FindingCode`]'s ten codes** are this step's registry, chosen to cover every dimension
//!   R6/spec.md name (payload size, intermediate networks, compile work, proposal volume,
//!   confirmation work, duplicate-analysis overlap, unknown/unbounded cost, a terminal
//!   budget-reached outcome, a proven-bound rejection, and apply-time work) without inventing
//!   per-construct codes no instrumentation exists to emit yet. Growing this list is additive
//!   (new codes only ever append; no code is ever renumbered or removed).
//! - **[`Phase`]** has three values (`Preflight`, `Compile`, `Apply`) rather than literally the
//!   two words design.md's prose uses ("preflight/observed phase"): `Compile` and `Apply` are
//!   R6's own two production phases (compile-time construction vs. per-word application), and
//!   `Preflight` is the characteristics-profile-style prediction stage that runs before either.
//!   "Observed" is not a `Phase` value here — it is [`ValueProvenance::Observed`], the axis
//!   distinguishing predicted/proven-bound/measured values *within* a phase.
//! - **[`OverrideRecord`] carries no timestamp type** (a plain caller-supplied `recorded_at:
//!   String`) to avoid adding a date/time dependency to this crate for a purely additive schema
//!   step; a later change may tighten this to a real timestamp type if one is already available
//!   workspace-wide by then.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// This schema's own version, written into every [`HealthReport`] (design.md task 1.3:
/// "schema-versioned canonical JSON"). Bump only on a wire-incompatible change to this module's
/// types.
pub const HEALTH_SCHEMA_VERSION: u32 = 1;

// =================================================================================================
// Severity + size bands (R6)
// =================================================================================================

/// The cost/health severity axis (R6; CONTEXT.md `FST compilation health`) — deliberately
/// **distinct** from the capability-trust axis (ADR 0001/0005's proven-vs-unproven). Declaration
/// order is worst-last and is what [`Ord`] and [`HealthReport::admission`]'s `max` rely on: `Ideal
/// < Info < Warning < Error < Critical`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Within every band; nothing to report.
    Ideal,
    /// Above the Ideal band but not yet action-worthy.
    Info,
    /// Action-worthy but does not block publication.
    Warning,
    /// Requires an explicit, recorded [`OverrideRecord`] (R6-corrected: overridable, not a hard
    /// refusal) before the compilation/artifact may publish.
    Error,
    /// The worst band. R6-corrected: still overridable via the same ADR-0005 capability override
    /// as [`Severity::Error`] — see this module's doc "Override policy" section. The only
    /// non-overridable floor is ADR 0003 apply-time containment, which is not a `Severity` value
    /// at all (it is a runtime containment outcome, not a predicted health verdict).
    Critical,
}

impl Severity {
    /// R6-corrected override policy: **both** [`Severity::Error`] and [`Severity::Critical`] are
    /// overridable via the ADR-0005 capability override; [`Severity::Warning`] and below never
    /// need one (design.md: "Warning and below publish normally"). No catch-all arm.
    pub const fn overridable(self) -> bool {
        match self {
            Severity::Ideal | Severity::Info | Severity::Warning => false,
            Severity::Error | Severity::Critical => true,
        }
    }
}

/// R6's exact decimal-byte FST-payload size bands, inclusive upper edges as specified:
/// Ideal `<=10_000_000`; Info `>10_000_000..=20_000_000`; Warning `>20_000_000..=100_000_000`;
/// Error `>100_000_000..=500_000_000`; Critical `>500_000_000`. Spec.md's own worked scenario
/// ("FST payload is exactly 100,000,000 bytes" -> Warning) is pinned by this function's test.
///
/// Size is one health dimension, not the whole story — R6: "Size is one dimension; compile work,
/// intermediate nets, candidates, paths, time, and unknown/unbounded constructs may raise
/// severity." Combine this with other dimensions' findings via [`HealthReport::admission`], never
/// use this function's result alone as overall admission.
pub const fn severity_for_size_bytes(bytes: u64) -> Severity {
    const IDEAL_MAX: u64 = 10_000_000;
    const INFO_MAX: u64 = 20_000_000;
    const WARNING_MAX: u64 = 100_000_000;
    const ERROR_MAX: u64 = 500_000_000;
    if bytes <= IDEAL_MAX {
        Severity::Ideal
    } else if bytes <= INFO_MAX {
        Severity::Info
    } else if bytes <= WARNING_MAX {
        Severity::Warning
    } else if bytes <= ERROR_MAX {
        Severity::Error
    } else {
        Severity::Critical
    }
}

// =================================================================================================
// Phase, Metric, ValueProvenance, MetricValue
// =================================================================================================

/// Which production stage a [`HealthFinding`] was produced in or predicted for. See this module's
/// doc "Judgment calls" note for why this has three values rather than literally
/// design.md's "preflight/observed" pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Predicted before any construction begins (characteristics-profile-style projection).
    Preflight,
    /// During or immediately after compile-time FST construction.
    Compile,
    /// During or immediately after per-word application (propose + HermitCrab confirm, or
    /// HermitCrab-only analysis).
    Apply,
}

/// The specific measured or predicted quantity a [`HealthFinding`] reports (R6's named
/// dimensions: "compile work, intermediate nets, candidates, paths, time, and unknown/unbounded
/// constructs"; spec.md's "Proposal and confirmation work" and "Compilation health is not grammar
/// quality" requirements). The finding's [`FindingCode`] names *why* it was raised; `Metric` names
/// *what was measured*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Final FST payload size, in bytes (decimal, matching [`severity_for_size_bytes`]).
    PayloadBytes,
    /// An intermediate composition/union/minimize product's state count (`Fsm::statecount`,
    /// mirroring `crate::compose_budget::DEFAULT_STATE_BUDGET`'s own dimension).
    IntermediateStateCount,
    /// An intermediate product's arc count (`Fsm::arccount`).
    IntermediateArcCount,
    /// Alpha-tuple assignment count for a rewrite-rule subset before per-tuple compilation.
    AlphaTupleCount,
    /// Gated-partition group count.
    GateGroupCount,
    /// Emitted lexc line count.
    EmittedLineCount,
    /// Wall-clock or logical elapsed compile time, in milliseconds.
    ElapsedMillis,
    /// FST-propose candidate count for one word or one compilation-wide sample.
    ProposalCandidateCount,
    /// FST-propose path count.
    ProposalPathCount,
    /// HermitCrab confirmation attempt count.
    ConfirmationCount,
    /// Rejection share: confirmed / proposed, reported as a [`MetricValue::Ratio`].
    RejectionShare,
    /// Pre-dedup duplicate analysis count (spec.md: "24 copies of the same structured analysis").
    DuplicateAnalysisCount,
    /// Pre-dedup duplicate analysis ratio, reported as a [`MetricValue::Ratio`].
    DuplicateAnalysisRatio,
    /// Apply-time derivation/unapplication chain depth (ADR 0003's stack-overflow dimension).
    ApplyChainDepth,
    /// Apply-time reserved allocation/logical-memory budget, in bytes (ADR 0003's OOM dimension).
    ApplyAllocationBytes,
    /// A construct whose cost cannot be bounded ahead of time (CONTEXT.md `Cost uncertainty`);
    /// paired with [`MetricValue::Unbounded`] and [`ValueProvenance::Predicted`].
    UnknownUnboundedWork,
}

/// Whether a [`HealthFinding`]'s [`MetricValue`] is a heuristic estimate, a trustworthy proof, or
/// an actual post-hoc measurement (CONTEXT.md `Proven work bound`; spec.md scenarios "A trusted
/// bound exceeds the remaining budget" vs. "A large value is only a heuristic estimate").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueProvenance {
    /// A heuristic estimate: diagnostic evidence only, never by itself a rejection proof (spec.md:
    /// "it may raise a finding but cannot by itself prevent an attempted budgeted compilation").
    Predicted,
    /// An exact value or a conservative mathematical lower bound, sound enough to prove an
    /// operation cannot fit the remaining budget (spec.md: "compilation stops before that
    /// operation").
    ProvenBound,
    /// An actual measured value from a completed (possibly budget-terminated) attempt.
    Observed,
}

/// A finding's measured/predicted value, or [`MetricValue::Unbounded`] when the compiler cannot
/// state one (CONTEXT.md `Cost uncertainty`). Adjacently tagged (`"kind"`/`"value"`) so
/// [`MetricValue::Unbounded`] serializes as `{"kind":"unbounded"}` with no dangling `null` value
/// field.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum MetricValue {
    /// A plain count (candidates, paths, confirmations, duplicates, states, arcs, ...).
    Count(u64),
    /// A byte quantity (payload size, reserved allocation, ...).
    Bytes(u64),
    /// A millisecond duration.
    Millis(u64),
    /// A dimensionless ratio (rejection share, duplicate ratio, ...), `0.0..=1.0` by convention
    /// but not enforced by this type.
    Ratio(f64),
    /// Cost uncertainty: no bound is available at all (see [`ValueProvenance::Predicted`] paired
    /// with this — R6: "Unknown cost is not itself Critical when construction is
    /// recall-preserving").
    Unbounded,
}

// =================================================================================================
// FindingCode registry
// =================================================================================================

/// The immutable `PGFdddd` finding-code registry (design.md: "Finding codes use `PGF` plus four
/// decimal digits and never change meaning after publication"). Closed on purpose — see this
/// module's doc "Judgment calls" note for what each code covers and why this set was chosen for
/// this schema-only step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FindingCode {
    /// Final FST payload size crossed a severity band ([`severity_for_size_bytes`]).
    PayloadSizeBand,
    /// An intermediate composition/union/minimize product grew large relative to its budget.
    IntermediateNetworkGrowth,
    /// Compile-time logical construction work (states/arcs/tuples/groups/lines) approached or
    /// reached its budget.
    CompileWorkBudget,
    /// FST-propose candidate or path volume is large, independent of final correctness or size
    /// (spec.md: "A compact FST proposes excessive candidates").
    ProposalVolume,
    /// HermitCrab confirmation count, rejection share, or confirmation work is large.
    ConfirmationWork,
    /// Pre-dedup duplicate analysis count/ratio with rule or proposal-path provenance, when
    /// available (spec.md: "Overlapping rules produce many identical analyses").
    DuplicateAnalysisOverlap,
    /// A recall-preserving construct's cost cannot be bounded ahead of time (cost uncertainty; not
    /// itself Critical — R6).
    UnknownUnboundedConstruct,
    /// A compilation attempt reached an enforced logical/byte/time budget and stopped with a typed
    /// resource finding (spec.md: "Unknown growth reaches a resource limit").
    ResourceBudgetReached,
    /// An exact value or proven conservative lower bound shows an operation cannot fit the
    /// remaining budget; compilation stopped before that operation (spec.md: "A trusted bound
    /// exceeds the remaining budget").
    ProvenBoundExceedsBudget,
    /// Per-word apply-time work (chain depth, allocation, elapsed time) is elevated (ADR 0003's
    /// dimensions).
    ApplicationTimeWork,
}

impl FindingCode {
    /// Every registered code, in registry order. The single source of truth every registry test
    /// (uniqueness, format, round trip) iterates.
    pub const ALL: &'static [FindingCode] = &[
        FindingCode::PayloadSizeBand,
        FindingCode::IntermediateNetworkGrowth,
        FindingCode::CompileWorkBudget,
        FindingCode::ProposalVolume,
        FindingCode::ConfirmationWork,
        FindingCode::DuplicateAnalysisOverlap,
        FindingCode::UnknownUnboundedConstruct,
        FindingCode::ResourceBudgetReached,
        FindingCode::ProvenBoundExceedsBudget,
        FindingCode::ApplicationTimeWork,
    ];

    /// The immutable `PGFdddd` wire code. Exhaustive match, no catch-all arm — adding a variant
    /// breaks this build until it is given a code here.
    pub const fn code(self) -> &'static str {
        match self {
            FindingCode::PayloadSizeBand => "PGF0001",
            FindingCode::IntermediateNetworkGrowth => "PGF0002",
            FindingCode::CompileWorkBudget => "PGF0003",
            FindingCode::ProposalVolume => "PGF0004",
            FindingCode::ConfirmationWork => "PGF0005",
            FindingCode::DuplicateAnalysisOverlap => "PGF0006",
            FindingCode::UnknownUnboundedConstruct => "PGF0007",
            FindingCode::ResourceBudgetReached => "PGF0008",
            FindingCode::ProvenBoundExceedsBudget => "PGF0009",
            FindingCode::ApplicationTimeWork => "PGF0010",
        }
    }

    /// A one-line, stable meaning for this code (design.md: "each code carries its meaning").
    /// Exhaustive match, no catch-all arm.
    pub const fn meaning(self) -> &'static str {
        match self {
            FindingCode::PayloadSizeBand => {
                "Final FST payload size crossed a severity band (R6 decimal-byte thresholds)."
            }
            FindingCode::IntermediateNetworkGrowth => {
                "An intermediate composition/union/minimize product grew large relative to its \
                 budget."
            }
            FindingCode::CompileWorkBudget => {
                "Compile-time logical construction work (states/arcs/tuples/groups/lines) \
                 approached or reached its budget."
            }
            FindingCode::ProposalVolume => {
                "FST-propose candidate or path volume is large, independent of final correctness \
                 or size."
            }
            FindingCode::ConfirmationWork => {
                "HermitCrab confirmation count, rejection share, or confirmation work is large."
            }
            FindingCode::DuplicateAnalysisOverlap => {
                "Pre-dedup duplicate analysis count/ratio with rule or proposal-path provenance, \
                 when available."
            }
            FindingCode::UnknownUnboundedConstruct => {
                "A recall-preserving construct's cost cannot be bounded ahead of time (cost \
                 uncertainty, not itself Critical)."
            }
            FindingCode::ResourceBudgetReached => {
                "A compilation attempt reached an enforced logical/byte/time budget and stopped \
                 with a typed resource finding."
            }
            FindingCode::ProvenBoundExceedsBudget => {
                "An exact value or proven conservative lower bound shows an operation cannot fit \
                 in the remaining budget; compilation stopped before it."
            }
            FindingCode::ApplicationTimeWork => {
                "Per-word apply-time work (chain depth, allocation, elapsed time) is elevated."
            }
        }
    }

    /// Reverse lookup by wire code, used by [`Deserialize`]. Generic over [`FindingCode::ALL`], so
    /// there is only one hand-written code<->variant mapping ([`FindingCode::code`]) to keep in
    /// sync, not two.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.code() == code)
    }
}

impl Serialize for FindingCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for FindingCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        FindingCode::from_code(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown FST health code: {s}")))
    }
}

// =================================================================================================
// Remedy, OverrideRecord, HealthFinding, HealthReport
// =================================================================================================

/// One ranked, applicable remedy for a [`HealthFinding`] (design.md: "zero or more ranked remedy
/// records with applicability conditions"). Findings "explain computational consequences only ...
/// but never assert that such a change improves the grammar" (design.md) — a remedy that would
/// edit the grammar (reordering, constraining, decomposing a rule) must set
/// `requires_linguistic_equivalence` and SHOULD carry a `caveat` (spec.md "Compiler remedies do
/// not silently change grammar meaning").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Remedy {
    /// 1-based rank among this finding's remedies; lower ranks are recommended first.
    pub rank: u32,
    /// The remedy's computational-consequence description. Never a linguistic-quality claim.
    pub description: String,
    /// `true` when applying this remedy edits the grammar (reordering, constraining, decomposing
    /// a rule) and its safety depends on linguistic equivalence the compiler cannot verify on its
    /// own (CONTEXT.md `Semantics-preserving compiler transformation`). `false` for compiler-
    /// internal transformations with an owned correctness argument, or non-grammar-editing advice
    /// (e.g. "retry with a larger named envelope").
    pub requires_linguistic_equivalence: bool,
    /// Free-text caveat surfaced alongside the remedy when `requires_linguistic_equivalence` is
    /// `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caveat: Option<String>,
}

/// The permanent record of one ADR-0005 capability override as it bears on a single
/// [`HealthFinding`] (design.md: "Error requires an explicit override recorded in the health
/// report and package manifest"; ADR 0005: "the override is explicit and recorded ... written into
/// the pack manifest override record"). This struct is this report's own copy of that fact — the
/// authoritative, indelible pack-manifest record is a separate artifact this change does not
/// define.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverrideRecord {
    /// Who or what authorized the override (a caller identity, tool name, or operator label).
    pub authorized_by: String,
    /// Why the override was exercised.
    pub reason: String,
    /// Caller-supplied record of when the override was exercised (free-form; see this module's
    /// doc "Judgment calls" note for why this is a plain `String`, not a timestamp type).
    pub recorded_at: String,
}

/// One stable compiler diagnostic (design.md: "Each finding records code, severity,
/// preflight/observed phase, metric, predicted/observed value, effective thresholds,
/// grammar/rule/construct identifiers, concise explanation, and zero or more ranked remedy
/// records"). Every field this requirement names has a slot here; `override_record` is this
/// change's brief's explicit "override flag/field per finding".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthFinding {
    /// The immutable `PGFdddd` code (design.md/[`FindingCode`]).
    pub code: FindingCode,
    /// This finding's severity on the cost/health axis (never the capability-trust axis).
    pub severity: Severity,
    /// Which production stage produced or predicted this finding.
    pub phase: Phase,
    /// Stable grammar/rule/construct identifiers this finding is about (design.md: "affected
    /// identifiers"). Freeform stable strings (e.g. a rule/template/stratum ID as the owning
    /// grammar names it) — this schema step does not mint or constrain an ID format.
    pub affected: Vec<String>,
    /// Which quantity `value`/`threshold` measure.
    pub metric: Metric,
    /// This finding's measured or predicted value.
    pub value: MetricValue,
    /// Whether `value` is a heuristic estimate, a proven bound, or an actual observation.
    pub provenance: ValueProvenance,
    /// The effective threshold `value` is compared against, if any (some findings, e.g. cost
    /// uncertainty, have no threshold at all).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<MetricValue>,
    /// A concise, human-readable explanation of the computational consequence (never a linguistic-
    /// quality judgment — spec.md "Compilation health is not grammar quality").
    pub explanation: String,
    /// Zero or more ranked, applicable remedies.
    #[serde(default)]
    pub remedies: Vec<Remedy>,
    /// Present only when this finding's severity was explicitly overridden (ADR 0005). `None`
    /// means not overridden — including for every [`Severity::Ideal`]/[`Severity::Info`]/
    /// [`Severity::Warning`] finding, which never need one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_record: Option<OverrideRecord>,
}

/// The aggregated report for one grammar compilation (design.md: "A `HealthReport` aggregating
/// findings + the worst non-overridden severity"). See [`HealthReport::admission`] for the
/// aggregation rule and this module's doc for why an overridden Critical does not dominate a
/// lower non-overridden severity elsewhere in the same report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthReport {
    /// This schema's version ([`HEALTH_SCHEMA_VERSION`]) at the time this report was produced.
    pub schema_version: u32,
    /// Every finding for this compilation, in producer order (not sorted or deduplicated by this
    /// type).
    #[serde(default)]
    pub findings: Vec<HealthFinding>,
}

impl HealthReport {
    /// Builds a report stamped with the current [`HEALTH_SCHEMA_VERSION`].
    pub fn new(findings: Vec<HealthFinding>) -> Self {
        Self {
            schema_version: HEALTH_SCHEMA_VERSION,
            findings,
        }
    }

    /// The "FST admission result" (CONTEXT.md): the worst severity among findings that do **not**
    /// carry an [`OverrideRecord`]. An empty report, or a report whose only findings are all
    /// overridden, admits at [`Severity::Ideal`] — the override itself remains permanently
    /// attached to its finding (and, at the pack level, surfaces via the separate ADR-0005
    /// degraded-trust signal), but it never inflates this aggregation.
    pub fn admission(&self) -> Severity {
        self.findings
            .iter()
            .filter(|finding| finding.override_record.is_none())
            .map(|finding| finding.severity)
            .max()
            .unwrap_or(Severity::Ideal)
    }

    /// Canonical machine-readable form (design.md: "Canonical JSON is the source artifact").
    /// Pretty-printed, two-space indent, fields in Rust declaration order — serde's unmodified
    /// default, matching `pg-snapshot`'s own determinism convention.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Parses a report from its canonical JSON form.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------------------------
    // fst_health_size_bands: R6's exact decimal-byte thresholds, every band edge.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn fst_health_size_bands_zero_is_ideal() {
        assert_eq!(severity_for_size_bytes(0), Severity::Ideal);
    }

    #[test]
    fn fst_health_size_bands_ideal_upper_edge_inclusive() {
        assert_eq!(severity_for_size_bytes(10_000_000), Severity::Ideal);
    }

    #[test]
    fn fst_health_size_bands_info_lower_edge_exclusive_of_ideal() {
        assert_eq!(severity_for_size_bytes(10_000_001), Severity::Info);
    }

    #[test]
    fn fst_health_size_bands_info_upper_edge_inclusive() {
        assert_eq!(severity_for_size_bytes(20_000_000), Severity::Info);
    }

    #[test]
    fn fst_health_size_bands_warning_lower_edge_exclusive_of_info() {
        assert_eq!(severity_for_size_bytes(20_000_001), Severity::Warning);
    }

    #[test]
    fn fst_health_size_bands_warning_upper_edge_exactly_100_000_000_bytes() {
        // spec.md's own worked scenario: "FST payload is exactly 100,000,000 bytes" -> Warning.
        assert_eq!(severity_for_size_bytes(100_000_000), Severity::Warning);
    }

    #[test]
    fn fst_health_size_bands_error_lower_edge_exclusive_of_warning() {
        assert_eq!(severity_for_size_bytes(100_000_001), Severity::Error);
    }

    #[test]
    fn fst_health_size_bands_error_upper_edge_inclusive() {
        assert_eq!(severity_for_size_bytes(500_000_000), Severity::Error);
    }

    #[test]
    fn fst_health_size_bands_critical_lower_edge_exclusive_of_error() {
        assert_eq!(severity_for_size_bytes(500_000_001), Severity::Critical);
    }

    #[test]
    fn fst_health_size_bands_critical_far_above_floor() {
        assert_eq!(severity_for_size_bytes(u64::MAX), Severity::Critical);
    }

    // ---------------------------------------------------------------------------------------
    // fst_health_override_policy: R6-corrected Error/Critical overridability + worst-non-
    // overridden aggregation ("FST admission result").
    // ---------------------------------------------------------------------------------------

    #[test]
    fn fst_health_override_policy_error_and_critical_are_overridable() {
        // R6-corrected: NOT "Critical = no override" (see this module's doc "Override policy").
        assert!(Severity::Error.overridable());
        assert!(Severity::Critical.overridable());
    }

    #[test]
    fn fst_health_override_policy_warning_and_below_never_need_override() {
        assert!(!Severity::Ideal.overridable());
        assert!(!Severity::Info.overridable());
        assert!(!Severity::Warning.overridable());
    }

    fn synthetic_finding(severity: Severity, override_record: Option<OverrideRecord>) -> HealthFinding {
        HealthFinding {
            code: FindingCode::PayloadSizeBand,
            severity,
            phase: Phase::Compile,
            affected: vec!["synthetic-construct".to_string()],
            metric: Metric::PayloadBytes,
            value: MetricValue::Bytes(1),
            provenance: ValueProvenance::Observed,
            threshold: None,
            explanation: "synthetic test finding".to_string(),
            remedies: Vec::new(),
            override_record,
        }
    }

    fn synthetic_override() -> OverrideRecord {
        OverrideRecord {
            authorized_by: "synthetic-test-operator".to_string(),
            reason: "synthetic field-trial override".to_string(),
            recorded_at: "2026-07-24T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn fst_health_override_policy_overridden_critical_does_not_dominate_non_overridden_warning() {
        let report = HealthReport::new(vec![
            synthetic_finding(Severity::Critical, Some(synthetic_override())),
            synthetic_finding(Severity::Warning, None),
        ]);
        assert_eq!(
            report.admission(),
            Severity::Warning,
            "an overridden Critical finding must not dominate a non-overridden Warning finding \
             in the same report"
        );
    }

    #[test]
    fn fst_health_override_policy_overridden_error_does_not_dominate_non_overridden_info() {
        let report = HealthReport::new(vec![
            synthetic_finding(Severity::Error, Some(synthetic_override())),
            synthetic_finding(Severity::Info, None),
        ]);
        assert_eq!(report.admission(), Severity::Info);
    }

    #[test]
    fn fst_health_override_policy_all_findings_overridden_admits_ideal() {
        let report = HealthReport::new(vec![synthetic_finding(
            Severity::Critical,
            Some(synthetic_override()),
        )]);
        assert_eq!(report.admission(), Severity::Ideal);
    }

    #[test]
    fn fst_health_override_policy_empty_report_admits_ideal() {
        let report = HealthReport::new(Vec::new());
        assert_eq!(report.admission(), Severity::Ideal);
    }

    #[test]
    fn fst_health_override_policy_worst_non_overridden_wins_among_several() {
        let report = HealthReport::new(vec![
            synthetic_finding(Severity::Info, None),
            synthetic_finding(Severity::Error, None),
            synthetic_finding(Severity::Warning, None),
        ]);
        assert_eq!(report.admission(), Severity::Error);
    }

    // ---------------------------------------------------------------------------------------
    // fst_health_schema: code registry, golden JSON, round trip, closed-enum exhaustiveness.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn fst_health_schema_codes_are_unique_and_well_formed() {
        let mut seen = std::collections::HashSet::new();
        for code in FindingCode::ALL {
            let wire = code.code();
            assert!(wire.starts_with("PGF"), "{wire} must start with PGF");
            let digits = &wire[3..];
            assert_eq!(digits.len(), 4, "{wire} must have exactly 4 digits");
            assert!(
                digits.chars().all(|c| c.is_ascii_digit()),
                "{wire} digits must be numeric"
            );
            assert!(seen.insert(wire), "duplicate finding code {wire}");
            assert!(!code.meaning().is_empty(), "{wire} must document its meaning");
        }
    }

    #[test]
    fn fst_health_schema_from_code_round_trips_every_registered_code() {
        for code in FindingCode::ALL {
            assert_eq!(FindingCode::from_code(code.code()), Some(*code));
        }
    }

    #[test]
    fn fst_health_schema_from_code_rejects_unknown_code() {
        assert_eq!(FindingCode::from_code("PGF9999"), None);
    }

    /// Closed-enum exhaustiveness demonstration: an exhaustive `match` with **no catch-all arm**
    /// over every [`Severity`] variant, the same discipline `crate::plan`/`crate::capability`
    /// document for their own closed enums. If a variant is ever added to [`Severity`], this stops
    /// compiling until this function (and every other exhaustive match in this file) is updated.
    #[test]
    fn fst_health_schema_severity_is_closed_and_exhaustive() {
        const fn label(severity: Severity) -> &'static str {
            match severity {
                Severity::Ideal => "ideal",
                Severity::Info => "info",
                Severity::Warning => "warning",
                Severity::Error => "error",
                Severity::Critical => "critical",
            }
        }
        assert_eq!(label(Severity::Ideal), "ideal");
        assert_eq!(label(Severity::Info), "info");
        assert_eq!(label(Severity::Warning), "warning");
        assert_eq!(label(Severity::Error), "error");
        assert_eq!(label(Severity::Critical), "critical");
    }

    /// A representative, fully-populated report exercising: two findings, one Warning with a
    /// linguistic-equivalence-caveated remedy (tasks.md 3.2), one Error that carries a permanent
    /// [`OverrideRecord`] (design.md: "remains recorded in reports and packages").
    fn representative_report() -> HealthReport {
        HealthReport::new(vec![
            HealthFinding {
                code: FindingCode::IntermediateNetworkGrowth,
                severity: Severity::Warning,
                phase: Phase::Compile,
                affected: vec!["mrule:0042".to_string(), "mrule:0043".to_string()],
                metric: Metric::IntermediateStateCount,
                value: MetricValue::Count(1_250_000),
                provenance: ValueProvenance::Observed,
                threshold: Some(MetricValue::Count(1_000_000)),
                explanation: "Composing mrule:0042 with mrule:0043 produced an intermediate \
                    network of 1,250,000 states, above the 1,000,000-state compile-work band."
                    .to_string(),
                remedies: vec![Remedy {
                    rank: 1,
                    description: "Reorder mrule:0042 and mrule:0043 within their stratum."
                        .to_string(),
                    requires_linguistic_equivalence: true,
                    caveat: Some(
                        "Only applies if the two orders are linguistically equivalent; the \
                            compiler cannot verify that on its own."
                            .to_string(),
                    ),
                }],
                override_record: None,
            },
            HealthFinding {
                code: FindingCode::PayloadSizeBand,
                severity: Severity::Error,
                phase: Phase::Compile,
                affected: vec!["synthetic-stress-grammar".to_string()],
                metric: Metric::PayloadBytes,
                value: MetricValue::Bytes(150_000_000),
                provenance: ValueProvenance::Observed,
                threshold: Some(MetricValue::Bytes(100_000_000)),
                explanation: "Final FST payload is 150,000,000 bytes, in the Error band \
                    (>100,000,000..=500,000,000)."
                    .to_string(),
                remedies: vec![Remedy {
                    rank: 1,
                    description: "Retry compilation with an explicit larger named envelope."
                        .to_string(),
                    requires_linguistic_equivalence: false,
                    caveat: None,
                }],
                override_record: Some(OverrideRecord {
                    authorized_by: "ci-field-trial-operator".to_string(),
                    reason: "Field trial requested under the ADR 0005 development on-ramp."
                        .to_string(),
                    recorded_at: "2026-07-24T00:00:00Z".to_string(),
                }),
            },
        ])
    }

    const GOLDEN_JSON: &str = r#"{
  "schema_version": 1,
  "findings": [
    {
      "code": "PGF0002",
      "severity": "warning",
      "phase": "compile",
      "affected": [
        "mrule:0042",
        "mrule:0043"
      ],
      "metric": "intermediate_state_count",
      "value": {
        "kind": "count",
        "value": 1250000
      },
      "provenance": "observed",
      "threshold": {
        "kind": "count",
        "value": 1000000
      },
      "explanation": "Composing mrule:0042 with mrule:0043 produced an intermediate network of 1,250,000 states, above the 1,000,000-state compile-work band.",
      "remedies": [
        {
          "rank": 1,
          "description": "Reorder mrule:0042 and mrule:0043 within their stratum.",
          "requires_linguistic_equivalence": true,
          "caveat": "Only applies if the two orders are linguistically equivalent; the compiler cannot verify that on its own."
        }
      ]
    },
    {
      "code": "PGF0001",
      "severity": "error",
      "phase": "compile",
      "affected": [
        "synthetic-stress-grammar"
      ],
      "metric": "payload_bytes",
      "value": {
        "kind": "bytes",
        "value": 150000000
      },
      "provenance": "observed",
      "threshold": {
        "kind": "bytes",
        "value": 100000000
      },
      "explanation": "Final FST payload is 150,000,000 bytes, in the Error band (>100,000,000..=500,000,000).",
      "remedies": [
        {
          "rank": 1,
          "description": "Retry compilation with an explicit larger named envelope.",
          "requires_linguistic_equivalence": false
        }
      ],
      "override_record": {
        "authorized_by": "ci-field-trial-operator",
        "reason": "Field trial requested under the ADR 0005 development on-ramp.",
        "recorded_at": "2026-07-24T00:00:00Z"
      }
    }
  ]
}"#;

    #[test]
    fn fst_health_schema_golden_json() {
        let report = representative_report();
        let json = report.to_json().expect("serialization must succeed");
        assert_eq!(json, GOLDEN_JSON, "canonical JSON drifted from the committed golden");
    }

    #[test]
    fn fst_health_schema_round_trip() {
        let report = representative_report();
        let json = report.to_json().expect("serialization must succeed");
        let parsed = HealthReport::from_json(&json).expect("deserialization must succeed");
        assert_eq!(parsed, report, "round trip through canonical JSON must be lossless");
        assert_eq!(parsed.admission(), Severity::Warning);
    }

    #[test]
    fn fst_health_schema_golden_admission_is_warning() {
        // Confirms the golden's Error finding is overridden and therefore does not dominate --
        // the same "FST admission result" semantics `fst_health_override_policy` pins directly.
        assert_eq!(representative_report().admission(), Severity::Warning);
    }
}
