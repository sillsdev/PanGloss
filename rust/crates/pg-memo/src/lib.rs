//! Order-invariant analysis-cascade memoization (the #451 memo; plan §1.2, §6.3).
//!
//! Ports three C# types from the parse-opt worktree:
//! - `AnalysisStateKey.cs` — the order-independent identity of an analysis-cascade node.
//! - `AnalysisScope.cs` — the two memo tables + in-flight re-entrancy guard + entry cap.
//! - `MemoEntry` (in `AnalysisScope.cs`) — a memoized subtree (positive replay or nogood).
//!
//! The unordered morphological-rule cascade re-reaches the *same* analysis state via every
//! permutation of the rules that got there — a `k!` walk. This key collapses that: two words with an
//! equal key make identical decisions in every analysis-side rule (each reads only shape + syntactic
//! FS + a per-rule *unapplication count*, never the order), so the second arrival replays the first's
//! stored subtree instead of re-searching (`pg_rules::stratum`'s memoized cascade).
//!
//! ## Deviations from the C# / the crate-stub sketch (flagged per "flag-don't-hack")
//! - **Owned `Shape`/`FeatureStruct`, not interned `ShapeId`/`FsId`.** The stub sketched a ~32-byte
//!   all-`u32` key, but the current `pg_rules::Word` owns its `Shape`/`FeatureStruct` (per-parse
//!   interning is deferred past M6, see `word.rs`), so the key mirrors `WordKey` and clones them. The
//!   `size_of <= 32` guard the stub carried is therefore dropped.
//! - **`rule_counts: BTreeMap`, not a hand-XOR hash.** C# uses a `Dictionary` + a commutative XOR
//!   hash because a `Dictionary` has no canonical order. Rust cannot *derive* `Hash` on a `HashMap`;
//!   a `BTreeMap`'s canonical (sorted) order gives the same order-invariance with a derived `Eq`+
//!   `Hash` that are guaranteed mutually consistent — the idiomatic equivalent of the XOR trick,
//!   without the classic hand-rolled-`Hash`/`Eq`-mismatch hazard.
//! - **Generic over the stored word `W`.** `MemoEntry`/`AnalysisScope` are generic so this crate does
//!   not depend on `pg-rules` (which depends on this crate — a cycle otherwise). `pg-rules`
//!   instantiates `AnalysisScope<Word>`.
//! - **A second in-flight guard for the template table.** C# `AnalysisScope` shows one `InProgress`;
//!   the template battery is a distinct computation over the same key space, so it gets its own guard
//!   (`template_in_progress`). A shared guard would be correctness-neutral (a false hit only forgoes
//!   memoization for one call), but a separate one is cleaner.
#![forbid(unsafe_code)]

use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::BuildHasherDefault;

use pg_featstruct::FeatureStruct;
use pg_grammar::model::{MRuleId, StratumId};
use pg_shape::Shape;

/// Fixed-seed hash map/set: `AnalysisScope`'s tables are read back inside a `--step-cap`-interruptible cascade, so they use the same process-stable hasher as every other accumulator in the pipeline, never a randomly-seeded default.
type HashMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<DefaultHasher>>;
type HashSet<T> = std::collections::HashSet<T, BuildHasherDefault<DefaultHasher>>;

/// Order-independent identity of an analysis-cascade node (C# `AnalysisStateKey`,
/// AnalysisStateKey.cs:29-116).
///
/// Fields are exactly those every analysis-side rule reads (AnalysisStateKey.cs:14-21):
/// `shape` (FST pattern match), `syntactic_fs` (unifiability gate), `realizational_fs`
/// (realizational rules — always empty in v1), `non_head_count` (`MaxStemCount` gate — never the
/// non-heads' *content*), `stratum`, and the per-rule unapplication-count **multiset**.
///
/// Deliberately **excludes** what C# `Word.ValueEquals` includes for result-dedup (the mrule trail as
/// an ordered *sequence* — replaced here by the order-independent `rule_counts` multiset — plus
/// `is_last_applied_rule_final` / `is_partial`, which no analysis rule reads). This exclusion of trail
/// *order* is the order-invariance that collapses the `k!`-walk. It also **includes** the syntactic FS
/// (which `WordKey`/`Word.ValueEquals` deliberately exclude), because analysis rules gate on it.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct AnalysisStateKey {
    shape: Shape,
    stratum: StratumId,
    syntactic_fs: FeatureStruct,
    realizational_fs: FeatureStruct,
    non_head_count: u32,
    /// The per-rule unapplication multiset; a `BTreeMap` so equal multisets built up in different orders compare and hash identically.
    rule_counts: BTreeMap<MRuleId, u32>,
}

