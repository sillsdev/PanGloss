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
    // Expected: 0->1 = 0, 1->3 = 1, 0->2 = 2, 2->3 = 3.
    assert_eq!(nfa.states[0].arcs[0].priority, 0);
    assert_eq!(nfa.states[1].arcs[0].priority, 1);
    assert_eq!(nfa.states[0].arcs[1].priority, 2);
    assert_eq!(nfa.states[2].arcs[0].priority, 3);
}
