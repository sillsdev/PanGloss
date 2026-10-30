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
