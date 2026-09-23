use super::*;
use crate::compat::RequiredRuntimeFeatures;
use pg_health::health::HealthReport;

fn synthetic_manifest_for(runtime_payload: &[u8], foma_payload: &[u8]) -> PackManifest {
    PackManifest {
        manifest_schema_version: crate::manifest::MANIFEST_SCHEMA_VERSION,
        grammar_id: "synthetic-stress-grammar".to_string(),
        package_fingerprint: fingerprint_hex(runtime_payload, foma_payload),
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

const SYNTHETIC_RUNTIME_PAYLOAD: &[u8] = b"synthetic-rust-hermitcrab-runtime-payload-bytes";
const SYNTHETIC_FOMA_PAYLOAD: &[u8] = b"synthetic-opaque-foma-binary-memory-payload-bytes";

// --- Round trip ---

#[test]
fn round_trip_write_then_read_is_identical() {
    let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let bytes = write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
    let read = read_pack(&bytes).unwrap();
    assert_eq!(read.manifest, manifest);
    assert_eq!(read.runtime_payload, SYNTHETIC_RUNTIME_PAYLOAD);
    assert_eq!(read.foma_payload, SYNTHETIC_FOMA_PAYLOAD);
    assert_eq!(read.signature_state, SignatureState::Unsigned);
}

#[test]
fn write_pack_rejects_stale_manifest_schema() {
    let mut manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    manifest.manifest_schema_version = 5;

    let error = write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD)
        .expect_err("stale manifest schema must not be written");
    assert_eq!(error, PgPackError::UnsupportedManifestSchema { found: 5 });
}

#[test]
fn read_pack_rejects_stale_manifest_schema() {
    let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let mut bytes =
        write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();

    let mut stale_value: serde_json::Value =
        serde_json::from_str(&manifest.to_canonical_json()).expect("valid manifest JSON");
    stale_value["manifest_schema_version"] = serde_json::json!(5);
    let stale_json = serde_json::to_string_pretty(&stale_value)
        .expect("stale manifest JSON serialization must succeed");
    let current_json_len = manifest.to_canonical_json().len();
    assert_eq!(
        stale_json.len(),
        current_json_len,
        "schema v5 and v8 fixtures must retain the same framed manifest length"
    );

    bytes[HEADER_LEN..HEADER_LEN + stale_json.len()].copy_from_slice(stale_json.as_bytes());
    let digest_offset = bytes.len() - DIGEST_LEN;
    let digest = Sha256::digest(&bytes[..digest_offset]);
    bytes[digest_offset..].copy_from_slice(&digest);

    let error = read_pack(&bytes).expect_err("stale manifest schema must not be read");
    assert_eq!(error, PgPackError::UnsupportedManifestSchema { found: 5 });
}

#[test]
fn write_pack_rejects_stale_embedded_health_schema() {
    let mut manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    manifest.fst_health.schema_version = 6;

    let error = write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD)
        .expect_err("manifest v8 with stale embedded health schema v6 must not be written");
    assert_eq!(error, PgPackError::UnsupportedHealthSchema { found: 6 });
}

#[test]
fn read_pack_rejects_manifest_v8_with_stale_embedded_health_schema_v6() {
    let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let mut bytes =
        write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();

    let mut stale_value: serde_json::Value =
        serde_json::from_str(&manifest.to_canonical_json()).expect("valid manifest JSON");
    stale_value["fst_health"]["schema_version"] = serde_json::json!(6);
    let stale_json = serde_json::to_string_pretty(&stale_value)
        .expect("stale health JSON serialization must succeed");
    let current_json_len = manifest.to_canonical_json().len();
    assert_eq!(
        stale_json.len(),
        current_json_len,
        "health schema v6 and v7 fixtures must retain the same framed manifest length"
    );

    bytes[HEADER_LEN..HEADER_LEN + stale_json.len()].copy_from_slice(stale_json.as_bytes());
    let digest_offset = bytes.len() - DIGEST_LEN;
    let digest = Sha256::digest(&bytes[..digest_offset]);
    bytes[digest_offset..].copy_from_slice(&digest);

    let error = read_pack(&bytes)
        .expect_err("manifest v8 with stale embedded health schema v6 must not be read");
    assert_eq!(error, PgPackError::UnsupportedHealthSchema { found: 6 });
}

