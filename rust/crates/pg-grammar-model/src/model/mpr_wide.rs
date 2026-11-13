//! Per-grammar tables of wide `MprSet` word lists (handle = `generation << 38 | index`); see ADR 0017.

use std::cell::Cell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

const INDEX_BITS: u32 = 38;
const INDEX_MASK: u64 = (1 << INDEX_BITS) - 1;
const GENERATION_MASK: u32 = (1 << 24) - 1;
/// Bytes of stored lists the ambient table accepts from deserialisation.
const AMBIENT_STORED_BYTES: usize = 1 << 20;
const DEAD: &str = "a wide MprSet outlived the grammar that owns its table";

struct Table {
    generation: u32,
    lists: Vec<Box<[u64]>>,
    indexes: HashMap<Box<[u64]>, u64>,
    list_bytes: usize,
}

/// Live tables, generation 0 (the ambient one, never freed) first. A handful at most, so lookup is a scan; one lock covers reads.
static TABLES: RwLock<Vec<Table>> = RwLock::new(Vec::new());
static NEXT_GENERATION: AtomicU32 = AtomicU32::new(1);

thread_local! {
    static CURRENT: Cell<Option<u32>> = const { Cell::new(None) };
}

fn read() -> RwLockReadGuard<'static, Vec<Table>> {
    TABLES.read().unwrap_or_else(PoisonError::into_inner)
}

fn write() -> RwLockWriteGuard<'static, Vec<Table>> {
    let mut tables = TABLES.write().unwrap_or_else(PoisonError::into_inner);
    if tables.is_empty() {
        tables.push(Table::new(0));
    }
    tables
}

impl Table {
    fn new(generation: u32) -> Table {
        Table {
            generation,
            lists: Vec::new(),
            indexes: HashMap::new(),
            list_bytes: 0,
        }
    }
}

fn find(tables: &[Table], generation: u32) -> Option<&Table> {
    tables.iter().find(|t| t.generation == generation)
}

/// A grammar's table of wide MPR sets; dropping it frees every set interned into it.
pub struct MprTable(u32);

impl std::fmt::Debug for MprTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MprTable(generation {}, {} sets)", self.0, self.len())
    }
}

impl Default for MprTable {
    fn default() -> Self {
        Self::new()
    }
}

impl MprTable {
    pub fn new() -> Self {
        let mut tables = write();
        let generation = loop {
            let g = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed) & GENERATION_MASK;
            if g != 0 && find(&tables, g).is_none() {
                break g;
            }
        };
        tables.push(Table::new(generation));
        MprTable(generation)
    }

    /// Makes this the table a new wide set with no wide operand is built into, on this thread, until the guard drops.
    pub fn enter(&self) -> MprScope {
        MprScope {
            prev: CURRENT.with(|c| c.replace(Some(self.0))),
            _not_send: PhantomData,
        }
    }

    pub fn generation(&self) -> u32 {
        self.0
    }

    /// Distinct wide sets interned so far.
    pub fn len(&self) -> usize {
        find(&read(), self.0).map_or(0, |t| t.lists.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Heap bytes the table holds, counting capacity: list bodies once, the two index tables.
    pub fn retained_bytes(&self) -> usize {
        let tables = read();
        let Some(t) = find(&tables, self.0) else {
            return 0;
        };
        let bodies: usize = t.lists.iter().map(|l| l.len() * 8).sum();
        let item = size_of::<Box<[u64]>>();
        bodies + t.lists.capacity() * item + t.indexes.capacity() * (item + size_of::<u64>() + 1)
    }
}

impl Drop for MprTable {
    fn drop(&mut self) {
        write().retain(|t| t.generation != self.0);
    }
}

/// Restores the previous current table when dropped.
pub struct MprScope {
    prev: Option<u32>,
    _not_send: PhantomData<*const ()>,
}

impl Drop for MprScope {
    fn drop(&mut self) {
        CURRENT.with(|c| c.set(self.prev));
    }
}

/// Whether a table with this generation is still alive.
pub fn is_live(generation: u32) -> bool {
    find(&read(), generation).is_some()
}

pub(super) fn generation_of(handle: u64) -> u32 {
    (handle >> INDEX_BITS) as u32 & GENERATION_MASK
}

/// Interns a list into one table and returns its handle; stored data may not grow the ambient table past its limits.
pub(super) fn intern(generation: Option<u32>, words: &[u64], stored: bool) -> Option<u64> {
    let generation = generation.or_else(|| CURRENT.with(Cell::get)).unwrap_or(0);
    let tag = u64::from(generation) << INDEX_BITS;
    if let Some(index) = find(&read(), generation).and_then(|t| t.indexes.get(words)) {
        return Some(tag | index);
    }
    let mut tables = write();
    let table = tables
        .iter_mut()
        .find(|t| t.generation == generation)
        .expect(DEAD);
    if let Some(index) = table.indexes.get(words) {
        return Some(tag | index);
    }
    if stored && generation == 0 && table.list_bytes + words.len() * 8 > AMBIENT_STORED_BYTES {
        return None;
    }
    table.list_bytes += words.len() * 8;
    let index = table.lists.len() as u64;
    assert!(index <= INDEX_MASK, "wide MprSet table is full");
    table.lists.push(words.into());
    table.indexes.insert(words.into(), index);
    Some(tag | index)
}

fn list(tables: &[Table], handle: u64) -> Option<&[u64]> {
    let table = find(tables, generation_of(handle))?;
    table
        .lists
        .get((handle & INDEX_MASK) as usize)
        .map(|l| &**l)
}

/// Runs `f` on a handle's list in place, or returns `None` when its grammar is gone; `f` must not intern.
pub(super) fn try_with_words<R>(handle: u64, f: impl FnOnce(&[u64]) -> R) -> Option<R> {
    let tables = read();
    list(&tables, handle).map(f)
}

pub(super) fn with_words<R>(handle: u64, f: impl FnOnce(&[u64]) -> R) -> R {
    try_with_words(handle, f).expect(DEAD)
}

/// [`with_words`] for two handles, which may name different tables.
pub(super) fn with_two<R>(a: u64, b: u64, f: impl FnOnce(&[u64], &[u64]) -> R) -> R {
    let tables = read();
    f(list(&tables, a).expect(DEAD), list(&tables, b).expect(DEAD))
}
