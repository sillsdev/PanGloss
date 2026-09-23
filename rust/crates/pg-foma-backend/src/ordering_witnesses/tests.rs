use super::*;

const A: &str = "prA<prB";
const B: &str = "prB<prA";

#[test]
fn the_lone_contrary_witness_is_kept_with_certainty() {
    // Falsify by sampling below the cap: the lone witness must never be dropped.
    let mut w = OrderingWitnesses::default();
    for idx in 0..39_999 {
        w.observe(A, idx);
    }
    w.observe(B, 28_413);

    assert_eq!(w.count(A), 39_999);
    assert_eq!(w.count(B), 1);
    assert_eq!(
        w.witnesses(B),
        &[28_413],
        "the single minority witness must survive"
    );
}

#[test]
fn counts_are_exact_and_independent_of_the_cap() {
    for cap in [0usize, 1, 3, DEFAULT_WITNESS_CAP, 64] {
        let mut w = OrderingWitnesses::with_cap(cap);
        for idx in 0..1_000 {
            w.observe(A, idx);
        }
        assert_eq!(w.count(A), 1_000, "cap {cap} must not change the count");
        assert!(
            w.witnesses(A).len() <= cap,
            "cap {cap} must bound the sample"
        );
    }
}

#[test]
fn everything_is_kept_below_the_cap_and_nothing_beyond_it() {
    let mut w = OrderingWitnesses::with_cap(10);
    for idx in 0..7 {
        w.observe(A, idx);
    }
    assert_eq!(w.witnesses(A), &[0, 1, 2, 3, 4, 5, 6]);

    for idx in 7..100 {
        w.observe(A, idx);
    }
    assert_eq!(w.witnesses(A).len(), 10);
}

#[test]
fn a_zero_cap_keeps_counts_and_no_witnesses() {
    let mut w = OrderingWitnesses::with_cap(0);
    w.observe(A, 1);
    w.observe(A, 2);
    assert_eq!(w.count(A), 2);
    assert!(w.witnesses(A).is_empty());
}

#[test]
fn the_same_corpus_produces_the_same_witnesses_every_run() {
    // Without this a report cannot be diffed between grammar revisions and no golden can exist.
    let run = || {
        let mut w = OrderingWitnesses::default();
        for idx in 0..5_000 {
            w.observe(A, idx);
        }
        w.witnesses(A).to_vec()
    };
    assert_eq!(run(), run());
}

#[test]
fn the_sample_is_spread_across_the_corpus_not_stuck_in_its_prefix() {
    // A prefix sample would answer "which text came first", not "is this systematic".
    let mut w = OrderingWitnesses::default();
    for idx in 0..40_000 {
        w.observe(A, idx);
    }
    let kept = w.witnesses(A);
    assert_eq!(kept.len(), DEFAULT_WITNESS_CAP);
    assert!(
        kept.iter().any(|&idx| idx > 20_000),
        "a prefix-only sample would fail this: {kept:?}"
    );
}

#[test]
fn keys_are_independent_of_each_other_and_of_interleaving() {
    let mut apart = OrderingWitnesses::default();
    for idx in 0..500 {
        apart.observe(A, idx);
    }
    for idx in 0..500 {
        apart.observe(B, idx);
    }

    let mut interleaved = OrderingWitnesses::default();
    for idx in 0..500 {
        interleaved.observe(A, idx);
        interleaved.observe(B, idx);
    }

    assert_eq!(apart.witnesses(A), interleaved.witnesses(A));
    assert_eq!(apart.witnesses(B), interleaved.witnesses(B));
}

#[test]
fn minority_orderings_lists_the_rare_rows_rarest_first() {
    let mut w = OrderingWitnesses::default();
    for idx in 0..39_999 {
        w.observe(A, idx);
    }
    w.observe(B, 28_413);
    w.observe("prC<prD", 7);
    w.observe("prC<prD", 9);

    assert_eq!(w.minority_orderings(100), vec![(B, 1), ("prC<prD", 2)]);
    assert!(w.minority_orderings(1).is_empty());
}
