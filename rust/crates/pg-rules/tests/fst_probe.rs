//! Confirms a compiled LHS pattern reports the physical match span under both LtoR and RtoL, since the RtoL `get_offsets` un-swap is the single place the analysis path can silently go wrong.

mod common;

use pg_fst::{Direction, Segment, Transduce, ENTIRE_MATCH};
use pg_grammar::model::{Pattern, PatternNode};
use pg_rules::bridge::PatternBridge;

/// Build the FST segment list (segments only) from a shape's interior, with the mapping back to shape node indices.
fn segs_of(shape: &pg_shape::Shape) -> (Vec<Segment>, Vec<usize>) {
    let mut segs = Vec::new();
    let mut nodes = Vec::new();
    for (i, kind, _cd, _f) in shape.interior() {
        if kind == pg_shape::NodeKind::Segment {
            segs.push(Segment::new(shape.node_lanes(i).to_vec()));
            nodes.push(i);
        }
    }
    (segs, nodes)
}

#[test]
fn match_span_is_physical_in_both_directions() {
    let g = common::load_probe_grammar();
    // LHS = the single segment `t`.
    let lhs = Pattern {
        nodes: vec![PatternNode::CharDef(common::char_def(&g, "char_t"))],
    };

    let shape = pg_rules::shape_feat::segment_with_features(&g, common::table(&g), "ata").unwrap();
    let (segs, node_of) = segs_of(&shape);
    assert_eq!(segs.len(), 3, "a t a -> 3 segments");

    // Deterministic (synthesis-like) forward.
    let bridge = PatternBridge::new(&g);
    let compiled = bridge.compile_pattern(&lhs).unwrap();

    // ---- LtoR ----
    let fst = compiled
        .input
        .compile_with_direction(Direction::LeftToRight);
    let res = Transduce::new(&fst, segs.clone()).all_matches();
    assert!(!res.is_empty(), "t should match in ata");
    // Exactly one match, covering physical segment 1 (the `t`).
    let spans: Vec<(i32, i32)> = res
        .iter()
        .filter_map(|r| fst.get_offsets(ENTIRE_MATCH, &r.registers))
        .collect();
    assert_eq!(spans, vec![(1, 2)], "LtoR: t is physical segment [1,2)");
    // The matched shape node is the middle segment.
    assert_eq!(
        node_of[1], 2,
        "seg 1 maps to shape node 2 (LA=0, a=1, t=2, a=3, RA=4)"
    );

    // ---- RtoL (analysis walks the reverse direction; the same forward-built FST) ----
    let bridge_nd = PatternBridge::new(&g).deterministic(false);
    let compiled_nd = bridge_nd.compile_pattern(&lhs).unwrap();
    let fst_r = compiled_nd
        .input
        .compile_with_direction(Direction::RightToLeft);
    let res_r = Transduce::new(&fst_r, segs.clone()).all_matches();
    let spans_r: Vec<(i32, i32)> = res_r
        .iter()
        .filter_map(|r| fst_r.get_offsets(ENTIRE_MATCH, &r.registers))
        .collect();
    assert_eq!(
        spans_r,
        vec![(1, 2)],
        "RtoL: get_offsets un-swaps back to the physical span [1,2)"
    );
}

#[test]
fn flat_unifiable_is_not_usedefaults() {
    // flat_unifiable treats an absent/short lane as all-ones (unconstrained), never a feature's default value: `a` = [0b10, 0b01] unifies with constraint "voi+" = [UNC, 0b01].
    assert!(pg_featstruct::flat_unifiable(
        &[0b10, 0b01],
        &[u64::MAX, 0b01]
    ));
    // An underspecified input lane 0 (all-ones) unifies with any constraint on lane 0 -- over-permissive relative to C#'s UseDefaults, which would resolve to the feature's default instead.
    assert!(pg_featstruct::flat_unifiable(
        &[u64::MAX, 0b01],
        &[0b01, 0b01]
    ));
}
