//! The FST compilation-health finding schema: types, stable codes, severity and override
//! semantics, size bands, and canonical JSON.
//!
//! Health is REPORTED about a compile, never consulted during one — `crate::health_evaluator`
//! produces `HealthFinding`s from budget measurements after the fact, so no compiler pass
//! branches on anything here. Observed audit fields are populated by whichever pass owns the
//! measurement and are never independently remeasured.
//!
//! # Two distinct axes (do not conflate)
//! This module's severity axis (`Severity`: Ideal/Info/Warning/Error/Critical — a **cost/size**
//! axis) is a *different* dimension from the capability-trust axis (characteristics-check
//! hard-fail vs. capability override, binary proven-vs-unproven). A pack can be cost-healthy yet
//! capability-unproven, or vice versa — this module models only the cost/health axis. The
//! `OverrideRecord` on a `HealthFinding` is the capability override *as it bears on one
//! health finding* (a severity that would otherwise gate publication was force-compiled through),
//! not a re-implementation of the capability registry itself.
//!
//! # Severity and size bands
//! `severity_for_size_bytes` implements the exact decimal-byte FST-payload bands from the
//! `*_MAX_BYTES` constants. The warning a crossed band raises is wanted; the exact edge is
//! provisional — read `IDEAL_MAX_BYTES` before citing an edge as evidence. Size is one dimension among several —
//! see `Metric` for the others (compile work, intermediate nets, candidates, paths, application
//! time, unknown/unbounded constructs) — and `HealthReport::admission` aggregates across all of
//! them, not size alone.
//!
//! # Override policy
//! Both `Severity::Error` and `Severity::Critical` are overridable via the same capability
//! override: `Severity::overridable` returns `true` for both. The trust axis is binary and the
//! only non-overridable floor is apply-time execution containment — that floor is a runtime
//! containment outcome, never a predicted health verdict, so no `Severity` value represents it.
//!
//! # Worst non-overridden severity ("FST admission result")
//! `HealthReport::admission` is the worst severity among findings that do **not** carry an
//! `OverrideRecord`. An overridden Critical finding is permanently recorded (the
//! `OverrideRecord` itself, forever attached to that finding) but does **not** dominate a lower,
//! non-overridden severity elsewhere in the same report — the loud safety signal for an override
//! is the separate capability degraded-trust broadcast (pack-level `unproven` + per-result flag),
//! not this report's admission severity.
//!
//! # Cost uncertainty is not itself Critical
//! `ValueProvenance` and `MetricValue::Unbounded` encode that unknown cost is not itself
//! Critical when construction is recall-preserving: an `Unbounded` value with
//! `ValueProvenance::Predicted` is diagnostic evidence only and cannot by itself justify
//! `Severity::Critical` — only an actual observed `Metric::ResourceBudget`-style outcome (a
//! `FindingCode::ResourceBudgetReached` finding, `ValueProvenance::Observed`) or a
//! `ValueProvenance::ProvenBound` that cannot fit the remaining budget
//! (`FindingCode::ProvenBoundExceedsBudget`) does. This module records the distinction; it does
//! not enforce it at construction time, so a caller-supplied `HealthFinding` is still free-form
//! data as far as this schema is concerned — `crate::health_evaluator` is where this policy
//! becomes load-bearing.
//!
//! # Finding codes
//! `FindingCode` is the immutable `PGFdddd` registry: codes never renumber after publication,
//! so a stored report or external reference to a code stays valid forever.
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
//! meant to be derived from it rather than authored independently — no such renderer exists yet.
//! Pretty-printed with two-space indentation and struct fields in Rust declaration order (serde's
//! default, unmodified), mirroring `pg-snapshot`'s own determinism convention.
//!
//! # Design notes
//! - `FindingCode` covers every dimension this crate currently measures (payload size,
//!   intermediate networks, compile work, proposal volume, confirmation work, duplicate-analysis
//!   overlap, unknown/unbounded cost, a terminal budget-reached outcome, a proven-bound rejection,
//!   and apply-time work) without inventing per-construct codes no instrumentation exists to emit
//!   yet. Growing this list is additive (new codes only ever append; no code is ever renumbered
//!   or removed).
//! - `Phase` has three values (`Preflight`, `Compile`, `Apply`) rather than a simpler
//!   "preflight/observed" split: `Compile` and `Apply` are the two production phases (compile-time
//!   construction vs. per-word application), and `Preflight` is the characteristics-profile-style
//!   prediction stage that runs before either. "Observed" is not a `Phase` value here — it is
//!   `ValueProvenance::Observed`, the axis distinguishing predicted/proven-bound/measured values
//!   *within* a phase.
//! - `OverrideRecord` carries no timestamp type (a plain caller-supplied `recorded_at: String`)
//!   to avoid adding a date/time dependency to this crate for a schema-only type.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// This schema's own version, written into every `HealthReport`. Bump only on a
/// wire-incompatible change to this module's types.
pub const HEALTH_SCHEMA_VERSION: u32 = 1;

