use std::rc::Rc;

use pg_featstruct::{FeatId, FeatureStruct, FeatureStructBuilder, FeatureValue, SymbolBits};
use pg_grammar_model::model::{AllomorphId, MRuleId, MorphemeId, MprSet, PRuleId, StratumId};
use pg_shape::ShapeBuilder;

use super::*;
use crate::trace::TraceHandle;
use crate::word::MorphRecord;
use crate::word_set::WordSet;

pub(crate) fn fs_with(bits: u64) -> FeatureStruct {
    let mut b = FeatureStructBuilder::new();
    b.add(FeatId(0), FeatureValue::Symbolic(SymbolBits(bits)));
    b.build()
}

pub(crate) fn shape(char_def: u32) -> Shape {
    let mut b = ShapeBuilder::new();
    b.push_segment(char_def);
    b.finish()
}

/// A battery input with every kind of state a battery can meet: a trail, a morph, obligatory features, a source and applied rules.
pub(crate) fn input() -> Word {
    let mut w = Word::new(shape(1), StratumId(1));
    w.source = Some(Rc::new(Word::new(shape(1), StratumId(2))));
    w.syn_fs = fs_with(0b01);
    w.real_fs = fs_with(0b100);
    w.mpr = MprSet(0b11);
    w.morphs = vec![MorphRecord::new(AllomorphId(5), MorphemeId(6), 0)];
    w.obligatory = vec![FeatId(3)];
    w.trail = Trail::from_slots([(Some(MRuleId(1)), Some(AllomorphId(10)))]);
    w.mrule_app_index = 0;
    w.applied_realizational_rules_mut().push(MorphemeId(8));
    w.applied_phonological_rules_mut()
        .push((StratumId(1), PRuleId(2)));
    w
}

/// `input` with one slot rule unapplied, as `apply_one_mrule` leaves it.
pub(crate) fn slot_output(input: &Word, rule: u32, char_def: u32) -> Word {
    let mut w = input.clone();
    w.shape = shape(char_def);
    w.syn_fs = fs_with(0b10);
    w.trail
        .push(Some(MRuleId(rule)), Some(AllomorphId(rule + 20)));
    w.mrule_app_index = w.trail.len() as i32 - 1;
    w.flags.final_template_state = FinalTemplateState::NonTemplate;
    w
}

pub(crate) fn printed(w: &Word) -> String {
    format!("{w:?}")
}

#[test]
fn a_stored_output_is_a_quarter_of_a_word() {
    assert!(std::mem::size_of::<BatteryWord>() * 4 <= std::mem::size_of::<Word>());
}

#[test]
fn a_slot_output_is_stored_as_a_delta_and_rebuilds_exactly() {
    let input = input();
    for w in [slot_output(&input, 2, 7), input.clone()] {
        let stored = BatteryWord::of(&w, &input);
        assert!(matches!(stored, BatteryWord::Delta(_)));
        assert_eq!(stored.dedup_key(&input), w.dedup_key());
        assert!(stored.matches(&w.dedup_key(), &input));
        assert_eq!(*stored.syn_fs(), w.syn_fs);
        assert_eq!(stored.estimated_bytes(&input), estimate_word_bytes(&w));
        assert_eq!(printed(&stored.into_word(&input)), printed(&w));
    }
}

pub(crate) type Edit = Box<dyn Fn(&mut Word)>;

/// Every way a word can differ from the input outside the delta's own fields.
pub(crate) fn edits() -> Vec<Edit> {
    vec![
        Box::new(|w| w.stratum = StratumId(9)),
        Box::new(|w| w.real_fs = fs_with(0b1000)),
        Box::new(|w| w.mpr = MprSet(0b1)),
        Box::new(|w| w.morphs[0].order = 4),
        Box::new(|w| w.morphs.clear()),
        Box::new(|w| w.non_heads_mut().push(Word::new(shape(3), StratumId(0)))),
        Box::new(|w| w.non_head_app_index = 0),
        Box::new(|w| w.analysis_allomorph = Some(AllomorphId(1))),
        Box::new(|w| w.applied_realizational_rules_mut().push(MorphemeId(9))),
        Box::new(|w| w.applied_phonological_rules_mut().clear()),
        Box::new(|w| w.root_allomorph = Some(AllomorphId(2))),
        Box::new(|w| w.set_root_runtime_id(Some("r".to_owned()))),
        Box::new(|w| w.obligatory.push(FeatId(4))),
        Box::new(|w| w.flags.is_partial = true),
        Box::new(|w| w.flags.is_last_applied_rule_final = Some(true)),
        Box::new(|w| w.source = None),
        Box::new(|w| w.source = Some(Rc::new(Word::new(shape(1), StratumId(2))))),
        Box::new(|w| w.fold_alternative(Word::new(shape(4), StratumId(0)))),
        Box::new(|w| w.trace = Some(TraceHandle::DUMMY)),
    ]
}

