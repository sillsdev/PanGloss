use super::*;

fn build_cvc() -> Shape {
    // A tiny "shape" of three segments (char-def ids 10, 11, 10) with no boundaries.
    let mut b = ShapeBuilder::with_interior_capacity(3);
    b.push_segment(10);
    b.push_segment(11);
    b.push_segment(10);
    b.finish()
}

#[test]
fn cdbits_intersect_keeps_only_shared_members() {
    let a = CdBits::from_ids([1, 2, 3, 70]); // 70 forces the SmallVec to spill to a 2nd word.
    let b = CdBits::from_ids([2, 3, 4, 70]);
    let i = a.intersect(&b);
    assert_eq!(i.count(), 3);
    assert!(i.contains(2) && i.contains(3) && i.contains(70));
    assert!(!i.contains(1) && !i.contains(4));
}

#[test]
fn cdbits_intersect_of_disjoint_sets_is_empty() {
    let a = CdBits::from_ids([1, 2]);
    let b = CdBits::from_ids([3, 4]);
    assert_eq!(a.intersect(&b).count(), 0);
}

#[test]
fn anchors_bracket_the_interior() {
    let s = build_cvc();
    assert_eq!(s.len(), 5); // left anchor + 3 segments + right anchor
    assert_eq!(s.kind(0), NodeKind::LeftAnchor);
    assert_eq!(s.kind(4), NodeKind::RightAnchor);
    assert_eq!(s.char_def(0), NO_CHAR_DEF);
    assert_eq!(s.char_def(4), NO_CHAR_DEF);
    let interior: Vec<_> = s.interior().map(|(_, k, cd, _)| (k, cd)).collect();
    assert_eq!(
        interior,
        vec![
            (NodeKind::Segment, 10),
            (NodeKind::Segment, 11),
            (NodeKind::Segment, 10),
        ]
    );
}

#[test]
fn boundaries_are_optional() {
    let mut b = ShapeBuilder::new();
    b.push_segment(1);
    b.push_boundary(99);
    b.push_segment(2);
    let s = b.finish();
    let flags: Vec<_> = s
        .interior()
        .map(|(_, k, _, f)| (k, f.is_optional()))
        .collect();
    assert_eq!(
        flags,
        vec![
            (NodeKind::Segment, false),
            (NodeKind::Boundary, true),
            (NodeKind::Segment, false),
        ]
    );
}

#[test]
fn interner_dedups_equal_shapes() {
    let mut it = ShapeInterner::new();
    let a = it.intern(build_cvc());
    let b = it.intern(build_cvc());
    assert_eq!(a, b);
    assert_eq!(it.len(), 1);
    assert_eq!(it.get(a).len(), 5);

    let mut bld = ShapeBuilder::new();
    bld.push_segment(10);
    let c = it.intern(bld.finish());
    assert_ne!(a, c);
    assert_eq!(it.len(), 2);
}

// --- M3: feature lanes + positional COW mutation -------------------------------------------

/// Build a two-segment shape with a `W`-lane feature matrix from explicit lane rows.
fn build_with_lanes(w: u32, s0: &[u64], s1: &[u64]) -> Shape {
    let mut b = ShapeBuilder::with_features_capacity(w, 2);
    b.push_segment_with_lanes(10, s0);
    b.push_segment_with_lanes(11, s1);
    b.finish()
}

#[test]
fn feature_lanes_round_trip_and_default_fill() {
    let s = build_with_lanes(2, &[0b0011, 0b0101], &[0b1000, 0b0001]);
    // len = LA + 2 segments + RA; feat_lanes = 4 nodes * 2 lanes.
    assert_eq!(s.len(), 4);
    assert_eq!(s.feat_width(), 2);
    assert_eq!(s.node_lanes(1), &[0b0011, 0b0101]); // segment 0
    assert_eq!(s.node_lanes(2), &[0b1000, 0b0001]); // segment 1
                                                    // Anchors got the default unconstrained fill.
    assert_eq!(s.node_lanes(0), &[u64::MAX, u64::MAX]);
    assert_eq!(s.node_lanes(3), &[u64::MAX, u64::MAX]);
    // A feature-less shape has width 0 and empty lane rows (back-compat with the append path).
    assert_eq!(build_cvc().feat_width(), 0);
    assert_eq!(build_cvc().node_lanes(1), &[] as &[u64]);
}

