//! Traversal of a frozen `Fst` over a linear segment sequence: the acceptor port of `Fst.Transduce` (Fst.cs:304-414), the deterministic/nondeterministic FSA traversal methods, and `ResultCompare` (Fst.cs:416-441). Direction is baked into offset arithmetic (`phys`/`ann_start`/`ann_end`), not the walk itself, matching C#'s `Range.GetStart/GetEnd(_dir)` convention.
//! See `docs/research/pg-fst-traverse-design-notes.md` for the full input model and direction derivation.

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use rustc_hash::FxBuildHasher;

use crate::fst::Fst;
use crate::{Cmd, Direction, Register, CURRENT_POSITION};

// A permanent diagnostic, near-zero cost when unread (a few `Instant::now()` calls per `Transduce::run`/`distinct()`, thread-local `Cell` adds). Read via `pg_fst::profile::snapshot()`, gated on `HC_FST_PROFILE=1` in `pg-cli`.
pub mod profile {
    use super::Cell;

    pub(super) fn start() -> Option<web_time::Instant> {
        ENABLED.with(|e| e.get()).then(web_time::Instant::now)
    }

    /// Ask for timing on the current thread (the counters below are per thread too), after `reset`. Without it no clock is read per traversal.
    pub fn enable() {
        ENABLED.with(|e| e.set(true));
    }

    /// Whether timing is being read on the current thread.
    pub fn enabled() -> bool {
        ENABLED.with(|e| e.get())
    }

    thread_local! {
        /// Timing is read only when a host asks for diagnostics via `enable`, so otherwise no clock is read per traversal.
        static ENABLED: Cell<bool> = const { Cell::new(false) };
        static RUN_CALLS: Cell<u64> = const { Cell::new(0) };
        static RUN_NANOS: Cell<u128> = const { Cell::new(0) };
        static RUN_MAX_NANOS: Cell<u128> = const { Cell::new(0) };
        static NONDET_CALLS: Cell<u64> = const { Cell::new(0) };
        static NONDET_NANOS: Cell<u128> = const { Cell::new(0) };
        static NONDET_MAX_TRAVERSED: Cell<usize> = const { Cell::new(0) };
        static NONDET_TOTAL_TRAVERSED: Cell<u64> = const { Cell::new(0) };
        static DET_CALLS: Cell<u64> = const { Cell::new(0) };
        static DET_NANOS: Cell<u128> = const { Cell::new(0) };
        static DISTINCT_CALLS: Cell<u64> = const { Cell::new(0) };
        static DISTINCT_NANOS: Cell<u128> = const { Cell::new(0) };
        static DISTINCT_MAX_INPUT_LEN: Cell<usize> = const { Cell::new(0) };
        static DISTINCT_TOTAL_INPUT_LEN: Cell<u64> = const { Cell::new(0) };
    }

    pub(super) fn record_run(elapsed_nanos: u128) {
        RUN_CALLS.with(|c| c.set(c.get() + 1));
        RUN_NANOS.with(|c| c.set(c.get() + elapsed_nanos));
        RUN_MAX_NANOS.with(|c| c.set(c.get().max(elapsed_nanos)));
    }

    pub(super) fn record_nondet(elapsed_nanos: u128, traversed_len: usize) {
        NONDET_CALLS.with(|c| c.set(c.get() + 1));
        NONDET_NANOS.with(|c| c.set(c.get() + elapsed_nanos));
        NONDET_MAX_TRAVERSED.with(|c| c.set(c.get().max(traversed_len)));
        NONDET_TOTAL_TRAVERSED.with(|c| c.set(c.get() + traversed_len as u64));
    }

    pub(super) fn record_det(elapsed_nanos: u128) {
        DET_CALLS.with(|c| c.set(c.get() + 1));
        DET_NANOS.with(|c| c.set(c.get() + elapsed_nanos));
    }

    pub(super) fn record_distinct(elapsed_nanos: u128, input_len: usize) {
        DISTINCT_CALLS.with(|c| c.set(c.get() + 1));
        DISTINCT_NANOS.with(|c| c.set(c.get() + elapsed_nanos));
        DISTINCT_MAX_INPUT_LEN.with(|c| c.set(c.get().max(input_len)));
        DISTINCT_TOTAL_INPUT_LEN.with(|c| c.set(c.get() + input_len as u64));
    }

    /// (run_calls, run_total_ns, run_max_ns, nondet_calls, nondet_total_ns, nondet_max_traversed, nondet_total_traversed, det_calls, det_total_ns, distinct_calls, distinct_total_ns, distinct_max_input_len, distinct_total_input_len) -- snapshot only, never reset.
    #[allow(clippy::type_complexity)]
    pub fn snapshot() -> (
        u64,
        u128,
        u128,
        u64,
        u128,
        usize,
        u64,
        u64,
        u128,
        u64,
        u128,
        usize,
        u64,
    ) {
        (
            RUN_CALLS.with(|c| c.get()),
            RUN_NANOS.with(|c| c.get()),
            RUN_MAX_NANOS.with(|c| c.get()),
            NONDET_CALLS.with(|c| c.get()),
            NONDET_NANOS.with(|c| c.get()),
            NONDET_MAX_TRAVERSED.with(|c| c.get()),
            NONDET_TOTAL_TRAVERSED.with(|c| c.get()),
            DET_CALLS.with(|c| c.get()),
            DET_NANOS.with(|c| c.get()),
            DISTINCT_CALLS.with(|c| c.get()),
            DISTINCT_NANOS.with(|c| c.get()),
            DISTINCT_MAX_INPUT_LEN.with(|c| c.get()),
            DISTINCT_TOTAL_INPUT_LEN.with(|c| c.get()),
        )
    }

