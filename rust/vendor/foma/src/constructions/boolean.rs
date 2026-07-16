//! Wave-4 split of constructions.c (see mod.rs). Cross-module and
//! external names come via `use super::*` (re-exported by mod.rs).
use super::*;

pub const COMPLEMENT: i32 = 0;
pub const COMPLETE: i32 = 1;

// [spec:foma:def:constructions.fsm-concat-fn]
// [spec:foma:sem:constructions.fsm-concat-fn]
// [spec:foma:def:fomalib.fsm-concat-fn]
// [spec:foma:sem:fomalib.fsm-concat-fn]
pub fn fsm_concat(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    let mut net1 = net1;
    let mut net2 = net2;

    fsm_merge_sigma(opts, &mut net1, &mut net2);

    fsm_count(&mut net1);
    fsm_count(&mut net2);
    /* The concatenation of a language with no final state should yield the empty language */
    if net1.finalcount == 0 || net2.finalcount == 0 {
        fsm_destroy(net1);
        fsm_destroy(net2);
        let net1 = fsm_empty_set();
        return net1;
    }

    /* Add |fsm1| states to the state numbers of fsm2 */
    let statecount1 = net1.statecount;
    fsm_add_to_states(&mut net2, statecount1);

    /* C: malloc'd (uninitialized); zeroed lines here */
    let mut new_fsm: Vec<FsmState> = vec![
        FsmState {
            state_no: 0,
            r#in: 0,
            out: 0,
            target: 0,
            final_state: 0,
            start_state: 0,
        };
        (net1.linecount + net2.linecount + net1.finalcount + 2)
            as usize
    ];
    let mut current_final = -1;
    /* Copy fsm1, fsm2 after each other, adding appropriate epsilon arcs */
    let mut j: i32 = 0;
    let mut i = 0usize;
    while net1.states[i].state_no != -1 {
        if net1.states[i].final_state == 1 && net1.states[i].state_no != current_final {
            let (state_no, start_state) = (net1.states[i].state_no, net1.states[i].start_state);
            add_fsm_arc(
                &mut new_fsm,
                j,
                state_no,
                EPSILON,
                EPSILON,
                net1.statecount,
                0,
                start_state as i32,
            );
            current_final = net1.states[i].state_no;
            j += 1;
        }
        if !(net1.states[i].target == -1 && net1.states[i].final_state == 1) {
            let (state_no, line_in, line_out, target, start_state) = (
                net1.states[i].state_no,
                net1.states[i].r#in as i32,
                net1.states[i].out as i32,
                net1.states[i].target,
                net1.states[i].start_state as i32,
            );
            add_fsm_arc(
                &mut new_fsm,
                j,
                state_no,
                line_in,
                line_out,
                target,
                0,
                start_state,
            );
            j += 1;
        }
        i += 1;
    }

    let mut i = 0usize;
    while net2.states[i].state_no != -1 {
        let (state_no, line_in, line_out, target, final_state) = (
            net2.states[i].state_no,
            net2.states[i].r#in as i32,
            net2.states[i].out as i32,
            net2.states[i].target,
            net2.states[i].final_state as i32,
        );
        add_fsm_arc(
            &mut new_fsm,
            j,
            state_no,
            line_in,
            line_out,
            target,
            final_state,
            0,
        );
        i += 1;
        j += 1;
    }
    add_fsm_arc(&mut new_fsm, j, -1, -1, -1, -1, -1, -1);
    /* free(net1->states) */
    fsm_destroy(net2);
    net1.states = new_fsm;
    if sigma_find_number(EPSILON, &net1.sigma).is_none() {
        sigma_add_special(EPSILON, &mut net1.sigma);
    }
    fsm_count(&mut net1);
    net1.is_epsilon_free = NO;
    net1.is_deterministic = NO;
    net1.is_minimized = NO;
    net1.is_pruned = NO;
    fsm_minimize(opts, net1)
}

