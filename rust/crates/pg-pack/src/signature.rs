//! Optional Ed25519 publisher signature (R2A: "optional Ed25519 publisher signature... signature
//! state is reported `unsigned`/`valid`/`invalid` and NEVER controls analysis"; `make-wasm-
//! analysis-only` spec.md "Publisher signatures are optional offline provenance": "verification
//! SHALL require no secret, entitlement, account, or network service").
//!
//! **What is signed.** [`domain_separated_signed_bytes`] builds the exact byte sequence design.md
//! specifies: "a domain-separated canonical representation of the container version, pack manifest
//! excluding its signature value, and both framed payloads." [`crate::format::write_pack`]/
//! [`crate::format::read_pack`] are the only callers that assemble those bytes for real container
//! framing; this module owns the domain-separation tag and the byte-assembly function so signing
//! and verification can never drift apart on what bytes actually get hashed/signed.
//!
//! **No `rand`/`rand_core` dependency.** This crate never generates a "real" production signing
//! key itself — `make-wasm-analysis-only` design.md is explicit that "signing tooling accepts the
//! private key outside the package," i.e. key generation is an external concern. Every test key in
//! this crate is therefore built from a fixed synthetic 32-byte seed via
//! [`ed25519_dalek::SigningKey::from_bytes`] (deterministic, no CSPRNG needed), keeping the
//! dependency footprint to `ed25519-dalek` alone.
//!
//! **Never gates analysis.** [`SignatureState`] is a pure report computed by
//! [`crate::format::read_pack`] after every other structural check has already passed; a `read`
//! call always returns the parsed manifest and both payloads regardless of which
//! [`SignatureState`] it reports (see that function's own doc and this crate's `tamper` tests).

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Domain-separation tag mixed into every signed byte sequence (spec.md: "domain-separated
/// canonical representation"). Changing this tag is a wire-breaking change to every existing
/// signature; it is not itself versioned because [`crate::format::CONTAINER_VERSION`] is already
/// part of the signed bytes and any future incompatible signing scheme is a new container version.
const SIGNATURE_DOMAIN_TAG: &[u8] = b"pangloss.pgpack.signature.v1";

/// The only signature algorithm this schema step defines. Kept as an explicit field (rather than
/// assumed) so a future algorithm can be added without an incompatible schema change — an unknown
/// value simply cannot verify (see [`SignatureBlock::algorithm`]'s doc).
pub const ALGORITHM_ED25519: &str = "ed25519";

/// An optional publisher signature attached to a pack manifest.
/// [`crate::manifest::PackManifest::signature`] is `None` for an unsigned pack; `Some` here does
/// not by itself mean the signature verifies — see [`SignatureState`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignatureBlock {
    /// Which algorithm produced `signature_hex`. Only [`ALGORITHM_ED25519`] is understood by this
    /// build; any other value reports [`SignatureState::Invalid`] rather than panicking (see
    /// [`verify`]).
    pub algorithm: String,
    /// Lowercase-hex-encoded 32-byte Ed25519 public key.
    pub public_key_hex: String,
    /// Lowercase-hex-encoded 64-byte Ed25519 signature.
    pub signature_hex: String,
    /// Optional human/tool-assigned key identifier, for a publisher rotating keys over time.
    /// Informational only; verification uses `public_key_hex`, never this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
}

/// The loader's tri-state signature report (R2A: "`unsigned`, `valid`, or `invalid`... NEVER
/// controls analysis"). Never itself serialized into a manifest — it is always *derived* fresh by
/// [`crate::format::read_pack`], never trusted from the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureState {
    /// No [`SignatureBlock`] present at all.
    Unsigned,
    /// A [`SignatureBlock`] was present and its signature verified against its declared public
    /// key over the exact domain-separated signed bytes.
    Valid,
    /// A [`SignatureBlock`] was present but did not verify (wrong key, tampered content after
    /// signing, malformed hex, unknown algorithm, or malformed key/signature bytes). Reported, not
    /// fatal — see this module's doc "Never gates analysis."
    Invalid,
}