    /// Clear traversal counters on the current thread and turn timing off; call `enable` after it to measure the next word.
    pub fn reset() {
        ENABLED.with(|e| e.set(false));
        RUN_CALLS.with(|c| c.set(0));
        RUN_NANOS.with(|c| c.set(0));
        RUN_MAX_NANOS.with(|c| c.set(0));
        NONDET_CALLS.with(|c| c.set(0));
        NONDET_NANOS.with(|c| c.set(0));
        NONDET_MAX_TRAVERSED.with(|c| c.set(0));
        NONDET_TOTAL_TRAVERSED.with(|c| c.set(0));
        DET_CALLS.with(|c| c.set(0));
        DET_NANOS.with(|c| c.set(0));
        DISTINCT_CALLS.with(|c| c.set(0));
        DISTINCT_NANOS.with(|c| c.set(0));
        DISTINCT_MAX_INPUT_LEN.with(|c| c.set(0));
        DISTINCT_TOTAL_INPUT_LEN.with(|c| c.set(0));
    }
}

/// One input segment: its symbolic-feature lanes and whether it is optional (C# `Annotation.Optional`, e.g. a boundary node).
#[derive(Clone, Debug)]
pub struct Segment {
    pub lanes: Vec<u64>,
    pub optional: bool,
    /// Features whose full-mask lane was authored with every symbol, not left absent; `UseDefaults` must not replace them. Empty for nearly every segment.
    pub explicit_full: Vec<u32>,
}

impl Segment {
    pub fn new(lanes: Vec<u64>) -> Self {
        Segment {
            lanes,
            optional: false,
            explicit_full: Vec::new(),
        }
    }
    pub fn optional(lanes: Vec<u64>) -> Self {
        Segment {
            lanes,
            optional: true,
            explicit_full: Vec::new(),
        }
    }
    pub fn with_explicit_full(mut self, explicit_full: Vec<u32>) -> Self {
        self.explicit_full = explicit_full;
        self
    }
}

/// A match result (C# `FstResult`, acceptor projection). `registers` is the flat `register_count * 2` scaffold; use `Fst::get_offsets` to read a group span.
#[derive(Clone, Debug)]
pub struct FstResult {
    pub id: Option<String>,
    pub registers: Vec<Register>,
    /// Accept priority (`AcceptInfo.Priority`); `-1` when the state had no accept info.
    pub priority: i32,
    pub is_lazy: bool,
    /// Position of `NextAnnotation` (the not-yet-consumed segment index, or `n` at end).
    pub next_ann: i32,
    /// Insertion order within a start position (`FstResult.Order`).
    pub order: usize,
}

impl FstResult {
    /// C# `FstResult.Equals` (acceptor): same id and value-equal registers (output is unchanged).
    fn result_eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.registers.len() == other.registers.len()
            && self
                .registers
                .iter()
                .zip(&other.registers)
                .all(|(a, b)| a.value_eq(b))
    }
}

/// A capture register in one `u32` (bit 0 `has`, bit 1 `start`, the rest the offset): half the bytes of `Register`, which is what the visited set keeps one block of per configuration. Equality is `Register` equality on every register the traversal can build (`unset` is the single zero value).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct Packed(u32);

impl Packed {
    const UNSET: Packed = Packed(0);

    #[inline]
    fn at(offset: i32, start: bool) -> Packed {
        assert!(
            (0..1 << 30).contains(&offset),
            "register offset {offset} does not fit the packed form"
        );
        Packed(((offset as u32) << 2) | (u32::from(start) << 1) | 1)
    }

    #[inline]
    fn unpack(self) -> Register {
        if self.0 & 1 == 0 {
            Register::unset()
        } else {
            Register::at((self.0 >> 2) as i32, self.0 & 2 != 0)
        }
    }
}

/// A live traversal instance (C# `TraversalInstance`). `registers` is copy-on-write shared (`Rc<[Packed]>`, one allocation): cloning an `Inst` is an O(1) refcount bump, and `writable` deep-copies lazily, only when an arc's non-empty command range actually writes.
/// See `docs/research/pg-fst-traverse-design-notes.md` for why this representation change was needed and why it's purely a representation change.
#[derive(Clone)]
struct Inst {
    state: usize,
    ann_index: usize,
    registers: Rc<[Packed]>,
    /// This instance's `Visited` record, the parent of the keys it spawns; `None` until the visited set has switched to its compact form.
    record: Option<VisitedRecord>,
}

/// The registers of `rc` for writing: in place when this instance is their only owner, else a private copy first (the copy-on-write step `Rc::make_mut` did for `Rc<Vec<_>>`).
fn writable(rc: &mut Rc<[Packed]>) -> &mut [Packed] {
    if Rc::get_mut(rc).is_none() {
        *rc = Rc::from(&**rc);
    }
    Rc::get_mut(rc).expect("a just-created Rc is unique")
}

/// Capture state charged to the active work budget; returns whatever is still charged when the traversal ends, however it ends.
struct RetainedCharge(usize);

impl RetainedCharge {
    fn add(&mut self, bytes: usize) -> bool {
        if !crate::work::charge_retained(bytes) {
            return false;
        }
        self.0 += bytes;
        true
    }
}

impl Drop for RetainedCharge {
    fn drop(&mut self) {
        crate::work::release_retained(self.0);
    }
}

/// Start positions already seeded in one `run`; a cheap hash, since nothing here needs the standard hasher's flooding resistance.
type InitAnns = HashSet<usize, FxBuildHasher>;

