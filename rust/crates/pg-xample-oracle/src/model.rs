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
mod tests {
    use super::*;

    fn sig(morphemes: &[&str], msa_ids: &[&str], surface: &str) -> AnalysisSignature {
        AnalysisSignature {
            morphemes: morphemes.iter().map(|s| s.to_string()).collect(),
            msa_ids: msa_ids.iter().map(|s| s.to_string()).collect(),
            category_id: None,
            surface_nfd: surface.to_string(),
        }
    }

    #[test]
    fn identical_signatures_count_rather_than_collapse() {
        let mut analyses = BTreeMap::new();
        let a = sig(&["P1", "K"], &["guid-p1", "guid-k"], "xk");
        *analyses.entry(a.clone()).or_insert(0) += 1;
        *analyses.entry(a.clone()).or_insert(0) += 1;
        assert_eq!(analyses.len(), 1, "one distinct signature key");
        assert_eq!(analyses[&a], 2, "counted twice, never deduplicated to 1");
    }

    #[test]
    fn xample_result_partial_eq_must_keep_comparing_reached_max_analyses() {
        // A fence against a future hand-written XampleResult comparison dropping this field the way AnalysisSignature's derive once dropped its morphemes exclusion.
        let mut analyses = BTreeMap::new();
        analyses.insert(sig(&["P1", "K"], &["guid-p1", "guid-k"], "xk"), 1);
        let capped = XampleResult {
            analyses: analyses.clone(),
            reached_max_analyses: Some(1),
            engine_error: None,
        };
        let uncapped = XampleResult {
            analyses,
            reached_max_analyses: None,
            engine_error: None,
        };
        assert_ne!(
            capped, uncapped,
            "a capped result must never compare equal to an uncapped one with the same members"
        );
    }

    #[test]
    fn xample_result_partial_eq_must_keep_comparing_engine_error() {
        // Same fence as above, for engine_error: an empty analyses set alone must never look like success.
        let empty_success = XampleResult {
            analyses: BTreeMap::new(),
            reached_max_analyses: None,
            engine_error: None,
        };
        let empty_failure = XampleResult {
            analyses: BTreeMap::new(),
            reached_max_analyses: None,
            engine_error: Some("LoadFiles failed".to_string()),
        };
        assert_ne!(
            empty_success, empty_failure,
            "an engine error must never read as an ordinary empty result"
        );
    }

    #[test]
    fn signatures_differing_only_in_display_morphemes_are_equal_and_merge_counts() {
        let a = sig(&["P1", "K"], &["guid-p1", "guid-k"], "xk");
        let b = sig(
            &["different-label", "other-label"],
            &["guid-p1", "guid-k"],
            "xk",
        );
        assert_eq!(
            a, b,
            "morphemes must never participate in AnalysisSignature identity"
        );
        let mut analyses = BTreeMap::new();
        *analyses.entry(a).or_insert(0) += 1;
        *analyses.entry(b).or_insert(0) += 1;
        assert_eq!(
            analyses.len(),
            1,
            "the two arrivals must collapse into one multiset entry"
        );
        assert_eq!(
            *analyses.values().next().unwrap(),
            2,
            "and their counts must sum"
        );
    }

    #[test]
    fn signatures_differing_only_in_category_id_are_equal_and_merge_counts() {
        // category_id must not participate in identity: XAMPLE's own categoryId is unpopulated.
        let mut xample_side = sig(&["K"], &["guid-k"], "k");
        xample_side.category_id = None;
        let mut hc_side = sig(&["K"], &["guid-k"], "k");
        hc_side.category_id = Some("pos-guid".to_string());
        assert_eq!(
            xample_side, hc_side,
            "category_id must never participate in AnalysisSignature identity"
        );
        let mut analyses = BTreeMap::new();
        *analyses.entry(xample_side).or_insert(0) += 1;
        *analyses.entry(hc_side).or_insert(0) += 1;
        assert_eq!(
            analyses.len(),
            1,
            "the two arrivals must collapse into one multiset entry"
        );
        assert_eq!(
            *analyses.values().next().unwrap(),
            2,
            "and their counts must sum"
        );
    }

    #[test]
    fn error_can_coexist_with_non_empty_analyses() {
        // The other shape the type must not forbid: a partial result alongside a reported failure.
        let mut analyses = BTreeMap::new();
        analyses.insert(sig(&["K"], &["guid-k"], "k"), 1);
        let partial_then_failed = XampleResult {
            analyses,
            reached_max_analyses: None,
            engine_error: Some("engine exception mid-word".to_string()),
        };
        assert!(!partial_then_failed.analyses.is_empty());
        assert!(partial_then_failed.engine_error.is_some());
    }
}
