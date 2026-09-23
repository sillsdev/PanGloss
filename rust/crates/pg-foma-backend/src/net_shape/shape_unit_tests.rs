//! Hand-built nets, compiled from foma regexes, so each assertion pins ONE property of the walk
//! with no grammar, no lexc, and no compile pipeline in the way. A screen whose graph walk is
//! only ever exercised through a full grammar compile cannot be debugged when it disagrees.

use super::*;
use foma::options::FomaOptions;

fn net(regex: &str) -> Fsm {
    let opts = FomaOptions::default();
    foma::regex::fsm_parse_regex(&opts, regex, None, None)
        .unwrap_or_else(|| panic!("regex failed to compile: {regex}"))
}

/// A plain Kleene star over a real character is a self-loop but NOT a defect: every lap consumes a character, so lap count is bounded by query length.
#[test]
fn a_real_character_loop_is_a_cycle_but_not_a_zero_width_one() {
    let shape = NetShape::inspect(&net("[a]*"), ApplyDirection::Up);
    eprintln!("a_real_character_loop: {}", shape.evidence_line());
    assert!(
        shape.cycles.any(),
        "`[a]*` must be recognized as containing a cycle: {}",
        shape.evidence_line()
    );
    assert!(
        shape.cycles.self_loop_states > 0,
        "`[a]*` must be recognized as containing a self-loop: {}",
        shape.evidence_line()
    );
    assert_eq!(
        shape.zero_width_cycles,
        CycleCensus::default(),
        "a loop consuming a real character is not zero-width: {}",
        shape.evidence_line()
    );
    assert_eq!(shape.zero_width_arcs, 0, "{}", shape.evidence_line());
    assert_eq!(shape.verdict(), ShapeVerdict::ZeroWidthBounded);
}

/// `a:0` pairs an upper `a` with a lower epsilon, so starred it self-loops while consuming nothing -- the null-morph pathology, stripped to its minimal shape.
#[test]
fn an_epsilon_consuming_loop_is_flagged_zero_width() {
    let shape = NetShape::inspect(&net("[a:0]*"), ApplyDirection::Up);
    eprintln!("an_epsilon_consuming_loop: {}", shape.evidence_line());
    assert!(
        shape.zero_width_cycles.any(),
        "`[a:0]*` consumes nothing per lap on the UP tape and must be flagged: {}",
        shape.evidence_line()
    );
    assert!(shape.zero_width_arcs > 0, "{}", shape.evidence_line());
    assert!(
        shape.verdict().is_pathological(),
        "verdict must name the defect: {:?}",
        shape.verdict()
    );
}

/// `a:0` is unbounded going up and bounded going down on the SAME net, so reading the wrong tape would silently report it clean.
#[test]
fn direction_decides_whether_a_zero_width_cycle_exists() {
    let n = net("[a:0]*");
    let up = NetShape::inspect(&n, ApplyDirection::Up);
    let down = NetShape::inspect(&n, ApplyDirection::Down);
    eprintln!("up:   {}", up.evidence_line());
    eprintln!("down: {}", down.evidence_line());
    assert!(
        up.zero_width_cycles.any(),
        "UP consumes the lower tape, which is epsilon here: {}",
        up.evidence_line()
    );
    assert!(
        !down.zero_width_cycles.any(),
        "DOWN consumes the upper tape, which is `a` here -- bounded: {}",
        down.evidence_line()
    );
    assert_eq!(
        up.cycles, down.cycles,
        "the FULL continuation graph is direction-independent, so a difference here means the \
         direction filter leaked into the unfiltered walk"
    );
}

/// A zero-width cycle need not be a self-loop: `[[a:0] [b:0]]*` is a two-state zero-width cycle with no self-loop on either state.
#[test]
fn a_multi_state_zero_width_cycle_is_found_without_any_self_loop() {
    let shape = NetShape::inspect(&net("[[a:0] [b:0]]*"), ApplyDirection::Up);
    eprintln!("multi_state_zero_width: {}", shape.evidence_line());
    assert!(
        shape.zero_width_cycles.any(),
        "a two-arc zero-width loop must be found: {}",
        shape.evidence_line()
    );
    assert!(
        shape.zero_width_cycles.largest_cycle_states >= 2,
        "the cycle spans more than one state, so SCC detection (not self-loop detection) is \
         what found it: {}",
        shape.evidence_line()
    );
}

/// `[a] [b:0] [c]` has a zero-width arc but no zero-width cycle, so `zero_width_arcs > 0` alone must never drive the verdict.
#[test]
fn an_acyclic_epsilon_arc_is_not_a_defect() {
    let shape = NetShape::inspect(&net("[a] [b:0] [c]"), ApplyDirection::Up);
    eprintln!("acyclic_epsilon_arc: {}", shape.evidence_line());
    assert!(
        shape.zero_width_arcs > 0,
        "fixture must actually contain a zero-width arc or this test is vacuous: {}",
        shape.evidence_line()
    );
    assert!(
        !shape.zero_width_cycles.any(),
        "an epsilon arc on a straight path is a jump, not a repeatable loop: {}",
        shape.evidence_line()
    );
    assert_eq!(shape.verdict(), ShapeVerdict::ZeroWidthBounded);
}

