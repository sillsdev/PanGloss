//! `HC_CLOCK_SAMPLE=1` diagnostic (env-gated, off by default): puts allocator bytes, the
//! live-frontier model, and both memo tables' model bytes on ONE clock, sampled at the SAME instant,
//! instead of the three separate maxima `pg_rules::word_stats`/`pg-cli`'s `alloc_trace` measured over
//! three different windows that never co-occurred (`docs/research/memory-measurement-repair.md`,
//! task T3). Two sample points: [`record`] called once per completed stratum pass
//! (`crate::stratum::analyze`, mirroring `word_stats::record_live_words`'s own call site) and once
//! per canonical in `pg-parse`'s synthesis loop (the phase `word_stats` cannot see at all -- see the
//! research note's "Blind phase" finding). Samples accumulate in a thread-local buffer; `pg-cli`
//! drains them once per word (`take_samples`) and prints one TSV line per sample.
//!
//! Zero cost when unset: [`enabled`] caches one env read per thread, and [`record`] checks it before
//! doing any work (the byte walks below are not free).

use std::cell::{Cell, RefCell};

use crate::word::estimate_words_breakdown;
use crate::Word;

thread_local! {
    static ENABLED: Cell<Option<bool>> = const { Cell::new(None) };
    static SEQ: Cell<u64> = const { Cell::new(0) };
    static SAMPLES: RefCell<Vec<ClockSample>> = const { RefCell::new(Vec::new()) };
    /// Installed by `pg-cli` only in an `alloc-trace` build; unset otherwise reports `None`, never a wrong zero.
    static ALLOC_QUERY: Cell<Option<fn() -> (u64, u64)>> = const { Cell::new(None) };
}

/// Cached `HC_CLOCK_SAMPLE` read (one env lookup per thread), mirroring
/// `word_stats::enabled`/`stratum::frontier_profile::enabled`.
pub fn enabled() -> bool {
    ENABLED.with(|c| {
        if let Some(v) = c.get() {
            return v;
        }
        let v = std::env::var("HC_CLOCK_SAMPLE").is_ok();
        c.set(Some(v));
        v
    })
}

/// `f()` must return `(peak_bytes, live_bytes)` from `pg-cli`'s `alloc_trace` counting allocator.
/// Call once, at process start, only in an `alloc-trace` build.
pub fn set_allocator_query_hook(f: fn() -> (u64, u64)) {
    ALLOC_QUERY.with(|c| c.set(Some(f)));
}

/// One instant's reading across every pool this repair tracks. `alloc_peak_bytes`/`alloc_live_bytes`
/// are `None` whenever no `set_allocator_query_hook` was installed (i.e. not an `alloc-trace` build)
/// -- an absent number, never a zero standing in for one.
#[derive(Debug, Clone, Copy)]
pub struct ClockSample {
    pub seq: u64,
    /// Where in the pipeline this was taken: `"stratum_pass"` or `"synthesis_loop"`.
    pub point: &'static str,
    pub alloc_peak_bytes: Option<u64>,
    pub alloc_live_bytes: Option<u64>,
    /// `estimate_words_breakdown`'s pass-level-deduped total over the `words` passed to `record`.
    pub live_frontier_bytes: u64,
    pub memo_key_bytes: u64,
    pub memo_results_bytes: u64,
    pub tpl_key_bytes: u64,
    pub tpl_results_bytes: u64,
}

/// Record one instant. `live_words` is whatever this call site's own live accumulator is at this
/// moment (a stratum pass's `words`, or the synthesis loop's growing `matches`) -- a borrowed
/// iterator, never an owned collection the caller had to clone/collect just to call this: doing so
/// would inflate the very allocator peak this sample exists to read. `scope`, when available,
/// supplies both memo tables' key/results bytes at the same instant.
pub fn record<'w>(
    point: &'static str,
    live_words: impl IntoIterator<Item = &'w Word>,
    scope: Option<&pg_memo::AnalysisScope<Word>>,
) {
    if !enabled() {
        return;
    }
    let live_frontier_bytes = estimate_words_breakdown(live_words).total() as u64;
    let (memo_key_bytes, tpl_key_bytes) = scope
        .map(pg_memo::AnalysisScope::estimate_key_bytes)
        .unwrap_or((0, 0));
    let memo_results_bytes = scope.map_or(0, pg_memo::AnalysisScope::memo_bytes_used) as u64;
    let tpl_results_bytes = scope.map_or(0, pg_memo::AnalysisScope::template_bytes_used) as u64;
    let (alloc_peak_bytes, alloc_live_bytes) = match ALLOC_QUERY.with(Cell::get) {
        Some(f) => {
            let (peak, live) = f();
            (Some(peak), Some(live))
        }
        None => (None, None),
    };
    let seq = SEQ.with(|c| {
        let n = c.get();
        c.set(n + 1);
        n
    });
    SAMPLES.with(|v| {
        v.borrow_mut().push(ClockSample {
            seq,
            point,
            alloc_peak_bytes,
            alloc_live_bytes,
            live_frontier_bytes,
            memo_key_bytes: memo_key_bytes as u64,
            memo_results_bytes,
            tpl_key_bytes: tpl_key_bytes as u64,
            tpl_results_bytes,
        })
    });
}

/// Drain every sample recorded since the last drain, in recording order. `pg-cli` calls this once
/// per word so the buffer does not grow across a whole batch and each word's rows print together.
pub fn take_samples() -> Vec<ClockSample> {
    SAMPLES.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_is_a_no_op_when_disabled() {
        // ENABLED defaults to reading HC_CLOCK_SAMPLE, which is unset in a normal test run.
        if enabled() {
            return; // Some other test in this process set the env var first; skip rather than false-fail.
        }
        record("stratum_pass", &[], None);
        assert!(take_samples().is_empty());
    }
}
