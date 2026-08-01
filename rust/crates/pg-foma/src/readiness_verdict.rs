//! Section 3 of `openspec/changes/certify-language-readiness` (tasks.md §3; `specs/
//! language-readiness-certification/spec.md`'s tiered-verdict and honesty requirements): the
//! **tiered certification verdict** — [`certify`] evaluates a grammar's real capability decision,
//! its trust status, and its measured facts against a [`crate::readiness_policy::ThresholdPolicy`],
//! and produces a [`ReadinessReport`] naming every failed check, never presenting an unassessed or
//! override-blocked check as passed.
//!
//! **Non-goal, per design.md**: certifying correctness. This module composes evidence produced
//! elsewhere (the capability gate, the conformance suite via an attested coverage rate, measured
//! latency/size); it does not independently verify any of it.
//!
//! # The two tiers, and why a flat pass/fail cannot do this job
//! - [`Tier::NotYet`]: the grammar compiles and runs (capability `Admit`/`ConfirmOnly`, trust
//!   `Proven`), but at least one threshold is missed or a required check could not be assessed.
//!   Actionable by the language team — more lexicon, better data, a smaller pack.
//! - [`Tier::NotSupported`]: either (a) the capability gate refuses the grammar outright — cited
//!   from the **real** [`crate::capability::CompileDecision::Refuse`] this module always computes
//!   itself (never a caller-supplied guess, never inferred from a failure to run — task 3.2), or
//!   (b) the artifact carries an ADR-0005 override (`trust=unproven`) — see the next section.
//!   Actionable only by compiler work (or, for (b), a clean recompile without the override).
//!
//! A single pass/fail bit cannot distinguish these — "too slow today" and "contains a permanently
//! carved-out construct" call for completely different responses (design.md's own framing).
//!
//! # Rule 1: an override-trusted artifact never certifies, under any configuration
//! [`certify`] takes a caller-supplied [`TrustStatus`]. Whenever it is [`TrustStatus::Overridden`],
//! **every** [`CheckOutcome`] this call produces is [`CheckOutcome::Blocked`] — never `Pass`, even
//! if the underlying measured value would numerically satisfy its threshold — and [`Tier`] is
//! forced to [`Tier::NotSupported`], regardless of what the real capability decision or any
//! threshold comparison would otherwise say. This is deliberately **two independent enforcement
//! points** (the per-check outcome AND the tier), not one: a caller that renders `checks` directly
//! without consulting `tier` still cannot accidentally print a "Pass" for an unproven pack. See
//! `override_forces_not_supported_and_blocks_every_check_even_when_everything_else_would_pass` for
//! the sabotage proof this rule is non-vacuous (construct an artifact that would certify cleanly
//! under `TrustStatus::Proven`, flip only the trust field to `Overridden`, show the verdict flips
//! too).
//!
//! # Rule 2: held-out coverage is an attestation, never a measurement
//! [`CoverageAssessment::Attested`] carries an `attestor` and a `attested_on` date and is rendered
//! with [`COVERAGE_UNVERIFIED_STATEMENT`] stating plainly that it is unverified — nothing in this
//! module checks whether the named attestor actually held the corpus out of authoring (PanGloss
//! does not train, and nothing in a grammar artifact records what its author read). Absent a
//! corpus, [`CoverageAssessment::NotAssessed`] renders as [`CheckOutcome::NotAssessed`], which
//! [`compute_tier`] treats as blocking [`Tier::Certified`] exactly like a real `Fail` — an
//! unassessed check must never render as passed (rule 4 below; this is the same check).
//!
//! # Rule 3: coverage is a token-level analysis rate, never accuracy
//! [`COVERAGE_RATE_STATEMENT`] is the fixed disclaimer every coverage [`CheckResult`] carries: the
//! rate is the fraction of tokens receiving **at least one** analysis; a token may receive a
//! *wrong* analysis and still count. Correctness is the conformance suite's job, not this module's.
//!
//! # Rule 4: an unassessed check never renders as passed
//! [`CheckOutcome`] is a closed, four-variant enum (`Pass`/`Fail`/`NotAssessed`/`Blocked`) with no
//! variant that could be mistaken for `Pass` by a renderer matching loosely — and [`compute_tier`]
//! only ever returns [`Tier::Certified`] when **every** check is `Pass`, so a single `NotAssessed`
//! or `Blocked` check anywhere denies `Certified` outright.
//!
//! # Latency's own below-floor discipline (composes with, but is distinct from, section 1's)
//! [`LatencyMeasurement`] mirrors `tests/typology_speedup.rs`'s "never emit `0`" rule at this
//! module's own layer (that harness's types are test-only and not importable as a library):
//! [`LatencyMeasurement::BelowFloor`] records that the true value is somewhere under the stated
//! floor, and [`compare_latency`] treats a below-floor measurement as a **safe** (conservative)
//! comparison — the true value is less than the floor, so a floor at or under the threshold proves
//! a pass; a floor above the threshold cannot be resolved finely enough to call, and is reported as
//! [`CheckOutcome::NotAssessed`] (an honest "cannot tell", never a guessed `Pass` or `Fail`) rather
//! than silently treating "below floor" as "zero" and calling it a pass by assumption.

use serde::{Deserialize, Serialize};

use crate::capability::{CapabilityDiagnostic, CompileDecision};
use crate::capability_entry::evaluate_capability;
use crate::readiness_policy::ThresholdPolicy;
use pg_grammar::model::Grammar;

/// This report's own wire-shape version (independent of [`crate::readiness_policy::
/// THRESHOLD_POLICY_SCHEMA_VERSION`] — the report's shape and the policy's shape can each change on
/// their own schedule, mirroring `pg-pack::manifest`'s `MANIFEST_SCHEMA_VERSION` vs. its embedded
/// `RequiredRuntimeFeatures::payload_format_version`).
pub const READINESS_REPORT_SCHEMA_VERSION: u32 = 1;

