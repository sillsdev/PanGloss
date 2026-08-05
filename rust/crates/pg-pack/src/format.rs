//! The `.pgpack` container's exact physical byte layout. This module owns `write_pack`/`read_pack` and every typed failure
//! `PgPackError` names; nothing above this module touches raw bytes.
//!
//! # Byte layout (container version 1)
//!
//! ```text
//! offset  size   field
//! 0       8      magic            fixed PanGloss magic bytes (MAGIC)
//! 8       4      version          u32, little-endian (CONTAINER_VERSION)
//! 12      8      manifest_len     u64, little-endian
//! 20      8      runtime_len      u64, little-endian
//! 28      8      foma_len         u64, little-endian
//! 36      ..     manifest_bytes   manifest_len bytes: canonical UTF-8 JSON pack manifest
//! ..      ..     runtime_bytes    runtime_len bytes: opaque Rust-HermitCrab runtime payload
//! ..      ..     foma_bytes       foma_len bytes: opaque existing-foma binary-memory payload
//! ..      32     digest           SHA-256 over every byte at offset 0 up to (not including) this
//!                                 field -- i.e. magic+version+all three length prefixes+all three
//!                                 payload sections.
//! ```
//!
//! **Judgment call: the three length prefixes are grouped in a fixed-size header** (offsets
//! 12..36), rather than interleaved immediately before each of their own sections. Each section is
//! length-prefixed without mandating interleaving; grouping them
//! together is what makes the hard rule -- "EVERY length validated against versioned limits
//! BEFORE allocation" -- straightforward to enforce as a single up-front pass: `read_pack` reads
//! and validates all three declared lengths (bounds, overflow, per-section limit, total-package
//! limit) using only fixed-offset, fixed-size reads (never a length-dependent slice) before it
//! computes a single "does this container actually contain that many more bytes" check and only
//! then takes its first length-dependent slice. See `read_pack`'s own body comments for exactly
//! where allocation (`.to_vec()`/`String`/JSON parse, all of which allocate) first happens --
//! strictly after every length/limit/truncation/trailing-byte check has passed.
//!
//! The foma payload's *content* is an opaque byte blob in foma's own existing binary-memory
//! encoding (`fsm_read_binary_mem`) -- this module never parses it, per the hard rule against
//! inventing a second network format. `pg_foma::analyzer::FomaProposer::foma_binary_payload`
//! (`pg-cli`'s `pack.rs` production caller) writes real bytes in exactly this encoding via
//! `foma::io::fsm_write_binary`; this module's own tests below exercise both that real encoding
//! (`round_trip_with_real_foma_binary_payload_not_just_synthetic_ascii`, gzip magic bytes and all)
//! and plain-ASCII synthetic fixtures, since this module's byte-handling correctness must not
//! depend on which kind of content either section happens to carry. The Rust-HermitCrab runtime
//! payload is likewise opaque bytes from this module's point of view, but unlike the foma payload
//! it has no real producer yet anywhere in this workspace (`pg_grammar::model::Grammar` is not
//! serde-serializable today -- see `crate`'s own top-level doc, "What this crate is not (yet)") --
//! its tests use only synthetic byte fixtures, honestly, because that is all that exists.

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::manifest::PackManifest;
use crate::signature::{self, SignatureState};

/// Fixed PanGloss magic bytes opening every `.pgpack` container.
pub const MAGIC: [u8; 8] = *b"PGLOPACK";
/// The container framing version this build writes and reads. Distinct from
/// `crate::manifest::MANIFEST_SCHEMA_VERSION` (the manifest's own shape) and from
/// `crate::compat::RequiredRuntimeFeatures::payload_format_version` (the runtime payload's own
/// format) -- three independently-versioned dimensions.
pub const CONTAINER_VERSION: u32 = 1;