// Severity + size bands

/// The cost/health severity axis — deliberately **distinct** from the capability-trust axis
/// (proven-vs-unproven capability checks). Declaration order is worst-last and is what `Ord`
/// and `HealthReport::admission`'s `max` rely on: `Ideal < Info < Warning < Error < Critical`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Within every band; nothing to report.
    Ideal,
    /// Above the Ideal band but not yet action-worthy.
    Info,
    /// Action-worthy but does not block publication.
    Warning,
    /// Requires an explicit, recorded `OverrideRecord` before the artifact may publish.
    Error,
    /// The worst band; still overridable like `Severity::Error`. Apply-time execution containment is the only non-overridable floor, and it is not a `Severity` value at all.
    Critical,
}

impl Severity {
    /// Both `Severity::Error` and `Severity::Critical` are overridable via the capability
    /// override; `Severity::Warning` and below never need one. No catch-all arm.
    pub const fn overridable(self) -> bool {
        match self {
            Severity::Ideal | Severity::Info | Severity::Warning => false,
            Severity::Error | Severity::Critical => true,
        }
    }
}

/// Inclusive upper edge of the Ideal payload band.
///
/// **The warning these bands raise is real; the exact numbers are provisional.** A compiled
/// grammar that runs to a gigabyte is not something anyone can ship, so a payload that large has
/// to reach its author as a warning — that much is settled, and it is why these are thresholds and
/// not just a reported number. What is unsettled is where each edge sits: no grammar was measured
/// to pick them, and the change whose job was to derive such thresholds from evidence was retired
/// without producing one.
///
/// They encode an intent. A grammar is on the order of a thousand parameters, so the whole
/// difficulty is combining them compactly — which is exactly what different backends do better or
/// worse. Read a crossed band as "this backend did not combine this grammar well", never as a
/// proven resource limit. Provenance and the pending recalibration against a real spread across
/// backends and grammars: `docs/change-retirement-grills.md`.
pub const IDEAL_MAX_BYTES: u64 = 100_000_000;
/// Inclusive upper edge of the Info payload band. Provenance: see [`IDEAL_MAX_BYTES`].
pub const INFO_MAX_BYTES: u64 = 200_000_000;
/// Inclusive upper edge of the Warning payload band. Provenance: see [`IDEAL_MAX_BYTES`].
pub const WARNING_MAX_BYTES: u64 = 1_000_000_000;
/// Inclusive upper edge of the Error payload band; above this is Critical. Provenance: see
/// [`IDEAL_MAX_BYTES`].
pub const ERROR_MAX_BYTES: u64 = 5_000_000_000;

/// The exact decimal-byte FST-payload size bands, inclusive upper edges, from the
/// `*_MAX_BYTES` constants — which are a stated target, NOT a measured limit; read
/// [`IDEAL_MAX_BYTES`] before citing any band as evidence. Each edge is pinned by this
/// function's tests, so the constants and the bands cannot drift apart silently.
///
/// Size is one health dimension, not the whole story: compile work, intermediate nets,
/// candidates, paths, time, and unknown/unbounded constructs may also raise severity. Combine
/// this with other dimensions' findings via `HealthReport::admission`, never use this
/// function's result alone as overall admission.
pub const fn severity_for_size_bytes(bytes: u64) -> Severity {
    if bytes <= IDEAL_MAX_BYTES {
        Severity::Ideal
    } else if bytes <= INFO_MAX_BYTES {
        Severity::Info
    } else if bytes <= WARNING_MAX_BYTES {
        Severity::Warning
    } else if bytes <= ERROR_MAX_BYTES {
        Severity::Error
    } else {
        Severity::Critical
    }
}

