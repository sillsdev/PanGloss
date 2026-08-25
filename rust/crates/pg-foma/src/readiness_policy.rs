//! One
//! declared, **versioned** place for the thresholds a certification verdict ([`crate::
//! readiness_verdict`]) is measured against — pack size, lexicon scale, token analysis rate, and
//! p50/p90/p99 latency against a named device class — so a verdict can cite the policy version
//! that produced it and an older certificate stays interpretable after the numbers move.
//!
//! This module owns the schema and today's seed
//! values only; it does not gate any compile path, mirroring `crate::health`/`crate::plan_diagram`'s
//! own "define the versioned schema first" precedent.
//!
//! # Calibration is a first-class field, not a doc comment
//! Every threshold value carries a `Calibration` tag: `Calibration::Measured` (backed by real,
//! cited evidence) or `Calibration::Placeholder` (explicitly un-calibrated, with a rationale for
//! why the chosen number is not arbitrary even though it is not yet evidence-backed). This is the
//! same discipline `compose_budget.rs`'s "conservative placeholder pending real-grammar
//! measurement" / `characterization.rs`'s provisional-bound comments already use: provisional values
//! must never be presented as release policy without being flagged as such —
//! made machine-readable here so a report can never silently launder a placeholder
//! into a value that merely *looks* measured. Never invent a number that looks authoritative: every
//! `Calibration::Placeholder` below names exactly why it has no evidence yet.
//!
//! # Today's seed values (policy v1) and what backs each one
//! - **`device_class`**: named as `"dev-workstation-v1"` — the exact machine/configuration
//!   `docs/benchmark-matrix.md` measured on (Windows 11 x64, `pangloss batch
//!   --threads 1`, release build) and the machine `rust/tools/typology-speedup.sh` runs on
//!   locally. This is deliberately **not** a mobile/embedded device name: no such device has been
//!   benchmarked yet, and inventing an evocative name ("reference-mobile-tier-1") for a number with
//!   no device behind it would be exactly the kind of authoritative-looking invention this module
//!   exists to refuse. A certificate under this policy version is scoped to this workstation class
//!   and must not be read as evidence about any other device class: no silent
//!   generalization beyond it.
//! - **Latency (p50/p90/p99), `Calibration::Measured`**: grounded in two real sources — (1)
//!   `docs/benchmark-matrix.md`'s one force-compiled data point (Indonesian, `--allow-unproven`:
//!   p50 `<1`ms, p95 1ms, p99 1ms, max 8ms — reported there as force-compiled, not certified), and
//!   (2) the typology-speedup harness's own compiled-engine column (`rust/tools/
//!   typology-speedup.sh`; `rust/crates/pg-foma/tests/typology_speedup.rs`), which shows
//!   sub-millisecond medians across nearly every tiny synthetic edge-case/typology fixture. Both
//!   citations are recorded verbatim in each field's `citation` string, **including the caveat
//!   that this evidence is thin** (one reference grammar, measured only via a force-compiled
//!   capability override; tiny synthetic fixtures, not a full-scale lexicon) — an early, revisable estimate with an
//!   explicit safety margin over the observed numbers, not a robust calibration. Marked `Measured`
//!   because the numbers ARE real measurements, honestly scoped to this policy's own
//!   `dev-workstation-v1` device class (the class those numbers were actually taken on) rather than
//!   projected onto a device nobody has benchmarked.
//! - **`pack_size_max_bytes`, `Calibration::Placeholder`**: no full-scale (10^4-10^5 entry) pack
//!   has ever been built and measured end-to-end, so there is no real evidence to calibrate a
//!   device-storage-appropriate cap against. Borrows `crate::health::severity_for_size_bytes`'s own
//!   production-readiness threshold (100,000,000 bytes) as a starting reference point ONLY, because
//!   that is the one artifact-size policy already declared anywhere in this repo — not itself
//!   derived from a device memory/storage budget.
//! - **`lexicon_min_entries`, `Calibration::Placeholder`**: no full-scale reference grammar has
//!   been compiled and certified end-to-end yet (a 10^4-10^5
//!   entry design target is a goal, not a measurement). Seeded low (1,000) as a clearly-provisional
//!   floor pending a real study, not a claimed target.
//! - **`coverage_min_analysis_rate`, `Calibration::Placeholder`**: no held-out corpus has ever
//!   been measured against any grammar under this scheme. 0.90 is a conventional discussion-starter
//!   bar, not derived from evidence.
//!
//! # Versioning
//! `THRESHOLD_POLICY_SCHEMA_VERSION` is this module's own wire-shape version (bumped only on a
//! wire-incompatible change to `ThresholdPolicy`'s shape); `ThresholdPolicy::policy_id` is the
//! POLICY-CONTENT version a verdict cites (bumped whenever the seeded *values* change, independent
//! of the wire shape) — mirroring `pg-pack::manifest`'s own two-independent-version convention
//! (`MANIFEST_SCHEMA_VERSION` vs. `RequiredRuntimeFeatures::payload_format_version`).

