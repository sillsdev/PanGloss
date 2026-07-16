//! foma/coaccessible.c — literal (bug-for-bug) Wave-2 port per
//! docs/port/rust-conventions.md. Sem rules:
//! docs/spec/port/foma/coaccessible.md (per-file ids) plus the fomalib.h
//! prototype id.
//!
//! fsm_coaccessible prunes states from which no final state is reachable,
//! compacting the line array in place. All scratch (inverse adjacency,
//! coacc/mapping/added marker arrays) is locally owned — nothing survives the
//! call. Kept quirks:
//! - mapping[0] = 0 is set unconditionally ("state 0 always exists"). Evaluated
//!   in Wave 4 and KEPT: foma only ever prunes accessible nets (every state is
//!   reachable from start state 0), so whenever the language is nonempty state 0
//!   reaches a final and is itself coaccessible, making mapping[0] = 0 correct;
//!   the empty-language case is diverted through the markcount == 0 branch. The
//!   renumber-from-1 outcome only arises for a disconnected net (state 0 not
//!   reachable to any final while another component is) that the pipeline never
//!   builds — pinned by a test but benign, so it stays C-compatible.
//! - when the write index has caught up with the read index the post-write
//!   re-reads of line i observe the remapped values, as in C.

use crate::constructions::add_fsm_arc;
use crate::int_stack::IntStack;
#[cfg(test)]
use crate::options::FomaOptions;
use crate::sigma::sigma_create;
use crate::structures::{fsm_empty, fsm_sigma_destroy};
use crate::types::{Fsm, YES};

// [spec:foma:def:coaccessible.invtable]
pub struct Invtable {
    pub state: i32,
    pub next: Option<Box<Invtable>>,
}

