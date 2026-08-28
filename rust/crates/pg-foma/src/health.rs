//! The FST compilation-health finding schema: types, stable codes, severity, the payload-size
//! threshold, and canonical JSON.
//!
//! Health is REPORTED about a compile, never consulted during one — `crate::health_evaluator`
//! produces `HealthFinding`s from budget measurements after the fact, so no compiler pass
//! branches on anything here. Observed fields are populated by whichever pass owns the measurement
//! and are never independently remeasured.
//!
//! # Two distinct axes (do not conflate)
//! This module's severity axis (`Severity`: WithinLimits/Elevated/LargeMultiplier/
//! NotProductionReady/MachineLimit/CannotRepresent — a **cost/size** axis) is a *different*
//! dimension from capability correctness (the characteristics-check hard-fail boundary). A build
//! can be cost-healthy yet fail a capability check, or vice versa — this module models only the
//! cost/health axis; it is not an admission mechanism for capability correctness and does not
//! re-implement the capability registry.
//!
//! # Severity names the blocking TIER; FindingClass names the fact
//! `Severity` is the publication-blocking axis, not an alarm level and not itself a statement of
//! WHY: `WithinLimits`/`Elevated`/`LargeMultiplier` never block, `NotProductionReady`/
//! `MachineLimit`/`CannotRepresent` always do. Several unrelated causes converge on the same
//! blocking tier -- `NotProductionReady` alone is emitted for an oversized-but-compiled payload, a
//! self-imposed budget stop with nothing built, and a build-process fault, none of which share a
//! phase or a cause. `FindingCode::class()` (`FindingClass`) is what
//! answers WHY: see `FindingClass`'s own doc for the four independent questions it distinguishes,
//! and `HealthReport::admission_by_class` for reading them separately from the plain severity max.
//!
//! # Severity and the payload-size threshold
//! `severity_for_size_bytes` compares a compiled FST payload's byte count against the single
//! `IDEAL_MAX_BYTES` threshold: at or under it, `Severity::WithinLimits`; over it,
//! `Severity::NotProductionReady`. Payload size is a post-compile MEASUREMENT, never a static
//! pre-compile analysis or containment verdict (see `Severity`'s own doc), and conflating the two
//! would be exactly the category blur this module's design exists to avoid — pinned by
//! `size_never_reports_an_analysis_verdict`.
//! The readiness failure a crossed threshold raises is wanted; the exact edge is provisional —
//! read `IDEAL_MAX_BYTES` before citing it as evidence. Size is one dimension among several — see
//! `Metric` for the others (candidates, paths, chain depth, unknown/unbounded constructs, and
//! backend coverage gaps) — and `HealthReport::admission` aggregates across all of them,
//! not size alone.
//!
//! # Admission boundary
//! `Severity::NotProductionReady` and `Severity::MachineLimit` remain explicit readiness
//! failures. Health admission always reflects raw severity. Apply-time execution containment
//! remains a hard boundary as well.
//!
//! # Worst severity ("FST admission result")
//! `HealthReport::admission` returns the worst raw finding severity — the publication gate's floor.
//! It says nothing about WHICH axis failed, so read `HealthReport::admission_by_class` whenever the
//! answer is going to be shown to someone or routed on.
//!
//! # Cost uncertainty is not itself a machine limit
//! `ValueProvenance` and `MetricValue::Unbounded` encode that unknown cost is not itself
//! `Severity::MachineLimit` when construction is recall-preserving: an `Unbounded` value with
//! `ValueProvenance::Predicted` is diagnostic evidence only and cannot by itself justify
//! `Severity::MachineLimit` — only an actual observed `Metric::ResourceBudget`-style outcome (a
//! `FindingCode::ResourceBudgetReached` finding, `ValueProvenance::Observed`) or a
//! `ValueProvenance::ProvenBound` remains diagnostic evidence about an exact value or conservative
//! lower bound. This module records the distinction; it does not enforce it at construction time,
//! so a caller-supplied `HealthFinding` is still free-form data as far as this schema is concerned
//! — `crate::health_evaluator` is where this policy becomes load-bearing.
//!
//! # Finding codes
//! `FindingCode` is the current `PGFdddd` registry: each published code keeps its meaning within
//! a schema version, while pre-1.0 schema revisions may remove producerless codes.
//! `FindingCode::ALL` plus `FindingCode::code`/`FindingCode::meaning`
//! are the registry; `FindingCode::from_code` is the reverse lookup used by
//! `Deserialize`. Every `match` over `FindingCode`/`Severity`/`Phase`/`Metric`/
//! `ValueProvenance` in this file has **no catch-all arm** — the same closed-enum discipline
//! `crate::plan`/`crate::capability` document for their own enums — so adding a variant breaks
//! this module's build until every site is updated.
//!
//! # Canonical JSON
//! `HealthReport::to_json`/`HealthReport::from_json` are this schema's canonical
//! machine-readable form: the source artifact, with any human-readable rendering (e.g. Markdown)
//! derived by consumers such as `pg-cli make-report` rather than authored independently.
//! Pretty-printed with two-space indentation and struct fields in Rust declaration order (serde's
//! default, unmodified), mirroring `pg-snapshot`'s own determinism convention.
//!
//! # Design notes
//! - `FindingCode` covers the dimensions this crate currently measures (payload size,
//!   unknown/unbounded cost, internal budget stops, backend/process
//!   failures, coverage gaps, and bounded rule-interaction products). New measured dimensions
//!   require a real producer and a versioned schema update.
//! - `Phase` has three values (`Characterization`, `Compile`, `Apply`) rather than a simpler
//!   "characterization/observed" split: `Compile` and `Apply` are the two production phases (compile-time
//!   construction vs. per-word application), and `Characterization` is the characteristics-profile-style
//!   prediction stage that runs before either. "Observed" is not a `Phase` value here — it is
//!   `ValueProvenance::Observed`, the axis distinguishing predicted/proven-bound/measured values
//!   *within* a phase.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// This schema's own version, written into every `HealthReport`. Bump only on a
/// wire-incompatible change to this module's types.
///
/// Bumped to 7 when producerless health labels and value variants were removed.
pub const HEALTH_SCHEMA_VERSION: u32 = 7;