const MAGIC_LEN: usize = 8;
const VERSION_LEN: usize = 4;
const LEN_FIELD_SIZE: usize = 8;
/// magic + version + three u64 length prefixes.
const HEADER_LEN: usize = MAGIC_LEN + VERSION_LEN + 3 * LEN_FIELD_SIZE;
const DIGEST_LEN: usize = 32;

/// Versioned per-section and total byte ceilings. These are deliberately conservative, provisional
/// container-level allocation ceilings for this additive step -- distinct from, and not derived
/// from, `pg_foma::health`'s FST-payload severity bands (which judge a *compiled FST's* health,
/// not this container's allocation safety) -- flagged as a judgment call for later calibration,
/// mirroring R6's own "final numerical calibration is a late gate" stance for its own budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionLimits {
    pub max_manifest_bytes: u64,
    pub max_runtime_payload_bytes: u64,
    pub max_foma_payload_bytes: u64,
    pub max_total_bytes: u64,
}

/// Container version 1's limits. `limits_for_version` is the only place a future container
/// version's limits would be added (new arm, never mutating this one -- versioned limits, not a
/// single global).
pub const V1_LIMITS: VersionLimits = VersionLimits {
    max_manifest_bytes: 16 * 1024 * 1024, // 16 MiB
    max_runtime_payload_bytes: 2_000_000_000,
    max_foma_payload_bytes: 2_000_000_000,
    // Deliberately less than `max_runtime_payload_bytes + max_foma_payload_bytes`: a package can
    // legally max out one section, but not both at once -- the total ceiling is its own
    // independent check, not merely the sum of per-section ceilings (see the total-limit test).
    max_total_bytes: 3_000_000_000,
};

/// Looks up the versioned limits for a container version. `None` for any version this build
/// doesn't understand -- callers turn that into `PgPackError::UnsupportedVersion`.
pub const fn limits_for_version(version: u32) -> Option<VersionLimits> {
    match version {
        1 => Some(V1_LIMITS),
        _ => None,
    }
}

/// Every typed failure `read_pack` (or, for writer-side validation, `write_pack`) can return.
/// Never a panic -- malformed/hostile input always reaches one of these variants.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PgPackError {
    #[error("container too short to contain a {expected}-byte {what} (only {available} byte(s) available)")]
    TooShort {
        what: &'static str,
        expected: usize,
        available: usize,
    },
    #[error("bad magic bytes: expected {expected:02x?}, found {found:02x?}")]
    BadMagic { expected: [u8; 8], found: [u8; 8] },
    #[error("unsupported container version {found}")]
    UnsupportedVersion { found: u32 },
    #[error("declared {what} length {declared} exceeds this container version's limit of {limit} byte(s)")]
    LengthExceedsLimit {
        what: &'static str,
        declared: u64,
        limit: u64,
    },
    #[error("declared total package length {declared} exceeds this container version's total limit of {limit} byte(s)")]
    TotalLengthExceedsLimit { declared: u64, limit: u64 },
    #[error("declared section lengths overflow container-size arithmetic")]
    LengthOverflow,
    #[error("truncated package: declared sections need {needed} total byte(s) but only {available} byte(s) are present")]
    Truncated { needed: u64, available: u64 },
    #[error("non-canonical package: {extra} trailing byte(s) after the digest")]
    TrailingBytes { extra: u64 },
    #[error("SHA-256 digest mismatch: package content does not match its recorded structural-integrity digest (tamper detected)")]
    DigestMismatch,
    #[error("package fingerprint mismatch: the runtime and foma payloads do not match the manifest's recorded package fingerprint (they may come from different grammars)")]
    FingerprintMismatch,
    #[error("invalid pack manifest JSON: {0}")]
    ManifestJson(String),
}

