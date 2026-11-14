use super::*;

/// Hand-reasoned pre-order DFS numbering of `MarkArcPriorities` (Fst.cs:801-817): a deeper state is fully descended before the start's next arc, verifying the exact stack discipline.
#[test]
fn mark_arc_priorities_preorder() {
    // states: 0(start) -> 1 (arc s0a0), 0 -> 2 (arc s0a1); 1 -> 3, 2 -> 3.
    let mut nfa = Nfa::new(); // creates state 0
    let s1 = nfa.create_state();
    let s2 = nfa.create_state();
    let s3 = nfa.create_state();
    nfa.add_epsilon(nfa.start, s1); // state0 arc0
    nfa.add_epsilon(nfa.start, s2); // state0 arc1
    nfa.add_epsilon(s1, s3); // state1 arc0
    nfa.add_epsilon(s2, s3); // state2 arc0
    nfa.mark_arc_priorities();
    // Equal-type arcs insert at the search hit (ArcCollection.cs), so the later arc is first.
    assert_eq!(nfa.states[0].arcs[0].priority, 0);
    assert_eq!(nfa.states[2].arcs[0].priority, 1);
    assert_eq!(nfa.states[0].arcs[1].priority, 2);
    assert_eq!(nfa.states[1].arcs[0].priority, 3);
}

/// `insert_arc` reproduces `ArcCollection.AddInternal` (BinarySearch with the reversed priority-type comparer, insert at the hit) across types.
#[test]
fn insert_arc_orders_across_priority_types_like_csharp() {
    let mut nfa = Nfa::new();
    let t: Vec<usize> = (0..6).map(|_| nfa.create_state()).collect();
    // Low=2, Medium=1, High=0; targets named by add order a..f.
    for (target, ptype) in [
        (t[0], 2u8),
        (t[1], 0),
        (t[2], 1),
        (t[3], 1),
        (t[4], 0),
        (t[5], 2),
    ] {
        nfa.add_epsilon_typed(nfa.start, target, ptype);
    }
    let order: Vec<usize> = nfa.states[0].arcs.iter().map(|a| a.target).collect();
    assert_eq!(order, vec![t[5], t[0], t[3], t[2], t[4], t[1]]);
}
