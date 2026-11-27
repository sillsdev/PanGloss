//! #380: after a metathesis swaps a segment across a morph boundary, the morph's range wraps the other morph, and an allomorph environment reads the end of that wrapped range as C# `AllomorphEnvironment.IsMatch` does (`morph.Range.End.Next`). Expected signatures are the C# replay in the suite fixture `edge-cases/metathesis-allomorph-env-after-swap` (Machine ba6dfa106); `data/metathesis-allomorph-env/grammar.xml` is its grammar.

use pg_parse::Morpher;

fn signature(word: &str) -> String {
    let xml = include_str!("data/metathesis-allomorph-env/grammar.xml");
    let g = pg_grammar::load(xml).expect("the grammar loads");
    Morpher::new(&g, usize::MAX).parse_word(word).signature()
}

#[test]
fn a_root_wrapped_around_a_swapped_prefix_sees_the_word_end() {
    assert_eq!(signature("tsa"), "Z+PRE|t+?sa");
    assert_eq!(signature("t+sa"), "Z+PRE|t+?sa");
}

#[test]
fn an_excluded_word_final_environment_still_matches_past_a_wrapped_prefix() {
    assert_eq!(signature("tsaa"), "-");
    assert_eq!(signature("taa"), "-");
}

#[test]
fn controls_without_a_cross_morph_swap_are_unchanged() {
    assert_eq!(signature("ta"), "Z|ta");
    assert_eq!(signature("sp"), "PRE+Y|s+?p");
    assert_eq!(signature("tsap"), "-");
}