/// The fixed disclaimer every coverage [`CheckResult`] carries (rule 3: never worded as accuracy).
pub const COVERAGE_RATE_STATEMENT: &str = "Coverage is a token-level ANALYSIS RATE: the fraction \
    of tokens receiving at least one analysis. A token may receive an INCORRECT analysis and still \
    count -- this is not an accuracy or correctness measurement. Correctness is the conformance \
    suite's job.";

/// The fixed disclaimer every attested coverage [`CheckResult`] carries (rule 2: an attestation is
/// not a measurement).
pub const COVERAGE_UNVERIFIED_STATEMENT: &str = "Held-out status is an ATTESTATION, not a \
    measurement: nothing in the artifact records what its author read while authoring, and \
    PanGloss does not train. This property is UNVERIFIED beyond the named attestor's own claim.";

// =================================================================================================
// Trust status (ADR 0005) -- a local mirror, not a dependency on pg-pack (which itself depends on
// pg-foma for HealthReport; depending back would be circular). Field-for-field shape matches
// pg_pack::trust::CapabilityTrust/CapabilityOverrideRecord/OverriddenConfig, the same
// "duplicate the small shape at each layer" precedent pg-pack's own trust.rs already documents
// against crate::health::OverrideRecord (that module's own top-doc: "not a reuse ... stays exactly
// what it is ... distinct fields per axis").
// =================================================================================================

/// One fail-closed configuration an ADR-0005 override force-compiled through — mirrors
/// `pg_pack::trust::OverriddenConfig`'s shape (predicate/construct/witness), the same vocabulary
/// [`CapabilityDiagnostic`] already uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverriddenConfig {
    pub predicate: String,
    pub construct: String,
    pub witness: String,
}

/// The permanent ADR-0005 override record — mirrors `pg_pack::trust::CapabilityOverrideRecord`'s
/// shape field-for-field, so a caller assembling this from a real pack manifest is a trivial
/// projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverrideRecord {
    pub authorized_by: String,
    pub reason: String,
    pub recorded_at: String,
    pub overridden_configs: Vec<OverriddenConfig>,
}

/// ADR 0005's binary capability-trust axis, as this module consumes it. Mirrors
/// `pg_pack::trust::CapabilityTrust` exactly (tag `"status"`, `snake_case` variants) so the two
/// serialize identically wherever that matters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TrustStatus {
    /// The characteristics-check gate admitted (or ConfirmOnly-admitted) this artifact cleanly;
    /// no override was exercised.
    Proven,
    /// This artifact was force-compiled past a refusal (capability `Refuse`, or an overridden
    /// FST-health Error/Critical band — ADR 0005 covers both) via the ADR 0005 capability
    /// override. Permanently disqualifies certification (rule 1) regardless of every other input.
    Overridden(OverrideRecord),
}

impl TrustStatus {
    pub fn is_unproven(&self) -> bool {
        matches!(self, TrustStatus::Overridden(_))
    }
}

// =================================================================================================
// Coverage: an attestation, never a measurement (rule 2) -- worded as a rate, never accuracy (rule 3)
// =================================================================================================

/// Held-out coverage status for one language (rule 2/rule 3; tasks.md 3.4/3.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CoverageAssessment {
    /// A held-out corpus was supplied, with an attestation of who attested it held-out and when.
    /// `analysis_rate` is the token-level analysis rate (rule 3), `0.0..=1.0`.
    Attested {
        attestor: String,
        attested_on: String,
        analysis_rate: f64,
    },
    /// No held-out corpus is available for this language. Reports as [`CheckOutcome::NotAssessed`]
    /// -- never silently passing (rule 4).
    NotAssessed,
}

// =================================================================================================
// Latency: never rendered as a literal zero (mirrors section 1's rule at this module's own layer)
// =================================================================================================

/// One latency percentile measurement, in milliseconds, with the same below-floor discipline
/// `tests/typology_speedup.rs` established for section 1 (see this module's top doc).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum LatencyMeasurement {
    /// A resolvable measurement, in milliseconds.
    Millis(f64),
    /// The true value is below this measurement path's resolution floor (also in milliseconds) --
    /// never reported as a literal `0`.
    BelowFloor { floor_ms: f64 },
}

// =================================================================================================
// Measured facts a caller supplies (this module does not itself measure anything -- that is
// `pangloss make-report`'s job, section 4, out of this module's scope)
// =================================================================================================

/// The measured facts [`certify`] checks against a [`ThresholdPolicy`]. `None` for the whole
/// struct (via [`certify`]'s `Option` parameter) means no compiled artifact exists to measure at
/// all (e.g. the grammar was refused before anything compiled); every field of coverage is its own
/// independent [`CoverageAssessment`] since a corpus can be present or absent independent of
/// whether size/latency were measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measurements {
    pub pack_size_bytes: u64,
    pub lexicon_entries: u64,
    pub coverage: CoverageAssessment,
    pub latency_p50: LatencyMeasurement,
    pub latency_p90: LatencyMeasurement,
    pub latency_p99: LatencyMeasurement,
}

// =================================================================================================
// Checks: one per threshold dimension, always reported (tasks.md 3.6: every failed check reported
// with its measured value and threshold -- this module reports EVERY check, passed or not, so a
// reader sees the whole picture, not just the failures).
// =================================================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    PackSize,
    LexiconScale,
    CoverageAnalysisRate,
    LatencyP50,
    LatencyP90,
    LatencyP99,
}

