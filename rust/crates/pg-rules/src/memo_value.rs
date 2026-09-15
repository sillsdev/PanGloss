//! `HC_MEMO_VALUE_STATS=1` diagnostic (env-gated, off by default): task T5(b) --
//! `docs/research/memory-measurement-repair.md`, extended by `docs/research/memo-entry-work-value.md`
//! (work-based value). Answers whether the mrule-memo table's bytes are spent uniformly or have a
//! low-value tail: per stored `AnalysisStateKey`, tracks bytes stored, `results.len()` at insert
//! time, the cascade's own in-progress-set size at insert time (a cheap, already-computed proxy for
//! cascade depth/position -- no new recursion tracking added), how many times that exact key was
//! subsequently looked up and found (a hit, positive or nogood), and now the recompute cost a hit
//! avoids: `subtree_work_inclusive`/`_exclusive` and `descendant_count` (see `enter_subtree`/
//! `exit_subtree`). Measurement only: no policy anywhere reads this table. Mrule-memo only (the
//! table every prior measurement doc found dominant); the template-memo table is not instrumented
//! here.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use pg_memo::AnalysisStateKey;

thread_local! {
    static ENABLED: Cell<Option<bool>> = const { Cell::new(None) };
    static ENTRIES: RefCell<HashMap<AnalysisStateKey, EntryValue>> = RefCell::new(HashMap::new());
    static SUBTREE_STACK: RefCell<Vec<SubtreeFrame>> = RefCell::new(Vec::new());
}

#[derive(Clone, Copy, Debug, Default)]
struct EntryValue {
    bytes: u64,
    results_len: u32,
    depth_at_insert: u32,
    hits: u64,
    subtree_work_inclusive: u64,
    subtree_work_exclusive: u64,
    descendant_count: u32,
}

/// One open `memo_apply_rules` frame's window, keyed by `StepBudget::steps()` ticks (one tick per
/// `apply_one_mrule` attempt -- see `stratum.rs`). `child_ticks`/`descendant_count` accumulate as
/// nested frames close beneath this one, before this frame itself closes.
struct SubtreeFrame {
    tick_start: u64,
    child_ticks: u64,
    descendant_count: u32,
}

/// The recompute cost one stored entry's subtree consumed, read off `StepBudget` ticks at the
/// `enter_subtree`/`exit_subtree` boundary around that entry's own (un-memoized) expansion.
///
/// `inclusive` = ticks from entry to exit of this state's own expansion, INCLUDING every ticked
/// descendant reached along the way (whether or not that descendant itself got stored) -- this is
/// what the state cost the FIRST time it was ever computed, with nothing below it memoized yet.
/// `exclusive` = `inclusive` minus the inclusive cost of every immediate child frame that closed
/// within this one's window -- this is the ticks attributable to this state alone, given that
/// every descendant already has (or would already have, on a re-derivation) its own memo entry.
/// Nested entries double-count under `inclusive` (a stored child's ticks are counted once in its
/// own entry AND again inside every ancestor's `inclusive`); `exclusive` is double-count-free by
/// construction, since sibling windows are disjoint (single-threaded) and a child's ticks are
/// subtracted from its parent exactly once.
#[derive(Clone, Copy, Debug, Default)]
pub struct SubtreeWork {
    pub inclusive: u64,
    pub exclusive: u64,
    /// Memo-able states reached beneath this one before it closed (fresh `memo_apply_rules`
    /// invocations nested in this window, transitively) -- the direct reading of "how many nodes
    /// beneath it," independent of whether each one was actually admitted to the memo table.
    pub descendant_count: u32,
}

/// Open a subtree-work window for the state about to be expanded via `memo_apply_rules_raw`.
/// Call exactly once, immediately before that call, passing `StepBudget::steps()` at that instant.
/// Must be paired with exactly one `exit_subtree` call, even when the store that follows is
/// refused -- the pairing is what keeps nested windows correctly nested and their ticks correctly
/// attributed to the right parent, not just what makes a state get an entry.
pub fn enter_subtree(tick_start: u64) {
    if !enabled() {
        return;
    }
    push_frame(tick_start);
}

