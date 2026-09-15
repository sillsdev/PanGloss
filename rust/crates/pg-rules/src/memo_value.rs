//! `HC_MEMO_VALUE_STATS=1` diagnostic (env-gated, off by default): task T5(b) --
//! `docs/research/memory-measurement-repair.md`. Answers whether the mrule-memo table's bytes are
//! spent uniformly or have a low-value tail: per stored `AnalysisStateKey`, tracks bytes stored,
//! `results.len()` at insert time, the cascade's own in-progress-set size at insert time (a cheap,
//! already-computed proxy for cascade depth/position -- no new recursion tracking added), and how
//! many times that exact key was subsequently looked up and found (a hit, positive or nogood).
//! Measurement only: no policy anywhere reads this table. Mrule-memo only (the table every prior
//! measurement doc found dominant); the template-memo table is not instrumented here.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use pg_memo::AnalysisStateKey;

thread_local! {
    static ENABLED: Cell<Option<bool>> = const { Cell::new(None) };
    static ENTRIES: RefCell<HashMap<AnalysisStateKey, EntryValue>> = RefCell::new(HashMap::new());
}

#[derive(Clone, Copy, Debug, Default)]
struct EntryValue {
    bytes: u64,
    results_len: u32,
    depth_at_insert: u32,
    hits: u64,
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
/// `memo_apply_rules`, mirroring where `pg_memo::profile::record_insert` is called).
pub fn record_insert(
    key: &AnalysisStateKey,
    bytes: usize,
    results_len: usize,
    depth_at_insert: usize,
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
        let n = entries.max(1) as f64;
        MemoValueSnapshot {
            entries,
            total_bytes,
            zero_hit_entries,
            zero_hit_bytes,
            mean_hits: hit_total as f64 / n,
            mean_results_len: results_len_total as f64 / n,
            mean_depth_at_insert: depth_total as f64 / n,
        }
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
        record_insert(&key(0), 100, 3, 2);
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
}