/// The nondeterministic traversal's visited set (C# `traversed`): each key is a byte record holding only the registers that differ from its parent instance's record, with a full snapshot at least every `SNAPSHOT_HOPS` levels; equality reconstructs the stored registers exactly, so `insert` answers as a `HashSet` of `(state, ann_index, registers)` would.
struct Visited {
    slots: usize,
    state_width: usize,
    width: usize,
    slot_width: usize,
    chunks: Vec<Vec<u8>>,
    /// Record locations, `chunk << 32 | byte offset`; the hash is re-derived from the record's stored 32-bit key hash.
    table: hashbrown::HashTable<u64>,
    /// `seen[slot] == generation` marks a slot already compared while walking one record chain.
    seen: Vec<u32>,
    generation: u32,
    /// All ones; tests narrow it to force hash collisions, so equality is decided by the record walk.
    hash_mask: u32,
    charge: RetainedCharge,
}

/// Retained bytes below which the visited set stays a plain hash set of shared register blocks: small sets (the common case) are cheaper that way, and only a set that grows large switches to the compact records.
const SMALL_SET_BYTES: usize = 1 << 20;

/// Bytes of capture state the plain set keeps alive per entry besides the registers: the `(state, ann_index, key)` entry, its control byte, and the `Rc` counts.
const SMALL_ENTRY_BYTES: usize =
    std::mem::size_of::<(usize, usize, Rc<[Packed]>)>() + 1 + 2 * std::mem::size_of::<usize>();

/// The nondeterministic traversal's visited set. It answers every `insert` exactly as a `HashSet` of `(state, ann_index, registers)` keys would in both forms; the switch from the plain set to `Visited` copies every key across, so no decision depends on when it happens.
enum Seen {
    Small {
        set: HashSet<(usize, usize, RegKey), FxBuildHasher>,
        charge: RetainedCharge,
    },
    Large(Visited),
}

/// Plain-set key: contents are hashed and compared, so independently allocated equal blocks deduplicate.
struct RegKey(Rc<[Packed]>);

impl PartialEq for RegKey {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}
impl Eq for RegKey {}

impl std::hash::Hash for RegKey {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.0.hash(h);
    }
}

/// Outcome of `Seen::insert`.
enum Insert {
    Present,
    /// New and kept; the record is `Some` once the compact form is in use.
    New(Option<VisitedRecord>),
    /// Its storage passed the retained-capture ceiling.
    Ceiling,
}

impl Seen {
    fn new() -> Self {
        Seen::Small {
            set: HashSet::default(),
            charge: RetainedCharge(0),
        }
    }

    fn len(&self) -> usize {
        match self {
            Seen::Small { set, .. } => set.len(),
            Seen::Large(v) => v.len(),
        }
    }

    fn insert(
        &mut self,
        inst: &Inst,
        parent: Option<(VisitedRecord, &[Packed])>,
        fst_states: usize,
        n: usize,
        slots: usize,
    ) -> Insert {
        match self {
            Seen::Small { set, charge } => {
                let key = (
                    inst.state,
                    inst.ann_index,
                    RegKey(Rc::clone(&inst.registers)),
                );
                if !set.insert(key) {
                    return Insert::Present;
                }
                if !charge.add(SMALL_ENTRY_BYTES + std::mem::size_of_val(&*inst.registers)) {
                    return Insert::Ceiling;
                }
                if charge.0 >= SMALL_SET_BYTES {
                    // Grown large: carry every key into the compact form (their order cannot matter) and release the plain set's charge.
                    let mut visited = Visited::new(fst_states, n, slots);
                    let old = std::mem::take(set);
                    *charge = RetainedCharge(0);
                    for (state, ann, key) in old {
                        match visited.insert(state, ann, &key.0, None) {
                            Some(Some(_)) => {}
                            Some(None) => unreachable!("plain-set keys are distinct"),
                            None => return Insert::Ceiling,
                        }
                    }
                    *self = Seen::Large(visited);
                }
                Insert::New(None)
            }
            Seen::Large(v) => match v.insert(inst.state, inst.ann_index, &inst.registers, parent) {
                Some(Some(r)) => Insert::New(Some(r)),
                Some(None) => Insert::Present,
                None => Insert::Ceiling,
            },
        }
    }
}

/// Where a kept key's record is and how many diff records separate it from the nearest full snapshot.
#[derive(Clone, Copy)]
struct VisitedRecord {
    loc: u64,
    hops: u8,
}

/// Bound on diff records walked per comparison; a record this far from a snapshot is stored in full.
const SNAPSHOT_HOPS: u8 = 8;
const FULL: u64 = u64::MAX;
const FIRST_CHUNK_BYTES: usize = 4 << 10;
const MAX_CHUNK_BYTES: usize = 1 << 20;
/// Hash-table bytes per unit of capacity: a `u64` slot and a control byte, at hashbrown's 7/8 maximum load.
const TABLE_BYTES_PER_ENTRY: usize = (std::mem::size_of::<u64>() + 1) * 8 / 7 + 1;
/// Record prefix: key hash, then (after state and position) the parent location.
const HASH_BYTES: usize = 4;
const PARENT_BYTES: usize = 8;

/// Little-endian bytes needed to hold every value up to `max`.
fn width_for(max: u64) -> usize {
    match max {
        0..=0xFF => 1,
        0x100..=0xFFFF => 2,
        0x1_0000..=0xFFFF_FFFF => 4,
        _ => 8,
    }
}

fn read_le(bytes: &[u8], at: usize, width: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf[..width].copy_from_slice(&bytes[at..at + width]);
    u64::from_le_bytes(buf)
}

fn key_hash(state: usize, ann_index: usize, registers: &[Packed]) -> u32 {
    use std::hash::Hasher;
    let mut h = rustc_hash::FxHasher::default();
    h.write_usize(state);
    h.write_usize(ann_index);
    for r in registers {
        h.write_u32(r.0);
    }
    let x = h.finish();
    (x ^ (x >> 32)) as u32
}

