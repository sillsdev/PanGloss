use super::*;

#[test]
fn same_name_and_seed_reproduce_the_same_stream() {
    let mut a = Rng::seeded("recipe-a", 42);
    let mut b = Rng::seeded("recipe-a", 42);
    let seq_a: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
    let seq_b: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
    assert_eq!(seq_a, seq_b);
}

#[test]
fn different_name_or_seed_diverges() {
    let mut a = Rng::seeded("recipe-a", 42);
    let mut b = Rng::seeded("recipe-b", 42);
    let mut c = Rng::seeded("recipe-a", 43);
    assert_ne!(a.next_u64(), b.next_u64());
    assert_ne!(a.next_u64(), c.next_u64());
}

#[test]
fn gen_below_stays_in_bounds() {
    let mut r = Rng::seeded("bounds", 7);
    for _ in 0..100 {
        assert!(r.gen_below(5) < 5);
    }
}
