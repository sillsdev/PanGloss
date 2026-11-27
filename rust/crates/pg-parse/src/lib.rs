//! The Morpher pipeline (plan §2.1, §5): segment → analyze (unapply) → lexical lookup →
//! synthesize (confirm) → dedup → canonical order → morpheme sequences out.
//!
//! This is the only crate `pg-ffi` and `pg-cli` call; everything below it is engine-internal
//! and free to change shape under the parity gates. Per-word transients allocate in a
//! `bumpalo::Bump` arena reset at word completion — the no-GC answer to the Server-GC memory
//! blowup (plan §6).
#![forbid(unsafe_code)]

pub mod alt_yield;
pub mod batch;
pub mod guess;
pub mod identity;
mod lookup_replay;
pub mod morpher;
pub mod overlay;
pub mod parse_morph;
pub mod partials;
pub use partials::PartialAnalysis;
pub mod phonology;
pub mod pins;
mod root_answers;
mod root_search_cache;
pub mod root_trie;
pub mod source_identity;
pub mod surface;
pub mod why_not;

pub use batch::{hc_parse_batch, BatchWordOutcome};
pub use identity::{AnalysisIdentity, IdentityError, MorphemeKey, IDENTITY_PROFILE};
pub use morpher::{
    GenMorpheme, GenerateWordsOutcome, GenerateWordsStopReason, Morpher, ParseOptions,
    ParseOutcome, StopReasons, SynthesisBudget, TracedSynthesisOutcome,
};
pub use overlay::{RootAuthority, SuppliedRoot, SuppliedRootOverlay};
pub use parse_morph::{
    project_parse_analyses, project_parse_analysis, project_parse_analysis_with_morphemes,
    ParseAnalysis, ParseMorph, ParseProjectionError, ProjectedMorphs, PARSE_ANALYSIS_PROFILE,
};
pub use phonology::{
    measure_phonology, PhonologyCompletion, PhonologyContext, PhonologyMeasurement,
    PhonologyRequest, PhonologyResult, PhonologyStopReason, PhonologyUnavailableReason,
};
pub use root_trie::{RootAllomorphIndex, RootAllomorphTrie};

/// One analysis of a word: ordered morpheme ids, the root's index within that sequence, and an
/// optional part-of-speech id — the Rust mirror of C# `WordAnalysis` (Morpher.cs:637).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordAnalysis<Mpr = pg_grammar_model::model::MprSet> {
    pub morpheme_ids: Vec<u32>,
    /// Every final annotation record in surface order, including repeated applications and
    /// multiple runs emitted by a circumfix.
    pub morph_occurrences: Vec<MorphOccurrence>,
    pub root_morpheme_index: i32,
    pub pos_id: Option<u32>,
    pub syn_fs: pg_featstruct::FeatureStruct,
    pub mpr: Mpr,
    /// P11 §4.1: whether this analysis came from the guess branch (`Morpher::parse_word_opts`
    /// with `ParseOptions.guess_root = true`, on a total normal-lexicon miss). Always equal to
    /// the owning `ParseOutcome.guessed` today (the branch is all-or-nothing), but carried
    /// per-analysis too since the wire format shouldn't bake in that coupling (`pg-ffi`'s eventual
    /// consumer reads one `WordAnalysis` at a time). `morpheme_ids` carries `AllomorphId::GUESSED`/
    /// `MorphemeId::GUESSED`'s numeric value (`u32::MAX`) for the fabricated root's own slot when
    /// `guessed` is true — no separate sentinel handling needed here, the id just passes through.
    pub guessed: bool,
    /// Exact text carried by a fabricated guessed root, when this analysis contains one.
    pub guessed_string: Option<String>,
    /// The lexical pattern a guessed root was fabricated from; C# copies its allomorph's and
    /// entry's properties onto the guess, so a FieldWorks host reports the pattern's MoForm and MSA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guessed_pattern: Option<GuessedPattern>,
    pub provenance: AnalysisProvenance,
    /// Self-contained root payload for regeneration when provenance is supplied.
    pub supplied_root: Option<SuppliedRoot<Mpr>>,
    /// Runtime supplied payload aligned one-for-one with `morpheme_ids`.
    pub morpheme_roots: Vec<Option<SuppliedRoot<Mpr>>>,
}

impl<Mpr> WordAnalysis<Mpr> {
    /// Converts every MPR field, including both nested root payload locations.
    pub fn try_map_mpr<E>(
        self,
        mut map: impl FnMut(Mpr) -> Result<pg_grammar_model::model::MprSet, E>,
    ) -> Result<WordAnalysis, E> {
        Ok(WordAnalysis {
            morpheme_ids: self.morpheme_ids,
            morph_occurrences: self.morph_occurrences,
            root_morpheme_index: self.root_morpheme_index,
            pos_id: self.pos_id,
            syn_fs: self.syn_fs,
            mpr: map(self.mpr)?,
            guessed: self.guessed,
            guessed_string: self.guessed_string,
            guessed_pattern: self.guessed_pattern,
            provenance: self.provenance,
            supplied_root: self
                .supplied_root
                .map(|root| root.try_map_mpr(&mut map))
                .transpose()?,
            morpheme_roots: self
                .morpheme_roots
                .into_iter()
                .map(|root| root.map(|root| root.try_map_mpr(&mut map)).transpose())
                .collect::<Result<_, E>>()?,
        })
    }
}

/// Dense ids of the lexical-pattern root allomorph and its owning morpheme behind a guessed root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuessedPattern {
    pub allomorph_id: u32,
    pub morpheme_id: u32,
}

/// Dense runtime identity and surface position for one final annotation record.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MorphOccurrence {
    pub allomorph_id: u32,
    pub morpheme_id: u32,
    pub order: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AnalysisProvenance {
    Grammar,
    Supplied {
        entry_id: String,
    },
    SuppliedOverride {
        entry_id: String,
        overridden_grammar_entry_id: String,
    },
    Guessed,
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
/// `free_fluctuation_gate.rs`). This was flagged as a tear-out candidate
/// (rust-optimizations-phase2.md T-B) and rejected after a dedup attempt broke that gate — the
/// duplicate count is real signal, not byte-parity noise. See
/// `identical_rendered_signatures_are_kept_not_deduped` below.
pub fn result_signature(analyses: &[(String, String)]) -> String {
    if analyses.is_empty() {
        return "-".to_string();
    }
    result_multiset(analyses).join(";")
}

/// The sorted, non-deduped entries `result_signature` joins — exposed separately so a caller (the
/// conformance replay) can compare multiplicities directly instead of re-parsing the joined string.
pub fn result_multiset(analyses: &[(String, String)]) -> Vec<String> {
    let mut sigs: Vec<String> = analyses
        .iter()
        .map(|(morphs, surface)| format!("{morphs}|{surface}"))
        .collect();
    sigs.sort();
    sigs
}

#[cfg(test)]
mod tests;
