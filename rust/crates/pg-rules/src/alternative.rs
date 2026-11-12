//! How a canonical word holds the candidates folded into it (C# `Word.Alternatives`).
//!
//! A folded candidate reached the canonical's analysis state by another route, so nearly all of it repeats the canonical: Mbugwe `mindeshi` at a 200M work cap folds 285,384 candidates into 94,010 canonicals, and they differ from their canonicals in the allomorph trail, sometimes the rule order, and nothing else. Such a candidate is stored as those trails plus its syntactic feature structure and rebuilt from the canonical when synthesis expands it. A candidate that differs anywhere else is held whole, so the stored form never loses a field.

use std::rc::Rc;

use pg_featstruct::FeatureStruct;

use crate::trail::Trail;
use crate::word::{estimate_word_bytes, KeyFields, Word, WordKey};

/// `size_of::<Alternative>()` when the byte estimates were calibrated.
const LEGACY_ALTERNATIVE_BYTES: usize = 32;

/// One candidate folded into a canonical word.
#[derive(Clone, Debug)]
pub enum Alternative {
    /// Equal to its canonical in every field but the rule trail, the allomorph trail and the syntactic feature structure.
    Trail(TrailAlternative),
    /// Differs from its canonical in some other field.
    Whole(Rc<Word>),
}

/// The fields of a folded candidate that its canonical does not already hold.
#[derive(Clone, Debug)]
pub struct TrailAlternative {
    /// The folded word's own trail, sharing its steps with the search.
    trail: Trail,
    syn_fs: FeatureStruct,
}

fn same_source(a: &Option<Rc<Word>>, b: &Option<Rc<Word>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

impl TrailAlternative {
    /// `w` as a trail against `canonical`, or `None` when rebuilding from `canonical` would not give `w` back exactly.
    fn of(w: &Word, fields: &KeyFields<'_>) -> Option<Self> {
        // The canonical may be stored as a difference: its shape, trail, rule index and template state are `fields`' own, and `canonical` holds the rest.
        let canonical = fields.rest;
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
        let rebuilds = *shape == *fields.shape
            && *stratum == canonical.stratum
            && *real_fs == canonical.real_fs
            && *mpr == canonical.mpr
            && *morphs == canonical.morphs
            && extras.non_heads().is_empty()
            && canonical.non_heads().is_empty()
            && *non_head_app_index == canonical.non_head_app_index
            && *analysis_allomorph == canonical.analysis_allomorph
            && extras.applied_realizational_rules() == canonical.applied_realizational_rules()
            // The stats collector merges this list into a stored canonical after the fold, so only the empty list rebuilds.
            && extras.applied_phonological_rules().is_empty()
            && *mrule_app_index == fields.mrule_app_index
            && *root_allomorph == canonical.root_allomorph
            && extras.root_runtime_id() == canonical.root_runtime_id()
            && *obligatory == canonical.obligatory
            && flags.is_partial == canonical.flags.is_partial
            && flags.is_last_applied_rule_final == canonical.flags.is_last_applied_rule_final
            && flags.final_template_state == fields.final_template_state
            && same_source(source, &canonical.source)
            && alternatives.is_empty()
            && *trace == canonical.trace;
        // The stored trail carries its own unapplication counts, so they need no comparison with the canonical's.
        rebuilds.then(|| TrailAlternative {
            trail: trail.clone(),
            syn_fs: syn_fs.clone(),
        })
    }

    fn rebuild(&self, canonical: &Word) -> Word {
        let mut w = canonical.clone_without_alternatives();
        w.trail = self.trail.clone();
        w.syn_fs = self.syn_fs.clone();
        if !w.applied_phonological_rules().is_empty() {
            w.applied_phonological_rules_mut().clear();
        }
        w
    }
}

impl Alternative {
    /// The stored form of `w`, a candidate folded into `canonical`.
    pub fn of(w: Word, canonical: &Word) -> Self {
        Self::of_fields(w, &KeyFields::of(canonical))
    }

    /// `of` against a canonical read in place.
    pub(crate) fn of_fields(w: Word, canonical: &KeyFields<'_>) -> Self {
        match TrailAlternative::of(&w, canonical) {
            Some(trail) => Alternative::Trail(trail),
            None => Alternative::Whole(Rc::new(w)),
        }
    }

    /// The folded candidate as it arrived. `canonical` is the word holding this alternative, or any clone of it.
    pub fn word(&self, canonical: &Word) -> Rc<Word> {
        match self {
            Alternative::Trail(trail) => Rc::new(trail.rebuild(canonical)),
            Alternative::Whole(w) => w.clone(),
        }
    }

    pub fn syn_fs(&self) -> &FeatureStruct {
        match self {
            Alternative::Trail(trail) => &trail.syn_fs,
            Alternative::Whole(w) => &w.syn_fs,
        }
    }

    /// Whether the folded candidate's `dedup_key()` equals `key` and its obligatory features equal `obligatory`, read in place.
    pub fn matches(
        &self,
        key: &WordKey,
        obligatory: &[pg_featstruct::FeatId],
        canonical: &Word,
    ) -> bool {
        self.matches_fields(key, obligatory, &KeyFields::of(canonical))
    }

    /// `matches` against a canonical read in place.
    pub(crate) fn matches_fields(
        &self,
        key: &WordKey,
        obligatory: &[pg_featstruct::FeatId],
        canonical: &KeyFields<'_>,
    ) -> bool {
        match self {
            Alternative::Trail(trail) => {
                canonical.rest.obligatory == obligatory
                    && key.matches_fields(&KeyFields {
                        trail: &trail.trail,
                        ..*canonical
                    })
            }
            Alternative::Whole(w) => w.obligatory == obligatory && key.matches(w),
        }
    }

    /// Approximate heap bytes this alternative holds.
    pub fn estimated_bytes(&self) -> usize {
        match self {
            Alternative::Trail(trail) => {
                LEGACY_ALTERNATIVE_BYTES + trail.trail.len() * 2 * std::mem::size_of::<u32>()
            }
            Alternative::Whole(w) => estimate_word_bytes(w),
        }
    }
}

/// A canonical word's alternatives. Clones of the canonical share one list.
#[derive(Clone, Debug, Default)]
pub struct Alternatives(Option<Rc<Vec<Alternative>>>);

impl Alternatives {
    pub fn len(&self) -> usize {
        self.0.as_ref().map_or(0, |list| list.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Alternative> {
        self.0.as_deref().map_or(&[][..], Vec::as_slice).iter()
    }

    pub fn get(&self, at: usize) -> Option<&Alternative> {
        self.0.as_ref()?.get(at)
    }

    pub fn push(&mut self, alternative: Alternative) {
        Rc::make_mut(self.0.get_or_insert_with(Rc::default)).push(alternative);
    }

    pub fn clear(&mut self) {
        self.0 = None;
    }
}

#[cfg(test)]
mod tests;
