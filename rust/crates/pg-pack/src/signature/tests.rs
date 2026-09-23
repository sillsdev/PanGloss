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
    let message = domain_separated_signed_bytes(1, b"{}", b"synthetic-runtime", b"synthetic-foma");
    let block = sign(
        &SYNTHETIC_SEED_A,
        &message,
        Some("synthetic-key-1".to_string()),
    );
    assert!(verify(&block, &message));
}

#[test]
fn verify_fails_with_wrong_key() {
    let message = domain_separated_signed_bytes(1, b"{}", b"synthetic-runtime", b"synthetic-foma");
    let mut block = sign(&SYNTHETIC_SEED_A, &message, None);
    let wrong_key_block = sign(&SYNTHETIC_SEED_B, &message, None);
    block.public_key_hex = wrong_key_block.public_key_hex;
    assert!(!verify(&block, &message));
}

#[test]
fn verify_fails_when_message_changes_after_signing() {
    let message = domain_separated_signed_bytes(1, b"{}", b"synthetic-runtime", b"synthetic-foma");
    let block = sign(&SYNTHETIC_SEED_A, &message, None);
    let tampered_message =
        domain_separated_signed_bytes(1, b"{}", b"synthetic-runtime-TAMPERED", b"synthetic-foma");
    assert!(!verify(&block, &tampered_message));
}

#[test]
fn verify_fails_on_unknown_algorithm() {
    let message = domain_separated_signed_bytes(1, b"{}", b"synthetic-runtime", b"synthetic-foma");
    let mut block = sign(&SYNTHETIC_SEED_A, &message, None);
    block.algorithm = "synthetic-unknown-algorithm".to_string();
    assert!(!verify(&block, &message));
}

#[test]
fn verify_fails_on_malformed_hex_without_panicking() {
    let message = domain_separated_signed_bytes(1, b"{}", b"synthetic-runtime", b"synthetic-foma");
    let mut block = sign(&SYNTHETIC_SEED_A, &message, None);
    block.signature_hex = "not-hex-at-all!!".to_string();
    assert!(!verify(&block, &message));
    block.public_key_hex = "zz".to_string();
    assert!(!verify(&block, &message));
}