/// A measured or threshold value, in whatever unit its [`CheckKind`] uses -- shares one shape
/// across all six checks rather than six near-identical structs (mirrors `crate::health::
/// MetricValue`'s own closed-enum convention).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CheckValue {
    Bytes(u64),
    Count(u64),
    Rate(f64),
    Millis(f64),
    /// Mirrors [`LatencyMeasurement::BelowFloor`] for a measured (not threshold) value.
    BelowFloorMillis(f64),
}

/// The outcome of one check. **Closed, four variants, no catch-all match anywhere in this module**
/// (the same discipline `crate::health`/`crate::plan` document for their own closed enums) --
/// [`CheckOutcome::Blocked`] is a structurally distinct variant from [`CheckOutcome::Pass`], so an
/// override-blocked check cannot be confused with a passed one even by a renderer that pattern-
/// matches loosely.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum CheckOutcome {
    Pass {
        measured: CheckValue,
    },
    Fail {
        measured: CheckValue,
    },
    /// No measurement exists to compare (no corpus, no artifact, or the below-floor measurement
    /// is too coarse relative to the threshold to resolve a call). Never rendered as a pass.
    NotAssessed {
        reason: String,
    },
    /// This artifact's trust status is [`TrustStatus::Overridden`] -- rule 1 forces every check to
    /// this outcome, never `Pass`, regardless of the underlying measured value (still recorded,
    /// for transparency, but never presented as passing).
    Blocked {
        reason: String,
        measured: Option<CheckValue>,
    },
}

impl CheckOutcome {
    pub fn is_pass(&self) -> bool {
        matches!(self, CheckOutcome::Pass { .. })
    }
}

/// One dimension's full result: which check, its outcome, its threshold, and (for coverage) the
/// two fixed honesty disclaimers (rules 2/3) rendered alongside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckResult {
    pub kind: CheckKind,
    pub outcome: CheckOutcome,
    pub threshold: CheckValue,
    /// Present only for [`CheckKind::CoverageAnalysisRate`]: [`COVERAGE_RATE_STATEMENT`] always,
    /// plus [`COVERAGE_UNVERIFIED_STATEMENT`] when the coverage was [`CoverageAssessment::
    /// Attested`] (an attestation, not a check that could fail on its own terms).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statements: Vec<String>,
}

// =================================================================================================
// Capability: always the REAL evaluation (task 3.2), never a caller-supplied guess
// =================================================================================================

/// One capability refusal citation, owned (not borrowed) so it outlives the [`Grammar`] this
/// report was computed from -- mirrors [`CapabilityDiagnostic`]'s own predicate/construct/witness
/// shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefusalCitation {
    pub predicate: String,
    pub construct: String,
    pub witness: String,
}

impl From<&CapabilityDiagnostic> for RefusalCitation {
    fn from(d: &CapabilityDiagnostic) -> Self {
        RefusalCitation {
            predicate: d.predicate.to_string(),
            construct: d.construct.clone(),
            witness: d.witness.clone(),
        }
    }
}

/// The real capability decision this report was computed from ([`certify`] always calls
/// [`evaluate_capability`] itself -- see this module's top doc, task 3.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum CapabilitySummary {
    Admit,
    ConfirmOnly,
    Refuse { refusals: Vec<RefusalCitation> },
}

impl CapabilitySummary {
    fn from_decision(decision: &CompileDecision) -> Self {
        match decision {
            CompileDecision::Admit => CapabilitySummary::Admit,
            CompileDecision::ConfirmOnly => CapabilitySummary::ConfirmOnly,
            CompileDecision::Refuse(diags) => CapabilitySummary::Refuse {
                refusals: diags.iter().map(RefusalCitation::from).collect(),
            },
        }
    }
}

// =================================================================================================
// Tier + report
// =================================================================================================

/// The tiered verdict (design.md's core decision; tasks.md 3.1). See this module's top doc for
/// the full contract each variant carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Every check passed, capability is `Admit`/`ConfirmOnly`, and trust is `Proven`.
    Certified,
    /// Compiles and runs, but at least one threshold was missed or a check could not be assessed.
    /// Actionable by the language team.
    NotYet,
    /// Either the capability gate refuses this grammar outright, or the artifact carries an
    /// ADR-0005 override. Actionable only by compiler work (or, for an override, a clean recompile
    /// without it).
    NotSupported,
}

/// The full certification report (tasks.md §3; spec.md's tiered-verdict requirement).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadinessReport {
    pub report_schema_version: u32,
    /// The [`ThresholdPolicy::policy_id`] that produced this verdict (spec.md: "it records the
    /// threshold policy version that produced it").
    pub policy_id: String,
    pub device_class: String,
    pub tier: Tier,
    pub capability: CapabilitySummary,
    pub trust: TrustStatus,
    /// Every check this policy declares, always -- passed, failed, not-assessed, or blocked. Never
    /// filtered down to only the failures, so a reader sees the whole picture (tasks.md 3.6 asks
    /// for every FAILED check to carry its measured value/threshold; reporting all of them is a
    /// strict superset of that).
    pub checks: Vec<CheckResult>,
    /// Free-form explanatory notes: why the tier is what it is, and (rule 1) that the override is
    /// the reason certification refused, when applicable.
    pub notes: Vec<String>,
}

impl ReadinessReport {
    pub fn is_certified(&self) -> bool {
        matches!(self.tier, Tier::Certified)
    }

    /// Canonical machine-readable form -- same convention as `crate::health`/`crate::
    /// coverage_ledger`/`crate::readiness_policy`.
    pub fn to_canonical_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("ReadinessReport serialization is infallible")
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }
}

// =================================================================================================
// Per-check comparison helpers
// =================================================================================================

