use super::*;
use pg_health::health::HealthReport;
use pg_pack::LicenseDeclaration;

fn synthetic_required(runtime_operations: Vec<String>) -> RequiredRuntimeFeatures {
    RequiredRuntimeFeatures {
        payload_format_version: pg_pack::CONTAINER_VERSION,
        runtime_operations,
        foma_feature_level: FOMA_FEATURE_LEVEL,
        hc_port_semver: this_crate_semver(),
        extensions: Vec::new(),
    }
}

fn synthetic_manifest(
    required_runtime_features: RequiredRuntimeFeatures,
    runtime_payload: &[u8],
    foma_payload: &[u8],
) -> PackManifest {
    PackManifest {
        manifest_schema_version: pg_pack::MANIFEST_SCHEMA_VERSION,
        grammar_id: "synthetic-wasm-wiring-grammar".to_string(),
        package_fingerprint: pg_pack::fingerprint_hex(runtime_payload, foma_payload),
        required_runtime_features,
        fst_health: HealthReport::new(Vec::new()),
        backend_assessments: vec![],
        license: None::<LicenseDeclaration>,
        created_by: "synthetic-test-builder".to_string(),
        created_at: "2026-07-25T00:00:00Z".to_string(),
        signature: None,
    }
}

const RUNTIME_PAYLOAD: &[u8] = b"synthetic-rust-hermitcrab-runtime-payload-bytes";
const FOMA_PAYLOAD: &[u8] = b"synthetic-opaque-foma-binary-memory-payload-bytes";

#[test]
fn pack_whose_required_features_are_a_subset_of_provided_loads() {
    let manifest = synthetic_manifest(
        synthetic_required(vec![OP_REDUPLICATION_PEEL.to_string()]),
        RUNTIME_PAYLOAD,
        FOMA_PAYLOAD,
    );
    let bytes = pg_pack::write_pack(&manifest, RUNTIME_PAYLOAD, FOMA_PAYLOAD).unwrap();

    let loaded = load_pack(&bytes).expect("required ⊆ provided must load");
    assert_eq!(loaded.manifest, manifest);
    assert_eq!(loaded.runtime_payload, RUNTIME_PAYLOAD);
    assert_eq!(loaded.foma_payload, FOMA_PAYLOAD);
    assert_eq!(loaded.signature_state, SignatureState::Unsigned);
    assert_eq!(
        loaded.fst_health_admission(),
        pg_health::health::Severity::WithinLimits
    );
}

#[test]
fn pack_requiring_an_unprovided_runtime_operation_is_rejected_with_typed_diagnostic() {
    let manifest = synthetic_manifest(
        synthetic_required(vec!["pg.brand-new.unimplemented-op".to_string()]),
        RUNTIME_PAYLOAD,
        FOMA_PAYLOAD,
    );
    let bytes = pg_pack::write_pack(&manifest, RUNTIME_PAYLOAD, FOMA_PAYLOAD).unwrap();

    let err = load_pack(&bytes).expect_err("an unprovided runtime operation must be refused");
    match err {
        PackLoadError::IncompatibleRuntimeFeatures { required, provided } => {
            assert!(required
                .runtime_operations
                .contains(&"pg.brand-new.unimplemented-op".to_string()));
            assert!(!provided
                .runtime_operations
                .contains(&"pg.brand-new.unimplemented-op".to_string()));
        }
        other => panic!("expected IncompatibleRuntimeFeatures, got {other:?}"),
    }
}

#[test]
fn pack_requiring_a_newer_hc_port_semver_than_this_build_provides_is_rejected() {
    let mut required = synthetic_required(Vec::new());
    required.hc_port_semver = (
        this_crate_semver().0,
        this_crate_semver().1 + 1,
        this_crate_semver().2,
    );
    let manifest = synthetic_manifest(required, RUNTIME_PAYLOAD, FOMA_PAYLOAD);
    let bytes = pg_pack::write_pack(&manifest, RUNTIME_PAYLOAD, FOMA_PAYLOAD).unwrap();
    assert!(matches!(
        load_pack(&bytes),
        Err(PackLoadError::IncompatibleRuntimeFeatures { .. })
    ));
}

#[test]
fn a_malformed_container_never_reaches_the_containment_check() {
    let mut bytes = b"not a real pgpack container at all, far too short".to_vec();
    bytes.truncate(4);
    assert!(matches!(
        load_pack(&bytes),
        Err(PackLoadError::Container(_))
    ));
}

// Signature state is reported, never gates: it plays no role in the containment decision.
#[test]
fn signature_state_is_independent_of_runtime_feature_compatibility() {
    let manifest = synthetic_manifest(
        synthetic_required(vec![OP_REDUPLICATION_PEEL.to_string()]),
        RUNTIME_PAYLOAD,
        FOMA_PAYLOAD,
    );
    let manifest_no_sig_json = manifest.to_canonical_json();
    let message = pg_pack::sign(&[3u8; 32], &manifest_no_sig_json.into_bytes(), None);
    // Deliberately signed over the wrong bytes, so this reports `Invalid` and still loads.
    let mut manifest = manifest;
    manifest.signature = Some(message);
    let bytes = pg_pack::write_pack(&manifest, RUNTIME_PAYLOAD, FOMA_PAYLOAD).unwrap();

    let loaded = load_pack(&bytes).expect("an invalid signature must not block loading");
    assert_eq!(loaded.signature_state, SignatureState::Invalid);
}

#[test]
fn provided_runtime_features_declares_this_containers_own_version() {
    let provided = provided_runtime_features();
    assert!(provided
        .payload_format_versions
        .contains(&pg_pack::CONTAINER_VERSION));
    assert!(provided
        .runtime_operations
        .contains(&OP_REDUPLICATION_PEEL.to_string()));
}