#[test]
fn feature_lanes_are_part_of_identity() {
    let mut it = ShapeInterner::new();
    let a = it.intern(build_with_lanes(2, &[0b0011, 0b0101], &[0b1000, 0b0001]));
    // Identical structure AND lanes -> same id.
    let b = it.intern(build_with_lanes(2, &[0b0011, 0b0101], &[0b1000, 0b0001]));
    assert_eq!(a, b);
    assert_eq!(it.len(), 1);
    // Same structure/char_defs but one differing lane -> different id.
    let c = it.intern(build_with_lanes(2, &[0b0011, 0b0101], &[0b1000, 0b0010]));
    assert_ne!(a, c);
    assert_eq!(it.len(), 2);
}

#[test]
fn insert_segment_mid_shape() {
    // Start: LA, seg(10)[lanes a], seg(11)[lanes b], RA  (indices 0..=3)
    let base = build_with_lanes(2, &[0b0001, 0b0001], &[0b0010, 0b0010]);
    let mut m = ShapeBuilder::from_shape(&base);
    // Insert a new segment at index 2 (between the two segments; epenthesis-style AddAfter).
    m.insert(
        2,
        NodeKind::Segment,
        99,
        NodeFlags::EMPTY,
        &[0b0100, 0b1000],
    );
    let s = m.freeze();
    assert_eq!(s.len(), 5); // one more node
    let interior: Vec<_> = s.interior().map(|(_, k, cd, _)| (k, cd)).collect();
    assert_eq!(
        interior,
        vec![
            (NodeKind::Segment, 10),
            (NodeKind::Segment, 99),
            (NodeKind::Segment, 11),
        ]
    );
    assert_eq!(s.node_lanes(1), &[0b0001, 0b0001]); // original seg 10
    assert_eq!(s.node_lanes(2), &[0b0100, 0b1000]); // inserted seg 99
    assert_eq!(s.node_lanes(3), &[0b0010, 0b0010]); // shifted seg 11
    assert_eq!(s.kind(4), NodeKind::RightAnchor); // anchor shifted to the tail
}

#[test]
fn delete_node_shifts_positions() {
    // LA, seg10, seg11, seg12, RA
    let mut b = ShapeBuilder::with_features_capacity(1, 3);
    b.push_segment_with_lanes(10, &[0b001]);
    b.push_segment_with_lanes(11, &[0b010]);
    b.push_segment_with_lanes(12, &[0b100]);
    let base = b.finish();
    let mut m = ShapeBuilder::from_shape(&base);
    // Delete the middle segment (index 2). Everything after shifts down by one.
    m.delete(2);
    let s = m.freeze();
    assert_eq!(s.len(), 4);
    let interior: Vec<_> = s.interior().map(|(_, _, cd, _)| cd).collect();
    assert_eq!(interior, vec![10, 12]);
    assert_eq!(s.node_lanes(1), &[0b001]);
    assert_eq!(s.node_lanes(2), &[0b100]); // seg12's lanes moved down with it
}

#[test]
fn delete_two_nodes_descending_index() {
    // Deleting several nodes must go descending-index (the documented shift contract).
    let mut b = ShapeBuilder::with_features_capacity(1, 4);
    for (cd, lane) in [(10, 0b0001), (11, 0b0010), (12, 0b0100), (13, 0b1000)] {
        b.push_segment_with_lanes(cd, &[lane]);
    }
    let base = b.finish(); // LA,10,11,12,13,RA  (indices 1..=4)
    let mut m = ShapeBuilder::from_shape(&base);
    // Remove seg11 (idx 2) and seg13 (idx 4): process the higher index first.
    m.delete(4);
    m.delete(2);
    let s = m.freeze();
    let interior: Vec<_> = s.interior().map(|(_, _, cd, _)| cd).collect();
    assert_eq!(interior, vec![10, 12]);
    assert_eq!(s.node_lanes(1), &[0b0001]);
    assert_eq!(s.node_lanes(2), &[0b0100]);
}