fn check_pack_size(measured: u64, threshold: &ThresholdU64) -> CheckOutcome {
    let value = CheckValue::Bytes(measured);
    if measured <= threshold.value {
        CheckOutcome::Pass { measured: value }
    } else {
        CheckOutcome::Fail { measured: value }
    }
}

fn check_lexicon_scale(measured: u64, threshold: &ThresholdU64) -> CheckOutcome {
    let value = CheckValue::Count(measured);
    if measured >= threshold.value {
        CheckOutcome::Pass { measured: value }
    } else {
        CheckOutcome::Fail { measured: value }
    }
}

fn check_coverage(assessment: &CoverageAssessment, threshold: &ThresholdF64) -> CheckOutcome {
    match assessment {
        CoverageAssessment::NotAssessed => CheckOutcome::NotAssessed {
            reason: "no held-out corpus is available for this language".to_string(),
        },
        CoverageAssessment::Attested { analysis_rate, .. } => {
            let value = CheckValue::Rate(*analysis_rate);
            if *analysis_rate >= threshold.value {
                CheckOutcome::Pass { measured: value }
            } else {
                CheckOutcome::Fail { measured: value }
            }
        }
    }
}

/// Compares one latency measurement against its maximum-ms threshold, honoring the below-floor
/// discipline (this module's top doc, "Latency's own below-floor discipline"): a below-floor
/// measurement is a conservative safe upper bound on the true value, so it can prove a `Pass` (true
/// value < floor <= threshold) but can never itself prove a `Fail`; if the floor exceeds the
/// threshold, the measurement is too coarse to resolve a call and reports `NotAssessed` rather than
/// guessing.
fn check_latency(measured: LatencyMeasurement, threshold: &ThresholdF64) -> CheckOutcome {
    match measured {
        LatencyMeasurement::Millis(ms) => {
            let value = CheckValue::Millis(ms);
            if ms <= threshold.value {
                CheckOutcome::Pass { measured: value }
            } else {
                CheckOutcome::Fail { measured: value }
            }
        }
        LatencyMeasurement::BelowFloor { floor_ms } => {
            let value = CheckValue::BelowFloorMillis(floor_ms);
            if floor_ms <= threshold.value {
                CheckOutcome::Pass { measured: value }
            } else {
                CheckOutcome::NotAssessed {
                    reason: format!(
                        "measured only as below a {floor_ms}ms floor, which exceeds the \
                         {}ms threshold -- too coarse to resolve a pass/fail call",
                        threshold.value
                    ),
                }
            }
        }
    }
}

// Local generic-free aliases so the helpers above don't need to name
// `crate::readiness_policy::Threshold<u64>` in full at every call site.
type ThresholdU64 = crate::readiness_policy::Threshold<u64>;
type ThresholdF64 = crate::readiness_policy::Threshold<f64>;

/// Applies `blocked_reason` (rule 1) or `measurements` (normal path) to produce every check's
/// [`CheckOutcome`], in a fixed declaration order matching [`CheckKind`]'s own order.
fn compute_checks(
    policy: &ThresholdPolicy,
    measurements: Option<&Measurements>,
    blocked_reason: Option<&str>,
) -> Vec<CheckResult> {
    let raw: Vec<(CheckKind, CheckOutcome, CheckValue, Vec<String>)> = match measurements {
        None => vec![
            (
                CheckKind::PackSize,
                CheckOutcome::NotAssessed {
                    reason: "no compiled artifact exists to measure".to_string(),
                },
                CheckValue::Bytes(policy.pack_size_max_bytes.value),
                vec![],
            ),
            (
                CheckKind::LexiconScale,
                CheckOutcome::NotAssessed {
                    reason: "no compiled artifact exists to measure".to_string(),
                },
                CheckValue::Count(policy.lexicon_min_entries.value),
                vec![],
            ),
            (
                CheckKind::CoverageAnalysisRate,
                CheckOutcome::NotAssessed {
                    reason: "no held-out corpus is available for this language".to_string(),
                },
                CheckValue::Rate(policy.coverage_min_analysis_rate.value),
                vec![COVERAGE_RATE_STATEMENT.to_string()],
            ),
            (
                CheckKind::LatencyP50,
                CheckOutcome::NotAssessed {
                    reason: "no compiled artifact exists to measure".to_string(),
                },
                CheckValue::Millis(policy.latency_p50_max_ms.value),
                vec![],
            ),
            (
                CheckKind::LatencyP90,
                CheckOutcome::NotAssessed {
                    reason: "no compiled artifact exists to measure".to_string(),
                },
                CheckValue::Millis(policy.latency_p90_max_ms.value),
                vec![],
            ),
            (
                CheckKind::LatencyP99,
                CheckOutcome::NotAssessed {
                    reason: "no compiled artifact exists to measure".to_string(),
                },
                CheckValue::Millis(policy.latency_p99_max_ms.value),
                vec![],
            ),
        ],
        Some(m) => {
            let coverage_statements = {
                let mut s = vec![COVERAGE_RATE_STATEMENT.to_string()];
                if matches!(m.coverage, CoverageAssessment::Attested { .. }) {
                    s.push(COVERAGE_UNVERIFIED_STATEMENT.to_string());
                }
                s
            };
            vec![
                (
                    CheckKind::PackSize,
                    check_pack_size(m.pack_size_bytes, &policy.pack_size_max_bytes),
                    CheckValue::Bytes(policy.pack_size_max_bytes.value),
                    vec![],
                ),
                (
                    CheckKind::LexiconScale,
                    check_lexicon_scale(m.lexicon_entries, &policy.lexicon_min_entries),
                    CheckValue::Count(policy.lexicon_min_entries.value),
                    vec![],
                ),
                (
                    CheckKind::CoverageAnalysisRate,
                    check_coverage(&m.coverage, &policy.coverage_min_analysis_rate),
                    CheckValue::Rate(policy.coverage_min_analysis_rate.value),
                    coverage_statements,
                ),
                (
                    CheckKind::LatencyP50,
                    check_latency(m.latency_p50, &policy.latency_p50_max_ms),
                    CheckValue::Millis(policy.latency_p50_max_ms.value),
                    vec![],
                ),
                (
                    CheckKind::LatencyP90,
                    check_latency(m.latency_p90, &policy.latency_p90_max_ms),
                    CheckValue::Millis(policy.latency_p90_max_ms.value),
                    vec![],
                ),
                (
                    CheckKind::LatencyP99,
                    check_latency(m.latency_p99, &policy.latency_p99_max_ms),
                    CheckValue::Millis(policy.latency_p99_max_ms.value),
                    vec![],
                ),
            ]
        }
    };

    raw.into_iter()
        .map(|(kind, outcome, threshold, statements)| {
            let outcome = match blocked_reason {
                None => outcome,
                Some(reason) => {
                    let measured = match &outcome {
                        CheckOutcome::Pass { measured } | CheckOutcome::Fail { measured } => {
                            Some(*measured)
                        }
                        CheckOutcome::NotAssessed { .. } | CheckOutcome::Blocked { .. } => None,
                    };
                    CheckOutcome::Blocked {
                        reason: reason.to_string(),
                        measured,
                    }
                }
            };
            CheckResult {
                kind,
                outcome,
                threshold,
                statements,
            }
        })
        .collect()
}