/// Close the window opened by the matching `enter_subtree`, passing `StepBudget::steps()` at this
/// instant. Propagates this frame's inclusive ticks and total descendant count into the parent
/// frame (if any) before returning this frame's own figures.
pub fn exit_subtree(tick_end: u64) -> SubtreeWork {
    if !enabled() {
        return SubtreeWork::default();
    }
    pop_frame(tick_end)
}

/// Pure stack push, testable without the env gate (mirrors `percentile_of`).
fn push_frame(tick_start: u64) {
    SUBTREE_STACK.with(|s| {
        s.borrow_mut().push(SubtreeFrame {
            tick_start,
            child_ticks: 0,
            descendant_count: 0,
        })
    });
}

/// Pure stack pop + parent propagation, testable without the env gate (mirrors `percentile_of`).
fn pop_frame(tick_end: u64) -> SubtreeWork {
    let frame = SUBTREE_STACK
        .with(|s| s.borrow_mut().pop())
        .expect("pop_frame called without a matching push_frame");
    let inclusive = tick_end.saturating_sub(frame.tick_start);
    let exclusive = inclusive.saturating_sub(frame.child_ticks);
    let work = SubtreeWork {
        inclusive,
        exclusive,
        descendant_count: frame.descendant_count,
    };
    SUBTREE_STACK.with(|s| {
        if let Some(parent) = s.borrow_mut().last_mut() {
            parent.child_ticks = parent.child_ticks.saturating_add(inclusive);
            parent.descendant_count = parent
                .descendant_count
                .saturating_add(frame.descendant_count + 1);
        }
    });
    work
}

pub fn enabled() -> bool {
    ENABLED.with(|c| {
        if let Some(v) = c.get() {
            return v;
        }
        let v = std::env::var("HC_MEMO_VALUE_STATS").is_ok();
        c.set(Some(v));
        v
    })
}

/// Call once, right after a store this call's own caller already decided to admit (`stratum.rs`'s
/// `memo_apply_rules`, mirroring where `pg_memo::profile::record_insert` is called). `subtree_work`
/// is the `exit_subtree` result from the matching window around this same store's raw expansion.
pub fn record_insert(
    key: &AnalysisStateKey,
    bytes: usize,
    results_len: usize,
    depth_at_insert: usize,
    subtree_work: SubtreeWork,
) {
    if !enabled() {
        return;
    }
    ENTRIES.with(|m| {
        m.borrow_mut().insert(
            key.clone(),
            EntryValue {
                bytes: bytes as u64,
                results_len: results_len as u32,
                depth_at_insert: depth_at_insert as u32,
                hits: 0,
                subtree_work_inclusive: subtree_work.inclusive,
                subtree_work_exclusive: subtree_work.exclusive,
                descendant_count: subtree_work.descendant_count,
            },
        );
    });
}

/// Call once per lookup that finds `key` already stored (positive or nogood -- both avoid
/// re-derivation, so both count as value delivered).
pub fn record_hit(key: &AnalysisStateKey) {
    if !enabled() {
        return;
    }
    ENTRIES.with(|m| {
        if let Some(e) = m.borrow_mut().get_mut(key) {
            e.hits += 1;
        }
    });
}

/// One run's whole per-entry value picture.
pub struct MemoValueSnapshot {
    pub entries: usize,
    pub total_bytes: u64,
    /// Entries that were stored but never subsequently hit -- the "low-value tail" the task asks
    /// about -- and the bytes they hold, as a fraction of `total_bytes`.
    pub zero_hit_entries: usize,
    pub zero_hit_bytes: u64,
    pub mean_hits: f64,
    pub mean_results_len: f64,
    pub mean_depth_at_insert: f64,
    pub mean_subtree_work_inclusive: f64,
    pub mean_subtree_work_exclusive: f64,
    pub mean_descendant_count: f64,
    /// Sum over every entry of `hits * subtree_work_exclusive` -- the aggregate work this run's
    /// memo actually saved (see `docs/research/memo-entry-work-value.md`).
    pub total_work_saved_exclusive: u64,
}

