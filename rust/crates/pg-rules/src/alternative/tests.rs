use std::rc::Rc;

use pg_featstruct::{FeatId, FeatureStruct, FeatureStructBuilder, FeatureValue, SymbolBits};
use pg_grammar_model::model::{AllomorphId, MRuleId, MorphemeId, MprSet, PRuleId, StratumId};
use pg_shape::ShapeBuilder;

use super::*;
use crate::trail::Trail;
use crate::word::{FinalTemplateState, MorphRecord};

fn fs_with(bits: u64) -> FeatureStruct {
    let mut b = FeatureStructBuilder::new();
    b.add(FeatId(0), FeatureValue::Symbolic(SymbolBits(bits)));
    b.build()
}

fn shape(char_def: u32) -> pg_shape::Shape {
    let mut b = ShapeBuilder::new();
    b.push_segment(char_def);
    b.finish()
}

/// A canonical with two unapplied rules, a morph, obligatory features and a source, as a stratum fold sees it.
fn canonical() -> Word {
    let mut w = Word::new(shape(1), StratumId(1));
    w.source = Some(Rc::new(Word::new(shape(1), StratumId(2))));
    w.syn_fs = fs_with(0b01);
    w.morphs = vec![MorphRecord::new(AllomorphId(5), MorphemeId(6), 0)];
    w.obligatory = vec![FeatId(3)];
    w.trail = Trail::from_slots([
        (Some(MRuleId(1)), Some(AllomorphId(10))),
        (Some(MRuleId(2)), None),
    ]);
    w
}

/// The same analysis state reached with the rules in the other order and other allomorphs.
fn reordered(canonical: &Word) -> Word {
    let mut w = canonical.clone();
    w.trail = Trail::from_slots([
        (Some(MRuleId(2)), None),
        (Some(MRuleId(1)), Some(AllomorphId(11))),
    ]);
    w.syn_fs = fs_with(0b10);
    w
}

fn printed(w: &Word) -> String {
    format!("{w:?}")
}

#[test]
fn a_trail_alternative_rebuilds_the_word_that_was_folded() {
    let canonical = canonical();
    let folded = reordered(&canonical);
    let stored = Alternative::of(folded.clone(), &canonical);
    assert!(matches!(stored, Alternative::Trail(_)));
    assert_eq!(printed(&stored.word(&canonical)), printed(&folded));
    assert_eq!(*stored.syn_fs(), folded.syn_fs);
}

#[test]
fn later_changes_to_the_canonical_do_not_reach_a_trail_alternative() {
    let mut canonical = canonical();
    let folded = reordered(&canonical);
    let stored = Alternative::of(folded.clone(), &canonical);
    assert!(matches!(stored, Alternative::Trail(_)));
    // The fold widens the canonical's syntactic feature structure; the stats collector merges phonological rules into it.
    canonical.syn_fs = fs_with(0b11);
    canonical
        .applied_phonological_rules_mut()
        .push((StratumId(1), PRuleId(4)));
    canonical.alternatives.push(stored.clone());
    assert_eq!(printed(&stored.word(&canonical)), printed(&folded));
}