// Severity + payload-size threshold

/// The cost/health severity axis — deliberately **distinct** from capability correctness checks.
/// This is the publication-blocking TIER, not a statement
/// of WHY: `FindingCode::class()` (`FindingClass`) is what names the fact behind a finding, and a
/// single severity here can be reached by several unrelated facts (see `NotProductionReady`'s own
/// doc for the clearest case).
///
/// - [`Severity::WithinLimits`] / [`Severity::Elevated`] / [`Severity::LargeMultiplier`]: an
///   analysis of magnitude — how much of something there is. Never blocks. These say nothing about
///   WHEN the quantity was learned: `ValueProvenance` carries that, so a predicted product and a
///   measured count can both be `LargeMultiplier` when both are simply too large.
/// - [`Severity::CannotRepresent`]: analysis of representability, and nothing can be built for the
///   affected feature.
/// - [`Severity::NotProductionReady`]: this tier blocks publication; see its own doc for the
///   several distinct facts (compiled-but-oversized, budget-stopped, process-faulted) that all
///   reach it.
/// - [`Severity::MachineLimit`]: external process containment fired DURING a compile and aborted
///   it; never a statement about the grammar.
///
/// Declaration order is worst-last and is what `Ord` and `HealthReport::admission`'s `max` rely
/// on: `WithinLimits < Elevated < LargeMultiplier < NotProductionReady < MachineLimit <
/// CannotRepresent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Within every band; nothing to report.
    WithinLimits,
    /// Above the within-limits band but not yet action-worthy.
    Elevated,
    /// A multiplier is too large — an N x M x O product, or a count crowding its budget. Predicted
    /// or measured alike (`ValueProvenance` says which); never blocks. Remedy: check grammar
    /// optimization.
    LargeMultiplier,
    /// The tier that blocks publication, whatever the underlying cause: an oversized-but-compiled
    /// payload, an internal-cap stop with nothing compiled, a build-process or backend-compilation
    /// fault. Must not itself block compiling. Read the finding's `FindingClass` for which of
    /// those it is.
    NotProductionReady,
    /// External process containment fired DURING a compile and aborted it. Remedy: adjust the
    /// configured execution limit, use more machine, or choose a different algorithm.
    MachineLimit,
    /// Candidates using this feature cannot be faithfully proposed, so nothing can be built for it.
    /// Remedy: implement the feature, or use the full engine.
    CannotRepresent,
}

/// The single payload-size threshold `severity_for_size_bytes` applies.
///
/// **The warning this threshold raises is real; the exact number is provisional.** A compiled
/// grammar that runs to a gigabyte is not something anyone can ship, so a payload that large has
/// to reach its author as a warning — that much is settled, and it is why this is a threshold and
/// not just a reported number. What is unsettled is where the edge sits: no grammar was measured
/// to pick it, and the change whose job was to derive such a threshold from evidence was retired
/// without producing one.
///
/// It encodes an intent. A grammar is on the order of a thousand parameters, so the whole
/// difficulty is combining them compactly — which is exactly what different backends do better or
/// worse. Read a crossed threshold as "this backend did not combine this grammar well", never as a
/// proven resource limit. Provenance and the pending recalibration against a real spread across
/// backends and grammars: `docs/change-retirement-grills.md`.
pub const IDEAL_MAX_BYTES: u64 = 100_000_000;

/// A compiled FST payload's byte count, compared against the single [`IDEAL_MAX_BYTES`]
/// threshold — a stated target, NOT a measured limit; read [`IDEAL_MAX_BYTES`] before citing it
/// as evidence. Pinned by this function's tests so the constant and this mapping cannot drift
/// apart silently.
///
/// This is a post-compile MEASUREMENT of an artifact that already compiled successfully, never a
/// pre-compile static-analysis verdict, so only [`Severity::WithinLimits`] and
/// [`Severity::NotProductionReady`] are possible outputs: no size input may ever produce
/// [`Severity::Elevated`], [`Severity::LargeMultiplier`], [`Severity::MachineLimit`], or
/// [`Severity::CannotRepresent`] — those verdicts belong to other producers (static analysis or
/// containment), not to a payload byte count.
///
/// Size is one health dimension, not the whole story: compile work, intermediate nets,
/// candidates, paths, time, and unknown/unbounded constructs may also raise severity. Combine
/// this with other dimensions' findings via `HealthReport::admission`, never use this
/// function's result alone as overall admission.
pub const fn severity_for_size_bytes(bytes: u64) -> Severity {
    if bytes <= IDEAL_MAX_BYTES {
        Severity::WithinLimits
    } else {
        Severity::NotProductionReady
    }
}

