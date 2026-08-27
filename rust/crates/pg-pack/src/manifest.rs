//! The `.pgpack` **pack manifest**: carries package/grammar identity, payload format versions,
//! the required-runtime-feature set, an FST-health admission/findings field, creation
//! metadata, and a versioned licensing/authenticity
//! section. "Pack manifest" is the per-`.pgpack` blob's own name -- distinct from the
//! source-controlled capability registry; bare unqualified "manifest" is banned -- every doc
//! comment in this crate uses the full term.
//!
//! Field declaration order below follows this crate's own [serde] default
//! (unmodified struct-field order), the same "canonical JSON" convention `pg-snapshot` and
//! `pg_foma::health` already use.

use serde::{Deserialize, Serialize};

use crate::compat::RequiredRuntimeFeatures;
use crate::license::LicenseDeclaration;
use crate::signature::SignatureBlock;
use pg_foma::advice_catalog::RemedyEffort;
use pg_foma::backend_selection::{BackendReport, BackendSelection, BackendStatus};
use pg_foma::capability::CompileDecision;
use pg_foma::health::{HealthFinding, HealthReport, Metric, MetricValue, ValueProvenance};

/// This manifest schema's own version. Bump only on a wire-incompatible change to
/// `PackManifest`'s shape — independent of `crate::format::CONTAINER_VERSION` (the container
/// framing) and of `crate::compat::RequiredRuntimeFeatures::payload_format_version` (the
/// runtime-payload format), which each version separately.
/// Bumped to 8 because the embedded FST-health report moved to health schema v7.
pub const MANIFEST_SCHEMA_VERSION: u32 = 8;

/// One catalog remedy linked to the grammar shape it addresses for one backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendAdviceReference {
    pub shape_key: String,
    pub remedy_key: String,
    pub effort: RemedyEffort,
}

/// One observed or predicted cost contributing to a backend's report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackendCostEvidence {
    pub metric: Metric,
    pub value: MetricValue,
    pub threshold: Option<MetricValue>,
    pub provenance: ValueProvenance,
}

/// The complete diagnostic record for one backend, including failed backends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackendAssessment {
    pub backend: String,
    pub decision: String,
    pub status: String,
    pub findings: Vec<HealthFinding>,
    pub failed_predicates: Vec<String>,
    pub shapes: Vec<String>,
    pub cost_evidence: Vec<BackendCostEvidence>,
    pub advice_references: Vec<BackendAdviceReference>,
    pub status_detail: Option<String>,
}

impl BackendAssessment {
    /// One backend's Compatibility report, projected into the manifest shape.
    ///
    /// Lives here rather than in a caller because the manifest owns this shape: a second consumer
    /// would otherwise copy the label spellings, and `"admit"`/`"accepted"` are wire values that
    /// must not drift. `pg-foma` cannot host it -- that would need `pg-pack` in its dependencies,
    /// and the dependency runs the other way.
    ///
    /// `cost_evidence` is always empty. `BackendReport` no longer carries per-backend cost
    /// measurements, so the honest projection is an empty vector rather than an invented one.
    pub fn from_report(report: &BackendReport) -> Self {
        BackendAssessment {
            backend: report.strategy().label().to_string(),
            decision: decision_label(report.decision()).to_string(),
            status: backend_status_label(report.status()).to_string(),
            findings: report.findings().to_vec(),
            failed_predicates: report.failed_predicates().to_vec(),
            shapes: report.shapes().to_vec(),
            cost_evidence: Vec::new(),
            advice_references: report
                .advice_references()
                .iter()
                .map(|reference| BackendAdviceReference {
                    shape_key: reference.shape_key.clone(),
                    remedy_key: reference.remedy_key.clone(),
                    effort: reference.effort,
                })
                .collect(),
            status_detail: report.status_detail().map(str::to_string),
        }
    }
}

/// Every backend's report in manifest shape, one entry per backend and none singled out.
pub fn backend_assessments(selection: &BackendSelection) -> Vec<BackendAssessment> {
    selection
        .reports()
        .iter()
        .map(BackendAssessment::from_report)
        .collect()
}

/// The manifest's wire spelling for a `CompileDecision`.
fn decision_label(decision: &CompileDecision) -> &'static str {
    match decision {
        CompileDecision::Admit => "admit",
        CompileDecision::ConfirmOnly => "confirm_only",
        CompileDecision::Refuse(_) => "refuse",
    }
}

/// The manifest's wire spelling for a `BackendStatus`.
fn backend_status_label(status: BackendStatus) -> &'static str {
    match status {
        BackendStatus::Accepted => "accepted",
        BackendStatus::Refused => "refused",
        BackendStatus::Missing => "missing",
        BackendStatus::Failed => "failed",
    }
}

