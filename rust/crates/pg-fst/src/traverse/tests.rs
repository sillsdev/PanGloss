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

fn result(id: &str, registers: Vec<Register>, order: usize) -> FstResult {
    FstResult {
        id: Some(id.to_string()),
        registers,
        priority: order as i32,
        is_lazy: false,
        next_ann: 0,
        order,
    }
}

/// `distinct` keeps the first occurrence of each `result_eq` class in input order, ignores the fields `result_eq` ignores, and treats every `has == false` register as equal whatever its stale offset.
#[test]
fn distinct_keeps_first_occurrences_in_input_order() {
    let stale_unset = Register {
        offset: 7,
        start: true,
        has: false,
    };
    let input = vec![
        result("b", vec![Register::at(1, true), Register::unset()], 0),
        result("a", vec![Register::at(1, true), Register::unset()], 1),
        result("b", vec![Register::at(1, true), stale_unset], 2), // equal to #0
        result("b", vec![Register::at(2, true), Register::unset()], 3),
        result("a", vec![Register::at(1, true), Register::unset()], 4), // equal to #1
        result("b", vec![Register::at(1, true)], 5),                    // different register count
    ];
    let kept: Vec<usize> = distinct(input).into_iter().map(|r| r.order).collect();
    assert_eq!(kept, vec![0, 1, 3, 5]);
}

/// A larger input whose results share registers across many ids: every distinct id survives once, in order, and the two repeat rounds are dropped.
#[test]
fn distinct_resolves_many_results_by_result_eq() {
    let mut input = Vec::new();
    for round in 0..3 {
        for i in 0..500 {
            input.push(result(
                &format!("id{i}"),
                vec![Register::at(i as i32 % 7, true)],
                round * 500 + i,
            ));
        }
    }
    let kept: Vec<usize> = distinct(input).into_iter().map(|r| r.order).collect();
    assert_eq!(kept, (0..500).collect::<Vec<_>>());
}