/// Spreads a stored 32-bit key hash over the 64 bits hashbrown reads (bucket bits low, tag bits high).
fn table_hash(h: u32) -> u64 {
    u64::from(h).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

impl Visited {
    /// For keys whose state is below `state_count` and whose positions and register offsets are at most `n`.
    fn new(state_count: usize, n: usize, slots: usize) -> Self {
        Visited {
            slots,
            state_width: width_for(state_count.saturating_sub(1) as u64),
            // Largest `Packed` over offsets `0..=n` (start and has bits set); `ann_index <= n` is smaller.
            width: width_for(((n as u64) << 2) | 3),
            slot_width: width_for(slots as u64),
            chunks: Vec::new(),
            table: hashbrown::HashTable::new(),
            seen: vec![0; slots],
            generation: 0,
            hash_mask: u32::MAX,
            charge: RetainedCharge(0),
        }
    }

    fn len(&self) -> usize {
        self.table.len()
    }

    fn bytes_at(&self, loc: u64) -> &[u8] {
        &self.chunks[(loc >> 32) as usize][(loc & 0xFFFF_FFFF) as usize..]
    }

    fn header(&self) -> usize {
        HASH_BYTES + self.state_width + self.width
    }

    /// Appends one record (`None` when its storage passes the retained-capture ceiling).
    fn push_record(
        &mut self,
        hash: u32,
        state: usize,
        ann_index: usize,
        parent: u64,
        body: impl FnOnce(&mut Vec<u8>),
        len: usize,
    ) -> Option<u64> {
        let fits = self
            .chunks
            .last()
            .is_some_and(|c| c.capacity() - c.len() >= len);
        if !fits {
            let bytes = match self.chunks.last() {
                None => FIRST_CHUNK_BYTES,
                Some(c) => (c.capacity() * 2).min(MAX_CHUNK_BYTES),
            }
            .max(len);
            if !self.charge.add(bytes) {
                return None;
            }
            self.chunks.push(Vec::with_capacity(bytes));
        }
        let chunk_index = self.chunks.len() - 1;
        let (state_width, width) = (self.state_width, self.width);
        // A caller violating the key bounds given to `Visited::new` must fail loudly, not truncate.
        debug_assert!(
            state_width >= 8 || (state as u64) >> (8 * state_width) == 0,
            "state {state} exceeds the {state_width}-byte state width"
        );
        debug_assert!(
            width >= 8 || (ann_index as u64) >> (8 * width) == 0,
            "ann_index {ann_index} exceeds the {width}-byte width"
        );
        let chunk = &mut self.chunks[chunk_index];
        let offset = chunk.len();
        chunk.extend_from_slice(&hash.to_le_bytes());
        chunk.extend_from_slice(&(state as u64).to_le_bytes()[..state_width]);
        chunk.extend_from_slice(&(ann_index as u64).to_le_bytes()[..width]);
        chunk.extend_from_slice(&parent.to_le_bytes());
        body(chunk);
        debug_assert_eq!(chunk.len() - offset, len);
        Some(((chunk_index as u64) << 32) | offset as u64)
    }

    /// Stores `registers` in full outside the set, as the parent record of a seed.
    #[cfg(test)]
    fn root(&mut self, registers: &[Packed]) -> Option<VisitedRecord> {
        let width = self.width;
        let len = self.header() + PARENT_BYTES + self.slots * width;
        let loc = self.push_record(
            0,
            0,
            0,
            FULL,
            |c| {
                for r in registers {
                    c.extend_from_slice(&u64::from(r.0).to_le_bytes()[..width]);
                }
            },
            len,
        )?;
        Some(VisitedRecord { loc, hops: 0 })
    }

    /// Whether the record at `loc` holds exactly this key: compares the header, then walks toward the snapshot, checking each slot at its nearest recorded value.
    fn matches(
        &mut self,
        loc: u64,
        hash: u32,
        state: usize,
        ann_index: usize,
        registers: &[Packed],
    ) -> bool {
        let (sw, w, cw) = (self.state_width, self.width, self.slot_width);
        let head = self.bytes_at(loc);
        if read_le(head, 0, HASH_BYTES) != u64::from(hash)
            || read_le(head, HASH_BYTES, sw) != state as u64
            || read_le(head, HASH_BYTES + sw, w) != ann_index as u64
        {
            return false;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen.fill(0);
            self.generation = 1;
        }
        let generation = self.generation;
        let header = self.header();
        let mut at = loc;
        loop {
            let chunk = &self.chunks[(at >> 32) as usize];
            let rec = &chunk[(at & 0xFFFF_FFFF) as usize..];
            let parent = read_le(rec, header, PARENT_BYTES);
            let body = header + PARENT_BYTES;
            if parent == FULL {
                return (0..self.slots).all(|slot| {
                    self.seen[slot] == generation
                        || read_le(rec, body + slot * w, w) == u64::from(registers[slot].0)
                });
            }
            let count = read_le(rec, body, cw) as usize;
            for i in 0..count {
                let entry = body + cw + i * (cw + w);
                let slot = read_le(rec, entry, cw) as usize;
                if self.seen[slot] != generation {
                    if read_le(rec, entry + cw, w) != u64::from(registers[slot].0) {
                        return false;
                    }
                    self.seen[slot] = generation;
                }
            }
            at = parent;
        }
    }

    /// `Some(Some(record))` when the key is new (and is now kept), `Some(None)` when already present, `None` when its storage passes the retained-capture ceiling. `parent` must be the record of the instance whose registers are `parent_registers`.
    fn insert(
        &mut self,
        state: usize,
        ann_index: usize,
        registers: &[Packed],
        parent: Option<(VisitedRecord, &[Packed])>,
    ) -> Option<Option<VisitedRecord>> {
        let hash = key_hash(state, ann_index, registers) & self.hash_mask;
        let mut candidates: smallvec::SmallVec<[u64; 4]> = smallvec::SmallVec::new();
        for &loc in self.table.iter_hash(table_hash(hash)) {
            candidates.push(loc);
        }
        for loc in candidates {
            if self.matches(loc, hash, state, ann_index, registers) {
                return Some(None);
            }
        }
        let (w, cw) = (self.width, self.slot_width);
        let full_len = self.slots * w;
        // A key with no kept parent (spawned before the switch to this set) is stored in full.
        let (parent, parent_registers, changed, diff_len) = match parent {
            Some((p, pr)) => {
                let changed = (0..self.slots).filter(|&i| registers[i] != pr[i]).count();
                (p, pr, changed, cw + changed * (cw + w))
            }
            None => (
                VisitedRecord {
                    loc: FULL,
                    hops: SNAPSHOT_HOPS,
                },
                registers,
                0,
                full_len,
            ),
        };
        let full = parent.hops + 1 >= SNAPSHOT_HOPS || diff_len >= full_len;
        let len = self.header() + PARENT_BYTES + if full { full_len } else { diff_len };
        let loc = if full {
            self.push_record(
                hash,
                state,
                ann_index,
                FULL,
                |c| {
                    for r in registers {
                        c.extend_from_slice(&u64::from(r.0).to_le_bytes()[..w]);
                    }
                },
                len,
            )?
        } else {
            self.push_record(
                hash,
                state,
                ann_index,
                parent.loc,
                |c| {
                    c.extend_from_slice(&(changed as u64).to_le_bytes()[..cw]);
                    for (i, (r, p)) in registers.iter().zip(parent_registers).enumerate() {
                        if r != p {
                            c.extend_from_slice(&(i as u64).to_le_bytes()[..cw]);
                            c.extend_from_slice(&u64::from(r.0).to_le_bytes()[..w]);
                        }
                    }
                },
                len,
            )?
        };
        let chunks = &self.chunks;
        let rehash = |&l: &u64| {
            let rec = &chunks[(l >> 32) as usize][(l & 0xFFFF_FFFF) as usize..];
            table_hash(read_le(rec, 0, HASH_BYTES) as u32)
        };
        let before = self.table.capacity();
        self.table.reserve(1, rehash);
        let grown = self.table.capacity() - before;
        self.table.insert_unique(table_hash(hash), loc, rehash);
        if !self.charge.add(grown * TABLE_BYTES_PER_ENTRY) {
            return None;
        }
        Some(Some(VisitedRecord {
            loc,
            hops: if full { 0 } else { parent.hops + 1 },
        }))
    }
}

/// The traversal driver.
pub struct Transduce<'f> {
    fst: &'f Fst,
    segs: std::borrow::Cow<'f, [Segment]>,
    dir: Direction,
    start_anchor: bool,
    end_anchor: bool,
}

