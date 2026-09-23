//! The R4 gloss signature: a shared canonical gloss/analysis-signature unit, matching
//! `machine/conformance/PROTOCOL.md`'s §3-4 multiset comparison semantics. Any caller doing a
//! gloss-signature parity check or gloss-based diagnostics is meant to call this one
//! implementation instead of growing its own — see this module's doc for why no such unit existed
//! to extract before this change (only the morpheme-ID-keyed `pg_parse::result_signature` did).
//!
//! **A parallel, gloss-keyed signature, not a replacement.** `pg_parse::result_signature`
//! (`crates/pg-parse/src/lib.rs`) is the frozen PROTOCOL.md §3 adapter-contract format —
//! `<morph1.MorphemeId>+...|<shape>` sorted and joined with `;` — that the conformance harness's
//! checked-in `expected.tsv` golden files already commit to; this module never touches it and
//! never changes its output. The gloss signature exists for callers that need to compare on
//! gloss + surface-shape terms alone (a gloss-only reference tool, or a grammar representation
//! whose morpheme IDs don't line up 1:1 with `grammar.xml`'s).
//!
//! # Per-analysis component encoding
//!
//! Each analysis contributes one entry, in `crate::gloss_bundle`'s own token order (root
//! included in place, not pulled out separately):
//!
//! - a token with a literal `<Gloss>` renders `g:<canonical-json-string>`;
//! - a token with no `<Gloss>` renders `m:<owning-morpheme-id-as-canonical-json-string>` — the
//!   owning morpheme's `<MorphemeId>` text, the same string `pg_parse::morpher`'s own
//!   `morpheme_join` already treats as *the* morpheme id for the plain signature (empty string
//!   when the grammar declares none; the `u32::MAX` guessed-root sentinel — which `BatchCommand`'s
//!   `batch`/`gloss-batch` contract can never actually produce, PROTOCOL.md §3's guess-stem note —
//!   resolves the same defensive way, empty string, rather than panicking);
//! - after all morpheme components (joined `+`), the analysis's surface shape renders
//!   `|s:<canonical-json-string>`.
//!
//! Every `<canonical-json-string>` is an RFC 8785 canonical JSON string: `serde_json::to_string`
//! on a `&str` already produces this (mandatory-only escapes for `"`, `\`, and control characters
//! 0x00-0x1F; no escaping of non-ASCII; no Unicode normalization of any kind), which is exactly
//! why `+`, `|`, and `;` inside a literal gloss or a surface shape never get mistaken for a
//! separator: those three bytes are separators **only outside** a JSON string, and the tagged
//! JSON-string encoding is what makes that unambiguous.
//!
//! # Multiset assembly
//!
//! A word's full signature multiset-joins its distinct analyses' entries, **sorted
//! lexicographically by unsigned canonical UTF-8 bytes** (Rust `str`/`[u8]` ordering already is
//! this — no locale/culture comparer is involved, mirroring PROTOCOL.md §3's own citation of C#'s
//! `StringComparer.Ordinal`), duplicates kept (never deduped — see
//! `pg_parse::result_signature`'s own doc for why a duplicate entry is real signal, not noise, the
//! same reasoning applies here), joined with `;`. Zero analyses render the literal `-`; a
//! `SKIPPED` row keeps that same literal by caller convention (callers hardcode it rather than
//! calling into this module at all, exactly how `pg_parse::result_signature`'s own callers already
//! treat `SKIPPED` in `pg-cli/src/main.rs`).
#![forbid(unsafe_code)]

use crate::gloss_bundle;
use pg_grammar::model::{Grammar, MorphemeId};
use pg_parse::WordAnalysis;

/// RFC 8785 canonical JSON string encoding: `serde_json::to_string` on a `&str` already satisfies it (mandatory-only escapes, no non-ASCII escaping, no normalization), and is infallible since a `String` sink never produces I/O errors.
fn canonical_json_string(s: &str) -> String {
    serde_json::to_string(s).expect("&str -> JSON string serialization is infallible")
}

/// The `<MorphemeId>` text owning a grammar-tier morpheme ordinal, empty when absent; the guessed-root sentinel has no `Grammar::morphemes` row at all, so it resolves the same defensive empty string an out-of-range ordinal would, never panicking.
fn owning_morpheme_id(grammar: &Grammar, morpheme_ordinal: u32) -> String {
    if morpheme_ordinal == MorphemeId::GUESSED.0 {
        return String::new();
    }
    grammar
        .morphemes
        .get(morpheme_ordinal as usize)
        .and_then(|m| m.morph_id.clone())
        .unwrap_or_default()
}

/// One analysis's gloss-signature entry: its tagged gloss/missing-gloss chain, `+`-joined, then
/// `|s:<canonical-json-string>` for `surface_shape`. `surface_shape` is the same already-rendered
/// shape string every other signature call site computes (`Shape.ToRegexString`'s Rust
/// equivalent) — this function never renders shape itself, it only encodes what it's given.
///
/// Callers assemble a word's full signature by collecting one entry per distinct analysis and
/// passing them to `gloss_analysis_set_signature` (or use `word_gloss_signature`, which does
/// both steps for a `(WordAnalysis, shape)` list in one call).
pub fn gloss_signature_entry(grammar: &Grammar, wa: &WordAnalysis, surface_shape: &str) -> String {
    let bundle = gloss_bundle(grammar, wa);
    let components: Vec<String> = bundle
        .tokens
        .iter()
        .zip(wa.morpheme_ids.iter())
        .map(|(token, &id)| match &token.gloss {
            Some(g) => format!("g:{}", canonical_json_string(g)),
            None => format!(
                "m:{}",
                canonical_json_string(&owning_morpheme_id(grammar, id))
            ),
        })
        .collect();
    format!(
        "{}|s:{}",
        components.join("+"),
        canonical_json_string(surface_shape)
    )
}

/// Assemble a word's full gloss signature from its distinct-analysis entries (each produced by
/// `gloss_signature_entry`): sorted lexicographically by unsigned canonical UTF-8 bytes,
/// duplicates preserved, joined with `;`. An empty entry set — zero analyses, or (by caller
/// convention) a `SKIPPED` row — renders the literal `-`.
pub fn gloss_analysis_set_signature(entries: &[String]) -> String {
    if entries.is_empty() {
        return "-".to_string();
    }
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    sorted.join(";")
}

/// Convenience wrapper combining `gloss_signature_entry` and `gloss_analysis_set_signature`
/// for a word's full `(WordAnalysis, surface_shape)` analysis list, mirroring
/// `pg_parse::result_signature`'s call shape one layer up (that function takes pre-rendered
/// `(morphs, surface)` string pairs; this one takes structured `WordAnalysis`es plus shape since
/// the gloss/missing-gloss resolution needs `Grammar` access per morpheme).
pub fn word_gloss_signature(grammar: &Grammar, analyses: &[(WordAnalysis, String)]) -> String {
    let entries: Vec<String> = analyses
        .iter()
        .map(|(wa, shape)| gloss_signature_entry(grammar, wa, shape))
        .collect();
    gloss_analysis_set_signature(&entries)
}

#[cfg(test)]
mod tests;