#[test]
fn modify_changes_lanes_but_preserves_char_def() {
    let base = build_with_lanes(2, &[0b0011, 0b0101], &[0b1000, 0b0001]);
    let mut m = ShapeBuilder::from_shape(&base);
    // Feature-change on segment 0 (index 1): caller hands in the post-union lane row.
    m.modify(1, &[0b0001, 0b0100]);
    let s = m.freeze();
    assert_eq!(s.node_lanes(1), &[0b0001, 0b0100]); // lanes changed
    assert_eq!(s.char_def(1), 10); // char_def deliberately unchanged (stale display identity)
    assert_eq!(s.node_lanes(2), &[0b1000, 0b0001]); // other node untouched
}

#[test]
fn mutation_is_copy_on_write_independent_of_source() {
    let base = build_with_lanes(2, &[0b0011, 0b0101], &[0b1000, 0b0001]);
    let mut m = ShapeBuilder::from_shape(&base);
    m.modify(1, &[0b0001, 0b0001]);
    m.delete(2);
    m.insert(
        2,
        NodeKind::Segment,
        77,
        NodeFlags::EMPTY,
        &[0b1111, 0b1111],
    );
    let mutated = m.freeze();
    // The original frozen shape is completely unaffected by the builder's edits.
    assert_eq!(base.len(), 4);
    assert_eq!(base.node_lanes(1), &[0b0011, 0b0101]);
    assert_eq!(base.node_lanes(2), &[0b1000, 0b0001]);
    assert_eq!(base.char_def(2), 11);
    // ...and the mutated shape reflects them.
    assert_ne!(mutated, base);
    assert_eq!(mutated.node_lanes(1), &[0b0001, 0b0001]);
    assert_eq!(mutated.char_def(2), 77);
}

#[test]
fn frozen_build_mutate_freeze_columns_round_trip() {
    // Full round trip: frozen -> from_shape -> mutate -> freeze -> assert every column.
    let base = build_with_lanes(1, &[0b01], &[0b10]);
    let mut m = ShapeBuilder::from_shape(&base);
    m.insert(
        2,
        NodeKind::Boundary,
        5,
        NodeFlags(NodeFlags::OPTIONAL),
        &[u64::MAX],
    );
    let s = m.freeze();
    let cols: Vec<_> = (0..s.len())
        .map(|i| {
            (
                s.kind(i),
                s.char_def(i),
                s.flags(i).is_optional(),
                s.node_lanes(i)[0],
            )
        })
        .collect();
    assert_eq!(
        cols,
        vec![
            (NodeKind::LeftAnchor, NO_CHAR_DEF, false, u64::MAX),
            (NodeKind::Segment, 10, false, 0b01),
            (NodeKind::Boundary, 5, true, u64::MAX),
            (NodeKind::Segment, 11, false, 0b10),
            (NodeKind::RightAnchor, NO_CHAR_DEF, false, u64::MAX),
        ]
    );
}

/// A `Shape` clone shares one allocation, and editing goes through `ShapeBuilder::from_shape` into a fresh shape, so the original (and every other clone) is unchanged and equality stays by content.
#[test]
fn cloned_shape_edit_leaves_original_unchanged() {
    let original = build_cvc();
    let clone = original.clone();
    assert_eq!(original, clone);

    let mut b = ShapeBuilder::from_shape(&clone);
    b.delete(2);
    let edited = b.freeze();

    assert_ne!(edited, original);
    assert_eq!(original, clone, "the clone is untouched too");
    assert_eq!(original.len(), 5, "two anchors plus three segments");
    assert_eq!(original.char_def(1), 10);
    assert_eq!(original.char_def(2), 11);
    assert_eq!(original.char_def(3), 10);
    assert_eq!(edited.len(), 4, "the edit landed in the new shape");
    assert_eq!(edited.char_def(2), 10);
    // Rebuilding the same content is equal and hashes alike.
    let rebuilt = ShapeBuilder::from_shape(&original).freeze();
    assert_eq!(rebuilt, original);
}