impl<'f> Transduce<'f> {
    /// `segs` may be owned or borrowed from a caller that keeps them: a borrow saves copying every lane vector per traversal.
    pub fn new(fst: &'f Fst, segs: impl Into<std::borrow::Cow<'f, [Segment]>>) -> Self {
        Transduce {
            fst,
            dir: fst.direction(),
            segs: segs.into(),
            start_anchor: false,
            end_anchor: false,
        }
    }

    /// Convenience constructor from raw lane vectors (no optional segments).
    pub fn from_lanes(fst: &'f Fst, segs: Vec<Vec<u64>>) -> Self {
        Transduce::new(fst, segs.into_iter().map(Segment::new).collect::<Vec<_>>())
    }

    pub fn anchored(mut self, start: bool, end: bool) -> Self {
        self.start_anchor = start;
        self.end_anchor = end;
        self
    }

    fn n(&self) -> usize {
        self.segs.len()
    }

    // --- offset helpers (linear model, direction-aware) ---------------------------------------

    /// Physical segment index for traversal index `j` (`GetNodes(_dir)` ordering): L2R is the identity; R2L reverses.
    #[inline]
    fn phys(&self, j: usize) -> usize {
        match self.dir {
            Direction::LeftToRight => j,
            Direction::RightToLeft => self.n() - 1 - j,
        }
    }

    /// The input segment at traversal index `j` (physically remapped under R2L). Callers guard `j < n`.
    #[inline]
    fn seg(&self, j: usize) -> &Segment {
        &self.segs[self.phys(j)]
    }

    /// Direction-relative start offset `Range.GetStart(_dir)`: physical left of `phys(j)` under L2R, physical right under R2L.
    #[inline]
    fn ann_start(&self, j: usize) -> i32 {
        match self.dir {
            Direction::LeftToRight => j as i32,
            Direction::RightToLeft => (self.n() - j) as i32,
        }
    }
    /// Direction-relative end offset `Range.GetEnd(_dir)`: physical right of `phys(j)` under L2R, physical left under R2L.
    #[inline]
    fn ann_end(&self, j: usize) -> i32 {
        match self.dir {
            Direction::LeftToRight => j as i32 + 1,
            Direction::RightToLeft => (self.n() - 1 - j) as i32,
        }
    }
    /// Direction-relative end-of-data offset: `GetLast(_dir).Range.GetEnd(_dir)` -- physical right end `n` under L2R, physical left end `0` under R2L.
    #[inline]
    fn end_pos(&self) -> i32 {
        match self.dir {
            Direction::LeftToRight => self.n() as i32,
            Direction::RightToLeft => 0,
        }
    }
    /// The **physical** `Range.Start` of the next annotation (or end-of-data) -- C# `FstResult.NextAnnotation`, compared via direction-agnostic `Range.CompareTo` before `ResultCompare`'s own R2L sign flip.
    #[inline]
    fn next_ann_pos(&self, j: usize) -> i32 {
        if j < self.n() {
            self.phys(j) as i32
        } else {
            match self.dir {
                Direction::LeftToRight => self.n() as i32,
                Direction::RightToLeft => 0,
            }
        }
    }

