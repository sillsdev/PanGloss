//! How a stratum's analysis holds its output words, and how the parse holds them until lexical lookup has read each one.
//!
//! Every output of a stratum is the stratum's input with rules unapplied, so nearly all of it repeats that input: Mbugwe `mindeshi` at a 1G work cap holds 257,264 output words that differ from the input in shape, syntactic features, trail and template state only. Such a word is stored as those fields plus its alternatives and rebuilt from the input when it is read. A word that differs anywhere else is held whole, so the stored form never loses a field.

use pg_featstruct::FeatureStruct;

use crate::alternative::{Alternative, Alternatives};
use crate::battery::DeltaWord;
use crate::word::{KeyFields, Word, WordKey};
use crate::word_set::{Releasing, WordIndex};

/// One output word, stored against the input of the stratum analysis that made it.
enum Stored {
    /// Equal to its base in every field but these.
    Delta(Delta),
    /// Differs from its base in some other field.
    Whole(Box<Word>),
}

struct Delta {
    fields: DeltaWord,
    /// The word's own alternatives; its base holds none.
    alternatives: Alternatives,
    /// The position of the base in `Canonicals::bases`.
    base: u32,
}

/// A `WordKey`-deduped set of stratum output words in first-insertion order, each stored against the input of the analysis that made it.
pub struct Canonicals {
    /// The inputs the stored words are differences from: one for each analysis whose words the set took.
    bases: Vec<Word>,
    words: Vec<Stored>,
    index: WordIndex,
    memory: pg_fst::work::WordMemory,
}

impl Default for Canonicals {
    fn default() -> Self {
        Self {
            bases: Vec::new(),
            words: Vec::new(),
            index: WordIndex::default(),
            memory: pg_fst::work::WordMemory::active(),
        }
    }
}

impl Canonicals {
    /// An empty set whose words will be stored against `base`.
    pub(crate) fn of_base(base: Word) -> Self {
        Canonicals {
            bases: vec![base],
            ..Canonicals::default()
        }
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// The word every field outside the stored difference is read from.
    fn rest(&self, at: usize) -> &Word {
        match &self.words[at] {
            Stored::Delta(delta) => &self.bases[delta.base as usize],
            Stored::Whole(w) => w,
        }
    }

    /// The key fields of the word at `at`, read in place.
    pub(crate) fn key_fields(&self, at: usize) -> KeyFields<'_> {
        match &self.words[at] {
            Stored::Delta(delta) => delta.fields.key_fields(&self.bases[delta.base as usize]),
            Stored::Whole(w) => KeyFields::of(w),
        }
    }

    pub(crate) fn syn_fs(&self, at: usize) -> &FeatureStruct {
        match &self.words[at] {
            Stored::Delta(delta) => delta.fields.syn_fs(),
            Stored::Whole(w) => &w.syn_fs,
        }
    }

    /// Syntactic features are no part of a `WordKey`, so widening them leaves the index valid.
    pub(crate) fn syn_fs_mut(&mut self, at: usize) -> &mut FeatureStruct {
        match &mut self.words[at] {
            Stored::Delta(delta) => delta.fields.syn_fs_mut(),
            Stored::Whole(w) => &mut w.syn_fs,
        }
    }

    pub(crate) fn alternatives(&self, at: usize) -> &Alternatives {
        match &self.words[at] {
            Stored::Delta(delta) => &delta.alternatives,
            Stored::Whole(w) => &w.alternatives,
        }
    }

    /// Folds `w` into the word at `at` as an alternative (`Word::fold_alternative`).
    pub(crate) fn fold_alternative(&mut self, at: usize, w: Word) {
        if !self.memory.charge(crate::word::retained_word_bytes(&w)) {
            return;
        }
        let alternative = Alternative::of_fields(w, &self.key_fields(at));
        match &mut self.words[at] {
            Stored::Delta(delta) => delta.alternatives.push(alternative),
            Stored::Whole(canonical) => canonical.alternatives.push(alternative),
        }
    }

    /// The position of the word whose key equals `key`.
    pub(crate) fn position(&self, key: &WordKey) -> Option<usize> {
        self.index
            .find_by(key, |at| key.matches_fields(&self.key_fields(at)))
    }

