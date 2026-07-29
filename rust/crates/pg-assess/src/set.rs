//! Canonical analysis sets: deduplicated by identity, sorted by digest, duplicates retained.
//!
//! `define-grammar-coverage-contract`: "Exact duplicate copies of the same complete structured
//! analysis identity SHALL be deduplicated for semantic equality; their counts and provenance SHALL
//! remain diagnostic evidence." Two runs that found the same analyses a different number of times
//! agree semantically and differ in health evidence, so duplicate counts participate in the
//! semantic projection and are excluded from the outcome projection (design D3/D4).

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::digest::identity_digest;
use crate::identity::AnalysisIdentity;

/// One distinct analysis and how many times it was observed before deduplication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisSetEntry {
    pub identity: AnalysisIdentity,
    pub identity_digest: String,
    /// Copies observed before deduplication. Always at least 1.
    pub duplicate_count: u32,
    /// Whether this analysis came from the guess branch — a fabricated root on a total
    /// normal-lexicon miss. An annotation, never part of identity: `false → true` across two runs
    /// means the root stopped being found, which is a real regression the delta reports as
    /// `annotation_changed` rather than as an addition paired with a removal.
    pub guessed: bool,
}

/// A complete analysis set in canonical order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnalysisSet {
    entries: Vec<AnalysisSetEntry>,
}

impl AnalysisSet {
    /// Build from observed analyses in discovery order, without annotations.
    pub fn from_observed<I: IntoIterator<Item = AnalysisIdentity>>(observed: I) -> Self {
        Self::from_annotated(observed.into_iter().map(|identity| (identity, false)))
    }

    /// Build from observed `(identity, guessed)` pairs in discovery order.
    ///
    /// Discovery order is deliberately discarded: it is not semantic, and letting it reach a digest
    /// would make engine internals observable as grammar changes.
    pub fn from_annotated<I: IntoIterator<Item = (AnalysisIdentity, bool)>>(observed: I) -> Self {
        // Keyed by digest so ordering is stable under any future field reordering of
        // `AnalysisIdentity`, which a derived `Ord` would not survive.
        let mut by_digest: BTreeMap<String, AnalysisSetEntry> = BTreeMap::new();
        for (identity, guessed) in observed {
            let digest = identity_digest(&identity);
            match by_digest.get_mut(&digest) {
                Some(existing) => {
                    debug_assert_eq!(
                        existing.identity, identity,
                        "identity digest collision: unequal identities share {digest}"
                    );
                    existing.duplicate_count += 1;
                    // The guess branch is all-or-nothing per parse, so copies of one identity agree
                    // in practice. If they ever disagree, keep the fact that a fabricated root was
                    // involved rather than letting whichever copy arrived last decide.
                    existing.guessed |= guessed;
                }
                None => {
                    by_digest.insert(
                        digest.clone(),
                        AnalysisSetEntry {
                            identity,
                            identity_digest: digest,
                            duplicate_count: 1,
                            guessed,
                        },
                    );
                }
            }
        }

        AnalysisSet {
            entries: by_digest.into_values().collect(),
        }
    }

    pub fn entries(&self) -> &[AnalysisSetEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether this set contains an identity, confirmed against the structured value rather than
    /// trusting the digest alone.
    pub fn contains(&self, identity: &AnalysisIdentity) -> bool {
        let digest = identity_digest(identity);
        self.entries
            .iter()
            .any(|e| e.identity_digest == digest && &e.identity == identity)
    }

    /// The outcome projection's view: identities and their `guessed` annotation. Blind to how many
    /// times each was found, so an FST change that removes redundant proposal paths leaves this
    /// untouched.
    ///
    /// `guessed` is here despite being an annotation rather than identity, because `false → true`
    /// means the root stopped being found in the lexicon and the parser fabricated one. That is a
    /// behavior change, and an outcome digest blind to it would report "the grammar behaved the
    /// same" about a real regression.
    pub fn to_outcome_value(&self) -> Value {
        Value::Array(
            self.entries
                .iter()
                .map(|e| {
                    json!({
                        "identity": e.identity.to_canonical_value(),
                        "guessed": e.guessed,
                    })
                })
                .collect(),
        )
    }

    /// The semantic projection's view: the outcome view plus duplicate evidence.
    pub fn to_semantic_value(&self) -> Value {
        Value::Array(
            self.entries
                .iter()
                .map(|e| {
                    json!({
                        "identity": e.identity.to_canonical_value(),
                        "guessed": e.guessed,
                        "duplicateCount": e.duplicate_count,
                    })
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(morphemes: &[&str]) -> AnalysisIdentity {
        AnalysisIdentity {
            morphemes: morphemes.iter().map(|m| Some(m.to_string())).collect(),
            root_index: 0,
            category: None,
        }
    }

    #[test]
    fn discovery_order_does_not_change_the_set() {
        let a = AnalysisSet::from_observed([id(&["x"]), id(&["y"]), id(&["z"])]);
        let b = AnalysisSet::from_observed([id(&["z"]), id(&["x"]), id(&["y"])]);
        assert_eq!(a, b);
        assert_eq!(a.to_outcome_value(), b.to_outcome_value());
    }

    #[test]
    fn duplicates_collapse_but_are_counted() {
        let set = AnalysisSet::from_observed([id(&["x"]), id(&["x"]), id(&["y"])]);
        assert_eq!(set.len(), 2);
        let x = set
            .entries()
            .iter()
            .find(|e| e.identity == id(&["x"]))
            .unwrap();
        assert_eq!(x.duplicate_count, 2);
    }

    #[test]
    fn duplicate_counts_are_invisible_to_the_outcome_projection() {
        // The load-bearing property behind `outcomeDigest`: redundant proposal work is not a
        // behavior change.
        let once = AnalysisSet::from_observed([id(&["x"])]);
        let thrice = AnalysisSet::from_observed([id(&["x"]), id(&["x"]), id(&["x"])]);
        assert_eq!(once.to_outcome_value(), thrice.to_outcome_value());
        assert_ne!(once.to_semantic_value(), thrice.to_semantic_value());
    }

    #[test]
    fn a_complete_empty_set_is_representable() {
        // Distinct from an incomplete outcome, which carries no authoritative set at all.
        let empty = AnalysisSet::from_observed([]);
        assert!(empty.is_empty());
        assert_eq!(empty.to_outcome_value(), json!([]));
    }

    #[test]
    fn a_guessed_flip_is_visible_to_the_outcome_projection() {
        // A root that fell out of the lexicon and was fabricated instead is a behavior change, even
        // though the morpheme sequence and category are untouched.
        let found = AnalysisSet::from_annotated([(id(&["x"]), false)]);
        let fabricated = AnalysisSet::from_annotated([(id(&["x"]), true)]);
        assert_eq!(
            found.entries()[0].identity,
            fabricated.entries()[0].identity,
            "identity is unchanged — that is the point"
        );
        assert_ne!(found.to_outcome_value(), fabricated.to_outcome_value());
    }

    #[test]
    fn contains_confirms_the_structured_value() {
        let set = AnalysisSet::from_observed([id(&["x"])]);
        assert!(set.contains(&id(&["x"])));
        assert!(!set.contains(&id(&["y"])));
    }
}
