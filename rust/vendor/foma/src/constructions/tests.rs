//! Wave-4 split: constructions.c unit tests. `use super::*` resolves to the
//! `constructions` module, which re-exports every submodule's public surface,
//! so all `/test` facets keep a stable home here.
use super::*;
use crate::apply::{
    apply_clear, apply_down, apply_init, apply_reset_enumerator, apply_up, apply_words,
};
use crate::regex::fsm_parse_regex;

/* ---- fixtures & helpers ------------------------------------------- */

fn re(s: &str) -> Box<Fsm> {
    let opts = &FomaOptions::default();
    fsm_parse_regex(opts, s, None, None).unwrap_or_else(|| panic!("regex failed to compile: {s:?}"))
}

fn st(state_no: i32, i: i32, o: i32, target: i32, f: i32, s: i32) -> FsmState {
    FsmState {
        state_no,
        r#in: i as i16,
        out: o as i16,
        target,
        final_state: f as i8,
        start_state: s as i8,
    }
}

fn sentinel() -> FsmState {
    st(-1, -1, -1, -1, -1, -1)
}

/// Line table up to (excluding) the state_no == -1 sentinel.
fn lines(net: &Fsm) -> Vec<(i32, i16, i16, i32, i8, i8)> {
    net.states
        .iter()
        .take_while(|l| l.state_no != -1)
        .map(|l| {
            (
                l.state_no,
                l.r#in,
                l.out,
                l.target,
                l.final_state,
                l.start_state,
            )
        })
        .collect()
}

fn sigma_pairs(net: &Fsm) -> Vec<(i32, String)> {
    net.sigma
        .iter()
        .map(|n| (n.number, n.symbol.to_string()))
        .collect()
}

fn sigma_nums(net: &Fsm) -> Vec<i32> {
    sigma_pairs(net).into_iter().map(|(n, _)| n).collect()
}

/// Enumerate the whole (finite acceptor) language, sorted+deduped.
/// Minimizes a copy first (as foma's `print words` does), so raw
/// epsilon arcs from unminimized constructions do not surface as the
/// literal epsilon symbol in the apply enumerator.
fn words(net: &Fsm) -> Vec<String> {
    let opts = &FomaOptions::default();
    let m = fsm_minimize(opts, Box::new(net.clone()));
    let mut h = apply_init(&m);
    apply_reset_enumerator(&mut h);
    let mut out = Vec::new();
    while let Some(w) = apply_words(&mut h) {
        out.push(w);
    }
    apply_clear(h);
    out.sort();
    out.dedup();
    out
}

/// All lower-side outputs of applying `word` downward, sorted+deduped.
fn down(net: &Fsm, word: &str) -> Vec<String> {
    let mut h = apply_init(net);
    let mut out = Vec::new();
    let mut r = apply_down(&mut h, Some(word));
    while let Some(s) = r {
        out.push(s);
        r = apply_down(&mut h, None);
    }
    apply_clear(h);
    out.sort();
    out.dedup();
    out
}

/// All upper-side inputs of applying `word` upward, sorted+deduped.
fn up(net: &Fsm, word: &str) -> Vec<String> {
    let mut h = apply_init(net);
    let mut out = Vec::new();
    let mut r = apply_up(&mut h, Some(word));
    while let Some(s) = r {
        out.push(s);
        r = apply_up(&mut h, None);
    }
    apply_clear(h);
    out.sort();
    out.dedup();
    out
}

fn ws(items: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = items.iter().map(|s| s.to_string()).collect();
    v.sort();
    v
}

/* ---- infrastructure: comparators / renumbering -------------------- */

// [spec:foma:sem:constructions.sort-cmp-fn+1/test]
// [spec:foma:sem:fomalibconf.sort-cmp-fn+1/test]
#[test]
fn sort_cmp_orders_by_state_no() {
    use core::cmp::Ordering;
    assert_eq!(
        sort_cmp(&st(5, 0, 0, 0, 0, 0), &st(2, 0, 0, 0, 0, 0)),
        Ordering::Greater
    );
    assert_eq!(
        sort_cmp(&st(2, 0, 0, 0, 0, 0), &st(5, 0, 0, 0, 0, 0)),
        Ordering::Less
    );
    assert_eq!(
        sort_cmp(&st(4, 0, 0, 0, 0, 0), &st(4, 9, 9, 9, 1, 1)),
        Ordering::Equal
    );
}

// [spec:foma:sem:constructions.fsm-sort-lines-fn/test]
// [spec:foma:sem:fomalibconf.fsm-sort-lines-fn/test]
#[test]
fn fsm_sort_lines_groups_by_state_keeps_sentinel_last() {
    let mut net = fsm_create("");
    net.states = vec![
        st(1, 4, 4, 0, 0, 0),
        st(0, 3, 3, 1, 0, 1),
        st(1, 5, 5, 0, 0, 0),
        sentinel(),
    ];
    fsm_sort_lines(&mut net);
    let states: Vec<i32> = lines(&net).iter().map(|l| l.0).collect();
    assert_eq!(states, vec![0, 1, 1], "grouped by ascending state_no");
    assert_eq!(
        net.states.last().unwrap().state_no,
        -1,
        "sentinel stays last"
    );
}

// [spec:foma:sem:constructions.fsm-update-flags-fn/test]
// [spec:foma:sem:fomalibconf.fsm-update-flags-fn/test]
#[test]
fn fsm_update_flags_assigns_verbatim_and_clears_arc_sort() {
    let mut net = fsm_create("");
    net.arcs_sorted_in = YES;
    net.arcs_sorted_out = YES;
    fsm_update_flags(&mut net, 1, 0, 2, 1, 0, 2);
    assert_eq!(net.is_deterministic, 1);
    assert_eq!(net.is_pruned, 0);
    assert_eq!(net.is_minimized, 2);
    assert_eq!(net.is_epsilon_free, 1);
    assert_eq!(net.is_loop_free, 0);
    assert_eq!(net.is_completed, 2);
    assert_eq!(net.arcs_sorted_in, NO);
    assert_eq!(net.arcs_sorted_out, NO);
}

