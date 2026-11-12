//! Order-independent identity of an analysis-cascade node (C# `AnalysisStateKey`,
//! AnalysisStateKey.cs:29-116).
//!
//! Its one consumer is `AnalyzerConfig::merge_equivalent` — C# `Morpher.MergeEquivalentAnalyses`,
//! which folds candidates reaching the same analysis state into one canonical word plus
//! `Word::alternatives`. It lived in the now-removed `pg-memo` crate while the analysis memo also
//! keyed on it (`docs/divergences/045-memoization-removed.md`); nothing else reads it.

use std::collections::BTreeMap;

use pg_featstruct::FeatureStruct;
use pg_grammar_model::model::{AllomorphId, MRuleId, MorphemeId, StratumId};
use pg_shape::Shape;

use crate::word::{runtime_id, FinalTemplateState, MorphStatus, Word};

/// The source-bearing morphology history that distinguishes equal-shaped analysis arrivals.
///
/// Deliberately omits procedural fields such as `passed_over`, which do not identify source
/// morphology and would split otherwise identical arrivals.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct MorphHistoryKey {
    pub allomorph: AllomorphId,
    pub morpheme: MorphemeId,
    pub order: u32,
    pub status: MorphStatus,
    pub runtime_identity: Option<String>,
}

/// Fields cover every analysis-side rule input plus the source-bearing morphology history.
///
/// Deliberately **excludes** procedural fields such as `passed_over` and the mrule trail's order:
/// the order-independent `rule_counts` multiset is what lets two arrivals differing only in
/// unapplication order fold together, while morph history keeps two source-distinct arrivals apart.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct AnalysisStateKey {
    shape: Shape,
    stratum: StratumId,
    syntactic_fs: FeatureStruct,
    realizational_fs: FeatureStruct,
    non_head_count: u32,
    /// Per-rule unapplication multiset; sorted keys make order-independent arrivals compare alike.
    rule_counts: BTreeMap<MRuleId, u32>,
    /// Source-bearing history prevents equal shapes with different trails folding together.
    morph_history: Vec<MorphHistoryKey>,
    /// Final-template interleaving state.
    state: FinalTemplateState,
}

impl AnalysisStateKey {
    /// Build a key from a word's already-extracted components, with `rule_counts` saturated at each rule's `max_apps`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        shape: Shape,
        stratum: StratumId,
        syntactic_fs: FeatureStruct,
        realizational_fs: FeatureStruct,
        non_head_count: u32,
        rule_counts: BTreeMap<MRuleId, u32>,
        state: FinalTemplateState,
        morph_history: Vec<MorphHistoryKey>,
    ) -> Self {
        AnalysisStateKey {
            shape,
            stratum,
            syntactic_fs,
            realizational_fs,
            non_head_count,
            rule_counts,
            morph_history,
            state,
        }
    }

    /// Whether this is the key [`key_of`] gives for `w` taken with syntactic feature structure `syntactic_fs`, decided from the word's own fields without building its key. The stratum's visited set holds positions of canonical words and compares through this, so it keeps no second copy of a canonical's shape, counts and morph history. `syntactic_fs` is passed apart because a stored canonical's own is widened after its key was taken.
    pub fn matches_word(
        &self,
        w: &Word,
        syntactic_fs: &FeatureStruct,
        max_apps: &impl Fn(MRuleId) -> u32,
    ) -> bool {
        // Every field is named, so a new key field fails to compile here until it is compared.
        let AnalysisStateKey {
            shape,
            stratum,
            syntactic_fs: key_syntactic_fs,
            realizational_fs,
            non_head_count,
            rule_counts,
            morph_history,
            state,
        } = self;
        *stratum == w.stratum
            && *state == w.flags.final_template_state
            && *non_head_count == w.non_heads().len() as u32
            && *shape == w.shape
            && key_syntactic_fs == syntactic_fs
            && *realizational_fs == w.real_fs
            && morph_history.len() == w.morphs.len()
            && morph_history.iter().zip(&w.morphs).all(|(key, morph)| {
                key.allomorph == morph.allomorph
                    && key.morpheme == morph.morpheme
                    && key.order == morph.order
                    && key.status == morph.status
                    && key.runtime_identity.as_deref() == runtime_id(morph.runtime_root.as_deref())
            })
            // The key holds each rule's count saturated at its limit and omits a rule saturated to zero. Every keyed rule must have that count on the word's trail, and every rule on the trail must be keyed unless its limit is zero.
            && rule_counts
                .iter()
                .all(|(&rule, &count)| w.trail.count_of(rule).min(max_apps(rule)) == count)
            && w
                .trail
                .counted_rules()
                .all(|rule| rule_counts.contains_key(&rule) || max_apps(rule) == 0)
    }
}

