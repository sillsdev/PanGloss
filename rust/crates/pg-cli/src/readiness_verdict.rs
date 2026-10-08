//! A human-facing readiness summary over measured facts and a versioned threshold policy.
//! It reports capability, trust, and every threshold check; meeting the thresholds is not a
//! correctness verdict. Missing, blocked, and invalid measurements never pass silently.
//!
//! Coverage is a token analysis rate supplied with an attestation, not an accuracy measurement.
//! A latency value below the timer floor can prove a pass only when that floor is within threshold.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::readiness_policy::ThresholdPolicy;
use pg_foma::analyzer::FomaProposer;
use pg_foma::capability::{CapabilityDiagnostic, CompileDecision};
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma_backend::backend_selection::select_backends;
// Test-only: production code holds `GrammarSemantics` rather than a bare `Grammar`.
#[cfg(test)]
use pg_grammar::model::Grammar;

/// This report's own wire-shape version (independent of [`crate::readiness_policy::
/// THRESHOLD_POLICY_SCHEMA_VERSION`] — the report's shape and the policy's shape can each change on
/// their own schedule, mirroring `pg-pack::manifest`'s `MANIFEST_SCHEMA_VERSION` vs. its embedded
/// `RequiredRuntimeFeatures::payload_format_version`).
pub const READINESS_REPORT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessMeasurementError {
    InvalidAnalysisRate,
}

impl fmt::Display for ReadinessMeasurementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAnalysisRate => {
                f.write_str("analysis_rate must be finite and within 0.0..=1.0")
            }
        }
    }
}

impl std::error::Error for ReadinessMeasurementError {}

/// The fixed disclaimer every coverage `CheckResult` carries (rule 3: never worded as accuracy).
pub const COVERAGE_RATE_STATEMENT: &str = "Coverage is a token-level ANALYSIS RATE: the fraction \
    of tokens receiving at least one analysis. A token may receive an INCORRECT analysis and still \
    count -- this is not an accuracy or correctness measurement. Correctness is the conformance \
    suite's job.";

/// The fixed disclaimer every attested coverage `CheckResult` carries (rule 2: an attestation is
/// not a measurement).
pub const COVERAGE_UNVERIFIED_STATEMENT: &str = "Held-out status is an ATTESTATION, not a \
    measurement: nothing in the artifact records what its author read while authoring, and \
    PanGloss does not train. This property is UNVERIFIED beyond the named attestor's own claim.";

// Trust status is local readiness metadata, not pack metadata.

/// One fail-closed configuration a capability override force-compiled through, using the same
/// predicate/construct/witness vocabulary `CapabilityDiagnostic` already uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverriddenConfig {
    pub predicate: String,
    pub construct: String,
    pub witness: String,
}

/// The local capability override record retained on readiness reports; it is not pack metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverrideRecord {
    pub authorized_by: String,
    pub reason: String,
    pub recorded_at: String,
    pub overridden_configs: Vec<OverriddenConfig>,
}

/// The binary capability-trust axis used by local readiness reports (tag `"status"`, with
/// `snake_case` variants).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TrustStatus {
    /// The characteristics-check gate admitted this artifact cleanly; no override was exercised.
    Proven,
    /// This artifact was force-compiled past a capability/correctness refusal, permanently
    /// forces `NotSupported` regardless of every other input. FST-health findings are a
    /// separate readiness axis and are never admitted by this trust status.
    Overridden(OverrideRecord),
}

impl TrustStatus {
    pub fn is_unproven(&self) -> bool {
        matches!(self, TrustStatus::Overridden(_))
    }
}

// Coverage: an attestation, never a measurement, worded as a rate, never accuracy.

/// Held-out coverage status for one language.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CoverageAssessment {
    /// A held-out corpus was supplied, with an attestation of who attested it and when; `analysis_rate` is the token-level analysis rate, `0.0..=1.0`.
    Attested {
        attestor: String,
        attested_on: String,
        analysis_rate: f64,
    },
    /// No held-out corpus is available for this language; reports as `NotAssessed`, never silently passing.
    NotAssessed,
}

// Latency: never rendered as a literal zero.

/// One latency percentile measurement, in milliseconds, with the same below-floor discipline
/// `tests/typology_speedup.rs` established for section 1 (see this module's top doc).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum LatencyMeasurement {
    /// A resolvable measurement, in milliseconds.
    Millis(f64),
    /// The true value is below this measurement path's resolution floor, never reported as a literal `0`.
    BelowFloor { floor_ms: f64 },
}

// Measured facts a caller supplies; this module does not itself measure anything.

/// The measured facts `assess_readiness` checks against a `ThresholdPolicy`. `None` for the whole
/// struct (via its `Option` parameter) means no compiled artifact exists to measure at
/// all (e.g. the grammar was refused before anything compiled); every field of coverage is its own
/// independent `CoverageAssessment` since a corpus can be present or absent independent of
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

