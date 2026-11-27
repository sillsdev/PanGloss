use super::*;

#[test]
fn parses_unbounded() {
    assert_eq!("unbounded".parse::<WorkCap>().unwrap(), WorkCap::Unbounded);
}

#[test]
fn parses_a_positive_integer() {
    assert_eq!(
        "100".parse::<WorkCap>().unwrap(),
        WorkCap::Finite(NonZeroU64::new(100).unwrap())
    );
}

#[test]
fn rejects_zero_with_a_specific_message() {
    let err = "0".parse::<WorkCap>().unwrap_err();
    assert!(err.contains("fires before the first work step"), "{err}");
}

#[test]
fn rejects_garbage() {
    assert!("banana".parse::<WorkCap>().is_err());
}

#[test]
fn displays_unbounded_and_the_number() {
    assert_eq!(WorkCap::Unbounded.to_string(), "unbounded");
    assert_eq!(
        WorkCap::Finite(NonZeroU64::new(42).unwrap()).to_string(),
        "42"
    );
}

#[test]
fn storage_round_trips_both_variants() {
    let unbounded = WorkCap::Unbounded;
    assert_eq!(
        WorkCap::from_storage(unbounded.to_storage().unwrap()),
        unbounded
    );
    let finite = WorkCap::Finite(NonZeroU64::new(50_000_000).unwrap());
    assert_eq!(WorkCap::from_storage(finite.to_storage().unwrap()), finite);
}

#[test]
fn as_morpher_cap_maps_unbounded_to_usize_max() {
    assert_eq!(WorkCap::Unbounded.as_morpher_cap(), usize::MAX);
    assert_eq!(
        WorkCap::Finite(NonZeroU64::new(7).unwrap()).as_morpher_cap(),
        7
    );
}
