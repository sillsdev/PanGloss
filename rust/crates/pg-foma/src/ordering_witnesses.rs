//! Bounded, deterministic retention of the words that witness each attested rule ordering.
//!
//! A corpus run over analyzed texts answers "which orderings of these two rules actually occur",
//! and the interesting answer is lopsided: 39,999 words take one ordering and one word takes the
//! other. That single contrary word is the most valuable datum in the row — it is what tells a
//! reader whether the minority is a real alternation or a mis-analyzed text — and it **cannot be
//! recovered later**: `pg_parse::WordAnalysis` carries morphemes, POS, features and provenance but
//! no rule trace, so derivation order is not part of any stored analysis. Either the pass that saw
//! it keeps a locator, or the fact is gone until the whole corpus is re-run with tracing on.
//!
//! # What is kept, and what is not
//! Traces are not kept. A trace is built, read and dropped per word; what survives is a
//! [`WitnessId`] — the caller's own word locator, an integer. So the cost of this whole mechanism
//! is a few thousand integers, not a forest of derivation trees.
//!
//! # Counts are exact; samples are bounded
//! [`OrderingWitnesses::count`] is the statistic and [`OrderingWitnesses::witnesses`] is the
//! evidence, and one is never derived from the other. Deriving the count from the retained sample
//! would silently turn "how many words did this" into "how many we chose to keep".
//!
//! # Why reservoir sampling rather than "keep the first N"
//! A corpus is ordered by document, so the first N witnesses of an ordering tend to come from one
//! text — which is precisely the question a reader is asking (is this systematic, or one bad
//! passage?), answered wrongly by construction. Reservoir sampling is correct at BOTH ends with one
//! mechanism: below the cap it keeps everything, so a lone 1-in-40,000 witness is retained with
//! certainty; above it, the sample is spread across the whole corpus rather than clustered in its
//! prefix.
//!
//! # Determinism
//! The same corpus must produce the same witnesses on every run, or a report cannot be diffed
//! between grammar revisions and a golden test cannot exist. Each ordering key seeds its own
//! `splitmix64` stream from an unseeded FNV-1a of the key (the same choice, for the same reason, as
//! `crate::plan`'s content addressing: `DefaultHasher`'s `RandomState` is per-process and would make
//! every run disagree). Keys are therefore independent of each other and of observation
//! interleaving across keys.

use std::collections::BTreeMap;

/// A caller-supplied locator for one word occurrence.
///
/// This is deliberately the CALLER's identifier, not one minted here: `machine/conformance/
/// PROTOCOL.md` § 1 already defines `idx`, the 0-based line index into the batch word list, and
/// emits it as an output column. A caller that thinks in FieldWorks occurrence GUIDs keeps its own
/// mapping to line numbers; nothing in this crate needs to understand that object model.
pub type WitnessId = u64;

/// Default number of witnesses retained per ordering. Configurable via
/// [`OrderingWitnesses::with_cap`].
///
/// Ten rather than one because a single witness cannot distinguish a systematic rare pattern from a
/// one-off data error — around three to five is where shared structure (same text, same lexeme,
/// same POS) becomes visible, and ten leaves headroom. Larger caps are cheap (a witness is an
/// integer) but answer no further question.
pub const DEFAULT_WITNESS_CAP: usize = 10;

/// Exact counts plus a bounded, deterministic witness sample, per ordering key.
///
/// The cap applies PER KEY, which is what makes a single streaming pass sufficient: while reading a
/// corpus you do not yet know which ordering will turn out to be the rare one, and keeping up to
/// `cap` of each means the answer is retained whichever way the split falls.
#[derive(Debug, Clone)]
pub struct OrderingWitnesses {
    cap: usize,
    counts: BTreeMap<String, u64>,
    kept: BTreeMap<String, Vec<WitnessId>>,
}

impl Default for OrderingWitnesses {
    fn default() -> Self {
        Self::with_cap(DEFAULT_WITNESS_CAP)
    }
}

impl OrderingWitnesses {
    /// A collector retaining up to `cap` witnesses per ordering. A `cap` of 0 keeps counts only.
    pub fn with_cap(cap: usize) -> Self {
        OrderingWitnesses {
            cap,
            counts: BTreeMap::new(),
            kept: BTreeMap::new(),
        }
    }

    pub fn cap(&self) -> usize {
        self.cap
    }

    /// Records one occurrence of `ordering`, witnessed by the word at `witness`.
    pub fn observe(&mut self, ordering: &str, witness: WitnessId) {
        let seen = self.counts.entry(ordering.to_string()).or_insert(0);
        *seen += 1;
        let seen = *seen;

        if self.cap == 0 {
            return;
        }
        let kept = self.kept.entry(ordering.to_string()).or_default();
        if kept.len() < self.cap {
            kept.push(witness);
            return;
        }
        // Replace a uniform slot with probability cap/seen -- what spreads the sample.
        let draw = stream_value(ordering, seen) % seen;
        if (draw as usize) < self.cap {
            kept[draw as usize] = witness;
        }
    }

    /// Exact number of occurrences of `ordering` — never the size of the retained sample.
    pub fn count(&self, ordering: &str) -> u64 {
        self.counts.get(ordering).copied().unwrap_or(0)
    }

