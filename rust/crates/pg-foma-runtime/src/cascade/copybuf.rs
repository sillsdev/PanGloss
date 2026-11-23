//! The limit and the buffer logic of a copy layer, limited by the surface (plan S.8, "Copy,
//! limited by the surface").
//!
//! No copy layer exists yet (W10 builds it), so this is the interface and the rule only: a buffer
//! of token classes that is checked against the source and that falls to `Free`, the superset,
//! whenever the surface cannot vouch for it. Nothing here touches a layer.

use std::collections::BTreeMap;

/// A class of tokens: same spellings and same pattern-node sets of the copied part.
pub type ClassId = u32;

/// Partition of tokens into classes, built once per copied part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassTable {
    class: Vec<ClassId>,
    count: u32,
}

impl ClassTable {
    /// Two tokens share a class exactly when their keys are equal. `keys[t]` is the key of token
    /// `t` (its set of spellings and node sets); rows of a closure never appear here.
    pub fn from_keys<K: Ord>(keys: &[K]) -> Self {
        let mut ids: BTreeMap<&K, ClassId> = BTreeMap::new();
        let class = keys
            .iter()
            .map(|k| {
                let next = ids.len() as ClassId;
                *ids.entry(k).or_insert(next)
            })
            .collect();
        ClassTable {
            class,
            count: ids.len() as u32,
        }
    }

    pub fn class_of(&self, token: u32) -> ClassId {
        self.class[token as usize]
    }

    /// `K_c`: the number of classes (printed per part by KC-S0).
    pub fn classes(&self) -> u32 {
        self.count
    }
}

/// Longest buffer for a word of `word_len` units: `n + 2` entries (two units may be inserted).
pub const fn buffer_cap(word_len: usize) -> usize {
    word_len + 2
}

/// What the buffer holds for one copy on one path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CopyBuffer {
    /// The classes of the tokens that reached the surface, in order.
    Classes(Vec<ClassId>),
    /// The surface cannot vouch for the copy: the relation is the free one of 2.4.4.
    Free,
}

impl CopyBuffer {
    pub fn new() -> Self {
        CopyBuffer::Classes(Vec::new())
    }

    pub fn is_free(&self) -> bool {
        matches!(self, CopyBuffer::Free)
    }

    /// Record the class of a token that has just reached the surface. Past `buffer_cap(word_len)`
    /// entries the buffer becomes `Free`.
    pub fn record(&mut self, class: ClassId, word_len: usize) {
        if let CopyBuffer::Classes(v) = self {
            if v.len() >= buffer_cap(word_len) {
                *self = CopyBuffer::Free;
            } else {
                v.push(class);
            }
        }
    }

    /// The chain of a token ended above `Units` (deleted, or spelled as nothing), or a layer
    /// between the copy and `Units` held the token back: the buffer is `Free` from here on.
    pub fn chain_ended_above_surface(&mut self) {
        *self = CopyBuffer::Free;
    }

    /// The check when the source token at buffer position `pos` passes: some token of its image
    /// (`image_classes`, the classes of `img_c(x)`; the token's own class for a plain copy) is the
    /// class stored there. A superset of equality, which is the sound direction. A `Free` buffer
    /// passes everything; a stored class is never compared past its length.
    pub fn check_source(&self, pos: usize, image_classes: &[ClassId]) -> bool {
        match self {
            CopyBuffer::Free => true,
            CopyBuffer::Classes(v) => v.get(pos).is_some_and(|c| image_classes.contains(c)),
        }
    }

    /// Read-then-emit copies: the stored classes' length after the source has passed.
    pub fn len(&self) -> Option<usize> {
        match self {
            CopyBuffer::Free => None,
            CopyBuffer::Classes(v) => Some(v.len()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == Some(0)
    }
}

impl Default for CopyBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_group_equal_keys_and_count_them() {
        let t = ClassTable::from_keys(&["a", "b", "a", "c", "b"]);
        assert_eq!(t.classes(), 3);
        assert_eq!(t.class_of(0), t.class_of(2));
        assert_eq!(t.class_of(1), t.class_of(4));
        assert_ne!(t.class_of(0), t.class_of(1));
    }

    #[test]
    fn buffer_holds_n_plus_two_then_is_free() {
        let n = 3;
        let mut b = CopyBuffer::new();
        for i in 0..buffer_cap(n) {
            b.record(i as u32, n);
            assert!(!b.is_free(), "entry {i} fits");
        }
        assert_eq!(b.len(), Some(5));
        b.record(9, n);
        assert!(b.is_free(), "the n+3rd entry makes it Free");
    }

    #[test]
    fn a_chain_that_ends_above_the_surface_frees_the_buffer() {
        let mut b = CopyBuffer::new();
        b.record(1, 4);
        b.chain_ended_above_surface();
        assert!(b.is_free());
        assert!(b.check_source(0, &[7]), "Free passes every source token");
    }

    #[test]
    fn source_must_match_the_recorded_class_and_a_mismatch_kills() {
        let mut b = CopyBuffer::new();
        b.record(1, 4);
        b.record(2, 4);
        assert!(b.check_source(0, &[1]));
        assert!(
            b.check_source(1, &[5, 2]),
            "any image token in the class passes"
        );
        assert!(!b.check_source(1, &[1]), "wrong class is cut");
        assert!(
            !b.check_source(2, &[1, 2]),
            "past the record nothing is vouched for"
        );
    }

    #[test]
    fn recording_after_free_stays_free() {
        let mut b = CopyBuffer::Free;
        b.record(1, 2);
        assert!(b.is_free());
    }
}
