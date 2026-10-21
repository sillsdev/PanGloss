use super::*;
use pg_grammar_model::model::StratumId;

fn lookup(node: u64, hit: bool, work: u64) {
    charge_lookup(
        node,
        StratumId(0),
        &pg_shape::Shape::default(),
        hit,
        work,
        0,
    );
}

#[test]
fn liveness_climbs_edges_and_reaches_aliases_of_live_canonicals() {
    reset();
    // 1 -> 2 -> 3 (hit); 1 -> 4 (miss); 5 folded into 2; 6 -> 5 only.
    edge(1, 2);
    edge(2, 3);
    edge(1, 4);
    alias(2, 5);
    edge(6, 5);
    for n in [1, 2, 4, 6] {
        charge_analysis(n, 1, 10, 0);
    }
    lookup(3, true, 7);
    lookup(4, false, 5);
    let s = snapshot();
    assert_eq!(s.work, 40);
    // Only node 4 is dead: 6 lives through its alias child 5, whose canonical 2 is live.
    assert_eq!(s.dead_work, 10);
    assert_eq!(s.dead_lookup_work, 5);
    assert_eq!(s.dead_leaf_work, 10);
    assert_eq!(s.dead_nodes, 1);
    reset();
}

#[test]
fn a_repeated_shape_charges_its_lookup_work_as_duplicate() {
    reset();
    lookup(1, false, 4);
    lookup(2, false, 6);
    let s = snapshot();
    assert_eq!(s.lookups, 2);
    assert_eq!(s.distinct_lookup_shapes, 1);
    assert_eq!(s.dup_lookup_work, 6);
    reset();
}

#[test]
fn a_dead_internal_node_that_may_compound_is_bucketed_apart() {
    reset();
    edge(1, 2);
    edge(3, 4);
    mark_may_compound(3);
    for n in [1, 2, 3, 4] {
        charge_analysis(n, 1, 10, 0);
    }
    let s = snapshot();
    assert_eq!(s.dead_work, 40);
    assert_eq!(s.dead_leaf_work, 20);
    assert_eq!(s.dead_compound_work, 10);
    assert_eq!(s.dead_internal_plain_work, 10);
    reset();
}