pub fn snapshot() -> MemoValueSnapshot {
    ENTRIES.with(|m| {
        let m = m.borrow();
        let entries = m.len();
        let total_bytes: u64 = m.values().map(|e| e.bytes).sum();
        let zero_hit_entries = m.values().filter(|e| e.hits == 0).count();
        let zero_hit_bytes: u64 = m.values().filter(|e| e.hits == 0).map(|e| e.bytes).sum();
        let hit_total: u64 = m.values().map(|e| e.hits).sum();
        let results_len_total: u64 = m.values().map(|e| e.results_len as u64).sum();
        let depth_total: u64 = m.values().map(|e| e.depth_at_insert as u64).sum();
        let work_incl_total: u64 = m.values().map(|e| e.subtree_work_inclusive).sum();
        let work_excl_total: u64 = m.values().map(|e| e.subtree_work_exclusive).sum();
        let descendant_total: u64 = m.values().map(|e| e.descendant_count as u64).sum();
        let total_work_saved_exclusive: u64 = m
            .values()
            .map(|e| e.hits.saturating_mul(e.subtree_work_exclusive))
            .sum();
        let n = entries.max(1) as f64;
        MemoValueSnapshot {
            entries,
            total_bytes,
            zero_hit_entries,
            zero_hit_bytes,
            mean_hits: hit_total as f64 / n,
            mean_results_len: results_len_total as f64 / n,
            mean_depth_at_insert: depth_total as f64 / n,
            mean_subtree_work_inclusive: work_incl_total as f64 / n,
            mean_subtree_work_exclusive: work_excl_total as f64 / n,
            mean_descendant_count: descendant_total as f64 / n,
            total_work_saved_exclusive,
        }
    })
}

/// One stored entry's raw fields, for offline per-entry analysis (`docs/research/
/// memo-entry-work-value.md`'s tuning curve and joint distribution) that `snapshot()`'s aggregates
/// cannot answer -- e.g. "what share of bytes sits in entries with subtree work <= K".
#[derive(Clone, Copy, Debug)]
pub struct EntryDump {
    pub bytes: u64,
    pub results_len: u32,
    pub depth_at_insert: u32,
    pub hits: u64,
    pub subtree_work_inclusive: u64,
    pub subtree_work_exclusive: u64,
    pub descendant_count: u32,
}

pub fn dump_entries() -> Vec<EntryDump> {
    ENTRIES.with(|m| {
        m.borrow()
            .values()
            .map(|e| EntryDump {
                bytes: e.bytes,
                results_len: e.results_len,
                depth_at_insert: e.depth_at_insert,
                hits: e.hits,
                subtree_work_inclusive: e.subtree_work_inclusive,
                subtree_work_exclusive: e.subtree_work_exclusive,
                descendant_count: e.descendant_count,
            })
            .collect()
    })
}

/// Percentile (0.0-1.0) over `hits / bytes` (value per byte) across every stored entry, sorted
/// ascending -- the low end of this distribution is the tail `snapshot`'s `zero_hit_*` fields
/// already summarize in aggregate; this gives the shape.
pub fn value_per_byte_percentile(p: f64) -> f64 {
    let ratios: Vec<f64> = ENTRIES.with(|m| {
        m.borrow()
            .values()
            .filter(|e| e.bytes > 0)
            .map(|e| e.hits as f64 / e.bytes as f64)
            .collect()
    });
    percentile_of(&ratios, p)
}

