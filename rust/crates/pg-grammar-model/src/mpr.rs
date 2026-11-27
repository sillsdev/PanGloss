//! Canonical MPR sets: one inline word for ordinary grammars, shared words for wider inventories.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Dense index into a grammar's authored MPR inventory.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct MprId(pub u32);

impl TryFrom<usize> for MprId {
    type Error = std::num::TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        u32::try_from(value).map(Self)
    }
}

#[derive(Clone)]
enum Storage {
    Inline(u64),
    Shared(Arc<[u64]>),
}

/// A canonical MPR set with inline low IDs and immutable shared wide words.
#[derive(Clone)]
pub struct MprSet(Storage);

impl Default for MprSet {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl MprSet {
    pub const EMPTY: Self = Self(Storage::Inline(0));

    /// Constructs the legacy low-word set, without exposing the representation to consumers.
    pub const fn from_bits(bits: u64) -> Self {
        Self(Storage::Inline(bits))
    }

    #[inline]
    fn words(&self) -> &[u64] {
        match &self.0 {
            Storage::Inline(bits) => std::slice::from_ref(bits),
            Storage::Shared(words) => words,
        }
    }

    fn from_words(mut words: Vec<u64>) -> Self {
        while words.last() == Some(&0) {
            words.pop();
        }
        match words.len() {
            0 => Self::EMPTY,
            1 => Self::from_bits(words[0]),
            _ => Self(Storage::Shared(words.into())),
        }
    }

    #[inline]
    pub fn insert(&mut self, id: MprId) {
        if let Storage::Inline(bits) = &mut self.0 {
            if id.0 < 64 {
                *bits |= 1u64 << id.0;
                return;
            }
        }
        self.insert_wide(id);
    }

    #[cold]
    fn insert_wide(&mut self, id: MprId) {
        if self.contains(id) {
            return;
        }
        let index = id.0 as usize / 64;
        let mut words = self.words().to_vec();
        words.resize(words.len().max(index + 1), 0);
        words[index] |= 1u64 << (id.0 % 64);
        *self = Self::from_words(words);
    }

    #[inline]
    pub fn contains(&self, id: MprId) -> bool {
        self.words()
            .get(id.0 as usize / 64)
            .is_some_and(|bits| bits & (1u64 << (id.0 % 64)) != 0)
    }

    /// C# `HashSet.Overlaps`.
    #[inline]
    pub fn overlaps(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Storage::Inline(a), Storage::Inline(b)) => a & b != 0,
            _ => self
                .words()
                .iter()
                .zip(other.words())
                .any(|(a, b)| a & b != 0),
        }
    }

    /// C# `HashSet.IsSubsetOf`.
    #[inline]
    pub fn is_subset_of(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Storage::Inline(a), Storage::Inline(b)) => a & !b == 0,
            _ => self
                .words()
                .iter()
                .enumerate()
                .all(|(i, a)| a & !other.words().get(i).copied().unwrap_or(0) == 0),
        }
    }

    #[inline]
    pub fn union(&self, other: &Self) -> Self {
        match (&self.0, &other.0) {
            (Storage::Inline(a), Storage::Inline(b)) => Self::from_bits(a | b),
            _ => self.combine(other, |a, b| a | b),
        }
    }

    #[inline]
    pub fn intersection(&self, other: &Self) -> Self {
        match (&self.0, &other.0) {
            (Storage::Inline(a), Storage::Inline(b)) => Self::from_bits(a & b),
            _ => self.combine(other, |a, b| a & b),
        }
    }

    /// Members of this set absent from `other`; never complements bits outside this set.
    #[inline]
    pub fn difference(&self, other: &Self) -> Self {
        match (&self.0, &other.0) {
            (Storage::Inline(a), Storage::Inline(b)) => Self::from_bits(a & !b),
            _ => self.combine(other, |a, b| a & !b),
        }
    }

    #[cold]
    fn combine(&self, other: &Self, op: impl Fn(u64, u64) -> u64) -> Self {
        let a = self.words();
        let b = other.words();
        Self::from_words(
            (0..a.len().max(b.len()))
                .map(|i| {
                    op(
                        a.get(i).copied().unwrap_or(0),
                        b.get(i).copied().unwrap_or(0),
                    )
                })
                .collect(),
        )
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        match self.0 {
            Storage::Inline(bits) => bits == 0,
            Storage::Shared(_) => false,
        }
    }

    /// A compound restriction is satisfied by any shared feature, or by no restriction.
    #[inline]
    pub fn compound_match(&self, stem: &Self) -> bool {
        self.is_empty() || self.overlaps(stem)
    }

    /// Feature IDs in ascending declaration order, with no width-dependent truncation.
    pub fn iter(&self) -> impl Iterator<Item = MprId> + '_ {
        self.words().iter().enumerate().flat_map(|(index, &word)| {
            let mut pending = word;
            std::iter::from_fn(move || {
                if pending == 0 {
                    return None;
                }
                let bit = pending.trailing_zeros();
                pending &= pending - 1;
                Some(MprId(index as u32 * 64 + bit))
            })
        })
    }

    /// Conservative allocation charge per clone, including Arc counters.
    pub fn heap_bytes(&self) -> usize {
        match &self.0 {
            Storage::Inline(_) => 0,
            Storage::Shared(words) => {
                std::mem::size_of_val(words.as_ref()) + 2 * std::mem::size_of::<usize>()
            }
        }
    }
}

