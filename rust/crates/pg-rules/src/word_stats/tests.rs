use super::*;

#[test]
fn percentiles_of_empty_are_zero() {
    assert_eq!(percentiles(&[]), (0, 0, 0));
}

#[test]
fn percentiles_pick_sorted_positions() {
    let v = [1u32, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let (p50, p90, max) = percentiles(&v);
    assert_eq!(max, 10);
    assert!((5..=6).contains(&p50));
    assert!(p90 >= 9);
}
