//! Iterative metathesis matches where C# HermitCrab (Machine 1ddef2bd) matches: each search resumes past the previous match, and at one start the furthest match wins. Expected analyses are C# replays of each grammar, except the skipped-optional cases, which are C#'s own forward generation (divergence 132).

use pg_parse::Morpher;
use std::collections::BTreeSet;

fn analyses(xml: &str, word: &str) -> BTreeSet<String> {
    let g = pg_grammar::load(xml).expect("the grammar loads");
    Morpher::new(&g, usize::MAX)
        .parse_word(word)
        .analyses
        .iter()
        .map(|(morphs, _)| morphs.clone())
        .collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// `IterativePhonologicalPatternRule.Apply` resumes the search past the node that ended the last match, so analysis never unions a switch the previous match covered (#389).
#[test]
fn metathesis_resumes_past_the_previous_match() {
    for (xml, word, expected) in [
        (
            include_str!("data/metathesis-match-order/resume-v12-157.xml"),
            "sasasa",
            &[][..],
        ),
        (
            include_str!("data/metathesis-match-order/resume-v12-157.xml"),
            "saaa",
            &["PR0+R0"][..],
        ),
        (
            include_str!("data/metathesis-match-order/resume-u11-021.xml"),
            "tsta+tatap",
            &[][..],
        ),
        (
            include_str!("data/metathesis-match-order/resume-u11-021.xml"),
            "stpt",
            &["PR1+R1"][..],
        ),
        (
            include_str!("data/metathesis-match-order/resume-v12-090.xml"),
            "paa+aaas",
            &[][..],
        ),
        (
            include_str!("data/metathesis-match-order/resume-v12-090.xml"),
            "pa+aaaas",
            &["CI0+R2"][..],
        ),
    ] {
        assert_eq!(analyses(xml, word), set(expected), "{word}");
    }
}

/// Divergence 132: a segment a deletion's analysis restored between or beside the switches is not paired with a switch, so each word C# generates from `PRE+R*` parses back to it; C# (machine 1ddef2bd) pairs the skipped optional node with a switch and finds no parse for `ats`, `at+s` (q3f, q3g) or `pts` (d3).
#[test]
fn a_skipped_optional_node_is_not_paired_with_a_switch() {
    for (xml, word, expected) in [
        (
            include_str!("data/metathesis-match-order/skipped-optional-q3f.xml"),
            "ats",
            &["PRE+R0"][..],
        ),
        (
            include_str!("data/metathesis-match-order/skipped-optional-q3f.xml"),
            "at+s",
            &["PRE+R0"][..],
        ),
        (
            include_str!("data/metathesis-match-order/skipped-optional-q3f.xml"),
            "ast",
            &["PRE+R1"][..],
        ),
        (
            include_str!("data/metathesis-match-order/skipped-optional-q3f-rtl.xml"),
            "ats",
            &["PRE+R0"][..],
        ),
        (
            include_str!("data/metathesis-match-order/skipped-optional-q3f-rtl.xml"),
            "atsa",
            &["PRE+R2"][..],
        ),
        (
            include_str!("data/metathesis-match-order/skipped-optional-q3g.xml"),
            "ats",
            &["PRE+R0"][..],
        ),
        (
            include_str!("data/metathesis-match-order/skipped-optional-d3.xml"),
            "pts",
            &["PRE+R0", "PRE+R1"][..],
        ),
    ] {
        assert_eq!(analyses(xml, word), set(expected), "{word}");
    }
}

/// At one start the C# matcher prefers the match that reaches furthest (`Fst.ResultCompare`), and deletion analysis re-inserts a deleted segment just before the next segment, past a boundary, so the optional node joins the capture C# gives it.
#[test]
fn the_furthest_match_wins_and_a_restored_segment_follows_the_boundary() {
    for (xml, word, expected) in [
        (
            include_str!("data/metathesis-match-order/reach-a51-155.xml"),
            "asps",
            &[][..],
        ),
        (
            include_str!("data/metathesis-match-order/reach-v12-312.xml"),
            "ataa",
            &["PR1+R1+SU0"][..],
        ),
        (
            include_str!("data/metathesis-match-order/boundary-v12-072.xml"),
            "pa+p",
            &["PR0+R2"][..],
        ),
        (
            include_str!("data/metathesis-match-order/boundary-v12-568.xml"),
            "s+ps",
            &["PR0+R1", "PR0+SU1+R2"][..],
        ),
    ] {
        assert_eq!(analyses(xml, word), set(expected), "{word}");
    }
}
