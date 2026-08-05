//! Hash-consing interner for frozen feature structures (plan §5.3, §6.2).
//!
//! Interning is what makes the memo key and every gating comparison an integer compare: two
//! structurally-equal frozen FS values get the same `crate::FsId`, so `ValueEquals` (deep
//! structural equality, the cost center of the C# engine) is paid **once** at intern time and
//! never again. Grammar-tier FS (syntactic/head/foot, arc constraints) are interned at load and
//! frozen for the process; per-parse interners live in the parse arena and die with it (§6.2).
//!
//! Interning happens at grammar-load / shape-freeze time, never inside the traversal loop
//! (plan §9: "hash only at memo boundaries"), so the map stores an owned clone of each value
//! rather than using the raw-entry API — simplicity over a cold-path micro-optimization.

use crate::FsId;
use hashbrown::HashMap;
use std::hash::Hash;

/// A hash-cons interner mapping structurally-equal values to a dense `u32` id.
///
/// Generic over the interned value `V` so the same machinery serves both feature-structure
/// representations (flat lane vectors and DAG structs). Ids are assigned densely from 0 in
/// first-seen order and are stable for the interner's lifetime.
#[derive(Debug, Default, Clone)]
pub struct Interner<V: Hash + Eq + Clone> {
    values: Vec<V>,
    index: HashMap<V, u32>,
}

impl<V: Hash + Eq + Clone> Interner<V> {
    /// A fresh, empty interner.
    pub fn new() -> Self {
        Interner {
            values: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// An interner pre-sized for `cap` distinct values (avoids reallocation during load).
    pub fn with_capacity(cap: usize) -> Self {
        Interner {
            values: Vec::with_capacity(cap),
            index: HashMap::with_capacity(cap),
        }
    }

    /// Intern `value`, returning its stable id. Structurally-equal values always map to the
    /// same id; the first insertion assigns the next dense id.
    pub fn intern(&mut self, value: V) -> FsId {
        if let Some(&id) = self.index.get(&value) {
            return FsId(id);
        }
        let id = self.values.len() as u32;
        self.values.push(value.clone());
        self.index.insert(value, id);
        FsId(id)
    }

    /// The value behind an id previously returned by [`intern`](Self::intern).
    #[inline]
    pub fn get(&self, id: FsId) -> &V {
        &self.values[id.0 as usize]
    }

    /// The value behind an id, or `None` if the id was never assigned by this interner.
    #[inline]
    pub fn try_get(&self, id: FsId) -> Option<&V> {
        self.values.get(id.0 as usize)
    }

    /// Number of distinct interned values.
    #[inline]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether nothing has been interned yet.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterate `(FsId, &V)` over all interned values in id order.
    pub fn iter(&self) -> impl Iterator<Item = (FsId, &V)> {
        self.values
            .iter()
            .enumerate()
            .map(|(i, v)| (FsId(i as u32), v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedups_and_assigns_dense_ids() {
        let mut it: Interner<Vec<u64>> = Interner::new();
        let a = it.intern(vec![0b101, 0b011]);
        let b = it.intern(vec![0b111]);
        let a2 = it.intern(vec![0b101, 0b011]); // structurally equal to `a`
        assert_eq!(a, FsId(0));
        assert_eq!(b, FsId(1));
        assert_eq!(a2, a, "equal values must intern to the same id");
        assert_eq!(it.len(), 2);
    }

    #[test]
    fn round_trips_values() {
        let mut it: Interner<Vec<u64>> = Interner::with_capacity(4);
        let id = it.intern(vec![1, 2, 3]);
        assert_eq!(it.get(id), &vec![1, 2, 3]);
        assert_eq!(it.try_get(FsId(99)), None);
    }

    #[test]
    fn iter_is_id_ordered() {
        let mut it: Interner<u32> = Interner::new();
        it.intern(10);
        it.intern(20);
        it.intern(10);
        let collected: Vec<_> = it.iter().map(|(id, v)| (id.0, *v)).collect();
        assert_eq!(collected, vec![(0, 10), (1, 20)]);
    }
}
