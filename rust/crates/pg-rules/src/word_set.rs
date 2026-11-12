//! A `WordKey`-deduped set of words that iterates in first-insertion order.
//!
//! Deterministic first-seen order, the same container semantics as C#'s `HashSet<Word>` at MaxDegreeOfParallelism 1 (C#'s parallel path has no defined order). A hash map's own iteration order depends on the hasher and table layout. The first word wins a key collision, as with `entry().or_insert`.

use hashbrown::HashTable;
use rustc_hash::FxBuildHasher;
use std::hash::BuildHasher;

use crate::word::{Word, WordKey};

/// Folds a key's 64-bit hash to the 32 bits an index entry keeps.
#[inline]
pub(crate) fn fold_hash(h: u64) -> u32 {
    (h >> 32) as u32 ^ h as u32
}

/// The 64-bit table hash for a stored 32-bit one; both halves carry it so hashbrown's tag bits (the top seven) vary.
#[inline]
fn spread(h: u32) -> u64 {
    u64::from(h) << 32 | u64::from(h)
}

#[inline]
fn position_u32(at: usize) -> u32 {
    u32::try_from(at).expect("a word index holds fewer than 2^32 words")
}

/// Dedup index over words that live in a `Vec` elsewhere: each entry is a 32-bit hash and a position (8 bytes), where a `HashMap<WordKey, usize>` kept a second full copy of every word's key. Equality is still exact: a hash match is confirmed by [`WordKey::matches`] against the stored word, so a collision can never merge two words.
#[derive(Default)]
pub struct WordIndex {
    table: HashTable<(u32, u32)>,
}

impl WordIndex {
    /// The position of the word whose key equals `key`; `word_at` reads the words this index was built over.
    pub fn find<'a>(&self, key: &WordKey, word_at: impl Fn(usize) -> &'a Word) -> Option<usize> {
        let h = fold_hash(FxBuildHasher.hash_one(key));
        self.table
            .find(spread(h), |&(stored, at)| {
                stored == h && key.matches(word_at(at as usize))
            })
            .map(|&(_, at)| at as usize)
    }

    /// Records that the word at `at` has `key`. The caller has checked `key` is absent.
    pub fn insert(&mut self, key: &WordKey, at: usize) {
        let h = fold_hash(FxBuildHasher.hash_one(key));
        self.table
            .insert_unique(spread(h), (h, position_u32(at)), |&(stored, _)| {
                spread(stored)
            });
    }
}

#[derive(Default)]
pub struct WordSet {
    index: WordIndex,
    words: Vec<Word>,
}

impl WordSet {
    /// Inserts `make()` unless `key` is present. Returns the stored word's position and whether it was new. `make()` must produce a word whose `dedup_key()` is `key`.
    pub fn insert_with(&mut self, key: WordKey, make: impl FnOnce() -> Word) -> (usize, bool) {
        if let Some(at) = self.position(&key) {
            return (at, false);
        }
        let at = self.words.len();
        self.words.push(make());
        self.index.insert(&key, at);
        (at, true)
    }

    /// Inserts `word` unless an equal-keyed word is present.
    pub fn insert(&mut self, word: Word) -> (usize, bool) {
        self.insert_with(word.dedup_key(), || word)
    }

    pub fn position(&self, key: &WordKey) -> Option<usize> {
        self.index.find(key, |at| &self.words[at])
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// The word at `at`, in first-insertion order.
    pub fn get(&self, at: usize) -> &Word {
        &self.words[at]
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Word> {
        self.words.iter()
    }

    /// The word at `at`. Edits must leave its `dedup_key()` alone, since the index reads the stored word.
    pub fn get_mut(&mut self, at: usize) -> &mut Word {
        &mut self.words[at]
    }

    /// The words in first-insertion order.
    pub fn into_vec(self) -> Vec<Word> {
        self.words
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::word::MorphRecord;
    use pg_grammar_model::model::{AllomorphId, MorphemeId, PRuleId, StratumId};
    use pg_shape::ShapeBuilder;

    fn word(allomorph: u32) -> Word {
        let mut w = Word::new(ShapeBuilder::new().finish(), StratumId(0));
        w.morphs = vec![MorphRecord::new(AllomorphId(allomorph), MorphemeId(1), 0)];
        w
    }

    #[test]
    fn iterates_in_first_insertion_order() {
        // Scrambled ids so insertion order differs from key order and from any plausible hash order.
        let ids: Vec<u32> = (0..200).map(|i| (i * 7919) % 211).collect();
        let mut set = WordSet::default();
        for &id in &ids {
            set.insert(word(id));
        }
        let got: Vec<u32> = set
            .into_vec()
            .iter()
            .map(|w| w.morphs[0].allomorph.0)
            .collect();
        assert_eq!(got, ids);
    }

    #[test]
    fn first_word_wins_a_key_collision() {
        let mut first = word(1);
        first.applied_phonological_rules = vec![(StratumId(0), PRuleId(10))];
        let mut second = word(1);
        second.applied_phonological_rules = vec![(StratumId(0), PRuleId(20))];
        assert_eq!(first.dedup_key(), second.dedup_key());
        let mut set = WordSet::default();
        assert_eq!(set.insert(first), (0, true));
        assert_eq!(set.insert(second), (0, false));
        let kept = set.into_vec();
        assert_eq!(kept.len(), 1);
        assert_eq!(
            kept[0].applied_phonological_rules,
            vec![(StratumId(0), PRuleId(10))]
        );
    }
}