/// Errors from encoding/decoding this module's hex fields. Never panics on malformed input.
#[derive(Debug, thiserror::Error)]
pub enum HexError {
    #[error("odd-length hex string (length {0})")]
    OddLength(usize),
    #[error("invalid hex digit {0:?}")]
    InvalidDigit(char),
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn decode_hex(s: &str) -> Result<Vec<u8>, HexError> {
    if !s.len().is_multiple_of(2) {
        return Err(HexError::OddLength(s.len()));
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let chars: Vec<char> = s.chars().collect();
    for pair in chars.chunks(2) {
        let hi = pair[0]
            .to_digit(16)
            .ok_or(HexError::InvalidDigit(pair[0]))?;
        let lo = pair[1]
            .to_digit(16)
            .ok_or(HexError::InvalidDigit(pair[1]))?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

/// Builds the exact domain-separated byte sequence that is signed/verified: the fixed domain tag,
/// the container version (little-endian `u32`, matching [`crate::format`]'s own byte order), the
/// pack manifest's canonical JSON bytes **with its `signature` field absent/`None`** (the manifest
/// this crate signs must never include the signature it is itself producing), and both payloads
/// framed exactly as [`crate::format::write_pack`] frames them on the wire (length-prefix then
/// content, so a signature also pins each payload's declared length, not only its bytes).
pub fn domain_separated_signed_bytes(
    container_version: u32,
    manifest_json_without_signature: &[u8],
    runtime_payload: &[u8],
    foma_payload: &[u8],
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(
        SIGNATURE_DOMAIN_TAG.len()
            + 4
            + 8
            + manifest_json_without_signature.len()
            + 8
            + runtime_payload.len()
            + 8
            + foma_payload.len(),
    );
    buf.extend_from_slice(SIGNATURE_DOMAIN_TAG);
    buf.extend_from_slice(&container_version.to_le_bytes());
    buf.extend_from_slice(&(manifest_json_without_signature.len() as u64).to_le_bytes());
    buf.extend_from_slice(manifest_json_without_signature);
    buf.extend_from_slice(&(runtime_payload.len() as u64).to_le_bytes());
    buf.extend_from_slice(runtime_payload);
    buf.extend_from_slice(&(foma_payload.len() as u64).to_le_bytes());
    buf.extend_from_slice(foma_payload);
    buf
}

/// Signs `message` (normally the output of [`domain_separated_signed_bytes`]) with an Ed25519
/// signing key, producing a [`SignatureBlock`]. `seed` is the 32-byte Ed25519 secret scalar seed;
/// see this module's doc for why callers (including this crate's own tests) build it from a fixed
/// synthetic byte array rather than a CSPRNG.
pub fn sign(seed: &[u8; 32], message: &[u8], key_id: Option<String>) -> SignatureBlock {
    let signing_key = SigningKey::from_bytes(seed);
    let verifying_key: VerifyingKey = signing_key.verifying_key();
    let sig = signing_key.sign(message);
    SignatureBlock {
        algorithm: ALGORITHM_ED25519.to_string(),
        public_key_hex: encode_hex(verifying_key.as_bytes()),
        signature_hex: encode_hex(&sig.to_bytes()),
        key_id,
    }
}

/// Verifies `block` against `message`. Returns `false` (never panics/errors) for any malformed
/// hex, wrong-length key/signature bytes, unknown algorithm, or a cryptographically invalid
/// signature — every failure mode collapses to [`SignatureState::Invalid`] at the call site in
/// [`crate::format::read_pack`].
pub fn verify(block: &SignatureBlock, message: &[u8]) -> bool {
    if block.algorithm != ALGORITHM_ED25519 {
        return false;
    }
    let Ok(key_bytes) = decode_hex(&block.public_key_hex) else {
        return false;
    };
    let Ok(key_bytes): Result<[u8; 32], _> = key_bytes.try_into() else {
        return false;
    };
    let Ok(sig_bytes) = decode_hex(&block.signature_hex) else {
        return false;
    };
    let Ok(sig_bytes): Result<[u8; 64], _> = sig_bytes.try_into() else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&key_bytes) else {
        return false;
    };
    let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);
    verifying_key.verify(message, &signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixed synthetic seed — not a real key, never used outside this crate's own tests.
    const SYNTHETIC_SEED_A: [u8; 32] = [7u8; 32];
    const SYNTHETIC_SEED_B: [u8; 32] = [9u8; 32];

    #[test]
    fn hex_round_trips() {
        let bytes = [0u8, 1, 2, 254, 255, 16];
        let hex = encode_hex(&bytes);
        assert_eq!(decode_hex(&hex).unwrap(), bytes);
    }

    #[test]
    fn decode_hex_rejects_odd_length() {
        assert!(matches!(decode_hex("abc"), Err(HexError::OddLength(3))));
    }

    #[test]
    fn decode_hex_rejects_bad_digit() {
        assert!(matches!(decode_hex("zz"), Err(HexError::InvalidDigit(_))));
    }

    #[test]
    fn sign_then_verify_succeeds() {
        let message =
            domain_separated_signed_bytes(1, b"{}", b"synthetic-runtime", b"synthetic-foma");
        let block = sign(
            &SYNTHETIC_SEED_A,
            &message,
            Some("synthetic-key-1".to_string()),
        );
        assert!(verify(&block, &message));
    }

    #[test]
    fn verify_fails_with_wrong_key() {
        let message =
            domain_separated_signed_bytes(1, b"{}", b"synthetic-runtime", b"synthetic-foma");
        let mut block = sign(&SYNTHETIC_SEED_A, &message, None);
        let wrong_key_block = sign(&SYNTHETIC_SEED_B, &message, None);
        block.public_key_hex = wrong_key_block.public_key_hex;
        assert!(!verify(&block, &message));
    }

    #[test]
    fn verify_fails_when_message_changes_after_signing() {
        let message =
            domain_separated_signed_bytes(1, b"{}", b"synthetic-runtime", b"synthetic-foma");
        let block = sign(&SYNTHETIC_SEED_A, &message, None);
        let tampered_message = domain_separated_signed_bytes(
            1,
            b"{}",
            b"synthetic-runtime-TAMPERED",
            b"synthetic-foma",
        );
        assert!(!verify(&block, &tampered_message));
    }

    #[test]
    fn verify_fails_on_unknown_algorithm() {
        let message =
            domain_separated_signed_bytes(1, b"{}", b"synthetic-runtime", b"synthetic-foma");
        let mut block = sign(&SYNTHETIC_SEED_A, &message, None);
        block.algorithm = "synthetic-unknown-algorithm".to_string();
        assert!(!verify(&block, &message));
    }

    #[test]
    fn verify_fails_on_malformed_hex_without_panicking() {
        let message =
            domain_separated_signed_bytes(1, b"{}", b"synthetic-runtime", b"synthetic-foma");
        let mut block = sign(&SYNTHETIC_SEED_A, &message, None);
        block.signature_hex = "not-hex-at-all!!".to_string();
        assert!(!verify(&block, &message));
        block.public_key_hex = "zz".to_string();
        assert!(!verify(&block, &message));
    }
}
