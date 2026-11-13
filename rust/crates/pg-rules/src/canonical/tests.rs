use pg_featstruct::FeatureStruct;
use pg_grammar_model::model::{PRuleId, StratumId};

use super::*;
use crate::battery::tests::{edits, fs_with, input, printed, shape, slot_output};
use crate::word::FinalTemplateState;
use crate::word_set::WordSet;

fn set_of(base: &Word, arrivals: &[Word]) -> Canonicals {
    let mut set = Canonicals::of_base(base.clone());
    for w in arrivals {
        let key = w.dedup_key();
        if set.position(&key).is_none() {
            set.push_new(&key, w.clone());
        }
    }
    set
}

/// Outputs of one analysis: the input itself, slot outputs, and a word for every way one can differ from the input outside a stored difference.
fn arrivals(input: &Word) -> Vec<Word> {
    let mut arrivals = vec![
        input.clone(),
        slot_output(input, 2, 7),
        slot_output(input, 3, 7),
        slot_output(input, 2, 7),
        slot_output(input, 2, 8),
        input.clone(),
    ];
    let mut state = slot_output(input, 4, 7);
    state.flags.final_template_state = FinalTemplateState::None;
    arrivals.push(state);
    // Equal by key to an earlier arrival but with other syntactic features: the first one stays.
    let mut same_key = slot_output(input, 2, 7);
    same_key.syn_fs = FeatureStruct::EMPTY;
    arrivals.push(same_key);
    for edit in edits() {
        let mut w = slot_output(input, 2, 7);
        edit(&mut w);
        arrivals.push(w.clone());
        arrivals.push(w);
    }
    arrivals
}

fn printed_all(words: &[Word]) -> Vec<String> {
    words.iter().map(printed).collect()
}

#[test]
fn a_stored_word_is_under_half_a_word() {
    assert!(std::mem::size_of::<Stored>() * 2 < std::mem::size_of::<Word>());
}

#[test]
fn a_set_keeps_what_a_word_set_keeps_in_the_same_order() {
    let input = input();
    let arrivals = arrivals(&input);
    let set = set_of(&input, &arrivals);
    let mut words = WordSet::default();
    for w in &arrivals {
        words.insert(w.clone());
    }
    assert_eq!(set.len(), words.len());
    assert!(matches!(set.words[0], Stored::Delta(_)));
    assert!(matches!(set.words[1], Stored::Delta(_)));
    assert!(set.words.iter().any(|w| matches!(w, Stored::Whole(_))));
    let expected = printed_all(&words.into_vec());
    let one_by_one: Vec<Word> = (0..set.len()).map(|at| set.word(at)).collect();
    assert_eq!(printed_all(&one_by_one), expected);
    assert_eq!(printed_all(&set.words().collect::<Vec<_>>()), expected);
    let strata: Vec<StratumId> = set.strata().collect();
    assert_eq!(strata.len(), expected.len());
    assert_eq!(printed_all(&set.into_words()), expected);
}

#[test]
fn reading_in_place_agrees_with_the_word() {
    let input = input();
    let arrivals = arrivals(&input);
    let set = set_of(&input, &arrivals);
    for at in 0..set.len() {
        let w = set.word(at);
        assert_eq!(WordKey::of(&set.key_fields(at)), w.dedup_key());
        assert_eq!(*set.syn_fs(at), w.syn_fs);
        assert_eq!(set.alternatives(at).len(), w.alternatives.len());
        assert_eq!(set.key_fields(at).rest.obligatory, w.obligatory);
        for b in &arrivals {
            let key = b.dedup_key();
            assert_eq!(set.position(&key) == Some(at), key == w.dedup_key());
        }
    }
}

