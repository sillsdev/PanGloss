//! A nesting-depth-aware timer stack backing `crate::stats::StatsCollector`'s `AnalysisPhase`
//! sub-phase breakdown: real elapsed nanoseconds attributed to the innermost active region, with
//! every nested region's own elapsed time subtracted from its parent so a rule that triggers other
//! rules is charged only its own cost, never theirs too. Gated behind the `stats-calibrate` Cargo
//! feature; off (every ordinary build), `enter`/`totals` never read the clock — see
//! `off_build_records_no_time_even_across_a_real_sleep` below.
//!
//! Generic over the caller's own key type so this crate carries no opinion about what a "kind" is;
//! `crate::stats::AnalysisPhase` supplies its own.

#[cfg(feature = "stats-calibrate")]
mod live {
    use std::cell::RefCell;
    use std::hash::Hash;

    use rustc_hash::FxHashMap as HashMap;
    use web_time::Instant;

    /// One key's accumulated self time and work, in the caller's own work unit.
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
    pub struct KindTotals {
        pub ns: u64,
        pub work: u64,
    }

    struct Frame<K> {
        kind: K,
        start: Instant,
        nested_ns: u64,
    }

    struct Inner<K> {
        stack: Vec<Frame<K>>,
        totals: HashMap<K, KindTotals>,
    }

    /// Construct one per calibration run, mirroring `crate::stats::StatsCollector::new`'s
    /// per-word lifetime and interior-mutability shape.
    pub struct SelfTimeAccumulator<K> {
        inner: RefCell<Inner<K>>,
    }

    /// Exiting (via `Drop`) attributes this region's self time to its `kind` and folds its own
    /// elapsed time into the enclosing region's nested tally.
    #[must_use]
    pub struct RegionGuard<'a, K: Copy + Eq + Hash> {
        acc: &'a SelfTimeAccumulator<K>,
        kind: K,
        work: u64,
    }

    impl<K: Copy + Eq + Hash> SelfTimeAccumulator<K> {
        pub fn new() -> Self {
            SelfTimeAccumulator {
                inner: RefCell::new(Inner {
                    stack: Vec::new(),
                    totals: HashMap::default(),
                }),
            }
        }

        /// Enter a timed region for `kind`, carrying `work` units to accumulate alongside its
        /// self time. The guard must be dropped before an enclosing region's own guard drops.
        pub fn enter(&self, kind: K, work: u64) -> RegionGuard<'_, K> {
            self.inner.borrow_mut().stack.push(Frame {
                kind,
                start: Instant::now(),
                nested_ns: 0,
            });
            RegionGuard {
                acc: self,
                kind,
                work,
            }
        }

        fn exit(&self, kind: K, work: u64) {
            let mut inner = self.inner.borrow_mut();
            let frame = inner
                .stack
                .pop()
                .expect("SelfTimeAccumulator::exit called without a matching enter");
            assert!(
                frame.kind == kind,
                "SelfTimeAccumulator: enter/exit are not LIFO"
            );
            let elapsed_ns = frame.start.elapsed().as_nanos() as u64;
            let self_ns = elapsed_ns.saturating_sub(frame.nested_ns);
            if let Some(parent) = inner.stack.last_mut() {
                parent.nested_ns += elapsed_ns;
            }
            let totals = inner.totals.entry(kind).or_default();
            totals.ns += self_ns;
            totals.work += work;
        }

        /// A snapshot of every kind's accumulated totals so far.
        pub fn totals(&self) -> HashMap<K, KindTotals> {
            self.inner.borrow().totals.clone()
        }
    }

    impl<K: Copy + Eq + Hash> Default for SelfTimeAccumulator<K> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<K: Copy + Eq + Hash> Drop for RegionGuard<'_, K> {
        fn drop(&mut self) {
            self.acc.exit(self.kind, self.work);
        }
    }
}

#[cfg(not(feature = "stats-calibrate"))]
mod off {
    use std::hash::Hash;
    use std::marker::PhantomData;

    use rustc_hash::FxHashMap as HashMap;

    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
    pub struct KindTotals {
        pub ns: u64,
        pub work: u64,
    }

    /// The feature is off: no stack, no clock reads. `enter`/`totals` are complete no-ops.
    pub struct SelfTimeAccumulator<K> {
        _marker: PhantomData<K>,
    }

    #[must_use]
    pub struct RegionGuard<'a, K> {
        _marker: PhantomData<(&'a (), K)>,
    }

    impl<K: Copy + Eq + Hash> SelfTimeAccumulator<K> {
        pub fn new() -> Self {
            SelfTimeAccumulator {
                _marker: PhantomData,
            }
        }

        pub fn enter(&self, _kind: K, _work: u64) -> RegionGuard<'_, K> {
            RegionGuard {
                _marker: PhantomData,
            }
        }

        pub fn totals(&self) -> HashMap<K, KindTotals> {
            HashMap::default()
        }
    }

    impl<K: Copy + Eq + Hash> Default for SelfTimeAccumulator<K> {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(feature = "stats-calibrate")]
pub use live::{KindTotals, RegionGuard, SelfTimeAccumulator};
#[cfg(not(feature = "stats-calibrate"))]
pub use off::{KindTotals, RegionGuard, SelfTimeAccumulator};

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
    enum K {
        Outer,
        Inner,
    }

    #[cfg(feature = "stats-calibrate")]
    #[test]
    fn self_time_excludes_nested_region() {
        let acc: SelfTimeAccumulator<K> = SelfTimeAccumulator::new();
        {
            let _outer = acc.enter(K::Outer, 1);
            std::thread::sleep(std::time::Duration::from_millis(40));
            {
                let _inner = acc.enter(K::Inner, 1);
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        let totals = acc.totals();
        let outer = totals.get(&K::Outer).copied().unwrap_or_default();
        let inner = totals.get(&K::Inner).copied().unwrap_or_default();

        // Two 40ms sleeps of outer's own time (~80ms); inclusive time would be ~120ms with inner's sleep folded in.
        assert!(
            outer.ns < 110_000_000,
            "outer self time {outer:?} must exclude inner's nested elapsed time (inclusive would read ~120ms)"
        );
        assert!(
            inner.ns >= 20_000_000,
            "inner self time {inner:?} must still reflect its own real sleep"
        );
    }

    #[cfg(feature = "stats-calibrate")]
    #[test]
    fn work_accumulates_per_kind_independent_of_self_time() {
        let acc: SelfTimeAccumulator<K> = SelfTimeAccumulator::new();
        {
            let _a = acc.enter(K::Outer, 3);
        }
        {
            let _b = acc.enter(K::Outer, 4);
        }
        let totals = acc.totals();
        assert_eq!(totals.get(&K::Outer).unwrap().work, 7);
    }

    #[cfg(not(feature = "stats-calibrate"))]
    #[test]
    fn off_build_records_no_time_even_across_a_real_sleep() {
        let acc: SelfTimeAccumulator<K> = SelfTimeAccumulator::new();
        {
            let _outer = acc.enter(K::Outer, 5);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let totals = acc.totals();
        assert!(
            totals.is_empty(),
            "an ordinary build must not accumulate any calibration totals: {totals:?}"
        );
    }
}
