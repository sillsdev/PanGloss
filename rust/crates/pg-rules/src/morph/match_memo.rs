//! The per-word analysis match memo.

use std::cell::RefCell;

#[cfg(test)]
thread_local! {
    static TEST_DISABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn test_disable(disabled: bool) {
    TEST_DISABLED.with(|value| value.set(disabled));
}

use pg_fst::work::WorkBudget;
use pg_grammar_model::model::AllomorphId;
use pg_shape::Shape;
use rustc_hash::FxHashMap;

/// Most first-sighting fingerprints held before the set restarts: the load limit of a 4096-bucket table.
const SEEN_LIMIT: usize = 3584;

/// Bytes that table takes at full size (8-byte slot plus control byte per bucket, plus its header); reserved while the memo holds anything.
pub const SEEN_TABLE_BYTES: usize = 4096 * 9 + 32;

/// Hard ceiling on the memo's real heap bytes, resize transients included; the least-hit entries are evicted to stay under it.
pub const DEFAULT_CAP_BYTES: usize = 10 * 1024 * 1024;

/// The memo keeps its accounted bytes at this fraction of its ceiling (numerator over denominator); the rest covers evictions' rank list and allocator rounding the model does not see.
const MARGIN: (usize, usize) = (5, 6);

/// A word's memo may take at most this fraction of its `--word-mem-cap-mib` allowance, so optional state cannot crowd out the search's own.
const ALLOWANCE_SHARE: usize = 8;

/// Everything besides the shape that the match result depends on.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct MatchKey {
    allomorph: AllomorphId,
    prune_disagreeing_copies: bool,
}

struct Entry {
    outputs: Box<[Shape]>,
    /// The matcher found no match at all, which an observation recorder reports.
    no_match: bool,
    charge: usize,
    hits: u32,
    last_use: u64,
}

type EntryKey = (MatchKey, Shape);

const ENTRY_SLOT: usize = std::mem::size_of::<(EntryKey, Entry)>();
const SHAPE_SLOT: usize = std::mem::size_of::<(usize, u32)>();

/// A glibc chunk: 8-byte header, 16-byte steps, 32 bytes at least.
fn chunk(bytes: usize) -> usize {
    if bytes == 0 {
        0
    } else {
        ((bytes + 8 + 15) & !15).max(32)
    }
}

/// Buckets a hash table with this usable capacity has allocated.
fn buckets_of(capacity: usize) -> usize {
    match capacity {
        0 => 0,
        1..=7 => (capacity + 1).next_power_of_two(),
        _ => capacity / 7 * 8,
    }
}

/// Bytes of a hash table with this many buckets: slots, one control byte per bucket and a trailing group.
fn table_bytes(buckets: usize, slot: usize) -> usize {
    if buckets == 0 {
        0
    } else {
        chunk(((buckets * slot + 15) & !15) + buckets + 16)
    }
}

/// Buckets after the table next doubles.
fn grown(buckets: usize) -> usize {
    (buckets * 2).max(4)
}

/// What the stored shapes weigh, each distinct allocation once however many entries point at it.
#[derive(Default)]
struct Shapes {
    refs: FxHashMap<usize, u32>,
    bytes: usize,
}

impl Shapes {
    fn acquire(&mut self, shape: &Shape) {
        let count = self.refs.entry(shape.allocation_id()).or_insert(0);
        *count += 1;
        if *count == 1 {
            self.bytes += shape.heap_bytes();
        }
    }

    fn release(&mut self, shape: &Shape) {
        let id = shape.allocation_id();
        let count = self.refs.get_mut(&id).expect("a stored shape is counted");
        *count -= 1;
        if *count == 0 {
            self.refs.remove(&id);
            self.bytes -= shape.heap_bytes();
        }
    }
}

fn slice_bytes(outputs: usize) -> usize {
    chunk(outputs * std::mem::size_of::<Shape>())
}

#[derive(Default)]
struct Table {
    /// Fingerprints of searches seen once; only a repeat is worth storing, so one-off matches never allocate.
    seen: rustc_hash::FxHashSet<u64>,
    entries: FxHashMap<EntryKey, Entry>,
    shapes: Shapes,
    /// Bytes of the output slices.
    slices: usize,
    /// Bytes reserved against the word allowance so far.
    charged: usize,
    clock: u64,
    hits: usize,
}

