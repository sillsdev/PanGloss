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

#[test]
fn deterministic_minimization_merges_equivalent_suffix_states() {
    let fst = CompileInput::new(vec![CompileNode::Alternation(vec![
        vec![
            CompileNode::Constraint(vec![1]),
            CompileNode::Constraint(vec![4]),
        ],
        vec![
            CompileNode::Constraint(vec![2]),
            CompileNode::Constraint(vec![4]),
        ],
    ])])
    .compile();
    assert_eq!(
        fst.state_count(),
        3,
        "equivalent states before the shared suffix must merge"
    );
}

fn minimize_test_state(
    accepting: bool,
    id: Option<&str>,
    finishers: Vec<Cmd>,
    arcs: Vec<BuildArc>,
) -> BuildState {
    BuildState {
        accepting,
        accept_infos: if accepting {
            vec![AcceptInfo {
                id: id.map(str::to_owned),
                priority: 0,
            }]
        } else {
            Vec::new()
        },
        finishers,
        arcs,
    }
}

fn minimize_test_arc(input: u64, target: usize, commands: Vec<Cmd>, priority: i32) -> BuildArc {
    BuildArc {
        input: MatchInput {
            pos: vec![input],
            negated: Vec::new(),
        },
        target,
        commands,
        priority,
    }
}

#[test]
fn minimization_preserves_accept_metadata_and_arc_commands() {
    let command = Cmd { dest: 0, src: 1 };
    for difference in 0..3 {
        let mut states = vec![
            minimize_test_state(
                false,
                None,
                vec![],
                vec![
                    minimize_test_arc(1, 1, vec![], 0),
                    minimize_test_arc(2, 2, vec![], 0),
                ],
            ),
            minimize_test_state(
                false,
                None,
                vec![],
                vec![minimize_test_arc(4, 3, vec![], 0)],
            ),
            minimize_test_state(
                false,
                None,
                vec![],
                vec![minimize_test_arc(
                    4,
                    4,
                    if difference == 2 {
                        vec![command]
                    } else {
                        vec![]
                    },
                    0,
                )],
            ),
            minimize_test_state(true, Some("a"), vec![], vec![]),
            minimize_test_state(
                true,
                Some(if difference == 0 { "b" } else { "a" }),
                if difference == 1 {
                    vec![command]
                } else {
                    vec![]
                },
                vec![],
            ),
        ];
        minimize(&mut states, 0);
        assert_ne!(
            states[0].arcs[0].target, states[0].arcs[1].target,
            "difference {difference} must keep the suffix states separate"
        );
    }
}

#[test]
fn minimization_uses_negated_input_as_a_set_and_keeps_representative_priority() {
    let mut states = vec![
        minimize_test_state(
            false,
            None,
            vec![],
            vec![
                minimize_test_arc(1, 1, vec![], 0),
                minimize_test_arc(2, 2, vec![], 0),
            ],
        ),
        minimize_test_state(
            false,
            None,
            vec![],
            vec![minimize_test_arc(4, 3, vec![], 7)],
        ),
        minimize_test_state(
            false,
            None,
            vec![],
            vec![minimize_test_arc(4, 3, vec![], 9)],
        ),
        minimize_test_state(true, Some("a"), vec![], vec![]),
    ];
    states[1].arcs[0].input.negated = vec![vec![8], vec![16]];
    states[2].arcs[0].input.negated = vec![vec![16], vec![8]];
    minimize(&mut states, 0);
    assert_eq!(states.len(), 3);
    assert_eq!(states[0].arcs[0].target, states[0].arcs[1].target);
    assert_eq!(states[1].arcs[0].priority, 7);
}

#[test]
fn nondeterministic_matcher_is_not_minimized() {
    let fst = CompileInput::new(vec![CompileNode::Alternation(vec![
        vec![
            CompileNode::Constraint(vec![1]),
            CompileNode::Constraint(vec![4]),
        ],
        vec![
            CompileNode::Constraint(vec![2]),
            CompileNode::Constraint(vec![4]),
        ],
    ])])
    .deterministic(false)
    .compile();
    assert!(fst.state_count() > 3);
}

#[test]
fn minimization_accept_info_equality_ignores_priority() {
    let mut states = vec![
        minimize_test_state(
            false,
            None,
            vec![],
            vec![
                minimize_test_arc(1, 1, vec![], 0),
                minimize_test_arc(2, 2, vec![], 0),
            ],
        ),
        minimize_test_state(true, Some("a"), vec![], vec![]),
        minimize_test_state(true, Some("a"), vec![], vec![]),
    ];
    states[1].accept_infos[0].priority = 3;
    states[2].accept_infos[0].priority = 9;
    minimize(&mut states, 0);
    assert_eq!(states.len(), 2);
    assert_eq!(states[0].arcs[0].target, states[0].arcs[1].target);
    assert_eq!(states[1].accept_infos[0].priority, 3);
}
