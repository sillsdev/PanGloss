//! A `WordKey`-deduped set of words that iterates in first-insertion order.
//!
//! The order is part of the output contract: C# `Morpher` collects matches in a `HashSet<Word>` (and its stratum
//! rules in lists), which enumerates in insertion order while nothing is removed. A hash map's own iteration order
//! depends on the hasher and table layout, so it is neither C#'s order nor documented; this type pins first-seen
//! order and the first word wins a key collision, as with `entry().or_insert`.

use rustc_hash::FxHashMap;

use crate::word::{Word, WordKey};

#[derive(Default)]
pub struct WordSet {
    index: FxHashMap<WordKey, usize>,
    words: Vec<Word>,
}

impl WordSet {
    /// Inserts `make()` unless `key` is present. Returns the stored word's position and whether it was new.
    pub fn insert_with(&mut self, key: WordKey, make: impl FnOnce() -> Word) -> (usize, bool) {
        if let Some(&at) = self.index.get(&key) {
            return (at, false);
        }
        let at = self.words.len();
        self.words.push(make());
        self.index.insert(key, at);
        (at, true)
    }

    /// Inserts `word` unless an equal-keyed word is present.
    pub fn insert(&mut self, word: Word) -> (usize, bool) {
        self.insert_with(word.dedup_key(), || word)
    }

    pub fn position(&self, key: &WordKey) -> Option<usize> {
        self.index.get(key).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Word> {
        self.words.iter()
    }

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
    use pg_grammar_model::model::{AllomorphId, MorphemeId, StratumId};
    use pg_shape::ShapeBuilder;

    fn word(allomorph: u32) -> Word {
        let mut w = Word::new(ShapeBuilder::new().finish(), StratumId(0));
        w.morphs = vec![MorphRecord::new(AllomorphId(allomorph), MorphemeId(1), 0)];
        w
    }

    #[test]
    fn iterates_in_first_insertion_order_and_first_word_wins() {
        // Scrambled ids so insertion order differs from key order and from any plausible hash order.
        let ids: Vec<u32> = (0..200).map(|i| (i * 7919) % 211).collect();
        let mut set = WordSet::default();
        for &id in &ids {
            set.insert(word(id));
            set.insert(word(id));
        }
        let got: Vec<u32> = set
            .into_vec()
            .iter()
            .map(|w| w.morphs[0].allomorph.0)
            .collect();
        assert_eq!(got, ids);
    }
}