// Checks: one per threshold dimension; this module reports every check, passed or not, so a reader sees the whole picture, not just the failures.

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

/// A measured or threshold value, in whatever unit its `CheckKind` uses -- shares one shape
/// across all six checks rather than six near-identical structs (mirrors `pg_foma_backend::health::
/// MetricValue`'s own closed-enum convention).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CheckValue {
    Bytes(u64),
    Count(u64),
    Rate(f64),
    Millis(f64),
    /// Mirrors `LatencyMeasurement::BelowFloor` for a measured (not threshold) value.
    BelowFloorMillis(f64),
}

/// The outcome of one check. **Closed, four variants, no catch-all match anywhere in this module**
/// (the same discipline `pg_foma::health`/`pg_foma::plan` document for their own closed enums) --
/// `CheckOutcome::Blocked` is a structurally distinct variant from `CheckOutcome::Pass`, so an
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
    /// No measurement exists to compare, or a below-floor measurement is too coarse to resolve a call; never rendered as a pass.
    NotAssessed {
        reason: String,
    },
    /// This artifact's trust status is `Overridden`, forcing every check to this outcome regardless of the underlying measured value, which is still recorded but never presented as passing.
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
    /// Present only for `CheckKind::CoverageAnalysisRate`: `COVERAGE_RATE_STATEMENT` always,
    /// plus `COVERAGE_UNVERIFIED_STATEMENT` when the coverage was [`CoverageAssessment::
    /// Attested`] (an attestation, not a check that could fail on its own terms).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statements: Vec<String>,
}

// Capability: always the real evaluation, never a caller-supplied guess.

/// One capability refusal citation, owned (not borrowed) so it outlives the `Grammar` this
/// report was computed from -- mirrors `CapabilityDiagnostic`'s own predicate/construct/witness
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

/// The real capability decision this report was computed from (`assess_readiness` always resolves it
/// itself, through the gated backend's own report from `pg_foma_backend::backend_selection::select_backends`
/// -- see `assess_readiness_with_semantics`'s own doc, "Which backend the report is about").
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

// Tier + report

/// The tiered verdict. See this module's top doc for the full contract each variant carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Every measured check passed, capability is `Admit`/`ConfirmOnly`, and trust is `Proven`.
    ThresholdsMet,
    /// Compiles and runs, but at least one threshold was missed or a check could not be assessed; actionable by the language team.
    NotYet,
    /// Either the capability gate blocks this grammar outright, or the artifact carries a capability override; actionable only by compiler work or a clean recompile.
    NotSupported,
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Self::ThresholdsMet => "Thresholds met",
            Self::NotYet => "Not yet",
            Self::NotSupported => "Not supported",
        }
    }
}

/// The full readiness report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadinessReport {
    pub report_schema_version: u32,
    /// The `ThresholdPolicy::policy_id` that produced this verdict.
    pub policy_id: String,
    pub device_class: String,
    pub tier: Tier,
    pub capability: CapabilitySummary,
    pub trust: TrustStatus,
    /// Every check this policy declares, always -- passed, failed, not-assessed, or blocked. Never
    /// filtered down to only the failures, so a reader sees the whole picture: every failed check
    /// carries its measured value/threshold, and reporting all of them (not just the failures) is
    /// a strict superset of that.
    pub checks: Vec<CheckResult>,
    /// Free-form explanatory notes: why the tier is what it is, and (rule 1) that the override is
    /// the reason the report is not supported, when applicable.
    pub notes: Vec<String>,
}

impl ReadinessReport {
    pub fn thresholds_met(&self) -> bool {
        matches!(self.tier, Tier::ThresholdsMet)
    }

    /// Canonical machine-readable form, the same pretty-printed convention `pg_foma::health` uses.
    #[cfg(test)]
    pub fn to_canonical_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("ReadinessReport serialization is infallible")
    }

    #[cfg(test)]
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }
}

// Per-check comparison helpers

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

fn check_coverage(
    assessment: &CoverageAssessment,
    threshold: &ThresholdF64,
) -> Result<CheckOutcome, ReadinessMeasurementError> {
    match assessment {
        CoverageAssessment::NotAssessed => Ok(CheckOutcome::NotAssessed {
            reason: "no held-out corpus is available for this language".to_string(),
        }),
        CoverageAssessment::Attested { analysis_rate, .. } => {
            if !analysis_rate.is_finite() || !(0.0..=1.0).contains(analysis_rate) {
                return Err(ReadinessMeasurementError::InvalidAnalysisRate);
            }
            let value = CheckValue::Rate(*analysis_rate);
            if *analysis_rate >= threshold.value {
                Ok(CheckOutcome::Pass { measured: value })
            } else {
                Ok(CheckOutcome::Fail { measured: value })
            }
        }
    }
}

