//! foma/topsort.c — literal (bug-for-bug) Wave-2 port per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/topsort.md
//! (per-file ids) plus the fomalib.h prototype ids.
//!
//! fsm_topsort renumbers states in topological order (Kahn's algorithm over
//! inverse-arc counts) while counting accepting paths. Wave 4 widens invcount
//! from the C's `unsigned short` to `i32`, so in-degrees past 65535 no longer
//! wrap (a latent correctness bug — see the `+1`-bumped `fsm-topsort-fn` sem
//! rule). States unreachable from state 0 still make the net be misreported as
//! cyclic (a genuine C-compatible quirk, kept).

use crate::constructions::{add_fsm_arc, fsm_count};
use crate::int_stack::IntStack;
#[cfg(test)]
use crate::options::FomaOptions;
use crate::types::{Fsm, FsmState, PATHCOUNT_CYCLIC, PATHCOUNT_OVERFLOW};

// [spec:foma:def:topsort.fsm-topsort-fn]
// [spec:foma:sem:topsort.fsm-topsort-fn+1]
// [spec:foma:def:fomalib.fsm-topsort-fn]
// [spec:foma:sem:fomalib.fsm-topsort-fn+1]
pub fn fsm_topsort(net: Box<Fsm>) -> Box<Fsm> {
    let mut int_stack = IntStack::new();
    /* We topologically sort the network by looking for a state          */
    /* with inverse count 0. We then examine all the arcs from that      */
    /* state, and decrease the target invcounts. If we find a new        */
    /* state with invcount 0, we push that on the stack to be treated    */
    /* If the graph is cyclic, one of two things will happen:            */

    /* (1) We fail to find a state with invcount 0 before we've treated  */
    /*     all states                                                    */
    /* (2) A state under treatment has an arc to a state already treated */
    /*     or itself (we mark a state as treated as soon as we start     */
    /*     working on it).                                               */
    /* Of course we also count the number of paths in the network.       */

    let mut net = net;

    /* C: if (net == NULL) { return NULL; } — a Box argument is never
    NULL; NULL-able callers keep the check at the call site */

    fsm_count(&mut net);

    /* C: fsm = net->states — reads below index net.states directly */

    let mut statemap: Vec<i32> = vec![-1; net.statecount as usize];
    let mut order: Vec<i32> = vec![0; net.statecount as usize];
    let mut pathcount: Vec<i64> = vec![0; net.statecount as usize];
    /* C mallocs newnum uninitialized; only entries of treated states are
    ever read back */
    let mut newnum: Vec<i32> = vec![0; net.statecount as usize];
    /* Wave 4 fix: i32 in-degree (was unsigned short, which wrapped past
    65535 in-arcs); +1-bumped fsm-topsort-fn sem rule */
    let mut invcount: Vec<i32> = vec![0; net.statecount as usize];
    let mut treated: Vec<u8> = vec![0; net.statecount as usize];

    /* the vec! initializers above subsume C's explicit init loop over
    statemap/invcount/treated/order/pathcount */

    let mut lc: i32 = 0;

    /* goto cyclic → break 'cyclic (cleanup after the block, as in C) */
    'cyclic: {
        let mut i: i32 = 0;
        while net.states[i as usize].state_no != -1 {
            lc += 1;
            if net.states[i as usize].target != -1 {
                let target = net.states[i as usize].target;
                invcount[target as usize] += 1;
                /* Do a fast check here to see if we have a selfloop */
                if net.states[i as usize].state_no == target {
                    net.pathcount = PATHCOUNT_CYCLIC;
                    net.is_loop_free = 0;
                    break 'cyclic;
                }
            }
            if statemap[net.states[i as usize].state_no as usize] == -1 {
                statemap[net.states[i as usize].state_no as usize] = i;
            }
            i += 1;
        }

        let mut treatcount = net.statecount;
        int_stack.clear();
        int_stack.push(0);
        let mut grand_pathcount: i64 = 0;

        pathcount[0] = 1;

        let mut overflow: u8 = 0;
        let mut i: i32 = 0;
        while !int_stack.is_empty() {
            /* Treat a state */
            let curr_state = int_stack.pop();
            treated[curr_state as usize] = 1;
            order[i as usize] = curr_state;
            newnum[curr_state as usize] = i;

            treatcount -= 1;
            let mut curr_fsm = statemap[curr_state as usize] as usize;
            while net.states[curr_fsm].state_no == curr_state {
                if net.states[curr_fsm].target != -1 {
                    let target = net.states[curr_fsm].target;
                    invcount[target as usize] -= 1;

                    /* Check if we overflow the path counter */

                    if overflow == 0 {
                        /* C: signed 64-bit addition; overflow observed as wrap */
                        pathcount[target as usize] =
                            pathcount[target as usize].wrapping_add(pathcount[curr_state as usize]);
                        if pathcount[target as usize] < 0 {
                            overflow = 1;
                        }
                    }

                    /* Case (1) for cyclic */
                    if treated[target as usize] == 1 {
                        net.pathcount = PATHCOUNT_CYCLIC;
                        net.is_loop_free = 0;
                        break 'cyclic;
                    }
                    if invcount[target as usize] == 0 {
                        int_stack.push(target);
                    }
                }
                curr_fsm += 1;
            }
            i += 1;
        }

        /* Case (2) */
        if treatcount > 0 {
            net.pathcount = PATHCOUNT_CYCLIC;
            net.is_loop_free = 0;
            break 'cyclic;
        }

        /* C: malloc(sizeof(struct fsm_state) * (lc+1)), uninitialized;
        written by add_fsm_arc below */
        let mut new_fsm: Vec<FsmState> = vec![
            FsmState {
                state_no: 0,
                r#in: 0,
                out: 0,
                target: 0,
                final_state: 0,
                start_state: 0,
            };
            (lc + 1) as usize
        ];
        let mut j: i32 = 0;
        let mut i: i32 = 0;
        while i < net.statecount {
            let curr_state = order[i as usize];
            let mut curr_fsm = statemap[curr_state as usize] as usize;

            if net.states[curr_fsm].final_state == 1 && overflow == 0 {
                grand_pathcount = grand_pathcount.wrapping_add(pathcount[curr_state as usize]);
                if grand_pathcount < 0 {
                    overflow = 1;
                }
            }

            while net.states[curr_fsm].state_no == curr_state {
                let newstate = if net.states[curr_fsm].state_no == -1 {
                    -1
                } else {
                    newnum[net.states[curr_fsm].state_no as usize]
                };
                let newtarget = if net.states[curr_fsm].target == -1 {
                    -1
                } else {
                    newnum[net.states[curr_fsm].target as usize]
                };
                let (r#in, out) = (
                    net.states[curr_fsm].r#in as i32,
                    net.states[curr_fsm].out as i32,
                );
                let (final_state, start_state) = (
                    net.states[curr_fsm].final_state as i32,
                    net.states[curr_fsm].start_state as i32,
                );
                add_fsm_arc(
                    &mut new_fsm,
                    j,
                    newstate,
                    r#in,
                    out,
                    newtarget,
                    final_state,
                    start_state,
                );
                j += 1;
                curr_fsm += 1;
            }
            i += 1;
        }

        add_fsm_arc(&mut new_fsm, j, -1, -1, -1, -1, -1, -1);
        /* net->states = new_fsm; ... free(fsm) — the old array is dropped
        by the assignment */
        net.states = new_fsm;
        net.pathcount = grand_pathcount;
        net.is_loop_free = 1;
        if overflow == 1 {
            net.pathcount = PATHCOUNT_OVERFLOW;
        }
    }

    /* cyclic: free(statemap/order/pathcount/newnum/invcount/treated) —
    dropped on return */
    int_stack.clear();
    net
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynarray::{
        fsm_construct_add_arc, fsm_construct_done, fsm_construct_init, fsm_construct_set_final,
        fsm_construct_set_initial,
    };
    use crate::regex::fsm_parse_regex;

    /// Line table up to (excluding) the state_no == -1 sentinel.
    fn lines(net: &Fsm) -> Vec<(i32, i16, i16, i32, i8, i8)> {
        net.states
            .iter()
            .take_while(|l| l.state_no != -1)
            .map(|l| {
                (
                    l.state_no,
                    l.r#in,
                    l.out,
                    l.target,
                    l.final_state,
                    l.start_state,
                )
            })
            .collect()
    }

    // [spec:foma:sem:topsort.fsm-topsort-fn+1/test]
    // [spec:foma:sem:fomalib.fsm-topsort-fn+1/test]
    #[test]
    fn topsort_renumbers_acyclic_net_topologically() {
        /* Non-topological numbering: 0 -a-> 2, 2 -b-> 1, 1 final.
        Topological ranks: 0 -> 0, 2 -> 1, 1 -> 2. Sigma: a=3, b=4. */
        let mut h = fsm_construct_init("t");
        fsm_construct_add_arc(&mut h, 0, 2, "a", "a");
        fsm_construct_add_arc(&mut h, 2, 1, "b", "b");
        fsm_construct_set_final(&mut h, 1);
        fsm_construct_set_initial(&mut h, 0);
        let net = fsm_topsort(fsm_construct_done(h));
        assert_eq!(
            lines(&net),
            vec![
                (0, 3, 3, 1, 0, 1),
                (1, 4, 4, 2, 0, 0),
                (2, -1, -1, -1, 1, 0),
            ]
        );
        /* terminator line appended after the rebuilt lines */
        assert_eq!(net.states[3].state_no, -1);
        assert_eq!(net.pathcount, 1);
        assert_eq!(net.is_loop_free, 1);
    }

    // [spec:foma:sem:topsort.fsm-topsort-fn+1/test]
    // [spec:foma:sem:fomalib.fsm-topsort-fn+1/test]
    #[test]
    fn topsort_counts_paths_of_small_acyclic_languages() {
        let opts = &FomaOptions::default();
        for (re, expected) in [("[a|b] c", 2i64), ("[a|b] [c|d]", 4), ("a b c", 1)] {
            let net = fsm_topsort(fsm_parse_regex(opts, re, None, None).unwrap());
            assert_eq!(net.pathcount, expected, "pathcount of {re}");
            assert_eq!(net.is_loop_free, 1, "loop-free flag of {re}");
            /* state numbers equal topological rank: lines stay grouped in
            ascending new order and every arc goes low -> high */
            let mut prev = 0i32;
            for l in &net.states {
                if l.state_no == -1 {
                    break;
                }
                assert!(l.state_no >= prev, "{re}: states emitted in new order");
                prev = l.state_no;
                if l.target != -1 {
                    assert!(l.state_no < l.target, "{re}: arc goes low -> high");
                }
            }
            /* initial state keeps number 0 */
            assert_eq!(net.states[0].state_no, 0);
            assert_eq!(net.states[0].start_state, 1);
        }
    }

    // [spec:foma:sem:topsort.fsm-topsort-fn+1/test]
    // [spec:foma:sem:fomalib.fsm-topsort-fn+1/test]
    #[test]
    fn topsort_selfloop_marks_cyclic_and_leaves_states_untouched() {
        let opts = &FomaOptions::default();
        /* a+ has a self-loop: caught in pass 1 over the line array */
        let net = fsm_parse_regex(opts, "a+", None, None).unwrap();
        let before = lines(&net);
        let net = fsm_topsort(net);
        assert_eq!(net.pathcount, PATHCOUNT_CYCLIC);
        assert_eq!(net.pathcount, -1);
        assert_eq!(net.is_loop_free, 0);
        assert_eq!(lines(&net), before, "state array left untouched");
    }

    // [spec:foma:sem:topsort.fsm-topsort-fn+1/test]
    // [spec:foma:sem:fomalib.fsm-topsort-fn+1/test]
    #[test]
    fn topsort_two_state_cycle_marks_cyclic_via_treatcount() {
        let opts = &FomaOptions::default();
        /* [a b]+ has the cycle 1 -b-> 2 -a-> 1 and no self-loop: no state on
        the cycle ever reaches invcount 0, so treatcount stays > 0 */
        let net = fsm_parse_regex(opts, "[a b]+", None, None).unwrap();
        let before = lines(&net);
        let net = fsm_topsort(net);
        assert_eq!(net.pathcount, PATHCOUNT_CYCLIC);
        assert_eq!(net.is_loop_free, 0);
        assert_eq!(lines(&net), before, "state array left untouched");
    }

    // [spec:foma:sem:topsort.fsm-topsort-fn+1/test]
    // [spec:foma:sem:fomalib.fsm-topsort-fn+1/test]
    #[test]
    fn topsort_back_edge_into_treated_state_marks_cyclic() {
        let opts = &FomaOptions::default();
        /* [a b]* loops back into the initial state: the arc into already-
        treated state 0 triggers the treated[target] == 1 cyclic exit */
        let net = fsm_parse_regex(opts, "[a b]*", None, None).unwrap();
        let before = lines(&net);
        let net = fsm_topsort(net);
        assert_eq!(net.pathcount, PATHCOUNT_CYCLIC);
        assert_eq!(net.is_loop_free, 0);
        assert_eq!(lines(&net), before, "state array left untouched");
    }
}