impl AnalysisStateKey {
    /// Build a key from a word's already-extracted components (`pg-rules` supplies these from a
    /// `Word`; this crate does not depend on `Word`). `rule_counts` is cloned from the word's
    /// `unapplied_rule_counts`.
    pub fn new(
        shape: Shape,
        stratum: StratumId,
        syntactic_fs: FeatureStruct,
        realizational_fs: FeatureStruct,
        non_head_count: u32,
        rule_counts: BTreeMap<MRuleId, u32>,
    ) -> Self {
        AnalysisStateKey {
            shape,
            stratum,
            syntactic_fs,
            realizational_fs,
            non_head_count,
            rule_counts,
        }
    }
}

/// A memoized analysis-cascade subtree (C# `MemoEntry`, AnalysisScope.cs:75-87).
///
/// `results` empty = the "nogood" case (the subtree proved to yield nothing); non-empty = the
/// positive case, each result replayable onto a differently-ordered arrival at the same
/// `AnalysisStateKey`. `mrule_trail_prefix_length` / `non_head_prefix_length` are the memoized
/// node's own trail/non-head lengths at store time, so a replay knows where its (discarded) prefix
/// ends and the (kept) subtree-local suffix begins (see `Word::replay_onto`). There is no
/// "budget exhausted" flag — this branch has no per-subtree budget, so every stored subtree was
/// explored to completion.
#[derive(Clone, Debug)]
pub struct MemoEntry<W> {
    pub results: Vec<W>,
    pub mrule_trail_prefix_length: usize,
    pub non_head_prefix_length: usize,
}

impl<W> MemoEntry<W> {
    pub fn new(
        results: Vec<W>,
        mrule_trail_prefix_length: usize,
        non_head_prefix_length: usize,
    ) -> Self {
        MemoEntry {
            results,
            mrule_trail_prefix_length,
            non_head_prefix_length,
        }
    }

    /// Whether this is a positive (replayable) entry rather than a nogood.
    pub fn is_positive(&self) -> bool {
        !self.results.is_empty()
    }
}

/// OOM guard: past the cap, keep searching correctly, just stop growing the table; only the hit rate degrades.
const MAX_MEMO_ENTRIES: usize = 100_000;

/// Per-parse cache carrier (C# `AnalysisScope`, AnalysisScope.cs:21-63). One instance per
/// `Morpher::parse_word` call — entries are facts about *a specific parse's* states (a key does not
/// encode the target surface word), so sharing across parses of different words would be unsound (C#
/// AnalysisScope.cs:12-15). Single-threaded within a word, so plain `HashMap`s: the C# concurrency
/// was for the parallel cascade, which has no Rust descendant (plan §7).
pub struct AnalysisScope<W> {
    /// The morphological-rule-cascade memo (nogood + positive), keyed by state (C# `Memo`).
    pub memo: HashMap<AnalysisStateKey, MemoEntry<W>>,
    /// The template-battery memo — a separate table (C# `TemplateMemo`, AnalysisScope.cs:43-53):
    /// it records a different computation over the same key space (a state's one-level template
    /// outputs vs. its mrule subtree); merging them would conflate a "no template outputs" with a
    /// "no mrule results" nogood.
    pub template_memo: HashMap<AnalysisStateKey, MemoEntry<W>>,
    /// Keys currently under mrule expansion on some call stack — the in-flight re-entry guard (C#
    /// `InProgress`, AnalysisScope.cs:56-60). A hit falls through to plain, unmemoized expansion.
    pub in_progress: HashSet<AnalysisStateKey>,
    /// The same guard for the template battery (see the module-level deviation note).
    pub template_in_progress: HashSet<AnalysisStateKey>,
}