// [spec:foma:sem:constructions.add-fsm-arc-fn/test]
// [spec:foma:sem:fomalibconf.add-fsm-arc-fn/test]
#[test]
fn add_fsm_arc_writes_line_with_truncation_and_returns_next() {
    let mut arr = vec![st(0, 0, 0, 0, 0, 0); 2];
    let next = add_fsm_arc(&mut arr, 0, 7, 300000, -5, 42, 1, 1);
    assert_eq!(next, 1, "returns offset + 1");
    assert_eq!(arr[0].state_no, 7);
    // int -> short int / int -> char truncation, reproduced verbatim
    assert_eq!(arr[0].r#in, 300000_i32 as i16);
    assert_eq!(arr[0].out, (-5_i32) as i16);
    assert_eq!(arr[0].target, 42);
    assert_eq!(arr[0].final_state, 1);
    assert_eq!(arr[0].start_state, 1);
}

// [spec:foma:sem:constructions.fsm-add-to-states-fn/test]
#[test]
fn fsm_add_to_states_shifts_state_and_target_but_not_minus_one() {
    let mut net = fsm_create("");
    net.states = vec![st(0, 3, 3, 1, 0, 1), st(1, -1, -1, -1, 1, 0), sentinel()];
    fsm_add_to_states(&mut net, 5);
    assert_eq!(lines(&net), vec![(5, 3, 3, 6, 0, 1), (6, -1, -1, -1, 1, 0)]);
}

/* ---- infrastructure: counting / indexing -------------------------- */

// [spec:foma:sem:constructions.fsm-count-states-fn/test]
#[test]
fn fsm_count_states_counts_consecutive_runs_including_the_quirk() {
    let grouped = vec![
        st(0, 0, 0, 0, 0, 0),
        st(0, 0, 0, 0, 0, 0),
        st(1, 0, 0, 0, 0, 0),
        st(2, 0, 0, 0, 0, 0),
        st(2, 0, 0, 0, 0, 0),
        sentinel(),
    ];
    assert_eq!(fsm_count_states(&grouped), 3);
    // Non-adjacent runs of the same state are counted twice (documented quirk)
    let ungrouped = vec![
        st(0, 0, 0, 0, 0, 0),
        st(1, 0, 0, 0, 0, 0),
        st(0, 0, 0, 0, 0, 0),
        sentinel(),
    ];
    assert_eq!(fsm_count_states(&ungrouped), 3);
    // Array starting with the sentinel yields 0
    assert_eq!(fsm_count_states(&[sentinel()]), 0);
}

// [spec:foma:sem:constructions.fsm-count-fn/test]
// [spec:foma:sem:fomalibconf.fsm-count-fn/test]
#[test]
fn fsm_count_recomputes_bookkeeping_with_grouped_finalcount() {
    let mut net = fsm_create("");
    net.states = vec![
        st(0, 3, 3, 1, 0, 1),
        st(1, 4, 4, 2, 1, 0),
        st(1, 5, 5, 0, 1, 0), // second final line of state 1 -> NOT double counted
        st(2, -1, -1, -1, 1, 0),
        sentinel(),
    ];
    fsm_count(&mut net);
    assert_eq!(net.statecount, 3, "maxstate + 1");
    assert_eq!(net.linecount, 5, "non-sentinel lines + 1");
    assert_eq!(net.arccount, 3, "lines with target != -1");
    assert_eq!(net.finalcount, 2, "final runs counted at first line only");
}

// [spec:foma:sem:constructions.init-state-pointers-fn/test]
#[test]
fn init_state_pointers_records_flags_and_first_line_index() {
    let arr = vec![
        st(0, 3, 3, 1, 0, 1),
        st(1, 4, 4, 2, 0, 0),
        st(1, 5, 5, 0, 0, 0),
        st(2, -1, -1, -1, 1, 0),
        sentinel(),
    ];
    let sa = init_state_pointers(&arr);
    assert_eq!(sa.len(), 4, "states + 1 spare entry");
    assert_eq!((sa[0].start, sa[0].r#final, sa[0].transitions), (1, 0, 0));
    assert_eq!((sa[1].start, sa[1].r#final, sa[1].transitions), (0, 0, 1));
    assert_eq!((sa[2].start, sa[2].r#final, sa[2].transitions), (0, 1, 3));
}

/* ---- triplet hash ------------------------------------------------- */

// [spec:foma:sem:constructions.triplet-hash-init-fn/test]
#[test]
fn triplet_hash_init_128_slots_all_empty() {
    let th = triplet_hash_init();
    assert_eq!(th.tablesize, 128);
    assert_eq!(th.occupancy, 0);
    assert_eq!(th.triplets.len(), 128);
    assert!(th.triplets.iter().all(|t| t.key == -1));
}

// [spec:foma:sem:constructions.triplethash-hashf-fn/test]
#[test]
fn triplethash_hashf_exact_constants_with_wrapping() {
    assert_eq!(triplethash_hashf(1, 0, 0), 7907);
    assert_eq!(triplethash_hashf(0, 1, 0), 86028157);
    assert_eq!(triplethash_hashf(0, 0, 1), 7919);
    assert_eq!(triplethash_hashf(1, 1, 1), 86043983);
    // Signed-int overflow that wraps mod 2^32 (values derived independently)
    assert_eq!(triplethash_hashf(i32::MAX, 0, 0), 2147475741);
    assert_eq!(triplethash_hashf(100000, 100000, 100000), 1578806112);
}

// [spec:foma:sem:constructions.triplet-hash-insert-fn/test]
// [spec:foma:sem:constructions.triplet-hash-find-fn/test]
#[test]
fn triplet_hash_insert_find_roundtrip_and_duplicate_quirk() {
    let mut th = triplet_hash_init();
    assert_eq!(
        triplet_hash_insert(&mut th, 5, 6, 7),
        0,
        "keys are consecutive from 0"
    );
    assert_eq!(triplet_hash_insert(&mut th, 8, 9, 10), 1);
    assert_eq!(triplet_hash_find(&th, 5, 6, 7), Some(0));
    assert_eq!(triplet_hash_find(&th, 8, 9, 10), Some(1));
    assert_eq!(
        triplet_hash_find(&th, 100, 100, 100),
        None,
        "absent -> None"
    );
    // Duplicate insert silently creates a second entry with a fresh key,
    // but find still returns the first (home-slot) entry's key.
    assert_eq!(triplet_hash_insert(&mut th, 5, 6, 7), 2);
    assert_eq!(triplet_hash_find(&th, 5, 6, 7), Some(0));
}

// [spec:foma:sem:constructions.triplet-hash-insert-with-key-fn/test]
#[test]
fn triplet_hash_insert_with_key_stores_caller_key_without_occupancy() {
    let mut th = triplet_hash_init();
    triplet_hash_insert_with_key(&mut th, 1, 2, 3, 42);
    assert_eq!(triplet_hash_find(&th, 1, 2, 3), Some(42));
    assert_eq!(th.occupancy, 0, "occupancy untouched");
}

// [spec:foma:sem:constructions.triplet-hash-rehash-fn/test]
// [spec:foma:sem:constructions.triplet-hash-insert-fn/test]
#[test]
fn triplet_hash_rehash_doubles_at_half_occupancy_preserving_keys() {
    let mut th = triplet_hash_init();
    for i in 0..64 {
        triplet_hash_insert(&mut th, i, 0, 0);
    }
    assert_eq!(
        th.tablesize, 128,
        "occupancy 64 == tablesize/2, no rehash yet"
    );
    // 65th distinct entry: occupancy 65 > 64 triggers the doubling.
    triplet_hash_insert(&mut th, 64, 0, 0);
    assert_eq!(th.tablesize, 256);
    assert_eq!(th.occupancy, 65);
    for i in 0..65 {
        assert_eq!(
            triplet_hash_find(&th, i, 0, 0),
            Some(i),
            "keys survive rehash"
        );
    }
}

// [spec:foma:sem:constructions.triplet-hash-free-fn/test]
#[test]
fn triplet_hash_free_is_null_tolerant() {
    triplet_hash_free(None);
    let mut th = triplet_hash_init();
    triplet_hash_insert(&mut th, 1, 2, 3);
    triplet_hash_free(Some(th));
}

/* ---- mergesigma helpers ------------------------------------------- */

// [spec:foma:sem:constructions.add-to-mergesigma-fn/test]
#[test]
fn add_to_mergesigma_dense_ordinary_numbering_and_special_passthrough() {
    let mut head = Mergesigma {
        number: -1,
        symbol: None,
        presence: 0,
        next: None,
    };
    let s_id = Sigma {
        number: IDENTITY,
        symbol: "@_IDENTITY_SYMBOL_@".into(),
    };
    let s_a = Sigma {
        number: 3,
        symbol: "a".into(),
    };
    let s_b = Sigma {
        number: 9,
        symbol: "b".into(),
    };
    {
        let t1 = add_to_mergesigma(&mut head, &s_id, 1);
        let t2 = add_to_mergesigma(t1, &s_a, 2);
        add_to_mergesigma(t2, &s_b, 3);
    }
    // Dummy head overwritten in place; special keeps its number (2), presence stored.
    assert_eq!(
        (head.number, head.symbol.as_deref(), head.presence),
        (2, Some("@_IDENTITY_SYMBOL_@"), 1)
    );
    let n1 = head.next.as_deref().unwrap();
    // First ordinary symbol always numbered 3 regardless of source number.
    assert_eq!(
        (n1.number, n1.symbol.as_deref(), n1.presence),
        (3, Some("a"), 2)
    );
    let n2 = n1.next.as_deref().unwrap();
    // Consecutive ordinary symbols get consecutive dense numbers.
    assert_eq!(
        (n2.number, n2.symbol.as_deref(), n2.presence),
        (4, Some("b"), 3)
    );
    assert!(n2.next.is_none());
}

// [spec:foma:sem:constructions.copy-mergesigma-fn/test]
#[test]
fn copy_mergesigma_deep_copies_number_and_symbol_dropping_presence() {
    assert!(copy_mergesigma(None).is_empty());
    // Dummy-head-only list is copied verbatim (number -1 survives, NULL
    // symbol becomes the empty string).
    let only = Mergesigma {
        number: -1,
        symbol: None,
        presence: 0,
        next: None,
    };
    let s = copy_mergesigma(Some(&only));
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].number, -1);
    assert_eq!(s[0].symbol, "");
    // Multi-node list keeps number+symbol, in order.
    let m = Mergesigma {
        number: IDENTITY,
        symbol: Some("@_IDENTITY_SYMBOL_@".into()),
        presence: 3,
        next: Some(Box::new(Mergesigma {
            number: 3,
            symbol: Some("a".into()),
            presence: 1,
            next: None,
        })),
    };
    let s = copy_mergesigma(Some(&m));
    assert_eq!(
        s.iter()
            .map(|e| (e.number, e.symbol.as_str()))
            .collect::<Vec<_>>(),
        vec![(2, "@_IDENTITY_SYMBOL_@"), (3, "a")]
    );
}

/* ---- fsm_merge_sigma ---------------------------------------------- */

// [spec:foma:sem:constructions.fsm-merge-sigma-fn/test]
// [spec:foma:sem:fomalib.fsm-merge-sigma-fn/test]
#[test]
fn fsm_merge_sigma_disjoint_alphabets_shared_dense_numbering() {
    let opts = &FomaOptions::default();
    let mut n1 = re("a");
    let mut n2 = re("b");
    fsm_merge_sigma(opts, &mut n1, &mut n2);
    // Both nets end with one shared, densely renumbered sigma.
    assert_eq!(
        sigma_pairs(&n1),
        vec![(3, "a".to_string()), (4, "b".to_string())]
    );
    assert_eq!(
        sigma_pairs(&n2),
        vec![(3, "a".to_string()), (4, "b".to_string())]
    );
    // net1's arc keeps 3; net2's "b" arc is remapped from 3 to 4.
    assert_eq!(lines(&n1), vec![(0, 3, 3, 1, 0, 1), (1, -1, -1, -1, 1, 0)]);
    assert_eq!(lines(&n2), vec![(0, 4, 4, 1, 0, 1), (1, -1, -1, -1, 1, 0)]);
}

// [spec:foma:sem:constructions.fsm-merge-sigma-fn/test]
// [spec:foma:sem:fomalib.fsm-merge-sigma-fn/test]
#[test]
fn fsm_merge_sigma_expands_identity_over_other_nets_symbols() {
    let opts = &FomaOptions::default();
    // ? (IDENTITY) merged with {a}: the @:@ arc is expanded with an
    // explicit a:a arc so ? cannot silently match the new symbol a.
    let mut id = fsm_identity();
    let mut na = re("a");
    fsm_merge_sigma(opts, &mut id, &mut na);
    assert_eq!(sigma_nums(&id), vec![2, 3]);
    assert_eq!(sigma_nums(&na), vec![2, 3]);
    assert_eq!(
        lines(&id),
        vec![
            (0, 2, 2, 1, 0, 1),
            (0, 3, 3, 1, 0, 1),
            (1, -1, -1, -1, 1, 0)
        ],
        "IDENTITY arc plus one expansion arc per other-net symbol"
    );
    // na had no wildcard, so it is only renumbered, not expanded.
    assert_eq!(lines(&na), vec![(0, 3, 3, 1, 0, 1), (1, -1, -1, -1, 1, 0)]);
}

/* ---- product constructions: language + structure ------------------ */

// [spec:foma:sem:constructions.fsm-compose-fn/test]
// [spec:foma:sem:fomalib.fsm-compose-fn/test]
#[test]
fn fsm_compose_relates_input_to_output() {
    let opts = &FomaOptions::default();
    // a:b .o. b:c == a:c
    let c = fsm_compose(opts, re("a:b"), re("b:c"));
    assert_eq!(down(&c, "a"), ws(&["c"]));
    assert_eq!(up(&c, "c"), ws(&["a"]));
    assert_eq!(
        down(&c, "b"),
        Vec::<String>::new(),
        "b is not an upper string"
    );
}

// [spec:foma:sem:constructions.fsm-compose-fn/test]
// [spec:foma:sem:fomalib.fsm-compose-fn/test]
#[test]
fn fsm_compose_non_matching_middle_is_empty() {
    let opts = &FomaOptions::default();
    let mut c = fsm_compose(opts, re("a:b"), re("c:d"));
    assert!(
        fsm_isempty(opts, &mut c),
        "b != c yields the empty language"
    );
}

// [spec:foma:sem:constructions.fsm-intersect-fn/test]
// [spec:foma:sem:fomalib.fsm-intersect-fn/test]
#[test]
fn fsm_intersect_keeps_common_strings() {
    let opts = &FomaOptions::default();
    assert_eq!(
        words(&fsm_intersect(opts, re("a|b|c"), re("b|c|d"))),
        ws(&["b", "c"])
    );
}

// [spec:foma:sem:constructions.fsm-intersect-fn/test]
// [spec:foma:sem:fomalib.fsm-intersect-fn/test]
#[test]
fn fsm_intersect_identity_matches_ordinary_after_merge_expansion() {
    let opts = &FomaOptions::default();
    // [?] & [a] == a : merge_sigma expands ? to include an explicit a:a arc.
    assert_eq!(
        words(&fsm_intersect(opts, fsm_identity(), re("a"))),
        ws(&["a"])
    );
}

// [spec:foma:sem:constructions.fsm-intersect-fn/test]
// [spec:foma:sem:fomalib.fsm-intersect-fn/test]
#[test]
fn fsm_intersect_disjoint_is_empty() {
    let opts = &FomaOptions::default();
    let mut r = fsm_intersect(opts, re("a"), re("b"));
    assert!(fsm_isempty(opts, &mut r));
}

// [spec:foma:sem:constructions.fsm-cross-product-fn/test]
// [spec:foma:sem:fomalib.fsm-cross-product-fn/test]
#[test]
fn fsm_cross_product_pairs_languages() {
    let opts = &FomaOptions::default();
    let x = fsm_cross_product(opts, re("a"), re("b"));
    assert_eq!(down(&x, "a"), ws(&["b"]));
    assert_eq!(up(&x, "b"), ws(&["a"]));
}

// [spec:foma:sem:constructions.fsm-cross-product-fn/test]
// [spec:foma:sem:fomalib.fsm-cross-product-fn/test]
#[test]
fn fsm_cross_product_unequal_lengths_stay_in_final_state() {
    let opts = &FomaOptions::default();
    // [a b] .x. [c] : upper "ab" maps to lower "c".
    let x = fsm_cross_product(opts, re("a b"), re("c"));
    assert_eq!(down(&x, "ab"), ws(&["c"]));
    assert_eq!(up(&x, "c"), ws(&["ab"]));
}

// [spec:foma:sem:constructions.fsm-minus-fn/test]
// [spec:foma:sem:fomalib.fsm-minus-fn/test]
#[test]
fn fsm_minus_removes_second_language() {
    let opts = &FomaOptions::default();
    assert_eq!(
        words(&fsm_minus(opts, re("a|b|c"), re("b"))),
        ws(&["a", "c"])
    );
    assert_eq!(words(&fsm_minus(opts, re("a|b"), re("a"))), ws(&["b"]));
}

// [spec:foma:sem:constructions.fsm-minus-fn/test]
// [spec:foma:sem:fomalib.fsm-minus-fn/test]
#[test]
fn fsm_minus_lets_b_go_dead_and_accepts_remainder() {
    let opts = &FomaOptions::default();
    // [a b] - [a]: B accepts only "a", then goes dead; "ab" survives.
    assert_eq!(words(&fsm_minus(opts, re("a b"), re("a"))), ws(&["ab"]));
}

/* ---- boolean / closure constructions ------------------------------ */

// [spec:foma:sem:constructions.fsm-union-fn/test]
// [spec:foma:sem:fomalib.fsm-union-fn/test]
#[test]
fn fsm_union_epsilon_start_construction() {
    let opts = &FomaOptions::default();
    let u = fsm_union(opts, re("a"), re("b"));
    assert_eq!(words(&u), ws(&["a", "b"]));
    // Two epsilon start arcs from a fresh state 0, then both operands shifted.
    assert_eq!(
        lines(&u),
        vec![
            (0, 0, 0, 1, 0, 1),
            (0, 0, 0, 3, 0, 1),
            (1, 3, 3, 2, 0, 0),
            (2, -1, -1, -1, 1, 0),
            (3, 4, 4, 4, 0, 0),
            (4, -1, -1, -1, 1, 0),
        ]
    );
    assert_eq!(net_counts(&u), (5, 7, 4, 2));
    assert_eq!(
        u.is_deterministic, NO,
        "union leaves a nondeterministic net"
    );
}

// [spec:foma:sem:constructions.fsm-concat-fn/test]
// [spec:foma:sem:fomalib.fsm-concat-fn/test]
#[test]
fn fsm_concat_splices_languages() {
    let opts = &FomaOptions::default();
    assert_eq!(words(&fsm_concat(opts, re("a"), re("b"))), ws(&["ab"]));
}

// [spec:foma:sem:constructions.fsm-concat-fn/test]
// [spec:foma:sem:fomalib.fsm-concat-fn/test]
#[test]
fn fsm_concat_with_empty_language_is_empty() {
    let opts = &FomaOptions::default();
    let mut r = fsm_concat(opts, re("a"), fsm_empty_set());
    assert!(
        fsm_isempty(opts, &mut r),
        "no final state in an operand -> empty"
    );
}

// [spec:foma:sem:constructions.fsm-complement-fn/test]
// [spec:foma:sem:fomalib.fsm-complement-fn/test]
// [spec:foma:sem:constructions.fsm-completes-fn/test]
#[test]
fn fsm_complement_negates_over_extended_alphabet() {
    let opts = &FomaOptions::default();
    let c = fsm_complement(opts, re("a"));
    assert_eq!(c.is_completed, YES);
    assert_eq!(down(&c, ""), ws(&[""]), "empty string is in ~[a]");
    assert_eq!(down(&c, "a"), Vec::<String>::new(), "a is excluded");
    assert_eq!(down(&c, "aa"), ws(&["aa"]));
    assert_eq!(
        down(&c, "z"),
        ws(&["z"]),
        "unknown symbol accepted via IDENTITY"
    );
}

// [spec:foma:sem:constructions.fsm-complement-fn/test]
// [spec:foma:sem:fomalib.fsm-complement-fn/test]
#[test]
fn fsm_complement_is_involutive() {
    let opts = &FomaOptions::default();
    let cc = fsm_complement(opts, fsm_complement(opts, re("a")));
    assert_eq!(down(&cc, "a"), ws(&["a"]));
    assert_eq!(down(&cc, ""), Vec::<String>::new());
    assert_eq!(down(&cc, "aa"), Vec::<String>::new());
}

// [spec:foma:sem:constructions.fsm-complete-fn/test]
// [spec:foma:sem:fomalib.fsm-complete-fn/test]
// [spec:foma:sem:constructions.fsm-completes-fn/test]
#[test]
fn fsm_complete_preserves_language_but_marks_completed() {
    let opts = &FomaOptions::default();
    let comp = fsm_complete(opts, re("a"));
    assert_eq!(comp.is_completed, YES);
    assert_eq!(down(&comp, "a"), ws(&["a"]));
    assert_eq!(down(&comp, ""), Vec::<String>::new());
    assert_eq!(
        down(&comp, "b"),
        Vec::<String>::new(),
        "b routes to the non-final sink"
    );
}

// [spec:foma:sem:constructions.fsm-kleene-star-fn/test]
// [spec:foma:sem:fomalib.fsm-kleene-star-fn/test]
// [spec:foma:sem:constructions.fsm-kleene-closure-fn/test]
#[test]
fn fsm_kleene_star_exact_shape_and_language() {
    let opts = &FomaOptions::default();
    let s = fsm_kleene_star(opts, re("a"));
    assert_eq!(
        lines(&s),
        vec![(0, 0, 0, 1, 1, 1), (1, 3, 3, 2, 0, 0), (2, 0, 0, 0, 1, 0)],
        "prepended final start state 0, closure epsilon back to 0"
    );
    assert_eq!(net_counts(&s), (3, 4, 3, 2));
    assert_eq!(s.pathcount, PATHCOUNT_UNKNOWN);
    assert_eq!(down(&s, ""), ws(&[""]));
    assert_eq!(down(&s, "aa"), ws(&["aa"]));
    assert_eq!(down(&s, "b"), Vec::<String>::new());
}

// [spec:foma:sem:constructions.fsm-kleene-plus-fn/test]
// [spec:foma:sem:fomalib.fsm-kleene-plus-fn/test]
// [spec:foma:sem:constructions.fsm-kleene-closure-fn/test]
#[test]
fn fsm_kleene_plus_exact_shape_and_language() {
    let opts = &FomaOptions::default();
    let p = fsm_kleene_plus(opts, re("a"));
    // Same as star but the prepended start state 0 is NOT final.
    assert_eq!(
        lines(&p),
        vec![(0, 0, 0, 1, 0, 1), (1, 3, 3, 2, 0, 0), (2, 0, 0, 0, 1, 0)]
    );
    assert_eq!(net_counts(&p), (3, 4, 3, 1));
    assert_eq!(
        down(&p, ""),
        Vec::<String>::new(),
        "empty string not accepted by A+"
    );
    assert_eq!(down(&p, "a"), ws(&["a"]));
    assert_eq!(down(&p, "aa"), ws(&["aa"]));
}

// [spec:foma:sem:constructions.fsm-optionality-fn/test]
// [spec:foma:sem:fomalib.fsm-optionality-fn/test]
// [spec:foma:sem:constructions.fsm-kleene-closure-fn/test]
#[test]
fn fsm_optionality_short_circuits_to_union_with_empty_string() {
    let opts = &FomaOptions::default();
    let o = fsm_optionality(opts, re("a"));
    assert_eq!(words(&o), ws(&["", "a"]));
    // Built by the union construction, so nondeterministic with an epsilon start.
    assert_eq!(o.is_deterministic, NO);
    assert!(sigma_find_number(EPSILON, &o.sigma).is_some());
}

/* ---- rule-compilation helper -------------------------------------- */

// [spec:foma:sem:constructions.fsm-mark-fsm-tail-fn/test]
// [spec:foma:sem:fomalib.fsm-mark-fsm-tail-fn/test]
#[test]
fn fsm_mark_fsm_tail_inserts_marker_before_final_states() {
    // Reroute every arc entering a final state of {a} through the marker x.
    let marker = re("x");
    let m = fsm_mark_fsm_tail(re("a"), &marker);
    // All original states become final (so "" is accepted); the a-arc is
    // rerouted a then x into the old final state.
    assert_eq!(words(&m), ws(&["", "ax"]));
    assert_eq!(
        down(&m, "a"),
        Vec::<String>::new(),
        "fresh marker state is non-final"
    );
}

/* ==== SLICE 2: elementary machines, derived operators, substitutions ==== */

/// Sorted set of sigma symbol strings (ordinary + special), for structural
/// assertions on constructed alphabets.
fn syms(net: &Fsm) -> Vec<String> {
    let mut v: Vec<String> = sigma_pairs(net).into_iter().map(|(_, s)| s).collect();
    v.sort();
    v
}

/* ---- elementary single-symbol machines ---------------------------- */

// [spec:foma:sem:constructions.fsm-symbol-fn/test]
// [spec:foma:sem:fomalib.fsm-symbol-fn/test]
#[test]
fn fsm_symbol_ordinary_exact_shape_flags_and_language() {
    let net = fsm_symbol("a");
    assert_eq!(lines(&net), vec![(0, 3, 3, 1, 0, 1), (1, -1, -1, -1, 1, 0)]);
    assert_eq!(sigma_pairs(&net), vec![(3, "a".to_string())]);
    assert_eq!(net_counts(&net), (2, 2, 1, 1));
    assert_eq!((net.arity, net.pathcount), (1, 1));
    assert_eq!(net.is_deterministic, YES);
    assert_eq!(net.is_minimized, YES);
    assert_eq!(net.is_epsilon_free, YES);
    assert_eq!((net.arcs_sorted_in, net.arcs_sorted_out), (YES, YES));
    assert_eq!(down(&net, "a"), ws(&["a"]));
}

// [spec:foma:sem:constructions.fsm-symbol-fn/test]
// [spec:foma:sem:fomalib.fsm-symbol-fn/test]
#[test]
fn fsm_symbol_epsilon_is_final_start_with_overridden_no_flags() {
    let net = fsm_symbol("@_EPSILON_SYMBOL_@");
    assert_eq!(lines(&net), vec![(0, -1, -1, -1, 1, 1)]);
    assert_eq!(
        sigma_pairs(&net),
        vec![(0, "@_EPSILON_SYMBOL_@".to_string())]
    );
    assert_eq!(net_counts(&net), (1, 1, 0, 1));
    // Literal quirk: the epsilon machine is trivially det/min/eps-free but
    // fsm_symbol overrides all three to NO.
    assert_eq!(net.is_deterministic, NO);
    assert_eq!(net.is_minimized, NO);
    assert_eq!(net.is_epsilon_free, NO);
    assert_eq!(words(&net), ws(&[""]));
}

// [spec:foma:sem:constructions.fsm-symbol-fn/test]
// [spec:foma:sem:fomalib.fsm-symbol-fn/test]
#[test]
fn fsm_symbol_identity_uses_special_number_two() {
    let net = fsm_symbol("@_IDENTITY_SYMBOL_@");
    assert_eq!(lines(&net), vec![(0, 2, 2, 1, 0, 1), (1, -1, -1, -1, 1, 0)]);
    assert_eq!(sigma_nums(&net), vec![IDENTITY]);
    assert_eq!(
        down(&net, "z"),
        ws(&["z"]),
        "IDENTITY matches any single symbol"
    );
    assert!(down(&net, "").is_empty());
}

// [spec:foma:sem:constructions.fsm-escape-fn/test]
// [spec:foma:sem:fomalib.fsm-escape-fn/test]
#[test]
fn fsm_escape_skips_the_first_byte() {
    // "%a" -> fsm_symbol("a")
    let net = fsm_escape("%a");
    assert_eq!(sigma_pairs(&net), vec![(3, "a".to_string())]);
    assert_eq!(down(&net, "a"), ws(&["a"]));
    // A different escape char is equally ignored; only the tail matters.
    assert_eq!(down(&fsm_escape("\\+"), "+"), ws(&["+"]));
}

// [spec:foma:sem:constructions.fsm-explode-fn+1/test]
// [spec:foma:sem:fomalib.fsm-explode-fn+1/test]
#[test]
fn fsm_explode_spells_out_payload() {
    // One arc per UTF-8 char of the payload (no delimiters, unlike C).
    let net = fsm_explode("cat");
    assert_eq!(words(&net), ws(&["cat"]));
    assert_eq!(
        syms(&net),
        ws(&["a", "c", "t"]),
        "each char enters the sigma"
    );
    // Empty payload yields the single-state empty-string machine.
    assert_eq!(words(&fsm_explode("")), ws(&[""]));
    // Multi-byte chars are whole symbols, wherever they sit in the payload.
    assert_eq!(words(&fsm_explode("éxé")), ws(&["éxé"]));
    assert_eq!(syms(&fsm_explode("éxé")), ws(&["x", "é"]));
}

// [spec:foma:sem:constructions.fsm-universal-fn/test]
// [spec:foma:sem:fomalib.fsm-universal-fn/test]
#[test]
fn fsm_universal_is_the_identity_self_loop() {
    let net = fsm_universal();
    assert_eq!(lines(&net), vec![(0, 2, 2, 0, 1, 1)]);
    assert_eq!(sigma_nums(&net), vec![IDENTITY]);
    assert_eq!(net_counts(&net), (1, 2, 1, 1));
    assert_eq!(net.pathcount, PATHCOUNT_CYCLIC);
    assert_eq!(down(&net, ""), ws(&[""]));
    assert_eq!(down(&net, "abc"), ws(&["abc"]));
}

// [spec:foma:sem:constructions.fsm-network-to-char-fn/test]
// [spec:foma:sem:fomalib.fsm-network-to-char-fn/test]
#[test]
fn fsm_network_to_char_returns_last_highest_numbered_symbol() {
    assert_eq!(fsm_network_to_char(&fsm_symbol("a")).as_deref(), Some("a"));
    // Sigma is sorted by number: the last node is the highest, here "c".
    assert_eq!(fsm_network_to_char(&re("a b c")).as_deref(), Some("c"));
    // Empty-sigma dummy (number -1) -> None.
    assert!(fsm_network_to_char(&fsm_empty_string()).is_none());
}

/* ---- bounded repetition ------------------------------------------- */

// [spec:foma:sem:constructions.fsm-concat-m-n-fn/test]
// [spec:foma:sem:fomalib.fsm-concat-m-n-fn/test]
#[test]
fn fsm_concat_m_n_bounded_repetition() {
    let opts = &FomaOptions::default();
    assert_eq!(
        words(&fsm_concat_m_n(opts, re("a"), 1, 3)),
        ws(&["a", "aa", "aaa"])
    );
    assert_eq!(
        words(&fsm_concat_m_n(opts, re("a"), 0, 2)),
        ws(&["", "a", "aa"])
    );
    // m > n: all n copies mandatory (A^n).
    assert_eq!(words(&fsm_concat_m_n(opts, re("a"), 5, 2)), ws(&["aa"]));
    // n < 1: empty-string language regardless of m.
    assert_eq!(words(&fsm_concat_m_n(opts, re("a"), 3, 0)), ws(&[""]));
}

// [spec:foma:sem:constructions.fsm-concat-n-fn/test]
// [spec:foma:sem:fomalib.fsm-concat-n-fn/test]
#[test]
fn fsm_concat_n_exact_repetition() {
    let opts = &FomaOptions::default();
    assert_eq!(words(&fsm_concat_n(opts, re("a"), 3)), ws(&["aaa"]));
    assert_eq!(
        words(&fsm_concat_n(opts, re("a"), 0)),
        ws(&[""]),
        "n < 1 -> empty string"
    );
}

/* ---- letter machine ----------------------------------------------- */

// [spec:foma:sem:constructions.fsm-letter-machine-fn+1/test]
// [spec:foma:sem:fomalib.fsm-letter-machine-fn+1/test]
#[test]
fn fsm_letter_machine_splits_multichar_symbol_and_names_it_literally() {
    let opts = &FomaOptions::default();
    // The single 3-char symbol "abc" becomes a chain a b c.
    let lm = fsm_letter_machine(opts, fsm_symbol("abc"));
    assert_eq!(
        lm.name, "name",
        "output name is the literal \"name\", not preserved"
    );
    assert_eq!(words(&lm), ws(&["abc"]));
    assert_eq!(
        syms(&lm),
        ws(&["a", "b", "c"]),
        "sigma rebuilt from single letters"
    );
}

// [spec:foma:sem:constructions.fsm-letter-machine-fn+1/test]
// [spec:foma:sem:fomalib.fsm-letter-machine-fn+1/test]
#[test]
fn fsm_letter_machine_splits_multibyte_output_across_letters() {
    let opts = &FomaOptions::default();
    // a:"éé" — the output character (2 bytes) is longer than the input
    // character (1 byte). Wave 4 fix: the output copy is sized by
    // utf8skip(out), so each step copies one full "é"; the chain is
    // a:é then 0:é, and applying "a" downward yields the intact "éé".
    let t = fsm_cross_product(opts, fsm_symbol("a"), fsm_symbol("éé"));
    let lm = fsm_letter_machine(opts, t);
    assert_eq!(down(&lm, "a"), ws(&["éé"]));
}

/* ---- substitutions ------------------------------------------------ */

// [spec:foma:sem:constructions.fsm-substitute-label-fn/test]
// [spec:foma:sem:fomalib.fsm-substitute-label-fn/test]
#[test]
fn fsm_substitute_label_splices_network_for_double_sided_arc() {
    let opts = &FomaOptions::default();
    // Replace the a:a arc of "ab" with the network "xy": "ab" -> "xyb".
    let mut net = re("a b");
    let mut sub = re("x y");
    let r = fsm_substitute_label(opts, &mut net, "a", &mut sub);
    assert_eq!(words(&r), ws(&["xyb"]));
}

// [spec:foma:sem:constructions.fsm-substitute-label-fn/test]
// [spec:foma:sem:fomalib.fsm-substitute-label-fn/test]
#[test]
fn fsm_substitute_label_one_sided_pairs_substitute_with_other_side() {
    let opts = &FomaOptions::default();
    // a:b, replace label "a" (input side) with "x": arc becomes [x .x. b].
    let mut net = re("a:b");
    let mut sub = re("x");
    let r = fsm_substitute_label(opts, &mut net, "a", &mut sub);
    assert_eq!(down(&r, "x"), ws(&["b"]));
    assert_eq!(up(&r, "b"), ws(&["x"]));
}

// [spec:foma:sem:constructions.fsm-substitute-label-fn/test]
// [spec:foma:sem:fomalib.fsm-substitute-label-fn/test]
#[test]
fn fsm_substitute_label_absent_symbol_returns_net_unchanged() {
    let opts = &FomaOptions::default();
    let mut net = re("a b");
    let mut sub = re("x");
    let r = fsm_substitute_label(opts, &mut net, "z", &mut sub);
    assert_eq!(words(&r), ws(&["ab"]));
}

// [spec:foma:sem:constructions.fsm-substitute-symbol-fn/test]
// [spec:foma:sem:fomalib.fsm-substitute-symbol-fn/test]
#[test]
fn fsm_substitute_symbol_renames_and_epsilon_and_noops() {
    assert_eq!(
        words(&fsm_substitute_symbol(re("a b a"), "a", "x")),
        ws(&["xbx"])
    );
    // "0" substitutes toward EPSILON.
    assert_eq!(
        words(&fsm_substitute_symbol(re("a b"), "a", "0")),
        ws(&["b"])
    );
    // Same name -> untouched; absent original -> untouched.
    assert_eq!(words(&fsm_substitute_symbol(re("a"), "a", "a")), ws(&["a"]));
    assert_eq!(words(&fsm_substitute_symbol(re("a"), "z", "y")), ws(&["a"]));
}

/* ---- precedence / follows (copy-only, neither consumed) ----------- */

// [spec:foma:sem:constructions.fsm-precedes-fn/test]
// [spec:foma:sem:fomalib.fsm-precedes-fn/test]
#[test]
fn fsm_precedes_is_not_b_then_a_and_consumes_neither() {
    let opts = &FomaOptions::default();
    // ~$[B ?* A] with A={a}, B={b}: reject any b later followed by an a.
    let mut n1 = re("a");
    let mut n2 = re("b");
    let p = fsm_precedes(opts, &mut n1, &mut n2);
    assert_eq!(down(&p, "ab"), ws(&["ab"]));
    assert_eq!(down(&p, "aab"), ws(&["aab"]));
    assert_eq!(down(&p, "bb"), ws(&["bb"]));
    assert!(down(&p, "ba").is_empty());
    assert!(down(&p, "aba").is_empty());
    // Neither operand is consumed.
    assert_eq!(words(&n1), ws(&["a"]));
    assert_eq!(words(&n2), ws(&["b"]));
}

// [spec:foma:sem:constructions.fsm-follows-fn/test]
// [spec:foma:sem:fomalib.fsm-follows-fn/test]
#[test]
fn fsm_follows_is_not_a_then_b_and_consumes_neither() {
    let opts = &FomaOptions::default();
    // ~$[A ?* B] with A={a}, B={b}: reject any a later followed by a b.
    let mut n1 = re("a");
    let mut n2 = re("b");
    let f = fsm_follows(opts, &mut n1, &mut n2);
    assert_eq!(down(&f, "ba"), ws(&["ba"]));
    assert!(down(&f, "ab").is_empty());
    assert!(down(&f, "aba").is_empty());
    assert_eq!(words(&n1), ws(&["a"]));
    assert_eq!(words(&n2), ws(&["b"]));
}

/* ---- flatten / unflatten ------------------------------------------ */

// [spec:foma:sem:constructions.fsm-flatten-fn+1/test]
// [spec:foma:sem:fomalib.fsm-flatten-fn+1/test]
#[test]
fn fsm_flatten_splits_pairs_into_identity_arcs() {
    let opts = &FomaOptions::default();
    // Normal path: a:b -> acceptor "ab".
    let flat = fsm_flatten(opts, re("a:b"), fsm_symbol("E")).unwrap();
    assert_eq!(words(&flat), ws(&["ab"]));
    // EPSILON on a side is replaced by the epsilon machine's symbol "E".
    let a0 = fsm_cross_product(opts, fsm_symbol("a"), fsm_empty_string());
    let flat2 = fsm_flatten(opts, a0, fsm_symbol("E")).unwrap();
    assert_eq!(words(&flat2), ws(&["aE"]));
}

// [spec:foma:sem:constructions.fsm-flatten-fn+1/test]
// [spec:foma:sem:fomalib.fsm-flatten-fn+1/test]
#[test]
fn fsm_flatten_none_when_epsilon_machine_has_no_arcs() {
    let opts = &FomaOptions::default();
    // An arc-less epsilon machine (empty-set: a single non-final start, no arcs)
    // has no first arc, so fsm_flatten returns None. C tested fsm_get_next_arc
    // == -1 (never returned), fell through, and read an invalid arc (panic here).
    assert!(fsm_flatten(opts, re("a:b"), fsm_empty_set()).is_none());
}

// [spec:foma:sem:constructions.fsm-unflatten-fn/test]
// [spec:foma:sem:fomalib.fsm-unflatten-fn/test]
#[test]
fn fsm_unflatten_pairs_even_odd_symbols_into_transducer() {
    let opts = &FomaOptions::default();
    // Acceptor "ab" pairs a (even) with b (odd) -> transducer a:b.
    let flat = fsm_concat(opts, fsm_symbol("a"), fsm_symbol("b"));
    let t = fsm_unflatten(opts, flat, "E", "R");
    assert_eq!(down(&t, "a"), ws(&["b"]));
    // Odd symbol == epsilon_sym "E" -> output EPSILON (a:0).
    let flat_eps = fsm_concat(opts, fsm_symbol("a"), fsm_symbol("E"));
    let t_eps = fsm_unflatten(opts, flat_eps, "E", "R");
    assert_eq!(down(&t_eps, "a"), ws(&[""]));
    // Odd symbol == repeat_sym "R" -> output equals input (a:a).
    let flat_rep = fsm_concat(opts, fsm_symbol("a"), fsm_symbol("R"));
    let t_rep = fsm_unflatten(opts, flat_rep, "E", "R");
    assert_eq!(down(&t_rep, "a"), ws(&["a"]));
}

/* ---- shuffle / equivalence ---------------------------------------- */

// [spec:foma:sem:constructions.fsm-shuffle-fn/test]
// [spec:foma:sem:fomalib.fsm-shuffle-fn/test]
#[test]
fn fsm_shuffle_interleaves_both_languages() {
    let opts = &FomaOptions::default();
    assert_eq!(
        words(&fsm_shuffle(opts, re("a"), re("b"))),
        ws(&["ab", "ba"])
    );
    assert_eq!(
        words(&fsm_shuffle(opts, re("a"), re("b c"))),
        ws(&["abc", "bac", "bca"])
    );
}

// [spec:foma:sem:constructions.fsm-equivalent-fn/test]
// [spec:foma:sem:fomalib.fsm-equivalent-fn/test]
#[test]
fn fsm_equivalent_tests_path_equivalence() {
    let opts = &FomaOptions::default();
    assert!(fsm_equivalent(opts, re("a|b"), re("b|a")));
    assert!(fsm_equivalent(opts, re("a b"), re("a b")));
    assert!(!(fsm_equivalent(opts, re("a"), re("b"))));
}

/* ---- contains family ---------------------------------------------- */

// [spec:foma:sem:constructions.fsm-contains-fn/test]
// [spec:foma:sem:fomalib.fsm-contains-fn/test]
#[test]
fn fsm_contains_matches_strings_with_a_factor() {
    let opts = &FomaOptions::default();
    let c = fsm_contains(opts, re("a"));
    assert_eq!(down(&c, "a"), ws(&["a"]));
    assert_eq!(down(&c, "bac"), ws(&["bac"]));
    assert!(down(&c, "").is_empty());
    assert!(down(&c, "bbb").is_empty());
}

// [spec:foma:sem:constructions.fsm-contains-one-fn/test]
// [spec:foma:sem:fomalib.fsm-contains-one-fn/test]
#[test]
fn fsm_contains_one_matches_exactly_one_occurrence() {
    let opts = &FomaOptions::default();
    let c = fsm_contains_one(opts, re("a"));
    assert_eq!(down(&c, "a"), ws(&["a"]));
    assert_eq!(down(&c, "bab"), ws(&["bab"]));
    assert!(down(&c, "aba").is_empty(), "two occurrences excluded");
    assert!(down(&c, "aa").is_empty());
    assert!(down(&c, "b").is_empty(), "zero occurrences excluded");
}

// [spec:foma:sem:constructions.fsm-contains-opt-one-fn/test]
// [spec:foma:sem:fomalib.fsm-contains-opt-one-fn/test]
#[test]
fn fsm_contains_opt_one_matches_at_most_one() {
    let opts = &FomaOptions::default();
    let c = fsm_contains_opt_one(opts, re("a"));
    assert_eq!(down(&c, "a"), ws(&["a"]));
    assert_eq!(down(&c, "b"), ws(&["b"]), "zero occurrences allowed");
    assert_eq!(down(&c, "bab"), ws(&["bab"]));
    assert!(down(&c, "aa").is_empty(), "two occurrences excluded");
}

/* ---- replace / priority / lenient --------------------------------- */

// [spec:foma:sem:constructions.fsm-simple-replace-fn/test]
// [spec:foma:sem:fomalib.fsm-simple-replace-fn/test]
#[test]
fn fsm_simple_replace_is_obligatory() {
    let opts = &FomaOptions::default();
    let r = fsm_simple_replace(opts, re("a"), re("b"));
    assert_eq!(down(&r, "a"), ws(&["b"]), "a is obligatorily rewritten");
    assert_eq!(down(&r, "aba"), ws(&["bbb"]));
    assert_eq!(down(&r, "b"), ws(&["b"]));
}

// [spec:foma:sem:constructions.fsm-priority-union-upper-fn/test]
// [spec:foma:sem:fomalib.fsm-priority-union-upper-fn/test]
#[test]
fn fsm_priority_union_upper_prefers_a_on_shared_inputs() {
    let opts = &FomaOptions::default();
    // Disjoint upper strings: both contribute.
    let r = fsm_priority_union_upper(opts, re("a:b"), re("c:d"));
    assert_eq!(down(&r, "a"), ws(&["b"]));
    assert_eq!(down(&r, "c"), ws(&["d"]));
    // Shared upper "a": A wins, B's a:c is filtered out.
    let r2 = fsm_priority_union_upper(opts, re("a:b"), re("a:c"));
    assert_eq!(down(&r2, "a"), ws(&["b"]));
}

// [spec:foma:sem:constructions.fsm-priority-union-lower-fn/test]
// [spec:foma:sem:fomalib.fsm-priority-union-lower-fn/test]
#[test]
fn fsm_priority_union_lower_filters_b_by_shared_lower() {
    let opts = &FomaOptions::default();
    // B's lower "c" not in A.l={b}: both survive.
    let r = fsm_priority_union_lower(opts, re("a:b"), re("a:c"));
    assert_eq!(down(&r, "a"), ws(&["b", "c"]));
    // B's lower "b" is in A.l: B filtered out.
    let r2 = fsm_priority_union_lower(opts, re("a:b"), re("a:b"));
    assert_eq!(down(&r2, "a"), ws(&["b"]));
}

// [spec:foma:sem:constructions.fsm-lenient-compose-fn/test]
// [spec:foma:sem:fomalib.fsm-lenient-compose-fn/test]
#[test]
fn fsm_lenient_compose_falls_back_to_a_not_b() {
    let opts = &FomaOptions::default();
    // [A .o. B] .P. A (the documented bug: fallback is A, per the code,
    // NOT B per the comment). A = a:b|x:y, B = b:c. A.o.B = a:c.
    // Input "x": composition undefined; fallback A gives x:y -> "y"
    // (the .P. B version would leave "x" undefined).
    let r = fsm_lenient_compose(opts, re("a:b | x:y"), re("b:c"));
    assert_eq!(down(&r, "a"), ws(&["c"]));
    assert_eq!(down(&r, "x"), ws(&["y"]), "fallback is A, pinning the bug");
}

/* ---- term negation ------------------------------------------------ */

// [spec:foma:sem:constructions.fsm-term-negation-fn/test]
// [spec:foma:sem:fomalib.fsm-term-negation-fn/test]
#[test]
fn fsm_term_negation_is_single_symbols_not_in_a() {
    let opts = &FomaOptions::default();
    let n = fsm_term_negation(opts, re("a"));
    assert_eq!(down(&n, "b"), ws(&["b"]));
    assert_eq!(down(&n, "z"), ws(&["z"]), "any single symbol except a");
    assert!(down(&n, "a").is_empty());
    assert!(down(&n, "").is_empty(), "length-1 only");
    assert!(down(&n, "bb").is_empty());
}

/* ---- quotients ---------------------------------------------------- */

// [spec:foma:sem:constructions.fsm-quotient-interleave-fn/test]
// [spec:foma:sem:fomalib.fsm-quotient-interleave-fn/test]
#[test]
fn fsm_quotient_interleave_removes_b_from_a() {
    let opts = &FomaOptions::default();
    // [ab] /\/ [b]: strings interleavable with "b" to yield "ab" == {a}.
    assert_eq!(
        words(&fsm_quotient_interleave(opts, re("a b"), re("b"))),
        ws(&["a"])
    );
}

// [spec:foma:sem:constructions.fsm-quotient-left-fn/test]
// [spec:foma:sem:fomalib.fsm-quotient-left-fn/test]
#[test]
fn fsm_quotient_left_yields_appendable_suffixes() {
    let opts = &FomaOptions::default();
    // [ab] \\\ [abc]: suffixes appendable to A to reach B == {c}.
    assert_eq!(
        words(&fsm_quotient_left(opts, re("a b"), re("a b c"))),
        ws(&["c"])
    );
}

// [spec:foma:sem:constructions.fsm-quotient-right-fn/test]
// [spec:foma:sem:fomalib.fsm-quotient-right-fn/test]
#[test]
fn fsm_quotient_right_yields_extendable_prefixes() {
    let opts = &FomaOptions::default();
    // [abc] /// [c]: prefixes extendable by B to reach A == {ab}.
    assert_eq!(
        words(&fsm_quotient_right(opts, re("a b c"), re("c"))),
        ws(&["ab"])
    );
}

/* ---- ignore ------------------------------------------------------- */

// [spec:foma:sem:constructions.fsm-ignore-fn/test]
// [spec:foma:sem:fomalib.fsm-ignore-fn/test]
#[test]
fn fsm_ignore_all_intersperses_freely() {
    let opts = &FomaOptions::default();
    let g = fsm_ignore(opts, re("a b"), re("x"), OP_IGNORE_ALL);
    assert_eq!(down(&g, "ab"), ws(&["ab"]));
    assert_eq!(down(&g, "axb"), ws(&["axb"]));
    assert_eq!(
        down(&g, "xab"),
        ws(&["xab"]),
        "insertion at the edges allowed"
    );
    assert_eq!(down(&g, "xaxbx"), ws(&["xaxbx"]));
    assert!(down(&g, "acb").is_empty());
}

// [spec:foma:sem:constructions.fsm-ignore-fn/test]
// [spec:foma:sem:fomalib.fsm-ignore-fn/test]
#[test]
fn fsm_ignore_internal_only_intersperses_inside() {
    let opts = &FomaOptions::default();
    let g = fsm_ignore(opts, re("a b"), re("x"), OP_IGNORE_INTERNAL);
    assert_eq!(down(&g, "ab"), ws(&["ab"]));
    assert_eq!(down(&g, "axb"), ws(&["axb"]));
    assert_eq!(down(&g, "axxb"), ws(&["axxb"]));
    assert!(down(&g, "xab").is_empty(), "no insertion at the start");
    assert!(down(&g, "abx").is_empty(), "no insertion at the end");
}

// [spec:foma:sem:constructions.fsm-ignore-fn/test]
// [spec:foma:sem:fomalib.fsm-ignore-fn/test]
#[test]
fn fsm_ignore_empty_second_returns_first_unchanged() {
    let opts = &FomaOptions::default();
    assert_eq!(
        words(&fsm_ignore(opts, re("a"), fsm_empty_set(), OP_IGNORE_ALL)),
        ws(&["a"])
    );
}

/* ---- compact ------------------------------------------------------ */

// [spec:foma:sem:constructions.fsm-compact-fn/test]
// [spec:foma:sem:fomalib.fsm-compact-fn/test]
#[test]
fn fsm_compact_removes_completely_unused_symbol() {
    // No IDENTITY arcs: only wholly-unused sigma symbols are removed.
    let mut net = re("a b");
    sigma_add("z", &mut net.sigma);
    sigma_sort(&mut net);
    assert!(syms(&net).contains(&"z".to_string()));
    fsm_compact(&mut net);
    assert!(!syms(&net).contains(&"z".to_string()), "unused z dropped");
    assert_eq!(words(&net), ws(&["ab"]), "language preserved");
}

// [spec:foma:sem:constructions.fsm-compact-fn/test]
// [spec:foma:sem:fomalib.fsm-compact-fn/test]
#[test]
fn fsm_compact_removes_symbol_subsumed_by_identity() {
    // [?|a] keeps an explicit a:a arc parallel to @:@ going to the same
    // target; compact detects a's distribution == IDENTITY's and drops it.
    let mut net = re("? | a");
    assert!(syms(&net).contains(&"a".to_string()));
    fsm_compact(&mut net);
    assert!(!syms(&net).contains(&"a".to_string()));
    assert!(sigma_nums(&net).contains(&IDENTITY), "IDENTITY survives");
    assert_eq!(down(&net, "a"), ws(&["a"]), "a still matched via @");
    assert_eq!(down(&net, "z"), ws(&["z"]));
}

/* ---- symbol occurrence -------------------------------------------- */

// [spec:foma:sem:constructions.fsm-symbol-occurs-fn/test]
// [spec:foma:sem:fomalib.fsm-symbol-occurs-fn/test]
#[test]
fn fsm_symbol_occurs_checks_requested_sides() {
    let net = re("a:b");
    assert!(fsm_symbol_occurs(&net, "a", M_UPPER));
    assert!(!fsm_symbol_occurs(&net, "a", M_LOWER));
    assert!(fsm_symbol_occurs(&net, "b", M_LOWER));
    assert!(!fsm_symbol_occurs(&net, "b", M_UPPER));
    assert!(fsm_symbol_occurs(&net, "a", M_UPPER + M_LOWER));
    // Not in sigma -> false; unknown side value -> false.
    assert!(!fsm_symbol_occurs(&net, "z", M_UPPER));
    assert!(!fsm_symbol_occurs(&net, "a", 0));
}

/* ---- equal substrings --------------------------------------------- */

// [spec:foma:sem:constructions.fsm-equal-substrings-fn/test]
// [spec:foma:sem:fomalib.fsm-equal-substrings-fn/test]
#[test]
fn fsm_equal_substrings_keeps_only_consistent_delimited_x() {
    let opts = &FomaOptions::default();
    // _eq({larlar | larlbr}, l, r): "larlar" has X=a in both l_r slots
    // (kept); "larlbr" has X=a then X=b (dropped).
    let net = re("l a r l a r | l a r l b r");
    let mut left = re("l");
    let mut right = re("r");
    let res = fsm_equal_substrings(opts, net, &mut left, &mut right);
    assert_eq!(words(&res), ws(&["larlar"]));
    // left/right are only copied, never consumed.
    assert_eq!(words(&left), ws(&["l"]));
    assert_eq!(words(&right), ws(&["r"]));
}

/* ---- invert ------------------------------------------------------- */

// [spec:foma:sem:constructions.fsm-invert-fn/test]
// [spec:foma:sem:fomalib.fsm-invert-fn/test]
#[test]
fn fsm_invert_swaps_sides_and_sort_flags() {
    let n = re("a:b");
    let (si, so) = (n.arcs_sorted_in, n.arcs_sorted_out);
    let inv = fsm_invert(n);
    assert_eq!(inv.arcs_sorted_in, so);
    assert_eq!(inv.arcs_sorted_out, si);
    assert_eq!(down(&inv, "b"), ws(&["a"]));
    assert_eq!(up(&inv, "a"), ws(&["b"]));
}

/* ---- unimplemented stubs (pending) -------------------------------- */

// [spec:foma:sem:constructions.fsm-sequentialize-fn/test]
// [spec:foma:sem:fomalib.fsm-sequentialize-fn/test]
#[test]
fn fsm_sequentialize_is_a_noop_returning_input() {
    // Prints "Implementation pending" and returns the input unchanged.
    let before = lines(&re("a b"));
    let out = fsm_sequentialize(re("a b"));
    assert_eq!(lines(&out), before, "input returned unchanged");
    assert_eq!(words(&out), ws(&["ab"]));
}

// [spec:foma:sem:constructions.fsm-bimachine-fn/test]
// [spec:foma:sem:fomalib.fsm-bimachine-fn/test]
#[test]
fn fsm_bimachine_is_a_noop_returning_input() {
    // Prints "implementation pending" and returns the input unchanged.
    let before = lines(&re("a b"));
    let out = fsm_bimachine(re("a b"));
    assert_eq!(lines(&out), before, "input returned unchanged");
    assert_eq!(words(&out), ws(&["ab"]));
}

/* ---- fast left rewrite -------------------------------------------- */

// [spec:foma:sem:constructions.fsm-left-rewr-fn/test]
// [spec:foma:sem:fomalib.fsm-left-rewr-fn/test]
#[test]
fn fsm_left_rewr_with_universal_context_rewrites_everywhere() {
    let opts = &FomaOptions::default();
    // net = ?* (all states final -> left context always matched), so
    // a -> b applies at every position; every other symbol maps to itself.
    let r = fsm_left_rewr(opts, fsm_universal(), re("a:b"));
    assert_eq!(down(&r, "a"), ws(&["b"]));
    assert_eq!(down(&r, "aba"), ws(&["bbb"]));
    assert_eq!(down(&r, "bb"), ws(&["bb"]));
    assert_eq!(down(&r, "z"), ws(&["z"]), "identity for non-source symbols");
}

/* ---- add sink / add loop ------------------------------------------ */

// [spec:foma:sem:constructions.fsm-add-sink-fn/test]
// [spec:foma:sem:fomalib.fsm-add-sink-fn/test]
#[test]
fn fsm_add_sink_non_final_preserves_language() {
    let s = fsm_add_sink(re("a"), 0);
    assert_eq!(s.statecount, 3, "one fresh sink state added");
    assert_eq!(down(&s, "a"), ws(&["a"]));
    assert!(down(&s, "b").is_empty(), "b routes to the non-final sink");
    assert!(down(&s, "aa").is_empty());
}

// [spec:foma:sem:constructions.fsm-add-sink-fn/test]
// [spec:foma:sem:fomalib.fsm-add-sink-fn/test]
#[test]
fn fsm_add_sink_final_accepts_via_sink() {
    let s = fsm_add_sink(re("a"), 1);
    assert_eq!(down(&s, "a"), ws(&["a"]));
    assert_eq!(down(&s, "b"), ws(&["b"]), "b now reaches the final sink");
    assert_eq!(down(&s, "ab"), ws(&["ab"]));
    assert!(down(&s, "").is_empty(), "start state stays non-final");
}

// [spec:foma:sem:constructions.fsm-add-loop-fn/test]
// [spec:foma:sem:fomalib.fsm-add-loop-fn/test]
#[test]
fn fsm_add_loop_at_final_states_only() {
    let marker = fsm_symbol("#");
    let r = fsm_add_loop(re("a"), &marker, 1);
    assert_eq!(down(&r, "a"), ws(&["a"]));
    assert_eq!(down(&r, "a#"), ws(&["a#"]));
    assert_eq!(down(&r, "a##"), ws(&["a##"]));
    assert!(down(&r, "#a").is_empty(), "no loop at the non-final start");
    // marker is borrowed, not consumed.
    assert_eq!(words(&marker), ws(&["#"]));
}

// [spec:foma:sem:constructions.fsm-add-loop-fn/test]
// [spec:foma:sem:fomalib.fsm-add-loop-fn/test]
#[test]
fn fsm_add_loop_at_non_final_states_only() {
    let marker = fsm_symbol("#");
    let r = fsm_add_loop(re("a"), &marker, 0);
    assert_eq!(down(&r, "#a"), ws(&["#a"]));
    assert_eq!(down(&r, "##a"), ws(&["##a"]));
    assert!(down(&r, "a#").is_empty(), "no loop at the final state");
}

/* ---- context restriction ------------------------------------------ */

// [spec:foma:sem:constructions.fsm-context-restrict-fn/test]
// [spec:foma:sem:fomalib.fsm-context-restrict-fn/test]
#[test]
fn fsm_context_restrict_a_between_b_and_c() {
    let opts = &FomaOptions::default();
    // a => b _ c : every "a" must sit between a b and a c.
    let lr = Some(Box::new(Fsmcontexts {
        left: Some(re("b")),
        right: Some(re("c")),
        next: None,
        cpleft: None,
        cpright: None,
    }));
    let r = fsm_context_restrict(opts, re("a"), lr);
    assert_eq!(down(&r, "bac"), ws(&["bac"]));
    assert_eq!(down(&r, "bacbac"), ws(&["bacbac"]));
    assert!(down(&r, "ac").is_empty(), "a with no left b");
    assert!(down(&r, "bacac").is_empty(), "second a lacks a left b");
    assert!(down(&r, "abc").is_empty());
}

/* ---- close sigma -------------------------------------------------- */

// [spec:foma:sem:constructions.fsm-close-sigma-fn/test]
// [spec:foma:sem:fomalib.fsm-close-sigma-fn/test]
#[test]
fn fsm_close_sigma_mode0_drops_identity_arcs() {
    let opts = &FomaOptions::default();
    // [?|a]: mode 0 removes the @ (IDENTITY) arc, leaving only a:a.
    let c = fsm_close_sigma(opts, re("? | a"), 0);
    assert_eq!(words(&c), ws(&["a"]));
    assert!(down(&c, "z").is_empty(), "wildcard path removed");
}

// [spec:foma:sem:constructions.fsm-close-sigma-fn/test]
// [spec:foma:sem:fomalib.fsm-close-sigma-fn/test]
#[test]
fn fsm_close_sigma_mode1_keeps_identity_arcs() {
    let opts = &FomaOptions::default();
    // mode 1 removes only UNKNOWN; IDENTITY survives, so [?|a] is unchanged.
    let c = fsm_close_sigma(opts, re("? | a"), 1);
    assert_eq!(down(&c, "z"), ws(&["z"]));
    assert_eq!(down(&c, "a"), ws(&["a"]));
}

fn net_counts(net: &Fsm) -> (i32, i32, i32, i32) {
    (net.statecount, net.linecount, net.arccount, net.finalcount)
}