use serde::{Deserialize, Serialize};

/// This module's own wire-shape version. Bump only on a wire-incompatible change to
/// `ThresholdPolicy`/`Calibration`/`Threshold`'s shape.
pub const THRESHOLD_POLICY_SCHEMA_VERSION: u32 = 1;

/// Whether a threshold's value is backed by real, cited measurement, or is an explicitly-marked,
/// un-calibrated placeholder. See this module's doc for the full discipline this encodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Calibration {
    /// Backed by real measured evidence; `citation` must be specific enough to re-derive or re-check, never a bare "measured".
    Measured { citation: String },
    /// Not yet backed by measurement; `rationale` states why the value is not arbitrary even though no evidence backs it yet, and must never be empty or phrased to look authoritative.
    Placeholder { rationale: String },
}

impl Calibration {
    /// `true` for `Calibration::Placeholder` — the un-calibrated case a report must surface
    /// plainly rather than silently rendering identically to a measured value.
    pub fn is_placeholder(&self) -> bool {
        matches!(self, Calibration::Placeholder { .. })
    }

    fn measured(citation: impl Into<String>) -> Self {
        Calibration::Measured {
            citation: citation.into(),
        }
    }

    fn placeholder(rationale: impl Into<String>) -> Self {
        Calibration::Placeholder {
            rationale: rationale.into(),
        }
    }
}

/// One threshold value, paired with the `Calibration` that justifies it. Generic so the same
/// shape covers byte counts, entry counts, rates, and millisecond durations without four
/// near-identical structs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Threshold<T> {
    pub value: T,
    pub calibration: Calibration,
}

impl<T> Threshold<T> {
    pub fn new(value: T, calibration: Calibration) -> Self {
        Threshold { value, calibration }
    }
}

/// The declared, versioned threshold policy: pack size, lexicon scale, token analysis rate, and
/// p50/p90/p99 latency against a named device class. A [`crate::
/// readiness_verdict::ReadinessReport`] records `ThresholdPolicy::policy_id` so an older
/// certificate stays interpretable after the numbers move.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThresholdPolicy {
    /// This schema's wire-shape version at the time this policy was produced.
    pub schema_version: u32,
    /// The POLICY-CONTENT version this set of values represents (independent of `schema_version`
    /// — see this module's doc). Bump whenever a seeded value changes after review; never bump
    /// silently as part of an unrelated change.
    pub policy_id: String,
    /// The target device class every latency threshold and result is measured against, so a
    /// result is never read as evidence about an unbenchmarked device.
    /// Freeform, matching this crate's own predicate/construct/witness convention of
    /// stable strings over a premature enum.
    pub device_class: String,
    /// Maximum artifact (pack) size, in bytes.
    pub pack_size_max_bytes: Threshold<u64>,
    /// Minimum lexicon scale, in lexical entries.
    pub lexicon_min_entries: Threshold<u64>,
    /// Minimum token-level analysis rate (fraction of tokens receiving at least one analysis,
    /// `0.0..=1.0`) on a held-out corpus. See `crate::readiness_verdict`'s own doc for why this
    /// is never worded as accuracy.
    pub coverage_min_analysis_rate: Threshold<f64>,
    /// Maximum p50 (median) per-word latency, in milliseconds, against `device_class`.
    pub latency_p50_max_ms: Threshold<f64>,
    /// Maximum p90 per-word latency, in milliseconds, against `device_class`.
    pub latency_p90_max_ms: Threshold<f64>,
    /// Maximum p99 per-word latency, in milliseconds, against `device_class`.
    pub latency_p99_max_ms: Threshold<f64>,
}