    // --- command execution (TraversalMethodBase.ExecuteCommands) -----------------------------

    fn execute_commands(registers: &mut [Packed], cmds: &[Cmd], start: Packed, end: Packed) {
        for cmd in cmds {
            if !crate::work::consume() {
                return;
            }
            let d = cmd.dest as usize;
            if cmd.src == CURRENT_POSITION {
                registers[d * 2] = start;
                registers[d * 2 + 1] = end;
            } else {
                let s = cmd.src as usize;
                registers[d * 2] = registers[s * 2];
                registers[d * 2 + 1] = registers[s * 2 + 1];
            }
        }
    }

    fn state_arcs(&self, state: usize) -> &[crate::fst::Arc] {
        let m = &self.fst.states[state];
        &self.fst.arcs[m.arc_lo as usize..m.arc_hi as usize]
    }

    fn check_input_match(&self, arc: &crate::fst::Arc, ann_index: usize) -> bool {
        ann_index < self.n()
            && self
                .fst
                .constraint(arc.constraint)
                .matches(&self.seg(ann_index).lanes)
    }

    /// C# `CheckAccepting` (TraversalMethodBase.cs:130-191), acceptor path.
    fn check_accepting(
        &self,
        ann_index: usize,
        registers: &[Packed],
        state: usize,
        cur_results: &mut Vec<FstResult>,
    ) {
        let meta = self.fst.states[state];
        if !meta.accepting {
            return;
        }
        if self.end_anchor && ann_index != self.n() {
            return;
        }
        if !crate::work::consume() {
            return;
        }
        let next_ann = self.next_ann_pos(ann_index);
        pg_cost::add_registers(registers.len());
        let mut packed = registers.to_vec();
        let fin = &self.fst.commands[meta.fin_lo as usize..meta.fin_hi as usize];
        Self::execute_commands(&mut packed, fin, Packed::UNSET, Packed::UNSET);
        let match_registers: Vec<Register> = packed.iter().map(|r| r.unpack()).collect();

        let infos = &self.fst.accept_infos[meta.accept_lo as usize..meta.accept_hi as usize];
        if infos.is_empty() {
            cur_results.push(FstResult {
                id: None,
                registers: match_registers,
                priority: -1,
                is_lazy: false,
                next_ann,
                order: cur_results.len(),
            });
        } else {
            for info in infos {
                if !crate::work::consume() {
                    return;
                }
                pg_cost::add_registers(match_registers.len());
                cur_results.push(FstResult {
                    id: info.id.clone(),
                    registers: match_registers.clone(),
                    priority: info.priority,
                    is_lazy: false,
                    next_ann,
                    order: cur_results.len(),
                });
            }
        }
    }

    /// C# `Advance` (TraversalMethodBase.cs:248-351), linear acceptor path. Returns the produced continuation instances.
    fn advance(
        &self,
        mut inst: Inst,
        arc: &crate::fst::Arc,
        optional: bool,
        cur_results: &mut Vec<FstResult>,
    ) -> Vec<Inst> {
        if !crate::work::consume() {
            return Vec::new();
        }
        let next_index = inst.ann_index + 1; // GetNextNonoverlappingAnnotationIndex, linear
        let end = self.ann_end(inst.ann_index);
        // Borrowed straight from the CSR pool; most arcs have an EMPTY command range, so `Rc::make_mut` deep-copies the shared register scaffold only when an arc genuinely writes.
        // See `docs/research/pg-fst-traverse-design-notes.md` for why finisher command ranges are unaffected.
        let arc_cmds = &self.fst.commands[arc.cmd_lo as usize..arc.cmd_hi as usize];

        if next_index < self.n() {
            let next_offset = self.ann_start(next_index);
            let next_start = true;
            let mut out: Vec<Inst> = Vec::new();

            // optional next annotation -> also spawn the "skip it" path (registers pre-command).
            if self.seg(next_index).optional {
                let mut ti = inst.clone(); // O(1): registers shared, deep-copied only on write
                ti.ann_index = next_index;
                out.extend(self.advance(ti, arc, true, cur_results));
            }

            if !arc_cmds.is_empty() {
                if Rc::strong_count(&inst.registers) > 1 {
                    pg_cost::add_registers(inst.registers.len());
                }
                let regs = writable(&mut inst.registers);
                Self::execute_commands(
                    regs,
                    arc_cmds,
                    Packed::at(next_offset, next_start),
                    Packed::at(end, false),
                );
            }
            if !optional || self.end_anchor {
                self.check_accepting(
                    next_index,
                    &inst.registers,
                    arc.target as usize,
                    cur_results,
                );
            }
            inst.state = arc.target as usize;
            inst.ann_index = next_index;
            out.push(inst);
            out
        } else {
            let next_offset = self.end_pos();
            if !arc_cmds.is_empty() {
                if Rc::strong_count(&inst.registers) > 1 {
                    pg_cost::add_registers(inst.registers.len());
                }
                let regs = writable(&mut inst.registers);
                Self::execute_commands(
                    regs,
                    arc_cmds,
                    Packed::at(next_offset, false),
                    Packed::at(end, false),
                );
            }
            self.check_accepting(self.n(), &inst.registers, arc.target as usize, cur_results);
            inst.state = arc.target as usize;
            inst.ann_index = self.n();
            vec![inst]
        }
    }

