//! `HC_ALT_YIELD=1` diagnostic (env-gated, off by default): measures what `Word::alternatives`
//! actually yields once expanded, to answer one question — does a canonical with N alternatives
//! yield ~N distinct analyses at synthesis, or does it collapse to a handful?
//!
//! Four counters, all recorded from `Morpher::parse_word_core_selected`'s two
//! `expand_alternatives`/synthesis loops (`morpher.rs`, normal and guess paths):
//! - [`record_canonical`]: `alternatives.len()` on each canonical (`aw` in `results.values()`),
//!   once per canonical, summed and maxed across the whole parse.
//! - [`record_expansion`]: the length of each `Word::expand_alternatives()` call's return value.
//! - [`record_identity`]: every synthesized candidate that passes the validity/surface-match
//!   gate (the same gate feeding `matches`), projected to [`crate::identity::AnalysisIdentity`] —
//!   ordered stable morpheme keys, root position, and stable category — and
//!   inserted into a running set. Its final size is the DISTINCT count.
//!
//! Zero cost when unset: [`enabled`] caches one env read per thread.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;

use crate::identity::AnalysisIdentity;

thread_local! {
    static ENABLED: Cell<Option<bool>> = const { Cell::new(None) };
    static CANONICAL_ALT_TOTAL: Cell<u64> = const { Cell::new(0) };
    static CANONICAL_ALT_MAX: Cell<u64> = const { Cell::new(0) };
    static EXPANDED_TOTAL: Cell<u64> = const { Cell::new(0) };
    static IDENTITIES: RefCell<BTreeSet<AnalysisIdentity>> = RefCell::new(BTreeSet::new());
}

/// Cached `HC_ALT_YIELD` read (one env lookup per thread), mirroring `pg_rules::word_stats::enabled`.
pub fn enabled() -> bool {
    ENABLED.with(|c| {
        if let Some(v) = c.get() {
            return v;
        }
        let v = std::env::var("HC_ALT_YIELD").is_ok();
        c.set(Some(v));
        v
    })
}

/// Record one canonical's stashed `Word::alternatives` length, right before it (via its
/// lexical-lookup descendants) is expanded.
pub fn record_canonical(alt_len: usize) {
    if !enabled() {
        return;
    }
    CANONICAL_ALT_TOTAL.with(|c| c.set(c.get() + alt_len as u64));
    CANONICAL_ALT_MAX.with(|c| c.set(c.get().max(alt_len as u64)));
}

/// Record one `Word::expand_alternatives()` call's output length.
pub fn record_expansion(len: usize) {
    if !enabled() {
        return;
    }
    EXPANDED_TOTAL.with(|c| c.set(c.get() + len as u64));
}

/// Record one surviving (valid + surface-matched) candidate's identity.
pub fn record_identity(id: AnalysisIdentity) {
    if !enabled() {
        return;
    }
    IDENTITIES.with(|s| {
        s.borrow_mut().insert(id);
    });
}

/// One word's whole cumulative alternatives-yield picture — snapshot only, never reset.
#[derive(Debug, Clone, Copy, Default)]
pub struct AltYieldSnapshot {
    pub canonical_alt_total: u64,
    pub canonical_alt_max: u64,
    pub expanded_total: u64,
    pub distinct_identities: u64,
}

pub fn snapshot() -> AltYieldSnapshot {
    AltYieldSnapshot {
        canonical_alt_total: CANONICAL_ALT_TOTAL.with(Cell::get),
        canonical_alt_max: CANONICAL_ALT_MAX.with(Cell::get),
        expanded_total: EXPANDED_TOTAL.with(Cell::get),
        distinct_identities: IDENTITIES.with(|s| s.borrow().len() as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_of_untouched_counters_is_zero() {
        // Independent of `enabled()` state: a fresh thread's counters start zeroed.
        let s = AltYieldSnapshot::default();
        assert_eq!(s.canonical_alt_total, 0);
        assert_eq!(s.distinct_identities, 0);
    }

    #[test]
    fn recorders_are_no_ops_when_env_var_is_unset() {
        // No `HC_ALT_YIELD` here, so `enabled()` caches `false` and every recorder is a no-op.
        record_canonical(1919);
        record_expansion(1919);
        let s = snapshot();
        assert_eq!(s.canonical_alt_total, 0);
        assert_eq!(s.expanded_total, 0);
    }
}
