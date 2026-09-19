//! `HC_WORD_STATS=1` diagnostic (env-gated, off by default): attributes the live search
//! frontier's retained bytes to `Word`'s own fields, checked against
//! `pg-cli`'s allocator-level `HC_ALLOC_STATS=1` ground truth (`docs/research/word-memory-trace.md`).
//!
//! Recorded, mirroring `crate::stratum::frontier_profile`'s existing shape:
//! - [`record_live_words`] is called once per completed stratum pass (`crate::stratum::analyze`)
//!   with that pass's own durable `words` accumulator — the only point that set exists before it
//!   is either consumed by the next stratum or dropped. "Peak" here means the largest such
//!   snapshot seen across the whole parse, not a continuously-sampled allocator peak.
//!
//! Zero cost when unset: [`enabled`] caches one env read per thread, and every record function
//! checks it before doing any work.

use std::cell::{Cell, RefCell};

use crate::word::{estimate_word_bytes_breakdown, WordByteBreakdown};
use crate::Word;

thread_local! {
    static ENABLED: Cell<Option<bool>> = const { Cell::new(None) };

    static LIVE_PEAK_TOTAL: Cell<u64> = const { Cell::new(0) };
    static LIVE_PEAK_COUNT: Cell<u64> = const { Cell::new(0) };
    static LIVE_PEAK_BREAKDOWN: RefCell<WordByteBreakdown> = const { RefCell::new(WordByteBreakdown {
        base: 0, shape: 0, syn_fs: 0, real_fs: 0, morphs: 0, mrule_apps: 0, obligatory: 0,
        unapplied_rule_counts: 0, root_runtime_id: 0, non_heads: 0, alternatives: 0,
    }) };
    static MAX_SINGLE_WORD_BYTES: Cell<u64> = const { Cell::new(0) };
    // Per-stratum-pass samples (not deduplicated) for the p50/p90/max distribution below.
    static ALT_LENS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    static NON_HEAD_LENS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

/// Cached `HC_WORD_STATS` read (one env lookup per thread), mirroring
/// `crate::stratum::frontier_profile::enabled`.
pub fn enabled() -> bool {
    ENABLED.with(|c| {
        if let Some(v) = c.get() {
            return v;
        }
        let v = std::env::var("HC_WORD_STATS").is_ok();
        c.set(Some(v));
        v
    })
}

/// Record one stratum pass's live `words` accumulator. Walks every word's field-level byte
/// breakdown (`crate::word::estimate_word_bytes_breakdown`, which recurses into `non_heads` and
/// `alternatives`); keeps the snapshot with the largest total seen so far, plus a running
/// distribution of `alternatives.len()`/`non_heads.len()` and the largest single `Word`.
pub fn record_live_words(words: &[Word]) {
    if !enabled() {
        return;
    }
    let mut total = 0u64;
    let mut breakdown = WordByteBreakdown::default();
    for w in words {
        let b = estimate_word_bytes_breakdown(w);
        let t = b.total() as u64;
        total += t;
        breakdown.add_assign(&b);
        MAX_SINGLE_WORD_BYTES.with(|c| c.set(c.get().max(t)));
        ALT_LENS.with(|v| v.borrow_mut().push(w.alternatives.len() as u32));
        NON_HEAD_LENS.with(|v| v.borrow_mut().push(w.non_heads.len() as u32));
    }
    if total > LIVE_PEAK_TOTAL.with(Cell::get) {
        LIVE_PEAK_TOTAL.with(|c| c.set(total));
        LIVE_PEAK_COUNT.with(|c| c.set(words.len() as u64));
        LIVE_PEAK_BREAKDOWN.with(|c| *c.borrow_mut() = breakdown);
    }
}

/// p50/p90/max over a `u32` sample vector (sorted copy; small under this diagnostic's gate).
fn percentiles(samples: &[u32]) -> (u32, u32, u32) {
    if samples.is_empty() {
        return (0, 0, 0);
    }
    let mut v = samples.to_vec();
    v.sort_unstable();
    let idx = |p: f64| -> u32 {
        let i = ((v.len() - 1) as f64 * p).round() as usize;
        v[i]
    };
    (idx(0.5), idx(0.9), *v.last().unwrap())
}

/// One word's whole cumulative word-byte picture — snapshot only, never reset.
#[derive(Debug, Clone, Copy, Default)]
pub struct WordStatsSnapshot {
    pub live_peak_total: u64,
    pub live_peak_count: u64,
    pub live_peak_breakdown: WordByteBreakdown,
    pub max_single_word_bytes: u64,
    pub alt_len_p50: u32,
    pub alt_len_p90: u32,
    pub alt_len_max: u32,
    pub non_head_len_p50: u32,
    pub non_head_len_p90: u32,
    pub non_head_len_max: u32,
}

pub fn snapshot() -> WordStatsSnapshot {
    let (alt_p50, alt_p90, alt_max) = ALT_LENS.with(|v| percentiles(&v.borrow()));
    let (nh_p50, nh_p90, nh_max) = NON_HEAD_LENS.with(|v| percentiles(&v.borrow()));
    WordStatsSnapshot {
        live_peak_total: LIVE_PEAK_TOTAL.with(Cell::get),
        live_peak_count: LIVE_PEAK_COUNT.with(Cell::get),
        live_peak_breakdown: LIVE_PEAK_BREAKDOWN.with(|c| *c.borrow()),
        max_single_word_bytes: MAX_SINGLE_WORD_BYTES.with(Cell::get),
        alt_len_p50: alt_p50,
        alt_len_p90: alt_p90,
        alt_len_max: alt_max,
        non_head_len_p50: nh_p50,
        non_head_len_p90: nh_p90,
        non_head_len_max: nh_max,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_of_empty_are_zero() {
        assert_eq!(percentiles(&[]), (0, 0, 0));
    }

    #[test]
    fn percentiles_pick_sorted_positions() {
        let v = [1u32, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let (p50, p90, max) = percentiles(&v);
        assert_eq!(max, 10);
        assert!((5..=6).contains(&p50));
        assert!(p90 >= 9);
    }
}
