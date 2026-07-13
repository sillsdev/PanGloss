//! The Morpher pipeline (plan §2.1, §5): segment → analyze (unapply) → lexical lookup →
//! synthesize (confirm) → dedup → canonical order → morpheme sequences out.
//!
//! This is the only crate `hc-ffi` and `hc-cli` call; everything below it is engine-internal
//! and free to change shape under the parity gates. Per-word transients allocate in a
//! `bumpalo::Bump` arena reset at word completion — the no-GC answer to the Server-GC memory
//! blowup (plan §6).
#![forbid(unsafe_code)]

pub mod batch;
pub mod guess;
pub mod morpher;
pub mod root_trie;
pub mod surface;

pub use batch::{hc_parse_batch, BatchWordOutcome};
pub use morpher::{GenMorpheme, Morpher, ParseOptions, ParseOutcome};
pub use root_trie::{RootAllomorphIndex, RootAllomorphTrie};

/// One analysis of a word: ordered morpheme ids, the root's index within that sequence, and an
/// optional part-of-speech id — the Rust mirror of C# `WordAnalysis` (Morpher.cs:637).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordAnalysis {
    pub morpheme_ids: Vec<u32>,
    pub root_morpheme_index: i32,
    pub pos_id: Option<u32>,
    /// P11 §4.1: whether this analysis came from the guess branch (`Morpher::parse_word_opts`
    /// with `ParseOptions.guess_root = true`, on a total normal-lexicon miss). Always equal to
    /// the owning `ParseOutcome.guessed` today (the branch is all-or-nothing), but carried
    /// per-analysis too since the wire format shouldn't bake in that coupling (`hc-ffi`'s eventual
    /// consumer reads one `WordAnalysis` at a time). `morpheme_ids` carries `AllomorphId::GUESSED`/
    /// `MorphemeId::GUESSED`'s numeric value (`u32::MAX`) for the fabricated root's own slot when
    /// `guessed` is true — no separate sentinel handling needed here, the id just passes through.
    pub guessed: bool,
}

/// The canonical signature of a result set, matching the C# `BatchCommand` protocol exactly so
/// existing managed golden TSVs are directly reusable (plan §8 layer 0). Per analysis:
/// `join("+", morpheme string ids) + "|" + surface`; the set is sorted and joined with `;`;
/// an empty set is `"-"`.
///
/// Deliberately **not** deduped: C#'s own gold reference legitimately repeats identical-looking
/// rendered strings for genuinely distinct underlying analyses (e.g. Sena `mbali` →
/// `++|[mn]+?bal+?i` ×9, `+|[mn]+?bali` ×6, in both `parity-out/golden/*/sena-*.tsv`; same
/// phenomenon covers the free-fluctuation case in
/// `sena_free_fluctuation_gate.rs`). This was flagged as a tear-out candidate
/// (rust-optimizations-phase2.md T-B) and rejected after a dedup attempt broke that gate — the
/// duplicate count is real signal, not byte-parity noise. See
/// `identical_rendered_signatures_are_kept_not_deduped` below.
pub fn result_signature(analyses: &[(String, String)]) -> String {
    if analyses.is_empty() {
        return "-".to_string();
    }
    let mut sigs: Vec<String> = analyses
        .iter()
        .map(|(morphs, surface)| format!("{morphs}|{surface}"))
        .collect();
    sigs.sort();
    sigs.join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_result_is_dash() {
        assert_eq!(result_signature(&[]), "-");
    }

    #[test]
    fn signatures_are_sorted_and_joined() {
        let got = result_signature(&[("b+c".into(), "surf".into()), ("a+c".into(), "surf".into())]);
        assert_eq!(got, "a+c|surf;b+c|surf");
    }

    #[test]
    fn identical_rendered_signatures_are_kept_not_deduped() {
        // See result_signature's doc comment: duplicates here can be genuinely distinct
        // analyses (free fluctuation) that happen to render identically — not noise.
        let got = result_signature(&[
            ("a+c".into(), "surf".into()),
            ("b+c".into(), "surf".into()),
            ("a+c".into(), "surf".into()),
        ]);
        assert_eq!(got, "a+c|surf;a+c|surf;b+c|surf");
    }
}
