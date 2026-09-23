use super::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn hash_of<T: Hash>(v: &T) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/// `RegKey`'s Eq/Hash contract: content-equal keys in DIFFERENT allocations must be equal and must collide, since the `Rc::ptr_eq` fast path is only ever an accelerator for the same-allocation case.
#[test]
fn regkey_eq_and_hash_are_content_based() {
    let a = RegKey(Rc::new(vec![Register::at(1, true), Register::unset()]));
    let b = RegKey(Rc::new(vec![Register::at(1, true), Register::unset()])); // distinct alloc
    let c = RegKey(Rc::clone(&a.0)); // same alloc (ptr_eq fast path)
    let d = RegKey(Rc::new(vec![Register::at(2, true), Register::unset()]));
    assert!(a == b, "content-equal keys in different allocations");
    assert!(a == c, "shared-allocation keys");
    assert!(a != d, "different contents differ");
    assert_eq!(hash_of(&a), hash_of(&b), "equal keys must hash equal");
    assert_eq!(hash_of(&a), hash_of(&c));
    // and RegKey's hash must equal the plain Vec<Register> hash it replaced (same visited-set behavior, representation aside).
    assert_eq!(hash_of(&a), hash_of(&*a.0));
}