    /// Appends `w`, whose key is `key`, stored against the first base. The caller has checked `key` is absent.
    pub(crate) fn push_new(&mut self, key: &WordKey, w: Word) {
        if !self.memory.charge(crate::word::retained_word_bytes(&w)) {
            return;
        }
        let stored = match DeltaWord::of(&w, &self.bases[0]) {
            Some(fields) => Stored::Delta(Delta {
                fields,
                alternatives: Alternatives::default(),
                base: 0,
            }),
            None => Stored::Whole(Box::new(w)),
        };
        self.index.insert(key, self.words.len());
        self.words.push(stored);
    }

    fn to_word(&self, stored: &Stored) -> Word {
        match stored {
            Stored::Delta(delta) => {
                let mut w = delta
                    .fields
                    .clone()
                    .into_word(&self.bases[delta.base as usize]);
                w.alternatives = delta.alternatives.clone();
                w
            }
            Stored::Whole(w) => (**w).clone(),
        }
    }

    /// The word at `at`, in first-insertion order, as it was stored.
    pub fn word(&self, at: usize) -> Word {
        self.to_word(&self.words[at])
    }

    /// The words in first-insertion order, each built as it is asked for.
    pub fn words(&self) -> impl Iterator<Item = Word> + '_ {
        self.words.iter().map(|stored| self.to_word(stored))
    }

    /// The words in first-insertion order.
    pub fn into_words(self) -> Vec<Word> {
        let Canonicals {
            bases,
            words,
            memory,
            ..
        } = self;
        let _memory = memory;
        words
            .into_iter()
            .map(|stored| match stored {
                Stored::Delta(delta) => {
                    let mut w = delta.fields.into_word(&bases[delta.base as usize]);
                    w.alternatives = delta.alternatives;
                    w
                }
                Stored::Whole(w) => *w,
            })
            .collect()
    }

    /// The stratum of each word, in first-insertion order.
    pub fn strata(&self) -> impl Iterator<Item = pg_grammar_model::model::StratumId> + '_ {
        (0..self.words.len()).map(|at| self.rest(at).stratum)
    }

    /// Adds the words of `other`, the set of one analysis, whose key this set does not hold, in `other`'s order; the first word wins a key. With `merge_phonological_rules`, a word whose key is held adds its applied phonological rules to the held word's.
    pub fn absorb(&mut self, other: Canonicals, merge_phonological_rules: bool) {
        let Canonicals {
            bases,
            words,
            memory,
            ..
        } = other;
        self.memory.absorb(memory);
        let first_base = self.bases.len();
        let rebase = u32::try_from(first_base).expect("a set holds fewer than 2^32 bases");
        self.bases.extend(bases);
        let mut based = false;
        // The buffer is given back as this set takes its words, so the two are not both held whole.
        for mut stored in Releasing::new(words) {
            if let Stored::Delta(delta) = &mut stored {
                delta.base += rebase;
            }
            let fields = match &stored {
                Stored::Delta(delta) => delta.fields.key_fields(&self.bases[delta.base as usize]),
                Stored::Whole(w) => KeyFields::of(w),
            };
            let key = WordKey::of(&fields);
            match self.position(&key) {
                Some(at) => {
                    if merge_phonological_rules {
                        let rules = fields.rest.applied_phonological_rules().to_vec();
                        self.merge_phonological_rules(at, &rules);
                    }
                }
                None => {
                    based |= matches!(stored, Stored::Delta(_));
                    self.index.insert(&key, self.words.len());
                    self.words.push(stored);
                }
            }
        }
        // An analysis none of whose words is stored against its input leaves no input behind.
        if !based {
            self.bases.truncate(first_base);
        }
    }

    /// Adds `rules` to the applied phonological rules of the word at `at`. Only the stats collector reads the list.
    fn merge_phonological_rules(
        &mut self,
        at: usize,
        rules: &[(
            pg_grammar_model::model::StratumId,
            pg_grammar_model::model::PRuleId,
        )],
    ) {
        for rule in rules {
            if self.rest(at).applied_phonological_rules().contains(rule) {
                continue;
            }
            // The list is no part of a stored difference, so the word is held whole from here on.
            if let Stored::Delta(_) = &self.words[at] {
                self.words[at] = Stored::Whole(Box::new(self.word(at)));
            }
            if let Stored::Whole(w) = &mut self.words[at] {
                w.applied_phonological_rules_mut().push(*rule);
            }
        }
    }
}

#[cfg(test)]
mod tests;