// Phase, Metric, ValueProvenance, MetricValue

/// Which production stage a `HealthFinding` was produced in or predicted for. See this module's
/// doc "Design notes" section for why this has three values rather than a simpler
/// "characterization/observed" pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Predicted before any construction begins (characteristics-profile-style projection).
    Characterization,
    /// During or immediately after compile-time FST construction.
    Compile,
    /// During or immediately after per-word application (propose + HermitCrab confirm, or HermitCrab-only analysis).
    Apply,
}

/// The specific measured or predicted quantity a `HealthFinding` reports. The finding's
/// `FindingCode` names *why* it was raised; `Metric` names *what was measured*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Final FST payload size, in bytes (decimal, matching `severity_for_size_bytes`).
    PayloadBytes,
    /// FST-propose candidate count for one word or one compilation-wide sample.
    ProposalCandidateCount,
    /// FST-propose path count.
    ProposalPathCount,
    /// Apply-time derivation/unapplication chain depth — an unbounded chain risks stack overflow.
    ApplyChainDepth,
    /// A construct whose cost cannot be bounded ahead of time; paired with `MetricValue::Unbounded` and `ValueProvenance::Predicted`.
    UnknownUnboundedWork,
    /// Reachable root/chain-state x morphological-rule applications that a composite-emitting
    /// backend must synthesize while proving finite closure.
    CompositeRulePairCount,
    /// Required grammar constructs or plan subtrees the named backend cannot represent completely.
    BackendCoverageGapCount,
}

/// Whether a `HealthFinding`'s `MetricValue` is a heuristic estimate, a trustworthy proof, or
/// an actual post-hoc measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueProvenance {
    /// A heuristic estimate: diagnostic evidence only, never by itself a rejection proof.
    Predicted,
    /// An exact value or conservative lower bound, sound enough to prove an operation cannot fit the remaining budget.
    ProvenBound,
    /// An actual measured value from a completed (possibly budget-terminated) attempt.
    Observed,
}

/// A finding's measured/predicted value, or `MetricValue::Unbounded` when the compiler cannot
/// state one. Adjacently tagged (`"kind"`/`"value"`) so `MetricValue::Unbounded` serializes as
/// `{"kind":"unbounded"}` with no dangling `null` value field.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum MetricValue {
    /// A plain count (candidates, paths, states, arcs, ...).
    Count(u64),
    /// A byte quantity (payload size, reserved allocation, ...).
    Bytes(u64),
    /// Cost uncertainty: no bound is available at all (paired with `ValueProvenance::Predicted`).
    Unbounded,
}

// FindingCode registry

/// The current `PGFdddd` finding-code registry: codes use `PGF` plus four decimal digits and
/// retain their meaning within a schema version. Pre-1.0 schema revisions may remove
/// producerless codes. Closed on purpose — see this module's doc "Design notes" section for what
/// each code covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FindingCode {
    /// Final FST payload size crossed the size threshold (`severity_for_size_bytes`).
    PayloadSizeBand,
    /// A recall-preserving construct's cost cannot be bounded ahead of time; not itself a MachineLimit.
    UnknownUnboundedConstruct,
    /// An INTERNAL, self-imposed compile/apply-time budget (net size, emit lines, compose
    /// timeout, chain depth, apply-time proposal/path volume) was reached and stopped this
    /// attempt.
    ResourceBudgetReached,
    /// A backend failed while compiling its emitted representation and produced no usable artifact.
    BackendCompilationFailed,
    /// Invalid build input, worker protocol failure, or a worker-process failure prevented a build.
    BuildProcessFailed,
    /// A backend is known to omit or reject one or more required grammar constructs.
    BackendCoverageIncomplete,
    /// An exact, already-computed morphological x phonological rule-count product is large.
    /// Distinct from [`FindingCode::UnknownUnboundedConstruct`]: this cost IS bounded ahead of
    /// time, just large.
    RuleInteractionProduct,
}

/// Which of the three independent admission questions a `FindingCode` answers. A finding never
/// blurs these: representability, readiness, and containment are checked separately and none may
/// stand in for another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FindingClass {
    /// Can this backend preserve every valid HermitCrab analysis? A failure here means PanGloss
    /// cannot prove a recall-preserving representation.
    Representability,
    /// Is the complete result acceptably sized/fast/maintainable to release? A failure here does
    /// not mean the grammar is unsupported.
    Readiness,
    /// Did THIS attempt stay inside its operational safety boundary? Says nothing about the
    /// language; never makes partial output usable.
    Containment,
    /// The attempt failed for a reason that is not a statement about the grammar at all (bad
    /// input, worker/protocol failure, internal compiler fault).
    Process,
}