#[test]
fn folding_and_widening_change_the_stored_word_as_they_change_a_word() {
    let input = input();
    let arrivals = arrivals(&input);
    let mut set = set_of(&input, &arrivals);
    let mut words = set.words().collect::<Vec<_>>();
    for (at, word) in words.iter_mut().enumerate() {
        // One alternative that is stored as a trail and one that is held whole.
        let mut by_trail = word.clone_without_alternatives();
        by_trail.applied_phonological_rules_mut().clear();
        by_trail.trail.push(None, None);
        let mut whole = by_trail.clone();
        whole.shape = shape(9);
        for alternative in [by_trail, whole] {
            set.fold_alternative(at, alternative.clone());
            word.fold_alternative(alternative);
        }
        *set.syn_fs_mut(at) = fs_with(0b111);
        word.syn_fs = fs_with(0b111);
    }
    for (at, w) in words.iter().enumerate() {
        let stored = set.word(at);
        assert_eq!(printed(&stored), printed(w));
        assert_eq!(stored.alternatives.len(), 2);
        let rebuilt: Vec<String> = stored
            .alternatives
            .iter()
            .map(|alternative| printed(&alternative.word(&stored)))
            .collect();
        let expected: Vec<String> = w
            .alternatives
            .iter()
            .map(|alternative| printed(&alternative.word(w)))
            .collect();
        assert_eq!(rebuilt, expected);
    }
    assert_eq!(printed_all(&set.into_words()), printed_all(&words));
}

/// A second analysis' input: another shape and source, as another candidate of the same stratum gives.
fn second_input() -> Word {
    let mut w = input();
    w.shape = shape(2);
    w.source = Some(std::rc::Rc::new(Word::new(shape(2), StratumId(2))));
    w.applied_phonological_rules_mut()
        .push((StratumId(1), PRuleId(5)));
    w
}

#[test]
fn absorbing_keeps_what_inserting_each_word_keeps() {
    for merge in [false, true] {
        let first = input();
        let second = second_input();
        let third = input();
        let mut first_arrivals = arrivals(&first);
        first_arrivals[1].fold_alternative(Word::new(shape(4), StratumId(0)));
        let mut second_arrivals = arrivals(&second);
        // Words the first analysis also gave, some with other applied phonological rules.
        second_arrivals.push(slot_output(&first, 2, 7));
        let mut other_rules = slot_output(&first, 3, 7);
        other_rules
            .applied_phonological_rules_mut()
            .push((StratumId(1), PRuleId(7)));
        second_arrivals.push(other_rules);
        // Every word of the third analysis is held already.
        let third_arrivals = vec![third.clone(), slot_output(&third, 2, 7)];

        let mut set = set_of(&first, &first_arrivals);
        // A word stored as a difference that holds an alternative, which the second analysis' rules then make whole.
        let folded_at = set
            .position(&slot_output(&first, 3, 7).dedup_key())
            .expect("the first analysis gave this word");
        assert!(matches!(set.words[folded_at], Stored::Delta(_)));
        set.fold_alternative(folded_at, Word::new(shape(4), StratumId(0)));
        // `set_of` stores the folded word whole; the same word stored as a difference holds its alternative too.
        let mut words = WordSet::default();
        for w in set.words() {
            words.insert(w);
        }
        for (base, arrivals) in [(&second, &second_arrivals), (&third, &third_arrivals)] {
            let other = set_of(base, arrivals);
            for candidate in other.words() {
                let key = candidate.dedup_key();
                match words.position(&key).filter(|_| merge) {
                    None => {
                        words.insert_with(key, || candidate);
                    }
                    Some(at) => {
                        let stored = words.get_mut(at);
                        for rule in candidate.applied_phonological_rules() {
                            if !stored.applied_phonological_rules().contains(rule) {
                                stored.applied_phonological_rules_mut().push(*rule);
                            }
                        }
                    }
                }
            }
            set.absorb(other, merge);
        }
        assert_eq!(matches!(set.words[folded_at], Stored::Whole(_)), merge);
        assert_eq!(set.alternatives(folded_at).len(), 1);
        // The third analysis gave nothing new, so its input is not held.
        assert_eq!(set.bases.len(), 2);
        assert_eq!(set.len(), words.len());
        for at in 0..set.len() {
            assert_eq!(set.position(&set.word(at).dedup_key()), Some(at));
        }
        assert_eq!(
            printed_all(&set.words().collect::<Vec<_>>()),
            printed_all(&words.into_vec()),
            "merge {merge}"
        );
    }
}