/// The anti-mix-across-grammars package fingerprint: one fingerprint binds both
/// payloads so they can't be mixed across grammars. Lowercase-hex SHA-256 over each payload's
/// own length prefix (u64 little-endian) followed by its bytes, runtime then foma -- so the
/// fingerprint pins each payload's exact length as well as its content, and is independent of
/// everything else in the manifest (identity, license, health, signature, ...), letting
/// `read_pack` recompute and compare it purely from the payload bytes it read.
pub fn fingerprint_hex(runtime_payload: &[u8], foma_payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((runtime_payload.len() as u64).to_le_bytes());
    hasher.update(runtime_payload);
    hasher.update((foma_payload.len() as u64).to_le_bytes());
    hasher.update(foma_payload);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn check_section_limit(what: &'static str, declared: u64, limit: u64) -> Result<(), PgPackError> {
    if declared > limit {
        return Err(PgPackError::LengthExceedsLimit {
            what,
            declared,
            limit,
        });
    }
    Ok(())
}

/// Writes a complete `.pgpack` container. `manifest.package_fingerprint` must already equal
/// `fingerprint_hex` of `runtime_payload`/`foma_payload` (typically set via that function before
/// constructing the manifest, and via `crate::signature::sign`-populated `manifest.signature` if
/// the pack is to be signed) -- this function validates that consistency defensively and returns
/// `PgPackError::FingerprintMismatch` rather than writing a self-inconsistent pack.
///
/// Performs the same versioned-limit validation `read_pack` performs on the way in, so a caller
/// can never accidentally produce a pack this build's own reader would refuse.
pub fn write_pack(
    manifest: &PackManifest,
    runtime_payload: &[u8],
    foma_payload: &[u8],
) -> Result<Vec<u8>, PgPackError> {
    let limits = limits_for_version(CONTAINER_VERSION)
        .expect("CONTAINER_VERSION must always have limits registered for itself");

    let expected_fingerprint = fingerprint_hex(runtime_payload, foma_payload);
    if manifest.package_fingerprint != expected_fingerprint {
        return Err(PgPackError::FingerprintMismatch);
    }

    let manifest_json = manifest.to_canonical_json();
    let manifest_bytes = manifest_json.as_bytes();

    let manifest_len = manifest_bytes.len() as u64;
    let runtime_len = runtime_payload.len() as u64;
    let foma_len = foma_payload.len() as u64;
    check_section_limit("manifest", manifest_len, limits.max_manifest_bytes)?;
    check_section_limit(
        "runtime payload",
        runtime_len,
        limits.max_runtime_payload_bytes,
    )?;
    check_section_limit("foma payload", foma_len, limits.max_foma_payload_bytes)?;

    let total = (HEADER_LEN as u64)
        .checked_add(manifest_len)
        .and_then(|t| t.checked_add(runtime_len))
        .and_then(|t| t.checked_add(foma_len))
        .and_then(|t| t.checked_add(DIGEST_LEN as u64))
        .ok_or(PgPackError::LengthOverflow)?;
    if total > limits.max_total_bytes {
        return Err(PgPackError::TotalLengthExceedsLimit {
            declared: total,
            limit: limits.max_total_bytes,
        });
    }

    let mut out = Vec::with_capacity(total as usize);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
    out.extend_from_slice(&manifest_len.to_le_bytes());
    out.extend_from_slice(&runtime_len.to_le_bytes());
    out.extend_from_slice(&foma_len.to_le_bytes());
    out.extend_from_slice(manifest_bytes);
    out.extend_from_slice(runtime_payload);
    out.extend_from_slice(foma_payload);

    let digest = Sha256::digest(&out);
    out.extend_from_slice(&digest);

    Ok(out)
}

/// The result of a successful `read_pack` call: the parsed manifest, both payloads as owned
/// bytes, and the derived signature state.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadPack {
    pub manifest: PackManifest,
    pub runtime_payload: Vec<u8>,
    pub foma_payload: Vec<u8>,
    /// See `crate::signature::SignatureState`'s own doc: reported for the caller's information
    /// only. **Never used by this function to decide whether to return `Ok`** -- an `Invalid`
    /// signature state is returned inside a successful `ReadPack`, exactly like `Valid` and
    /// `Unsigned` are: signature state NEVER controls analysis.
    pub signature_state: SignatureState,
}