// --- Real foma binary-memory bytes (not just the plain-ASCII synthetic fixtures above) ---

/// A tiny, deterministic, real compiled foma network for exercising binary payload handling.
const REAL_LEXC_SOURCE: &str = "LEXICON Root\ncat # ;\ndog # ;\n";

fn compile_real_network() -> foma::types::Fsm {
    let opts = foma::options::FomaOptions::default();
    foma::lexcread::fsm_lexc_parse_string(&opts, None, REAL_LEXC_SOURCE)
        .expect("minimal lexc source must compile")
}

/// A real, gzip-compressed foma binary-memory payload, unlike the plain-ASCII `SYNTHETIC_FOMA_PAYLOAD` every other test uses — this format must handle genuine binary content just as well.
fn real_foma_payload_bytes() -> Vec<u8> {
    let net = compile_real_network();
    let mut bytes = Vec::new();
    foma::io::fsm_write_binary(&net, &mut bytes).expect("fsm_write_binary must succeed");
    bytes
}

#[test]
fn round_trip_with_real_foma_binary_payload_not_just_synthetic_ascii() {
    let real_foma = real_foma_payload_bytes();
    // Sanity: gzip magic bytes prove this exercises materially different bytes than `SYNTHETIC_FOMA_PAYLOAD` above.
    assert!(
        real_foma.len() >= 2 && real_foma[0] == 0x1f && real_foma[1] == 0x8b,
        "expected gzip magic bytes at the front of a real foma binary payload, got {:02x?}",
        &real_foma[..real_foma.len().min(4)]
    );

    let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, &real_foma);
    let bytes = write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, &real_foma).unwrap();
    let read = read_pack(&bytes).unwrap();
    assert_eq!(read.manifest, manifest);
    assert_eq!(read.runtime_payload, SYNTHETIC_RUNTIME_PAYLOAD);
    assert_eq!(read.foma_payload, real_foma);
    assert_eq!(read.signature_state, SignatureState::Unsigned);

    // Reconstruct from the PACKED bytes (never re-deriving from `REAL_LEXC_SOURCE`) and confirm state/arc counts and `apply_up` agree with an independent fresh compile.
    let reconstructed = foma::io::fsm_read_binary_mem(&read.foma_payload).expect(
        "a real foma payload read back out of this container must still be readable \
                     by fsm_read_binary_mem",
    );
    let original = compile_real_network();
    assert_eq!(reconstructed.statecount, original.statecount);
    assert_eq!(reconstructed.arccount, original.arccount);

    let mut original_handle = foma::apply::apply_init(&original);
    let mut reconstructed_handle = foma::apply::apply_init(&reconstructed);
    for word in ["cat", "dog"] {
        let original_out: Vec<String> = original_handle.up(word).collect();
        let reconstructed_out: Vec<String> = reconstructed_handle.up(word).collect();
        assert_eq!(
            original_out, reconstructed_out,
            "apply_up({word:?}) must agree between the original compile and the network \
                 reconstructed from this container's own packed bytes"
        );
        assert!(
            !original_out.is_empty(),
            "sanity: {word:?} is in REAL_LEXC_SOURCE's own lexicon"
        );
    }
}

#[test]
fn round_trip_with_signed_manifest() {
    let mut manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let manifest_no_sig_json = manifest.to_canonical_json();
    let message = signature::domain_separated_signed_bytes(
        CONTAINER_VERSION,
        manifest_no_sig_json.as_bytes(),
        SYNTHETIC_RUNTIME_PAYLOAD,
        SYNTHETIC_FOMA_PAYLOAD,
    );
    let seed = [3u8; 32];
    manifest.signature = Some(signature::sign(
        &seed,
        &message,
        Some("synthetic-key".to_string()),
    ));

    let bytes = write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
    let read = read_pack(&bytes).unwrap();
    assert_eq!(read.signature_state, SignatureState::Valid);
    assert_eq!(read.manifest, manifest);
}

// --- Bad magic / bad version ---

#[test]
fn rejects_bad_magic() {
    let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let mut bytes =
        write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
    bytes[0] = b'X';
    let err = read_pack(&bytes).unwrap_err();
    assert!(matches!(err, PgPackError::BadMagic { .. }));
}

