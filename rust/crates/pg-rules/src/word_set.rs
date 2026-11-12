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

/// Positions of words that live in a `Vec` elsewhere, found by a 32-bit hash the caller computes and confirmed by the caller's own exact comparison against the stored word: 8 bytes an entry, and a hash collision can never merge two words.
#[derive(Default)]
pub struct PositionIndex {
    table: HashTable<(u32, u32)>,
}

impl PositionIndex {
    /// The position stored under `hash` for which `is_match` holds.
    pub fn find(&self, hash: u32, is_match: impl Fn(usize) -> bool) -> Option<usize> {
        self.table
            .find(spread(hash), |&(stored, at)| {
                stored == hash && is_match(at as usize)
            })
            .map(|&(_, at)| at as usize)
    }

    /// Records position `at` under `hash`. The caller has checked no stored position matches.
    pub fn insert(&mut self, hash: u32, at: usize) {
        self.table
            .insert_unique(spread(hash), (hash, position_u32(at)), |&(stored, _)| {
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

    /// The words in first-insertion order, each moved out as it is reached; see [`Releasing`].
    pub fn into_releasing(self) -> Releasing {
        Releasing::new(self.words)
    }
}

/// Below this many words a buffer is not worth giving back early.
const RELEASE_MIN_WORDS: usize = 4096;

/// Moves the words of a buffer out front to back and gives the buffer's memory back as it empties, where `Vec::into_iter` holds the whole buffer until the last word is taken.
pub struct Releasing<T = Word>(Rest<T>);

enum Rest<T> {
    /// A buffer too small to be worth giving back early: the plain iterator.
    Whole(std::vec::IntoIter<T>),
    /// The words still to yield, last first, so the next one pops off the end and the buffer can shrink behind it.
    Reversed(Vec<T>),
}

impl<T> Releasing<T> {
    pub fn new(mut words: Vec<T>) -> Self {
        if words.len() < RELEASE_MIN_WORDS {
            return Releasing(Rest::Whole(words.into_iter()));
        }
        words.reverse();
        Releasing(Rest::Reversed(words))
    }
}

impl<T> Iterator for Releasing<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        match &mut self.0 {
            Rest::Whole(words) => words.next(),
            Rest::Reversed(rest) => {
                let word = rest.pop()?;
                // Halving keeps the copying linear in the words yielded.
                if rest.capacity() >= RELEASE_MIN_WORDS && rest.len() <= rest.capacity() / 2 {
                    rest.shrink_to_fit();
                }
                Some(word)
            }
        }
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

    fn held(releasing: &Releasing) -> usize {
        match &releasing.0 {
            Rest::Whole(words) => words.as_slice().len(),
            Rest::Reversed(rest) => rest.capacity(),
        }
    }

    #[test]
    fn releasing_yields_first_insertion_order_and_shrinks() {
        let n = RELEASE_MIN_WORDS as u32 * 4;
        let words: Vec<Word> = (0..n).map(word).collect();
        let mut releasing = Releasing::new(words);
        let full = held(&releasing);
        for expected in 0..n {
            let w = releasing.next().expect("one word per input word");
            assert_eq!(w.morphs[0].allomorph, AllomorphId(expected));
            if expected == n - n / 8 {
                assert!(
                    held(&releasing) <= full / 4,
                    "a buffer seven eighths consumed has been given back at least twice"
                );
            }
        }
        assert!(releasing.next().is_none());
    }

    #[test]
    fn releasing_a_small_buffer_yields_first_insertion_order() {
        let mut releasing = Releasing::new((0..5).map(word).collect());
        assert!(matches!(releasing.0, Rest::Whole(_)));
        for expected in 0..5 {
            let w = releasing.next().expect("one word per input word");
            assert_eq!(w.morphs[0].allomorph, AllomorphId(expected));
        }
        assert!(releasing.next().is_none());
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
        *first.applied_phonological_rules_mut() = vec![(StratumId(0), PRuleId(10))];
        let mut second = word(1);
        *second.applied_phonological_rules_mut() = vec![(StratumId(0), PRuleId(20))];
        assert_eq!(first.dedup_key(), second.dedup_key());
        let mut set = WordSet::default();
        assert_eq!(set.insert(first), (0, true));
        assert_eq!(set.insert(second), (0, false));
        let kept = set.into_vec();
        assert_eq!(kept.len(), 1);
        assert_eq!(
            kept[0].applied_phonological_rules(),
            vec![(StratumId(0), PRuleId(10))]
        );
    }
}