impl FindingCode {
    /// Every registered code, in registry order. The single source of truth every registry test
    /// (uniqueness, format, round trip) iterates.
    pub const ALL: &'static [FindingCode] = &[
        FindingCode::PayloadSizeBand,
        FindingCode::UnknownUnboundedConstruct,
        FindingCode::ResourceBudgetReached,
        FindingCode::BackendCompilationFailed,
        FindingCode::BuildProcessFailed,
        FindingCode::BackendCoverageIncomplete,
        FindingCode::RuleInteractionProduct,
    ];

    /// The current `PGFdddd` wire code. Exhaustive match, no catch-all arm — adding a variant
    /// breaks this build until it is given a code here.
    pub const fn code(self) -> &'static str {
        match self {
            FindingCode::PayloadSizeBand => "PGF0001",
            FindingCode::UnknownUnboundedConstruct => "PGF0007",
            FindingCode::ResourceBudgetReached => "PGF0008",
            FindingCode::BackendCompilationFailed => "PGF0011",
            FindingCode::BuildProcessFailed => "PGF0012",
            FindingCode::BackendCoverageIncomplete => "PGF0013",
            FindingCode::RuleInteractionProduct => "PGF0015",
        }
    }

    /// A one-line, stable meaning for this code. Exhaustive match, no catch-all arm.
    pub const fn meaning(self) -> &'static str {
        match self {
            FindingCode::PayloadSizeBand => {
                "Final FST payload size crossed the size threshold (R6 decimal-byte threshold)."
            }
            FindingCode::UnknownUnboundedConstruct => {
                "A recall-preserving construct's cost cannot be bounded ahead of time (cost \
                 uncertainty, not itself a MachineLimit)."
            }
            FindingCode::ResourceBudgetReached => {
                "An internal, self-imposed compile/apply-time budget (net size, emit lines, \
                 compose timeout, chain depth, or apply-time proposal/path volume) was reached \
                 and stopped this attempt."
            }
            FindingCode::BackendCompilationFailed => {
                "A backend failed to compile its emitted representation into a usable artifact."
            }
            FindingCode::BuildProcessFailed => {
                "Invalid build input or a worker-process failure prevented any usable artifact."
            }
            FindingCode::BackendCoverageIncomplete => {
                "A backend is known to omit or reject required grammar constructs and therefore \
                 cannot produce a complete artifact."
            }
            FindingCode::RuleInteractionProduct => {
                "An exact morphological x phonological rule-count product is large; this cost is \
                 bounded ahead of time, not unknown."
            }
        }
    }

    /// Reverse lookup by wire code, used by `Deserialize`. Generic over `FindingCode::ALL`, so
    /// there is only one hand-written code<->variant mapping (`FindingCode::code`) to keep in
    /// sync, not two.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.code() == code)
    }

    /// Which of the three independent admission questions this code answers. Exhaustive match, no
    /// catch-all arm — adding a variant breaks this build until it is classified here.
    pub const fn class(self) -> FindingClass {
        match self {
            FindingCode::BackendCoverageIncomplete => FindingClass::Representability,
            FindingCode::PayloadSizeBand => FindingClass::Readiness,
            FindingCode::UnknownUnboundedConstruct => FindingClass::Readiness,
            FindingCode::ResourceBudgetReached => FindingClass::Containment,
            FindingCode::BackendCompilationFailed => FindingClass::Process,
            FindingCode::BuildProcessFailed => FindingClass::Process,
            FindingCode::RuleInteractionProduct => FindingClass::Readiness,
        }
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

// Remedy, HealthFinding, HealthReport

/// One ranked, applicable remedy for a `HealthFinding`. Findings explain computational
/// consequences only and never assert that a change improves the grammar — a remedy that would
/// edit the grammar (reordering, constraining, decomposing a rule) must set
/// `requires_linguistic_equivalence` and SHOULD carry a `caveat`, since the compiler cannot
/// verify linguistic equivalence on its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Remedy {
    /// 1-based rank among this finding's remedies; lower ranks are recommended first.
    pub rank: u32,
    /// The remedy's computational-consequence description. Never a linguistic-quality claim.
    pub description: String,
    /// `true` when applying this remedy edits the grammar (reordering, constraining, decomposing
    /// a rule) and its safety depends on linguistic equivalence the compiler cannot verify on its
    /// own. `false` for compiler-internal transformations with an owned correctness argument, or
    /// non-grammar-editing computational-cost advice.
    pub requires_linguistic_equivalence: bool,
    /// Free-text caveat surfaced alongside the remedy when `requires_linguistic_equivalence` is
    /// `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caveat: Option<String>,
}

/// One stable compiler diagnostic: code, severity, phase, metric, predicted/observed value,
/// effective threshold, affected grammar/rule/construct identifiers, a concise explanation, zero
/// or more ranked remedies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthFinding {
    /// The current `PGFdddd` code (`FindingCode`).
    pub code: FindingCode,
    /// This finding's severity on the cost/health axis.
    pub severity: Severity,
    /// Which production stage produced or predicted this finding.
    pub phase: Phase,
    /// Stable grammar/rule/construct identifiers this finding is about. Freeform stable strings
    /// (e.g. a rule/template/stratum ID as the owning grammar names it) — this schema does not
    /// mint or constrain an ID format.
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
    /// A concise, human-readable explanation of the computational consequence — never a
    /// linguistic-quality judgment.
    pub explanation: String,
    /// Zero or more ranked, applicable remedies.
    #[serde(default)]
    pub remedies: Vec<Remedy>,
}