impl Table {
    /// Real heap bytes of everything held, tables included at one and a half times their size so a resize's old and new tables fit; zero while nothing is held.
    fn accounted(&self, extra_entries: usize, extra_shapes: usize, extra_bytes: usize) -> usize {
        if self.entries.is_empty() && self.seen.is_empty() && extra_entries == 0 {
            return 0;
        }
        let mut entry_buckets = buckets_of(self.entries.capacity());
        if self.entries.len() + extra_entries > self.entries.capacity() {
            entry_buckets = grown(entry_buckets);
        }
        let mut shape_buckets = buckets_of(self.shapes.refs.capacity());
        if self.shapes.refs.len() + extra_shapes > self.shapes.refs.capacity() {
            shape_buckets = grown(shape_buckets);
        }
        let tables =
            table_bytes(entry_buckets, ENTRY_SLOT) + table_bytes(shape_buckets, SHAPE_SLOT);
        self.shapes.bytes + self.slices + extra_bytes + tables + tables / 2 + SEEN_TABLE_BYTES
    }

    fn add(&mut self, key: EntryKey, entry: Entry) {
        self.shapes.acquire(&key.1);
        for output in entry.outputs.iter() {
            self.shapes.acquire(output);
        }
        self.slices += slice_bytes(entry.outputs.len());
        self.entries.insert(key, entry);
    }

    fn remove(&mut self, key: &EntryKey) -> bool {
        let Some(entry) = self.entries.remove(key) else {
            return false;
        };
        self.shapes.release(&key.1);
        for output in entry.outputs.iter() {
            self.shapes.release(output);
        }
        self.slices -= slice_bytes(entry.outputs.len());
        true
    }

    /// Evict the entries with the fewest hits (oldest use first) in batches until `done` holds.
    fn evict_until(&mut self, done: impl Fn(&Table) -> bool) {
        while !done(self) && !self.entries.is_empty() {
            let mut ranks: Vec<(u32, u64)> = self
                .entries
                .values()
                .map(|e| (e.hits, e.last_use))
                .collect();
            let batch = (ranks.len() / 8).max(1);
            let (_, &mut limit, _) = ranks.select_nth_unstable(batch - 1);
            drop(ranks);
            let Table {
                entries,
                shapes,
                slices,
                ..
            } = self;
            entries.retain(|key, e| {
                if (e.hits, e.last_use) > limit {
                    return true;
                }
                shapes.release(&key.1);
                for output in e.outputs.iter() {
                    shapes.release(output);
                }
                *slices -= slice_bytes(e.outputs.len());
                false
            });
            if self.entries.len() * 2 < self.entries.capacity() {
                self.entries.shrink_to_fit();
                self.shapes.refs.shrink_to_fit();
            }
        }
    }

    /// Bring the word allowance's reservation to the bytes held; false when it refuses a larger one.
    fn sync(&mut self, work: &WorkBudget) -> bool {
        let held = self.accounted(0, 0, 0);
        if held > self.charged {
            if !work.try_charge_optional(held - self.charged) {
                return false;
            }
        } else {
            work.release_optional(self.charged - held);
        }
        self.charged = held;
        true
    }
}

