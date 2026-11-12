use super::nets::{Dfa, Nfa};
use super::*;
use foma::constructions::{fsm_equivalent, fsm_kleene_plus};
use foma::determinize::fsm_determinize;
use foma::dynarray::{
    fsm_construct_add_arc, fsm_construct_done, fsm_construct_init, fsm_construct_set_final,
    fsm_construct_set_initial,
};
use foma::minimize::fsm_minimize;
use foma::regex::fsm_parse_regex;
use foma::structures::fsm_empty_set;
use foma::types::Tern;

fn regex(text: &str) -> Fsm {
    fsm_parse_regex(&FomaOptions::default(), text, None, None).expect("the regex compiles")
}

/// `count` words of `length` letters over five, picked by `seed`.
fn words(seed: u64, count: usize, length: u32) -> Vec<String> {
    let letters = ['a', 'b', 'c', 'd', 'e'];
    let mut picked = seed;
    let word = |_| {
        picked = picked
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (0..length)
            .map(|place| letters[(picked >> (16 + 5 * place)) as usize % 5])
            .collect()
    };
    (0..count).map(word).collect()
}

/// How [`chains`] writes its words.
#[derive(Clone, Copy, Default)]
struct Written {
    /// Every `a` is read as `a` and written as `b`.
    crossed: bool,
    /// Every third letter may be any symbol the net does not name.
    open: bool,
    /// A word may follow a word.
    again: bool,
}

/// A non-deterministic net past the size that goes to foma: every word on its own path from the start.
fn chains(words: &[String], written: Written) -> Fsm {
    let mut handle = fsm_construct_init("chains");
    fsm_construct_set_initial(&mut handle, 0);
    let mut last = 0;
    for word in words {
        let mut at = 0;
        for (place, letter) in word.chars().enumerate() {
            last += 1;
            let upper = letter.to_string();
            let lower = if written.crossed && letter == 'a' {
                "b".to_owned()
            } else {
                upper.clone()
            };
            fsm_construct_add_arc(&mut handle, at, last, &upper, &lower);
            if written.open && place == 2 {
                fsm_construct_add_arc(
                    &mut handle,
                    at,
                    last,
                    "@_IDENTITY_SYMBOL_@",
                    "@_IDENTITY_SYMBOL_@",
                );
            }
            at = last;
        }
        fsm_construct_set_final(&mut handle, at);
        if written.again {
            fsm_construct_add_arc(
                &mut handle,
                at,
                0,
                "@_EPSILON_SYMBOL_@",
                "@_EPSILON_SYMBOL_@",
            );
        }
    }
    let mut net = fsm_construct_done(handle);
    fsm_count(&mut net);
    assert!(net.statecount + net.arccount > SMALL as i32);
    assert_ne!(net.is_deterministic, Tern::Yes);
    net
}

fn loose() -> [Fsm; 4] {
    let words = words(7, 500, 6);
    let plain = Written::default();
    [
        chains(&words, plain),
        chains(
            &words,
            Written {
                again: true,
                ..plain
            },
        ),
        chains(
            &words,
            Written {
                crossed: true,
                again: true,
                ..plain
            },
        ),
        chains(
            &words,
            Written {
                open: true,
                ..plain
            },
        ),
    ]
}

/// The minimal net of some words, large enough that joining two does not go to foma.
fn list(seed: u64, written: Written) -> Fsm {
    let mut net = fsm_minimize(
        &FomaOptions::default(),
        chains(&words(seed, 600, 8), written),
    );
    fsm_count(&mut net);
    assert!(net.statecount + net.arccount > SMALL as i32);
    net
}

fn shape(net: &Fsm) -> impl PartialEq + std::fmt::Debug {
    let sigma: Vec<(i32, String)> = net
        .sigma
        .iter()
        .map(|s| (s.number, s.symbol.to_string()))
        .collect();
    let flags = (
        net.is_deterministic,
        net.is_pruned,
        net.is_minimized,
        net.is_epsilon_free,
    );
    // Not the count of accepting states: foma's minimizing leaves the count of the net it read.
    let counts = (net.statecount, net.arccount, net.arity);
    (counts, flags, sigma)
}

fn assert_fomas_minimal(ours: &Fsm, net: &Fsm) {
    let opts = FomaOptions::default();
    let theirs = fsm_minimize(&opts, net.clone());
    assert_eq!(shape(ours), shape(&theirs));
    assert!(fsm_equivalent(&opts, ours.clone(), theirs));
}

fn determinized(nfa: &Nfa) -> Dfa {
    let mut subsets = Subsets::new(nfa);
    assert!(subsets.run(u64::MAX));
    subsets.finish()
}

