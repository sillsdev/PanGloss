use super::*;
use pg_featstruct::flat_unifiable;

#[test]
fn unify_absent_is_unconstrained() {
    assert_eq!(flat_unify(&[0b011], &[]), Some(vec![0b011]));
    assert_eq!(
        flat_unify(&[0b011, UNCONSTRAINED], &[0b001]),
        Some(vec![0b001])
    );
}

#[test]
fn unify_conflict_is_none() {
    assert_eq!(flat_unify(&[0b010], &[0b001]), None);
}

#[test]
fn unify_trims_trailing_all_ones() {
    // lane 1 intersects to all-ones -> trimmed away.
    assert_eq!(
        flat_unify(&[0b01, UNCONSTRAINED], &[UNCONSTRAINED, UNCONSTRAINED]),
        Some(vec![0b01])
    );
}

#[test]
fn subsumes_reference() {
    assert!(flat_subsumes(&[0b111], &[0b010]));
    assert!(!flat_subsumes(&[0b010], &[0b111]));
    assert!(flat_subsumes(&[], &[])); // both unconstrained
    assert!(flat_subsumes(&[], &[UNCONSTRAINED])); // canonical vs raw
    assert!(!flat_subsumes(&[0b01], &[])); // constrained cannot subsume unconstrained
}

#[test]
fn unify_agrees_with_unifiable() {
    // flat_unify is Some iff flat_unifiable is true (same lane-AND test).
    let a = [0b0110u64, 0b1100u64];
    let b = [0b0100u64, 0b1000u64];
    assert_eq!(flat_unify(&a, &b).is_some(), flat_unifiable(&a, &b));
    let c = [0b0010u64];
    let d = [0b0001u64];
    assert_eq!(flat_unify(&c, &d).is_some(), flat_unifiable(&c, &d));
}