impl HealthFinding {
    /// Which of the three independent admission questions this finding's code answers.
    pub fn class(&self) -> FindingClass {
        self.code.class()
    }

    /// The only supported way to build a finding.
    ///
    /// Takes what every producer already sets; `affected`, `threshold` and `remedies` are opt-in
    /// through the builders below because they genuinely vary. Constructing the struct literally
    /// is refused by `tests/health_finding_seam.rs`, so a field added here reaches every producer
    /// at once rather than silently defaulting at the sites that forgot it.
    ///
    /// Three severities name an axis outright and must agree with the code's own class:
    /// `CannotRepresent` is `Representability`, `MachineLimit` is `Containment`,
    /// `LargeMultiplier` is `Readiness`. The other three -- `WithinLimits`, `Elevated`,
    /// `NotProductionReady` -- are positions on the blocking scale, reachable from any class;
    /// `NotProductionReady` is commonly a payload size, but a budget stop, a proven bound and a
    /// build-process fault all land there too, and the class is what says which.
    ///
    /// # Panics
    /// On a severity/class disagreement. `CONTEXT.md`'s three axes must never merge, so that
    /// pairing is unrepresentable rather than merely discouraged.
    #[allow(clippy::too_many_arguments)] // the seven facts every finding carries; the three that vary are builders
    pub fn new(
        code: FindingCode,
        severity: Severity,
        phase: Phase,
        metric: Metric,
        value: MetricValue,
        provenance: ValueProvenance,
        explanation: impl Into<String>,
    ) -> Self {
        if let Some(axis) = severity_axis(severity) {
            assert_eq!(
                code.class(),
                axis,
                "{code:?} answers {:?} but {severity:?} names {axis:?}; a severity that names an \
                 axis must agree with its code's class (CONTEXT.md, \"three axes that must never \
                 merge\")",
                code.class()
            );
        }
        HealthFinding {
            code,
            severity,
            phase,
            affected: Vec::new(),
            metric,
            value,
            provenance,
            threshold: None,
            explanation: explanation.into(),
            remedies: Vec::new(),
        }
    }

    /// The grammar/rule/construct identifiers this finding is about.
    #[must_use]
    pub fn affecting(mut self, affected: Vec<String>) -> Self {
        self.affected = affected;
        self
    }

    /// The effective threshold `value` was compared against.
    #[must_use]
    pub fn against_threshold(mut self, threshold: MetricValue) -> Self {
        self.threshold = Some(threshold);
        self
    }

    /// Ranked, applicable remedies.
    #[must_use]
    pub fn with_remedies(mut self, remedies: Vec<Remedy>) -> Self {
        self.remedies = remedies;
        self
    }
}

/// The axis a severity names, or `None` for a tier reachable from any class.
fn severity_axis(severity: Severity) -> Option<FindingClass> {
    match severity {
        Severity::CannotRepresent => Some(FindingClass::Representability),
        Severity::MachineLimit => Some(FindingClass::Containment),
        Severity::LargeMultiplier => Some(FindingClass::Readiness),
        Severity::WithinLimits | Severity::Elevated | Severity::NotProductionReady => None,
    }
}

/// The aggregated report for one grammar compilation. See `HealthReport::admission` for the
/// raw-severity aggregation rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthReport {
    /// This schema's version (`HEALTH_SCHEMA_VERSION`) at the time this report was produced.
    pub schema_version: u32,
    /// Every finding for this compilation, in producer order (not sorted or deduplicated by this
    /// type).
    pub findings: Vec<HealthFinding>,
}

/// The four admission questions (`FindingClass`) answered separately instead of collapsed into
/// one severity. Computed from `HealthReport::findings`, never stored on the wire: every field
/// here is fully derivable from data the canonical JSON already carries, so adding this type does
/// not change `HealthReport`'s serialized shape. Each field is independent — a `containment`
/// severity says nothing about `representability`, and neither says anything about `readiness` or
/// `process`. See `HealthReport::admission_by_class`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionByClass {
    /// Worst severity among this report's `FindingClass::Representability` findings. A
    /// non-`WithinLimits` value here means PanGloss cannot prove a recall-preserving
    /// representation; it is not a statement about size, speed, or resource use.
    pub representability: Severity,
    /// Worst severity among this report's `FindingClass::Readiness` findings. A non-`WithinLimits`
    /// value here is about shippability (size/speed/maintainability), not about whether the
    /// grammar is representable at all.
    pub readiness: Severity,
    /// Worst severity among this report's `FindingClass::Containment` findings. A
    /// non-`WithinLimits` value here means only that THIS attempt hit its own operational safety
    /// boundary; it says nothing about the grammar's representability and never makes partial
    /// output usable.
    pub containment: Severity,
    /// Worst severity among this report's `FindingClass::Process` findings. A non-`WithinLimits`
    /// value here reflects bad input or a worker/protocol/internal fault, not a fact about the
    /// grammar.
    pub process: Severity,
}

