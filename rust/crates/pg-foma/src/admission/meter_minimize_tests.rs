//! TEST 18: what `Meter::minimize` stops, and where; synthetic nets only.

use super::meter::Meter;
use foma::constructions::fsm_count;
use foma::determinize::fsm_determinize;
use foma::dynarray::{
    fsm_construct_add_arc, fsm_construct_done, fsm_construct_init, fsm_construct_set_final,
    fsm_construct_set_initial,
};
use foma::options::FomaOptions;
use foma::types::{Fsm, Tern};

/// The size above which `subset::minimize` hands a net to foma.
const SMALL: i32 = 2_000;

/// An eight-letter word over `c`, `d`, `e`, picked by `number`; padding that keeps the net past the size that goes to foma.
fn padding_word(number: usize) -> String {
    let letters = ['c', 'd', 'e'];
    (0..8)
        .map(|place| letters[(number / 3usize.pow(place % 6) + place as usize) % 3])
        .collect()
}

/// Which way the core net is hard to determinize.
#[derive(Clone, Copy, PartialEq)]
enum Core {
    /// Label `a` is `back` symbols from the end: the net determinizes to `2^back` states, its reverse to `back + 1`.
    FromEnd,
    /// As `FromEnd`, but every state accepts: the language is every text, its minimal net has one state, and the subsets still number `2^back`.
    AllFinal,
    /// Label `a` is `back` symbols from the start or from the end: neither direction is small.
    BothEnds,
}

/// A non-deterministic net over `a b` that determinizes large, with `padded` words over other letters on a start of their own when it must pass the size that goes to foma.
fn net(core: Core, back: usize, padded: bool) -> Fsm {
    let mut handle = fsm_construct_init("test18");
    let mut arc = |from: usize, label: &str, to: usize| {
        fsm_construct_add_arc(&mut handle, from as i32, to as i32, label, label)
    };
    arc(0, "a", 0);
    arc(0, "b", 0);
    arc(0, "a", 1);
    for state in 1..back {
        arc(state, "a", state + 1);
        arc(state, "b", state + 1);
    }
    let mut last = back;
    let mut finals = vec![back];
    if core == Core::AllFinal {
        finals = (0..=back).collect();
    }
    let mut starts = vec![0];
    if core == Core::BothEnds {
        let first = back + 1;
        starts.push(first);
        for state in first..first + back - 1 {
            arc(state, "a", state + 1);
            arc(state, "b", state + 1);
        }
        last = first + back;
        arc(last - 1, "a", last);
        arc(last, "a", last);
        arc(last, "b", last);
        finals.push(last);
    }
    if padded {
        let start = last + 1;
        starts.push(start);
        last = start;
        for number in 0..300 {
            let mut at = start;
            for letter in padding_word(number).chars() {
                last += 1;
                arc(at, &letter.to_string(), last);
                at = last;
            }
            finals.push(at);
        }
    }
    for start in starts {
        fsm_construct_set_initial(&mut handle, start as i32);
    }
    for end in finals {
        fsm_construct_set_final(&mut handle, end as i32);
    }
    let mut net = fsm_construct_done(handle);
    fsm_count(&mut net);
    assert_ne!(net.is_deterministic, Tern::Yes);
    net.is_deterministic = Tern::No;
    net.is_minimized = Tern::No;
    net
}

fn weight(net: &Fsm) -> u64 {
    (net.statecount + net.arccount) as u64
}

/// States plus arcs of the net foma's determinizing makes of `net`.
fn determinized_weight(net: &Fsm) -> u64 {
    let mut determinized = fsm_determinize(net.clone());
    fsm_count(&mut determinized);
    weight(&determinized)
}

fn minimal() -> FomaOptions {
    FomaOptions {
        minimal: true,
        ..FomaOptions::default()
    }
}

fn unminimal() -> FomaOptions {
    FomaOptions {
        minimal: false,
        ..FomaOptions::default()
    }
}

fn is_the_empty_net(net: &Fsm) -> bool {
    net.statecount == 1 && net.arccount == 0
}

#[test]
fn the_fixtures_are_large_nets_whose_determinized_net_is_far_larger() {
    let big = net(Core::BothEnds, 12, true);
    assert!(big.statecount + big.arccount > SMALL);
    assert!(determinized_weight(&big) > 5 * weight(&big));
    let small = net(Core::FromEnd, 14, false);
    assert!(small.statecount + small.arccount <= SMALL);
    assert!(determinized_weight(&small) > 20 * weight(&small));
}