#[test]
fn an_output_that_differs_elsewhere_is_held_whole_and_comes_back_exactly() {
    let input = input();
    for (i, edit) in edits().iter().enumerate() {
        let mut w = slot_output(&input, 2, 7);
        edit(&mut w);
        let stored = BatteryWord::of(&w, &input);
        assert!(matches!(stored, BatteryWord::Whole(_)), "edit {i}");
        assert_eq!(stored.dedup_key(&input), w.dedup_key(), "edit {i}");
        assert_eq!(printed(&stored.into_word(&input)), printed(&w), "edit {i}");
    }
}

#[test]
fn an_input_with_non_heads_or_alternatives_holds_its_outputs_whole() {
    let mut with_non_head = input();
    with_non_head
        .non_heads_mut()
        .push(Word::new(shape(3), StratumId(0)));
    let mut with_alternative = input();
    with_alternative.fold_alternative(Word::new(shape(4), StratumId(0)));
    for input in [with_non_head, with_alternative] {
        let w = slot_output(&input, 2, 7);
        let stored = BatteryWord::of(&w, &input);
        assert!(matches!(stored, BatteryWord::Whole(_)));
        assert_eq!(printed(&stored.into_word(&input)), printed(&w));
    }
}

#[test]
fn matching_in_place_agrees_with_key_equality() {
    let input = input();
    let mut variants = vec![input.clone(), slot_output(&input, 2, 7)];
    variants.push(slot_output(&input, 3, 7));
    variants.push(slot_output(&input, 2, 8));
    variants.push(slot_output(&variants[1], 3, 8));
    let mut state = slot_output(&input, 2, 7);
    state.flags.final_template_state = FinalTemplateState::None;
    variants.push(state);
    let mut index = slot_output(&input, 2, 7);
    index.mrule_app_index = 0;
    variants.push(index);
    let mut syn = slot_output(&input, 2, 7);
    syn.syn_fs = FeatureStruct::EMPTY;
    variants.push(syn);
    for edit in edits() {
        let mut w = slot_output(&input, 2, 7);
        edit(&mut w);
        variants.push(w);
    }
    for a in &variants {
        let stored = BatteryWord::of(a, &input);
        for b in &variants {
            assert_eq!(
                stored.matches(&b.dedup_key(), &input),
                a.dedup_key() == b.dedup_key()
            );
        }
    }
}

#[test]
fn a_battery_set_keeps_what_a_word_set_keeps_in_the_same_order() {
    let input = input();
    let mut arrivals = vec![
        slot_output(&input, 2, 7),
        input.clone(),
        slot_output(&input, 3, 7),
        slot_output(&input, 2, 7),
        slot_output(&input, 2, 8),
        input.clone(),
    ];
    // Equal by key to the first arrival but with other syntactic features: the first one stays.
    let mut same_key = slot_output(&input, 2, 7);
    same_key.syn_fs = FeatureStruct::EMPTY;
    arrivals.push(same_key);
    for edit in edits() {
        let mut w = slot_output(&input, 2, 7);
        edit(&mut w);
        arrivals.push(w.clone());
        arrivals.push(w);
    }
    let mut battery = BatterySet::new(&input);
    let mut words = WordSet::default();
    for w in &arrivals {
        battery.insert(w);
        words.insert(w.clone());
    }
    assert_eq!(battery.len(), words.len());
    assert!(!battery.is_empty());
    let kept: Vec<String> = battery.into_vec().iter().map(printed).collect();
    let expected: Vec<String> = words.into_vec().iter().map(printed).collect();
    assert_eq!(kept, expected);
}