// [spec:foma:def:constructions.fsm-union-fn]
// [spec:foma:sem:constructions.fsm-union-fn]
// [spec:foma:def:fomalib.fsm-union-fn]
// [spec:foma:sem:fomalib.fsm-union-fn]
pub fn fsm_union(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    let mut net1 = net1;
    let mut net2 = net2;

    fsm_merge_sigma(opts, &mut net1, &mut net2);

    fsm_count(&mut net1);
    fsm_count(&mut net2);

    let net1_offset = 1;
    let net2_offset = net1.statecount + 1;
    /* C: malloc'd (uninitialized); zeroed lines here */
    let mut new_fsm: Vec<FsmState> = vec![
        FsmState {
            state_no: 0,
            r#in: 0,
            out: 0,
            target: 0,
            final_state: 0,
            start_state: 0,
        };
        (net1.linecount + net2.linecount + 2) as usize
    ];

    let mut j: i32 = 0;

    add_fsm_arc(&mut new_fsm, j, 0, EPSILON, EPSILON, net1_offset, 0, 1);
    j += 1;
    add_fsm_arc(&mut new_fsm, j, 0, EPSILON, EPSILON, net2_offset, 0, 1);
    j += 1;
    let mut arccount = 2;
    let mut i = 0usize;
    while net1.states[i].state_no != -1 {
        let new_target = if net1.states[i].target == -1 {
            -1
        } else {
            net1.states[i].target + net1_offset
        };
        let (state_no, line_in, line_out, final_state) = (
            net1.states[i].state_no,
            net1.states[i].r#in as i32,
            net1.states[i].out as i32,
            net1.states[i].final_state as i32,
        );
        add_fsm_arc(
            &mut new_fsm,
            j,
            state_no + net1_offset,
            line_in,
            line_out,
            new_target,
            final_state,
            0,
        );
        j += 1;
        if new_target != -1 {
            arccount += 1;
        }
        i += 1;
    }
    let mut i = 0usize;
    while net2.states[i].state_no != -1 {
        let new_target = if net2.states[i].target == -1 {
            -1
        } else {
            net2.states[i].target + net2_offset
        };
        let (state_no, line_in, line_out, final_state) = (
            net2.states[i].state_no,
            net2.states[i].r#in as i32,
            net2.states[i].out as i32,
            net2.states[i].final_state as i32,
        );
        add_fsm_arc(
            &mut new_fsm,
            j,
            state_no + net2_offset,
            line_in,
            line_out,
            new_target,
            final_state,
            0,
        );
        j += 1;
        if new_target != -1 {
            arccount += 1;
        }
        i += 1;
    }
    add_fsm_arc(&mut new_fsm, j, -1, -1, -1, -1, -1, -1);
    j += 1;
    /* free(net1->states) */
    net1.states = new_fsm;
    net1.statecount = net1.statecount + net2.statecount + 1;
    net1.linecount = j;
    net1.arccount = arccount;
    net1.finalcount += net2.finalcount;
    fsm_destroy(net2);
    fsm_update_flags(&mut net1, NO, NO, NO, NO, UNK, NO);
    if sigma_find_number(EPSILON, &net1.sigma).is_none() {
        sigma_add_special(EPSILON, &mut net1.sigma);
    }
    net1
}