impl AdmissionByClass {
    /// A single-line rendering of all four fields, for a report surface that already prints the
    /// plain `HealthReport::admission` value and wants the per-axis breakdown alongside it.
    pub fn render(&self) -> String {
        format!(
            "representability={:?}, readiness={:?}, containment={:?}, process={:?}",
            self.representability, self.readiness, self.containment, self.process
        )
    }
}

impl HealthReport {
    /// Builds a report stamped with the current `HEALTH_SCHEMA_VERSION`.
    pub fn new(findings: Vec<HealthFinding>) -> Self {
        Self {
            schema_version: HEALTH_SCHEMA_VERSION,
            findings,
        }
    }

    /// The worst raw severity among this report's findings.
    pub fn admission(&self) -> Severity {
        self.findings
            .iter()
            .map(|finding| finding.severity)
            .max()
            .unwrap_or(Severity::WithinLimits)
    }

    /// The worst severity among this report's findings of `class`, or `Severity::WithinLimits`
    /// when no finding of that class is present. This is additive reporting alongside
    /// `admission`: it answers one of the three independent admission questions in isolation,
    /// never combined with the others.
    pub fn worst_by_class(&self, class: FindingClass) -> Severity {
        self.findings
            .iter()
            .filter(|finding| finding.class() == class)
            .map(|finding| finding.severity)
            .max()
            .unwrap_or(Severity::WithinLimits)
    }

    /// The three independent admission questions (plus `Process`) answered separately, never
    /// blurred into one severity. `admission()` remains the single worst-of-all value that gates
    /// publication; this is an additional per-class view onto the same findings, not a
    /// replacement.
    pub fn admission_by_class(&self) -> AdmissionByClass {
        AdmissionByClass {
            representability: self.worst_by_class(FindingClass::Representability),
            readiness: self.worst_by_class(FindingClass::Readiness),
            containment: self.worst_by_class(FindingClass::Containment),
            process: self.worst_by_class(FindingClass::Process),
        }
    }