/// Per-word memo of an allomorph's analysis-side match result, keyed by its exact input `Shape`.
///
/// The match, `GenerateShape` and dedup in `ana_allomorph_matches` are a pure function of the
/// allomorph, the input shape and the grammar-constant `prune_disagreeing_copies` flag. Everything
/// that varies per path (syntactic and realizational FS, rule counts, trail, template state) is read
/// or written outside it, so a hit replays the stored output shapes into a fresh clone of the
/// caller's word and the caller's gates still run on every attempt.
///
/// An entry is stored on the second miss of the same key, so one-off matches cost a fingerprint, not an allocation.
///
/// The memo lives in one word's `WordBudget` and is dropped with it. A hit is charged
/// one work unit plus one per output shape it materialises (the unit `ana_allomorph_matches` charges
/// a candidate), and an entry is stored only when its miss cost more than that, so a hit never
/// charges more than the search it replaces and every hit still moves `--work-cap` toward its limit.
///
/// Interim measure for PanGloss#477; the structural fix is PanGloss#481. Memory is hard-capped in
/// real heap bytes: every distinct stored shape is counted once with `Shape::heap_bytes`, hash tables
/// by their actual capacity at one and a half times (a resize holds the old and the new table), plus
/// the fingerprint table. Eviction (fewest hits, oldest first) keeps that accounted total at five
/// sixths of `DEFAULT_CAP_BYTES` and is planned before the insertion that would grow a table, so the
/// real peak, resize and the eviction's rank list included, stays under the cap. The model assumes
/// glibc's 8-byte header and 16-byte steps. The real-peak bound is verified on glibc only and assumed
/// elsewhere (Windows x64 has a 16-byte header), where the per-column slack and the sixth of the
/// ceiling left unaccounted are the margin.
/// Stored bytes are also reserved against the word's `--word-mem-cap-mib` allowance, but as optional
/// state: the search's own charges ignore the reservation, so the memo never changes whether a word
/// hits the allowance. The memo takes at most an eighth of the allowance and gives entries back as
/// the search's retained state grows; an entry that does not fit is simply not stored, so the memo
/// never stops a search.
pub struct MatchMemo {
    table: RefCell<Table>,
    work: WorkBudget,
    cap: usize,
}

impl MatchMemo {
    pub fn new(work: WorkBudget) -> Self {
        Self::with_cap(work, DEFAULT_CAP_BYTES)
    }

    pub fn with_cap(work: WorkBudget, cap: usize) -> Self {
        Self {
            table: RefCell::default(),
            work,
            cap,
        }
    }

    /// Lookups that returned stored outputs so far.
    pub fn hits(&self) -> usize {
        self.table.borrow().hits
    }

    /// Accounted real heap bytes held now, the fingerprint table and resize margin included.
    pub fn bytes(&self) -> usize {
        self.table.borrow().accounted(0, 0, 0)
    }

