//! How a template battery holds its outputs until the stratum's fold takes them: an output is the battery's input with slot rules unapplied, so it is stored as the few fields that changes and rebuilt from the input when the fold takes it, in `crate::word_set::WordSet`'s order and with its dedup.

use std::hash::BuildHasher;
use std::rc::Rc;

use pg_featstruct::FeatureStruct;
use pg_shape::Shape;
use rustc_hash::FxBuildHasher;

use crate::trail::Trail;
use crate::word::{estimate_word_bytes, FinalTemplateState, KeyFields, Word, WordFlags, WordKey};
use crate::word_set::{fold_hash, PositionIndex};

/// One battery output, stored against the battery's input.
pub(crate) enum BatteryWord {
    /// Equal to the input in every field but these.
    Delta(DeltaWord),
    /// Differs from the input in some other field.
    Whole(Box<Word>),
}

/// The fields of a battery output that the battery's input does not already hold.
pub(crate) struct DeltaWord {
    shape: Shape,
    syn_fs: FeatureStruct,
    trail: Trail,
    mrule_app_index: i32,
    final_template_state: FinalTemplateState,
}

fn same_source(a: &Option<Rc<Word>>, b: &Option<Rc<Word>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

impl DeltaWord {
    /// `w` as a delta against `input`, or `None` when rebuilding from `input` would not give `w` back exactly.
    fn of(w: &Word, input: &Word) -> Option<Self> {
        // Every field is named, so a new `Word` field fails to compile here until it is compared or stored.
        let Word {
            shape,
            stratum,
            syn_fs,
            real_fs,
            mpr,
            morphs,
            extras,
            non_head_app_index,
            trail,
            analysis_allomorph,
            mrule_app_index,
            root_allomorph,
            obligatory,
            flags,
            source,
            alternatives,
            trace,
        } = w;
        let WordFlags {
            is_partial,
            is_last_applied_rule_final,
            final_template_state,
        } = flags;
        let rebuilds = *stratum == input.stratum
            && *real_fs == input.real_fs
            && *mpr == input.mpr
            && *morphs == input.morphs
            // Words have no equality of their own, so only the absence of non-heads on both sides is taken as equal.
            && extras.non_heads().is_empty()
            && input.non_heads().is_empty()
            && *non_head_app_index == input.non_head_app_index
            && *analysis_allomorph == input.analysis_allomorph
            && extras.applied_realizational_rules() == input.applied_realizational_rules()
            && extras.applied_phonological_rules() == input.applied_phonological_rules()
            && *root_allomorph == input.root_allomorph
            && extras.root_runtime_id() == input.root_runtime_id()
            && *obligatory == input.obligatory
            && *is_partial == input.flags.is_partial
            && *is_last_applied_rule_final == input.flags.is_last_applied_rule_final
            && same_source(source, &input.source)
            && alternatives.is_empty()
            && input.alternatives.is_empty()
            && *trace == input.trace;
        rebuilds.then(|| DeltaWord {
            shape: shape.clone(),
            syn_fs: syn_fs.clone(),
            trail: trail.clone(),
            mrule_app_index: *mrule_app_index,
            final_template_state: *final_template_state,
        })
    }

    fn key_fields<'a>(&'a self, input: &'a Word) -> KeyFields<'a> {
        KeyFields {
            shape: &self.shape,
            trail: &self.trail,
            mrule_app_index: self.mrule_app_index,
            final_template_state: self.final_template_state,
            rest: input,
        }
    }
}

impl BatteryWord {
    /// The stored form of `w`, an output of the battery run on `input`.
    pub(crate) fn of(w: &Word, input: &Word) -> Self {
        match DeltaWord::of(w, input) {
            Some(delta) => BatteryWord::Delta(delta),
            None => BatteryWord::Whole(Box::new(w.clone())),
        }
    }

    /// The output as it was produced. `input` is the word the battery ran on.
    pub(crate) fn into_word(self, input: &Word) -> Word {
        match self {
            // Built field by field: cloning `input` whole would copy its shape, features and trail only to drop them.
            BatteryWord::Delta(delta) => Word {
                shape: delta.shape,
                stratum: input.stratum,
                syn_fs: delta.syn_fs,
                real_fs: input.real_fs.clone(),
                mpr: input.mpr,
                morphs: input.morphs.clone(),
                extras: input.extras.clone(),
                non_head_app_index: input.non_head_app_index,
                trail: delta.trail,
                analysis_allomorph: input.analysis_allomorph,
                mrule_app_index: delta.mrule_app_index,
                root_allomorph: input.root_allomorph,
                obligatory: input.obligatory.clone(),
                flags: WordFlags {
                    is_partial: input.flags.is_partial,
                    is_last_applied_rule_final: input.flags.is_last_applied_rule_final,
                    final_template_state: delta.final_template_state,
                },
                source: input.source.clone(),
                alternatives: input.alternatives.clone(),
                trace: input.trace,
            },
            BatteryWord::Whole(w) => *w,
        }
    }

    /// The byte estimate of the output as a whole word, the unit the frontier diagnostic counts in.
    pub(crate) fn estimated_bytes(&self, input: &Word) -> usize {
        match self {
            BatteryWord::Delta(delta) => {
                let mut w = input.clone();
                w.shape = delta.shape.clone();
                w.syn_fs = delta.syn_fs.clone();
                w.trail = delta.trail.clone();
                estimate_word_bytes(&w)
            }
            BatteryWord::Whole(w) => estimate_word_bytes(w),
        }
    }

    /// The output's `dedup_key()`.
    pub(crate) fn dedup_key(&self, input: &Word) -> WordKey {
        match self {
            BatteryWord::Delta(delta) => WordKey::of(&delta.key_fields(input)),
            BatteryWord::Whole(w) => w.dedup_key(),
        }
    }

    /// Whether the output's `dedup_key()` equals `key`, read in place.
    fn matches(&self, key: &WordKey, input: &Word) -> bool {
        match self {
            BatteryWord::Delta(delta) => key.matches_fields(&delta.key_fields(input)),
            BatteryWord::Whole(w) => key.matches(w),
        }
    }

    pub(crate) fn syn_fs(&self) -> &FeatureStruct {
        match self {
            BatteryWord::Delta(delta) => &delta.syn_fs,
            BatteryWord::Whole(w) => &w.syn_fs,
        }
    }

    pub(crate) fn syn_fs_mut(&mut self) -> &mut FeatureStruct {
        match self {
            BatteryWord::Delta(delta) => &mut delta.syn_fs,
            BatteryWord::Whole(w) => &mut w.syn_fs,
        }
    }
}

/// A `WordKey`-deduped set of the outputs of a battery run on one input, in first-insertion order.
pub(crate) struct BatterySet<'i> {
    input: &'i Word,
    index: PositionIndex,
    words: Vec<BatteryWord>,
}

