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
use pg_grammar::model::{AllomorphId, MRuleId, MorphemeId, StratumId};
use pg_shape::Shape;

/// Fixed-seed hash map/set: `AnalysisScope`'s tables are read back inside a `--step-cap`-interruptible cascade, so they use the same process-stable hasher as every other accumulator in the pipeline, never a randomly-seeded default.
type HashMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<DefaultHasher>>;
type HashSet<T> = std::collections::HashSet<T, BuildHasherDefault<DefaultHasher>>;

/// The source-bearing morphology history that distinguishes equal-shaped analysis arrivals.
///
/// `status` is an opaque `pg-rules::MorphStatus` discriminant at this crate boundary. The key
/// intentionally omits procedural fields such as `passed_over`, which do not identify source
/// morphology and would prevent safe sharing of otherwise identical arrivals.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct MorphHistoryKey {
    pub allomorph: AllomorphId,
    pub morpheme: MorphemeId,
    pub order: u32,
    pub status: u8,
    pub runtime_identity: Option<String>,
}

impl MorphHistoryKey {
    /// Build one source-bearing morphology-history component for an analysis-state key.
    pub fn new(
        allomorph: AllomorphId,
        morpheme: MorphemeId,
        order: u32,
        status: u8,
        runtime_identity: Option<String>,
    ) -> Self {
        Self {
            allomorph,
            morpheme,
            order,
            status,
            runtime_identity,
        }
    }
}