fn table(dfa: &Dfa) -> Vec<(bool, Vec<(u32, u32)>)> {
    (0..dfa.accepting.len())
        .map(|state| (dfa.accepting[state], dfa.rows.row(state).collect()))
        .collect()
}

/// The net of the texts over two labels whose `back`th symbol from the end is label 0.
fn nth_from_the_end(back: u32) -> Nfa {
    let mut arcs = vec![(0, 0, 0), (0, 1, 0), (0, 0, 1)];
    for state in 1..back {
        arcs.push((state, 0, state + 1));
        arcs.push((state, 1, state + 1));
    }
    let mut accepting = vec![false; back as usize + 1];
    accepting[back as usize] = true;
    Nfa::new(accepting, vec![0], 2, &arcs, &[])
}

#[test]
fn refining_the_determinized_net_gives_the_minimal_net_foma_makes() {
    for net in loose() {
        let (nfa, pairs) = Nfa::read(&net);
        let minimal = refine::refine(&determinized(&nfa)).expect("not empty");
        assert_fomas_minimal(&minimal.write(net.clone(), &pairs), &net);
    }
}

#[test]
fn determinizing_the_reverse_and_its_reverse_gives_the_same_minimal_net() {
    for net in loose() {
        let (nfa, pairs) = Nfa::read(&net);
        let forward = refine::refine(&determinized(&nfa)).expect("not empty");
        let back = determinized(&determinized(&nfa.reversed()).reversed());
        assert_eq!(table(&back), table(&forward));
        assert_fomas_minimal(&back.write(net.clone(), &pairs), &net);
    }
}

#[test]
fn a_net_that_outgrows_its_head_start_tries_its_reverse_and_ends_there_when_that_is_small() {
    let nfa = nth_from_the_end(12);
    let meter = Meter::default();
    let ended = construct(&meter, &nfa);
    let Ended::Reverse(reverse) = ended else {
        panic!("the reverse determinizes in 13 states and the net in 4,096");
    };
    assert_eq!(reverse.accepting.len(), 13);
    let made = HEAD_START * nfa.size() + nfa.size() + reverse.size();
    assert!(
        (made..made + 8).contains(&meter.units()),
        "the head start, the reversed net and the reverse are charged"
    );
    let minimal = minimal(&Meter::default(), nth_from_the_end(12))
        .expect("not refused")
        .expect("not empty");
    let forward = refine::refine(&determinized(&nfa)).expect("not empty");
    assert_eq!(minimal.accepting.len(), 4096);
    assert_eq!(table(&minimal), table(&forward));
}

/// A net neither direction of which determinizes small: label 0 is `back` symbols from the start or from the end.
fn both_ends(back: u32) -> Nfa {
    let mut arcs = vec![(0, 0, 0), (0, 1, 0), (0, 0, 1)];
    for state in 1..back {
        arcs.extend([(state, 0, state + 1), (state, 1, state + 1)]);
    }
    let first = back + 1;
    for state in first..first + back - 1 {
        arcs.extend([(state, 0, state + 1), (state, 1, state + 1)]);
    }
    let last = first + back;
    arcs.extend([(last - 1, 0, last), (last, 0, last), (last, 1, last)]);
    let mut accepting = vec![false; last as usize + 1];
    accepting[back as usize] = true;
    accepting[last as usize] = true;
    Nfa::new(accepting, vec![0, first], 2, &arcs, &[])
}

#[test]
fn a_reverse_that_does_not_end_within_its_bound_is_dropped_and_still_charged() {
    let nfa = both_ends(12);
    let meter = Meter::default();
    let Ended::Forward(forward) = construct(&meter, &nfa) else {
        panic!("neither direction ends within sixteen times the net");
    };
    let tried = meter.units() - forward.size();
    assert!(
        tried > nfa.size() && tried <= nfa.size() * (1 + REVERSE_TRY) + 8,
        "the reversed net and what its try made are charged on top of the determinized net: {tried}"
    );
    assert_eq!(table(&forward), table(&determinized(&nfa)));
}

#[test]
fn a_language_with_no_text_has_no_minimal_net() {
    let unreachable = Nfa::new(
        vec![false, false, true],
        vec![0],
        1,
        &[(0, 0, 1), (2, 0, 2)],
        &[],
    );
    assert!(matches!(minimal(&Meter::default(), unreachable), Ok(None)));
}

