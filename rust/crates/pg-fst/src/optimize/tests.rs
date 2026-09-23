use crate::compile::{CompileInput, CompileNode};

/// White-box check of the freeze-time `min_hops_to_accept` BFS on a plain 3-symbol chain, checked on both the determinized and epsilon-removed compilations.
#[test]
fn min_hops_to_accept_chain_pattern() {
    let nodes = vec![
        CompileNode::Constraint(vec![0b001u64]),
        CompileNode::Constraint(vec![0b010u64]),
        CompileNode::Constraint(vec![0b100u64]),
    ];
    for det in [true, false] {
        let fst = CompileInput::new(nodes.clone())
            .deterministic(det)
            .compile();
        let hops = fst.min_hops_to_accept();
        assert_eq!(hops.len(), fst.state_count());
        assert_eq!(
            hops[fst.start() as usize],
            3,
            "start of `a b c` needs exactly 3 arcs (det={det})"
        );
        for (s, meta) in fst.states().iter().enumerate() {
            if meta.accepting {
                assert_eq!(hops[s], 0, "accepting state {s} (det={det})");
            } else {
                // Every non-dead, non-accepting state's bound is 1 + min over its arcs' targets (BFS relaxation fixpoint).
                let best = fst.arcs()[meta.arc_lo as usize..meta.arc_hi as usize]
                    .iter()
                    .map(|a| hops[a.target as usize])
                    .min()
                    .expect("chain states all have arcs");
                assert_eq!(hops[s], best + 1, "state {s} (det={det})");
            }
        }
    }
}
