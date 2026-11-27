use super::*;
use crate::root_search_cache::{test_disable, test_last_hits};

#[test]
fn actual_hit_preserves_parse_and_saves_work_without_cross_word_state() {
    let grammar = pg_grammar::load(include_str!("root_cache_test_grammar.xml")).unwrap();
    let morpher = Morpher::new(&grammar, usize::MAX);
    test_disable(true);
    let uncached = morpher.parse_word("zudzit");
    assert_eq!(test_last_hits(), 0);
    test_disable(false);
    let cached = morpher.parse_word("zudzit");
    let hits = test_last_hits();
    assert!(hits > 0, "the parse must exercise a real cache hit");
    assert!(!cached.capped && !cached.timed_out && !uncached.capped && !uncached.timed_out);
    assert!(!cached.analyses.is_empty());
    assert_eq!(
        crate::result_multiset(&cached.analyses),
        crate::result_multiset(&uncached.analyses)
    );
    assert!(cached.work_steps <= uncached.work_steps);
    let next_word = morpher.parse_word("zudzit");
    assert_eq!(test_last_hits(), hits);
    assert_eq!(
        crate::result_multiset(&next_word.analyses),
        crate::result_multiset(&cached.analyses)
    );
    assert_eq!(next_word.work_steps, cached.work_steps);
}

fn unrooted_word() -> Word {
    let mut builder = pg_shape::ShapeBuilder::new();
    for cd in [1, 2, 3, 4, 5, 6, 7, 8] {
        builder.push_segment(cd);
    }
    Word::new(builder.finish(), StratumId(0))
}

fn answers() -> std::cell::RefCell<crate::root_answers::RootAnswers> {
    std::cell::RefCell::new(crate::root_answers::RootAnswers::new(StratumId(0), None))
}

#[test]
fn the_early_root_check_prunes_a_shape_with_no_root_while_it_has_steps() {
    let grammar = pg_grammar::load(include_str!("root_cache_test_grammar.xml")).unwrap();
    let morpher = Morpher::new(&grammar, usize::MAX);
    let (word, answers) = (unrooted_word(), answers());
    assert!(!morpher.has_root(&word, None, &answers));
    assert_eq!(answers.borrow().get(StratumId(0), &word.shape), Some(false));
}

#[test]
fn the_early_root_check_answers_yes_and_keeps_nothing_once_its_steps_are_spent() {
    let grammar = pg_grammar::load(include_str!("root_cache_test_grammar.xml")).unwrap();
    let morpher = Morpher::new(&grammar, usize::MAX);
    let (word, answers) = (unrooted_word(), answers());
    answers
        .borrow_mut()
        .spend_steps(crate::root_answers::STEP_LIMIT);
    assert!(morpher.has_root(&word, None, &answers));
    assert_eq!(answers.borrow().get(StratumId(0), &word.shape), None);
}

#[test]
fn the_early_root_check_stops_at_the_words_deadline_and_keeps_nothing() {
    let grammar = pg_grammar::load(include_str!("root_cache_test_grammar.xml")).unwrap();
    let morpher = Morpher::new(&grammar, usize::MAX);
    let (word, answers) = (unrooted_word(), answers());
    let budget =
        pg_fst::work::WorkBudget::new(usize::MAX).with_timeout(Some(std::time::Duration::ZERO));
    let _scope = budget.enter();
    assert!(morpher.has_root(&word, None, &answers));
    assert_eq!(answers.borrow().get(StratumId(0), &word.shape), None);
}