/// Pure sorted-percentile helper, testable without the env gate (mirrors `word_stats::percentiles`).
fn percentile_of(samples: &[f64], p: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut v = samples.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).expect("ratios are never NaN"));
    let idx = ((v.len() - 1) as f64 * p).round() as usize;
    v[idx]
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_featstruct::FeatureStruct;
    use pg_grammar::model::StratumId;
    use pg_shape::ShapeBuilder;
    use std::collections::BTreeMap;

    fn key(non_head_count: u32) -> AnalysisStateKey {
        AnalysisStateKey::new(
            ShapeBuilder::new().finish(),
            StratumId(0),
            FeatureStruct::EMPTY,
            FeatureStruct::EMPTY,
            non_head_count,
            BTreeMap::new(),
        )
    }

    #[test]
    fn record_is_a_no_op_when_disabled() {
        if enabled() {
            return; // env var already set by another test in this process; skip rather than false-fail.
        }
        record_insert(&key(0), 100, 3, 2, SubtreeWork::default());
        record_hit(&key(0));
        assert_eq!(snapshot().entries, 0);
    }

    #[test]
    fn percentile_of_picks_sorted_positions() {
        let v = [4.0, 1.0, 3.0, 2.0, 5.0];
        assert_eq!(percentile_of(&v, 0.0), 1.0);
        assert_eq!(percentile_of(&v, 1.0), 5.0);
        assert_eq!(percentile_of(&[], 0.5), 0.0);
    }

    /// Self-verification (repo rule: a counter that reads the same on every sample is a bug until
    /// proven otherwise): a flat leaf (no nested pushes) must have `inclusive == exclusive` and
    /// `descendant_count == 0`; a parent with one child pushed/popped inside its window must come
    /// back with `inclusive > exclusive` (the child's ticks subtracted out) and
    /// `descendant_count == 1` -- i.e. these two shapes are provably NOT the same reading.
    #[test]
    fn subtree_work_differs_between_a_leaf_and_a_parent_with_one_child() {
        // Leaf: push at tick 10, pop at tick 16 -- 6 ticks, nothing nested beneath it.
        push_frame(10);
        let leaf = pop_frame(16);
        assert_eq!(leaf.inclusive, 6, "leaf inclusive = raw tick delta");
        assert_eq!(
            leaf.exclusive, leaf.inclusive,
            "no child pushed inside a leaf's window -- exclusive must equal inclusive"
        );
        assert_eq!(leaf.descendant_count, 0, "a leaf has no descendants");

        // Parent: push at tick 100, one child pushed/popped fully inside (104..109, 5 ticks), then
        // 3 more of the parent's own ticks before it closes at 112. Parent inclusive = 12
        // (100..112); child's 5 ticks must be subtracted out of the parent's exclusive.
        push_frame(100);
        push_frame(104);
        let child = pop_frame(109);
        let parent = pop_frame(112);
        assert_eq!(child.inclusive, 5);
        assert_eq!(parent.inclusive, 12, "parent inclusive = its own full raw tick delta");
        assert_eq!(
            parent.exclusive, 7,
            "parent exclusive = inclusive(12) - child inclusive(5) = 7, not equal to inclusive"
        );
        assert_ne!(
            parent.inclusive, parent.exclusive,
            "nesting must make inclusive and exclusive diverge -- a degenerate reading here would mean the child's ticks were never subtracted"
        );
        assert_eq!(
            parent.descendant_count, 1,
            "exactly one child frame closed inside the parent's window"
        );
    }

    /// Two children (one nested two deep) must roll their descendant counts up correctly: the
    /// grandparent's `descendant_count` counts BOTH the middle child and the leaf beneath it, not
    /// just its immediate child -- proving the rollup is transitive, not a flat immediate-child tally.
    #[test]
    fn descendant_count_rolls_up_transitively_through_two_levels() {
        push_frame(0); // grandparent
        push_frame(1); // middle child
        push_frame(2); // leaf grandchild
        let leaf = pop_frame(3);
        let middle = pop_frame(5);
        let grandparent = pop_frame(6);
        assert_eq!(leaf.descendant_count, 0);
        assert_eq!(middle.descendant_count, 1, "middle has exactly the leaf beneath it");
        assert_eq!(
            grandparent.descendant_count, 2,
            "grandparent must see both the middle child and the leaf beneath it, not just 1"
        );
    }
}