/// The budget constant is the size of the seven inline columns plus `feat_width` (64-bit layout; wasm32 pointers are narrower).
#[test]
#[cfg(target_pointer_width = "64")]
fn inline_columns_bytes_matches_the_column_layout() {
    use std::mem::size_of;
    assert_eq!(
        INLINE_COLUMNS_BYTES + CONTENT_HASH_BYTES,
        size_of::<ShapeData>()
    );
    let by_hand = 7 * size_of::<Box<[u8]>>() + size_of::<u32>();
    assert_eq!(INLINE_COLUMNS_BYTES, by_hand.next_multiple_of(8));
}

/// Lane values distinct per node and per lane.
fn lane_row(tag: u64, w: usize) -> Vec<u64> {
    (0..w as u64).map(|k| tag * 1000 + k + 1).collect()
}

fn assert_lanes_match(b: &ShapeBuilder, model: &[Vec<u64>]) {
    assert_eq!(b.len(), model.len());
    for (i, row) in model.iter().enumerate() {
        assert_eq!(b.node_lanes(i), row.as_slice(), "lane row {i} out of place");
    }
}

#[test]
fn insert_and_delete_keep_feature_lanes_aligned_at_start_middle_and_end() {
    for w in [2usize, 3, 5] {
        let mut seed = ShapeBuilder::with_features(w as u32);
        for tag in 1..=4u64 {
            seed.push_segment(tag as u32);
            seed.modify(tag as usize, &lane_row(tag, w));
        }
        // Anchors, then the four interior rows, then the right anchor.
        let mut b = ShapeBuilder::from_shape(&seed.finish());
        let mut model: Vec<Vec<u64>> = (0..b.len()).map(|i| b.node_lanes(i).to_vec()).collect();
        assert_eq!(model.len(), 6);
        assert_lanes_match(&b, &model);

        // Insert before the first interior node, mid-way, and before the right anchor.
        for (at, next) in [(Some(1usize), 10u64), (Some(3), 11), (None, 12)] {
            let index = at.unwrap_or(b.len() - 1);
            let row = lane_row(next, w);
            b.insert(index, NodeKind::Segment, 7, NodeFlags::EMPTY, &row);
            model.insert(index, row);
            assert_lanes_match(&b, &model);
        }

        // Delete the first interior node, a middle one, and the last interior node.
        for at in [Some(1usize), Some(model.len() / 2 - 1), None] {
            let index = at.unwrap_or(model.len() - 2);
            b.delete(index);
            model.remove(index);
            assert_lanes_match(&b, &model);
        }
    }
}

#[test]
fn equal_shapes_hash_alike_and_edits_rehash() {
    use std::hash::BuildHasher;
    let state = rustc_hash::FxBuildHasher;
    let a = build_cvc();
    let b = build_cvc();
    assert_eq!(a, b);
    assert_eq!(state.hash_one(&a), state.hash_one(&b));
    assert_eq!(state.hash_one(&a), state.hash_one(a.clone()));
    let mut edited = ShapeBuilder::from_shape(&a);
    edited.push_segment(12);
    assert_ne!(state.hash_one(&a), state.hash_one(edited.freeze()));
}

#[test]
fn debug_text_does_not_depend_on_whether_the_shape_was_hashed() {
    use std::hash::BuildHasher;
    let a = build_cvc();
    let before = format!("{a:?}");
    let _ = rustc_hash::FxBuildHasher.hash_one(&a);
    assert_eq!(before, format!("{a:?}"));
    assert!(!before.contains("content_hash"));
}