impl<W> Default for AnalysisScope<W> {
    fn default() -> Self {
        Self::new()
    }
}

impl<W> AnalysisScope<W> {
    pub fn new() -> Self {
        AnalysisScope {
            memo: HashMap::default(),
            template_memo: HashMap::default(),
            in_progress: HashSet::default(),
            template_in_progress: HashSet::default(),
        }
    }

    /// C# `AnalysisScope.HasMemoCapacity` (AnalysisScope.cs:62): room to add another mrule-memo entry.
    pub fn has_memo_capacity(&self) -> bool {
        self.memo.len() < MAX_MEMO_ENTRIES
    }

    /// Room to add another template-memo entry (same cap discipline, per-table).
    pub fn has_template_capacity(&self) -> bool {
        self.template_memo.len() < MAX_MEMO_ENTRIES
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_featstruct::FeatureStruct;
    use pg_shape::ShapeBuilder;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn shape() -> Shape {
        ShapeBuilder::new().finish()
    }

    /// Fold a rule-unapplication sequence into the count multiset, as the cascade does via `Word::record_unapplication`.
    fn counts_from(seq: &[u32]) -> BTreeMap<MRuleId, u32> {
        let mut m = BTreeMap::new();
        for &id in seq {
            *m.entry(MRuleId(id)).or_insert(0) += 1;
        }
        m
    }

    fn key_with(counts: BTreeMap<MRuleId, u32>, non_head_count: u32) -> AnalysisStateKey {
        AnalysisStateKey::new(
            shape(),
            StratumId(0),
            FeatureStruct::EMPTY,
            FeatureStruct::EMPTY,
            non_head_count,
            counts,
        )
    }

    fn hash_of(k: &AnalysisStateKey) -> u64 {
        let mut h = DefaultHasher::new();
        k.hash(&mut h);
        h.finish()
    }

    #[test]
    fn key_is_order_invariant_over_the_rule_multiset() {
        // The SAME multiset {r0:2, r1:1} reached via three different unapplication orders → one key.
        let a = key_with(counts_from(&[0, 1, 0]), 0);
        let b = key_with(counts_from(&[0, 0, 1]), 0);
        let c = key_with(counts_from(&[1, 0, 0]), 0);
        assert_eq!(a, b);
        assert_eq!(b, c);
        // Equal keys must hash equally (they derive Hash together).
        assert_eq!(hash_of(&a), hash_of(&b));
        assert_eq!(hash_of(&b), hash_of(&c));
    }

    #[test]
    fn key_distinguishes_different_multisets() {
        let a = key_with(counts_from(&[0, 1, 0]), 0); // {0:2, 1:1}
        let b = key_with(counts_from(&[0, 1, 1]), 0); // {0:1, 1:2}
        assert_ne!(a, b);
    }

    #[test]
    fn key_distinguishes_non_head_count() {
        assert_ne!(key_with(BTreeMap::new(), 0), key_with(BTreeMap::new(), 1));
    }

    #[test]
    fn memo_entry_positive_vs_nogood() {
        let positive = MemoEntry::new(vec![1u32, 2, 3], 2, 1);
        assert!(positive.is_positive());
        let nogood: MemoEntry<u32> = MemoEntry::new(Vec::new(), 0, 0);
        assert!(!nogood.is_positive());
    }

    #[test]
    fn in_progress_guard_blocks_reentry() {
        // Synthetic: real analysis never re-enters, so the fall-through guard is exercised directly.
        let mut scope: AnalysisScope<u32> = AnalysisScope::new();
        let k = key_with(BTreeMap::new(), 0);
        assert!(scope.in_progress.insert(k.clone()), "first entry is fresh");
        assert!(
            !scope.in_progress.insert(k.clone()),
            "re-entry sees the key in-flight → cascade falls through to raw expansion"
        );
        scope.in_progress.remove(&k);
        assert!(scope.in_progress.insert(k), "after clear, fresh again");
    }

    #[test]
    fn capacity_reports_room() {
        let scope: AnalysisScope<u32> = AnalysisScope::new();
        assert!(scope.has_memo_capacity());
        assert!(scope.has_template_capacity());
    }
}