#[test]
fn minimizing_charges_what_foma_determinizing_would_and_returns_the_same_net() {
    let [_, net, _, _] = loose();
    let mut determinized = fsm_determinize(net.clone());
    fsm_count(&mut determinized);
    let size = (determinized.statecount + determinized.arccount) as u64;
    let meter = Meter::default();
    let ours = minimize(&meter, net.clone(), |_| {
        panic!("a large net does not go to foma")
    });
    assert_eq!(meter.units(), size);
    assert_fomas_minimal(&ours, &net);
}

#[test]
fn small_nets_and_nets_that_accept_nothing_go_to_foma() {
    let opts = FomaOptions::default();
    let mut small = fsm_kleene_plus(&opts, regex("a b | a c"));
    fsm_count(&mut small);
    let meter = Meter::default();
    let ours = minimize(&meter, small.clone(), |net| fsm_minimize(&opts, net));
    assert_fomas_minimal(&ours, &small);
    assert_eq!(
        meter.units(),
        0,
        "foma's minimizing is charged by the caller"
    );
    let [mut never, _, _, _] = loose();
    never.finalcount = 0;
    let marker = minimize(&meter, never, |_| fsm_empty_set());
    assert_eq!(marker.statecount, 1);
}

#[test]
fn a_refused_minimization_returns_the_empty_net_and_stops_within_a_turn() {
    let [_, net, _, _] = loose();
    let size = (net.statecount + net.arccount) as u64;
    let meter = Meter::within(size / 2);
    let refused = minimize(&meter, net, |_| panic!("a large net does not go to foma"));
    assert_eq!((refused.statecount, refused.arccount), (1, 0));
    assert!(meter.refused());
    assert_eq!(
        meter.units(),
        0,
        "a construction that did not fit is not counted"
    );
}

#[test]
fn joining_two_nets_gives_the_net_foma_concatenation_makes() {
    let opts = FomaOptions::default();
    let plain = Written::default();
    let open = Written {
        open: true,
        ..plain
    };
    let crossed = Written {
        crossed: true,
        ..plain
    };
    let pairs: [(fn(Written) -> Fsm, Written, fn(Written) -> Fsm, Written); 4] = [
        (|w| list(1, w), plain, |w| list(2, w), plain),
        (|w| list(1, w), open, |_| regex("?* z"), plain),
        (|_| regex("[? - q]* r"), plain, |w| list(2, w), crossed),
        (|w| list(3, w), crossed, |w| list(4, w), open),
    ];
    for (left, first, right, second) in pairs {
        let theirs = fsm_concat(&opts, left(first), right(second));
        let ours = concat(&Meter::default(), &opts, left(first), right(second));
        assert_eq!(shape(&ours), shape(&theirs));
        assert!(fsm_equivalent(&opts, ours, theirs));
    }
}

#[test]
fn joining_small_or_empty_nets_is_left_to_foma() {
    let opts = FomaOptions::default();
    let small = concat(&Meter::default(), &opts, regex("a b"), regex("c | d"));
    assert_eq!(
        shape(&small),
        shape(&fsm_concat(&opts, regex("a b"), regex("c | d")))
    );
    let none = concat(
        &Meter::default(),
        &opts,
        list(1, Written::default()),
        fsm_empty_set(),
    );
    assert_eq!((none.statecount, none.arccount, none.finalcount), (1, 0, 0));
}

#[test]
fn a_net_with_more_labels_than_two_bytes_number_keeps_them_all() {
    let labels = 70_000u32;
    let arcs: Vec<(u32, u32, u32)> = (0..labels)
        .flat_map(|label| [(0, label, 1), (0, label, 2)])
        .collect();
    let nfa = Nfa::new(
        vec![false, true, true],
        vec![0],
        labels as usize,
        &arcs,
        &[],
    );
    let minimal = minimal(&Meter::default(), nfa)
        .expect("not refused")
        .expect("not empty");
    assert_eq!(minimal.accepting, [false, true]);
    let row: Vec<(u32, u32)> = minimal.rows.row(0).collect();
    assert_eq!(row, (0..labels).map(|label| (label, 1)).collect::<Vec<_>>());
}

#[test]
fn joining_charges_its_subset_construction_and_stops_past_the_budget() {
    let opts = FomaOptions::default();
    let plain = Written::default();
    let meter = Meter::default();
    let joined = concat(&meter, &opts, list(1, plain), list(2, plain));
    assert!(meter.units() >= (joined.statecount + joined.arccount) as u64);
    let tight = Meter::within(meter.units() / 2);
    let refused = concat(&tight, &opts, list(1, plain), list(2, plain));
    assert_eq!((refused.statecount, refused.arccount), (1, 0));
    assert!(tight.refused());
}