/// [`Tier::Certified`] iff every check passed and neither the override nor the refusal gate fired.
/// Rule 1/rule 4 both live here: any [`CheckOutcome::Blocked`] or [`CheckOutcome::NotAssessed`]
/// anywhere denies `Certified`, same as an outright `Fail`.
fn compute_tier(
    trust: &TrustStatus,
    capability: &CapabilitySummary,
    checks: &[CheckResult],
) -> Tier {
    if trust.is_unproven() {
        return Tier::NotSupported;
    }
    if matches!(capability, CapabilitySummary::Refuse { .. }) {
        return Tier::NotSupported;
    }
    if checks.iter().all(|c| c.outcome.is_pass()) {
        Tier::Certified
    } else {
        Tier::NotYet
    }
}

fn build_notes(trust: &TrustStatus, capability: &CapabilitySummary, tier: Tier) -> Vec<String> {
    let mut notes = Vec::new();
    if let TrustStatus::Overridden(record) = trust {
        notes.push(format!(
            "BLOCKED: this artifact carries an ADR-0005 capability override (trust=unproven), \
             authorized by {} ({}), recorded at {}. An override-trusted artifact never certifies, \
             under any configuration -- see docs/adr/0005-capability-override-unproven-grammars.md. \
             {} fail-closed configuration(s) were force-compiled through.",
            record.authorized_by,
            record.reason,
            record.recorded_at,
            record.overridden_configs.len()
        ));
    }
    if let CapabilitySummary::Refuse { refusals } = capability {
        notes.push(format!(
            "NOT SUPPORTED: the capability gate refuses this grammar ({} refusal(s)) -- only \
             compiler work can move this tier, sourced from the real capability evaluation.",
            refusals.len()
        ));
    }
    match tier {
        Tier::Certified => notes.push(
            "CERTIFIED: every declared threshold passed under this policy version, on the \
             checks this report performed. See `checks` for exactly what was and was not \
             assessed."
                .to_string(),
        ),
        Tier::NotYet => notes.push(
            "NOT YET: this grammar compiles and runs, but at least one check failed or could \
             not be assessed. Actionable by the language team -- see `checks` for exactly which."
                .to_string(),
        ),
        Tier::NotSupported => {} // Already explained by the override/refusal notes above.
    }
    notes
}