#[test]
fn a_large_net_over_budget_is_refused_by_the_subset_path_before_its_determinization_is_counted() {
    let big = net(Core::BothEnds, 12, true);
    let budget = 4 * weight(&big);
    let meter = Meter::within(budget);
    let result = meter.minimize(&minimal(), big.clone());
    assert!(is_the_empty_net(&result));
    assert!(meter.refused());
    assert!(
        meter.units() <= budget,
        "the subset path never spends past the budget: {} of {budget}",
        meter.units()
    );
    assert!(
        meter.check(0).is_err(),
        "the wrapper's check turns the refusal into ConstructionBudget"
    );
    // Once refused, every later operation returns the empty net untouched.
    let after = meter.minimize(&minimal(), big);
    assert!(is_the_empty_net(&after));
    assert!(meter.units() <= budget);
}

#[test]
fn the_same_net_without_minimal_is_determinized_whole_before_the_meter_looks() {
    let big = net(Core::BothEnds, 12, true);
    let determinized = determinized_weight(&big);
    let budget = 4 * weight(&big);
    assert!(determinized > budget);
    let meter = Meter::within(budget);
    let result = meter.minimize(&unminimal(), big);
    assert!(is_the_empty_net(&result));
    assert!(meter.refused());
    assert!(
        meter.units() >= determinized,
        "foma built the whole determinized net ({determinized}) before it was charged: {}",
        meter.units()
    );
}

#[test]
fn a_net_whose_reverse_is_small_is_minimized_within_a_budget_below_its_determinized_size() {
    let big = net(Core::AllFinal, 15, true);
    let determinized = determinized_weight(&big);
    let budget = 8 * weight(&big);
    assert!(determinized > budget, "{determinized} against {budget}");
    let meter = Meter::within(budget);
    let result = meter.minimize(&minimal(), big.clone());
    assert!(!meter.refused());
    assert!(!is_the_empty_net(&result));
    assert!(meter.units() <= budget);
    assert!(
        meter.units() < determinized,
        "the determinized net is never made: {} of {determinized}",
        meter.units()
    );
    assert_eq!(result.is_deterministic, Tern::Yes);
    assert!(
        weight(&result) < 400,
        "the padding words and the one-state core"
    );
    let opts = FomaOptions::default();
    // foma's own minimal net is the reference; equivalence is checked against it, not against the raw nondeterministic fixture.
    let theirs = foma::minimize::fsm_minimize(&opts, big.clone());
    assert_eq!(
        (result.statecount, result.arccount),
        (theirs.statecount, theirs.arccount)
    );
    assert!(foma::constructions::fsm_equivalent(&opts, result, theirs));
    // A meter built without the minimal option refuses the same budget.
    let plain = Meter::within(budget);
    assert!(is_the_empty_net(&plain.minimize(&unminimal(), big)));
    assert!(plain.refused());
}

#[test]
fn the_forward_construction_is_checked_in_turns_not_per_state() {
    // CONTRADICTS PLAN: TEST 18 (granularity): the plan says "refuses per created subset state".
    let big = net(Core::BothEnds, 14, true);
    let determinized = determinized_weight(&big);
    // Past the head start and the reverse try, well inside what the forward construction needs.
    let mut gaps = Vec::new();
    for share in [3, 5, 8] {
        let budget = determinized / share;
        let meter = Meter::within(budget);
        let result = meter.minimize(&minimal(), big.clone());
        assert!(
            is_the_empty_net(&result),
            "budget {budget} of {determinized}"
        );
        assert!(meter.refused());
        assert!(meter.units() <= budget);
        gaps.push(budget - meter.units());
    }
    // One state's row is at most three units here; a refusal per state would stop within that of the budget.
    assert!(
        gaps.iter().any(|gap| *gap > 1_000),
        "refused only at a turn boundary, leaving unspent budget: {gaps:?}"
    );
}

#[test]
fn a_small_net_is_determinized_whole_and_unmetered_even_with_minimal() {
    // CONTRADICTS PLAN: TEST 18: the plan says minimize determinises through the metered subset tables when `opts.minimal` is set.
    let small = net(Core::FromEnd, 14, false);
    let determinized = determinized_weight(&small);
    assert!(small.statecount + small.arccount <= SMALL);
    let budget = 1_000;
    let meter = Meter::within(budget);
    let result = meter.minimize(&minimal(), small);
    assert!(is_the_empty_net(&result));
    assert!(meter.refused());
    assert!(
        meter.units() >= determinized,
        "the whole determinized net ({determinized}) was built, then charged: {}",
        meter.units()
    );
    assert!(meter.units() > 10 * budget);
}

#[test]
fn a_deterministic_net_goes_straight_to_foma_and_is_charged_for_itself_only() {
    let mut dfa = fsm_determinize(net(Core::FromEnd, 6, false));
    fsm_count(&mut dfa);
    assert_eq!(dfa.is_deterministic, Tern::Yes);
    let before = weight(&dfa);
    let meter = Meter::default();
    let result = meter.minimize(&minimal(), dfa);
    assert!(!meter.refused());
    assert!(meter.units() >= before);
    assert!(meter.units() <= before + weight(&result) + 1);
}
