//! Regression gate for two `pg_rules::metathesis` analysis-side bugs: round-trips a hand-built rule through `synthesize` then `analyze` and asserts the un-applied shape recovers the pre-synthesis original.

mod common;

use common::*;
use pg_grammar::model::{Dir, MetathesisRuleDef, Pattern, PatternNode};
use pg_shape::{NodeKind, Shape};

fn seg(g: &pg_grammar::model::Grammar, word: &str) -> Shape {
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

// Lane constants for the probe grammar ([cons, voi, Type]; see `common/mod.rs` for the segment inventory).
const A: [u64; 3] = [0b10, 0b01, 0b01]; // vowel, voiced
const T: [u64; 3] = [0b01, 0b10, 0b01]; // consonant, voiceless
const D: [u64; 3] = [0b01, 0b01, 0b01]; // consonant, voiced

// Bug 1: a reversed switch-tag order (`left_switch` tagging the physically-first node) must not make analysis rebuild a tag-name-driven pattern that searches for the un-swapped arrangement.

fn reversed_tag_rule(g: &pg_grammar::model::Grammar) -> MetathesisRuleDef {
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
        lanes_of(&synth[0]),
        vec![A.to_vec(), T.to_vec()],
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

// Bug 2: a middle context node strictly between the two switches must not be dropped from the rebuilt analysis pattern, which would wrongly require the switches strictly adjacent.

fn middle_context_rule(g: &pg_grammar::model::Grammar) -> MetathesisRuleDef {
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
fn metathesis_middle_context_node_round_trips() {
    let g = load_probe_grammar();
    let r = middle_context_rule(&g);
    let input = seg(&g, "tda"); // Q=t, M=d, P=a

    let synth = pg_rules::metathesis::synthesize(&g, &r, &input);
    assert_eq!(synth.len(), 1, "rule applies obligatorily");
    assert_eq!(
        lanes_of(&synth[0]),
        vec![A.to_vec(), D.to_vec(), T.to_vec()],
        "synthesize swaps the two endpoints (t,a -> a,t); the middle 'd' keeps its own slot \
         untouched (synthesis_reorder's own doc)"
    );

    let ana = pg_rules::metathesis::analyze(&g, &r, &synth[0]);
    assert_eq!(
        ana.len(),
        1,
        "REGRESSION GUARD (bug 2 fixed): before the fix, build_analysis_pattern dropped the middle \
         context node from its rebuilt search pattern (requiring the two switches strictly \
         adjacent), so a real surface with the middle segment intact ('a','d','t') could never \
         match -- this was always empty. See pg_foma::tests::phase_c_metathesis's own \
         middle-context test (updated alongside this fix) for the FST-containment side of the same \
         gap."
    );
    let ana_lanes = lanes_of(&ana[0]);
    assert_eq!(
        ana_lanes[1],
        D.to_vec(),
        "the middle node must be untouched by ana_union (only the two switch endpoints are unioned)"
    );
    let orig_lanes = lanes_of(&input);
    for (i, (widened, orig)) in ana_lanes.iter().zip(&orig_lanes).enumerate() {
        if i == 1 {
            continue; // middle: exact-match already asserted above.
        }
        assert!(
            pg_featstruct::flat_unifiable(widened, orig),
            "widened {widened:?} at endpoint {i} must be a superset of the original {orig:?}"
        );
    }
}