    /// C# `Initialize` (TraversalMethodBase.cs:193-246), linear model. Advances `ann_index` past the single co-starting annotation and returns seed instances.
    fn initialize(
        &self,
        ann_index: &mut usize,
        registers: &mut [Packed],
        cmds: &[Cmd],
        init_anns: &mut InitAnns,
    ) -> Vec<Inst> {
        if !crate::work::consume() {
            return Vec::new();
        }
        let mut insts: Vec<Inst> = Vec::new();
        let offset = self.ann_start(*ann_index);

        if self.start_anchor && self.seg(*ann_index).optional {
            let next_index = *ann_index + 1;
            if next_index != self.n() {
                pg_cost::add_registers(registers.len());
                let mut regs = registers.to_vec();
                let mut ni = next_index;
                insts.extend(self.initialize(&mut ni, &mut regs, cmds, init_anns));
            }
        }

        Self::execute_commands(registers, cmds, Packed::at(offset, true), Packed::UNSET);

        // linear: exactly one annotation starts at `offset`.
        if *ann_index < self.n() && !init_anns.contains(ann_index) {
            pg_cost::add_registers(registers.len());
            insts.push(Inst {
                state: self.fst.start() as usize,
                ann_index: *ann_index,
                registers: Rc::from(&*registers),
                record: None,
            });
            init_anns.insert(*ann_index);
        }
        *ann_index += 1;
        insts
    }

    fn check_accepting_start_state(
        &self,
        init_anns: &InitAnns,
        registers: &[Packed],
        cur_results: &mut Vec<FstResult>,
    ) {
        let start = self.fst.start() as usize;
        if !self.fst.states[start].accepting {
            return;
        }
        for &ann_index in init_anns {
            self.check_accepting(ann_index, registers, start, cur_results);
        }
    }

    // --- per-start-position traversal ---------------------------------------------------------

    fn traverse_from(
        &self,
        ann_index: &mut usize,
        init_registers: &mut [Packed],
        cmds: &[Cmd],
        init_anns: &mut InitAnns,
    ) -> Vec<FstResult> {
        let deterministic = self.fst.is_deterministic();
        let seeds = self.initialize(ann_index, init_registers, cmds, init_anns);
        let mut stack: Vec<Inst> = seeds;
        let mut cur_results: Vec<FstResult> = Vec::new();

        if deterministic {
            let __o2_det_start = profile::start();
            while let Some(inst) = stack.pop() {
                // `Arc` is `Copy` and `state_arcs` borrows straight from the frozen `Fst`'s CSR pool, so the slice is iterated in place instead of cloned into a fresh `Vec` every pop.
                let arcs = self.state_arcs(inst.state);
                let mut advanced = false;
                for arc in arcs {
                    if !crate::work::consume() {
                        return Vec::new();
                    }
                    if self.check_input_match(arc, inst.ann_index) {
                        for ni in self.advance(inst, arc, false, &mut cur_results) {
                            stack.push(ni);
                        }
                        advanced = true;
                        break;
                    }
                }
                let _ = advanced;
            }
            if let Some(start) = __o2_det_start {
                profile::record_det(start.elapsed().as_nanos());
            }
        } else {
            let __o2_nondet_start = profile::start();
            // Keyed on the full (state, ann_index, register contents) triple, since two insts at the same (state, ann_index) with different registers are genuinely different analyses.
            let n = self.n();
            let fst_states = self.fst.states.len();
            let slots = self.fst.register_count() as usize * 2;
            let mut traversed = Seen::new();
            let min_hops = self.fst.min_hops_to_accept();
            while let Some(inst) = stack.pop() {
                // See the deterministic branch's comment above: `Arc` is `Copy` and this slice borrows straight from the frozen `Fst`, so no per-pop clone is needed.
                let arcs = self.state_arcs(inst.state);
                for arc in arcs {
                    if !crate::work::consume() {
                        return Vec::new();
                    }
                    // frozen FSTs have no epsilon arcs; only the input-match branch fires.
                    if self.check_input_match(arc, inst.ann_index) {
                        // Min-hops-to-accept pruning: `remaining` upper-bounds arcs still takeable, and if even that many hops cannot reach an accepting state from `arc.target`, no thread through this arc can ever produce a result.
                        // See `docs/research/pg-fst-traverse-design-notes.md` for why `remaining` is a true upper bound in both traversal directions.
                        let remaining = (n - inst.ann_index - 1) as u32;
                        if min_hops[arc.target as usize] > remaining {
                            continue;
                        }
                        for mut new_inst in self.advance(inst.clone(), arc, false, &mut cur_results)
                        {
                            let parent = inst.record.map(|r| (r, &*inst.registers));
                            match traversed.insert(&new_inst, parent, fst_states, n, slots) {
                                Insert::New(record) => {
                                    new_inst.record = record;
                                    stack.push(new_inst);
                                }
                                Insert::Present => {}
                                Insert::Ceiling => return Vec::new(),
                            }
                        }
                    }
                }
            }
            if let Some(start) = __o2_nondet_start {
                profile::record_nondet(start.elapsed().as_nanos(), traversed.len());
            }
        }

        self.check_accepting_start_state(init_anns, init_registers, &mut cur_results);
        cur_results
    }

    // --- Transduce (Fst.cs:304-414) -----------------------------------------------------------

    fn run(&self, all_matches: bool) -> Vec<FstResult> {
        let __o2_start = profile::start();
        let __o2_result = self.run_inner(all_matches);
        if let Some(start) = __o2_start {
            profile::record_run(start.elapsed().as_nanos());
        }
        __o2_result
    }

