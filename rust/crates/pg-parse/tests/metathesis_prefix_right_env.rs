//! Divergence 133 (docs/divergences/133-metathesis-overlapping-morph-annotations.md), allomorph-environment case: after a metathesis swaps prefix `s+` into root `ta`, hc.dll reads the segment after the swapped prefix as end of input (`AnnotationList.FindDepthFirst` never visits the rebuilt root that holds it), so a required right environment `A` on the prefix finds no parse for `t+sa` and `tsa`. PanGloss reads the real segment `a` and parses both. These tests pin PanGloss's behaviour; the C# answer is `-` for both words. No suite fixture: rows come only from C# and this is a known divergence (sillsdev/machine#650). Grammar: `data/metathesis-prefix-renv/grammar.xml`, the minimal `PRE-RA-req-none` probe of #380.

use pg_parse::Morpher;

fn signature(word: &str) -> String {
    let xml = include_str!("data/metathesis-prefix-renv/grammar.xml");
    let g = pg_grammar::load(xml).expect("the grammar loads");
    Morpher::new(&g, usize::MAX).parse_word(word).signature()
}

#[test]
fn divergence_133_a_swapped_prefix_right_environment_sees_the_next_segment() {
    // C# (hc.dll Machine ba6dfa106): "-" for both.
    assert_eq!(signature("t+sa"), "RT+PRE|t+?sa");
    assert_eq!(signature("tsa"), "RT+PRE|t+?sa");
}

#[test]
fn divergence_133_control_the_root_alone_parses() {
    assert_eq!(signature("ta"), "RT|ta");
}