/// Compares one latency measurement against its maximum-ms threshold: a below-floor measurement is a safe upper bound, so it can prove a `Pass` but never a `Fail`, and reports `NotAssessed` rather than guessing when the floor exceeds the threshold.
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

// Local generic-free aliases so the helpers above don't need to name Threshold<u64>/<f64> in full at every call site.
type ThresholdU64 = crate::readiness_policy::Threshold<u64>;
type ThresholdF64 = crate::readiness_policy::Threshold<f64>;

/// Applies `blocked_reason` or `measurements` to produce every check's `CheckOutcome`, in a fixed declaration order matching `CheckKind`'s own order.
fn compute_checks(
    policy: &ThresholdPolicy,
    measurements: Option<&Measurements>,
    blocked_reason: Option<&str>,
) -> Result<Vec<CheckResult>, ReadinessMeasurementError> {
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
                    check_coverage(&m.coverage, &policy.coverage_min_analysis_rate)?,
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

    Ok(raw
        .into_iter()
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
        .collect())
}

/// Reports `ThresholdsMet` only when every check passed and no refusal or override blocks the report.
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
        Tier::ThresholdsMet
    } else {
        Tier::NotYet
    }
}

fn build_notes(trust: &TrustStatus, capability: &CapabilitySummary, tier: Tier) -> Vec<String> {
    let mut notes = Vec::new();
    if let TrustStatus::Overridden(record) = trust {
        notes.push(format!(
            "BLOCKED: this artifact carries an ADR-0005 capability override (trust=unproven), \
             authorized by {} ({}), recorded at {}. An override-trusted artifact is not supported, \
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
        Tier::ThresholdsMet => notes.push(
            "Thresholds met: every declared threshold passed under this policy version, on the \
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

/// Assesses `g` against `policy`, given its `trust` status and (if any) its `measurements`.
///
/// Always computes the capability verdict itself (never a caller-supplied one, never inferred from
/// a failure to run). `measurements` is `None` when no compiled
/// artifact exists to measure (e.g. the grammar was refused before compilation ever produced one);
/// each measurement's own coverage sub-field is independently `CoverageAssessment::NotAssessed`
/// or `CoverageAssessment::Attested` regardless of whether the rest of `measurements` is present.
/// Test-only: every production caller already holds a `GrammarSemantics` and calls
/// `assess_readiness_with_semantics` directly.
#[cfg(test)]
pub fn assess_readiness(
    g: &Grammar,
    trust: &TrustStatus,
    measurements: Option<&Measurements>,
    policy: &ThresholdPolicy,
) -> ReadinessReport {
    assess_readiness_with_semantics(&GrammarSemantics::derive(g), trust, measurements, policy)
        .expect("test readiness measurements must be valid")
}

/// Assesses readiness over an already-derived `GrammarSemantics`; the semantics value is pure deterministic
/// input, while this function still computes the `CompileDecision` itself.
///
/// This does not accept a caller-supplied capability
/// verdict: a `GrammarSemantics` is a pure, deterministic function of the grammar, not a verdict,
/// and this function still computes the `CompileDecision` itself through
/// `pg_foma_backend::backend_selection::select_backends`. The thing a caller cannot do — hand in a `Refuse`
/// it decided on its own — remains impossible.
///
/// # Which backend the report is about
/// `pg_foma::analyzer::FomaProposer::EMISSION_STRATEGY`'s own report, not the whole-grammar join
/// over every backend. A readiness report describes the artifact a `pangloss` run would produce, and
/// that artifact comes from exactly one backend; the join would let another backend's ability
/// claim readiness for an artifact it never built.
pub fn assess_readiness_with_semantics(
    semantics: &GrammarSemantics<'_>,
    trust: &TrustStatus,
    measurements: Option<&Measurements>,
    policy: &ThresholdPolicy,
) -> Result<ReadinessReport, ReadinessMeasurementError> {
    // One owner: `BackendSelection::decision_for`, the same one `capability_gate` calls.
    let decision = select_backends(semantics).decision_for(FomaProposer::EMISSION_STRATEGY);
    let capability = CapabilitySummary::from_decision(&decision);

    let blocked_reason = match trust {
        TrustStatus::Proven => None,
        TrustStatus::Overridden(record) => Some(format!(
            "trust=unproven (ADR-0005 capability override, authorized by {}: {})",
            record.authorized_by, record.reason
        )),
    };
    let checks = compute_checks(policy, measurements, blocked_reason.as_deref())?;
    let tier = compute_tier(trust, &capability, &checks);
    let notes = build_notes(trust, &capability, tier);

    Ok(ReadinessReport {
        report_schema_version: READINESS_REPORT_SCHEMA_VERSION,
        policy_id: policy.policy_id.clone(),
        device_class: policy.device_class.clone(),
        tier,
        capability,
        trust: trust.clone(),
        checks,
        notes,
    })
}

#[cfg(test)]
mod tests;