/// Order-independent identity of an analysis-cascade node (C# `AnalysisStateKey`,
/// AnalysisStateKey.cs:29-116).
///
/// Fields include every analysis-side rule input plus the source-bearing morphology history needed
/// to make positive replay preserve the selected allomorph and MSA.
///
/// Deliberately **excludes** procedural fields such as `passed_over` and the mrule trail's order.
/// The order-independent `rule_counts` multiset still collapses the `k!` walk, while morphology
/// history prevents two source-distinct arrivals from sharing a replay.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct AnalysisStateKey {
    shape: Shape,
    stratum: StratumId,
    syntactic_fs: FeatureStruct,
    realizational_fs: FeatureStruct,
    non_head_count: u32,
    /// Per-rule unapplication multiset; sorted keys make order-independent arrivals compare alike.
    rule_counts: BTreeMap<MRuleId, u32>,
    /// Source-bearing history prevents equal shapes with different trails sharing positive replays.
    morph_history: Vec<MorphHistoryKey>,
    /// Final-template interleaving state, opaque at this crate boundary to avoid a dependency cycle.
    state: u8,
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
        Self::new_with_state(
            shape,
            stratum,
            syntactic_fs,
            realizational_fs,
            non_head_count,
            rule_counts,
            0,
        )
    }

    /// Build a key including the final-template interleaving state. The state is intentionally
    /// opaque here; pg-rules owns its meaning and supplies its stable `repr(u8)` value.
    pub fn new_with_state(
        shape: Shape,
        stratum: StratumId,
        syntactic_fs: FeatureStruct,
        realizational_fs: FeatureStruct,
        non_head_count: u32,
        rule_counts: BTreeMap<MRuleId, u32>,
        state: u8,
    ) -> Self {
        Self::new_with_state_and_morph_history(
            shape,
            stratum,
            syntactic_fs,
            realizational_fs,
            non_head_count,
            rule_counts,
            state,
            Vec::new(),
        )
    }

    /// Build a key including final-template state and source-bearing morphology history.
    pub fn new_with_state_and_morph_history(
        shape: Shape,
        stratum: StratumId,
        syntactic_fs: FeatureStruct,
        realizational_fs: FeatureStruct,
        non_head_count: u32,
        rule_counts: BTreeMap<MRuleId, u32>,
        state: u8,
        morph_history: Vec<MorphHistoryKey>,
    ) -> Self {
        AnalysisStateKey {
            shape,
            stratum,
            syntactic_fs,
            realizational_fs,
            non_head_count,
            rule_counts,
            morph_history,
            state,
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

/// Retained-word budget shared across both tables -- the load-bearing cap, since `MAX_MEMO_ENTRIES` bounds entry count only and `MemoEntry::results` is itself unbounded (C# `AnalysisScope.MaxMemoWords`, `AnalysisScope.cs:27-30,97-108`).
const MAX_MEMO_WORDS: usize = 1_000_000;

/// A permanent diagnostic, near-zero cost when unread (thread-local `Cell` adds at each memo touch;
/// the per-insert size walk in `record_insert_size` is skipped entirely unless `enabled()` is true).
/// Read via `pg_memo::profile::snapshot()`, gated on `HC_MEMO_STATS=1` in `pg-cli`. Counts both
/// `AnalysisScope` tables (`memo`, `template_memo`) so a single word's memo effectiveness can be
/// read off at parse end without threading a collector through the cascade.
pub mod profile {
    use std::cell::Cell;

    thread_local! {
        static ENABLED: Cell<Option<bool>> = const { Cell::new(None) };

        static MEMO_LOOKUPS: Cell<u64> = const { Cell::new(0) };
        static MEMO_HITS_POSITIVE: Cell<u64> = const { Cell::new(0) };
        static MEMO_HITS_NOGOOD: Cell<u64> = const { Cell::new(0) };
        static MEMO_INSERTS: Cell<u64> = const { Cell::new(0) };
        static MEMO_INSERT_REFUSED: Cell<u64> = const { Cell::new(0) };
        static MEMO_INSERT_REFUSED_ENTRIES: Cell<u64> = const { Cell::new(0) };
        static MEMO_INSERT_REFUSED_WORDS: Cell<u64> = const { Cell::new(0) };
        static MEMO_FALLTHROUGH: Cell<u64> = const { Cell::new(0) };
        static MEMO_MAX_IN_PROGRESS: Cell<u64> = const { Cell::new(0) };

        static TPL_LOOKUPS: Cell<u64> = const { Cell::new(0) };
        static TPL_HITS_POSITIVE: Cell<u64> = const { Cell::new(0) };
        static TPL_HITS_NOGOOD: Cell<u64> = const { Cell::new(0) };
        static TPL_INSERTS: Cell<u64> = const { Cell::new(0) };
        static TPL_INSERT_REFUSED: Cell<u64> = const { Cell::new(0) };
        static TPL_INSERT_REFUSED_ENTRIES: Cell<u64> = const { Cell::new(0) };
        static TPL_INSERT_REFUSED_WORDS: Cell<u64> = const { Cell::new(0) };
        static TPL_FALLTHROUGH: Cell<u64> = const { Cell::new(0) };
        static TPL_MAX_IN_PROGRESS: Cell<u64> = const { Cell::new(0) };

        // Size-at-insert samples, mrule memo only.
        static INSERT_SAMPLES: Cell<u64> = const { Cell::new(0) };
        static INSERT_RESULTS_LEN_TOTAL: Cell<u64> = const { Cell::new(0) };
        static INSERT_RESULTS_LEN_MAX: Cell<u64> = const { Cell::new(0) };
        static INSERT_WORDS_TOTAL: Cell<u64> = const { Cell::new(0) };
        static INSERT_WORDS_MAX: Cell<u64> = const { Cell::new(0) };
        static INSERT_SHAPE_SEG_TOTAL: Cell<u64> = const { Cell::new(0) };
        static INSERT_SYNFS_TOTAL: Cell<u64> = const { Cell::new(0) };
        static INSERT_REALFS_TOTAL: Cell<u64> = const { Cell::new(0) };
        static INSERT_MORPHS_TOTAL: Cell<u64> = const { Cell::new(0) };

        // Clones performed materializing a memo hit's replayed words (mrule-memo path only).
        static REPLAY_CLONES: Cell<u64> = const { Cell::new(0) };
    }

    /// Cached `HC_MEMO_STATS` read (one env lookup per thread, not per call). Callers use this to
    /// skip the O(subtree) `record_insert_size` walk entirely when the diagnostic is off.
    pub fn enabled() -> bool {
        ENABLED.with(|c| {
            if let Some(v) = c.get() {
                return v;
            }
            let v = std::env::var("HC_MEMO_STATS").is_ok();
            c.set(Some(v));
            v
        })
    }

    pub fn record_lookup(is_template: bool) {
        if is_template {
            TPL_LOOKUPS.with(|c| c.set(c.get() + 1));
        } else {
            MEMO_LOOKUPS.with(|c| c.set(c.get() + 1));
        }
    }

    pub fn record_hit(is_template: bool, positive: bool) {
        match (is_template, positive) {
            (false, true) => MEMO_HITS_POSITIVE.with(|c| c.set(c.get() + 1)),
            (false, false) => MEMO_HITS_NOGOOD.with(|c| c.set(c.get() + 1)),
            (true, true) => TPL_HITS_POSITIVE.with(|c| c.set(c.get() + 1)),
            (true, false) => TPL_HITS_NOGOOD.with(|c| c.set(c.get() + 1)),
        }
    }

    pub fn record_insert(is_template: bool, refused: bool) {
        match (is_template, refused) {
            (false, false) => MEMO_INSERTS.with(|c| c.set(c.get() + 1)),
            (false, true) => MEMO_INSERT_REFUSED.with(|c| c.set(c.get() + 1)),
            (true, false) => TPL_INSERTS.with(|c| c.set(c.get() + 1)),
            (true, true) => TPL_INSERT_REFUSED.with(|c| c.set(c.get() + 1)),
        }
    }

    /// Classifies a refusal already recorded by `record_insert(_, true)` by which cap bound; a
    /// refusal may be counted under more than one reason (e.g. both caps exhausted at once).
    pub fn record_insert_refused_reason(is_template: bool, by_entry_cap: bool, by_word_cap: bool) {
        match is_template {
            false => {
                if by_entry_cap {
                    MEMO_INSERT_REFUSED_ENTRIES.with(|c| c.set(c.get() + 1));
                }
                if by_word_cap {
                    MEMO_INSERT_REFUSED_WORDS.with(|c| c.set(c.get() + 1));
                }
            }
            true => {
                if by_entry_cap {
                    TPL_INSERT_REFUSED_ENTRIES.with(|c| c.set(c.get() + 1));
                }
                if by_word_cap {
                    TPL_INSERT_REFUSED_WORDS.with(|c| c.set(c.get() + 1));
                }
            }
        }
    }

    pub fn record_fallthrough(is_template: bool) {
        if is_template {
            TPL_FALLTHROUGH.with(|c| c.set(c.get() + 1));
        } else {
            MEMO_FALLTHROUGH.with(|c| c.set(c.get() + 1));
        }
    }

    /// `depth` is the in-progress set's size right after the fresh key was inserted (the caller's
    /// own live call-stack depth for that table at this instant).
    pub fn record_in_progress_depth(is_template: bool, depth: usize) {
        let depth = depth as u64;
        if is_template {
            TPL_MAX_IN_PROGRESS.with(|c| c.set(c.get().max(depth)));
        } else {
            MEMO_MAX_IN_PROGRESS.with(|c| c.set(c.get().max(depth)));
        }
    }

    /// `results` is one memo entry's stored `Vec<W>` at insert time; the caller (`pg-rules`, which
    /// owns the concrete `Word` type) has already walked it into these plain counts.
    pub fn record_insert_size(
        results_len: usize,
        total_words: usize,
        shape_segments: usize,
        syn_feats: usize,
        real_feats: usize,
        morphs: usize,
    ) {
        INSERT_SAMPLES.with(|c| c.set(c.get() + 1));
        INSERT_RESULTS_LEN_TOTAL.with(|c| c.set(c.get() + results_len as u64));
        INSERT_RESULTS_LEN_MAX.with(|c| c.set(c.get().max(results_len as u64)));
        INSERT_WORDS_TOTAL.with(|c| c.set(c.get() + total_words as u64));
        INSERT_WORDS_MAX.with(|c| c.set(c.get().max(total_words as u64)));
        INSERT_SHAPE_SEG_TOTAL.with(|c| c.set(c.get() + shape_segments as u64));
        INSERT_SYNFS_TOTAL.with(|c| c.set(c.get() + syn_feats as u64));
        INSERT_REALFS_TOTAL.with(|c| c.set(c.get() + real_feats as u64));
        INSERT_MORPHS_TOTAL.with(|c| c.set(c.get() + morphs as u64));
    }

    /// One clone performed while materializing a memo hit (`Word::replay_onto`'s own `self.clone()`,
    /// or a caller's clone of a replayed result into its output accumulator). Counts clones, not
    /// replayed words, so a caller doing two clones per word shows up as double the one-clone case.
    pub fn record_replay_clone() {
        REPLAY_CLONES.with(|c| c.set(c.get() + 1));
    }

    /// One word's whole cumulative memo picture -- snapshot only, never reset.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct MemoProfileSnapshot {
        pub memo_lookups: u64,
        pub memo_hits_positive: u64,
        pub memo_hits_nogood: u64,
        pub memo_inserts: u64,
        pub memo_insert_refused: u64,
        pub memo_insert_refused_entries: u64,
        pub memo_insert_refused_words: u64,
        pub memo_fallthrough: u64,
        pub memo_max_in_progress: u64,

        pub tpl_lookups: u64,
        pub tpl_hits_positive: u64,
        pub tpl_hits_nogood: u64,
        pub tpl_inserts: u64,
        pub tpl_insert_refused: u64,
        pub tpl_insert_refused_entries: u64,
        pub tpl_insert_refused_words: u64,
        pub tpl_fallthrough: u64,
        pub tpl_max_in_progress: u64,

        pub insert_samples: u64,
        pub insert_results_len_total: u64,
        pub insert_results_len_max: u64,
        pub insert_words_total: u64,
        pub insert_words_max: u64,
        pub insert_shape_seg_total: u64,
        pub insert_synfs_total: u64,
        pub insert_realfs_total: u64,
        pub insert_morphs_total: u64,

        pub replay_clones: u64,
    }

    pub fn snapshot() -> MemoProfileSnapshot {
        MemoProfileSnapshot {
            memo_lookups: MEMO_LOOKUPS.with(|c| c.get()),
            memo_hits_positive: MEMO_HITS_POSITIVE.with(|c| c.get()),
            memo_hits_nogood: MEMO_HITS_NOGOOD.with(|c| c.get()),
            memo_inserts: MEMO_INSERTS.with(|c| c.get()),
            memo_insert_refused: MEMO_INSERT_REFUSED.with(|c| c.get()),
            memo_insert_refused_entries: MEMO_INSERT_REFUSED_ENTRIES.with(|c| c.get()),
            memo_insert_refused_words: MEMO_INSERT_REFUSED_WORDS.with(|c| c.get()),
            memo_fallthrough: MEMO_FALLTHROUGH.with(|c| c.get()),
            memo_max_in_progress: MEMO_MAX_IN_PROGRESS.with(|c| c.get()),

            tpl_lookups: TPL_LOOKUPS.with(|c| c.get()),
            tpl_hits_positive: TPL_HITS_POSITIVE.with(|c| c.get()),
            tpl_hits_nogood: TPL_HITS_NOGOOD.with(|c| c.get()),
            tpl_inserts: TPL_INSERTS.with(|c| c.get()),
            tpl_insert_refused: TPL_INSERT_REFUSED.with(|c| c.get()),
            tpl_insert_refused_entries: TPL_INSERT_REFUSED_ENTRIES.with(|c| c.get()),
            tpl_insert_refused_words: TPL_INSERT_REFUSED_WORDS.with(|c| c.get()),
            tpl_fallthrough: TPL_FALLTHROUGH.with(|c| c.get()),
            tpl_max_in_progress: TPL_MAX_IN_PROGRESS.with(|c| c.get()),

            insert_samples: INSERT_SAMPLES.with(|c| c.get()),
            insert_results_len_total: INSERT_RESULTS_LEN_TOTAL.with(|c| c.get()),
            insert_results_len_max: INSERT_RESULTS_LEN_MAX.with(|c| c.get()),
            insert_words_total: INSERT_WORDS_TOTAL.with(|c| c.get()),
            insert_words_max: INSERT_WORDS_MAX.with(|c| c.get()),
            insert_shape_seg_total: INSERT_SHAPE_SEG_TOTAL.with(|c| c.get()),
            insert_synfs_total: INSERT_SYNFS_TOTAL.with(|c| c.get()),
            insert_realfs_total: INSERT_REALFS_TOTAL.with(|c| c.get()),
            insert_morphs_total: INSERT_MORPHS_TOTAL.with(|c| c.get()),

            replay_clones: REPLAY_CLONES.with(|c| c.get()),
        }
    }
}

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
    /// Words retained across both tables so far, against the shared `MAX_MEMO_WORDS` budget (C# `_storedWordCount`).
    stored_words: usize,
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
            stored_words: 0,
        }
    }

    /// C# `AnalysisScope.HasMemoCapacity` (AnalysisScope.cs:62), extended with the retained-word
    /// budget: room to add another mrule-memo entry storing `results_len` words.
    pub fn has_memo_capacity(&self, results_len: usize) -> bool {
        self.memo.len() < MAX_MEMO_ENTRIES
            && self.stored_words.saturating_add(results_len) <= MAX_MEMO_WORDS
    }

    /// Room to add another template-memo entry storing `results_len` words (same cap discipline,
    /// against the same shared `stored_words` budget as `has_memo_capacity`).
    pub fn has_template_capacity(&self, results_len: usize) -> bool {
        self.template_memo.len() < MAX_MEMO_ENTRIES
            && self.stored_words.saturating_add(results_len) <= MAX_MEMO_WORDS
    }

    /// Record `results_len` words as retained against the shared word budget. Call only once, right
    /// after a store that `has_memo_capacity`/`has_template_capacity` already admitted.
    pub fn record_stored_words(&mut self, results_len: usize) {
        self.stored_words += results_len;
    }

    /// The current retained-word count, for diagnostics (`MEMOPROF`'s `stored_words`).
    pub fn stored_words(&self) -> usize {
        self.stored_words
    }

    /// Diagnostic-only: whether the mrule-memo entry cap alone is exhausted, so a caller classifying
    /// a refusal from `has_memo_capacity` can report which cap actually bound.
    pub fn memo_entries_at_cap(&self) -> bool {
        self.memo.len() >= MAX_MEMO_ENTRIES
    }

    /// Diagnostic-only template-memo analog of `memo_entries_at_cap`.
    pub fn template_entries_at_cap(&self) -> bool {
        self.template_memo.len() >= MAX_MEMO_ENTRIES
    }

    /// Diagnostic-only: whether storing `results_len` more words would exceed the shared word
    /// budget, independent of entry-count capacity.
    pub fn would_exceed_word_budget(&self, results_len: usize) -> bool {
        self.stored_words.saturating_add(results_len) > MAX_MEMO_WORDS
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
    fn key_distinguishes_source_morphology_history() {
        let history_a = vec![MorphHistoryKey::new(
            AllomorphId(1),
            MorphemeId(2),
            0,
            0,
            None,
        )];
        let history_b = vec![MorphHistoryKey::new(
            AllomorphId(3),
            MorphemeId(2),
            0,
            0,
            None,
        )];
        let a = AnalysisStateKey::new_with_state_and_morph_history(
            shape(),
            StratumId(0),
            FeatureStruct::EMPTY,
            FeatureStruct::EMPTY,
            0,
            BTreeMap::new(),
            0,
            history_a,
        );
        let b = AnalysisStateKey::new_with_state_and_morph_history(
            shape(),
            StratumId(0),
            FeatureStruct::EMPTY,
            FeatureStruct::EMPTY,
            0,
            BTreeMap::new(),
            0,
            history_b,
        );
        assert_ne!(a, b);
    }

    #[test]
    fn key_distinguishes_final_template_state() {
        let a = AnalysisStateKey::new_with_state(
            shape(),
            StratumId(0),
            FeatureStruct::EMPTY,
            FeatureStruct::EMPTY,
            0,
            BTreeMap::new(),
            0,
        );
        let b = AnalysisStateKey::new_with_state(
            shape(),
            StratumId(0),
            FeatureStruct::EMPTY,
            FeatureStruct::EMPTY,
            0,
            BTreeMap::new(),
            1,
        );
        assert_ne!(a, b);
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
        assert!(scope.has_memo_capacity(0));
        assert!(scope.has_template_capacity(0));
    }

    #[test]
    fn word_budget_refuses_a_store_past_the_cap_but_keeps_serving_existing_hits() {
        let mut scope: AnalysisScope<u32> = AnalysisScope::new();
        let stored_key = key_with(counts_from(&[0]), 0);
        scope
            .memo
            .insert(stored_key.clone(), MemoEntry::new(vec![1u32, 2, 3], 0, 0));
        // Only 2 words of the shared budget remain.
        scope.stored_words = MAX_MEMO_WORDS - 2;

        let would_be_key = key_with(counts_from(&[1]), 0);
        assert!(
            !scope.has_memo_capacity(3),
            "3 requested words > 2 words of remaining budget"
        );
        assert!(
            scope.has_memo_capacity(2),
            "exactly the remaining budget must still be admitted"
        );
        assert!(
            !scope.has_template_capacity(3),
            "the word budget is shared across both tables"
        );

        // A refused store must leave the already-stored entry servable.
        assert!(scope.memo.get(&stored_key).is_some());
        assert!(scope.memo.get(&would_be_key).is_none());
    }
}