// Phase, Metric, ValueProvenance, MetricValue

/// Which production stage a `HealthFinding` was produced in or predicted for. See this module's
/// doc "Design notes" section for why this has three values rather than a simpler
/// "preflight/observed" pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Predicted before any construction begins (characteristics-profile-style projection).
    Preflight,
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
    /// An intermediate composition/union/minimize product's state count (`Fsm::statecount`).
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
    /// Rejection share: confirmed / proposed, reported as a `MetricValue::Ratio`.
    RejectionShare,
    /// Pre-dedup duplicate analysis count (e.g. many copies of the same structured analysis).
    DuplicateAnalysisCount,
    /// Pre-dedup duplicate analysis ratio, reported as a `MetricValue::Ratio`.
    DuplicateAnalysisRatio,
    /// Apply-time derivation/unapplication chain depth — an unbounded chain risks stack overflow.
    ApplyChainDepth,
    /// Apply-time reserved allocation/logical-memory budget, in bytes — an unbounded budget risks OOM.
    ApplyAllocationBytes,
    /// A construct whose cost cannot be bounded ahead of time; paired with `MetricValue::Unbounded` and `ValueProvenance::Predicted`.
    UnknownUnboundedWork,
    /// An `Unordered` stratum's own loose-rule count; kept distinct from `AlphaTupleCount`/`GateGroupCount` so neither variant's stored meaning becomes ambiguous in canonical JSON.
    OrderingRuleCount,
    /// A sampled compile-worker RSS reading, in bytes — never a hard ceiling, since allocation between samples means a reading below a guardrail is not proof the process stayed under it.
    SampledCompileRssBytes,
    /// The compound HEAD x NON-HEAD root-allomorph cross product a grammar's `CompoundingRuleDef`s license.
    CompoundRootPairCount,
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
    /// A plain count (candidates, paths, confirmations, duplicates, states, arcs, ...).
    Count(u64),
    /// A byte quantity (payload size, reserved allocation, ...).
    Bytes(u64),
    /// A millisecond duration.
    Millis(u64),
    /// A dimensionless ratio, `0.0..=1.0` by convention but not enforced by this type.
    Ratio(f64),
    /// Cost uncertainty: no bound is available at all (paired with `ValueProvenance::Predicted`).
    Unbounded,
}

// FindingCode registry

/// The immutable `PGFdddd` finding-code registry: codes use `PGF` plus four decimal digits and
/// never change meaning after publication, so a stored report or external reference to a code
/// stays valid forever. Closed on purpose — see this module's doc "Design notes" section for what
/// each code covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FindingCode {
    /// Final FST payload size crossed a severity band (`severity_for_size_bytes`).
    PayloadSizeBand,
    /// An intermediate composition/union/minimize product grew large relative to its budget.
    IntermediateNetworkGrowth,
    /// Compile-time logical construction work approached or reached its budget.
    CompileWorkBudget,
    /// FST-propose candidate or path volume is large, independent of final correctness or size.
    ProposalVolume,
    /// HermitCrab confirmation count, rejection share, or confirmation work is large.
    ConfirmationWork,
    /// Pre-dedup duplicate analysis count/ratio with rule or proposal-path provenance, when available.
    DuplicateAnalysisOverlap,
    /// A recall-preserving construct's cost cannot be bounded ahead of time; not itself Critical.
    UnknownUnboundedConstruct,
    /// A compilation attempt reached an enforced logical/byte/time budget and stopped.
    ResourceBudgetReached,
    /// An exact value or proven lower bound shows an operation cannot fit the remaining budget.
    ProvenBoundExceedsBudget,
    /// Per-word apply-time work (chain depth, allocation, elapsed time) is elevated.
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

    /// A one-line, stable meaning for this code. Exhaustive match, no catch-all arm.
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

    /// Reverse lookup by wire code, used by `Deserialize`. Generic over `FindingCode::ALL`, so
    /// there is only one hand-written code<->variant mapping (`FindingCode::code`) to keep in
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

