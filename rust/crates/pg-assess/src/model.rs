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
//! The obvious implementation is to walk the compiled [`pg_grammar::model::Grammar`] and hash every
//! analysis-relevant field. It is also the one that fails quietly: `Grammar` has eighteen fields,
//! several of them deep, and a fingerprint that forgets one is a fingerprint that says "nothing
//! changed" when something did. Because `semanticDigest` rests entirely on this value (design D3a),
//! that failure would be invisible and load-bearing at the same time.
//!
//! Reusing `.pgpack`'s `package_fingerprint` is not available either: `pg-cli`'s pack path still
//! writes `PLACEHOLDER_RUNTIME_PAYLOAD`, so that digest covers the foma payload and not the
//! compiled model.
//!
//! Hashing the canonical source instead is complete by construction. Compilation is deterministic,
//! so the canonical source plus the compiler version determines the model exactly, and no field can
//! be forgotten because no field is enumerated.
//!
//! ## What "canonical" covers, and what it does not
//!
//! [`SourceKind::Snapshot`] canonicalizes through JCS, so key order, whitespace, and line endings
//! are all absorbed. [`SourceKind::HcXml`] normalizes line endings only: attribute order and
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
            // Unparsable input is not this function's problem to diagnose — compilation will
            // report it properly. Fall back to line-ending normalization so a fingerprint still
            // exists for the failure artifact.
            Err(_) => Ok(normalize_line_endings(source)),
        },
        SourceKind::HcXml => Ok(normalize_line_endings(source)),
    }
}

fn normalize_line_endings(source: &str) -> String {
    source.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSION: &str = "test-compiler-1";

    const XML: &str =
        "<HermitCrabInput>\n  <Language>\n    <Name>X</Name>\n  </Language>\n</HermitCrabInput>\n";

    #[test]
    fn line_endings_move_the_source_hash_but_not_the_fingerprint() {
        // The exact case D3a exists for: one committed grammar, two checkouts.
        let lf = XML.to_string();
        let crlf = XML.replace('\n', "\r\n");

        assert_ne!(
            source_sha256(lf.as_bytes()),
            source_sha256(crlf.as_bytes()),
            "sourceSha256 identifies the file, so it must notice the bytes differ"
        );
        assert_eq!(
            model_fingerprint(SourceKind::HcXml, &lf, VERSION).unwrap(),
            model_fingerprint(SourceKind::HcXml, &crlf, VERSION).unwrap(),
            "modelFingerprint identifies what was analyzed, which did not change"
        );
    }

    #[test]
    fn any_content_change_moves_the_fingerprint() {
        let changed = XML.replace("<Name>X</Name>", "<Name>Y</Name>");
        assert_ne!(
            model_fingerprint(SourceKind::HcXml, XML, VERSION).unwrap(),
            model_fingerprint(SourceKind::HcXml, &changed, VERSION).unwrap()
        );
    }

    #[test]
    fn a_compiler_change_moves_the_fingerprint() {
        // Two compilers may turn identical source into different models, so the fingerprint has to
        // name the compiler. This is also why `outcomeDigest` excludes the fingerprint: an upgrade
        // that changes no analysis must not read as a behavior change.
        assert_ne!(
            model_fingerprint(SourceKind::HcXml, XML, "compiler-1").unwrap(),
            model_fingerprint(SourceKind::HcXml, XML, "compiler-2").unwrap()
        );
    }

    #[test]
    fn snapshot_key_order_and_whitespace_do_not_move_the_fingerprint() {
        let a = r#"{ "b": 1, "a": [2, 3] }"#;
        let b = r#"{"a":[2,3],"b":1}"#;
        assert_ne!(source_sha256(a.as_bytes()), source_sha256(b.as_bytes()));
        assert_eq!(
            model_fingerprint(SourceKind::Snapshot, a, VERSION).unwrap(),
            model_fingerprint(SourceKind::Snapshot, b, VERSION).unwrap()
        );
    }

    #[test]
    fn snapshot_array_order_does_move_the_fingerprint() {
        // Arrays are sequences: reordering them is a content change, not formatting.
        let a = r#"{"a":[2,3]}"#;
        let b = r#"{"a":[3,2]}"#;
        assert_ne!(
            model_fingerprint(SourceKind::Snapshot, a, VERSION).unwrap(),
            model_fingerprint(SourceKind::Snapshot, b, VERSION).unwrap()
        );
    }

    #[test]
    fn the_two_source_kinds_do_not_collide() {
        // Identical text under two source kinds compiles to different models.
        let text = "{}";
        assert_ne!(
            model_fingerprint(SourceKind::Snapshot, text, VERSION).unwrap(),
            model_fingerprint(SourceKind::HcXml, text, VERSION).unwrap()
        );
    }

    #[test]
    fn unparsable_snapshot_still_yields_a_fingerprint() {
        // A failed assessment artifact still records what it tried to compile.
        let broken = "{ not json";
        assert!(model_fingerprint(SourceKind::Snapshot, broken, VERSION).is_ok());
    }

    #[test]
    fn xml_attribute_order_moves_the_fingerprint_a_known_conservative_limit() {
        // Documented in the module doc: without real XML canonicalization this reports a
        // difference that is only formatting. Conservative, not silent — pinned so the limit is
        // visible rather than discovered.
        let a = r#"<E a="1" b="2"/>"#;
        let b = r#"<E b="2" a="1"/>"#;
        assert_ne!(
            model_fingerprint(SourceKind::HcXml, a, VERSION).unwrap(),
            model_fingerprint(SourceKind::HcXml, b, VERSION).unwrap()
        );
    }
}
