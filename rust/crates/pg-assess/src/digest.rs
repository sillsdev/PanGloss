//! Digests over named, independently versioned projections.
//!
//! Each digest canonicalizes an artifact and drops what is irrelevant *to its own question*; the
//! drop-list is the question (`add-grammar-assessment` design D3):
//!
//! - `reportId` drops nothing — "are these the same bytes?"
//! - `SEMANTIC_PROJECTION` drops timestamps, paths, timings, and `sourceSha256` — "was this the
//!   same run?"
//! - `OUTCOME_PROJECTION` additionally drops tool versions, budgets, pipeline, and duplicate
//!   counts — "did the grammar behave the same?"
//!
//! The projection name is part of every digest's preimage, so changing what a projection drops
//! cannot silently change what its digest means: a v2 projection produces visibly different values
//! rather than colliding with v1's.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::identity::AnalysisIdentity;
use crate::jcs::{self, JcsError};

/// "Was this the same run?" — includes `modelFingerprint`, pipeline, effective budgets, importer
/// and compiler versions, outcomes, analyses, and duplicate counts. Excludes `sourceSha256`
/// (design D3a): with `core.autocrlf` in play the same grammar has different bytes on Windows and
/// Linux, and run identity is carried by what was analyzed, not by the file on disk.
pub const SEMANTIC_PROJECTION: &str = "pangloss.assessment-semantic/v1";

/// "Did the grammar behave the same?" — suite digest, per-case outcome kind, and deduplicated
/// identity sets only. Survives a compiler upgrade that changes no analysis.
pub const OUTCOME_PROJECTION: &str = "pangloss.assessment-outcome/v1";

/// Hex SHA-256 of raw bytes, prefixed. Used for `sourceSha256` over **exact file bytes** — never
/// `pg_lexicon::grammar_source_fingerprint`, which normalizes CRLF before hashing and would make a
/// Windows-authored source and its Linux checkout hash alike.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::from("sha256:");
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Digest `value` under `projection`, binding the projection name into the preimage.
pub fn digest_projection(projection: &str, value: &Value) -> Result<String, JcsError> {
    let preimage = json!({ "projection": projection, "value": value });
    Ok(sha256_bytes(jcs::canonicalize(&preimage)?.as_bytes()))
}

/// The canonical digest of one analysis identity, used for indexing, CLI selection, and as the
/// sort key of a canonical analysis set. It is an index, never a substitute for the structured
/// value: equality is confirmed against the full identity, and two unequal identities sharing a
/// digest are an integrity error rather than a match.
pub fn identity_digest(identity: &AnalysisIdentity) -> String {
    // Cannot fail: an identity contains only strings, integers, and nulls.
    digest_projection(
        crate::identity::IDENTITY_PROFILE,
        &identity.to_canonical_value(),
    )
    .expect("analysis identities contain no floating-point values")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(morphemes: &[&str], root_index: i32, category: Option<&str>) -> AnalysisIdentity {
        AnalysisIdentity {
            morphemes: morphemes.iter().map(|m| Some(m.to_string())).collect(),
            root_index,
            category: category.map(str::to_string),
        }
    }

    #[test]
    fn identity_digest_is_stable_and_prefixed() {
        let d = identity_digest(&id(&["a", "b"], 0, Some("noun")));
        assert!(d.starts_with("sha256:"), "{d}");
        assert_eq!(d.len(), "sha256:".len() + 64);
        assert_eq!(d, identity_digest(&id(&["a", "b"], 0, Some("noun"))));
    }

    #[test]
    fn every_identity_field_changes_the_digest() {
        let base = identity_digest(&id(&["a", "b"], 0, Some("noun")));
        assert_ne!(
            base,
            identity_digest(&id(&["a", "c"], 0, Some("noun"))),
            "morpheme"
        );
        assert_ne!(
            base,
            identity_digest(&id(&["b", "a"], 0, Some("noun"))),
            "order"
        );
        assert_ne!(
            base,
            identity_digest(&id(&["a", "b"], 1, Some("noun"))),
            "root index"
        );
        assert_ne!(
            base,
            identity_digest(&id(&["a", "b"], 0, Some("verb"))),
            "category"
        );
        assert_ne!(
            base,
            identity_digest(&id(&["a", "b"], 0, None)),
            "no category"
        );
    }

    #[test]
    fn a_guessed_slot_differs_from_a_morpheme_literally_named_null() {
        let guessed = AnalysisIdentity {
            morphemes: vec![None],
            root_index: 0,
            category: None,
        };
        assert_ne!(
            identity_digest(&guessed),
            identity_digest(&id(&["null"], 0, None))
        );
    }

    #[test]
    fn projections_are_namespaced_so_the_same_value_digests_differently() {
        // The whole point of binding the projection name into the preimage: two different projections over identical bytes must never collide.
        let v = json!({ "cases": [] });
        assert_ne!(
            digest_projection(SEMANTIC_PROJECTION, &v).unwrap(),
            digest_projection(OUTCOME_PROJECTION, &v).unwrap()
        );
    }

    #[test]
    fn projection_digests_ignore_key_order() {
        let a = serde_json::from_str::<Value>(r#"{"b":1,"a":2}"#).unwrap();
        let b = serde_json::from_str::<Value>(r#"{"a":2,"b":1}"#).unwrap();
        assert_eq!(
            digest_projection(SEMANTIC_PROJECTION, &a).unwrap(),
            digest_projection(SEMANTIC_PROJECTION, &b).unwrap()
        );
    }

    #[test]
    fn sha256_bytes_is_exact_and_does_not_normalize_line_endings() {
        // The whole reason `sourceSha256` cannot reuse `pg_lexicon::grammar_source_fingerprint`.
        assert_ne!(sha256_bytes(b"a\r\nb"), sha256_bytes(b"a\nb"));
    }
}
