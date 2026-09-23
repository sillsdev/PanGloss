use super::*;

#[test]
fn mask_boundary() {
    // The exact boundary #446's regression test guards: 64 symbols ⇒ all-ones, not 0.
    assert_eq!(full_mask(64), u64::MAX);
    assert_eq!(full_mask(63), (1u64 << 63) - 1);
    assert_eq!(full_mask(20), (1u64 << 20) - 1); // Sena's widest feature (20 symbols)
    assert_eq!(full_mask(1), 1);
    assert_eq!(full_mask(0), 0);
}

#[test]
fn basic_set_ops() {
    let mut s = SymbolBits::EMPTY;
    assert!(!s.has_any());
    assert_eq!(s.first(), None);
    s.set(2);
    s.set(5);
    assert!(s.has_any());
    assert!(s.get(2) && s.get(5) && !s.get(3));
    assert_eq!(s.first(), Some(2));
    assert_eq!(
        s,
        SymbolBits::single(2).union_with(false, SymbolBits::single(5), false, full_mask(8))
    );
}

#[test]
fn has_all_needs_mask() {
    let mask = full_mask(3); // 3 symbols, bits 0..=2
    assert!(SymbolBits(0b111).has_all(mask));
    assert!(!SymbolBits(0b011).has_all(mask));
    assert!(SymbolBits::all(3).has_all(mask));
}

#[test]
fn not_is_complement_within_mask() {
    let mask = full_mask(3);
    assert_eq!(SymbolBits(0b010).not(mask), SymbolBits(0b101));
    // double negation is identity within the mask
    assert_eq!(SymbolBits(0b010).not(mask).not(mask), SymbolBits(0b010));
}

/// Exhaustively verifies the four not/notOther branches of every op against a brute-force reference over a small symbol universe.
#[test]
fn not_matrix_matches_reference() {
    let count = 5u32;
    let mask = full_mask(count);
    let universe: Vec<u64> = (0..(1u64 << count)).collect();
    // brute-force symbol-set helpers
    let neg = |x: u64| !x & mask;
    for &a in &universe {
        for &b in &universe {
            for &not in &[false, true] {
                for &noth in &[false, true] {
                    let ea = if not { neg(a) } else { a };
                    let eb = if noth { neg(b) } else { b };
                    let sa = SymbolBits(a);
                    let sb = SymbolBits(b);
                    assert_eq!(
                        sa.overlaps(not, sb, noth, mask),
                        (ea & eb) != 0,
                        "overlaps a={a:#07b} b={b:#07b} not={not} noth={noth}"
                    );
                    assert_eq!(
                        sa.is_superset_of(not, sb, noth, mask),
                        (ea & eb) == eb,
                        "superset a={a:#07b} b={b:#07b} not={not} noth={noth}"
                    );
                    assert_eq!(sa.intersect_with(not, sb, noth, mask).0, ea & eb);
                    assert_eq!(sa.union_with(not, sb, noth, mask).0, ea | eb);
                    assert_eq!(sa.except_with(not, sb, noth, mask).0, ea & neg(eb));
                }
            }
        }
    }
}

#[test]
fn flat_unify_absent_lane_is_unconstrained() {
    // A shorter slice means trailing features are unconstrained (all-ones), so length mismatch alone never blocks unification.
    let seg = [0b0011u64, 0b0101u64];
    let constraint = [0b0001u64]; // only constrains lane 0; lane 1 absent = unconstrained
    assert!(flat_unifiable(&seg, &constraint));
    assert!(flat_unifiable(&constraint, &seg));
}

#[test]
fn flat_unify_conflict_blocks() {
    let seg = [0b0010u64, 0b0100u64];
    let constraint = [0b0001u64, 0b0100u64]; // lane 0: 0b0010 & 0b0001 == 0 ⇒ conflict
    assert!(!flat_unifiable(&seg, &constraint));
}

#[test]
fn flat_unify_all_lanes_overlap() {
    let seg = [0b0110u64, 0b1100u64, 0b0001u64];
    let constraint = [0b0100u64, 0b1000u64, 0b0001u64];
    assert!(flat_unifiable(&seg, &constraint));
}

#[test]
fn flat_unify_empty_slices() {
    assert!(flat_unifiable(&[], &[])); // vacuously unifiable
    assert!(flat_unifiable(&[0b101], &[])); // constraint says nothing
}