/// The state key of `w`, with each rule's count saturated at `max_apps` and a rule saturated to zero left out.
pub fn key_of(w: &Word, max_apps: &impl Fn(MRuleId) -> u32) -> AnalysisStateKey {
    let morph_history = w
        .morphs
        .iter()
        .map(|morph| MorphHistoryKey {
            allomorph: morph.allomorph,
            morpheme: morph.morpheme,
            order: morph.order,
            status: morph.status,
            runtime_identity: runtime_id(morph.runtime_root.as_deref()).map(str::to_owned),
        })
        .collect();
    let rule_counts = w
        .trail
        .rule_counts()
        .into_iter()
        .filter_map(|(id, count)| {
            let saturated = count.min(max_apps(id));
            (saturated > 0).then_some((id, saturated))
        })
        .collect();
    AnalysisStateKey::new(
        w.shape.clone(),
        w.stratum,
        w.syn_fs.clone(),
        w.real_fs.clone(),
        w.non_heads().len() as u32,
        rule_counts,
        w.flags.final_template_state,
        morph_history,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::word::{GuessedRoot, MorphRecord, RuntimeRoot};
    use pg_featstruct::{FeatId, FeatureStructBuilder, FeatureValue, SymbolBits};
    use pg_grammar_model::model::LexEntryId;
    use pg_shape::ShapeBuilder;

    /// Rule 0 may apply once, rule 1 twice, rule 2 never; any other rule three times.
    fn max_apps(rule: MRuleId) -> u32 {
        match rule.0 {
            0 => 1,
            1 => 2,
            2 => 0,
            _ => 3,
        }
    }

    fn fs_with(bits: u64) -> FeatureStruct {
        let mut b = FeatureStructBuilder::new();
        b.add(FeatId(0), FeatureValue::Symbolic(SymbolBits(bits)));
        b.build()
    }

    fn shape(segments: &[u32]) -> Shape {
        let mut b = ShapeBuilder::new();
        for &cd in segments {
            b.push_segment(cd);
        }
        b.finish()
    }

    fn base() -> Word {
        let mut w = Word::new(shape(&[1, 2]), StratumId(0));
        w.morphs = vec![MorphRecord::new(AllomorphId(4), MorphemeId(5), 0)];
        w.trail.push(Some(MRuleId(0)), Some(AllomorphId(9)));
        w.trail.push(Some(MRuleId(1)), None);
        w
    }

    fn guessed(text: &str) -> RuntimeRoot {
        RuntimeRoot::Guessed(GuessedRoot {
            pattern_allo: AllomorphId(1),
            pattern_entry: LexEntryId(1),
            text: text.to_owned(),
        })
    }

    /// Words that differ from `base` in one key field each, plus words that differ only in ways the key ignores.
    fn variants() -> Vec<Word> {
        let mut out = vec![base()];
        let mut edit = |f: &dyn Fn(&mut Word)| {
            let mut w = base();
            f(&mut w);
            out.push(w);
        };
        edit(&|w| w.shape = shape(&[1, 3]));
        edit(&|w| w.shape = shape(&[1]));
        edit(&|w| w.stratum = StratumId(1));
        edit(&|w| w.syn_fs = fs_with(0b01));
        edit(&|w| w.syn_fs = fs_with(0b10));
        edit(&|w| w.real_fs = fs_with(0b01));
        edit(&|w| w.non_heads_mut().push(base()));
        edit(&|w| w.flags.final_template_state = FinalTemplateState::NonTemplate);
        edit(&|w| w.morphs[0].allomorph = AllomorphId(6));
        edit(&|w| w.morphs[0].morpheme = MorphemeId(6));
        edit(&|w| w.morphs[0].order = 1);
        edit(&|w| w.morphs[0].status = MorphStatus::Floating);
        edit(&|w| w.morphs[0].runtime_root = Some(std::rc::Rc::new(guessed("a"))));
        edit(&|w| w.morphs[0].runtime_root = Some(std::rc::Rc::new(guessed("b"))));
        edit(&|w| {
            w.morphs
                .push(MorphRecord::new(AllomorphId(4), MorphemeId(5), 0))
        });
        edit(&|w| w.morphs.clear());
        // The same rules in the other order, and with another allomorph: the key counts rules only.
        edit(&|w| {
            w.trail = crate::trail::Trail::default();
            w.trail.push(Some(MRuleId(1)), Some(AllomorphId(3)));
            w.trail.push(Some(MRuleId(0)), None);
        });
        // Rule 0 past its limit of one saturates to the same count.
        edit(&|w| w.trail.push(Some(MRuleId(0)), None));
        // Rule 1 a second time is within its limit of two, so the count differs.
        edit(&|w| w.trail.push(Some(MRuleId(1)), None));
        // And a third time saturates to the second.
        edit(&|w| {
            w.trail.push(Some(MRuleId(1)), None);
            w.trail.push(Some(MRuleId(1)), None);
        });
        // Rule 2 has a limit of zero and never enters the key.
        edit(&|w| w.trail.push(Some(MRuleId(2)), None));
        // A rule the base never unapplied.
        edit(&|w| w.trail.push(Some(MRuleId(7)), None));
        // A count-only step counts like a slot.
        edit(&|w| w.trail.push_count_only(MRuleId(7)));
        edit(&|w| w.trail.push_count_only(MRuleId(1)));
        // A slot with no known rule counts nothing.
        edit(&|w| w.trail.push(None, Some(AllomorphId(2))));
        edit(&|w| w.trail = crate::trail::Trail::default());
        // Fields the key ignores.
        edit(&|w| w.obligatory.push(FeatId(3)));
        edit(&|w| w.root_allomorph = Some(AllomorphId(8)));
        out
    }

    #[test]
    fn matches_word_agrees_with_key_equality_on_every_pair() {
        let words = variants();
        let (mut equal, mut unequal) = (0, 0);
        for a in &words {
            let key = key_of(a, &max_apps);
            for b in &words {
                let same = key == key_of(b, &max_apps);
                assert_eq!(
                    key.matches_word(b, &b.syn_fs, &max_apps),
                    same,
                    "{a:?}\nagainst\n{b:?}"
                );
                if same {
                    equal += 1;
                } else {
                    unequal += 1;
                }
            }
        }
        assert!(
            equal > words.len(),
            "some distinct words share a key: {equal}"
        );
        assert!(unequal > 0);
    }

    #[test]
    fn matches_word_compares_the_given_features_not_the_words_own() {
        let mut stored = base();
        stored.syn_fs = fs_with(0b01);
        let key = key_of(&stored, &max_apps);
        // The stored canonical is widened after its key was taken.
        stored.syn_fs = fs_with(0b11);
        assert!(!key.matches_word(&stored, &stored.syn_fs, &max_apps));
        assert!(key.matches_word(&stored, &fs_with(0b01), &max_apps));
    }
}