    fn run_inner(&self, all_matches: bool) -> Vec<FstResult> {
        if self.n() == 0 {
            return Vec::new();
        }
        let reg_slots = self.fst.register_count() as usize * 2;
        let mut result_list: Vec<FstResult> = Vec::new();
        let mut init_anns = InitAnns::default();
        let mut ann_index = 0usize;

        while ann_index < self.n() {
            if !crate::work::consume() {
                return Vec::new();
            }
            pg_cost::add_registers(reg_slots);
            let mut init_registers = vec![Packed::UNSET; reg_slots];
            let offset = self.ann_start(ann_index);
            // initializers: dest==0 set directly; others -> cmds (Fst.cs:376-387).
            let mut cmds: Vec<Cmd> = Vec::new();
            for cmd in &self.fst.initializers {
                if cmd.dest == 0 {
                    init_registers[0] = Packed::at(offset, true);
                } else {
                    cmds.push(*cmd);
                }
            }

            let mut cur =
                self.traverse_from(&mut ann_index, &mut init_registers, &cmds, &mut init_anns);
            if crate::work::stopped() {
                return Vec::new();
            }
            if !cur.is_empty() {
                cur.sort_by(|a, b| self.result_compare(a, b));
                result_list.append(&mut cur);
                if !all_matches {
                    break;
                }
            }

            if self.start_anchor {
                break;
            }
        }

        if all_matches {
            let __o2_distinct_start = profile::start();
            let __o2_distinct_input_len = result_list.len();
            let __o2_distinct_result = distinct(result_list);
            if let Some(start) = __o2_distinct_start {
                profile::record_distinct(start.elapsed().as_nanos(), __o2_distinct_input_len);
            }
            __o2_distinct_result
        } else {
            result_list
        }
    }

    /// All matches (C# `Transduce(..., out IEnumerable results)` / `Matcher.AllMatches`).
    pub fn all_matches(&self) -> Vec<FstResult> {
        self.run(true)
    }

    /// The single best match (C# `Transduce(..., out FstResult)` → `results.First()`).
    pub fn first_match(&self) -> Option<FstResult> {
        self.run(false).into_iter().next()
    }

    /// Does the automaton accept? (any match exists).
    pub fn accepts(&self) -> bool {
        self.first_match().is_some()
    }

    /// C# `ResultCompare` (Fst.cs:416-441), minus the `Priorities` zip tiebreak: byte-parity-only machinery, removed after verifying order-invariance by A/B diffing under step-caps low enough to force the code path it existed for.
    /// See `docs/research/pg-fst-traverse-design-notes.md` for the verification methodology.
    pub fn result_compare(&self, x: &FstResult, y: &FstResult) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        let c = x.priority.cmp(&y.priority);
        if c != Ordering::Equal {
            return c;
        }
        // -x.NextAnnotation.CompareTo(y.NextAnnotation)
        let mut c = y.next_ann.cmp(&x.next_ann);
        if self.fst.direction() == Direction::RightToLeft {
            c = c.reverse();
        }
        if self.fst.is_deterministic() && x.is_lazy {
            c = c.reverse();
        }
        if c != Ordering::Equal {
            return c;
        }
        x.order.cmp(&y.order)
    }
}

/// Hash a `FstResult` consistently with `FstResult::result_eq`: the `id`, the register count, and each register in **canonicalized** form, excluding `priority`/`is_lazy`/`next_ann`/`order` exactly as `result_eq` does.
/// See `docs/research/pg-fst-traverse-design-notes.md` for why canonicalizing removes a reliance on `Register::unset()` being the only `has == false` constructor.
fn result_hash<H: std::hash::Hasher>(r: &FstResult, hasher: &mut H) {
    use std::hash::Hash;
    r.id.hash(hasher);
    r.registers.len().hash(hasher);
    for reg in &r.registers {
        reg.has.hash(hasher);
        if reg.has {
            reg.offset.hash(hasher);
            reg.start.hash(hasher);
        }
    }
}

/// C# `Enumerable.Distinct` over `FstResult.Equals`, order-preserving (first occurrence wins), hash-backed rather than the original's linear pairwise scan (`O(n × kept)`, which reached 59-81% of wall-clock on pathological Amharic words).
/// See `docs/research/pg-fst-traverse-design-notes.md` for why which duplicate survives matters and how equality semantics stay bit-for-bit identical.
fn distinct(results: Vec<FstResult>) -> Vec<FstResult> {
    use std::collections::HashMap;
    use std::hash::{BuildHasher, BuildHasherDefault};

    // Keyed by the already-computed `result_hash`, so the map's own hasher is the identity.
    let mut buckets: HashMap<u64, Vec<usize>, BuildHasherDefault<PassThroughHasher>> =
        HashMap::with_capacity_and_hasher(results.len(), BuildHasherDefault::default());
    let mut out: Vec<FstResult> = Vec::new();
    for r in results {
        if !crate::work::consume() {
            return Vec::new();
        }
        let mut hasher = FxBuildHasher.build_hasher();
        result_hash(&r, &mut hasher);
        let bucket = buckets
            .entry(std::hash::Hasher::finish(&hasher))
            .or_default();
        if bucket.iter().all(|&i| !out[i].result_eq(&r)) {
            bucket.push(out.len());
            out.push(r);
        }
    }
    out
}

/// A `Hasher` for keys that are already well-mixed `u64` hashes: `finish` returns the last `u64` written.
#[derive(Default)]
struct PassThroughHasher(u64);

impl std::hash::Hasher for PassThroughHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(b);
        }
    }

    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests;