/// A net with no cycle reports zeros for every counter -- the negative control that makes a non-zero count elsewhere mean something.
#[test]
fn a_finite_net_has_no_cycles_at_all() {
    let shape = NetShape::inspect(&net("[a] [b] [c]"), ApplyDirection::Up);
    eprintln!("finite_net: {}", shape.evidence_line());
    assert_eq!(
        shape.cycles,
        CycleCensus::default(),
        "{}",
        shape.evidence_line()
    );
    assert_eq!(
        shape.zero_width_cycles,
        CycleCensus::default(),
        "{}",
        shape.evidence_line()
    );
    assert!(shape.states > 0, "a compiled net must have states");
    assert!(shape.sink_states > 0, "`abc` ends somewhere");
}

/// A three-way union must report `branch_max >= 3`, and a deterministic net must report `apply_ambiguity_max == 1`.
#[test]
fn branching_and_ambiguity_are_actually_measured() {
    let wide = NetShape::inspect(&net("[a|b|c]"), ApplyDirection::Up);
    eprintln!("wide: {}", wide.evidence_line());
    assert!(
        wide.branching.max >= 3,
        "three alternatives leave one state: {}",
        wide.evidence_line()
    );
    assert_eq!(
        wide.apply_ambiguity_max,
        1,
        "three DISTINCT labels are not ambiguity -- the traversal picks one by the input symbol: \
         {}",
        wide.evidence_line()
    );
    assert_eq!(wide.apply_ambiguity_total, 0, "{}", wide.evidence_line());

    // Genuinely ambiguous UP: one lower symbol, two different upper symbols.
    let ambiguous = NetShape::inspect(&net("[a:x]|[a:y]"), ApplyDirection::Down);
    eprintln!("ambiguous(down): {}", ambiguous.evidence_line());
    assert!(
        ambiguous.apply_ambiguity_max >= 2,
        "two arcs consuming the same upper `a` must register as a fork: {}",
        ambiguous.evidence_line()
    );
    assert!(
        ambiguous.apply_ambiguity_total >= 1,
        "{}",
        ambiguous.evidence_line()
    );
}

/// This module's full-graph SCC census must agree with `foma::topsort::fsm_topsort` on every net; a disagreement means the walk below is wrong, not foma. Only the coarse question is cross-checkable -- `fsm_topsort` has no notion of a zero-width cycle.
#[test]
fn full_graph_cycle_detection_agrees_with_fomas_own_topsort() {
    // Covers both answers and both mechanisms: straight-line, epsilon jump, self-loop, multi-state loop, and the epsilon-consuming variant of each loop.
    let cases = [
        ("[a] [b] [c]", false),
        ("[a] [b:0] [c]", false),
        ("[a]*", true),
        ("[[a] [b]]*", true),
        ("[a:0]*", true),
        ("[[a:0] [b:0]]*", true),
    ];
    for (regex, expect_cyclic) in cases {
        let shape = NetShape::inspect(&net(regex), ApplyDirection::Up);
        // fsm_topsort consumes its net, so it gets its own freshly compiled copy.
        let sorted = foma::topsort::fsm_topsort(net(regex));
        let foma_says_cyclic = match sorted.is_loop_free {
            foma::types::Tern::Yes => false,
            foma::types::Tern::No => true,
            foma::types::Tern::Unk => panic!(
                "fsm_topsort left is_loop_free unknown for `{regex}` -- the cross-check cannot \
                 be performed, which is NOT the same fact as agreement"
            ),
        };
        eprintln!(
            "topsort cross-check `{regex}`: foma_cyclic={foma_says_cyclic} \
             net_shape_cyclic={} {}",
            shape.cycles.any(),
            shape.evidence_line()
        );
        assert_eq!(
            shape.cycles.any(),
            foma_says_cyclic,
            "this module's full-graph SCC census disagrees with `fsm_topsort` on `{regex}`: {}",
            shape.evidence_line()
        );
        assert_eq!(
            foma_says_cyclic, expect_cyclic,
            "the fixture `{regex}` no longer has the shape this case was chosen for, so the \
             agreement above is about something other than what it claims"
        );
    }
}

/// Pinned on a synthetic sample, not a net, because the ordered/non-interpolating property belongs to the reduction, not to any automaton.
#[test]
fn degree_quantiles_are_ordered_and_nearest_rank() {
    let mut samples = vec![0u64, 1, 1, 2, 3, 5, 8, 13, 21, 34];
    let d = DegreeDistribution::from_samples(&mut samples);
    assert_eq!(d.max, 34);
    assert_eq!(d.sampled_states, 10);
    assert!(d.p50 <= d.p90 && d.p90 <= d.p99 && d.p99 <= d.max, "{d:?}");
    // Nearest-rank: index floor(0.5 * 10) == 5 -> 5; floor(0.9 * 10) == 9 -> 34.
    assert_eq!(d.p50, 5, "{d:?}");
    assert_eq!(d.p90, 34, "{d:?}");
    // Mean of the sample is 8.8 exactly.
    assert_eq!(d.mean_milli, 8_800, "{d:?}");
    assert_eq!(
        DegreeDistribution::from_samples(&mut []),
        DegreeDistribution::default(),
        "an empty sample must reduce to zeros rather than panic"
    );
}
