use super::*;

#[test]
fn dedups_and_assigns_dense_ids() {
    let mut it: Interner<Vec<u64>> = Interner::new();
    let a = it.intern(vec![0b101, 0b011]);
    let b = it.intern(vec![0b111]);
    let a2 = it.intern(vec![0b101, 0b011]); // structurally equal to `a`
    assert_eq!(a, FsId(0));
    assert_eq!(b, FsId(1));
    assert_eq!(a2, a, "equal values must intern to the same id");
    assert_eq!(it.len(), 2);
}

#[test]
fn round_trips_values() {
    let mut it: Interner<Vec<u64>> = Interner::with_capacity(4);
    let id = it.intern(vec![1, 2, 3]);
    assert_eq!(it.get(id), &vec![1, 2, 3]);
    assert_eq!(it.try_get(FsId(99)), None);
}

#[test]
fn iter_is_id_ordered() {
    let mut it: Interner<u32> = Interner::new();
    it.intern(10);
    it.intern(20);
    it.intern(10);
    let collected: Vec<_> = it.iter().map(|(id, v)| (id.0, *v)).collect();
    assert_eq!(collected, vec![(0, 10), (1, 20)]);
}
