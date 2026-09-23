//! The gloss-bundle extraction layer: additive, display-only presentation of an analysis, on top
//! of the frozen parity engine.
//!
//! `gloss_bundle` resolves a `pg_parse::WordAnalysis`'s grammar-tier morpheme ordinals against
//! `Grammar::morphemes` to produce a `GlossBundle`, and `leipzig` renders that bundle as a
//! Leipzig-style gloss string (`house-pl-poss.1s`). Neither function touches `result_signature`,
//! `ParseOutcome.analyses`, or any other parity-frozen output — this crate is consumed only by
//! new, additive call sites (`pg-cli`'s `--gloss` flag today; later IR/realizer layers build on
//! top of it).
//!
//! Degrades gracefully everywhere: an out-of-range morpheme ordinal, a missing gloss, or a
//! guessed root never panics — worst case is a `[?]` token in the rendered string.
//!
//! `ir` defines a typed IR on top of the gloss bundle: `ir::GlossIr` and its closed-enum
//! feature slots, and `map` defines `map::RealizeMap`, the per-grammar sidecar mapping from
//! raw gloss strings to those features. `ir::to_ir` builds a `GlossIr` from a `GlossBundle` the
//! same way `gloss_bundle`/`leipzig` are built: total, additive, never touching parity output.
//!
//! `realize` defines the realizer layer on top of the IR: the `realize::Realizer` trait and
//! its `realize::Realization` result type (the Architecture-A upgrade seam, see that module's
//! doc), and `table` defines `table::TableRealizer`, a compile-time English
//! construction-table implementation.
//!
//! `infer`: `infer_english` builds a `RealizeMap` straight from a grammar's affix glosses via
//! a built-in English alias table, so grammars without a hand-authored sidecar still get
//! natural-ish phrases. Wiring the inferred-map/sidecar-override precedence into `pg-wasm` is a
//! separate, later phase.
//!
//! `signature`: a canonical string encoding of a word's whole analysis set (`gloss_bundle`'s
//! tokens plus each analysis's surface shape), for shared use across downstream consumers that
//! need to compare whole analysis sets — see that module's doc for the full encoding, and its own
//! top-of-file note for why it is a parallel format next to `pg_parse::result_signature`, never a
//! change to it.
#![forbid(unsafe_code)]

use pg_grammar::model::Grammar;
use pg_parse::WordAnalysis;

pub mod infer;
pub mod ir;
pub mod map;
pub mod realize;
pub mod signature;
pub mod table;

pub use infer::infer_english;
pub use ir::{to_ir, CaseRole, Concept, GlossIr, Num, Poss};
pub use map::{FeatureAssignment, MapError, RealizeMap};
pub use realize::{Realization, Realizer};
pub use signature::{gloss_analysis_set_signature, gloss_signature_entry, word_gloss_signature};
pub use table::TableRealizer;

/// One morpheme's display data, resolved from `Grammar::morphemes` (or synthesized for the
/// guessed-root sentinel / an out-of-range ordinal — see `gloss_bundle`'s doc for the exact
/// resolution rules).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossToken {
    /// The morpheme's `<Gloss>` text, or `None` when the grammar author didn't supply one, the
    /// token is the `u32::MAX` guessed-root sentinel, or the ordinal was out of range.
    pub gloss: Option<String>,
    /// The morpheme's `<Properties>` entries (`MorphemeInfo::properties`), or empty for the
    /// guessed-root sentinel / an out-of-range ordinal.
    pub properties: Vec<(String, String)>,
    /// True for the token at `WordAnalysis.root_morpheme_index` and, defensively, for the
    /// guessed-root sentinel (`u32::MAX`) even if the index bookkeeping ever disagreed — a
    /// guessed root is always semantically the root.
    pub is_root: bool,
}

/// A word analysis's morphemes resolved to display data, in surface morpheme order (root
/// included at its own position — not pulled out separately) — the Rust mirror of C#'s
/// `Morpher.cs` gloss display data, built fresh per analysis by `gloss_bundle`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossBundle {
    /// One token per `WordAnalysis.morpheme_ids` entry, same order, same length.
    pub tokens: Vec<GlossToken>,
    /// Index into `tokens` of the root, or `None` when `WordAnalysis.root_morpheme_index` was
    /// negative or out of range (defensive; the normal engine always sets a valid index).
    pub root_index: Option<usize>,
    /// Mirrors `WordAnalysis.pos_id` unchanged.
    pub pos_id: Option<u32>,
    /// Mirrors `WordAnalysis.guessed` unchanged — whether this analysis came from the guess
    /// branch (`ParseOptions.guess_root = true` on a total lexicon miss).
    pub guessed: bool,
}

/// Resolve one `WordAnalysis`'s morpheme ordinals against `grammar.morphemes` into a
/// `GlossBundle`, in morpheme order.
///
/// Each `wa.morpheme_ids[i]` is a dense ordinal into `grammar.morphemes` EXCEPT the sentinel
/// `u32::MAX` (`pg_grammar::model::MorphemeId::GUESSED`), which is the fabricated root a
/// `guess_root` parse produces when the real lexicon has no entry — it has no `Grammar::morphemes`
/// row at all, so it resolves to a gloss-less, property-less, `is_root: true` token instead of an
/// index lookup. An out-of-range non-sentinel id (defensive only — the engine never emits one)
/// resolves the same way except `is_root` still follows `root_morpheme_index`, never panicking.
pub fn gloss_bundle(grammar: &Grammar, wa: &WordAnalysis) -> GlossBundle {
    let root_index = usize::try_from(wa.root_morpheme_index)
        .ok()
        .filter(|&i| i < wa.morpheme_ids.len());

    let tokens = wa
        .morpheme_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let guessed_sentinel = id == u32::MAX;
            let is_root = guessed_sentinel || root_index == Some(i);
            if guessed_sentinel {
                return GlossToken {
                    gloss: None,
                    properties: Vec::new(),
                    is_root,
                };
            }
            match grammar.morphemes.get(id as usize) {
                Some(info) => GlossToken {
                    gloss: info.gloss.clone(),
                    properties: info.properties.clone(),
                    is_root,
                },
                None => GlossToken {
                    gloss: None,
                    properties: Vec::new(),
                    is_root,
                },
            }
        })
        .collect();

    GlossBundle {
        tokens,
        root_index,
        pos_id: wa.pos_id,
        guessed: wa.guessed,
    }
}

/// Render a `GlossBundle` as a Leipzig-style gloss string: each token's rendering, joined with
/// `-` (`house-pl-poss.1s`). A token renders as, in priority order: its `gloss` if present; else,
/// when it is the bundle's (guessed) root, `*{surface_word}*`; else `[?]` (an unglossed real
/// morpheme, or a defensive out-of-range ordinal). An empty bundle renders as the empty string.
///
/// "Guessed root" here means `token.is_root && bundle.guessed` — a real (non-guessed) root
/// lacking a `<Gloss>` still renders `[?]`, not the surface word, since only the guess branch
/// fabricates the token from the surface form in the first place.
pub fn leipzig(bundle: &GlossBundle, surface_word: &str) -> String {
    bundle
        .tokens
        .iter()
        .map(|token| match &token.gloss {
            Some(g) => g.clone(),
            None if token.is_root && bundle.guessed => format!("*{surface_word}*"),
            None => "[?]".to_string(),
        })
        .collect::<Vec<String>>()
        .join("-")
}

#[cfg(test)]
mod tests;
