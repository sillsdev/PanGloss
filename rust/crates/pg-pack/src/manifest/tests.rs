use super::*;
use crate::compat::RequiredRuntimeFeatures;
use pg_health::health::HealthReport;

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
