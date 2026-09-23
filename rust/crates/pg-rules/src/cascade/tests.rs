use super::*;

// A mock rule set over integers: each "rule" i adds a distinct power of ten, so applications commute and terminate.

/// rule i: if digit i (10^i place) of `n` is 0, set it to (i+1); else no output (doesn't apply).
fn digit_rule(i: usize, n: &i64) -> Vec<i64> {
    let place = 10i64.pow(i as u32);
    let digit = (n / place) % 10;
    if digit == 0 {
        vec![n + (i as i64 + 1) * place]
    } else {
        vec![]
    }
}

const UNCAPPED: usize = usize::MAX;

#[test]
fn linear_applies_first_applicable_then_recurses_in_order() {
    // Linear over rules [0,1]: 0 -> rule0 -> 1 -> rule1 -> 21 (terminal); only 21 is added.
    let c = Cascade::new(false, UNCAPPED);
    let out = c.linear(2, 0i64, &digit_rule, &|n: &i64| *n);
    assert_eq!(out.words, vec![21]);
    assert!(!out.capped);
}

#[test]
fn permutation_adds_every_intermediate_subset_in_index_order() {
    // Permutation over [0,1] from 0: every intermediate is added (unlike linear), giving {1, 20, 21}.
    let c = Cascade::new(false, UNCAPPED);
    let mut got = c.permutation(2, 0i64, &digit_rule, &|n: &i64| *n).words;
    got.sort();
    assert_eq!(got, vec![1, 20, 21]);
}

#[test]
fn combination_explores_all_orderings() {
    // Combination reaches every subset in any order, recursing from 0 each level; same set here since the rules commute.
    let c = Cascade::new(false, UNCAPPED);
    let mut got = c.combination(2, 0i64, &digit_rule, &|n: &i64| *n).words;
    got.sort();
    assert_eq!(got, vec![1, 20, 21]);
}

#[test]
fn combination_order_sensitive_reachability() {
    // Rules where order changes what's reachable; asserts a value only reachable via r1-then-r0.
    let r = |i: usize, n: &i64| -> Vec<i64> {
        match i {
            0 if n % 2 == 0 => vec![n + 1],
            1 => vec![n * 2],
            _ => vec![],
        }
    };
    let c = Cascade::new(false, UNCAPPED);
    let got = c.combination(2, 2i64, &r, &|n: &i64| *n).words;
    // 2 --r1--> 4 --r0--> 5 is reachable only because combination retries r0 after r1.
    assert!(
        got.contains(&5),
        "combination must reach 5 via r1-then-r0; got {got:?}"
    );
}

#[test]
fn cap_fires_and_reports_partial() {
    // A rule that always produces a new value → unbounded without the cap.
    let r = |_i: usize, n: &i64| vec![n + 1];
    let c = Cascade::new(true, 50);
    let out = c.permutation(1, 0i64, &r, &|n: &i64| *n);
    assert!(out.capped, "cap must fire on an unbounded multi-app walk");
    assert!(out.words.len() <= 51);
}

#[test]
fn dedup_by_key_collapses_equal_results() {
    // Two rules producing the same value → output holds one copy (HashSet semantics).
    let r = |_i: usize, _n: &i64| vec![7i64];
    let c = Cascade::new(false, UNCAPPED);
    let out = c.permutation(2, 0i64, &r, &|n: &i64| *n);
    assert_eq!(out.words, vec![7]);
}