impl<'i> BatterySet<'i> {
    pub(crate) fn new(input: &'i Word) -> Self {
        BatterySet {
            input,
            index: PositionIndex::default(),
            words: Vec::new(),
        }
    }

    fn hash(key: &WordKey) -> u32 {
        fold_hash(FxBuildHasher.hash_one(key))
    }

    /// The position of the output whose key equals `key`.
    pub(crate) fn position(&self, key: &WordKey) -> Option<usize> {
        self.index.find(Self::hash(key), |at| {
            self.words[at].matches(key, self.input)
        })
    }

    /// Inserts `word`, whose key is `key`. The caller has checked `key` is absent.
    pub(crate) fn push_new(&mut self, key: &WordKey, word: BatteryWord) {
        self.index.insert(Self::hash(key), self.words.len());
        self.words.push(word);
    }

    /// Inserts `w` unless an equal-keyed output is present.
    pub(crate) fn insert(&mut self, w: &Word) {
        let key = w.dedup_key();
        if self.position(&key).is_none() {
            self.push_new(&key, BatteryWord::of(w, self.input));
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.words.len()
    }

    pub(crate) fn get_mut(&mut self, at: usize) -> &mut BatteryWord {
        &mut self.words[at]
    }

    /// The outputs in first-insertion order, in their stored form.
    pub(crate) fn into_stored(self) -> Vec<BatteryWord> {
        self.words
    }

    /// The outputs in first-insertion order.
    #[cfg(test)]
    pub(crate) fn into_vec(self) -> Vec<Word> {
        let input = self.input;
        self.words.into_iter().map(|w| w.into_word(input)).collect()
    }
}

#[cfg(test)]
mod tests;
