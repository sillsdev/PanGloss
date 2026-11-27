//! Bounds retained capture configurations without truncating ambiguous matches.

use pg_fst::work::WorkBudget;
use pg_fst::{CompileInput, CompileNode, Segment, Transduce};

const SEGMENTS: usize = 8;
const GROUPS: usize = 6;

fn adjacent_captures() -> pg_fst::Fst {
    let any = || vec![CompileNode::Constraint(vec![u64::MAX])];
    let mut nodes = vec![CompileNode::Group {
        name: "g0".into(),
        children: vec![CompileNode::Quantifier {
            min: 1,
            max: None,
            children: any(),
        }],
    }];
    for i in 1..GROUPS {
        nodes.push(CompileNode::Group {
            name: format!("g{i}"),
            children: vec![CompileNode::Quantifier {
                min: 0,
                max: None,
                children: any(),
            }],
        });
    }
    CompileInput::new(nodes).deterministic(false).compile()
}

fn run(budget: &WorkBudget, fst: &pg_fst::Fst) -> usize {
    run_segments(budget, fst, SEGMENTS)
}

fn run_segments(budget: &WorkBudget, fst: &pg_fst::Fst, segments: usize) -> usize {
    let _scope = budget.enter();
    Transduce::new(fst, vec![Segment::new(vec![1]); segments])
        .anchored(true, true)
        .all_matches()
        .len()
}

/// Splits of 8 segments into one non-empty and five possibly-empty groups: C(8 - 1 + 5, 5).
const SPLITS: usize = 792;

#[test]
fn ambiguous_captures_keep_one_result_per_split_and_return_their_charge() {
    let fst = adjacent_captures();
    let budget = WorkBudget::new(usize::MAX / 2).with_retained_cap(None);
    assert_eq!(run(&budget, &fst), SPLITS);
    assert!(
        budget.retained_peak() > 0,
        "the matcher must account its visited set"
    );
    assert_eq!(
        budget.retained(),
        0,
        "everything charged is returned when the search ends"
    );
    assert!(!budget.capped());
}

#[test]
fn retained_capture_state_stays_within_a_structural_bound() {
    // One visited-set entry per (state, position, registers) configuration; a configuration owns at most one fresh register block. 40 bytes covers the entry, 8 bytes per register slot the block.
    let fst = adjacent_captures();
    let slots = fst.register_count() as usize * 2;
    let configurations = SEGMENTS * (GROUPS + 1) * SPLITS; // generous: positions x group states x splits
    let per_config = 64 + slots * 8;
    let budget = WorkBudget::new(usize::MAX / 2).with_retained_cap(None);
    run(&budget, &fst);
    assert!(
        budget.retained_peak() <= configurations * per_config,
        "peak {} bytes",
        budget.retained_peak()
    );
}

#[test]
fn passing_the_ceiling_stops_the_search_with_a_typed_reason_not_a_truncated_result() {
    let fst = adjacent_captures();
    let unbounded = WorkBudget::new(usize::MAX / 2).with_retained_cap(None);
    run(&unbounded, &fst);
    let ceiling = unbounded.retained_peak() / 4;

    let bounded = WorkBudget::new(usize::MAX / 2).with_retained_cap(Some(ceiling));
    assert_eq!(
        run(&bounded, &fst),
        0,
        "a stopped search yields no partial matches"
    );
    assert!(
        bounded.retained_capped(),
        "the stop names the capture ceiling"
    );
    assert!(
        bounded.capped(),
        "and reads as incomplete to every existing cap check"
    );
    assert!(bounded.retained_peak() <= ceiling);
    assert_eq!(bounded.retained(), 0);
    assert!(bounded.used() < unbounded.used(), "it stopped early");
}

#[test]
fn a_ceiling_above_the_peak_changes_nothing() {
    let fst = adjacent_captures();
    let unbounded = WorkBudget::new(usize::MAX / 2).with_retained_cap(None);
    let expected = run(&unbounded, &fst);
    let roomy = WorkBudget::new(usize::MAX / 2).with_retained_cap(Some(unbounded.retained_peak()));
    assert_eq!(run(&roomy, &fst), expected);
    assert!(!roomy.retained_capped());
    assert_eq!(roomy.used(), unbounded.used());
}

/// Once the visited set is large it switches to compact records that share their parents' registers: the whole set costs less than one full packed register block per completed split. (Small sets keep the plain hash set, which costs a block each.)
#[test]
fn large_visited_sets_cost_less_than_a_register_block_per_split() {
    let fst = adjacent_captures();
    let block = fst.register_count() as usize * 2 * std::mem::size_of::<u32>();
    let segments = 14;
    let splits = 8568; // C(14 - 1 + 5, 5)
    let budget = WorkBudget::new(usize::MAX / 2).with_retained_cap(None);
    assert_eq!(run_segments(&budget, &fst, segments), splits);
    assert!(
        budget.retained_peak() < splits * block,
        "peak {} bytes, {} per split; a full block is {block}",
        budget.retained_peak(),
        budget.retained_peak() / splits
    );
}
