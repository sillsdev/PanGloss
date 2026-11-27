//! A deletion that empties a morph before a metathesis keeps that morph in its place, as C# HermitCrab (Machine 1ddef2bd) does: C# leaves a deleted node in the shape, so its morph annotation does not move when the swap reorders the others (#381).

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

/// Prefix `t`, root `ps`; `t` is deleted, then `p s -> s p` swaps inside the root. C# gives `PR0+R0` for `sp`.
#[test]
fn an_emptied_prefix_stays_before_the_root_it_preceded() {
    for xml in [
        include_str!("data/metathesis-after-deletion/emptied-prefix.xml"),
        include_str!("data/metathesis-after-deletion/emptied-prefix-two-strata.xml"),
    ] {
        assert_eq!(analyses(xml, "sp"), set(&["PR0+R0", "R0"]));
        assert_eq!(analyses(xml, "spp"), set(&["PR0+R1", "R1"]));
    }
}

/// A deletion that removes a morph's first segment leaves that morph's start in place unless the swap moves one of its segments in front of it, as C# gives it.
#[test]
fn a_shortened_morph_keeps_its_place() {
    for (xml, word, expected) in [
        (
            include_str!("data/metathesis-after-deletion/deleted-prefix-initial.xml"),
            "aa",
            &["PR0+R2", "R0"][..],
        ),
        (
            include_str!("data/metathesis-after-deletion/deleted-prefix-initial.xml"),
            "aaa",
            &["PR0+R0"][..],
        ),
        (
            include_str!("data/metathesis-after-deletion/deleted-circumfix-segment.xml"),
            "ppppa",
            &["CI1+R0"][..],
        ),
        (
            include_str!("data/metathesis-after-deletion/deleted-before-root.xml"),
            "ttatat",
            &["PR2+R0+SU0", "PR2+R2+SU0"][..],
        ),
        (
            include_str!("data/metathesis-after-deletion/deleted-before-suffix.xml"),
            "ptpa",
            &["PR0+R1+SU2"][..],
        ),
        // The root's deleted `a` stays put, but the swap moves its `t` in front of the prefix.
        (
            include_str!("data/metathesis-after-deletion/deleted-root-initial.xml"),
            "ts",
            &["R0+PRE"][..],
        ),
        (
            include_str!("data/metathesis-after-deletion/deleted-before-circumfix.xml"),
            "tstsa",
            &["R2+CI2"][..],
        ),
    ] {
        assert_eq!(analyses(xml, word), set(expected), "{word}");
    }
}
