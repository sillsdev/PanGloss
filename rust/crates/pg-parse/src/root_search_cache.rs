//! Per-word cache of complete, ordered grammar-trie results.

#[cfg(test)]
thread_local! {
    static TEST_DISABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TEST_LAST_HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn test_disable(disabled: bool) {
    TEST_DISABLED.with(|value| value.set(disabled));
}

#[cfg(test)]
pub(crate) fn test_last_hits() -> usize {
    TEST_LAST_HITS.with(std::cell::Cell::get)
}

use pg_grammar_model::model::{AllomorphId, LexEntryId, StratumId};
use pg_shape::Shape;
use rustc_hash::FxHashMap;

const LIMIT: usize = 1024 * 1024;
const SLOTS: usize = 128;
type ResultPair = (AllomorphId, LexEntryId);

struct Entry {
    results: Box<[ResultPair]>,
    charge: usize,
    bytes: usize,
}

type Table = FxHashMap<Shape, Entry>;

pub(crate) struct RootSearchCache {
    tables: Vec<Option<Table>>,
    bytes: usize,
    peak: usize,
    hits: usize,
}

impl RootSearchCache {
    pub(crate) fn new(strata: usize) -> Self {
        let tables: Vec<Option<Table>> = (0..strata.min(usize::from(u8::MAX) + 1))
            .map(|_| None)
            .collect();
        let bytes = tables.capacity() * std::mem::size_of::<Option<Table>>() * 2
            + std::mem::size_of::<Self>()
            + 4096;
        Self {
            tables,
            bytes,
            peak: bytes,
            hits: 0,
        }
    }

    pub(crate) fn get(&mut self, stratum: StratumId, shape: &Shape) -> Option<Vec<ResultPair>> {
        let entry = self
            .tables
            .get_mut(stratum.0 as usize)?
            .as_mut()?
            .get_mut(shape)?;
        for _ in 0..entry.charge {
            if !pg_fst::work::consume() {
                return Some(Vec::new());
            }
        }
        self.hits += 1;
        Some(entry.results.to_vec())
    }

    pub(crate) fn insert(
        &mut self,
        stratum: StratumId,
        shape: &Shape,
        results: &[ResultPair],
        search_work: usize,
    ) {
        #[cfg(test)]
        if TEST_DISABLED.with(std::cell::Cell::get) {
            return;
        }
        let charge = shape.key_work().saturating_add(results.len()).max(1);
        if search_work <= charge || pg_fst::work::stopped() {
            return;
        }
        let entry_bytes = shape.heap_bytes_upper_bound().saturating_add(
            results
                .len()
                .saturating_mul(std::mem::size_of::<ResultPair>())
                .saturating_mul(2)
                .saturating_add(4096),
        );
        let idx = stratum.0 as usize;
        if self.tables[idx].is_none() {
            // Allow for bucket rounding, control bytes and allocator overhead.
            let table_bytes = SLOTS * 4 * (std::mem::size_of::<(Shape, Entry)>() + 1) + 4096;
            if self
                .bytes
                .saturating_add(table_bytes)
                .saturating_add(entry_bytes)
                > LIMIT
            {
                return;
            }
            self.tables[idx] = Some(Table::with_capacity_and_hasher(SLOTS, Default::default()));
            self.bytes += table_bytes;
        }
        if self.bytes.saturating_add(entry_bytes) > LIMIT
            || self.tables[idx].as_ref().unwrap().len() == SLOTS
        {
            let table = self.tables[idx].as_mut().unwrap();
            self.bytes -= table.values().map(|entry| entry.bytes).sum::<usize>();
            table.clear();
        }
        if self.bytes.saturating_add(entry_bytes) > LIMIT {
            return;
        }
        self.tables[idx].as_mut().unwrap().insert(
            shape.clone(),
            Entry {
                results: results.into(),
                charge,
                bytes: entry_bytes,
            },
        );
        self.bytes += entry_bytes;
        self.peak = self.peak.max(self.bytes);
    }

    pub(crate) fn finish(&self) {
        #[cfg(test)]
        TEST_LAST_HITS.with(|value| value.set(self.hits));
        if std::env::var_os("PG_ROOT_CACHE_STATS").is_some() {
            eprintln!("ROOT_CACHE peak_bytes={} hits={}", self.peak, self.hits);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_shape::ShapeBuilder;

    fn shape(cd: u32) -> Shape {
        let mut builder = ShapeBuilder::new();
        builder.push_segment(cd);
        builder.finish()
    }

    #[test]
    fn hits_preserve_order_and_are_charged_per_word() {
        let mut cache = RootSearchCache::new(2);
        let s = shape(5);
        let raw = vec![
            (AllomorphId(7), LexEntryId(8)),
            (AllomorphId(1), LexEntryId(3)),
        ];
        let budget = pg_fst::work::WorkBudget::new(100);
        let _scope = budget.enter();
        cache.insert(StratumId(0), &s, &raw, 50);
        assert!(cache.get(StratumId(1), &s).is_none());
        assert_eq!(cache.get(StratumId(0), &s), Some(raw));
        assert!(budget.used() > 0);
        assert!(RootSearchCache::new(2).get(StratumId(0), &s).is_none());
    }

    #[test]
    fn evicts_and_rejects_oversized_entries_within_cap() {
        let mut cache = RootSearchCache::new(1);
        for i in 0..1000 {
            cache.insert(
                StratumId(0),
                &shape(i),
                &[(AllomorphId(i), LexEntryId(i))],
                100,
            );
            assert!(cache.bytes <= LIMIT);
            assert!(cache.peak <= LIMIT);
        }
        assert!(cache.get(StratumId(0), &shape(0)).is_none());
        assert_eq!(
            cache.get(StratumId(0), &shape(999)),
            Some(vec![(AllomorphId(999), LexEntryId(999))])
        );
        let huge = vec![(AllomorphId(0), LexEntryId(0)); LIMIT];
        cache.insert(StratumId(0), &shape(1001), &huge, usize::MAX);
        assert!(cache.get(StratumId(0), &shape(1001)).is_none());
        assert!(cache.peak <= LIMIT);
    }

    #[test]
    fn stopped_search_is_not_cached() {
        let mut cache = RootSearchCache::new(1);
        let budget = pg_fst::work::WorkBudget::new(1);
        let _scope = budget.enter();
        assert!(pg_fst::work::consume());
        cache.insert(StratumId(0), &shape(1), &[], 100);
        assert!(cache.get(StratumId(0), &shape(1)).is_none());
    }

    #[test]
    fn oversized_stratum_count_cannot_exceed_the_cache_cap() {
        let cache = RootSearchCache::new(usize::MAX);
        assert_eq!(cache.tables.len(), 256);
        assert!(cache.peak <= LIMIT);
    }
}