/// One change per `Word` field a trail does not store: each must be held whole, and come back as folded.
#[test]
fn a_candidate_differing_beside_its_trails_is_held_whole() {
    let canonical = canonical();
    type Change = (&'static str, fn(&mut Word));
    let changes: Vec<Change> = vec![
        ("shape", |w| w.shape = shape(2)),
        ("stratum", |w| w.stratum = StratumId(0)),
        ("real_fs", |w| w.real_fs = fs_with(0b100)),
        ("mpr", |w| w.mpr = MprSet(1)),
        ("morphs", |w| w.morphs[0].order = 1),
        ("passed_over", |w| {
            w.morphs[0].passed_over = Some(vec![1].into_boxed_slice())
        }),
        ("non_heads", |w| {
            w.non_heads_mut().push(Word::new(shape(3), StratumId(1)))
        }),
        ("non_head_app_index", |w| w.non_head_app_index = 0),
        ("analysis_allomorph", |w| {
            w.analysis_allomorph = Some(AllomorphId(1))
        }),
        ("applied_realizational_rules", |w| {
            w.applied_realizational_rules_mut().push(MorphemeId(1))
        }),
        ("applied_phonological_rules", |w| {
            w.applied_phonological_rules_mut()
                .push((StratumId(1), PRuleId(1)))
        }),
        ("mrule_app_index", |w| w.mrule_app_index = 0),
        ("root_allomorph", |w| {
            w.root_allomorph = Some(AllomorphId(2))
        }),
        ("root_runtime_id", |w| {
            w.set_root_runtime_id(Some("r".to_owned()))
        }),
        ("obligatory", |w| w.obligatory.push(FeatId(4))),
        ("is_partial", |w| w.flags.is_partial = true),
        ("is_last_applied_rule_final", |w| {
            w.flags.is_last_applied_rule_final = Some(true)
        }),
        ("final_template_state", |w| {
            w.flags.final_template_state = FinalTemplateState::NonTemplate
        }),
        ("source", |w| {
            w.source = Some(Rc::new(Word::new(shape(1), StratumId(2))))
        }),
        ("no source", |w| w.source = None),
        ("alternatives", |w| {
            let inner = w.clone();
            w.fold_alternative(inner)
        }),
        ("trace", |w| {
            w.trace = Some(crate::trace::TraceHandle::DUMMY)
        }),
    ];
    for (field, change) in changes {
        let mut folded = reordered(&canonical);
        change(&mut folded);
        let stored = Alternative::of(folded.clone(), &canonical);
        assert!(
            matches!(stored, Alternative::Whole(_)),
            "{field}: a trail cannot rebuild this candidate"
        );
        assert_eq!(
            printed(&stored.word(&canonical)),
            printed(&folded),
            "{field}"
        );
    }
}

/// `Alternative::matches` must say what `WordKey::matches` plus the obligatory comparison said of the folded word itself.
#[test]
fn matches_agrees_with_the_folded_word() {
    let canonical = canonical();
    let folded = reordered(&canonical);
    let mut probes = vec![canonical.clone(), folded.clone()];
    let mut shorter = folded.clone();
    shorter.trail = Trail::from_slots([(Some(MRuleId(2)), None)]);
    probes.push(shorter);
    let mut other_allomorph = folded.clone();
    other_allomorph.trail = Trail::from_slots([
        (Some(MRuleId(2)), None),
        (Some(MRuleId(1)), Some(AllomorphId(12))),
    ]);
    probes.push(other_allomorph);
    let mut unknown_rule = folded.clone();
    unknown_rule.trail =
        Trail::from_slots([(None, None), (Some(MRuleId(1)), Some(AllomorphId(11)))]);
    probes.push(unknown_rule);
    let mut other_obligatory = folded.clone();
    other_obligatory.obligatory.push(FeatId(9));
    probes.push(other_obligatory);
    let mut other_morphs = folded.clone();
    other_morphs.morphs[0].order = 2;
    probes.push(other_morphs);

    for stored_word in [folded.clone(), probes[4].clone(), probes[6].clone()] {
        let stored = Alternative::of(stored_word.clone(), &canonical);
        for probe in &probes {
            let key = probe.dedup_key();
            assert_eq!(
                stored.matches(&key, &probe.obligatory, &canonical),
                key.matches(&stored_word) && stored_word.obligatory == probe.obligatory,
                "stored {stored_word:?} against {probe:?}"
            );
        }
    }
}

#[test]
fn clones_of_a_canonical_share_one_list() {
    let mut canonical = canonical();
    let folded = reordered(&canonical);
    canonical.fold_alternative(folded);
    let copy = canonical.clone();
    assert_eq!(copy.alternatives.len(), 1);
    assert!(canonical
        .clone_without_alternatives()
        .alternatives
        .is_empty());
    // A push after a clone leaves the clone's list alone.
    let again = reordered(&canonical);
    canonical.fold_alternative(again);
    assert_eq!(canonical.alternatives.len(), 2);
    assert_eq!(copy.alternatives.len(), 1);
}

#[test]
fn a_trail_alternative_keeps_its_own_counts_and_any_id() {
    let canonical = canonical();
    let mut folded = reordered(&canonical);
    folded.trail.push_count_only(MRuleId(9));
    folded
        .trail
        .push(Some(MRuleId(u32::MAX)), Some(AllomorphId(u32::MAX)));
    let stored = Alternative::of(folded.clone(), &canonical);
    assert!(matches!(stored, Alternative::Trail(_)));
    let rebuilt = stored.word(&canonical);
    assert_eq!(printed(&rebuilt), printed(&folded));
    assert_eq!(rebuilt.trail.count_of(MRuleId(9)), 1);
}