    /// The retained witnesses for `ordering`, in retention order.
    pub fn witnesses(&self, ordering: &str) -> &[WitnessId] {
        self.kept.get(ordering).map_or(&[], Vec::as_slice)
    }

    /// Every ordering observed, in sorted order.
    pub fn orderings(&self) -> Vec<&str> {
        self.counts.keys().map(String::as_str).collect()
    }

    /// Orderings attested strictly fewer than `threshold` times, rarest first — the rows worth
    /// reading. Ties break on the key so the result is a stable diff between runs.
    pub fn minority_orderings(&self, threshold: u64) -> Vec<(&str, u64)> {
        let mut rows: Vec<(&str, u64)> = self
            .counts
            .iter()
            .filter(|(_, &count)| count < threshold)
            .map(|(key, &count)| (key.as_str(), count))
            .collect();
        rows.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)));
        rows
    }
}

/// FNV-1a of the key, advanced `step` times through `splitmix64`; unseeded so runs agree.
fn stream_value(key: &str, step: u64) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET_BASIS;
    for &byte in key.as_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    splitmix64(hash.wrapping_add(step))
}

/// Same mixer as `crate::recipe_space`'s, which is private to that module.
fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut mixed = value;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^ (mixed >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "prA<prB";
    const B: &str = "prB<prA";

    #[test]
    fn the_lone_contrary_witness_is_kept_with_certainty() {
        // Falsify by sampling below the cap: the lone witness must never be dropped.
        let mut w = OrderingWitnesses::default();
        for idx in 0..39_999 {
            w.observe(A, idx);
        }
        w.observe(B, 28_413);

        assert_eq!(w.count(A), 39_999);
        assert_eq!(w.count(B), 1);
        assert_eq!(
            w.witnesses(B),
            &[28_413],
            "the single minority witness must survive"
        );
    }

    #[test]
    fn counts_are_exact_and_independent_of_the_cap() {
        for cap in [0usize, 1, 3, DEFAULT_WITNESS_CAP, 64] {
            let mut w = OrderingWitnesses::with_cap(cap);
            for idx in 0..1_000 {
                w.observe(A, idx);
            }
            assert_eq!(w.count(A), 1_000, "cap {cap} must not change the count");
            assert!(
                w.witnesses(A).len() <= cap,
                "cap {cap} must bound the sample"
            );
        }
    }

    #[test]
    fn everything_is_kept_below_the_cap_and_nothing_beyond_it() {
        let mut w = OrderingWitnesses::with_cap(10);
        for idx in 0..7 {
            w.observe(A, idx);
        }
        assert_eq!(w.witnesses(A), &[0, 1, 2, 3, 4, 5, 6]);

        for idx in 7..100 {
            w.observe(A, idx);
        }
        assert_eq!(w.witnesses(A).len(), 10);
    }

    #[test]
    fn a_zero_cap_keeps_counts_and_no_witnesses() {
        let mut w = OrderingWitnesses::with_cap(0);
        w.observe(A, 1);
        w.observe(A, 2);
        assert_eq!(w.count(A), 2);
        assert!(w.witnesses(A).is_empty());
    }

    #[test]
    fn the_same_corpus_produces_the_same_witnesses_every_run() {
        // Without this a report cannot be diffed between grammar revisions and no golden can exist.
        let run = || {
            let mut w = OrderingWitnesses::default();
            for idx in 0..5_000 {
                w.observe(A, idx);
            }
            w.witnesses(A).to_vec()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn the_sample_is_spread_across_the_corpus_not_stuck_in_its_prefix() {
        // A prefix sample would answer "which text came first", not "is this systematic".
        let mut w = OrderingWitnesses::default();
        for idx in 0..40_000 {
            w.observe(A, idx);
        }
        let kept = w.witnesses(A);
        assert_eq!(kept.len(), DEFAULT_WITNESS_CAP);
        assert!(
            kept.iter().any(|&idx| idx > 20_000),
            "a prefix-only sample would fail this: {kept:?}"
        );
    }

    #[test]
    fn keys_are_independent_of_each_other_and_of_interleaving() {
        let mut apart = OrderingWitnesses::default();
        for idx in 0..500 {
            apart.observe(A, idx);
        }
        for idx in 0..500 {
            apart.observe(B, idx);
        }

        let mut interleaved = OrderingWitnesses::default();
        for idx in 0..500 {
            interleaved.observe(A, idx);
            interleaved.observe(B, idx);
        }

        assert_eq!(apart.witnesses(A), interleaved.witnesses(A));
        assert_eq!(apart.witnesses(B), interleaved.witnesses(B));
    }

    #[test]
    fn minority_orderings_lists_the_rare_rows_rarest_first() {
        let mut w = OrderingWitnesses::default();
        for idx in 0..39_999 {
            w.observe(A, idx);
        }
        w.observe(B, 28_413);
        w.observe("prC<prD", 7);
        w.observe("prC<prD", 9);

        assert_eq!(w.minority_orderings(100), vec![(B, 1), ("prC<prD", 2)]);
        assert!(w.minority_orderings(1).is_empty());
    }
}