#[test]
fn rejects_unsupported_version() {
    let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let mut bytes =
        write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
    bytes[MAGIC_LEN..MAGIC_LEN + VERSION_LEN].copy_from_slice(&999u32.to_le_bytes());
    // Version is read before the digest, so this is detected without needing a valid digest.
    let err = read_pack(&bytes).unwrap_err();
    assert!(matches!(
        err,
        PgPackError::UnsupportedVersion { found: 999 }
    ));
}

// --- Length exceeding the versioned limit, checked BEFORE allocation ---

#[test]
fn rejects_manifest_length_exceeding_versioned_limit_before_allocating() {
    let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let mut bytes =
        write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
    // Overwrite the declared length far beyond V1_LIMITS while leaving the buffer short, so an allocate-before-validate bug would panic instead of returning a clean typed error.
    let huge = V1_LIMITS.max_manifest_bytes + 1;
    bytes[MAGIC_LEN + VERSION_LEN..MAGIC_LEN + VERSION_LEN + LEN_FIELD_SIZE]
        .copy_from_slice(&huge.to_le_bytes());
    let err = read_pack(&bytes).unwrap_err();
    assert_eq!(
        err,
        PgPackError::LengthExceedsLimit {
            what: "manifest",
            declared: huge,
            limit: V1_LIMITS.max_manifest_bytes,
        }
    );
}

#[test]
fn rejects_runtime_payload_length_exceeding_versioned_limit() {
    let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let mut bytes =
        write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
    let huge = V1_LIMITS.max_runtime_payload_bytes + 1;
    let offset = MAGIC_LEN + VERSION_LEN + LEN_FIELD_SIZE;
    bytes[offset..offset + LEN_FIELD_SIZE].copy_from_slice(&huge.to_le_bytes());
    let err = read_pack(&bytes).unwrap_err();
    assert_eq!(
        err,
        PgPackError::LengthExceedsLimit {
            what: "runtime payload",
            declared: huge,
            limit: V1_LIMITS.max_runtime_payload_bytes,
        }
    );
}

#[test]
fn rejects_total_length_exceeding_versioned_total_limit_without_exceeding_any_single_section() {
    let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let mut bytes =
        write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
    // Each individual declared length stays within its own per-section limit, but their sum exceeds the total-package limit.
    let big_runtime = V1_LIMITS.max_runtime_payload_bytes;
    let big_foma = V1_LIMITS.max_foma_payload_bytes;
    assert!(big_runtime + big_foma > V1_LIMITS.max_total_bytes);
    let runtime_offset = MAGIC_LEN + VERSION_LEN + LEN_FIELD_SIZE;
    bytes[runtime_offset..runtime_offset + LEN_FIELD_SIZE]
        .copy_from_slice(&big_runtime.to_le_bytes());
    let foma_offset = runtime_offset + LEN_FIELD_SIZE;
    bytes[foma_offset..foma_offset + LEN_FIELD_SIZE].copy_from_slice(&big_foma.to_le_bytes());
    let err = read_pack(&bytes).unwrap_err();
    assert!(matches!(err, PgPackError::TotalLengthExceedsLimit { .. }));
}

// --- Truncated payload ---

#[test]
fn rejects_truncated_payload() {
    let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let bytes = write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
    let truncated = &bytes[..bytes.len() - 10];
    let err = read_pack(truncated).unwrap_err();
    assert!(matches!(err, PgPackError::Truncated { .. }));
}

#[test]
fn rejects_container_shorter_than_fixed_header() {
    let err = read_pack(&[1, 2, 3]).unwrap_err();
    assert!(matches!(err, PgPackError::TooShort { .. }));
}

#[test]
fn rejects_trailing_bytes() {
    let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let mut bytes =
        write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
    bytes.push(0xFF);
    let err = read_pack(&bytes).unwrap_err();
    assert!(matches!(err, PgPackError::TrailingBytes { extra: 1 }));
}

// --- Tamper: SHA-256 digest mismatch ---