/// Reads and fully validates a `.pgpack` container from `bytes`.
///
/// # Validation order (validate-before-allocate)
/// Every check up to and including the SHA-256 structural-integrity digest uses only fixed-offset
/// reads and (already-length-checked) slices of `bytes` -- **zero heap allocation** occurs before
/// that point. Only after magic, version, every declared length's versioned limit, total-package
/// limit, truncation, trailing bytes, and the digest have all passed does this function perform
/// its first allocation (copying the manifest/payload sections into owned buffers, and parsing the
/// manifest JSON). See this module's own doc for the exact byte layout these offsets index into.
///
/// # Signature never gates
/// A `crate::signature::SignatureState::Invalid` (or the manifest simply being unsigned) never
/// turns this into an `Err` -- see `ReadPack::signature_state`'s own doc.
pub fn read_pack(bytes: &[u8]) -> Result<ReadPack, PgPackError> {
    // ---- Fixed-size header reads only; no length-dependent slicing yet. ----
    if bytes.len() < HEADER_LEN {
        return Err(PgPackError::TooShort {
            what: "container header",
            expected: HEADER_LEN,
            available: bytes.len(),
        });
    }

    let magic: [u8; 8] = bytes[0..MAGIC_LEN].try_into().unwrap();
    if magic != MAGIC {
        return Err(PgPackError::BadMagic {
            expected: MAGIC,
            found: magic,
        });
    }

    let version = u32::from_le_bytes(
        bytes[MAGIC_LEN..MAGIC_LEN + VERSION_LEN]
            .try_into()
            .unwrap(),
    );
    let limits =
        limits_for_version(version).ok_or(PgPackError::UnsupportedVersion { found: version })?;

    let mut pos = MAGIC_LEN + VERSION_LEN;
    let read_len_field = |bytes: &[u8], pos: usize| -> u64 {
        u64::from_le_bytes(bytes[pos..pos + LEN_FIELD_SIZE].try_into().unwrap())
    };
    let manifest_len = read_len_field(bytes, pos);
    pos += LEN_FIELD_SIZE;
    let runtime_len = read_len_field(bytes, pos);
    pos += LEN_FIELD_SIZE;
    let foma_len = read_len_field(bytes, pos);
    pos += LEN_FIELD_SIZE;
    debug_assert_eq!(pos, HEADER_LEN);

    // ---- Every declared length validated against this version's limits, BEFORE any allocation. ----
    check_section_limit("manifest", manifest_len, limits.max_manifest_bytes)?;
    check_section_limit(
        "runtime payload",
        runtime_len,
        limits.max_runtime_payload_bytes,
    )?;
    check_section_limit("foma payload", foma_len, limits.max_foma_payload_bytes)?;

    let needed = (HEADER_LEN as u64)
        .checked_add(manifest_len)
        .and_then(|t| t.checked_add(runtime_len))
        .and_then(|t| t.checked_add(foma_len))
        .and_then(|t| t.checked_add(DIGEST_LEN as u64))
        .ok_or(PgPackError::LengthOverflow)?;
    if needed > limits.max_total_bytes {
        return Err(PgPackError::TotalLengthExceedsLimit {
            declared: needed,
            limit: limits.max_total_bytes,
        });
    }

    let available = bytes.len() as u64;
    if needed > available {
        return Err(PgPackError::Truncated { needed, available });
    }
    if needed < available {
        return Err(PgPackError::TrailingBytes {
            extra: available - needed,
        });
    }

    // `needed == available == bytes.len()`, and `needed` already proved `<= max_total_bytes`
    // (well under `usize::MAX` on every target this workspace builds for), so every offset below
    // is safe to convert to `usize` and slice with.
    let manifest_len = manifest_len as usize;
    let runtime_len = runtime_len as usize;
    let foma_len = foma_len as usize;

    let manifest_bytes = &bytes[pos..pos + manifest_len];
    pos += manifest_len;
    let runtime_bytes = &bytes[pos..pos + runtime_len];
    pos += runtime_len;
    let foma_bytes = &bytes[pos..pos + foma_len];
    pos += foma_len;
    let digest_bytes = &bytes[pos..pos + DIGEST_LEN];
    pos += DIGEST_LEN;
    debug_assert_eq!(pos, bytes.len());

    // ---- Structural-integrity digest, still zero-allocation (Sha256::digest over a borrowed slice). ----
    let computed_digest = Sha256::digest(&bytes[0..bytes.len() - DIGEST_LEN]);
    if computed_digest.as_slice() != digest_bytes {
        return Err(PgPackError::DigestMismatch);
    }

    // ---- First allocation: every length/limit/truncation/trailing/digest check has passed. ----
    let manifest_str = std::str::from_utf8(manifest_bytes)
        .map_err(|e| PgPackError::ManifestJson(format!("manifest is not valid UTF-8: {e}")))?;
    let manifest = PackManifest::from_json(manifest_str)
        .map_err(|e| PgPackError::ManifestJson(e.to_string()))?;

    let runtime_payload = runtime_bytes.to_vec();
    let foma_payload = foma_bytes.to_vec();

    // Anti-mix-across-grammars check: independent of the whole-file digest above.
    if manifest.package_fingerprint != fingerprint_hex(&runtime_payload, &foma_payload) {
        return Err(PgPackError::FingerprintMismatch);
    }

    // Signature state: reported, never gating (see `ReadPack::signature_state`'s doc).
    let signature_state = match &manifest.signature {
        None => SignatureState::Unsigned,
        Some(block) => {
            let manifest_no_sig_json = manifest.without_signature().to_canonical_json();
            let message = signature::domain_separated_signed_bytes(
                version,
                manifest_no_sig_json.as_bytes(),
                &runtime_payload,
                &foma_payload,
            );
            if signature::verify(block, &message) {
                SignatureState::Valid
            } else {
                SignatureState::Invalid
            }
        }
    };

    Ok(ReadPack {
        manifest,
        runtime_payload,
        foma_payload,
        signature_state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::RequiredRuntimeFeatures;
    use crate::trust::CapabilityTrust;
    use pg_foma::health::HealthReport;

    fn synthetic_manifest_for(runtime_payload: &[u8], foma_payload: &[u8]) -> PackManifest {
        PackManifest {
            format: crate::manifest::MANIFEST_FORMAT_TAG.to_string(),
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
            capability_trust: CapabilityTrust::Proven,
            fst_health: HealthReport::new(vec![]),
            license: None,
            created_by: "synthetic-test-builder".to_string(),
            created_at: "2026-07-24T00:00:00Z".to_string(),
            signature: None,
        }
    }

    const SYNTHETIC_RUNTIME_PAYLOAD: &[u8] = b"synthetic-rust-hermitcrab-runtime-payload-bytes";
    const SYNTHETIC_FOMA_PAYLOAD: &[u8] = b"synthetic-opaque-foma-binary-memory-payload-bytes";

    // ---------------------------------------------------------------------------------------
    // Round trip.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn round_trip_write_then_read_is_identical() {
        let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
        let bytes =
            write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
        let read = read_pack(&bytes).unwrap();
        assert_eq!(read.manifest, manifest);
        assert_eq!(read.runtime_payload, SYNTHETIC_RUNTIME_PAYLOAD);
        assert_eq!(read.foma_payload, SYNTHETIC_FOMA_PAYLOAD);
        assert_eq!(read.signature_state, SignatureState::Unsigned);
    }

    // ---------------------------------------------------------------------------------------
    // Real foma binary-memory bytes (not just the plain-ASCII synthetic fixtures above).
    // ---------------------------------------------------------------------------------------

    /// A tiny, deterministic, real compiled foma network (`LEXICON Root\ncat # ;\ndog # ;\n` --
    /// the same minimal syntax `foma`'s own `lexcread.rs` test suite uses), built independently of
    /// the whole HermitCrab grammar/emit pipeline via `foma::lexcread::fsm_lexc_parse_string` --
    /// the exact same compiler entry point `pg_foma::analyzer::FomaProposer` calls in production.
    const REAL_LEXC_SOURCE: &str = "LEXICON Root\ncat # ;\ndog # ;\n";

    fn compile_real_network() -> foma::types::Fsm {
        let opts = foma::options::FomaOptions::default();
        foma::lexcread::fsm_lexc_parse_string(&opts, None, REAL_LEXC_SOURCE)
            .expect("minimal lexc source must compile")
    }

    /// A REAL, gzip-compressed foma binary-memory payload -- `foma::io::fsm_write_binary`, the
    /// SAME function `crate::compat`'s production caller (`pg_foma::analyzer::FomaProposer::
    /// foma_binary_payload`) uses -- as opposed to the plain-ASCII `SYNTHETIC_FOMA_PAYLOAD` string
    /// literal every other test in this module uses. This crate's container format must handle
    /// genuine binary content (gzip magic bytes at the front, non-UTF8 bytes throughout, embedded
    /// NUL bytes) exactly as well as it handles a human-readable ASCII fixture.
    fn real_foma_payload_bytes() -> Vec<u8> {
        let net = compile_real_network();
        let mut bytes = Vec::new();
        foma::io::fsm_write_binary(&net, &mut bytes).expect("fsm_write_binary must succeed");
        bytes
    }

    #[test]
    fn round_trip_with_real_foma_binary_payload_not_just_synthetic_ascii() {
        let real_foma = real_foma_payload_bytes();
        // Sanity: this really is gzip-compressed binary content (magic bytes 0x1f 0x8b), not a
        // disguised ASCII string -- proves this test exercises materially different bytes than
        // `SYNTHETIC_FOMA_PAYLOAD` above.
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

        // Reconstruct the network from the PACKED bytes (never re-deriving it from `REAL_LEXC_SOURCE`
        // directly) and confirm it is a genuinely equivalent, applyable network: same state/arc
        // counts as an independent fresh compile, and `apply_up` agreement on every word in the
        // tiny lexicon above.
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
        let mut manifest =
            synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
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

        let bytes =
            write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
        let read = read_pack(&bytes).unwrap();
        assert_eq!(read.signature_state, SignatureState::Valid);
        assert_eq!(read.manifest, manifest);
    }

    #[test]
    fn round_trip_with_overridden_capability_trust() {
        let mut manifest =
            synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
        manifest.capability_trust =
            CapabilityTrust::Overridden(crate::trust::CapabilityOverrideRecord {
                authorized_by: "synthetic-test-operator".to_string(),
                reason: "synthetic field trial".to_string(),
                recorded_at: "2026-07-24T00:00:00Z".to_string(),
                overridden_configs: vec![crate::trust::OverriddenConfig {
                    predicate: "synthetic.simultaneous.subrule-overlap".to_string(),
                    construct: "mrule:synthetic-0001".to_string(),
                    witness: "synthetic-witness".to_string(),
                }],
            });
        // Fingerprint is independent of capability_trust, so it's still valid unchanged.
        let bytes =
            write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
        let read = read_pack(&bytes).unwrap();
        assert!(read.manifest.capability_trust.is_unproven());
        assert_eq!(read.manifest, manifest);
    }

    // ---------------------------------------------------------------------------------------
    // Bad magic / bad version.
    // ---------------------------------------------------------------------------------------

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

    // ---------------------------------------------------------------------------------------
    // Length exceeding the versioned limit, checked BEFORE allocation.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn rejects_manifest_length_exceeding_versioned_limit_before_allocating() {
        let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
        let mut bytes =
            write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
        // Overwrite the declared manifest length with something far beyond V1_LIMITS while
        // leaving the actual buffer short -- if this function allocated based on the declared
        // length before validating it, this would attempt a huge allocation/panic on the
        // out-of-bounds slice instead of returning a clean typed error.
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
        // Each individual declared length stays within its own per-section limit, but their sum
        // exceeds the total-package limit.
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

    // ---------------------------------------------------------------------------------------
    // Truncated payload.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn rejects_truncated_payload() {
        let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
        let bytes =
            write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
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

    // ---------------------------------------------------------------------------------------
    // Tamper: SHA-256 digest mismatch.
    // ---------------------------------------------------------------------------------------

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

    // ---------------------------------------------------------------------------------------
    // Fingerprint mismatch: payloads swapped across "grammars" while the manifest is untouched.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn rejects_mismatched_fingerprint_when_payloads_are_swapped() {
        let manifest_a = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
        // Build a container whose manifest fingerprint matches payload A, but physically carries
        // payload B's foma bytes instead -- simulating payloads mixed across grammars. Constructed
        // directly (bypassing `write_pack`'s own fingerprint check) to prove `read_pack` itself
        // catches this independent of the writer.
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

        // The whole-file digest is internally consistent (freshly recomputed over the swapped
        // content), so only the fingerprint check catches the mismatch.
        let err = read_pack(&out).unwrap_err();
        assert_eq!(err, PgPackError::FingerprintMismatch);
    }

    #[test]
    fn write_pack_rejects_caller_supplied_manifest_with_wrong_fingerprint() {
        let mut manifest =
            synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
        manifest.package_fingerprint = "0".repeat(64);
        let err =
            write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap_err();
        assert_eq!(err, PgPackError::FingerprintMismatch);
    }

    // ---------------------------------------------------------------------------------------
    // Signature state: unsigned / valid / invalid, and invalid never blocks the read.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn unsigned_pack_reports_unsigned_and_reads_successfully() {
        let manifest = synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
        let bytes =
            write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
        let read = read_pack(&bytes).unwrap();
        assert_eq!(read.signature_state, SignatureState::Unsigned);
    }

    #[test]
    fn invalidly_signed_pack_still_reads_successfully_and_reports_invalid() {
        let mut manifest =
            synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
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

        let bytes =
            write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
        // Must NOT be an Err: an invalid signature never blocks reading/analysis.
        let read = read_pack(&bytes).expect("an invalid signature must not block reading");
        assert_eq!(read.signature_state, SignatureState::Invalid);
        assert_eq!(read.runtime_payload, SYNTHETIC_RUNTIME_PAYLOAD);
        assert_eq!(read.foma_payload, SYNTHETIC_FOMA_PAYLOAD);
    }

    #[test]
    fn validly_signed_pack_reads_successfully_and_reports_valid() {
        let mut manifest =
            synthetic_manifest_for(SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD);
        let manifest_no_sig_json = manifest.to_canonical_json();
        let message = signature::domain_separated_signed_bytes(
            CONTAINER_VERSION,
            manifest_no_sig_json.as_bytes(),
            SYNTHETIC_RUNTIME_PAYLOAD,
            SYNTHETIC_FOMA_PAYLOAD,
        );
        let seed = [11u8; 32];
        manifest.signature = Some(signature::sign(&seed, &message, None));

        let bytes =
            write_pack(&manifest, SYNTHETIC_RUNTIME_PAYLOAD, SYNTHETIC_FOMA_PAYLOAD).unwrap();
        let read = read_pack(&bytes).expect("a validly signed pack must read successfully");
        assert_eq!(read.signature_state, SignatureState::Valid);
    }
}
