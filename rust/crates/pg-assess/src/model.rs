//! `sourceSha256` and `modelFingerprint` — two deliberately different questions about the same
//! grammar.
//!
//! `sourceSha256` identifies **the file**: exact bytes, no normalization. `modelFingerprint`
//! identifies **what was analyzed**: the source's canonical content together with the compiler that
//! turned it into a model. §17.9 requires both, and requires that formatting-only differences may
//! move the first without moving the second — line endings being the everyday example, since
//! `core.autocrlf` hands Windows and Linux checkouts of one committed grammar different bytes.
//!
//! ## Why the fingerprint is derived from the source rather than from `Grammar`
//!
//! The obvious implementation is to walk the compiled `pg_grammar_model::model::Grammar` and hash every
//! analysis-relevant field. It is also the one that fails quietly: `Grammar` has eighteen fields,
//! several of them deep, and a fingerprint that forgets one is a fingerprint that says "nothing
//! changed" when something did. Because `semanticDigest` rests entirely on this value (design D3a),
//! that failure would be invisible and load-bearing at the same time.
//!
//! Hashing the canonical source instead is complete by construction. Compilation is deterministic,
//! so the canonical source plus the compiler version determines the model exactly, and no field can
//! be forgotten because no field is enumerated.
//!
//! ## What "canonical" covers, and what it does not
//!
//! `SourceKind::Snapshot` canonicalizes through JCS, so key order, whitespace, and line endings
//! are all absorbed. `SourceKind::HcXml` normalizes line endings only: attribute order and
//! inter-element whitespace still move the fingerprint. That is a conservative failure — it reports
//! a difference where none exists semantically, rather than hiding one — and closing it needs real
//! XML canonicalization, which is deferred rather than faked.

use serde_json::{json, Value};

use crate::digest::{digest_projection, sha256_bytes};
use crate::jcs::JcsError;

/// The projection that binds a model fingerprint to its meaning.
pub const MODEL_PROJECTION: &str = "pangloss.model/v1";

/// Which of the accepted grammar sources produced a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// HermitCrab XML.
    HcXml,
    /// `pg-snapshot` JSON.
    Snapshot,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::HcXml => "hc-xml",
            SourceKind::Snapshot => "snapshot",
        }
    }
}

/// Hash of the exact source bytes, with no normalization whatsoever.
///
/// Deliberately not `pg_lexicon::grammar_source_fingerprint`, which collapses CRLF to LF before
/// hashing and would therefore report a Windows-authored grammar and its Linux checkout as the same
/// file. Here they are different files, which is true, and `modelFingerprint` is what says they
/// nevertheless describe the same model.
pub fn source_sha256(bytes: &[u8]) -> String {
    sha256_bytes(bytes)
}

/// Fingerprint of the model a source compiles to under a given compiler.
pub fn model_fingerprint(
    kind: SourceKind,
    source: &str,
    compiler_version: &str,
) -> Result<String, JcsError> {
    let canonical = canonical_source(kind, source)?;
    digest_projection(
        MODEL_PROJECTION,
        &json!({
            "sourceKind": kind.as_str(),
            "canonicalSource": canonical,
            "compilerVersion": compiler_version,
        }),
    )
}

/// The source reduced to the content that determines the compiled model.
fn canonical_source(kind: SourceKind, source: &str) -> Result<String, JcsError> {
    match kind {
        SourceKind::Snapshot => match serde_json::from_str::<Value>(source) {
            // A snapshot is JSON, so JCS absorbs key order and every kind of whitespace.
            Ok(value) => crate::jcs::canonicalize(&value),
            // Unparsable input is not this function's problem to diagnose; fall back to line-ending normalization so a fingerprint still exists for the failure artifact.
            Err(_) => Ok(normalize_line_endings(source)),
        },
        SourceKind::HcXml => Ok(normalize_line_endings(source)),
    }
}

fn normalize_line_endings(source: &str) -> String {
    source.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests;
