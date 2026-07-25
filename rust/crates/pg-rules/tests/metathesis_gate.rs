//! Regression gate for the two `pg_rules::metathesis` analysis-side bugs fixed alongside
//! `openspec/changes/compile-fst-metathesis` (discovered while building that change's FST
//! containment suite, `pg_foma::tests::phase_c_metathesis` — see that file's top doc for the full
//! empirical reproductions this gate pins as **fixed**, and `pg_rules::metathesis::
//! build_analysis_pattern`'s own doc for the C# citations + rationale). Both tests round-trip a
//! hand-built rule through `synthesize` then `analyze` and assert the un-applied shape recovers
//! (is unifiable with) the pre-synthesis original — modeled on `rewrite_gate.rs`'s own
//! `feature_change_round_trip_recovers_superset` test, at the rule level (no full `Morpher`/lexicon
//! needed to pin `pg_rules::metathesis` itself). Synthetic, delanguaged fixtures (single-letter
//! stand-in segments, no natural-language material).

mod common;

use common::*;
use pg_grammar::model::{Dir, MetathesisRuleDef, Pattern, PatternNode};
use pg_shape::{NodeKind, Shape};

fn seg(g: &pg_grammar::model::Grammar, word: &str) -> Shape {
    pg_rules::shape_feat::segment_with_features(g, table(g), word).unwrap()
}

/// The interior of a shape as `(NodeKind, char_def, lanes, optional)` — `rewrite_gate.rs`'s own
/// helper, reused verbatim.
fn interior(s: &Shape) -> Vec<(NodeKind, u32, Vec<u64>, bool)> {
    (0..s.len())
        .filter(|&i| !matches!(s.kind(i), NodeKind::LeftAnchor | NodeKind::RightAnchor))
        .map(|i| (s.kind(i), s.char_def(i), s.node_lanes(i).to_vec(), s.flags(i).is_optional()))
        .collect()
}

fn lanes_of(s: &Shape) -> Vec<Vec<u64>> {
    interior(s).into_iter().map(|x| x.2).collect()
}

// Lane constants for the probe grammar ([cons, voi, Type]; `rewrite_gate.rs`'s own constants,
// reused verbatim -- see `common/mod.rs`'s module doc for the segment inventory).
const A: [u64; 3] = [0b10, 0b01, 0b01]; // vowel, voiced
const T: [u64; 3] = [0b01, 0b10, 0b01]; // consonant, voiceless
const D: [u64; 3] = [0b01, 0b01, 0b01]; // consonant, voiced

// =================================================================================================
// Bug 1: reversed switch-tag order (`left_switch` tagging the node PHYSICALLY FIRST, the opposite
// of the well-formed convention every real HermitCrab fixture uses). Before the fix,
// `build_analysis_pattern` rebuilt a tag-name-driven (`left_switch`-always-first) search pattern
// that, for this tag order, searched for the surface's ORIGINAL un-swapped arrangement -- a vacuous
// no-op disagreeing with what `synthesize` actually produces.
// =================================================================================================

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
        left_switch: 0,  // tags the PHYSICALLY FIRST node (t) -- the reversed convention
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
    // `ana_union` widens BOTH matched positions to `t_lanes | a_lanes` (a "could be either switch
    // member" underspecification -- C#'s `FeatureStruct.Union`, see `ana_union`'s own doc), so the
    // widened value must remain unifiable (superset-compatible) with whatever was ORIGINALLY at
    // each surface position pre-synthesis.
    for (widened, orig) in ana_lanes.iter().zip(&orig_lanes) {
        assert!(
            pg_featstruct::flat_unifiable(widened, orig),
            "widened {widened:?} must be a superset of the original {orig:?}"
        );
    }
}

// =================================================================================================
// Bug 2: a middle context node strictly between the two switches (mirrors
// `machine/conformance/languages/metathesis-phase-isolation`'s own `mrComplexMeta` shape, but with
// a real SEGMENT in the middle rather than a `<BoundaryMarker>` -- see `build_analysis_pattern`'s
// own doc for why a boundary there was never actually a problem for either engine). Before the fix,
// `build_analysis_pattern` dropped the middle node from its rebuilt pattern entirely, requiring the
// two switches strictly ADJACENT -- a real surface with the middle segment intact could never match.
// =================================================================================================

fn middle_context_rule(g: &pg_grammar::model::Grammar) -> MetathesisRuleDef {
    MetathesisRuleDef {
        xml_id: "test-middle".into(),
        name: None,
        dir: Dir::LeftToRight,
        pattern: Pattern {
            nodes: vec![
                PatternNode::Context(ctx(nat_class(g, "nc_t"))),     // Q, physically first
                PatternNode::Context(ctx(nat_class(g, "nc_d"))),     // M, middle (untouched)
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