// Remedy, OverrideRecord, HealthFinding, HealthReport

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
    /// non-grammar-editing advice (e.g. "retry with a larger named envelope").
    pub requires_linguistic_equivalence: bool,
    /// Free-text caveat surfaced alongside the remedy when `requires_linguistic_equivalence` is
    /// `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caveat: Option<String>,
}

/// The permanent record of one capability override as it bears on a single `HealthFinding`.
/// This struct is this report's own copy of that fact — the authoritative, indelible
/// pack-manifest record is a separate artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverrideRecord {
    /// Who or what authorized the override (a caller identity, tool name, or operator label).
    pub authorized_by: String,
    /// Why the override was exercised.
    pub reason: String,
    /// Caller-supplied record of when the override was exercised (free-form; see this module's
    /// doc "Design notes" section for why this is a plain `String`, not a timestamp type).
    pub recorded_at: String,
}

/// One stable compiler diagnostic: code, severity, phase, metric, predicted/observed value,
/// effective threshold, affected grammar/rule/construct identifiers, a concise explanation, zero
/// or more ranked remedies, and an optional override record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthFinding {
    /// The immutable `PGFdddd` code (`FindingCode`).
    pub code: FindingCode,
    /// This finding's severity on the cost/health axis (never the capability-trust axis).
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
    /// Present only when this finding's severity was explicitly overridden. `None` means not
    /// overridden — including for every `Severity::Ideal`/`Severity::Info`/
    /// `Severity::Warning` finding, which never need one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_record: Option<OverrideRecord>,
}

/// The aggregated report for one grammar compilation. See `HealthReport::admission` for the
/// aggregation rule and this module's doc for why an overridden Critical does not dominate a
/// lower non-overridden severity elsewhere in the same report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthReport {
    /// This schema's version (`HEALTH_SCHEMA_VERSION`) at the time this report was produced.
    pub schema_version: u32,
    /// Every finding for this compilation, in producer order (not sorted or deduplicated by this
    /// type).
    #[serde(default)]
    pub findings: Vec<HealthFinding>,
}

impl HealthReport {
    /// Builds a report stamped with the current `HEALTH_SCHEMA_VERSION`.
    pub fn new(findings: Vec<HealthFinding>) -> Self {
        Self {
            schema_version: HEALTH_SCHEMA_VERSION,
            findings,
        }
    }

    /// The worst severity among findings that do **not** carry an `OverrideRecord` (the "FST
    /// admission result"). An empty report, or a report whose only findings are all overridden,
    /// admits at `Severity::Ideal` — the override itself remains permanently attached to its
    /// finding (and, at the pack level, surfaces via the separate capability degraded-trust
    /// signal), but it never inflates this aggregation.
    pub fn admission(&self) -> Severity {
        self.findings
            .iter()
            .filter(|finding| finding.override_record.is_none())
            .map(|finding| finding.severity)
            .max()
            .unwrap_or(Severity::Ideal)
    }

    /// Canonical machine-readable form. Pretty-printed, two-space indent, fields in Rust
    /// declaration order — serde's unmodified default, matching `pg-snapshot`'s own determinism
    /// convention.
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

    // Edges by relation via the constants; the four values are pinned once, separately, below.

    #[test]
    fn fst_health_size_band_values_are_the_declared_targets() {
        // Changing one of these changes a stated target: say so in `IDEAL_MAX_BYTES`'s doc.
        assert_eq!(IDEAL_MAX_BYTES, 100_000_000);
        assert_eq!(INFO_MAX_BYTES, 200_000_000);
        assert_eq!(WARNING_MAX_BYTES, 1_000_000_000);
        assert_eq!(ERROR_MAX_BYTES, 5_000_000_000);
    }

    #[test]
    fn fst_health_size_bands_are_strictly_ascending() {
        assert!(IDEAL_MAX_BYTES < INFO_MAX_BYTES);
        assert!(INFO_MAX_BYTES < WARNING_MAX_BYTES);
        assert!(WARNING_MAX_BYTES < ERROR_MAX_BYTES);
    }