// [spec:foma:def:constructions.fsm-completes-fn]
// [spec:foma:sem:constructions.fsm-completes-fn]
pub fn fsm_completes(opts: &FomaOptions, net: Box<Fsm>, operation: i32) -> Box<Fsm> {
    /* TODO: this currently relies on that the sigma is gap-free in its numbering  */
    /* which can't always be counted on, especially when reading external machines */

    /* TODO: check arity */

    let mut net = net;
    if net.is_minimized != YES {
        net = fsm_minimize(opts, net);
    }

    let mut incomplete = 0;
    if sigma_find_number(UNKNOWN, &net.sigma).is_some() {
        /* C: sigma_remove's returned new head is discarded (harmless
        unless UNKNOWN were the head node); the owned list here must be
        reassigned */
        sigma_remove("@_UNKNOWN_SYMBOL_@", &mut net.sigma);
    }
    if sigma_find_number(IDENTITY, &net.sigma).is_none() {
        sigma_add_special(IDENTITY, &mut net.sigma);
        incomplete = 1;
    }

    let mut sigsize = sigma_size(&net.sigma);
    let last_sigma = sigma_max(&net.sigma);

    if sigma_find_number(EPSILON, &net.sigma).is_some() {
        sigsize -= 1;
    }

    fsm_count(&mut net);
    let mut statecount = net.statecount;
    /* C: malloc'd short arrays (+1 for sink state; the spare entry is
    uninitialized in C, zeroed here) */
    let mut starts: Vec<i16> = vec![0; (statecount + 1) as usize];
    let mut finals: Vec<i16> = vec![0; (statecount + 1) as usize];
    let mut sinks: Vec<i16> = vec![0; (statecount + 1) as usize];

    /* Init starts, finals, sinks arrays */

    for i in 0..statecount {
        sinks[i as usize] = 1;
        finals[i as usize] = 0;
        starts[i as usize] = 0;
    }
    let mut arccount = 0;
    let mut i = 0usize;
    while net.states[i].state_no != -1 {
        if operation == COMPLEMENT {
            if net.states[i].final_state == 1 {
                net.states[i].final_state = 0;
            } else if net.states[i].final_state == 0 {
                net.states[i].final_state = 1;
            }
        }
        if net.states[i].target != -1 {
            arccount += 1;
        }
        starts[net.states[i].state_no as usize] = net.states[i].start_state as i16;
        finals[net.states[i].state_no as usize] = net.states[i].final_state as i16;
        if net.states[i].final_state != 0 && operation != COMPLEMENT {
            sinks[net.states[i].state_no as usize] = 0;
        }
        if net.states[i].final_state == 0 && operation == COMPLEMENT {
            sinks[net.states[i].state_no as usize] = 0;
        }
        if net.states[i].target != -1 && net.states[i].state_no != net.states[i].target {
            sinks[net.states[i].state_no as usize] = 0;
        }
        i += 1;
    }

    net.is_loop_free = NO;
    net.pathcount = PATHCOUNT_CYCLIC;

    if incomplete == 0 && arccount == sigsize * statecount {
        /*    printf("Already complete!\n"); */

        /*     if (operation == COMPLEMENT) { */
        /*       for (i=0; (fsm+i)->state_no != -1; i++) { */
        /* 	if ((fsm+i)->final_state) { */
        /* 	  (fsm+i)->final_state = 0; */
        /* 	} else { */
        /* 	  (fsm+i)->final_state = 1; */
        /* 	} */
        /*       } */
        /*     } */
        drop(starts);
        drop(finals);
        drop(sinks);
        net.is_completed = YES;
        net.is_minimized = YES;
        net.is_pruned = NO;
        net.is_deterministic = YES;
        return net;
    }

    /* Find an existing sink state, or invent a new one */

    let mut sink_state = -1;
    for i in 0..statecount {
        if sinks[i as usize] == 1 {
            sink_state = i;
            break;
        }
    }

    if sink_state == -1 {
        sink_state = statecount;
        starts[sink_state as usize] = 0;
        if operation == COMPLEMENT {
            finals[sink_state as usize] = 1;
        } else {
            finals[sink_state as usize] = 0;
        }
        statecount += 1;
    }

    /* We can build a state table without memory problems since the size */
    /* of the completed machine will be |Sigma| * |States| in all cases */

    sigsize += 2;

    /* C: malloc'd (uninitialized); initialized to -1 just below */
    let mut state_table: Vec<i32> = vec![0; (sigsize * statecount) as usize];

    /* Init state table */
    /* i = state #, j = sigma # */
    for i in 0..statecount {
        for j in 0..sigsize {
            state_table[(i * sigsize + j) as usize] = -1;
        }
    }

    let mut i = 0usize;
    while net.states[i].state_no != -1 {
        if net.states[i].target != -1 {
            state_table[(net.states[i].state_no * sigsize + net.states[i].r#in as i32) as usize] =
                net.states[i].target;
        }
        i += 1;
    }
    /* Add looping arcs from and to sink state */
    for j in 2..=last_sigma {
        state_table[(sink_state * sigsize + j) as usize] = sink_state;
    }
    /* Add missing arcs to sink state from all states */
    for i in 0..statecount {
        for j in 2..=last_sigma {
            if state_table[(i * sigsize + j) as usize] == -1 {
                state_table[(i * sigsize + j) as usize] = sink_state;
            }
        }
    }

    /* C: malloc'd (uninitialized); zeroed lines here */
    let mut new_fsm: Vec<FsmState> = vec![
        FsmState {
            state_no: 0,
            r#in: 0,
            out: 0,
            target: 0,
            final_state: 0,
            start_state: 0,
        };
        (sigsize * statecount + 1) as usize
    ];

    /* Complement requires toggling final, nonfinal states */
    /*   if (operation == COMPLEMENT) */
    /*     for (i=0; i < statecount; i++) */
    /*       *(finals+i) = *(finals+i) == 0 ? 1 : 0; */

    let mut offset: i32 = 0;
    for i in 0..statecount {
        for j in 2..=last_sigma {
            let target = if state_table[(i * sigsize + j) as usize] == -1 {
                sink_state
            } else {
                state_table[(i * sigsize + j) as usize]
            };
            add_fsm_arc(
                &mut new_fsm,
                offset,
                i,
                j,
                j,
                target,
                finals[i as usize] as i32,
                starts[i as usize] as i32,
            );
            offset += 1;
        }
    }
    add_fsm_arc(&mut new_fsm, offset, -1, -1, -1, -1, -1, -1);
    /* offset++ — the C bumps the counter one final time (unused) */
    /* free(net->states) */
    net.states = new_fsm;
    /* free(starts); free(finals); free(sinks); free(state_table) */
    drop(starts);
    drop(finals);
    drop(sinks);
    drop(state_table);
    net.is_minimized = NO;
    net.is_pruned = NO;
    net.is_completed = YES;
    net.statecount = statecount;
    net
}

// [spec:foma:def:constructions.fsm-complete-fn]
// [spec:foma:sem:constructions.fsm-complete-fn]
// [spec:foma:def:fomalib.fsm-complete-fn]
// [spec:foma:sem:fomalib.fsm-complete-fn]
pub fn fsm_complete(opts: &FomaOptions, net: Box<Fsm>) -> Box<Fsm> {
    fsm_completes(opts, net, COMPLETE)
}

// [spec:foma:def:constructions.fsm-complement-fn]
// [spec:foma:sem:constructions.fsm-complement-fn]
// [spec:foma:def:fomalib.fsm-complement-fn]
// [spec:foma:sem:fomalib.fsm-complement-fn]
pub fn fsm_complement(opts: &FomaOptions, net: Box<Fsm>) -> Box<Fsm> {
    fsm_completes(opts, net, COMPLEMENT)
}

// [spec:foma:def:constructions.fsm-minus-fn]
// [spec:foma:sem:constructions.fsm-minus-fn]
// [spec:foma:def:fomalib.fsm-minus-fn]
// [spec:foma:sem:fomalib.fsm-minus-fn]
pub fn fsm_minus(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    let mut int_stack = IntStack::new();
    let mut statecount = 0;

    let mut net1 = fsm_minimize(opts, net1);
    let mut net2 = fsm_minimize(opts, net2);

    fsm_merge_sigma(opts, &mut net1, &mut net2);

    fsm_count(&mut net1);
    fsm_count(&mut net2);

    /* new state 0 = {1,1} */

    int_stack.clear();
    /* STACK_2_PUSH(1,1) */
    int_stack.push(1);
    int_stack.push(1);

    let mut th = triplet_hash_init();
    triplet_hash_insert(&mut th, 1, 1, 0);

    let point_a = init_state_pointers(&net1.states);
    let point_b = init_state_pointers(&net2.states);

    let mut builder = fsm_state_init(sigma_max(&net1.sigma));

    while !int_stack.is_empty() {
        statecount += 1;
        /* Get a pair of states to examine */

        let mut a = int_stack.pop();
        let mut b = int_stack.pop();

        let current_state = triplet_hash_find(&th, a, b, 0)
            .expect("state pair popped off the work stack was inserted into the triplet hash");
        a -= 1;
        b -= 1;

        let (current_start, current_final);
        if b == -1 {
            current_start = 0;
            current_final = point_a[a as usize].r#final;
        } else {
            current_start = if a == 0 && b == 0 { 1 } else { 0 };
            current_final = if point_a[a as usize].r#final == 1 && point_b[b as usize].r#final == 0
            {
                1
            } else {
                0
            };
        }

        fsm_state_set_current_state(&mut builder, current_state, current_final, current_start);

        let mut ai = point_a[a as usize].transitions;
        while net1.states[ai].state_no == a {
            if net1.states[ai].target == -1 {
                break;
            }
            let target_number;
            if b == -1 {
                /* b is dead */
                let atarget = net1.states[ai].target;
                target_number = match triplet_hash_find(&th, atarget + 1, 0, 0) {
                    Some(found) => found,
                    None => {
                        /* STACK_2_PUSH(0, (machine_a->target)+1) */
                        int_stack.push(0);
                        int_stack.push(atarget + 1);
                        triplet_hash_insert(&mut th, atarget + 1, 0, 0)
                    }
                };
            } else {
                /* b is alive */
                let mut b_has_trans = 0;
                let mut btarget = 0;
                let mut bi = point_b[b as usize].transitions;
                while net2.states[bi].state_no == b {
                    if net1.states[ai].r#in == net2.states[bi].r#in
                        && net1.states[ai].out == net2.states[bi].out
                    {
                        b_has_trans = 1;
                        btarget = net2.states[bi].target;
                        break;
                    }
                    bi += 1;
                }
                if b_has_trans != 0 {
                    let atarget = net1.states[ai].target;
                    target_number = match triplet_hash_find(&th, atarget + 1, btarget + 1, 0) {
                        Some(found) => found,
                        None => {
                            /* STACK_2_PUSH(btarget+1, (machine_a->target)+1) */
                            int_stack.push(btarget + 1);
                            int_stack.push(atarget + 1);
                            /* C inserts (machine_b->target)+1, which equals
                            btarget+1 (the scan broke at the matching line) */
                            let mbtarget = net2.states[bi].target;
                            triplet_hash_insert(&mut th, atarget + 1, mbtarget + 1, 0)
                        }
                    };
                } else {
                    /* b is dead */
                    let atarget = net1.states[ai].target;
                    target_number = match triplet_hash_find(&th, atarget + 1, 0, 0) {
                        Some(found) => found,
                        None => {
                            /* STACK_2_PUSH(0, (machine_a->target)+1) */
                            int_stack.push(0);
                            int_stack.push(atarget + 1);
                            triplet_hash_insert(&mut th, atarget + 1, 0, 0)
                        }
                    };
                }
            }
            let (line_in, line_out) = (net1.states[ai].r#in as i32, net1.states[ai].out as i32);
            fsm_state_add_arc(
                &mut builder,
                current_state,
                line_in,
                line_out,
                target_number,
                current_final,
                current_start,
            );
            ai += 1;
        }
        fsm_state_end_state(&mut builder);
    }

    let _ = statecount;
    /* free(net1->states) */
    net1.states = Vec::new();
    fsm_state_close(&mut builder, &mut net1);
    /* free(point_a); free(point_b) */
    drop(point_a);
    drop(point_b);
    fsm_destroy(net2);
    triplet_hash_free(Some(th));
    fsm_minimize(opts, net1)
}

// [spec:foma:def:constructions.fsm-concat-m-n-fn]
// [spec:foma:sem:constructions.fsm-concat-m-n-fn]
// [spec:foma:def:fomalib.fsm-concat-m-n-fn]
// [spec:foma:sem:fomalib.fsm-concat-m-n-fn]
pub fn fsm_concat_m_n(opts: &FomaOptions, net1: Box<Fsm>, m: i32, n: i32) -> Box<Fsm> {
    let mut net1 = net1;
    let mut acc = fsm_empty_string();
    let mut i = 1;
    while i <= n {
        if i > m {
            acc = fsm_concat(opts, acc, fsm_optionality(opts, fsm_copy(&mut net1)));
        } else {
            acc = fsm_concat(opts, acc, fsm_copy(&mut net1));
        }
        i += 1;
    }
    fsm_destroy(net1);
    acc
}

// [spec:foma:def:constructions.fsm-concat-n-fn]
// [spec:foma:sem:constructions.fsm-concat-n-fn]
// [spec:foma:def:fomalib.fsm-concat-n-fn]
// [spec:foma:sem:fomalib.fsm-concat-n-fn]
pub fn fsm_concat_n(opts: &FomaOptions, net1: Box<Fsm>, n: i32) -> Box<Fsm> {
    fsm_concat_m_n(opts, net1, n, n)
}

// [spec:foma:def:constructions.fsm-term-negation-fn]
// [spec:foma:sem:constructions.fsm-term-negation-fn]
// [spec:foma:def:fomalib.fsm-term-negation-fn]
// [spec:foma:sem:fomalib.fsm-term-negation-fn]
pub fn fsm_term_negation(opts: &FomaOptions, net1: Box<Fsm>) -> Box<Fsm> {
    fsm_intersect(opts, fsm_identity(), fsm_complement(opts, net1))
}

// [spec:foma:def:constructions.fsm-invert-fn]
// [spec:foma:sem:constructions.fsm-invert-fn]
// [spec:foma:def:fomalib.fsm-invert-fn]
// [spec:foma:sem:fomalib.fsm-invert-fn]
pub fn fsm_invert(net: Box<Fsm>) -> Box<Fsm> {
    let mut net = net;
    let mut i = 0usize;
    while net.states[i].state_no != -1 {
        let s = &mut net.states[i];
        std::mem::swap(&mut s.r#in, &mut s.out);
        i += 1;
    }
    std::mem::swap(&mut net.arcs_sorted_in, &mut net.arcs_sorted_out);
    net
}