#[test]
fn rejects_tampered_content_via_digest_mismatch() {
    let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let mut bytes =
        write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
    // Flip a byte inside the runtime payload without touching any length prefix or the digest.
    let header_and_manifest = HEADER_LEN + manifest.to_canonical_json().len();
    bytes[header_and_manifest] ^= 0xFF;
    let err = read_pack(&bytes).unwrap_err();
    assert_eq!(err, PgPackError::DigestMismatch);
}

// --- Fingerprint mismatch: payloads swapped across "grammars" while the manifest is untouched ---

#[test]
fn rejects_mismatched_fingerprint_when_payloads_are_swapped() {
    let manifest_a = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    // Constructed directly, bypassing `write_pack`'s own fingerprint check, to prove `read_pack` itself catches payloads mixed across grammars.
    let other_foma_payload: &[u8] = b"synthetic-different-grammar-foma-payload";
    let manifest_json = manifest_a.to_canonical_json();
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
    out.extend_from_slice(&(manifest_json.len() as u64).to_le_bytes());
    out.extend_from_slice(&(SYNTHETIC_RUNTIME_PAYLOAD.len() as u64).to_le_bytes());
    out.extend_from_slice(&(other_foma_payload.len() as u64).to_le_bytes());
    out.extend_from_slice(manifest_json.as_bytes());
    out.extend_from_slice(SYNTHETIC_RUNTIME_PAYLOAD);
    out.extend_from_slice(other_foma_payload);
    let digest = Sha256::digest(&out);
    out.extend_from_slice(&digest);

    // The whole-file digest is freshly recomputed over the swapped content, so only the fingerprint check catches the mismatch.
    let err = read_pack(&out).unwrap_err();
    assert_eq!(err, PgPackError::FingerprintMismatch);
}

#[test]
fn write_pack_rejects_caller_supplied_manifest_with_wrong_fingerprint() {
    let mut manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    manifest.package_fingerprint = "0".repeat(64);
    let err = write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap_err();
    assert_eq!(err, PgPackError::FingerprintMismatch);
}

// --- Signature state: unsigned / valid / invalid, and invalid never blocks the read ---

#[test]
fn unsigned_pack_reports_unsigned_and_reads_successfully() {
    let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let bytes = write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
    let read = read_pack(&bytes).unwrap();
    assert_eq!(read.signature_state, SignatureState::Unsigned);
}

#[test]
fn invalidly_signed_pack_still_reads_successfully_and_reports_invalid() {
    let mut manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    // Sign with one key, then swap in a different key's public key, so verification fails.
    let manifest_no_sig_json = manifest.to_canonical_json();
    let message = signature::domain_separated_signed_bytes(
        CONTAINER_VERSION,
        manifest_no_sig_json.as_bytes(),
        SYNTHETIC_RUNTIME_PAYLOAD,
        SYNTHETIC_FOMA_PAYLOAD,
    );
    let signing_seed = [3u8; 32];
    let mut block = signature::sign(&signing_seed, &message, None);
    let other_seed = [5u8; 32];
    let other_block = signature::sign(&other_seed, &message, None);
    block.public_key_hex = other_block.public_key_hex;
    manifest.signature = Some(block);

    let bytes = write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
    // Must NOT be an Err: an invalid signature never blocks reading/analysis.
    let read = read_pack(&bytes).expect("an invalid signature must not block reading");
    assert_eq!(read.signature_state, SignatureState::Invalid);
    assert_eq!(read.runtime_payload, SYNTHETIC_RUNTIME_PAYLOAD);
    assert_eq!(read.foma_payload, SYNTHETIC_FOMA_PAYLOAD);
}

#[test]
fn validly_signed_pack_reads_successfully_and_reports_valid() {
    let mut manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
    let manifest_no_sig_json = manifest.to_canonical_json();
    let message = signature::domain_separated_signed_bytes(
        CONTAINER_VERSION,
        manifest_no_sig_json.as_bytes(),
        SYNTHETIC_RUNTIME_PAYLOAD,
        SYNTHETIC_FOMA_PAYLOAD,
    );
    let seed = [11u8; 32];
    manifest.signature = Some(signature::sign(&seed, &message, None));

    let bytes = write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
    let read = read_pack(&bytes).expect("a validly signed pack must read successfully");
    assert_eq!(read.signature_state, SignatureState::Valid);
}