    #[test]
    fn fst_health_size_bands_zero_is_ideal() {
        assert_eq!(severity_for_size_bytes(0), Severity::Ideal);
    }

    #[test]
    fn fst_health_size_bands_ideal_upper_edge_inclusive() {
        assert_eq!(severity_for_size_bytes(IDEAL_MAX_BYTES), Severity::Ideal);
    }

    #[test]
    fn fst_health_size_bands_info_lower_edge_exclusive_of_ideal() {
        assert_eq!(severity_for_size_bytes(IDEAL_MAX_BYTES + 1), Severity::Info);
    }

    #[test]
    fn fst_health_size_bands_info_upper_edge_inclusive() {
        assert_eq!(severity_for_size_bytes(INFO_MAX_BYTES), Severity::Info);
    }

    #[test]
    fn fst_health_size_bands_warning_lower_edge_exclusive_of_info() {
        assert_eq!(
            severity_for_size_bytes(INFO_MAX_BYTES + 1),
            Severity::Warning
        );
    }

    #[test]
    fn fst_health_size_bands_warning_upper_edge_inclusive() {
        // The band boundary is INCLUSIVE: exactly WARNING_MAX_BYTES is Warning, not Error.
        assert_eq!(
            severity_for_size_bytes(WARNING_MAX_BYTES),
            Severity::Warning
        );
    }

    #[test]
    fn fst_health_size_bands_error_lower_edge_exclusive_of_warning() {
        assert_eq!(
            severity_for_size_bytes(WARNING_MAX_BYTES + 1),
            Severity::Error
        );
    }

    #[test]
    fn fst_health_size_bands_error_upper_edge_inclusive() {
        assert_eq!(severity_for_size_bytes(ERROR_MAX_BYTES), Severity::Error);
    }

    #[test]
    fn fst_health_size_bands_critical_lower_edge_exclusive_of_error() {
        assert_eq!(
            severity_for_size_bytes(ERROR_MAX_BYTES + 1),
            Severity::Critical
        );
    }

    #[test]
    fn fst_health_size_bands_critical_far_above_floor() {
        assert_eq!(severity_for_size_bytes(u64::MAX), Severity::Critical);
    }

    // fst_health_override_policy: Error/Critical overridability + worst-non-overridden aggregation.

    #[test]
    fn fst_health_override_policy_error_and_critical_are_overridable() {
        // NOT "Critical = no override" — see this module's doc "Override policy" section.
        assert!(Severity::Error.overridable());
        assert!(Severity::Critical.overridable());
    }

    #[test]
    fn fst_health_override_policy_warning_and_below_never_need_override() {
        assert!(!Severity::Ideal.overridable());
        assert!(!Severity::Info.overridable());
        assert!(!Severity::Warning.overridable());
    }

    fn synthetic_finding(
        severity: Severity,
        override_record: Option<OverrideRecord>,
    ) -> HealthFinding {
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

    /// An exhaustive `match` with no catch-all arm over every `Severity` variant, so adding a variant stops this from compiling until every exhaustive match in this file is updated.
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

    /// Two findings: one Warning with a linguistic-equivalence-caveated remedy, one Error carrying a permanent `OverrideRecord`.
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
                value: MetricValue::Bytes(1_500_000_000),
                provenance: ValueProvenance::Observed,
                threshold: Some(MetricValue::Bytes(WARNING_MAX_BYTES)),
                explanation: "Final FST payload is 1,500,000,000 bytes, in the Error band \
                    (>1,000,000,000..=5,000,000,000)."
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
        "value": 1500000000
      },
      "provenance": "observed",
      "threshold": {
        "kind": "bytes",
        "value": 1000000000
      },
      "explanation": "Final FST payload is 1,500,000,000 bytes, in the Error band (>1,000,000,000..=5,000,000,000).",
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
        assert_eq!(parsed.admission(), Severity::Warning);
    }

    #[test]
    fn fst_health_schema_golden_admission_is_warning() {
        // The golden's Error finding is overridden and therefore must not dominate.
        assert_eq!(representative_report().admission(), Severity::Warning);
    }
}