// [spec:foma:def:coaccessible.fsm-coaccessible-fn]
// [spec:foma:sem:coaccessible.fsm-coaccessible-fn+2]
// [spec:foma:def:fomalib.fsm-coaccessible-fn]
// [spec:foma:sem:fomalib.fsm-coaccessible-fn+2]
pub fn fsm_coaccessible(net: Box<Fsm>) -> Box<Fsm> {
    let mut int_stack = IntStack::new();
    let mut net = net;

    /* C: fsm = net->states — reads/writes below index net.states directly */
    let mut new_arccount = 0;
    /* one inverse-adjacency head per state (zeroed) */
    let mut inverses: Vec<Invtable> = (0..net.statecount)
        .map(|_| Invtable {
            state: 0,
            next: None,
        })
        .collect();
    let mut coacc: Vec<i32> = vec![0; net.statecount as usize];
    /* only entries of coaccessible states (and slot 0) are ever read back */
    let mut mapping: Vec<i32> = vec![0; net.statecount as usize];
    let mut added: Vec<i32> = vec![0; net.statecount as usize];

    for i in 0..net.statecount {
        inverses[i as usize].state = -1;
        coacc[i as usize] = 0;
        added[i as usize] = 0;
    }

    let mut i: i32 = 0;
    while net.states[i as usize].state_no != -1 {
        let s = net.states[i as usize].state_no;
        let t = net.states[i as usize].target;
        if t != -1 && s != t {
            if inverses[t as usize].state == -1 {
                inverses[t as usize].state = s;
            } else {
                /* malloc'd chain node spliced directly after the head */
                let temp_i = Box::new(Invtable {
                    state: s,
                    next: inverses[t as usize].next.take(),
                });
                inverses[t as usize].next = Some(temp_i);
            }
        }
        i += 1;
    }

    /* Push & mark finals */

    let mut markcount = 0;
    let mut i: i32 = 0;
    while net.states[i as usize].state_no != -1 {
        if net.states[i as usize].final_state != 0
            && coacc[net.states[i as usize].state_no as usize] == 0
        {
            int_stack.push(net.states[i as usize].state_no);
            coacc[net.states[i as usize].state_no as usize] = 1;
            markcount += 1;
        }
        i += 1;
    }

    let mut terminate = 0;
    while !int_stack.is_empty() {
        let current_state = int_stack.pop();
        /* current_ptr = inverses+current_state; the array-resident head,
        then its malloc'd chain */
        let mut current_ptr: Option<&Invtable> = Some(&inverses[current_state as usize]);
        while let Some(p) = current_ptr {
            if p.state == -1 {
                break;
            }
            if coacc[p.state as usize] == 0 {
                coacc[p.state as usize] = 1;
                int_stack.push(p.state);
                markcount += 1;
            }
            current_ptr = p.next.as_deref();
        }
        if markcount >= net.statecount {
            /* printf("Already coacc\n");  */
            terminate = 1;
            int_stack.clear();
            break;
        }
    }

    if terminate == 0 {
        // [spec:foma:sem:coaccessible.fsm-coaccessible-fn+2] if the start state
        // (0) is itself not coaccessible (or the machine is already empty), no
        // path from the start reaches a final, so L = ∅. Return the canonical
        // empty machine in the same well-formed shape fsm_empty_set produces: a
        // single non-final start state (statecount 1, linecount 2) over a fresh
        // empty sigma. The coacc.is_empty() guard makes the function idempotent
        // — re-pruning an already-empty machine (which enters with statecount 0
        // and so an empty coacc) returns here instead of indexing coacc[0] out
        // of bounds. This subsumes the no-final-states case (markcount == 0 ⟹
        // coacc[0] == 0). The C source instead set mapping[0] = 0 unconditionally
        // ("state 0 always exists"), renumbering any orphaned coaccessible
        // component into a startless net; a pruned start makes L empty regardless
        // of a disconnected component, so the empty machine is the correct result.
        if coacc.is_empty() || coacc[0] == 0 {
            net.states = fsm_empty();
            fsm_sigma_destroy(core::mem::take(&mut net.sigma));
            net.sigma = sigma_create();
            net.statecount = 1;
            net.finalcount = 0;
            net.arccount = 0;
            net.linecount = 2;
            net.pathcount = 0;
            net.is_pruned = YES;
            return net;
        }
        mapping[0] = 0; /* state 0 always exists */
        let mut new_linecount = 0;
        {
            let mut j = 0;
            for i in 1..net.statecount {
                if coacc[i as usize] == 1 {
                    j += 1;
                    mapping[i as usize] = j;
                }
            }
        }

        let mut i: i32 = 0;
        let mut j: i32 = 0;
        while net.states[i as usize].state_no != -1 {
            if i > 0
                && net.states[i as usize].state_no != net.states[(i - 1) as usize].state_no
                && net.states[(i - 1) as usize].final_state != 0
                && added[net.states[(i - 1) as usize].state_no as usize] == 0
            {
                /* synthetic final line for a state all of whose arcs were
                pruned */
                let state_no = mapping[net.states[(i - 1) as usize].state_no as usize];
                let start_state = net.states[(i - 1) as usize].start_state as i32;
                add_fsm_arc(&mut net.states, j, state_no, -1, -1, -1, 1, start_state);
                j += 1;
                new_linecount += 1;
                added[net.states[(i - 1) as usize].state_no as usize] = 1;
                /* printf("addf ad %i\n",i); */
            }
            if coacc[net.states[i as usize].state_no as usize] != 0
                && (net.states[i as usize].target == -1
                    || coacc[net.states[i as usize].target as usize] != 0)
            {
                net.states[j as usize].state_no = mapping[net.states[i as usize].state_no as usize];
                if net.states[i as usize].target == -1 {
                    net.states[j as usize].target = -1;
                } else {
                    net.states[j as usize].target = mapping[net.states[i as usize].target as usize];
                }
                net.states[j as usize].final_state = net.states[i as usize].final_state;
                net.states[j as usize].start_state = net.states[i as usize].start_state;
                net.states[j as usize].r#in = net.states[i as usize].r#in;
                net.states[j as usize].out = net.states[i as usize].out;
                j += 1;
                new_linecount += 1;
                added[net.states[i as usize].state_no as usize] = 1;
                if net.states[i as usize].target != -1 {
                    new_arccount += 1;
                }
            }
            i += 1;
        }

        if i > 1
            && net.states[(i - 1) as usize].final_state != 0
            && added[net.states[(i - 1) as usize].state_no as usize] == 0
        {
            /* printf("addf\n"); */
            let state_no = mapping[net.states[(i - 1) as usize].state_no as usize];
            let start_state = net.states[(i - 1) as usize].start_state as i32;
            add_fsm_arc(&mut net.states, j, state_no, -1, -1, -1, 1, start_state);
            j += 1;
            new_linecount += 1;
        }

        if new_linecount == 0 {
            add_fsm_arc(&mut net.states, j, 0, -1, -1, -1, -1, -1);
            j += 1;
        }

        add_fsm_arc(&mut net.states, j, -1, -1, -1, -1, -1, -1);
        // (markcount == 0 — the empty language — is now handled up front by the
        // coacc[0] == 0 early return, so state 0 is always coaccessible here.)
        net.linecount = new_linecount;
        net.arccount = new_arccount;
        net.statecount = markcount;
    }

    /* printf("Markccount %i \n",markcount); */

    net.is_pruned = YES;
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
    use crate::structures::fsm_create;
    use crate::types::FsmState;

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

    // [spec:foma:sem:coaccessible.fsm-coaccessible-fn/test]
    // [spec:foma:sem:fomalib.fsm-coaccessible-fn/test]
    #[test]
    fn coaccessible_prunes_dead_end_state_and_updates_counts() {
        /* 0 -a-> 1 (final), 0 -b-> 2 (dead end): state 2 is pruned */
        let mut h = fsm_construct_init("c");
        fsm_construct_add_arc(&mut h, 0, 1, "a", "a");
        fsm_construct_add_arc(&mut h, 0, 2, "b", "b");
        fsm_construct_set_final(&mut h, 1);
        fsm_construct_set_initial(&mut h, 0);
        let net = fsm_coaccessible(fsm_construct_done(h));
        assert_eq!(lines(&net), vec![(0, 3, 3, 1, 0, 1), (1, -1, -1, -1, 1, 0)]);
        assert_eq!(net.statecount, 2);
        assert_eq!(net.linecount, 2);
        assert_eq!(net.arccount, 1);
        assert_eq!(net.is_pruned, YES);
    }

    // [spec:foma:sem:coaccessible.fsm-coaccessible-fn/test]
    // [spec:foma:sem:fomalib.fsm-coaccessible-fn/test]
    #[test]
    fn coaccessible_marks_predecessors_through_inverse_chain_nodes() {
        /* state 2 has two predecessors (0 and 1): the second one lives in a
        malloc'd invtable chain node spliced after the array-resident head;
        state 1 is only reached through that chain. State 3 is a dead end. */
        let mut h = fsm_construct_init("c");
        fsm_construct_add_arc(&mut h, 0, 2, "a", "a");
        fsm_construct_add_arc(&mut h, 1, 2, "b", "b");
        fsm_construct_add_arc(&mut h, 0, 3, "c", "c");
        fsm_construct_set_final(&mut h, 2);
        fsm_construct_set_initial(&mut h, 0);
        let net = fsm_coaccessible(fsm_construct_done(h));
        assert_eq!(
            lines(&net),
            vec![
                (0, 3, 3, 2, 0, 1),
                (1, 4, 4, 2, 0, 0),
                (2, -1, -1, -1, 1, 0),
            ]
        );
        assert_eq!(net.statecount, 3);
        assert_eq!(net.linecount, 3);
        assert_eq!(net.arccount, 2);
        assert_eq!(net.is_pruned, YES);
    }

    // [spec:foma:sem:coaccessible.fsm-coaccessible-fn+2/test]
    // [spec:foma:sem:fomalib.fsm-coaccessible-fn+2/test]
    #[test]
    fn coaccessible_empty_language_when_start_not_coaccessible() {
        /* State 0 (the start) is NOT coaccessible (its only arc reaches dead
        end 3); the 1 -b-> 2 (final) component is disconnected from the start.
        Since no path from the start reaches a final, L = ∅, so the result is
        the canonical empty automaton in the well-formed fsm_empty_set shape: a
        single non-final start state, statecount 1, linecount 2, fresh sigma —
        not a renumbering of the orphaned component into a startless net. */
        let mut h = fsm_construct_init("c");
        fsm_construct_add_arc(&mut h, 0, 3, "a", "a");
        fsm_construct_add_arc(&mut h, 1, 2, "b", "b");
        fsm_construct_set_final(&mut h, 2);
        fsm_construct_set_initial(&mut h, 0);
        let net = fsm_coaccessible(fsm_construct_done(h));
        assert_eq!(lines(&net), vec![(0, -1, -1, -1, 0, 1)]);
        assert_eq!(net.statecount, 1);
        assert_eq!(net.linecount, 2);
        assert_eq!(net.arccount, 0);
        assert!(net.sigma.is_empty(), "fresh empty sigma");
        assert_eq!(net.is_pruned, YES);
        // Idempotent: re-pruning the empty machine (statecount 1 ⟹ non-empty
        // coacc) returns the same shape instead of indexing coacc out of bounds.
        let net = fsm_coaccessible(net);
        assert_eq!(lines(&net), vec![(0, -1, -1, -1, 0, 1)]);
        assert_eq!(net.statecount, 1);
        assert_eq!(net.linecount, 2);
        assert_eq!(net.is_pruned, YES);
    }

    // [spec:foma:sem:coaccessible.fsm-coaccessible-fn/test]
    // [spec:foma:sem:fomalib.fsm-coaccessible-fn/test]
    #[test]
    fn coaccessible_emits_synthetic_final_line_when_all_arcs_pruned() {
        /* 0 -a-> 1 (final), 1 -b-> 2 (dead end): state 1 keeps no line of
        its own, so a synthetic arcless final line is emitted for it when
        the scan moves past its block */
        let mut h = fsm_construct_init("c");
        fsm_construct_add_arc(&mut h, 0, 1, "a", "a");
        fsm_construct_add_arc(&mut h, 1, 2, "b", "b");
        fsm_construct_set_final(&mut h, 1);
        fsm_construct_set_initial(&mut h, 0);
        let net = fsm_coaccessible(fsm_construct_done(h));
        assert_eq!(lines(&net), vec![(0, 3, 3, 1, 0, 1), (1, -1, -1, -1, 1, 0)]);
        assert_eq!(net.statecount, 2);
        assert_eq!(net.linecount, 2);
        assert_eq!(net.arccount, 1);
    }

    // [spec:foma:sem:coaccessible.fsm-coaccessible-fn/test]
    // [spec:foma:sem:fomalib.fsm-coaccessible-fn/test]
    #[test]
    fn coaccessible_emits_trailing_synthetic_final_line_for_last_state() {
        /* 0 -a-> 2 (final), 2 -b-> 1 (dead end): the final state 2 owns the
        LAST line block and keeps no line, exercising the post-scan
        synthetic-final branch. Survivors renumber 2 -> 1. */
        let mut h = fsm_construct_init("c");
        fsm_construct_add_arc(&mut h, 0, 2, "a", "a");
        fsm_construct_add_arc(&mut h, 2, 1, "b", "b");
        fsm_construct_set_final(&mut h, 2);
        fsm_construct_set_initial(&mut h, 0);
        let net = fsm_coaccessible(fsm_construct_done(h));
        assert_eq!(lines(&net), vec![(0, 3, 3, 1, 0, 1), (1, -1, -1, -1, 1, 0)]);
        assert_eq!(net.statecount, 2);
        assert_eq!(net.linecount, 2);
        assert_eq!(net.arccount, 1);
    }

    // [spec:foma:sem:coaccessible.fsm-coaccessible-fn/test]
    // [spec:foma:sem:fomalib.fsm-coaccessible-fn/test]
    #[test]
    fn coaccessible_all_coaccessible_terminates_early_with_counts_unchanged() {
        let opts = &FomaOptions::default();
        let net = fsm_parse_regex(opts, "a b", None, None).unwrap();
        let before = lines(&net);
        let (sc, lc, ac) = (net.statecount, net.linecount, net.arccount);
        let net = fsm_coaccessible(net);
        assert_eq!(lines(&net), before, "line array untouched");
        assert_eq!(net.statecount, sc);
        assert_eq!(net.linecount, lc);
        assert_eq!(net.arccount, ac);
        assert_eq!(net.is_pruned, YES);
    }

    // [spec:foma:sem:coaccessible.fsm-coaccessible-fn+2/test]
    // [spec:foma:sem:fomalib.fsm-coaccessible-fn+2/test]
    #[test]
    fn coaccessible_no_finals_yields_empty_language_shape() {
        /* no final state at all: markcount == 0 -> canonical empty machine
        in the well-formed fsm_empty_set shape (statecount 1, linecount 2,
        fresh sigma) */
        let mut net = fsm_create("");
        net.states = vec![
            FsmState {
                state_no: 0,
                r#in: 0,
                out: 0,
                target: 0,
                final_state: 0,
                start_state: 0,
            };
            3
        ];
        add_fsm_arc(&mut net.states, 0, 0, 3, 3, 1, 0, 1);
        add_fsm_arc(&mut net.states, 1, 1, -1, -1, -1, 0, 0);
        add_fsm_arc(&mut net.states, 2, -1, -1, -1, -1, -1, -1);
        net.statecount = 2;
        net.linecount = 2;
        net.arccount = 1;
        let net = fsm_coaccessible(net);
        /* fsm_empty(): one non-final start state, no arcs */
        assert_eq!(lines(&net), vec![(0, -1, -1, -1, 0, 1)]);
        assert_eq!(net.states[1].state_no, -1);
        assert_eq!(net.statecount, 1);
        assert_eq!(net.linecount, 2);
        assert_eq!(net.arccount, 0);
        assert!(net.sigma.is_empty(), "fresh empty sigma");
        assert_eq!(net.is_pruned, YES);
    }
}
