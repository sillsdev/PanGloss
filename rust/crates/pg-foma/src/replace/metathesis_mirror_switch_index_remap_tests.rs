//! Tests the metathesis mirror index mapping against off-by-one errors.
use super::metathesis_mirror_switch_indices;

/// Asymmetric placement, chosen so an off-by-one in either direction lands on a different, still in-bounds pair rather than masking a bug.
#[test]
fn asymmetric_five_slot_placement_matches_the_derived_formula_exactly() {
    let n = 5;
    let (left_idx, right_idx) = (0, 1);
    assert_eq!(
        metathesis_mirror_switch_indices(n, left_idx, right_idx),
        (4, 3),
        "correct remap: n - 1 - left_idx, n - 1 - right_idx"
    );
    // `n - left_idx` (no `-1`) would give (5, 4), out of bounds; `n - 2 - left_idx` would give (3, 2). Neither equals the correct (4, 3).
    assert_ne!(
        (n - left_idx, n - right_idx),
        (4, 3),
        "sanity: the n-left_idx off-by-one truly differs"
    );
    assert_ne!(
        (n - 2 - left_idx, n - 2 - right_idx),
        (4, 3),
        "sanity: the n-2-left_idx off-by-one truly differs"
    );
}

/// Documents (not a load-bearing witness) that `{0,3}` on a 4-slot pattern is its own mirror image under the correct formula, so alone it can't distinguish correct from off-by-one.
#[test]
fn four_slot_outer_placement_is_its_own_mirror_set_not_a_useful_off_by_one_witness() {
    let (mirror_left, mirror_right) = metathesis_mirror_switch_indices(4, 0, 3);
    let mut got = [mirror_left, mirror_right];
    got.sort_unstable();
    assert_eq!(
        got,
        [0, 3],
        "documented, not load-bearing: see this test's own doc"
    );
}