/// The `.pgpack` pack manifest: canonical JSON, embedded length-prefixed in the container by
/// `crate::format::write_pack`. Every field this module's own doc names has a slot
/// here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackManifest {
    /// `MANIFEST_SCHEMA_VERSION` at the time this manifest was produced.
    pub manifest_schema_version: u32,
    /// A stable identifier for the grammar this pack was compiled from (package/grammar identity;
    /// freeform — this schema step does not mint a grammar-ID registry).
    pub grammar_id: String,
    /// Lowercase-hex SHA-256 over both framed payloads (see `crate::format::fingerprint_hex`
    /// for the exact bytes hashed). Binds the runtime and foma payloads together so they cannot be
    /// mixed across grammars — `crate::format::read_pack` recomputes this from the payload
    /// bytes it actually read and rejects a mismatch as
    /// `crate::format::PgPackError::FingerprintMismatch`, independent of the container's own
    /// whole-file SHA-256 structural-integrity digest.
    pub package_fingerprint: String,
    /// The required-runtime-feature set this pack was built against.
    pub required_runtime_features: RequiredRuntimeFeatures,
    /// The FST-health raw admission/findings/audit-record report (reusing
    /// `pg_foma::health::HealthReport`/`Severity`/`HealthReport::admission` verbatim --
    /// never redefined here).
    pub fst_health: HealthReport,
    /// Findings and advice for every considered backend, successful or failed.
    pub backend_assessments: Vec<BackendAssessment>,
    /// Optional license declaration: declaration/provenance only; never gates analysis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<LicenseDeclaration>,
    /// Free-form creation metadata: who/what produced this pack.
    pub created_by: String,
    /// Free-form creation timestamp; this avoids a timestamp type dependency in the manifest
    /// schema.
    pub created_at: String,
    /// Optional publisher signature. `None` means this pack is unsigned
    /// (`crate::signature::SignatureState::Unsigned`). Always the **last** field serialized so
    /// the "manifest excluding its signature value" bytes `crate::signature::sign`/
    /// `crate::signature::verify` need are a simple prefix-truncation... in practice
    /// `PackManifest::without_signature` clones with `signature: None` instead, so this field's
    /// position does not matter for correctness, only for reading convenience.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<SignatureBlock>,
}

impl PackManifest {
    /// Canonical machine-readable form: pretty-printed, two-space indent, fields in Rust
    /// declaration order (serde's unmodified default) — the same "canonical JSON" convention
    /// `pg_snapshot::Snapshot::to_json`/`pg_foma::health::HealthReport::to_json` already
    /// establish. Infallible for the same reason `Snapshot::to_json` is: every field here is a
    /// plain data type with a total `Serialize` impl.
    pub fn to_canonical_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("PackManifest serialization is infallible")
    }

    /// Parses a pack manifest from its canonical JSON form. Returns `serde_json::Error` directly
    /// (not a manifest-specific error type) — callers needing container-level typed errors go
    /// through `crate::format::read_pack`, which wraps this.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    /// A clone of this manifest with `signature` cleared, exactly the bytes
    /// `crate::signature::sign`/`crate::signature::verify` must operate on: the pack
    /// manifest excluding its signature value.
    pub fn without_signature(&self) -> Self {
        let mut cleared = self.clone();
        cleared.signature = None;
        cleared
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::RequiredRuntimeFeatures;
    use pg_foma::health::HealthReport;

    fn synthetic_manifest() -> PackManifest {
        PackManifest {
            manifest_schema_version: MANIFEST_SCHEMA_VERSION,
            grammar_id: "synthetic-stress-grammar".to_string(),
            package_fingerprint: "0".repeat(64),
            required_runtime_features: RequiredRuntimeFeatures {
                payload_format_version: 1,
                runtime_operations: vec!["synthetic.reduplication.peel".to_string()],
                foma_feature_level: 1,
                hc_port_semver: (1, 0, 0),
                extensions: vec![],
            },
            fst_health: HealthReport::new(vec![]),
            backend_assessments: vec![],
            license: None,
            created_by: "synthetic-test-builder".to_string(),
            created_at: "2026-07-24T00:00:00Z".to_string(),
            signature: None,
        }
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let manifest = synthetic_manifest();
        let json = manifest.to_canonical_json();
        let parsed = PackManifest::from_json(&json).expect("valid manifest JSON must parse");
        assert_eq!(parsed, manifest);
    }

    #[test]
    fn without_signature_clears_only_signature_field() {
        let mut manifest = synthetic_manifest();
        manifest.signature = Some(crate::signature::SignatureBlock {
            algorithm: "ed25519".to_string(),
            public_key_hex: "aa".repeat(32),
            signature_hex: "bb".repeat(64),
            key_id: None,
        });
        let cleared = manifest.without_signature();
        assert!(cleared.signature.is_none());
        assert_eq!(cleared.grammar_id, manifest.grammar_id);
        assert_eq!(cleared.package_fingerprint, manifest.package_fingerprint);
    }

    #[test]
    fn to_canonical_json_is_deterministic() {
        let manifest = synthetic_manifest();
        assert_eq!(manifest.to_canonical_json(), manifest.to_canonical_json());
    }
}
