use super::*;
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

fn seed(state: usize, max: i32, last: i32, tags: &[(i32, i32)]) -> Nsi {
    Nsi {
        state,
        max_priority: max,
        last_priority: last,
        tags: tags.iter().copied().collect(),
    }
}

/// Closure seeds: one entry per NFA state, the last seed of a state wins in place, and entries keep first-insertion order (C# `Dictionary` overwrite).
#[test]
fn epsilon_closure_last_seed_wins_in_insertion_order() {
    let mut nfa = Nfa::new();
    nfa.create_state();
    let out = epsilon_closure(
        &nfa,
        vec![
            seed(0, 0, 0, &[]),
            seed(1, 0, 0, &[]),
            seed(0, 5, 5, &[(3, 0)]),
        ],
        0,
    );
    assert_eq!(out.len(), 2);
    assert_eq!((out[0].state, out[0].max_priority), (0, 5));
    assert_eq!(out[1].state, 1);
}

/// C# keys the closure on the NFA state alone: a state reached by a tagged and an untagged path is one entry, and only the NFA state is the key.
#[test]
fn epsilon_closure_keys_on_state_alone() {
    let mut nfa = Nfa::new();
    let a = nfa.create_state();
    let join = nfa.create_state();
    let tagged = nfa.create_tag(0, "g", true);
    nfa.add_epsilon(a, join);
    nfa.add_epsilon(tagged, join);
    nfa.mark_arc_priorities();
    let out = epsilon_closure(&nfa, vec![seed(0, 0, 0, &[])], 0);
    let at_join: Vec<&Nsi> = out.iter().filter(|n| n.state == join).collect();
    assert_eq!(at_join.len(), 1, "one entry per NFA state, not per tag set");
}