    /// Canonical machine-readable form. Pretty-printed, two-space indent, fields in Rust
    /// declaration order — serde's unmodified default, matching `pg-snapshot`'s own determinism
    /// convention.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Parses a report from its canonical JSON form.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        let value: serde_json::Value = serde_json::from_str(json)?;
        let schema_version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                <serde_json::Error as serde::de::Error>::custom(
                    "health report is missing an unsigned schema_version",
                )
            })?;
        if schema_version != u64::from(HEALTH_SCHEMA_VERSION) {
            return Err(<serde_json::Error as serde::de::Error>::custom(format!(
                "unsupported health schema version {}; expected {}",
                schema_version, HEALTH_SCHEMA_VERSION
            )));
        }
        serde_json::from_value(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The single threshold, pinned once.

    #[test]
    fn fst_health_size_threshold_value_is_the_declared_target() {
        // Changing this changes a stated target: say so in `IDEAL_MAX_BYTES`'s doc.
        assert_eq!(IDEAL_MAX_BYTES, 100_000_000);
    }

    #[test]
    fn fst_health_size_bands_zero_is_within_limits() {
        assert_eq!(severity_for_size_bytes(0), Severity::WithinLimits);
    }

    #[test]
    fn fst_health_size_bands_within_limits_upper_edge_inclusive() {
        assert_eq!(
            severity_for_size_bytes(IDEAL_MAX_BYTES),
            Severity::WithinLimits
        );
    }

    #[test]
    fn fst_health_size_bands_not_production_ready_lower_edge_exclusive_of_within_limits() {
        assert_eq!(
            severity_for_size_bytes(IDEAL_MAX_BYTES + 1),
            Severity::NotProductionReady
        );
    }

    #[test]
    fn fst_health_size_bands_far_above_floor_remains_not_production_ready() {
        assert_eq!(
            severity_for_size_bytes(u64::MAX),
            Severity::NotProductionReady
        );
    }

    /// The pin for the whole category-leak fix: a compiled-artifact size measurement must never surface as a pre-compile static-analysis or containment verdict, at any size.
    #[test]
    fn size_never_reports_an_analysis_verdict() {
        let sizes = [
            0,
            IDEAL_MAX_BYTES,
            IDEAL_MAX_BYTES + 1,
            150_000_000,
            200_000_000,
            250_000_000,
            1_000_000_000,
            3_000_000_000,
            5_000_000_000,
            6_000_000_000,
            u64::MAX,
        ];
        for bytes in sizes {
            let severity = severity_for_size_bytes(bytes);
            assert_ne!(
                severity,
                Severity::Elevated,
                "{bytes} bytes must never report the pre-compile Elevated verdict"
            );
            assert_ne!(
                severity,
                Severity::LargeMultiplier,
                "{bytes} bytes must never report the pre-compile LargeMultiplier verdict"
            );
            assert_ne!(
                severity,
                Severity::MachineLimit,
                "{bytes} bytes must never report the containment MachineLimit verdict"
            );
            assert_ne!(
                severity,
                Severity::CannotRepresent,
                "{bytes} bytes must never report the pre-compile CannotRepresent verdict"
            );
        }
    }

    fn synthetic_finding(severity: Severity) -> HealthFinding {
        HealthFinding::new(
            FindingCode::PayloadSizeBand,
            severity,
            Phase::Compile,
            Metric::PayloadBytes,
            MetricValue::Bytes(1),
            ValueProvenance::Observed,
            "synthetic test finding".to_string(),
        )
        .affecting(vec!["synthetic-construct".to_string()])
    }

    #[test]
    fn an_empty_report_admits_within_limits() {
        let report = HealthReport::new(Vec::new());
        assert_eq!(report.admission(), Severity::WithinLimits);
    }

    #[test]
    fn the_worst_severity_wins_among_several_findings() {
        let report = HealthReport::new(vec![
            synthetic_finding(Severity::Elevated),
            synthetic_finding(Severity::NotProductionReady),
            synthetic_finding(Severity::LargeMultiplier),
        ]);
        assert_eq!(report.admission(), Severity::NotProductionReady);
    }

    // fst_health_schema: code registry, golden JSON, round trip, closed-enum exhaustiveness.

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
            assert!(
                !code.meaning().is_empty(),
                "{wire} must document its meaning"
            );
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

    #[test]
    fn characterization_phase_has_product_vocabulary_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&Phase::Characterization).unwrap(),
            "\"characterization\""
        );
    }

    #[test]
    fn dead_health_labels_bump_health_schema_version() {
        assert_eq!(HEALTH_SCHEMA_VERSION, 7);
    }

    /// An exhaustive `match` with no catch-all arm over every `Severity` variant, so adding a variant stops this from compiling until every exhaustive match in this file is updated.
    #[test]
    fn fst_health_schema_severity_is_closed_and_exhaustive() {
        const fn label(severity: Severity) -> &'static str {
            match severity {
                Severity::WithinLimits => "within_limits",
                Severity::Elevated => "elevated",
                Severity::LargeMultiplier => "large_multiplier",
                Severity::NotProductionReady => "not_production_ready",
                Severity::MachineLimit => "machine_limit",
                Severity::CannotRepresent => "cannot_represent",
            }
        }
        assert_eq!(label(Severity::WithinLimits), "within_limits");
        assert_eq!(label(Severity::Elevated), "elevated");
        assert_eq!(label(Severity::LargeMultiplier), "large_multiplier");
        assert_eq!(label(Severity::NotProductionReady), "not_production_ready");
        assert_eq!(label(Severity::MachineLimit), "machine_limit");
        assert_eq!(label(Severity::CannotRepresent), "cannot_represent");
    }

    /// One NotProductionReady payload-size label.
    fn representative_report() -> HealthReport {
        HealthReport::new(vec![HealthFinding::new(
            FindingCode::PayloadSizeBand,
            Severity::NotProductionReady,
            Phase::Compile,
            Metric::PayloadBytes,
            MetricValue::Bytes(1_500_000_000),
            ValueProvenance::Observed,
            "Final FST payload is 1,500,000,000 bytes, over the 100,000,000-byte \
                    NotProductionReady threshold."
                .to_string(),
        )
        .affecting(vec!["synthetic-stress-grammar".to_string()])
        .against_threshold(MetricValue::Bytes(IDEAL_MAX_BYTES))
        .with_remedies(vec![Remedy {
            rank: 1,
            description:
                "Review the measured compile cost and simplify the grammar before publication."
                    .to_string(),
            requires_linguistic_equivalence: false,
            caveat: None,
        }])])
    }

    const GOLDEN_JSON: &str = r#"{
  "schema_version": 7,
  "findings": [
    {
      "code": "PGF0001",
      "severity": "not_production_ready",
      "phase": "compile",
      "affected": [
        "synthetic-stress-grammar"
      ],
      "metric": "payload_bytes",
      "value": {
        "kind": "bytes",
        "value": 1500000000
      },
      "provenance": "observed",
      "threshold": {
        "kind": "bytes",
        "value": 100000000
      },
      "explanation": "Final FST payload is 1,500,000,000 bytes, over the 100,000,000-byte NotProductionReady threshold.",
      "remedies": [
        {
          "rank": 1,
          "description": "Review the measured compile cost and simplify the grammar before publication.",
          "requires_linguistic_equivalence": false
        }
      ]
    }
  ]
}"#;

    #[test]
    fn fst_health_schema_golden_json() {
        let report = representative_report();
        let json = report.to_json().expect("serialization must succeed");
        assert_eq!(
            json, GOLDEN_JSON,
            "canonical JSON drifted from the committed golden"
        );
    }

    #[test]
    fn fst_health_schema_round_trip() {
        let report = representative_report();
        let json = report.to_json().expect("serialization must succeed");
        let parsed = HealthReport::from_json(&json).expect("deserialization must succeed");
        assert_eq!(
            parsed, report,
            "round trip through canonical JSON must be lossless"
        );
        assert_eq!(parsed.admission(), Severity::NotProductionReady);
    }

    #[test]
    fn fst_health_schema_rejects_stale_v6_reports() {
        let stale = GOLDEN_JSON.replacen("\"schema_version\": 7", "\"schema_version\": 6", 1);
        let error = HealthReport::from_json(&stale).expect_err("schema v6 must be rejected");
        assert!(error.to_string().contains("schema version 6"));
        assert!(error.to_string().contains("expected 7"));
    }

    // fst_health_finding_class: FindingCode -> FindingClass, the four-question vocabulary.

    #[test]
    fn every_finding_code_has_a_class() {
        let classified: Vec<FindingClass> =
            FindingCode::ALL.iter().map(|code| code.class()).collect();
        assert_eq!(classified.len(), FindingCode::ALL.len());
    }

    #[test]
    fn representability_is_the_only_class_that_denies_the_grammar() {
        assert_eq!(
            FindingCode::BackendCoverageIncomplete.class(),
            FindingClass::Representability
        );
        for code in FindingCode::ALL {
            if *code == FindingCode::BackendCoverageIncomplete {
                continue;
            }
            assert_ne!(
                code.class(),
                FindingClass::Representability,
                "{code:?} must not claim to deny the grammar's representability"
            );
        }
    }

    #[test]
    fn containment_codes_are_about_the_attempt_not_the_language() {
        let containment: Vec<FindingCode> = FindingCode::ALL
            .iter()
            .copied()
            .filter(|code| code.class() == FindingClass::Containment)
            .collect();
        assert_eq!(containment, vec![FindingCode::ResourceBudgetReached]);
    }

    #[test]
    fn unknown_unbounded_construct_is_not_representability() {
        // Its own doc calls the construct recall-preserving: cost uncertainty, not a denial.
        assert_eq!(
            FindingCode::UnknownUnboundedConstruct.class(),
            FindingClass::Readiness
        );
    }

    // fst_health_admission_by_class: the per-class view, additive alongside `admission`.

    fn class_finding(code: FindingCode, severity: Severity) -> HealthFinding {
        HealthFinding::new(
            code,
            severity,
            Phase::Compile,
            Metric::PayloadBytes,
            MetricValue::Bytes(1),
            ValueProvenance::Observed,
            "synthetic per-class test finding".to_string(),
        )
        .affecting(vec!["synthetic-construct".to_string()])
    }

    #[test]
    fn admission_by_class_separates_a_resource_stop_from_a_representability_gap() {
        // Demonstrates the blur is gone: one severity used to hide which question was failing.
        let report = HealthReport::new(vec![
            class_finding(
                FindingCode::ResourceBudgetReached,
                Severity::NotProductionReady,
            ), // Containment
            class_finding(
                FindingCode::BackendCoverageIncomplete,
                Severity::CannotRepresent,
            ), // Representability
        ]);

        // The existing publish-gating value is untouched: still the plain max over everything.
        assert_eq!(report.admission(), Severity::CannotRepresent);

        let by_class = report.admission_by_class();
        assert_eq!(
            by_class.containment,
            Severity::NotProductionReady,
            "the resource stop must be visible on its own axis"
        );
        assert_eq!(
            by_class.representability,
            Severity::CannotRepresent,
            "the representability gap must be visible on its own axis"
        );
        assert_eq!(by_class.readiness, Severity::WithinLimits);
        assert_eq!(by_class.process, Severity::WithinLimits);
    }

    #[test]
    fn admission_by_class_render_names_all_four_axes() {
        let report = HealthReport::new(vec![
            class_finding(
                FindingCode::ResourceBudgetReached,
                Severity::NotProductionReady,
            ), // Containment
            class_finding(
                FindingCode::BackendCoverageIncomplete,
                Severity::CannotRepresent,
            ), // Representability
        ]);
        assert_eq!(
            report.admission_by_class().render(),
            "representability=CannotRepresent, readiness=WithinLimits, \
             containment=NotProductionReady, process=WithinLimits"
        );
    }

    #[test]
    fn worst_by_class_is_within_limits_for_an_absent_class() {
        let report = HealthReport::new(vec![
            class_finding(FindingCode::PayloadSizeBand, Severity::LargeMultiplier), // Readiness
        ]);
        assert_eq!(
            report.worst_by_class(FindingClass::Representability),
            Severity::WithinLimits
        );
        assert_eq!(
            report.worst_by_class(FindingClass::Containment),
            Severity::WithinLimits
        );
    }

    #[test]
    fn admission_is_unchanged_by_the_per_class_view() {
        // Each code is chosen so its class agrees with the severity beside it; the assertion reads severities only.
        let reports = vec![
            HealthReport::new(Vec::new()),
            HealthReport::new(vec![class_finding(
                FindingCode::PayloadSizeBand,
                Severity::Elevated,
            )]),
            HealthReport::new(vec![
                class_finding(
                    FindingCode::UnknownUnboundedConstruct,
                    Severity::NotProductionReady,
                ),
                class_finding(
                    FindingCode::RuleInteractionProduct,
                    Severity::LargeMultiplier,
                ),
            ]),
            HealthReport::new(vec![class_finding(
                FindingCode::ResourceBudgetReached,
                Severity::MachineLimit,
            )]),
        ];

        for report in reports {
            let plain_max = report
                .findings
                .iter()
                .map(|finding| finding.severity)
                .max()
                .unwrap_or(Severity::WithinLimits);
            assert_eq!(
                report.admission(),
                plain_max,
                "admission() must still equal the plain max over all severities"
            );
        }
    }
}
