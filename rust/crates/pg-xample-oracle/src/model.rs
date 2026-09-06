//! The comparable result model: one [`AnalysisSignature`] per analysis, counted into a
//! [`XampleResult`] multiset per word.

use std::cmp::Ordering;
use std::collections::BTreeMap;

/// One analysis's identity, keyed for cross-engine comparison.
///
/// `msa_ids` and `category_id` are stable LCM guids (the XAMPLE side) or the equivalent HC XML
/// `id=` keys (the HC side, via `pg_parse::identity::AnalysisIdentity`) — never hvos, never
/// engine-generated text. `surface_nfd` is the NFD-normalized surface form both engines were asked
/// to parse.
///
/// `morphemes` is NOT identity, and is EXCLUDED from `Eq`/`Ord` below (hand-written, not derived):
/// it carries the projector's own `morphnameOrGloss` (XAMPLE) or `MorphemeInfo::gloss` (HC) — a
/// human label an author chose for their own reading convenience, and the two engines have no
/// reason to spell it identically for the same morpheme (`ParseCommand.cs`'s `morphnameOrGloss`
/// reads LibLCM's `BestVernacularAlternative`/`BestAnalysisAlternative`; HC's `gloss_of` reads
/// `Grammar::morphemes[_].gloss` — unrelated strings, same GUID). Neither the parse JSON
/// (`msaGuid` is the only per-morph guid the projector emits) nor a same-instructions constraint
/// against modifying `tools/xample-projector/**` leaves a stable per-morph key to promote
/// `morphemes` to instead, so it stays a non-comparing label carried alongside the key rather than
/// inside it — a real GUID-bearing field would be the better fix if the projector ever emits one.
#[derive(Debug, Clone)]
pub struct AnalysisSignature {
    pub morphemes: Vec<String>,
    pub msa_ids: Vec<String>,
    pub category_id: Option<String>,
    pub surface_nfd: String,
}

impl AnalysisSignature {
    /// The tuple `Eq`/`Ord` actually compare — every identity field, `morphemes` deliberately absent.
    fn identity_key(&self) -> (&[String], &Option<String>, &str) {
        (&self.msa_ids, &self.category_id, &self.surface_nfd)
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
        let b = sig(&["different-label", "other-label"], &["guid-p1", "guid-k"], "xk");
        assert_eq!(a, b, "morphemes must never participate in AnalysisSignature identity");
        let mut analyses = BTreeMap::new();
        *analyses.entry(a).or_insert(0) += 1;
        *analyses.entry(b).or_insert(0) += 1;
        assert_eq!(analyses.len(), 1, "the two arrivals must collapse into one multiset entry");
        assert_eq!(*analyses.values().next().unwrap(), 2, "and their counts must sum");
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