impl PartialEq for MprSet {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Storage::Inline(a), Storage::Inline(b)) => a == b,
            (Storage::Shared(a), Storage::Shared(b)) => Arc::ptr_eq(a, b) || a == b,
            _ => false, // Constructors canonicalize every low-only result back to Inline.
        }
    }
}

impl Eq for MprSet {}

impl Hash for MprSet {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.0 {
            Storage::Inline(bits) => bits.hash(state), // Preserve the legacy low-set hash.
            Storage::Shared(words) => words.hash(state),
        }
    }
}

impl fmt::Debug for MprSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter().map(|id| id.0)).finish()
    }
}

impl fmt::Binary for MprSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Storage::Inline(bits) => fmt::Binary::fmt(bits, f),
            Storage::Shared(words) => {
                if f.alternate() {
                    f.write_str("0b")?;
                }
                let (last, rest) = words
                    .split_last()
                    .expect("wide sets have at least two words");
                write!(f, "{last:b}")?;
                for word in rest.iter().rev() {
                    write!(f, "{word:064b}")?;
                }
                Ok(())
            }
        }
    }
}

impl serde::Serialize for MprSet {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut record = serializer.serialize_struct("MprSet", 2)?;
        record.serialize_field("version", &1u8)?;
        record.serialize_field("ids", &self.iter().map(|id| id.0).collect::<Vec<_>>())?;
        record.end()
    }
}

/// Untrusted wire IDs, retained without allocating a bitmap until the inventory is known.
#[derive(Clone, Debug)]
pub struct MprIds(Vec<u32>);

impl MprIds {
    /// Rejects every out-of-inventory ID before allocating dense storage.
    pub fn into_set(self, inventory_len: usize) -> Result<MprSet, MprId> {
        if let Some(&id) = self.0.iter().find(|&&id| id as usize >= inventory_len) {
            return Err(MprId(id));
        }
        Ok(self.into_set_unchecked())
    }

    fn into_set_unchecked(self) -> MprSet {
        let mut set = MprSet::EMPTY;
        for id in self.0 {
            set.insert(MprId(id));
        }
        set
    }
}

impl<'de> serde::Deserialize<'de> for MprIds {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Indexed {
            version: u8,
            ids: Vec<u32>,
        }
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Input {
            Legacy(u64),
            Indexed(Indexed),
        }
        match Input::deserialize(deserializer)? {
            Input::Legacy(bits) => Ok(Self(
                MprSet::from_bits(bits).iter().map(|id| id.0).collect(),
            )),
            Input::Indexed(value) => {
                if value.version != 1 {
                    return Err(serde::de::Error::custom("unsupported MPR set version"));
                }
                Ok(Self(value.ids))
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for MprSet {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        MprIds::deserialize(deserializer).map(MprIds::into_set_unchecked)
    }
}
