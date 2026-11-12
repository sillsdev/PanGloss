//! How a canonical word holds the candidates folded into it (C# `Word.Alternatives`).
//!
//! A folded candidate reached the canonical's analysis state by another route, so nearly all of it repeats the canonical: Mbugwe `mindeshi` at a 200M work cap folds 285,384 candidates into 94,010 canonicals, and they differ from their canonicals in the allomorph trail, sometimes the rule order, and nothing else. Such a candidate is stored as those trails plus its syntactic feature structure and rebuilt from the canonical when synthesis expands it. A candidate that differs anywhere else is held whole, so the stored form never loses a field.

use std::rc::Rc;

use pg_featstruct::FeatureStruct;
use pg_grammar_model::model::{AllomorphId, MRuleId};

use crate::word::{estimate_word_bytes, Word, WordKey};

/// Stands for `None` in a stored trail. A word with a real id of this value is held whole instead.
const NO_ID: u32 = u32::MAX;

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
    /// `mrule_apps` then `allomorph_apps`, half the length each.
    apps: Rc<[u32]>,
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
    fn of(w: &Word, canonical: &Word) -> Option<Self> {
        // Every field is named, so a new `Word` field fails to compile here until it is compared or stored.
        let Word {
            shape,
            stratum,
            syn_fs,
            real_fs,
            mpr,
            morphs,
            non_heads,
            non_head_app_index,
            mrule_apps,
            allomorph_apps,
            analysis_allomorph,
            applied_realizational_rules,
            applied_phonological_rules,
            mrule_app_index,
            root_allomorph,
            root_runtime_id,
            obligatory,
            unapplied_rule_counts,
            flags,
            source,
            alternatives,
            trace,
        } = w;
        let rebuilds = *shape == canonical.shape
            && *stratum == canonical.stratum
            && *real_fs == canonical.real_fs
            && *mpr == canonical.mpr
            && *morphs == canonical.morphs
            && non_heads.is_empty()
            && canonical.non_heads.is_empty()
            && *non_head_app_index == canonical.non_head_app_index
            && mrule_apps.len() == allomorph_apps.len()
            && *analysis_allomorph == canonical.analysis_allomorph
            && *applied_realizational_rules == canonical.applied_realizational_rules
            // The stats collector merges this list into a stored canonical after the fold, so only the empty list rebuilds.
            && applied_phonological_rules.is_empty()
            && *mrule_app_index == canonical.mrule_app_index
            && *root_allomorph == canonical.root_allomorph
            && *root_runtime_id == canonical.root_runtime_id
            && *obligatory == canonical.obligatory
            && *unapplied_rule_counts == canonical.unapplied_rule_counts
            && *flags == canonical.flags
            && same_source(source, &canonical.source)
            && alternatives.is_empty()
            && *trace == canonical.trace;
        if !rebuilds {
            return None;
        }
        let rules = mrule_apps.iter().map(|id| id.map_or(Some(NO_ID), real_id));
        let allomorphs = allomorph_apps
            .iter()
            .map(|id| id.map_or(Some(NO_ID), |id| real_id_of(id.0)));
        let apps: Option<Rc<[u32]>> = rules.chain(allomorphs).collect();
        Some(TrailAlternative {
            apps: apps?,
            syn_fs: syn_fs.clone(),
        })
    }

    fn rules(&self) -> &[u32] {
        &self.apps[..self.apps.len() / 2]
    }

    fn allomorphs(&self) -> &[u32] {
        &self.apps[self.apps.len() / 2..]
    }

    fn rebuild(&self, canonical: &Word) -> Word {
        let mut w = canonical.clone_without_alternatives();
        w.mrule_apps = self
            .rules()
            .iter()
            .map(|&id| (id != NO_ID).then_some(MRuleId(id)))
            .collect();
        w.allomorph_apps = self
            .allomorphs()
            .iter()
            .map(|&id| (id != NO_ID).then_some(AllomorphId(id)))
            .collect();
        w.syn_fs = self.syn_fs.clone();
        w.applied_phonological_rules.clear();
        w
    }
}

fn real_id(id: MRuleId) -> Option<u32> {
    real_id_of(id.0)
}

fn real_id_of(id: u32) -> Option<u32> {
    (id != NO_ID).then_some(id)
}

impl Alternative {
    /// The stored form of `w`, a candidate folded into `canonical`.
    pub fn of(w: Word, canonical: &Word) -> Self {
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
        match self {
            Alternative::Trail(trail) => {
                canonical.obligatory == obligatory
                    && key.matches_with_trail(canonical, trail.rules(), trail.allomorphs())
            }
            Alternative::Whole(w) => w.obligatory == obligatory && key.matches(w),
        }
    }

    /// Approximate heap bytes this alternative holds.
    pub fn estimated_bytes(&self) -> usize {
        match self {
            Alternative::Trail(trail) => {
                std::mem::size_of::<Alternative>() + std::mem::size_of_val(&*trail.apps)
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
