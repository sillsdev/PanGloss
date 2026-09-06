//! The comparable result model: one [`AnalysisSignature`] per analysis, counted into a
//! [`XampleResult`] multiset per word.

use std::collections::BTreeMap;

/// One analysis's identity, keyed for cross-engine comparison.
///
/// `msa_ids` and `category_id` are stable LCM guids (the XAMPLE side) or the equivalent HC XML
/// `id=` keys (the HC side, via `pg_parse::identity::AnalysisIdentity`) — never hvos, never
/// engine-generated text. `surface_nfd` is the NFD-normalized surface form both engines were asked
/// to parse.
///
/// `morphemes` is deliberately NOT part of any engine's identity: it carries the projector's own
/// `morphnameOrGloss` (XAMPLE) or `MorphemeInfo::gloss` (HC) — a human label an author chose for
/// their own reading convenience, not a stable key, and the two engines have no reason to spell it
/// identically for the same morpheme. It exists purely so a divergence report can show what a
/// mismatched analysis names its pieces; a comparison must never use it to decide whether two
/// analyses are the same one, only `msa_ids`/`category_id`/`surface_nfd` may decide that.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AnalysisSignature {
    pub morphemes: Vec<String>,
    pub msa_ids: Vec<String>,
    pub category_id: Option<String>,
    pub surface_nfd: String,
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
///   it participates in `PartialEq`/`Ord` alongside `analyses`, two results with an identical
///   member set but different `reached_max_analyses` never compare equal (see
///   `capped_result_is_distinguishable_from_uncapped_with_same_members` below) — a caller cannot
///   accidentally treat a capped multiset as if it were exhaustive just because its members match.
/// - `Some(_)` in `engine_error` means the engine reported a failure for this word. `analyses` may
///   still be non-empty (a partial result before the failure) or empty; either way, an empty
///   `analyses` with `engine_error: None` is a genuine "zero analyses" answer, and the two must
///   never be conflated by checking emptiness alone (see
///   `engine_error_is_distinguishable_from_empty_success` below).
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
    fn capped_result_is_distinguishable_from_uncapped_with_same_members() {
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
    fn engine_error_is_distinguishable_from_empty_success() {
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