impl ThresholdPolicy {
    /// Canonical machine-readable form: pretty-printed, two-space indent, fields in Rust
    /// declaration order — the same "canonical JSON" convention `crate::health`/`crate::
    /// coverage_ledger` already establish.
    pub fn to_canonical_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("ThresholdPolicy serialization is infallible")
    }

    /// Parses a policy from its canonical JSON form.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }
}

/// Today's seeded policy — "v1". See this module's top doc for exactly what backs each value.
/// Calling this function twice returns equal, deterministic policies (no clock/env dependence),
/// which is required for the golden-certificate regeneration precedent (`crate::readiness_verdict`)
/// to be reproducible.
pub fn policy_v1() -> ThresholdPolicy {
    ThresholdPolicy {
        schema_version: THRESHOLD_POLICY_SCHEMA_VERSION,
        policy_id: "readiness-policy-v1".to_string(),
        device_class: "dev-workstation-v1 (Windows 11 x64, single-threaded `pangloss batch \
            --threads 1`, release build -- the exact configuration docs/benchmark-matrix.md \
            measured 2026-07-26 at commit 85f25dc, and the machine rust/tools/typology-speedup.sh \
            runs on locally. NOT a mobile/embedded target device -- no such device has been \
            benchmarked yet, and a certificate under this policy version must not be read as \
            evidence about any other device class.)"
            .to_string(),
        pack_size_max_bytes: Threshold::new(
            100_000_000,
            Calibration::placeholder(
                "No full-scale (10^4-10^5 entry) .pgpack has been built and measured end-to-end, \
                 so there is no real evidence to calibrate a device-storage-appropriate cap \
                 against. Borrows crate::health::severity_for_size_bytes's own R6 production-readiness \
                 band edge (100,000,000 bytes) as a starting reference point only -- that is the \
                 one artifact-size policy already declared in this repo, not itself derived from \
                 a device memory/storage budget. Replace once a real pack-size-vs-device-capacity \
                 study exists (see calibrate-fst-resource-envelopes for the FST-internal analogue \
                 of this same discipline).",
            ),
        ),
        lexicon_min_entries: Threshold::new(
            1_000,
            Calibration::placeholder(
                "No full-scale reference grammar (10^4-10^5 entries, this project's own stated \
                 design target) has been compiled and certified end-to-end yet, so there is no \
                 measured evidence of what lexicon scale a device-viable language actually needs. \
                 Seeded at 1,000 entries as a low, clearly-provisional floor pending a real study \
                 -- a placeholder, not a target.",
            ),
        ),
        coverage_min_analysis_rate: Threshold::new(
            0.90,
            Calibration::placeholder(
                "No held-out corpus has ever been measured against any grammar under this scheme, \
                 so there is no evidence to calibrate against. 0.90 is a conventional NLP \
                 discussion-starter coverage bar, chosen only as a starting point, not derived \
                 from any held-out measurement.",
            ),
        ),
        latency_p50_max_ms: Threshold::new(
            1.0,
            Calibration::measured(
                "docs/benchmark-matrix.md's foma-path (--allow-unproven) Indonesian measurement: \
                 p50 <1ms. Corroborated by the typology-speedup harness's compiled-engine column \
                 (rust/target/typology-speedup/typology-speedup.md, reproducible via \
                 rust/tools/typology-speedup.sh), which shows sub-millisecond medians across \
                 nearly every tiny synthetic edge-case/typology fixture. Evidence is thin (one \
                 force-compiled reference grammar, tiny synthetic fixtures, not a full-scale \
                 lexicon) -- an early, revisable estimate, not a robust calibration.",
            ),
        ),
        latency_p90_max_ms: Threshold::new(
            5.0,
            Calibration::measured(
                "docs/benchmark-matrix.md only reports p50/p95/p99, not p90; this value sits \
                 between the source's Indonesian oracle p50 (<1ms) and p95 (5ms) figures as a \
                 conservative interpolation, and is well above the foma-path (--allow-unproven) \
                 p95 of 1ms observed on the same corpus. Same thinness caveat as \
                 latency_p50_max_ms.",
            ),
        ),
        latency_p99_max_ms: Threshold::new(
            20.0,
            Calibration::measured(
                "docs/benchmark-matrix.md: Indonesian foma-path (--allow-unproven) p99 1ms, max \
                 8ms; Indonesian default-engine (oracle) p99 16ms as a cross-check. 20ms adds \
                 explicit headroom over both observed numbers. Same thinness caveat as \
                 latency_p50_max_ms -- this is a single reference grammar's force-compiled \
                 measurement, not a broad calibration.",
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_v1_is_stamped_with_the_current_schema_version() {
        assert_eq!(policy_v1().schema_version, THRESHOLD_POLICY_SCHEMA_VERSION);
    }

    #[test]
    fn policy_v1_names_a_device_class() {
        let policy = policy_v1();
        assert!(!policy.device_class.trim().is_empty());
    }

    #[test]
    fn policy_v1_is_deterministic() {
        assert_eq!(policy_v1(), policy_v1());
    }

    #[test]
    fn policy_v1_round_trips_through_canonical_json() {
        let policy = policy_v1();
        let json = policy.to_canonical_json();
        let parsed = ThresholdPolicy::from_json(&json).expect("valid policy JSON must parse");
        assert_eq!(parsed, policy);
    }

    #[test]
    fn to_canonical_json_is_deterministic() {
        let policy = policy_v1();
        assert_eq!(policy.to_canonical_json(), policy.to_canonical_json());
    }

    /// The type system can't stop a caller writing `rationale: ""`, so this pins that today's seed values actually carry a non-empty citation/rationale.
    #[test]
    fn every_seeded_threshold_names_its_calibration_honestly() {
        let policy = policy_v1();
        let calibrations: Vec<(&str, &Calibration)> = vec![
            (
                "pack_size_max_bytes",
                &policy.pack_size_max_bytes.calibration,
            ),
            (
                "lexicon_min_entries",
                &policy.lexicon_min_entries.calibration,
            ),
            (
                "coverage_min_analysis_rate",
                &policy.coverage_min_analysis_rate.calibration,
            ),
            ("latency_p50_max_ms", &policy.latency_p50_max_ms.calibration),
            ("latency_p90_max_ms", &policy.latency_p90_max_ms.calibration),
            ("latency_p99_max_ms", &policy.latency_p99_max_ms.calibration),
        ];
        for (name, calibration) in calibrations {
            match calibration {
                Calibration::Measured { citation } => assert!(
                    !citation.trim().is_empty(),
                    "{name}'s Measured calibration must cite real evidence, not an empty string"
                ),
                Calibration::Placeholder { rationale } => assert!(
                    !rationale.trim().is_empty(),
                    "{name}'s Placeholder calibration must name a rationale, not an empty string"
                ),
            }
        }
    }

    /// Pack size, lexicon scale, and coverage rate have no measured evidence yet, so they must be `Placeholder`, not accidentally `Measured`.
    #[test]
    fn unmeasured_dimensions_are_placeholders_not_measured() {
        let policy = policy_v1();
        assert!(policy.pack_size_max_bytes.calibration.is_placeholder());
        assert!(policy.lexicon_min_entries.calibration.is_placeholder());
        assert!(policy
            .coverage_min_analysis_rate
            .calibration
            .is_placeholder());
    }

    /// The latency thresholds have real cited evidence, so they must be `Measured`, not a default placeholder.
    #[test]
    fn latency_dimensions_are_measured_not_placeholders() {
        let policy = policy_v1();
        assert!(!policy.latency_p50_max_ms.calibration.is_placeholder());
        assert!(!policy.latency_p90_max_ms.calibration.is_placeholder());
        assert!(!policy.latency_p99_max_ms.calibration.is_placeholder());
    }
}
