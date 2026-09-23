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
//! inventing a second network format; this module's own tests below exercise that encoding
//! (`round_trip_with_real_foma_binary_payload_not_just_synthetic_ascii`, gzip magic bytes and all)
//! and plain-ASCII synthetic fixtures, since this module's byte-handling correctness must not
//! depend on which kind of content either section happens to carry. The Rust-HermitCrab runtime
//! payload is likewise opaque bytes from this module's point of view; its tests use synthetic byte
//! fixtures for that section.

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
/// from, `pg_health::health`'s FST-payload severity bands (which judge a *compiled FST's* health,
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
    // Deliberately less than `max_runtime_payload_bytes + max_foma_payload_bytes`: a package can legally max out one section, but not both at once.
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
    #[error("unsupported pack manifest schema version {found}")]
    UnsupportedManifestSchema { found: u32 },
    #[error("unsupported embedded FST-health schema version {found}")]
    UnsupportedHealthSchema { found: u32 },
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

fn validate_manifest_schema(manifest: &PackManifest) -> Result<(), PgPackError> {
    if manifest.manifest_schema_version != crate::manifest::MANIFEST_SCHEMA_VERSION {
        return Err(PgPackError::UnsupportedManifestSchema {
            found: manifest.manifest_schema_version,
        });
    }
    Ok(())
}

fn validate_health_schema(manifest: &PackManifest) -> Result<(), PgPackError> {
    if manifest.fst_health.schema_version != pg_health::health::HEALTH_SCHEMA_VERSION {
        return Err(PgPackError::UnsupportedHealthSchema {
            found: manifest.fst_health.schema_version,
        });
    }
    Ok(())
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

    validate_manifest_schema(manifest)?;
    validate_health_schema(manifest)?;
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

    // `needed == available == bytes.len()` and already proved `<= max_total_bytes`, so every offset below is safe to convert to `usize` and slice with.
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
    validate_manifest_schema(&manifest)?;
    validate_health_schema(&manifest)?;

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
mod tests;
