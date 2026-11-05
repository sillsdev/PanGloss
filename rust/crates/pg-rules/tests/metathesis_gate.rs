//! Regression gate for two `pg_rules::metathesis` analysis-side bugs: round-trips a hand-built rule through `synthesize` then `analyze` and asserts the un-applied shape recovers the pre-synthesis original.

#[path = "common/mod.rs"]
mod common;

use common::*;
use pg_grammar_model::model::{Dir, MetathesisRuleDef, Pattern, PatternNode};
use pg_shape::{NodeKind, Shape};

fn seg(g: &pg_grammar_model::model::Grammar, word: &str) -> Shape {
    pg_rules::shape_feat::segment_with_features(g, table(g), word).unwrap()
}

/// The interior of a shape as `(NodeKind, char_def, lanes, optional)`.
fn interior(s: &Shape) -> Vec<(NodeKind, u32, Vec<u64>, bool)> {
    (0..s.len())
        .filter(|&i| !matches!(s.kind(i), NodeKind::LeftAnchor | NodeKind::RightAnchor))
        .map(|i| {
            (
                s.kind(i),
                s.char_def(i),
                s.node_lanes(i).to_vec(),
                s.flags(i).is_optional(),
            )
        })
        .collect()
}

fn lanes_of(s: &Shape) -> Vec<Vec<u64>> {
    interior(s).into_iter().map(|x| x.2).collect()
}

fn char_defs(s: &Shape) -> Vec<u32> {
    s.interior().map(|(_, _, cd, _)| cd).collect()
}

// Bug 1: a reversed switch-tag order (`left_switch` tagging the physically-first node) must not make analysis rebuild a tag-name-driven pattern that searches for the un-swapped arrangement.

fn reversed_tag_rule(g: &pg_grammar_model::model::Grammar) -> MetathesisRuleDef {
    MetathesisRuleDef {
        xml_id: "test-reversed".into(),
        name: None,
        dir: Dir::LeftToRight,
        pattern: Pattern {
            nodes: vec![
                PatternNode::Context(ctx(nat_class(g, "nc_t"))), // physically FIRST
                PatternNode::Context(ctx(nat_class(g, "nc_vowel"))), // physically LAST
            ],
        },
        left_switch: 0, // tags the PHYSICALLY FIRST node (t) -- the reversed convention
        right_switch: 1, // tags the PHYSICALLY LAST node (a)
    }
}

#[test]
fn metathesis_reversed_switch_tag_order_round_trips() {
    let g = load_probe_grammar();
    let r = reversed_tag_rule(&g);
    let input = seg(&g, "ta");

    let synth = pg_rules::metathesis::synthesize(&g, &r, &input);
    assert_eq!(synth.len(), 1, "rule applies obligatorily");
    assert_eq!(
        char_defs(&synth[0]),
        vec![char_def(&g, "char_a").0, char_def(&g, "char_t").0],
        "synthesize swaps by PHYSICAL position (t,a -> a,t), tag-name-agnostic -- NOT the vacuous \
         no-op a tag-name-driven convention would predict for this reversed left_switch/right_switch \
         assignment"
    );

    let ana = pg_rules::metathesis::analyze(&g, &r, &synth[0]);
    assert_eq!(
        ana.len(),
        1,
        "REGRESSION GUARD (bug 1 fixed): analysis must be able to un-apply exactly what synthesis \
         produced. Before the fix, build_analysis_pattern's tag-name-driven rebuild always searched \
         the surface for 'ta' (left_switch's own node first, unconditionally) -- never matching the \
         genuinely-swapped 'at' synthesis actually produces here -- so this was empty."
    );
    let ana_lanes = lanes_of(&ana[0]);
    let orig_lanes = lanes_of(&input);
    // `ana_union` widens both matched positions to `t_lanes | a_lanes`, so the widened value must remain unifiable with whatever was originally at each surface position pre-synthesis.
    for (widened, orig) in ana_lanes.iter().zip(&orig_lanes) {
        assert!(
            pg_featstruct::flat_unifiable(widened, orig),
            "widened {widened:?} must be a superset of the original {orig:?}"
        );
    }
}

// A middle context node between the two switches is dropped from the rebuilt analysis pattern, as in C# (`AnalysisMetathesisRuleSpec`), so analysis requires the switches adjacent and never un-applies synthesis's own output (divergence 127).

fn middle_context_rule(g: &pg_grammar_model::model::Grammar) -> MetathesisRuleDef {
    MetathesisRuleDef {
        xml_id: "test-middle".into(),
        name: None,
        dir: Dir::LeftToRight,
        pattern: Pattern {
            nodes: vec![
                PatternNode::Context(ctx(nat_class(g, "nc_t"))), // Q, physically first
                PatternNode::Context(ctx(nat_class(g, "nc_d"))), // M, middle (untouched)
                PatternNode::Context(ctx(nat_class(g, "nc_vowel"))), // P, physically last
            ],
        },
        left_switch: 2,  // tags P (physically last) -- the well-formed convention
        right_switch: 0, // tags Q (physically first)
    }
}

#[test]
fn metathesis_middle_context_is_not_unapplied_as_in_csharp() {
    let g = load_probe_grammar();
    let r = middle_context_rule(&g);
    let input = seg(&g, "tda"); // Q=t, M=d, P=a

    let synth = pg_rules::metathesis::synthesize(&g, &r, &input);
    assert_eq!(synth.len(), 1, "rule applies obligatorily");
    assert_eq!(
        char_defs(&synth[0]),
        vec![
            char_def(&g, "char_a").0,
            char_def(&g, "char_d").0,
            char_def(&g, "char_t").0,
        ],
        "synthesize swaps the two endpoints (t,a -> a,t); the middle 'd' keeps its own slot"
    );

    let ana = pg_rules::metathesis::analyze(&g, &r, &synth[0]);
    assert!(
        ana.is_empty(),
        "hc.dll drops the middle context from the analysis pattern, so the surface a,d,t has no \
         adjacent switch pair and is not un-applied"
    );

    // Adjacent switches are all C#'s rebuilt pattern looks for.
    let adjacent = seg(&g, "adt");
    assert_eq!(
        pg_rules::metathesis::analyze(&g, &r, &adjacent).len(),
        0,
        "a,d,t has d between the switches"
    );
    let adjacent = seg(&g, "at");
    assert_eq!(
        pg_rules::metathesis::analyze(&g, &r, &adjacent).len(),
        1,
        "adjacent switches match the dropped-middle pattern"
    );
}