/// Certifies `g` against `policy`, given its `trust` status and (if any) its `measurements`.
///
/// Always calls [`evaluate_capability`] itself (task 3.2: never a caller-supplied capability
/// verdict, never inferred from a failure to run). `measurements` is `None` when no compiled
/// artifact exists to measure (e.g. the grammar was refused before compilation ever produced one);
/// each measurement's own coverage sub-field is independently [`CoverageAssessment::NotAssessed`]
/// or [`CoverageAssessment::Attested`] regardless of whether the rest of `measurements` is present.
pub fn certify(
    g: &Grammar,
    trust: &TrustStatus,
    measurements: Option<&Measurements>,
    policy: &ThresholdPolicy,
) -> ReadinessReport {
    let decision = evaluate_capability(g);
    let capability = CapabilitySummary::from_decision(&decision);

    let blocked_reason = match trust {
        TrustStatus::Proven => None,
        TrustStatus::Overridden(record) => Some(format!(
            "trust=unproven (ADR-0005 capability override, authorized by {}: {})",
            record.authorized_by, record.reason
        )),
    };
    let checks = compute_checks(policy, measurements, blocked_reason.as_deref());
    let tier = compute_tier(trust, &capability, &checks);
    let notes = build_notes(trust, &capability, tier);

    ReadinessReport {
        report_schema_version: READINESS_REPORT_SCHEMA_VERSION,
        policy_id: policy.policy_id.clone(),
        device_class: policy.device_class.clone(),
        tier,
        capability,
        trust: trust.clone(),
        checks,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readiness_policy::policy_v1;

    fn load(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    /// A tiny, ordinary synthetic affix grammar -- no MPR groups, no Compounding, no Unordered
    /// strata, no true reduplication/circumfix/metathesis -- so `evaluate_capability` reaches
    /// `Admit`. Delanguaged single-letter segments only, per this repo's synthetic-data rule.
    const ADMIT_XML: &str = r#"<HermitCrabInput><Language><Name>Synthetic</Name>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cb" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="mr1">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <MorphologicalRule id="mr1">
              <Name>-a</Name>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="sub1">
                  <MorphologicalInput>
                    <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence>
                  </MorphologicalInput>
                  <MorphologicalOutput>
                    <CopyFromInput index="stem" />
                    <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                  </MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
            </MorphologicalRule>
          </MorphologicalRuleDefinitions>
          <LexicalEntries>
            <LexicalEntry id="e1">
              <Allomorphs><Allomorph id="a1"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
            </LexicalEntry>
          </LexicalEntries>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

    /// `openspec/changes/cover-compounding`'s single, non-recursive `Compounding` fixture (also
    /// used verbatim by `capability_entry`'s own test module): evaluates to `ConfirmOnly`, not
    /// `Admit` or `Refuse` -- gives this module's tests a second, distinct capability decision to
    /// exercise without inventing a third fixture shape.
    const CONFIRM_ONLY_XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <CompoundingRule id="cr1">
              <Name>Compound</Name>
              <CompoundingSubrules>
                <CompoundingSubrule>
                  <HeadMorphologicalInput>
                    <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </HeadMorphologicalInput>
                  <NonHeadMorphologicalInput>
                    <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </NonHeadMorphologicalInput>
                  <MorphologicalOutput>
                    <CopyFromInput index="n0" />
                    <CopyFromInput index="h0" />
                  </MorphologicalOutput>
                </CompoundingSubrule>
              </CompoundingSubrules>
            </CompoundingRule>
          </MorphologicalRuleDefinitions>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

    fn passing_measurements(policy: &ThresholdPolicy) -> Measurements {
        Measurements {
            pack_size_bytes: policy.pack_size_max_bytes.value / 2,
            lexicon_entries: policy.lexicon_min_entries.value * 2,
            coverage: CoverageAssessment::Attested {
                attestor: "synthetic-test-attestor".to_string(),
                attested_on: "2026-07-27".to_string(),
                analysis_rate: (policy.coverage_min_analysis_rate.value + 1.0) / 2.0
                    + policy.coverage_min_analysis_rate.value / 2.0,
            },
            latency_p50: LatencyMeasurement::Millis(policy.latency_p50_max_ms.value / 2.0),
            latency_p90: LatencyMeasurement::Millis(policy.latency_p90_max_ms.value / 2.0),
            latency_p99: LatencyMeasurement::Millis(policy.latency_p99_max_ms.value / 2.0),
        }
    }

    fn synthetic_override() -> OverrideRecord {
        OverrideRecord {
            authorized_by: "synthetic-test-operator".to_string(),
            reason: "synthetic field-trial override".to_string(),
            recorded_at: "2026-07-27T00:00:00Z".to_string(),
            overridden_configs: vec![OverriddenConfig {
                predicate: "synthetic.simultaneous.subrule-overlap".to_string(),
                construct: "mrule:synthetic-0001".to_string(),
                witness: "synthetic-witness-form".to_string(),
            }],
        }
    }

    // ---------------------------------------------------------------------------------------
    // Basic tiering
    // ---------------------------------------------------------------------------------------

    #[test]
    fn admit_grammar_with_passing_measurements_and_proven_trust_certifies() {
        let g = load(ADMIT_XML);
        let policy = policy_v1();
        let measurements = passing_measurements(&policy);
        let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);

        assert_eq!(report.tier, Tier::Certified);
        assert!(report.is_certified());
        assert_eq!(report.capability, CapabilitySummary::Admit);
        assert!(report.checks.iter().all(|c| c.outcome.is_pass()));
    }

    #[test]
    fn a_failing_threshold_produces_not_yet_not_not_supported() {
        let g = load(ADMIT_XML);
        let policy = policy_v1();
        let mut measurements = passing_measurements(&policy);
        // Blow the pack-size budget way past the threshold.
        measurements.pack_size_bytes = policy.pack_size_max_bytes.value * 100;
        let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);

        assert_eq!(report.tier, Tier::NotYet);
        let pack_check = report
            .checks
            .iter()
            .find(|c| c.kind == CheckKind::PackSize)
            .expect("pack size check must be present");
        assert!(
            matches!(pack_check.outcome, CheckOutcome::Fail { .. }),
            "expected a Fail outcome, got {:?}",
            pack_check.outcome
        );
    }

    #[test]
    fn confirm_only_grammar_can_still_certify_when_thresholds_pass() {
        // ConfirmOnly is first-class (ADR 0001), not a failure -- ConfirmOnly + Proven + all
        // thresholds passing must reach Certified, exactly like Admit.
        let g = load(CONFIRM_ONLY_XML);
        let policy = policy_v1();
        let measurements = passing_measurements(&policy);
        let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);

        assert_eq!(report.capability, CapabilitySummary::ConfirmOnly);
        assert_eq!(report.tier, Tier::Certified);
    }

    // ---------------------------------------------------------------------------------------
    // Task 5.1: an override-trusted artifact never certifies, proven non-vacuous by sabotage.
    // ---------------------------------------------------------------------------------------

    /// Sabotage proof (tasks.md 3.3/5.1): build a report that would clearly certify cleanly under
    /// `TrustStatus::Proven` (asserted first, so the "would have passed" premise is not assumed),
    /// then flip ONLY the trust field to `Overridden` with every other input held byte-identical,
    /// and show the verdict flips to `NotSupported` with every check `Blocked` -- never `Pass`.
    /// This is the gate that proves rule 1 is not vacuously satisfied by some other reason the
    /// grammar would have failed to certify anyway.
    #[test]
    fn override_forces_not_supported_and_blocks_every_check_even_when_everything_else_would_pass() {
        let g = load(ADMIT_XML);
        let policy = policy_v1();
        let measurements = passing_measurements(&policy);

        // Premise: with Proven trust, this exact grammar/measurements/policy combination
        // certifies cleanly. If this assertion ever fails, the sabotage below would be vacuous
        // (blocking something that was never going to pass anyway), so it must hold first.
        let proven_report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);
        assert_eq!(
            proven_report.tier,
            Tier::Certified,
            "premise failed -- this sabotage test requires a genuinely certifying baseline"
        );
        assert!(proven_report.checks.iter().all(|c| c.outcome.is_pass()));

        // Sabotage: flip ONLY the trust status.
        let overridden_report = certify(
            &g,
            &TrustStatus::Overridden(synthetic_override()),
            Some(&measurements),
            &policy,
        );

        assert_eq!(
            overridden_report.tier,
            Tier::NotSupported,
            "an override-trusted artifact must never certify, even when every threshold passes"
        );
        assert!(
            !overridden_report.is_certified(),
            "is_certified() must be false for an overridden artifact"
        );
        assert!(
            overridden_report
                .checks
                .iter()
                .all(|c| matches!(c.outcome, CheckOutcome::Blocked { .. })),
            "every check must be Blocked under an override, never Pass, Fail, or NotAssessed: {:?}",
            overridden_report.checks
        );
        assert!(
            !overridden_report.checks.iter().any(|c| c.outcome.is_pass()),
            "no threshold result may be presented as passing when the artifact is overridden"
        );
        assert!(
            overridden_report
                .notes
                .iter()
                .any(|n| n.contains("ADR-0005")),
            "the report must state the override is why: {:?}",
            overridden_report.notes
        );
    }

    #[test]
    fn override_blocks_even_a_capability_admit_grammar_under_any_configuration() {
        // "Under any configuration" (spec.md) -- exercise it against BOTH capability decisions
        // this test module has fixtures for, not just one.
        for xml in [ADMIT_XML, CONFIRM_ONLY_XML] {
            let g = load(xml);
            let policy = policy_v1();
            let measurements = passing_measurements(&policy);
            let report = certify(
                &g,
                &TrustStatus::Overridden(synthetic_override()),
                Some(&measurements),
                &policy,
            );
            assert_eq!(report.tier, Tier::NotSupported);
        }
    }

    // ---------------------------------------------------------------------------------------
    // Task 5.2: not-assessed coverage never renders as passed.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn not_assessed_coverage_blocks_certified_even_when_every_other_check_passes() {
        let g = load(ADMIT_XML);
        let policy = policy_v1();
        let mut measurements = passing_measurements(&policy);
        measurements.coverage = CoverageAssessment::NotAssessed;
        let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);

        let coverage_check = report
            .checks
            .iter()
            .find(|c| c.kind == CheckKind::CoverageAnalysisRate)
            .expect("coverage check must be present");
        assert!(
            matches!(coverage_check.outcome, CheckOutcome::NotAssessed { .. }),
            "expected NotAssessed, got {:?}",
            coverage_check.outcome
        );
        assert!(
            !coverage_check.outcome.is_pass(),
            "not-assessed coverage must never render as passed"
        );
        assert_eq!(
            report.tier,
            Tier::NotYet,
            "an unassessed required check must deny Certified even when every other check passes"
        );
    }

    #[test]
    fn no_measurements_at_all_reports_every_check_not_assessed_never_passed() {
        let g = load(ADMIT_XML);
        let policy = policy_v1();
        let report = certify(&g, &TrustStatus::Proven, None, &policy);

        assert!(
            report
                .checks
                .iter()
                .all(|c| matches!(c.outcome, CheckOutcome::NotAssessed { .. })),
            "every check must be NotAssessed with no measurements supplied: {:?}",
            report.checks
        );
        assert_eq!(report.tier, Tier::NotYet);
    }

    #[test]
    fn attested_coverage_carries_both_fixed_honesty_statements() {
        let g = load(ADMIT_XML);
        let policy = policy_v1();
        let measurements = passing_measurements(&policy);
        let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);

        let coverage_check = report
            .checks
            .iter()
            .find(|c| c.kind == CheckKind::CoverageAnalysisRate)
            .unwrap();
        assert!(coverage_check
            .statements
            .iter()
            .any(|s| s == COVERAGE_RATE_STATEMENT));
        assert!(coverage_check
            .statements
            .iter()
            .any(|s| s == COVERAGE_UNVERIFIED_STATEMENT));
    }

    // ---------------------------------------------------------------------------------------
    // Below-floor latency never renders as a bare zero or a guessed call.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn below_floor_latency_within_threshold_passes_conservatively() {
        let g = load(ADMIT_XML);
        let policy = policy_v1();
        let mut measurements = passing_measurements(&policy);
        measurements.latency_p50 = LatencyMeasurement::BelowFloor { floor_ms: 0.001 };
        let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);
        let p50 = report
            .checks
            .iter()
            .find(|c| c.kind == CheckKind::LatencyP50)
            .unwrap();
        assert!(matches!(p50.outcome, CheckOutcome::Pass { .. }));
    }

    #[test]
    fn below_floor_latency_coarser_than_threshold_is_not_assessed_not_guessed() {
        let g = load(ADMIT_XML);
        let policy = policy_v1();
        let mut measurements = passing_measurements(&policy);
        // A pathologically coarse floor, well above even the loosest threshold.
        measurements.latency_p50 = LatencyMeasurement::BelowFloor {
            floor_ms: 1_000_000.0,
        };
        let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);
        let p50 = report
            .checks
            .iter()
            .find(|c| c.kind == CheckKind::LatencyP50)
            .unwrap();
        assert!(
            matches!(p50.outcome, CheckOutcome::NotAssessed { .. }),
            "a floor coarser than the threshold must be NotAssessed, never a guessed Pass/Fail: \
             {:?}",
            p50.outcome
        );
    }

    // ---------------------------------------------------------------------------------------
    // Report always records the policy version + device class (spec.md).
    // ---------------------------------------------------------------------------------------

    #[test]
    fn report_records_policy_id_and_device_class() {
        let g = load(ADMIT_XML);
        let policy = policy_v1();
        let report = certify(&g, &TrustStatus::Proven, None, &policy);
        assert_eq!(report.policy_id, policy.policy_id);
        assert_eq!(report.device_class, policy.device_class);
    }

    // ---------------------------------------------------------------------------------------
    // Canonical JSON round trip.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn report_round_trips_through_canonical_json() {
        let g = load(ADMIT_XML);
        let policy = policy_v1();
        let measurements = passing_measurements(&policy);
        let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);
        let json = report.to_canonical_json();
        let parsed = ReadinessReport::from_json(&json).expect("valid report JSON must parse");
        assert_eq!(parsed, report);
    }

    // ---------------------------------------------------------------------------------------
    // Task 5.4: golden certificate for one small synthetic fixture, regenerated from the
    // generator's own output -- never hand-edited. Mirrors `crate::coverage_ledger`'s own
    // `regenerate_coverage_ledger_golden_json` precedent exactly.
    // ---------------------------------------------------------------------------------------

    /// A deterministic report over the `ADMIT_XML` fixture with fixed, hand-picked (not
    /// hand-edited-into-the-golden) measurements -- independent of any live grammar/pack state
    /// elsewhere in the repo, so this golden stays stable regardless of unrelated churn.
    fn golden_report() -> ReadinessReport {
        let g = load(ADMIT_XML);
        let policy = policy_v1();
        let measurements = Measurements {
            pack_size_bytes: 12_345,
            lexicon_entries: 2_000,
            coverage: CoverageAssessment::Attested {
                attestor: "synthetic-golden-attestor".to_string(),
                attested_on: "2026-07-27".to_string(),
                analysis_rate: 0.95,
            },
            latency_p50: LatencyMeasurement::Millis(0.5),
            latency_p90: LatencyMeasurement::Millis(2.0),
            latency_p99: LatencyMeasurement::Millis(10.0),
        };
        certify(&g, &TrustStatus::Proven, Some(&measurements), &policy)
    }

    #[track_caller]
    fn assert_readiness_verdict_golden(actual: &str, expected: &str) {
        crate::test_support::assert_canonical_lf_text_eq(actual, expected);
    }

    #[test]
    fn readiness_verdict_golden_boundary_accepts_lf_actual_against_crlf_expected() {
        let actual = "{\n  \"report_schema_version\": 1\n}\n";
        let expected = actual.replace('\n', "\r\n");
        assert_ne!(actual, expected);
        assert_readiness_verdict_golden(actual, &expected);
    }

    #[test]
    fn readiness_verdict_golden_boundary_rejects_crlf_actual() {
        let actual = "{\n  \"report_schema_version\": 1\n}\n";
        let expected = "{\n  \"report_schema_version\": 1\n}\n";
        let crlf_actual = actual.replace('\n', "\r\n");
        assert_ne!(crlf_actual, expected);
        let panic = std::panic::catch_unwind(|| {
            assert_readiness_verdict_golden(&crlf_actual, expected);
        });
        assert!(panic.is_err());
    }

    #[test]
    fn readiness_verdict_golden_boundary_rejects_ordering_and_trailing_newline_drift() {
        let ordering = std::panic::catch_unwind(|| {
            assert_readiness_verdict_golden(
                "{\n  \"a\": 1,\n  \"b\": 2\n}\n",
                "{\n  \"b\": 2,\n  \"a\": 1\n}\n",
            );
        });
        assert!(ordering.is_err());

        let trailing_newline = std::panic::catch_unwind(|| {
            assert_readiness_verdict_golden(
                "{\n  \"report_schema_version\": 1\n}",
                "{\n  \"report_schema_version\": 1\n}\n",
            );
        });
        assert!(trailing_newline.is_err());
    }

    #[test]
    fn readiness_verdict_golden_panic_location_is_the_external_assertion_callsite() {
        let expected_line = line!() + 2;
        let location = crate::test_support::capture_panic_location(|| {
            assert_readiness_verdict_golden("actual", "expected");
        });
        assert_eq!(location.file, file!());
        assert_eq!(location.line, expected_line);
        assert!(location.column > 0);
        assert!(!location.file.ends_with("test_support.rs"));
    }

    #[test]
    #[ignore = "regeneration helper, not a gate: run with --ignored to rewrite the golden from \
                this test's own computation after a reviewed, deliberate change to this module's \
                schema or the golden fixture's inputs"]
    fn regenerate_readiness_verdict_golden_json() {
        let json = golden_report().to_canonical_json();
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/readiness_verdict_golden.json"
            ),
            json,
        )
        .expect("golden must be writable");
    }

    #[test]
    fn readiness_verdict_golden_json() {
        let report = golden_report();
        let json = report.to_canonical_json();
        assert_readiness_verdict_golden(&json, GOLDEN_JSON);
    }

    #[test]
    fn golden_report_is_certified() {
        // Documents what the golden fixture's tier is, independent of the byte-for-byte JSON
        // comparison above, so a reader of this test file doesn't have to decode JSON to know.
        assert_eq!(golden_report().tier, Tier::Certified);
    }

    const GOLDEN_JSON: &str = include_str!("readiness_verdict_golden.json");
}
