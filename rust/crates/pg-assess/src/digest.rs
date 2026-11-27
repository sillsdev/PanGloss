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

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{BufWriter, Write};

use crate::identity::AnalysisIdentity;
use crate::jcs::{self, CanonicalSink, JcsError};

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
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// Digest `value` under `projection`, binding the projection name into the preimage.
pub fn digest_projection(projection: &str, value: &Value) -> Result<String, JcsError> {
    digest_with_projection(projection, |out| jcs::write_value(value, out))
}

pub(crate) fn digest_with_projection(
    projection: &str,
    write_value: impl FnOnce(&mut ProjectionWriter) -> Result<(), JcsError>,
) -> Result<String, JcsError> {
    let mut out = ProjectionWriter::default();
    out.push_str("{\"projection\":");
    jcs::write_string(projection, &mut out);
    out.push_str(",\"value\":");
    write_value(&mut out)?;
    out.push('}');
    let hash = out.0.into_inner().expect("SHA-256 writes are infallible");
    Ok(format!("sha256:{:x}", hash.finalize()))
}

pub(crate) struct ProjectionWriter(BufWriter<Sha256>);

impl Default for ProjectionWriter {
    fn default() -> Self {
        Self(BufWriter::new(Sha256::new()))
    }
}

impl CanonicalSink for ProjectionWriter {
    fn push_str(&mut self, text: &str) {
        self.0
            .write_all(text.as_bytes())
            .expect("SHA-256 writes are infallible");
    }
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
mod tests;
