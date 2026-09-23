//! The comparable result model: one [`AnalysisSignature`] per analysis, counted into a
//! [`XampleResult`] multiset per word.

use std::cmp::Ordering;
use std::collections::BTreeMap;

/// One analysis's identity, keyed for cross-engine comparison.
///
/// `msa_ids` and `category_id` are stable LCM guids (the XAMPLE side) or the equivalent HC XML
/// `id=` keys (the HC side, via `pg_parse::identity::AnalysisIdentity`) — never hvos, never
/// engine-generated text. A `msa_ids` entry is either a bare guid, or (an irregularly inflected
/// variant sharing its MSA with other variants) `"{variantEntryGuid}#{msaGuid}"` — the same
/// composite key `pg_grammar::compile::lexicon::build_variant_stem_entry` gives that variant on
/// the HC side, so a variant HC distinguishes by entry is no longer collapsed to one shared key on
/// the XAMPLE side either (`ParseCommand.cs`'s `DescribeMorph`) — any other shape is pinned refused
/// by `hvo_shaped_msa_guid_is_refused_not_trusted`. `surface_nfd` is the NFD-normalized surface form
/// both engines were asked to parse.
///
/// Three comparability ceilings remain, different in KIND, not just degree.
/// `crate::hc::signature_from_word_analysis` drops `AnalysisIdentity::root_index` because XAMPLE's
/// JSON has no root-position field either — SYMMETRIC: both sides lose the same information, so
/// neither can fabricate a divergence from it. A narrower variant gap is ASYMMETRIC still:
/// `DescribeMorph` composes the guid pair above only when a variant's MSI DbRef directly names a
/// resolvable MSA hvo (`XAmpleParser.cs`'s case 4, `stemMsa != null`); a bare-LexEntry-hvo variant
/// (case 3) or case 4's own sense-derived fallback instead report `msaGuid: null`, refused by
/// `crate::reader::ReadError::MissingMsaGuid` rather than silently collapsed — a refused read, never
/// a false divergence, but with no live fixture yet to regenerate a capture exercising either shape.
/// `category_id` is a THIRD, measured against a real category-bearing grammar rather than assumed:
/// `tools/xample-projector/README.md`'s own `parse` doc already flags its `categoryId` extraction
/// as "a best-effort read... no fixture in this slice's live proof exercises a non-null value,
/// so it is unverified" — and running this crate's differential gate against
/// `edge-cases/deep-optional-affix-nesting` (every lexical entry and rule carries `posV`) confirmed
/// it: XAMPLE reported `categoryId: null` for every analysis while HC's own identity resolved a real
/// part-of-speech guid, would-be diverging 100% of analyses on a field neither engine's absence of
/// which says anything about the grammar. Symmetric with `root_index`, so treated the same way.
///
/// `morphemes` and `category_id` are NOT identity, and are EXCLUDED from `Eq`/`Ord` below
/// (hand-written, not derived). `morphemes` carries the projector's own `morphnameOrGloss` (XAMPLE)
/// or `MorphemeInfo::gloss` (HC) — a human label an author chose for their own reading convenience,
/// and the two engines have no reason to spell it identically for the same morpheme
/// (`ParseCommand.cs`'s `morphnameOrGloss` reads LibLCM's
/// `BestVernacularAlternative`/`BestAnalysisAlternative`; HC's `gloss_of` reads
/// `Grammar::morphemes[_].gloss` — unrelated strings, same GUID), so it stays a non-comparing label
/// carried alongside the key rather than inside it. `category_id` stays in the struct as an
/// informational field (still readable, still serialized where a caller wants it) for the same
/// reason: comparing it would compare XAMPLE's structural inability to report it against whatever
/// HC happens to resolve, which is not a fact about either engine's analysis of the word.
#[derive(Debug, Clone)]
pub struct AnalysisSignature {
    pub morphemes: Vec<String>,
    pub msa_ids: Vec<String>,
    pub category_id: Option<String>,
    pub surface_nfd: String,
}

impl AnalysisSignature {
    /// The tuple `Eq`/`Ord` actually compare — every identity field, `morphemes`/`category_id` deliberately absent.
    fn identity_key(&self) -> (&[String], &str) {
        (&self.msa_ids, &self.surface_nfd)
    }
}

impl PartialEq for AnalysisSignature {
    fn eq(&self, other: &Self) -> bool {
        self.identity_key() == other.identity_key()
    }
}

impl Eq for AnalysisSignature {}

impl PartialOrd for AnalysisSignature {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AnalysisSignature {
    fn cmp(&self, other: &Self) -> Ordering {
        self.identity_key().cmp(&other.identity_key())
    }
}

/// One word's XAMPLE (or HC, once normalized — see `crate::hc`) result.
///
/// `analyses` is a multiset: a duplicate-looking analysis is a real differential signal (two
/// distinct derivations that happen to render the same), never deduplicated away. `usize` counts
/// how many times each distinct [`AnalysisSignature`] was reported.
///
/// `reached_max_analyses` and `engine_error` are independent facts about the SAME result, never a
/// substitute for one another:
/// - `Some(n)` in `reached_max_analyses` means the engine stopped after `n` analyses without
///   exhausting the search — this result is a LOWER bound, not the complete analysis set. Because
///   it participates in the derived `PartialEq` alongside `analyses`, two results with an identical
///   member set but different `reached_max_analyses` never compare equal (see
///   `xample_result_partial_eq_must_keep_comparing_reached_max_analyses` below) — a caller cannot
///   accidentally treat a capped multiset as if it were exhaustive just because its members match.
/// - `Some(_)` in `engine_error` means the engine reported a failure for this word. `analyses` may
///   still be non-empty (a partial result before the failure) or empty; either way, an empty
///   `analyses` with `engine_error: None` is a genuine "zero analyses" answer, and the two must
///   never be conflated by checking emptiness alone (see
///   `xample_result_partial_eq_must_keep_comparing_engine_error` below).
#[derive(Debug, Clone, PartialEq)]
pub struct XampleResult {
    pub analyses: BTreeMap<AnalysisSignature, usize>,
    pub reached_max_analyses: Option<usize>,
    pub engine_error: Option<String>,
}

#[cfg(test)]
mod tests;