    /// Stored entries.
    pub fn len(&self) -> usize {
        self.table.borrow().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The most accounted bytes the memo may hold now: its own ceiling, an eighth of the word allowance, and what the search's own retained state leaves free, so the memo yields as the search grows.
    fn room(&self) -> usize {
        let limit = match (self.work.word_cap(), self.work.word_headroom()) {
            (Some(allowance), Some(headroom)) => {
                self.cap.min(allowance / ALLOWANCE_SHARE).min(headroom)
            }
            _ => self.cap,
        };
        limit / MARGIN.1 * MARGIN.0
    }

    /// The stored outputs for this exact (allomorph, flag, shape), after charging the hit; an empty vector once the work cap stops the charge.
    #[doc(hidden)]
    pub fn get(
        &self,
        allomorph: AllomorphId,
        prune_disagreeing_copies: bool,
        shape: &Shape,
    ) -> Option<(Vec<Shape>, bool)> {
        #[cfg(test)]
        if TEST_DISABLED.with(std::cell::Cell::get) {
            return None;
        }
        let mut table = self.table.borrow_mut();
        let key = (
            MatchKey {
                allomorph,
                prune_disagreeing_copies,
            },
            shape.clone(),
        );
        table.clock += 1;
        let clock = table.clock;
        let entry = table.entries.get_mut(&key)?;
        let (charge, outputs, no_match) = (entry.charge, entry.outputs.to_vec(), entry.no_match);
        entry.last_use = clock;
        for _ in 0..charge {
            if !pg_fst::work::consume() {
                return Some((Vec::new(), false));
            }
        }
        let entry = table.entries.get_mut(&key)?;
        entry.hits = entry.hits.saturating_add(1);
        table.hits += 1;
        Some((outputs, no_match))
    }

    /// Store a completed match; skipped when the search was stopped or cost no more than a hit would.
    #[doc(hidden)]
    pub fn insert(
        &self,
        allomorph: AllomorphId,
        prune_disagreeing_copies: bool,
        shape: &Shape,
        outputs: &[Shape],
        no_match: bool,
        search_work: usize,
    ) {
        #[cfg(test)]
        if TEST_DISABLED.with(std::cell::Cell::get) {
            return;
        }
        let charge = outputs.len().saturating_add(1);
        if search_work <= charge || pg_fst::work::stopped() {
            return;
        }
        let mut table = self.table.borrow_mut();
        let fingerprint = {
            use std::hash::BuildHasher;
            rustc_hash::FxBuildHasher.hash_one((allomorph, prune_disagreeing_copies, shape))
        };
        if table.seen.len() >= SEEN_LIMIT {
            table.seen.clear();
        }
        if table.seen.insert(fingerprint) {
            return;
        }
        let key = (
            MatchKey {
                allomorph,
                prune_disagreeing_copies,
            },
            shape.clone(),
        );
        table.remove(&key);
        let room = self.room();
        let extra_shapes = 1 + outputs.len();
        let extra_bytes = outputs
            .iter()
            .fold(shape.heap_bytes(), |sum, s| sum + s.heap_bytes())
            + slice_bytes(outputs.len());
        let fits = |t: &Table, goal: usize| t.accounted(1, extra_shapes, extra_bytes) <= goal;
        if !fits(&table, room) {
            table.evict_until(|t| fits(t, room / 10 * 9));
        }
        if fits(&table, room) {
            table.clock += 1;
            let last_use = table.clock;
            table.add(
                key.clone(),
                Entry {
                    outputs: outputs.into(),
                    no_match,
                    charge,
                    hits: 0,
                    last_use,
                },
            );
            if !table.sync(&self.work) {
                table.remove(&key);
            }
        }
        table.sync(&self.work);
    }
}

impl Drop for MatchMemo {
    fn drop(&mut self) {
        self.work.release_optional(self.table.get_mut().charged);
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

    fn store(memo: &MatchMemo, allo: u32, shape: &Shape, outputs: &[Shape], work: usize) {
        for _ in 0..2 {
            memo.insert(AllomorphId(allo), true, shape, outputs, false, work);
        }
    }

    #[test]
    fn a_first_sighting_is_not_stored() {
        let memo = MatchMemo::new(WorkBudget::new(usize::MAX));
        memo.insert(AllomorphId(1), true, &shape(1), &[shape(7)], false, 100);
        assert!(memo.get(AllomorphId(1), true, &shape(1)).is_none());
    }

    #[test]
    fn a_hit_needs_the_same_allomorph_flag_and_shape() {
        let memo = MatchMemo::new(WorkBudget::new(usize::MAX));
        let (a, b) = (AllomorphId(1), AllomorphId(2));
        for _ in 0..2 {
            memo.insert(a, true, &shape(1), &[shape(7)], false, 100);
        }
        assert!(memo.get(b, true, &shape(1)).is_none());
        assert!(memo.get(a, false, &shape(1)).is_none());
        assert!(memo.get(a, true, &shape(2)).is_none());
        assert_eq!(memo.get(a, true, &shape(1)), Some((vec![shape(7)], false)));
        assert_eq!(memo.hits(), 1);
    }

    #[test]
    fn a_hit_is_charged_one_unit_plus_one_per_output() {
        let memo = MatchMemo::new(WorkBudget::new(usize::MAX));
        let budget = pg_fst::work::WorkBudget::new(100);
        let _scope = budget.enter();
        store(&memo, 1, &shape(1), &[shape(7), shape(8)], 50);
        memo.get(AllomorphId(1), true, &shape(1)).unwrap();
        assert_eq!(budget.used(), 3);
    }

    #[test]
    fn searches_no_dearer_than_a_hit_or_stopped_by_the_cap_are_not_stored() {
        let memo = MatchMemo::new(WorkBudget::new(usize::MAX));
        store(&memo, 1, &shape(1), &[shape(7)], 2);
        assert!(memo.get(AllomorphId(1), true, &shape(1)).is_none());
        let budget = pg_fst::work::WorkBudget::new(1);
        let _scope = budget.enter();
        assert!(pg_fst::work::consume());
        store(&memo, 1, &shape(2), &[shape(7)], 100);
        assert!(memo.get(AllomorphId(1), true, &shape(2)).is_none());
    }

    const SMALL: usize = 60_000;

    #[test]
    fn the_memo_never_passes_its_cap_and_keeps_the_entries_that_hit() {
        let work = WorkBudget::new(usize::MAX).with_word_cap(Some(SMALL * ALLOWANCE_SHARE));
        let memo = MatchMemo::with_cap(work.clone(), SMALL);
        store(&memo, 1, &shape(1), &[shape(7)], 100);
        assert!(memo.get(AllomorphId(1), true, &shape(1)).is_some());
        for i in 2..400 {
            store(&memo, 1, &shape(i), &[shape(7)], 100);
            assert!(memo.bytes() <= SMALL, "{} over {SMALL}", memo.bytes());
        }
        assert!(memo.len() > 4 && memo.len() < 398, "{} entries", memo.len());
        assert!(memo.get(AllomorphId(1), true, &shape(1)).is_some());
        assert!(memo.get(AllomorphId(1), true, &shape(399)).is_some());
        assert!(memo.get(AllomorphId(1), true, &shape(2)).is_none());
        drop(memo);
        assert!(
            work.try_charge_optional(SMALL * ALLOWANCE_SHARE),
            "dropping releases the bytes"
        );
    }

    #[test]
    fn the_memo_takes_at_most_an_eighth_of_the_word_allowance() {
        let allowance = SMALL * ALLOWANCE_SHARE;
        let work = WorkBudget::new(usize::MAX).with_word_cap(Some(allowance));
        let memo = MatchMemo::with_cap(work.clone(), usize::MAX);
        for i in 1..400 {
            store(&memo, 1, &shape(i), &[shape(7)], 100);
        }
        assert!(memo.bytes() <= allowance / ALLOWANCE_SHARE);
        assert!(work.try_charge_optional(allowance - allowance / ALLOWANCE_SHARE));
    }

    #[test]
    fn a_shape_shared_by_entries_is_counted_once() {
        let memo = MatchMemo::new(WorkBudget::new(usize::MAX));
        let key = shape(1);
        let out = shape(7);
        for _ in 0..2 {
            memo.insert(
                AllomorphId(1),
                true,
                &key,
                std::slice::from_ref(&out),
                false,
                100,
            );
        }
        let first = memo.table.borrow().shapes.bytes;
        for _ in 0..2 {
            memo.insert(
                AllomorphId(2),
                true,
                &key,
                std::slice::from_ref(&out),
                false,
                100,
            );
        }
        assert_eq!(memo.len(), 2);
        assert_eq!(memo.table.borrow().shapes.bytes, first);
    }

    #[test]
    fn the_memo_yields_so_a_word_just_under_the_allowance_is_not_capped() {
        let allowance = SMALL * ALLOWANCE_SHARE;
        let work = WorkBudget::new(usize::MAX).with_word_cap(Some(allowance));
        let memo = MatchMemo::with_cap(work.clone(), usize::MAX);
        for i in 1..20 {
            store(&memo, 1, &shape(i), &[shape(7)], 100);
        }
        assert!(memo.len() > 10);
        assert!(
            work.charge_word(allowance),
            "main completes this word under the allowance"
        );
        assert!(!work.word_capped());
        store(&memo, 1, &shape(99), &[shape(7)], 100);
        assert!(memo.is_empty(), "the memo gave its entries back");
        assert!(memo.get(AllomorphId(1), true, &shape(99)).is_none());
        assert!(!work.word_capped());
    }

    #[test]
    fn the_word_memory_allowance_declines_an_entry_without_stopping_the_search() {
        let work = WorkBudget::new(usize::MAX).with_word_cap(Some(1));
        let memo = MatchMemo::new(work.clone());
        store(&memo, 1, &shape(1), &[shape(7)], 100);
        assert!(memo.get(AllomorphId(1), true, &shape(1)).is_none());
        assert!(!work.word_capped());
    }

    #[test]
    fn the_fingerprint_table_counts_against_the_allowance() {
        let work = WorkBudget::new(usize::MAX).with_word_cap(Some(SMALL * ALLOWANCE_SHARE));
        let memo = MatchMemo::with_cap(work.clone(), SMALL);
        store(&memo, 1, &shape(1), &[shape(7)], 100);
        assert!(!work.try_charge_optional(SMALL * ALLOWANCE_SHARE - SEEN_TABLE_BYTES + 1));
    }
}
