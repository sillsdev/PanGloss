//! Wave-4 split of constructions.c (see mod.rs). Cross-module and
//! external names come via `use super::*` (re-exported by mod.rs).
use super::*;
use smol_str::SmolStr;

// [spec:foma:def:constructions.fsm-escape-fn]
// [spec:foma:sem:constructions.fsm-escape-fn]
// [spec:foma:def:fomalib.fsm-escape-fn]
// [spec:foma:sem:fomalib.fsm-escape-fn]
pub fn fsm_escape(symbol: &str) -> Box<Fsm> {
    /* C: fsm_symbol(symbol+1) — skip the first byte (the escape char) */
    fsm_symbol(&symbol[1..])
}

/* Convert a multicharacter-string-containing machine */
/* to the equivalent "letter" machine where all arcs  */
/* are single utf8 letters.                           */

// [spec:foma:def:constructions.fsm-letter-machine-fn]
// [spec:foma:sem:constructions.fsm-letter-machine-fn+1]
// [spec:foma:def:fomalib.fsm-letter-machine-fn]
// [spec:foma:sem:fomalib.fsm-letter-machine-fn+1]
pub fn fsm_letter_machine(opts: &FomaOptions, net: Box<Fsm>) -> Box<Fsm> {
    // DEVIATION from C (discarded minimize return; C reads net->statecount
    // through the original pointer after fsm_minimize and dangles under
    // Brzozowski — bind the returned Box and continue with it)
    let net = fsm_minimize(opts, net);
    let mut addstate = net.statecount;
    let mut inh = fsm_read_init(net);
    let mut outh = fsm_construct_init("name");

    while fsm_get_next_arc(&mut inh) != 0 {
        let in_full = fsm_get_arc_in(&inh)
            .expect("arc label present on the positioned cursor")
            .to_string();
        let out_full = fsm_get_arc_out(&inh)
            .expect("arc label present on the positioned cursor")
            .to_string();
        let innum = fsm_get_arc_num_in(&inh);
        let outnum = fsm_get_arc_num_out(&inh);
        let mut source = fsm_get_arc_source(&inh);
        let mut target = fsm_get_arc_target(&inh);

        if (innum > IDENTITY && in_full.chars().count() > 1)
            || (outnum > IDENTITY && out_full.chars().count() > 1)
        {
            let inlen = if innum <= IDENTITY {
                1
            } else {
                in_full.chars().count() as i32
            };
            let outlen = if outnum <= IDENTITY {
                1
            } else {
                out_full.chars().count() as i32
            };
            let steps = inlen.max(outlen);

            /* Split the multi-character label into a chain of single-character
            arcs, pulling one character per step from each side (C walked the
            label bytes with utf8skip; a &str yields characters directly). A
            special side (<= IDENTITY) repeats its whole label at every step; a
            normal side that runs out of characters pads with epsilon. */
            let mut in_chars = in_full.chars();
            let mut out_chars = out_full.chars();
            let mut inbuf = [0u8; 4];
            let mut outbuf = [0u8; 4];

            target = addstate;
            for i in 0..steps {
                let currin: &str = if innum <= IDENTITY {
                    &in_full
                } else if let Some(c) = in_chars.next() {
                    c.encode_utf8(&mut inbuf)
                } else {
                    "@_EPSILON_SYMBOL_@"
                };
                let currout: &str = if outnum <= IDENTITY {
                    &out_full
                } else if let Some(c) = out_chars.next() {
                    c.encode_utf8(&mut outbuf)
                } else {
                    "@_EPSILON_SYMBOL_@"
                };
                if i == 0 && steps > 1 {
                    target = addstate;
                    addstate += 1;
                }
                if i > 0 && (steps - i == 1) {
                    source = addstate - 1;
                    target = fsm_get_arc_target(&inh);
                }
                if i > 0 && (steps - i != 1) {
                    source = addstate - 1;
                    target = addstate;
                    addstate += 1;
                }
                fsm_construct_add_arc(&mut outh, source, target, currin, currout);
            }
        } else {
            fsm_construct_add_arc(&mut outh, source, target, &in_full, &out_full);
        }
    }
    for i in inh.finals() {
        fsm_construct_set_final(&mut outh, i);
    }
    for i in inh.initials() {
        fsm_construct_set_initial(&mut outh, i);
    }
    drop(fsm_read_done(inh));
    fsm_construct_done(outh)
}

// [spec:foma:def:constructions.fsm-explode-fn]
// [spec:foma:sem:constructions.fsm-explode-fn]
// [spec:foma:def:fomalib.fsm-explode-fn]
// [spec:foma:sem:fomalib.fsm-explode-fn]
pub fn fsm_explode(symbol: &str) -> Box<Fsm> {
    let mut h = fsm_construct_init("");
    fsm_construct_set_initial(&mut h, 0);

    /* one identity arc per character (`symbol` is the bare content; C received
    the brace-enclosed form and skipped the two delimiter bytes itself) */
    let mut j: i32 = 1;
    let mut buf = [0u8; 4];
    for ch in symbol.chars() {
        let sym = ch.encode_utf8(&mut buf);
        fsm_construct_add_arc(&mut h, j - 1, j, sym, sym);
        j += 1;
    }
    fsm_construct_set_final(&mut h, j - 1);
    fsm_construct_done(h)
}

// [spec:foma:def:constructions.fsm-symbol-fn]
// [spec:foma:sem:constructions.fsm-symbol-fn]
// [spec:foma:def:fomalib.fsm-symbol-fn]
// [spec:foma:sem:fomalib.fsm-symbol-fn]
pub fn fsm_symbol(symbol: &str) -> Box<Fsm> {
    let mut net = fsm_create("");
    fsm_update_flags(&mut net, YES, YES, YES, YES, YES, NO);
    if symbol == "@_EPSILON_SYMBOL_@" {
        /* Epsilon */
        sigma_add_special(EPSILON, &mut net.sigma);
        /* C: malloc(2 lines), uninitialized; written by add_fsm_arc below */
        net.states = vec![
            FsmState {
                state_no: 0,
                r#in: 0,
                out: 0,
                target: 0,
                final_state: 0,
                start_state: 0,
            };
            2
        ];
        add_fsm_arc(&mut net.states, 0, 0, -1, -1, -1, 1, 1);
        add_fsm_arc(&mut net.states, 1, -1, -1, -1, -1, -1, -1);
        net.arccount = 0;
        net.statecount = 1;
        net.linecount = 1;
        net.finalcount = 1;
        net.is_deterministic = NO;
        net.is_minimized = NO;
        net.is_epsilon_free = NO;
    } else {
        let symbol_no = if symbol == "@_IDENTITY_SYMBOL_@" {
            sigma_add_special(IDENTITY, &mut net.sigma)
        } else {
            sigma_add(symbol, &mut net.sigma)
        };
        /* C: malloc(3 lines), uninitialized; written by add_fsm_arc below */
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
        add_fsm_arc(&mut net.states, 0, 0, symbol_no, symbol_no, 1, 0, 1);
        add_fsm_arc(&mut net.states, 1, 1, -1, -1, -1, 1, 0);
        add_fsm_arc(&mut net.states, 2, -1, -1, -1, -1, -1, -1);
        net.arity = 1;
        net.pathcount = 1;
        net.arccount = 1;
        net.statecount = 2;
        net.linecount = 2;
        net.finalcount = 1;
        net.arcs_sorted_in = YES;
        net.arcs_sorted_out = YES;
        net.is_deterministic = YES;
        net.is_minimized = YES;
        net.is_epsilon_free = YES;
    }
    net
}

// [spec:foma:def:constructions.fsm-network-to-char-fn]
// [spec:foma:sem:constructions.fsm-network-to-char-fn]
// [spec:foma:def:fomalib.fsm-network-to-char-fn]
// [spec:foma:sem:fomalib.fsm-network-to-char-fn]
pub fn fsm_network_to_char(net: &Fsm) -> Option<SmolStr> {
    /* an empty alphabet has no last symbol; otherwise strdup(last->symbol) */
    net.sigma.last().map(|s| s.symbol.clone())
}

// [spec:foma:def:constructions.fsm-substitute-label-fn]
// [spec:foma:sem:constructions.fsm-substitute-label-fn]
// [spec:foma:def:fomalib.fsm-substitute-label-fn]
// [spec:foma:sem:fomalib.fsm-substitute-label-fn]
pub fn fsm_substitute_label(
    opts: &FomaOptions,
    net: &mut Fsm,
    original: &str,
    substitute: &mut Fsm,
) -> Box<Fsm> {
    fsm_merge_sigma(opts, net, substitute);
    let mut addstate1 = net.statecount;
    let addstate2 = substitute.statecount;

    /* C: the read handles borrow net and substitute (NEITHER is consumed
    on any path); the Rust handles own deep copies — read-only, observably
    equivalent */
    let mut inh = fsm_read_init(Box::new(net.clone()));
    let mut subh = fsm_read_init(Box::new(substitute.clone()));
    let repsym = fsm_get_symbol_number(&inh, original);
    if repsym == -1 {
        let _ = fsm_read_done(inh);
        // DEVIATION from C (C returns the input net aliased; a deep copy here)
        return Box::new(net.clone());
    }
    let name = net.name.clone();
    let mut outh = fsm_construct_init(&name);
    fsm_construct_copy_sigma(&mut outh, &net.sigma);
    while fsm_get_next_arc(&mut inh) != 0 {
        let mut source = fsm_get_arc_source(&inh);
        let mut target = fsm_get_arc_target(&inh);
        let r#in = fsm_get_arc_num_in(&inh);
        let out = fsm_get_arc_num_out(&inh);

        /* Double-sided arc, splice in substitute network */
        if r#in == repsym && out == repsym {
            fsm_read_reset(Some(&mut subh));
            fsm_construct_add_arc_nums(&mut outh, source, addstate1, EPSILON, EPSILON);
            while fsm_get_next_arc(&mut subh) != 0 {
                source = fsm_get_arc_source(&subh);
                target = fsm_get_arc_target(&subh);
                let subin = fsm_get_arc_in(&subh)
                    .expect("arc label present on the positioned cursor")
                    .to_string();
                let subout = fsm_get_arc_out(&subh)
                    .expect("arc label present on the positioned cursor")
                    .to_string();
                fsm_construct_add_arc(
                    &mut outh,
                    source + addstate1,
                    target + addstate1,
                    &subin,
                    &subout,
                );
            }
            loop {
                let i = fsm_get_next_final(&mut subh);
                if i == -1 {
                    break;
                }
                target = fsm_get_arc_target(&inh);
                fsm_construct_add_arc_nums(&mut outh, addstate1 + i, target, EPSILON, EPSILON);
            }
            addstate1 += addstate2;
            /* One-sided replace, splice in repsym .x. sub or sub .x. repsym */
        } else if r#in == repsym || out == repsym {
            let subnet2 = if r#in == repsym {
                let outlabel = fsm_get_arc_out(&inh)
                    .expect("arc label present on the positioned cursor")
                    .to_string();
                fsm_minimize(
                    opts,
                    fsm_cross_product(opts, fsm_copy(substitute), fsm_symbol(&outlabel)),
                )
            } else {
                let inlabel = fsm_get_arc_in(&inh)
                    .expect("arc label present on the positioned cursor")
                    .to_string();
                fsm_minimize(
                    opts,
                    fsm_cross_product(opts, fsm_symbol(&inlabel), fsm_copy(substitute)),
                )
            };
            fsm_construct_add_arc_nums(&mut outh, source, addstate1, EPSILON, EPSILON);
            let mut subh2 = fsm_read_init(subnet2);
            while fsm_get_next_arc(&mut subh2) != 0 {
                source = fsm_get_arc_source(&subh2);
                target = fsm_get_arc_target(&subh2);
                let subin = fsm_get_arc_in(&subh2)
                    .expect("arc label present on the positioned cursor")
                    .to_string();
                let subout = fsm_get_arc_out(&subh2)
                    .expect("arc label present on the positioned cursor")
                    .to_string();
                fsm_construct_add_arc(
                    &mut outh,
                    source + addstate1,
                    target + addstate1,
                    &subin,
                    &subout,
                );
            }
            loop {
                let i = fsm_get_next_final(&mut subh2);
                if i == -1 {
                    break;
                }
                target = fsm_get_arc_target(&inh);
                fsm_construct_add_arc_nums(&mut outh, addstate1 + i, target, EPSILON, EPSILON);
            }
            let subnet2 = fsm_read_done(subh2);
            addstate1 += subnet2.statecount;
            fsm_destroy(subnet2);
        } else {
            /* Default, just copy arc */
            fsm_construct_add_arc_nums(&mut outh, source, target, r#in, out);
        }
    }

    for i in inh.finals() {
        fsm_construct_set_final(&mut outh, i);
    }
    for i in inh.initials() {
        fsm_construct_set_initial(&mut outh, i);
    }
    let _ = fsm_read_done(inh);
    let _ = fsm_read_done(subh);
    fsm_construct_done(outh)
}

// [spec:foma:def:constructions.fsm-substitute-symbol-fn]
// [spec:foma:sem:constructions.fsm-substitute-symbol-fn]
// [spec:foma:def:fomalib.fsm-substitute-symbol-fn]
// [spec:foma:sem:fomalib.fsm-substitute-symbol-fn]
pub fn fsm_substitute_symbol(net: Box<Fsm>, original: &str, substitute: &str) -> Box<Fsm> {
    let mut net = net;
    if original == substitute {
        return net;
    }
    let o = match sigma_find(original, &net.sigma) {
        Some(o) => o,
        None => {
            //fprintf(stderr, "\nSymbol '%s' not found in network!\n", original);
            return net;
        }
    };
    let s: i32 = if substitute == "0" {
        EPSILON
    } else {
        /* C: substitute != NULL && (s = sigma_find(...)) == -1 → sigma_add
        (substitute is never NULL here) */
        match sigma_find(substitute, &net.sigma) {
            Some(found) => found,
            None => sigma_add(substitute, &mut net.sigma),
        }
    };
    let mut i = 0usize;
    while net.states[i].state_no != -1 {
        if net.states[i].r#in as i32 == o {
            net.states[i].r#in = s as i16;
        }
        if net.states[i].out as i32 == o {
            net.states[i].out = s as i16;
        }
        i += 1;
    }
    sigma_remove(original, &mut net.sigma);
    sigma_sort(&mut net);
    fsm_update_flags(&mut net, NO, NO, NO, NO, NO, NO);
    sigma_cleanup(&mut net, 0);
    /* if s = epsilon */
    net.is_minimized = NO;
    fsm_determinize(net)
}

// [spec:foma:def:constructions.fsm-precedes-fn]
// [spec:foma:sem:constructions.fsm-precedes-fn]
// [spec:foma:def:fomalib.fsm-precedes-fn]
// [spec:foma:sem:fomalib.fsm-precedes-fn]
pub fn fsm_precedes(opts: &FomaOptions, net1: &mut Fsm, net2: &mut Fsm) -> Box<Fsm> {
    /* Neither net1 nor net2 is consumed (copies only) */
    fsm_complement(
        opts,
        fsm_minimize(
            opts,
            fsm_contains(
                opts,
                fsm_minimize(
                    opts,
                    fsm_concat(
                        opts,
                        fsm_minimize(opts, fsm_copy(net2)),
                        fsm_concat(opts, fsm_universal(), fsm_minimize(opts, fsm_copy(net1))),
                    ),
                ),
            ),
        ),
    )
}

// [spec:foma:def:constructions.fsm-follows-fn]
// [spec:foma:sem:constructions.fsm-follows-fn]
// [spec:foma:def:fomalib.fsm-follows-fn]
// [spec:foma:sem:fomalib.fsm-follows-fn]
pub fn fsm_follows(opts: &FomaOptions, net1: &mut Fsm, net2: &mut Fsm) -> Box<Fsm> {
    /* Neither net1 nor net2 is consumed (copies only) */
    fsm_complement(
        opts,
        fsm_minimize(
            opts,
            fsm_contains(
                opts,
                fsm_minimize(
                    opts,
                    fsm_concat(
                        opts,
                        fsm_minimize(opts, fsm_copy(net1)),
                        fsm_concat(opts, fsm_universal(), fsm_minimize(opts, fsm_copy(net2))),
                    ),
                ),
            ),
        ),
    )
}

// [spec:foma:def:constructions.fsm-unflatten-fn]
// [spec:foma:sem:constructions.fsm-unflatten-fn]
// [spec:foma:def:fomalib.fsm-unflatten-fn]
// [spec:foma:sem:fomalib.fsm-unflatten-fn]
pub fn fsm_unflatten(
    opts: &FomaOptions,
    net: Box<Fsm>,
    epsilon_sym: &str,
    repeat_sym: &str,
) -> Box<Fsm> {
    let mut int_stack = IntStack::new();
    // DEVIATION from C (discarded minimize return; C dangles under Brzozowski)
    let mut net = fsm_minimize(opts, net);
    fsm_count(&mut net);

    /* -1 when the symbol is absent — a value no real symbol number ever takes,
    so the `in/out == epsilon/repeat` comparisons below simply never fire. */
    let epsilon = sigma_find(epsilon_sym, &net.sigma).unwrap_or(-1);
    let repeat = sigma_find(repeat_sym, &net.sigma).unwrap_or(-1);

    /* new state 0 = {0,0} */

    /* STACK_2_PUSH(0,0) */
    int_stack.push(0);
    int_stack.push(0);

    let mut th = triplet_hash_init();
    triplet_hash_insert(&mut th, 0, 0, 0);

    let mut builder = fsm_state_init(sigma_max(&net.sigma));

    let point_a = init_state_pointers(&net.states);

    while !int_stack.is_empty() {
        /* Get a pair of states to examine */

        /* C: both pops are assigned to a; the pair is always (s, s), so the
        first pop is discarded and the second is the state to examine. */
        let _ = int_stack.pop();
        let a = int_stack.pop();

        let current_state = triplet_hash_find(&th, a, a, 0)
            .expect("state popped off the work stack was inserted into the triplet hash");
        let current_start = if point_a[a as usize].start == 1 { 1 } else { 0 };
        let current_final = if point_a[a as usize].r#final == 1 {
            1
        } else {
            0
        };

        fsm_state_set_current_state(&mut builder, current_state, current_final, current_start);

        let mut ei = point_a[a as usize].transitions;
        while net.states[ei].state_no == a {
            if net.states[ei].target == -1 {
                ei += 1;
                continue;
            }
            let b = net.states[ei].target;
            let mut oi = point_a[b as usize].transitions;
            while net.states[oi].state_no == b {
                if net.states[oi].target == -1 {
                    oi += 1;
                    continue;
                }
                let odd_target = net.states[oi].target;
                let mut target_number = triplet_hash_find(&th, odd_target, odd_target, 0);
                if target_number.is_none() {
                    /* STACK_2_PUSH(odd_state->target, odd_state->target) */
                    int_stack.push(odd_target);
                    int_stack.push(odd_target);
                    target_number = Some(triplet_hash_insert(&mut th, odd_target, odd_target, 0));
                }
                let target_number = target_number.expect("found or just inserted");
                let mut r#in = net.states[ei].r#in as i32;
                let mut out = net.states[oi].r#in as i32;
                if out == repeat {
                    out = r#in;
                } else if r#in == IDENTITY || out == IDENTITY {
                    r#in = if r#in == IDENTITY { UNKNOWN } else { r#in };
                    out = if out == IDENTITY { UNKNOWN } else { out };
                }
                if r#in == epsilon {
                    r#in = EPSILON;
                }
                if out == epsilon {
                    out = EPSILON;
                }
                fsm_state_add_arc(
                    &mut builder,
                    current_state,
                    r#in,
                    out,
                    target_number,
                    current_final,
                    current_start,
                );
                oi += 1;
            }
            ei += 1;
        }
        fsm_state_end_state(&mut builder);
    }
    /* free(net->states) */
    net.states = Vec::new();
    fsm_state_close(&mut builder, &mut net);
    /* free(point_a) */
    drop(point_a);
    triplet_hash_free(Some(th));
    net
}

// [spec:foma:def:constructions.fsm-shuffle-fn]
// [spec:foma:sem:constructions.fsm-shuffle-fn]
// [spec:foma:def:fomalib.fsm-shuffle-fn]
// [spec:foma:sem:fomalib.fsm-shuffle-fn]
pub fn fsm_shuffle(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    let mut int_stack = IntStack::new();
    /* Shuffle A and B by making alternatively A move and B stay at each or */
    /* vice versa at each step */

    // DEVIATION from C (discarded minimize returns; C dangles under Brzozowski)
    let mut net1 = fsm_minimize(opts, net1);
    let mut net2 = fsm_minimize(opts, net2);

    fsm_merge_sigma(opts, &mut net1, &mut net2);

    fsm_count(&mut net1);
    fsm_count(&mut net2);

    /* new state 0 = {0,0} */

    /* STACK_2_PUSH(0,0) */
    int_stack.push(0);
    int_stack.push(0);

    let mut th = triplet_hash_init();
    triplet_hash_insert(&mut th, 0, 0, 0);

    let mut builder = fsm_state_init(sigma_max(&net1.sigma));

    let point_a = init_state_pointers(&net1.states);
    let point_b = init_state_pointers(&net2.states);

    while !int_stack.is_empty() {
        /* Get a pair of states to examine */

        let a = int_stack.pop();
        let b = int_stack.pop();

        /* printf("Treating pair: {%i,%i}\n",a,b); */

        let current_state = triplet_hash_find(&th, a, b, 0)
            .expect("state pair popped off the work stack was inserted into the triplet hash");
        let current_start = if point_a[a as usize].start == 1 && point_b[b as usize].start == 1 {
            1
        } else {
            0
        };
        let current_final = if point_a[a as usize].r#final == 1 && point_b[b as usize].r#final == 1
        {
            1
        } else {
            0
        };

        fsm_state_set_current_state(&mut builder, current_state, current_final, current_start);

        /* Follow A, B stays */
        let mut ai = point_a[a as usize].transitions;
        while net1.states[ai].state_no == a {
            if net1.states[ai].target == -1 {
                ai += 1;
                continue;
            }
            let atarget = net1.states[ai].target;
            let target_number = match triplet_hash_find(&th, atarget, b, 0) {
                Some(n) => n,
                None => {
                    /* STACK_2_PUSH(b, machine_a->target) */
                    int_stack.push(b);
                    int_stack.push(atarget);
                    triplet_hash_insert(&mut th, atarget, b, 0)
                }
            };
            let (ain, aout) = (net1.states[ai].r#in as i32, net1.states[ai].out as i32);
            fsm_state_add_arc(
                &mut builder,
                current_state,
                ain,
                aout,
                target_number,
                current_final,
                current_start,
            );
            ai += 1;
        }

        /* Follow B, A stays */
        let mut bi = point_b[b as usize].transitions;
        while net2.states[bi].state_no == b {
            if net2.states[bi].target == -1 {
                bi += 1;
                continue;
            }
            let btarget = net2.states[bi].target;
            let target_number = match triplet_hash_find(&th, a, btarget, 0) {
                Some(n) => n,
                None => {
                    /* STACK_2_PUSH(machine_b->target, a) */
                    int_stack.push(btarget);
                    int_stack.push(a);
                    triplet_hash_insert(&mut th, a, btarget, 0)
                }
            };
            let (bin, bout) = (net2.states[bi].r#in as i32, net2.states[bi].out as i32);
            fsm_state_add_arc(
                &mut builder,
                current_state,
                bin,
                bout,
                target_number,
                current_final,
                current_start,
            );
            bi += 1;
        }

        /* Check arctrack */
        fsm_state_end_state(&mut builder);
    }

    /* free(net1->states) */
    net1.states = Vec::new();
    fsm_state_close(&mut builder, &mut net1);
    /* free(point_a); free(point_b) */
    drop(point_a);
    drop(point_b);
    fsm_destroy(net2);
    triplet_hash_free(Some(th));
    net1
}

// [spec:foma:def:constructions.fsm-equivalent-fn]
// [spec:foma:sem:constructions.fsm-equivalent-fn]
// [spec:foma:def:fomalib.fsm-equivalent-fn]
// [spec:foma:sem:fomalib.fsm-equivalent-fn]
pub fn fsm_equivalent(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> bool {
    let mut int_stack = IntStack::new();
    /* Test path equivalence of two FSMs by traversing both in parallel */
    let mut net1 = net1;
    let mut net2 = net2;

    fsm_merge_sigma(opts, &mut net1, &mut net2);

    fsm_count(&mut net1);
    fsm_count(&mut net2);

    let mut equivalent = false;
    /* new state 0 = {0,0} */
    /* STACK_2_PUSH(0,0) */
    int_stack.push(0);
    int_stack.push(0);

    let mut th = triplet_hash_init();
    triplet_hash_insert(&mut th, 0, 0, 0);

    let point_a = init_state_pointers(&net1.states);
    let point_b = init_state_pointers(&net2.states);

    /* C: goto not_equivalent — labelled block with the same target */
    'not_equivalent: {
        while !int_stack.is_empty() {
            /* Get a pair of states to examine */

            let a = int_stack.pop();
            let b = int_stack.pop();

            if point_a[a as usize].r#final != point_b[b as usize].r#final {
                break 'not_equivalent;
            }
            /* Check that all arcs in A have matching arc in B, push new state pair on stack */
            let mut ai = point_a[a as usize].transitions;
            while net1.states[ai].state_no == a {
                if net1.states[ai].target == -1 {
                    break;
                }
                let mut matching_arc = 0;
                let mut bi = point_b[b as usize].transitions;
                while net2.states[bi].state_no == b {
                    if net2.states[bi].target == -1 {
                        break;
                    }
                    if net1.states[ai].r#in == net2.states[bi].r#in
                        && net1.states[ai].out == net2.states[bi].out
                    {
                        matching_arc = 1;
                        let (atarget, btarget) = (net1.states[ai].target, net2.states[bi].target);
                        if triplet_hash_find(&th, atarget, btarget, 0).is_none() {
                            /* STACK_2_PUSH(machine_b->target, machine_a->target) */
                            int_stack.push(btarget);
                            int_stack.push(atarget);
                            triplet_hash_insert(&mut th, atarget, btarget, 0);
                        }
                        break;
                    }
                    bi += 1;
                }
                if matching_arc == 0 {
                    break 'not_equivalent;
                }
                ai += 1;
            }
            let mut bi = point_b[b as usize].transitions;
            while net2.states[bi].state_no == b {
                if net2.states[bi].target == -1 {
                    break;
                }
                let mut matching_arc = 0;
                let mut ai = point_a[a as usize].transitions;
                while net1.states[ai].state_no == a {
                    if net1.states[ai].r#in == net2.states[bi].r#in
                        && net1.states[ai].out == net2.states[bi].out
                    {
                        matching_arc = 1;
                        break;
                    }
                    ai += 1;
                }
                if matching_arc == 0 {
                    break 'not_equivalent;
                }
                bi += 1;
            }
        }
        equivalent = true;
    }
    fsm_destroy(net1);
    fsm_destroy(net2);
    /* free(point_a); free(point_b) */
    drop(point_a);
    drop(point_b);
    triplet_hash_free(Some(th));
    equivalent
}

// [spec:foma:def:constructions.fsm-contains-fn]
// [spec:foma:sem:constructions.fsm-contains-fn]
// [spec:foma:def:fomalib.fsm-contains-fn]
// [spec:foma:sem:fomalib.fsm-contains-fn]
pub fn fsm_contains(opts: &FomaOptions, net: Box<Fsm>) -> Box<Fsm> {
    /* [?* A ?*] */
    fsm_concat(
        opts,
        fsm_concat(opts, fsm_universal(), net),
        fsm_universal(),
    )
}

// [spec:foma:def:constructions.fsm-universal-fn]
// [spec:foma:sem:constructions.fsm-universal-fn]
// [spec:foma:def:fomalib.fsm-universal-fn]
// [spec:foma:sem:fomalib.fsm-universal-fn]
pub fn fsm_universal() -> Box<Fsm> {
    let mut net = fsm_create("");
    fsm_update_flags(&mut net, YES, YES, YES, YES, NO, NO);
    /* C: malloc(2 lines), uninitialized; written by add_fsm_arc below */
    net.states = vec![
        FsmState {
            state_no: 0,
            r#in: 0,
            out: 0,
            target: 0,
            final_state: 0,
            start_state: 0,
        };
        2
    ];
    let s = sigma_add_special(IDENTITY, &mut net.sigma);
    add_fsm_arc(&mut net.states, 0, 0, s, s, 0, 1, 1);
    add_fsm_arc(&mut net.states, 1, -1, -1, -1, -1, -1, -1);
    net.arccount = 1;
    net.statecount = 1;
    net.linecount = 2;
    net.finalcount = 1;
    net.pathcount = PATHCOUNT_CYCLIC;
    net
}

// [spec:foma:def:constructions.fsm-contains-one-fn]
// [spec:foma:sem:constructions.fsm-contains-one-fn]
// [spec:foma:def:fomalib.fsm-contains-one-fn]
// [spec:foma:sem:fomalib.fsm-contains-one-fn]
pub fn fsm_contains_one(opts: &FomaOptions, net: Box<Fsm>) -> Box<Fsm> {
    /* $A - $[[?+ A ?* & A ?*] | [A ?+ & A]] */
    let mut net = net;
    let ret = fsm_minus(
        opts,
        fsm_contains(opts, fsm_copy(&mut net)),
        fsm_contains(
            opts,
            fsm_union(
                opts,
                fsm_intersect(
                    opts,
                    fsm_concat(
                        opts,
                        fsm_kleene_plus(opts, fsm_identity()),
                        fsm_concat(opts, fsm_copy(&mut net), fsm_universal()),
                    ),
                    fsm_concat(opts, fsm_copy(&mut net), fsm_universal()),
                ),
                fsm_intersect(
                    opts,
                    fsm_concat(
                        opts,
                        fsm_copy(&mut net),
                        fsm_kleene_plus(opts, fsm_identity()),
                    ),
                    fsm_copy(&mut net),
                ),
            ),
        ),
    );
    fsm_destroy(net);
    ret
}

// [spec:foma:def:constructions.fsm-contains-opt-one-fn]
// [spec:foma:sem:constructions.fsm-contains-opt-one-fn]
// [spec:foma:def:fomalib.fsm-contains-opt-one-fn]
// [spec:foma:sem:fomalib.fsm-contains-opt-one-fn]
pub fn fsm_contains_opt_one(opts: &FomaOptions, net: Box<Fsm>) -> Box<Fsm> {
    /* $.A | ~$A */
    let mut net = net;
    let ret = fsm_union(
        opts,
        fsm_contains_one(opts, fsm_copy(&mut net)),
        fsm_complement(opts, fsm_contains(opts, fsm_copy(&mut net))),
    );
    fsm_destroy(net);
    ret
}

// [spec:foma:def:constructions.fsm-simple-replace-fn]
// [spec:foma:sem:constructions.fsm-simple-replace-fn]
// [spec:foma:def:fomalib.fsm-simple-replace-fn]
// [spec:foma:sem:fomalib.fsm-simple-replace-fn]
pub fn fsm_simple_replace(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    /* [~[?* [A-0] ?*] [A.x.B]]* ~[?* [A-0] ?*] */

    let mut net1 = net1;
    let mut net2 = net2;
    let mut uplus = fsm_minimize(opts, fsm_kleene_plus(opts, fsm_identity()));
    let ret = fsm_concat(
        opts,
        fsm_minimize(
            opts,
            fsm_kleene_star(
                opts,
                fsm_minimize(
                    opts,
                    fsm_concat(
                        opts,
                        fsm_complement(
                            opts,
                            fsm_minimize(
                                opts,
                                fsm_concat(
                                    opts,
                                    fsm_concat(
                                        opts,
                                        fsm_universal(),
                                        fsm_minimize(
                                            opts,
                                            fsm_intersect(
                                                opts,
                                                fsm_copy(&mut net1),
                                                fsm_copy(&mut uplus),
                                            ),
                                        ),
                                    ),
                                    fsm_universal(),
                                ),
                            ),
                        ),
                        fsm_minimize(
                            opts,
                            fsm_cross_product(opts, fsm_copy(&mut net1), fsm_copy(&mut net2)),
                        ),
                    ),
                ),
            ),
        ),
        fsm_minimize(
            opts,
            fsm_complement(
                opts,
                fsm_minimize(
                    opts,
                    fsm_concat(
                        opts,
                        fsm_concat(
                            opts,
                            fsm_universal(),
                            fsm_intersect(opts, fsm_copy(&mut net1), fsm_copy(&mut uplus)),
                        ),
                        fsm_universal(),
                    ),
                ),
            ),
        ),
    );
    fsm_destroy(net1);
    fsm_destroy(net2);
    fsm_destroy(uplus);
    ret
}

// [spec:foma:def:constructions.fsm-priority-union-upper-fn]
// [spec:foma:sem:constructions.fsm-priority-union-upper-fn]
// [spec:foma:def:fomalib.fsm-priority-union-upper-fn]
// [spec:foma:sem:fomalib.fsm-priority-union-upper-fn]
pub fn fsm_priority_union_upper(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    /* A .P. B = A | [~[A.u] .o. B] */
    let mut net1 = net1;
    let ret = fsm_union(
        opts,
        fsm_copy(&mut net1),
        fsm_compose(
            opts,
            fsm_complement(opts, fsm_upper(fsm_copy(&mut net1))),
            net2,
        ),
    );
    fsm_destroy(net1);
    ret
}

// [spec:foma:def:constructions.fsm-priority-union-lower-fn]
// [spec:foma:sem:constructions.fsm-priority-union-lower-fn]
// [spec:foma:def:fomalib.fsm-priority-union-lower-fn]
// [spec:foma:sem:fomalib.fsm-priority-union-lower-fn]
pub fn fsm_priority_union_lower(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    /* A .p. B = A | B .o. ~[A.l] */
    let mut net1 = net1;
    let ret = fsm_union(
        opts,
        fsm_copy(&mut net1),
        fsm_compose(
            opts,
            net2,
            fsm_complement(opts, fsm_lower(fsm_copy(&mut net1))),
        ),
    );
    fsm_destroy(net1);
    ret
}

// [spec:foma:def:constructions.fsm-lenient-compose-fn]
// [spec:foma:sem:constructions.fsm-lenient-compose-fn]
// [spec:foma:def:fomalib.fsm-lenient-compose-fn]
// [spec:foma:sem:fomalib.fsm-lenient-compose-fn]
pub fn fsm_lenient_compose(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    /* A .O. B = [A .o. B] .P. A — lenient composition, with A (net1) as the
    priority-union fallback for inputs outside dom([A .o. B]). (The upstream
    C source comment read ".P. B", but its code passes a copy of A as the
    fallback; this is the actual foma semantics.) */
    let mut net1 = net1;
    let ret = fsm_priority_union_upper(
        opts,
        fsm_compose(opts, fsm_copy(&mut net1), net2),
        fsm_copy(&mut net1),
    );
    fsm_destroy(net1);
    ret
}

// [spec:foma:def:constructions.fsm-quotient-interleave-fn]
// [spec:foma:sem:constructions.fsm-quotient-interleave-fn]
// [spec:foma:def:fomalib.fsm-quotient-interleave-fn]
// [spec:foma:sem:fomalib.fsm-quotient-interleave-fn]
pub fn fsm_quotient_interleave(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    /* A/\/B = The set of strings you can interleave in B and get a string from A */
    /* [B/[x \x* x] & A/x .o. [[[\x]:0]* (x:0 \x* x:0)]*].l */
    let mut result = fsm_lower(fsm_compose(
        opts,
        fsm_intersect(
            opts,
            fsm_ignore(
                opts,
                net2,
                fsm_concat(
                    opts,
                    fsm_symbol("@>@"),
                    fsm_concat(
                        opts,
                        fsm_kleene_star(opts, fsm_term_negation(opts, fsm_symbol("@>@"))),
                        fsm_symbol("@>@"),
                    ),
                ),
                OP_IGNORE_ALL,
            ),
            fsm_ignore(opts, net1, fsm_symbol("@>@"), OP_IGNORE_ALL),
        ),
        fsm_kleene_star(
            opts,
            fsm_concat(
                opts,
                fsm_kleene_star(
                    opts,
                    fsm_cross_product(
                        opts,
                        fsm_term_negation(opts, fsm_symbol("@>@")),
                        fsm_empty_string(),
                    ),
                ),
                fsm_optionality(
                    opts,
                    fsm_concat(
                        opts,
                        fsm_cross_product(opts, fsm_symbol("@>@"), fsm_empty_string()),
                        fsm_concat(
                            opts,
                            fsm_kleene_star(opts, fsm_term_negation(opts, fsm_symbol("@>@"))),
                            fsm_cross_product(opts, fsm_symbol("@>@"), fsm_empty_string()),
                        ),
                    ),
                ),
            ),
        ),
    ));

    sigma_remove("@>@", &mut result.sigma);
    /* Could clean up sigma */
    result
}

// [spec:foma:def:constructions.fsm-quotient-left-fn]
// [spec:foma:sem:constructions.fsm-quotient-left-fn]
// [spec:foma:def:fomalib.fsm-quotient-left-fn]
// [spec:foma:sem:fomalib.fsm-quotient-left-fn]
pub fn fsm_quotient_left(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    /* A\\\B = [B .o. A:0 ?*].l; */
    /* A\\\B = the set of suffixes you can add to A to get a string in B */
    fsm_lower(fsm_compose(
        opts,
        net2,
        fsm_concat(
            opts,
            fsm_cross_product(opts, net1, fsm_empty_string()),
            fsm_universal(),
        ),
    ))
}

// [spec:foma:def:constructions.fsm-quotient-right-fn]
// [spec:foma:sem:constructions.fsm-quotient-right-fn]
// [spec:foma:def:fomalib.fsm-quotient-right-fn]
// [spec:foma:sem:fomalib.fsm-quotient-right-fn]
pub fn fsm_quotient_right(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    /* A///B = [A .o. ?* B:0].l; */
    /* A///B = the set of prefixes you can add to B to get strings in A */
    fsm_lower(fsm_compose(
        opts,
        net1,
        fsm_concat(
            opts,
            fsm_universal(),
            fsm_cross_product(opts, net2, fsm_empty_string()),
        ),
    ))
}

// [spec:foma:def:constructions.fsm-ignore-fn]
// [spec:foma:sem:constructions.fsm-ignore-fn+1]
// [spec:foma:def:fomalib.fsm-ignore-fn]
// [spec:foma:sem:fomalib.fsm-ignore-fn+1]
pub fn fsm_ignore(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>, operation: i32) -> Box<Fsm> {
    let mut net1 = fsm_minimize(opts, net1);
    let mut net2 = fsm_minimize(opts, net2);

    if fsm_isempty(opts, &mut net2) {
        fsm_destroy(net2);
        return net1;
    }
    fsm_merge_sigma(opts, &mut net1, &mut net2);

    fsm_count(&mut net1);
    fsm_count(&mut net2);

    let states1 = net1.statecount;
    let states2 = net2.statecount;
    let lines1 = net1.linecount;
    let lines2 = net2.linecount;

    if operation == OP_IGNORE_INTERNAL {
        let mut result = fsm_lower(fsm_compose(
            opts,
            fsm_ignore(opts, fsm_copy(&mut net1), fsm_symbol("@i<@"), OP_IGNORE_ALL),
            fsm_compose(
                opts,
                fsm_complement(
                    opts,
                    fsm_union(
                        opts,
                        fsm_concat(opts, fsm_symbol("@i<@"), fsm_universal()),
                        fsm_concat(opts, fsm_universal(), fsm_symbol("@i<@")),
                    ),
                ),
                fsm_simple_replace(opts, fsm_symbol("@i<@"), fsm_copy(&mut net2)),
            ),
        ));
        sigma_remove("@i<@", &mut result.sigma);
        fsm_destroy(net1);
        fsm_destroy(net2);
        return result;
    }

    let malloc_size = lines1 + states1 * (lines2 + net2.finalcount + 1);
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
        (malloc_size + 1) as usize
    ];

    /* Mark if a state has been handled with ignore */
    /* C: malloc'd (uninitialized); handled_states1 is initialized below,
    handled_states2 is re-zeroed per splice — zero-filled here */
    let mut handled_states1: Vec<i16> = vec![0; states1 as usize];
    let mut handled_states2: Vec<i16> = vec![0; states2 as usize];

    /* Mark which ignores return to which state */
    let mut return_state: Vec<i32> = vec![0; states1 as usize];
    let splice_size = states2;
    let start_splice = states1;
    for k in 0..states1 {
        handled_states1[k as usize] = 0;
    }

    let mut splices = 0;
    let mut j: i32 = 0;
    let mut i = 0usize;
    while net1.states[i].state_no != -1 {
        if handled_states1[net1.states[i].state_no as usize] == 0 {
            let target = start_splice + splices * splice_size;
            let (state_no, final_state, start_state) = (
                net1.states[i].state_no,
                net1.states[i].final_state as i32,
                net1.states[i].start_state as i32,
            );
            add_fsm_arc(
                &mut new_fsm,
                j,
                state_no,
                EPSILON,
                EPSILON,
                target,
                final_state,
                start_state,
            );
            return_state[splices as usize] = state_no;
            handled_states1[state_no as usize] = 1;
            j += 1;
            splices += 1;
            if net1.states[i].r#in != -1 {
                let (line_in, line_out, tgt) = (
                    net1.states[i].r#in as i32,
                    net1.states[i].out as i32,
                    net1.states[i].target,
                );
                add_fsm_arc(
                    &mut new_fsm,
                    j,
                    state_no,
                    line_in,
                    line_out,
                    tgt,
                    final_state,
                    start_state,
                );
                j += 1;
            }
        } else {
            let (state_no, line_in, line_out, tgt, final_state, start_state) = (
                net1.states[i].state_no,
                net1.states[i].r#in as i32,
                net1.states[i].out as i32,
                net1.states[i].target,
                net1.states[i].final_state as i32,
                net1.states[i].start_state as i32,
            );
            add_fsm_arc(
                &mut new_fsm,
                j,
                state_no,
                line_in,
                line_out,
                tgt,
                final_state,
                start_state,
            );
            j += 1;
        }
        i += 1;
    }

    /* Add a sequence of fsm2s at the end, with arcs back to the appropriate states */

    let mut state_add_counter = start_splice;

    let mut returns = 0;
    while splices > 0 {
        /* Zero handled return arc states */

        for k in 0..states2 {
            handled_states2[k as usize] = 0;
        }

        let mut i = 0usize;
        while net2.states[i].state_no != -1 {
            if net2.states[i].final_state == 1
                && handled_states2[net2.states[i].state_no as usize] == 0
            {
                let state_no = net2.states[i].state_no;
                add_fsm_arc(
                    &mut new_fsm,
                    j,
                    state_no + state_add_counter,
                    EPSILON,
                    EPSILON,
                    return_state[returns as usize],
                    0,
                    0,
                );
                j += 1;
                handled_states2[state_no as usize] = 1;
                if net2.states[i].target != -1 {
                    let (line_in, line_out, tgt) = (
                        net2.states[i].r#in as i32,
                        net2.states[i].out as i32,
                        net2.states[i].target,
                    );
                    add_fsm_arc(
                        &mut new_fsm,
                        j,
                        state_no + state_add_counter,
                        line_in,
                        line_out,
                        tgt + state_add_counter,
                        0,
                        0,
                    );
                    j += 1;
                }
            } else {
                // [spec:foma:sem:constructions.fsm-ignore-fn+1] preserve a -1
                // (final-marker) target rather than shifting it into a bogus state
                // number. C shifted unconditionally; a -1 target cannot occur for a
                // minimized net2, but a non-minimized net2 would be corrupted.
                let (state_no, line_in, line_out, tgt) = (
                    net2.states[i].state_no,
                    net2.states[i].r#in as i32,
                    net2.states[i].out as i32,
                    net2.states[i].target,
                );
                let shifted_tgt = if tgt == -1 {
                    -1
                } else {
                    tgt + state_add_counter
                };
                add_fsm_arc(
                    &mut new_fsm,
                    j,
                    state_no + state_add_counter,
                    line_in,
                    line_out,
                    shifted_tgt,
                    0,
                    0,
                );
                j += 1;
            }
            i += 1;
        }
        state_add_counter += states2;
        splices -= 1;
        returns += 1;
    }

    add_fsm_arc(&mut new_fsm, j, -1, -1, -1, -1, -1, -1);
    /* free(handled_states1); free(handled_states2); free(return_state) */
    drop(handled_states1);
    drop(handled_states2);
    drop(return_state);
    /* free(net1->states) */
    fsm_destroy(net2);
    net1.states = new_fsm;
    fsm_update_flags(&mut net1, NO, NO, NO, NO, NO, NO);
    fsm_count(&mut net1);
    net1
}

/* Remove those symbols from sigma that have the same distribution as IDENTITY */

// [spec:foma:def:constructions.fsm-compact-fn]
// [spec:foma:sem:constructions.fsm-compact-fn]
// [spec:foma:def:fomalib.fsm-compact-fn]
// [spec:foma:sem:fomalib.fsm-compact-fn]
pub fn fsm_compact(net: &mut Fsm) {
    /* C: struct checktable { int state_no; int target; } — function-local */
    #[derive(Clone)]
    struct Checktable {
        state_no: i32,
        target: i32,
    }

    let numsymbols = sigma_max(&net.sigma);

    /* C: malloc'd (uninitialized); every entry initialized just below */
    let mut potential: Vec<bool> = vec![false; (numsymbols + 1) as usize];
    let mut checktable: Vec<Checktable> = vec![
        Checktable {
            state_no: 0,
            target: 0,
        };
        (numsymbols + 1) as usize
    ];

    let mut i: i32 = 0;
    while i <= numsymbols {
        potential[i as usize] = true;
        checktable[i as usize].state_no = -1;
        checktable[i as usize].target = -1;
        i += 1;
    }
    /* For consistency reasons, can't remove symbols longer than 1 */
    /* since @ and ? only match utf8 symbols of length 1           */

    for s in &net.sigma {
        if s.symbol.chars().count() > 1 {
            potential[s.number as usize] = false;
        }
    }

    let mut prevstate = 0;

    let mut i = 0usize;
    loop {
        if net.states[i].state_no != prevstate {
            let mut j: i32 = 3;
            while j <= numsymbols {
                if checktable[j as usize].state_no != prevstate
                    && checktable[IDENTITY as usize].state_no != prevstate
                {
                    j += 1;
                    continue;
                }
                if checktable[j as usize].target == checktable[IDENTITY as usize].target
                    && checktable[j as usize].state_no == checktable[IDENTITY as usize].state_no
                {
                    j += 1;
                    continue;
                }
                potential[j as usize] = false;
                j += 1;
            }
        }

        if net.states[i].state_no == -1 {
            break;
        }

        let r#in = net.states[i].r#in as i32;
        let out = net.states[i].out as i32;
        let state = net.states[i].state_no;
        let target = net.states[i].target;

        if r#in != -1 && out != -1 {
            if (r#in == out && r#in > 2) || r#in == IDENTITY {
                checktable[r#in as usize].state_no = state;
                checktable[r#in as usize].target = target;
            }
            if r#in != out && r#in > 2 {
                potential[r#in as usize] = false;
            }
            if r#in != out && out > 2 {
                potential[out as usize] = false;
            }
        }
        prevstate = state;
        i += 1;
    }
    let mut removable = 0;
    let mut i: i32 = 3;
    while i <= numsymbols {
        if potential[i as usize] {
            removable = 1;
        }
        i += 1;
    }
    if removable == 0 {
        /* free(potential); free(checktable) */
        drop(potential);
        drop(checktable);
        return;
    }
    let mut i = 0usize;
    let mut j: i32 = 0;
    loop {
        let r#in = net.states[i].r#in as i32;

        let (state_no, out, target, final_state, start_state) = (
            net.states[i].state_no,
            net.states[i].out as i32,
            net.states[i].target,
            net.states[i].final_state as i32,
            net.states[i].start_state as i32,
        );
        add_fsm_arc(
            &mut net.states,
            j,
            state_no,
            r#in,
            out,
            target,
            final_state,
            start_state,
        );
        if r#in == -1 {
            i += 1;
            j += 1;
        } else if potential[r#in as usize] && r#in > 2 {
            i += 1;
        } else {
            i += 1;
            j += 1;
        }
        if net.states[i].state_no == -1 {
            break;
        }
    }
    let (state_no, r#in, out, target, final_state, start_state) = (
        net.states[i].state_no,
        net.states[i].r#in as i32,
        net.states[i].out as i32,
        net.states[i].target,
        net.states[i].final_state as i32,
        net.states[i].start_state as i32,
    );
    add_fsm_arc(
        &mut net.states,
        j,
        state_no,
        r#in,
        out,
        target,
        final_state,
        start_state,
    );

    /* drop every alphabet entry with number > 2 flagged in `potential` */
    net.sigma
        .retain(|s| !(s.number > 2 && potential[s.number as usize]));
    /* free(potential); free(checktable) */
    drop(potential);
    drop(checktable);
    sigma_cleanup(net, 0);
}

// [spec:foma:def:constructions.fsm-symbol-occurs-fn]
// [spec:foma:sem:constructions.fsm-symbol-occurs-fn]
// [spec:foma:def:fomalib.fsm-symbol-occurs-fn]
// [spec:foma:sem:fomalib.fsm-symbol-occurs-fn]
pub fn fsm_symbol_occurs(net: &Fsm, symbol: &str, side: i32) -> bool {
    let Some(sym) = sigma_find(symbol, &net.sigma) else {
        return false;
    };
    let mut i = 0usize;
    while net.states[i].state_no != -1 {
        if side == M_UPPER && net.states[i].r#in as i32 == sym {
            return true;
        }
        if side == M_LOWER && net.states[i].out as i32 == sym {
            return true;
        }
        if side == (M_UPPER + M_LOWER)
            && (net.states[i].r#in as i32 == sym || net.states[i].out as i32 == sym)
        {
            return true;
        }
        i += 1;
    }
    false
}

// [spec:foma:def:constructions.fsm-equal-substrings-fn]
// [spec:foma:sem:constructions.fsm-equal-substrings-fn]
// [spec:foma:def:fomalib.fsm-equal-substrings-fn]
// [spec:foma:sem:fomalib.fsm-equal-substrings-fn]
pub fn fsm_equal_substrings(
    opts: &FomaOptions,
    net: Box<Fsm>,
    left: &mut Fsm,
    right: &mut Fsm,
) -> Box<Fsm> {
    /* The algorithm extracts from the lower side all and only those strings where   */
    /* every X occurring in different substrings ... left X right ... is identical.  */

    /* Caveat: there is no reliable termination condition for the loop that extracts */
    /* identities.  This means that if run on languages where there are potentially  */
    /* infinite-length identical delimited substrings, it will not terminate.        */

    let mut net = net;
    let oldnet = fsm_copy(&mut net);

    /* LB = "@<eq<@" */
    /* RB = "@>eq>@" */

    let mut lb = fsm_symbol("@<eq<@");
    let mut nolb = fsm_minimize(opts, fsm_term_negation(opts, fsm_copy(&mut lb)));
    let mut rb = fsm_symbol("@>eq>@");
    let mut norb = fsm_minimize(opts, fsm_term_negation(opts, fsm_copy(&mut rb)));
    /* NOBR = ~$[LB|RB] */
    let mut nobr = fsm_minimize(
        opts,
        fsm_complement(
            opts,
            fsm_contains(opts, fsm_union(opts, fsm_copy(&mut lb), fsm_copy(&mut rb))),
        ),
    );

    sigma_add("@<eq<@", &mut net.sigma);
    sigma_add("@>eq>@", &mut net.sigma);
    sigma_sort(&mut net);

    /* Insert our aux markers into the language                */

    /* InsertBrackets = [~$[L|R] [L 0:LB|0:RB R]]* ~$[L|R];    */

    let insert_brackets = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_kleene_star(
                opts,
                fsm_concat(
                    opts,
                    fsm_complement(
                        opts,
                        fsm_contains(opts, fsm_union(opts, fsm_copy(left), fsm_copy(right))),
                    ),
                    fsm_union(
                        opts,
                        fsm_concat(
                            opts,
                            fsm_copy(left),
                            fsm_cross_product(opts, fsm_empty_string(), fsm_copy(&mut lb)),
                        ),
                        fsm_concat(
                            opts,
                            fsm_cross_product(opts, fsm_empty_string(), fsm_copy(&mut rb)),
                            fsm_copy(right),
                        ),
                    ),
                ),
            ),
            fsm_complement(
                opts,
                fsm_contains(opts, fsm_union(opts, fsm_copy(left), fsm_copy(right))),
            ),
        ),
    );

    /* Lbracketed = L .o. InsertBrackets                       */

    let mut lbracketed = fsm_compose(opts, fsm_copy(&mut net), insert_brackets);

    /* Filter out improper nestings, or languages with less than two marker pairs */

    /* BracketFilter = NOBR LB NOBR RB NOBR [LB NOBR RB NOBR]+  */

    let mut bracket_filter = fsm_concat(
        opts,
        fsm_copy(&mut nobr),
        fsm_concat(
            opts,
            fsm_copy(&mut lb),
            fsm_concat(
                opts,
                fsm_copy(&mut nobr),
                fsm_concat(
                    opts,
                    fsm_copy(&mut rb),
                    fsm_concat(
                        opts,
                        fsm_copy(&mut nobr),
                        fsm_kleene_plus(
                            opts,
                            fsm_concat(
                                opts,
                                fsm_copy(&mut lb),
                                fsm_concat(
                                    opts,
                                    fsm_copy(&mut nobr),
                                    fsm_concat(opts, fsm_copy(&mut rb), fsm_copy(&mut nobr)),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        ),
    );

    /* RemoveBrackets = [LB:0|RB:0|NOBR]*                       */
    /* Lbypass = [Lbracketed .o. ~BracketFilter .o. LB|RB -> 0] */
    /* Leq     = [Lbracketed .o.  BracketFilter]                */

    let remove_brackets = fsm_kleene_star(
        opts,
        fsm_union(
            opts,
            fsm_cross_product(opts, fsm_copy(&mut lb), fsm_empty_string()),
            fsm_union(
                opts,
                fsm_cross_product(opts, fsm_copy(&mut rb), fsm_empty_string()),
                fsm_copy(&mut nobr),
            ),
        ),
    );

    let lbypass = fsm_lower(fsm_compose(
        opts,
        fsm_copy(&mut lbracketed),
        fsm_compose(
            opts,
            fsm_complement(opts, fsm_copy(&mut bracket_filter)),
            remove_brackets,
        ),
    ));
    let mut leq = fsm_compose(opts, lbracketed, bracket_filter);

    /* Extract labels from lower side of L */
    /* [Leq .o. [\LB:0* LB:0 \RB* RB:0]* \LB:0*].l */

    let labels = fsm_sigma_pairs_net(fsm_lower(fsm_compose(
        opts,
        fsm_copy(&mut leq),
        fsm_concat(
            opts,
            fsm_kleene_star(
                opts,
                fsm_concat(
                    opts,
                    fsm_kleene_star(
                        opts,
                        fsm_cross_product(opts, fsm_copy(&mut nolb), fsm_empty_string()),
                    ),
                    fsm_concat(
                        opts,
                        fsm_cross_product(opts, fsm_copy(&mut lb), fsm_empty_string()),
                        fsm_concat(
                            opts,
                            fsm_kleene_star(opts, fsm_copy(&mut norb)),
                            fsm_cross_product(opts, fsm_copy(&mut rb), fsm_empty_string()),
                        ),
                    ),
                ),
            ),
            fsm_kleene_star(
                opts,
                fsm_cross_product(opts, fsm_copy(&mut nolb), fsm_empty_string()),
            ),
        ),
    )));

    /* Cleanup = \LB* [LB:0 RB:0 \LB*]* | ~$[LB RB] */

    let mut cleanup = fsm_minimize(
        opts,
        fsm_union(
            opts,
            fsm_concat(
                opts,
                fsm_kleene_star(opts, fsm_copy(&mut nolb)),
                fsm_kleene_star(
                    opts,
                    fsm_concat(
                        opts,
                        fsm_cross_product(opts, fsm_copy(&mut lb), fsm_empty_string()),
                        fsm_concat(
                            opts,
                            fsm_cross_product(opts, fsm_copy(&mut rb), fsm_empty_string()),
                            fsm_kleene_star(opts, fsm_copy(&mut nolb)),
                        ),
                    ),
                ),
            ),
            fsm_complement(
                opts,
                fsm_contains(opts, fsm_concat(opts, fsm_copy(&mut lb), fsm_copy(&mut rb))),
            ),
        ),
    );

    /* Construct the move function */

    let mut r#move = fsm_empty_string();

    let mut syms = 0;
    for s in &labels.sigma {
        /* Unclear which is faster: the first or the second version */
        /* ThisMove = [\LB* LB:X X:LB]* \LB*       */
        /* ThisMove = [\LB* LB:0 X 0:LB]* \LB*     */
        if s.number >= 3 {
            let mut this_symbol = fsm_symbol(&s.symbol);
            let this_move = fsm_concat(
                opts,
                fsm_kleene_star(
                    opts,
                    fsm_concat(
                        opts,
                        fsm_kleene_star(opts, fsm_copy(&mut nolb)),
                        fsm_concat(
                            opts,
                            fsm_cross_product(opts, fsm_copy(&mut lb), fsm_empty_string()),
                            fsm_concat(
                                opts,
                                fsm_copy(&mut this_symbol),
                                fsm_cross_product(opts, fsm_empty_string(), fsm_copy(&mut lb)),
                            ),
                        ),
                    ),
                ),
                fsm_kleene_star(opts, fsm_copy(&mut nolb)),
            );

            r#move = fsm_union(opts, r#move, this_move);
            syms += 1;
        }
    }
    let mut r#move = fsm_minimize(opts, r#move);
    if syms == 0 {
        //printf("no syms");
        fsm_destroy(net);
        return oldnet;
    }

    /* Move until no bracket symbols remain */
    loop {
        //printf("Zapping\n");
        leq = fsm_compose(opts, leq, fsm_copy(&mut cleanup));
        if !fsm_symbol_occurs(&leq, "@<eq<@", M_LOWER) {
            break;
        }
        leq = fsm_compose(opts, leq, fsm_copy(&mut r#move));
    }

    /* Result = L .o. [Leq | Lbypass] */
    let mut result = fsm_minimize(
        opts,
        fsm_compose(opts, net, fsm_union(opts, fsm_lower(leq), lbypass)),
    );
    /* C: sigma_remove's returned new head is discarded (harmless unless
    the removed node were the head); the owned list is reassigned here */
    sigma_remove("@<eq<@", &mut result.sigma);
    sigma_remove("@>eq>@", &mut result.sigma);
    fsm_compact(&mut result);
    sigma_sort(&mut result);
    fsm_destroy(oldnet);
    result
}

// [spec:foma:def:constructions.fsm-sequentialize-fn]
// [spec:foma:sem:constructions.fsm-sequentialize-fn]
// [spec:foma:def:fomalib.fsm-sequentialize-fn]
// [spec:foma:sem:fomalib.fsm-sequentialize-fn]
pub fn fsm_sequentialize(net: Box<Fsm>) -> Box<Fsm> {
    /* C: unimplemented stub — warns and returns the input unchanged */
    tracing::warn!("Implementation pending");
    net
}

// [spec:foma:def:constructions.fsm-bimachine-fn]
// [spec:foma:sem:constructions.fsm-bimachine-fn]
// [spec:foma:def:fomalib.fsm-bimachine-fn]
// [spec:foma:sem:fomalib.fsm-bimachine-fn]
pub fn fsm_bimachine(net: Box<Fsm>) -> Box<Fsm> {
    /* C: unimplemented stub — warns and returns the input unchanged */
    tracing::warn!("implementation pending");
    net
}

/* _leftrewr(L, a:b) does a -> b || .#. L _    */
/* _leftrewr(?* L, a:b) does a -> b || L _     */
/* works only with single symbols, but is fast */

// [spec:foma:def:constructions.fsm-left-rewr-fn]
// [spec:foma:sem:constructions.fsm-left-rewr-fn]
// [spec:foma:def:fomalib.fsm-left-rewr-fn]
// [spec:foma:sem:fomalib.fsm-left-rewr-fn]
pub fn fsm_left_rewr(opts: &FomaOptions, net: Box<Fsm>, rewr: Box<Fsm>) -> Box<Fsm> {
    let mut net = net;
    let mut rewr = rewr;
    fsm_merge_sigma(opts, &mut net, &mut rewr);
    let relabelin = rewr.states[0].r#in as i32;
    let relabelout = rewr.states[0].out as i32;

    let mut inh = fsm_read_init(net);
    let sinkstate = fsm_get_num_states(&inh);
    let name = inh
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .name
        .clone();
    let mut outh = fsm_construct_init(&name);
    fsm_construct_copy_sigma(
        &mut outh,
        &inh.net
            .as_ref()
            .expect("net present until fsm_read_done")
            .sigma,
    );
    let mut maxsigma = sigma_max(
        &inh.net
            .as_ref()
            .expect("net present until fsm_read_done")
            .sigma,
    );
    maxsigma += 1;
    /* C: malloc'd (uninitialized); initialized to -1 just below */
    let mut sigmatable: Vec<i32> = vec![0; maxsigma as usize];
    for i in 0..maxsigma {
        sigmatable[i as usize] = -1;
    }
    let mut addedsink = 0;
    loop {
        let currstate = fsm_get_next_state(&mut inh);
        if currstate == -1 {
            break;
        }
        let mut seensource = 0;
        fsm_construct_set_final(&mut outh, currstate);

        while fsm_get_next_state_arc(&mut inh) != 0 {
            let innum = fsm_get_arc_num_in(&inh);
            let mut outnum = fsm_get_arc_num_out(&inh);
            sigmatable[innum as usize] = currstate;
            if innum == relabelin {
                seensource = 1;
                if fsm_read_is_final(&inh, currstate) {
                    outnum = relabelout;
                }
            }
            let (source, target) = (fsm_get_arc_source(&inh), fsm_get_arc_target(&inh));
            fsm_construct_add_arc_nums(&mut outh, source, target, innum, outnum);
        }
        for i in 2..maxsigma {
            if sigmatable[i as usize] != currstate && i != relabelin {
                fsm_construct_add_arc_nums(&mut outh, currstate, sinkstate, i, i);
                addedsink = 1;
            }
        }
        if seensource == 0 {
            addedsink = 1;
            if fsm_read_is_final(&inh, currstate) {
                fsm_construct_add_arc_nums(&mut outh, currstate, sinkstate, relabelin, relabelout);
            } else {
                fsm_construct_add_arc_nums(&mut outh, currstate, sinkstate, relabelin, relabelin);
            }
        }
    }
    if addedsink != 0 {
        for i in 2..maxsigma {
            fsm_construct_add_arc_nums(&mut outh, sinkstate, sinkstate, i, i);
        }
        fsm_construct_set_final(&mut outh, sinkstate);
    }
    fsm_construct_set_initial(&mut outh, 0);
    let net = fsm_read_done(inh);
    let newnet = fsm_construct_done(outh);
    /* free(sigmatable) */
    drop(sigmatable);
    fsm_destroy(net);
    fsm_destroy(rewr);
    newnet
}

// [spec:foma:def:constructions.fsm-add-sink-fn]
// [spec:foma:sem:constructions.fsm-add-sink-fn]
// [spec:foma:def:fomalib.fsm-add-sink-fn]
// [spec:foma:sem:fomalib.fsm-add-sink-fn]
pub fn fsm_add_sink(net: Box<Fsm>, r#final: i32) -> Box<Fsm> {
    let mut inh = fsm_read_init(net);
    let sinkstate = fsm_get_num_states(&inh);
    let name = inh
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .name
        .clone();
    let mut outh = fsm_construct_init(&name);
    fsm_construct_copy_sigma(
        &mut outh,
        &inh.net
            .as_ref()
            .expect("net present until fsm_read_done")
            .sigma,
    );
    let mut maxsigma = sigma_max(
        &inh.net
            .as_ref()
            .expect("net present until fsm_read_done")
            .sigma,
    );
    maxsigma += 1;
    /* C: malloc'd (uninitialized); initialized to -1 just below */
    let mut sigmatable: Vec<i32> = vec![0; maxsigma as usize];
    for i in 0..maxsigma {
        sigmatable[i as usize] = -1;
    }
    loop {
        let currstate = fsm_get_next_state(&mut inh);
        if currstate == -1 {
            break;
        }
        while fsm_get_next_state_arc(&mut inh) != 0 {
            let (source, target, num_in, num_out) = (
                fsm_get_arc_source(&inh),
                fsm_get_arc_target(&inh),
                fsm_get_arc_num_in(&inh),
                fsm_get_arc_num_out(&inh),
            );
            fsm_construct_add_arc_nums(&mut outh, source, target, num_in, num_out);
            sigmatable[fsm_get_arc_num_in(&inh) as usize] = currstate;
        }
        for i in 2..maxsigma {
            if sigmatable[i as usize] != currstate {
                fsm_construct_add_arc_nums(&mut outh, currstate, sinkstate, i, i);
            }
        }
    }
    for i in 2..maxsigma {
        fsm_construct_add_arc_nums(&mut outh, sinkstate, sinkstate, i, i);
    }

    for i in inh.finals() {
        fsm_construct_set_final(&mut outh, i);
    }
    if r#final == 1 {
        fsm_construct_set_final(&mut outh, sinkstate);
    }
    fsm_construct_set_initial(&mut outh, 0);
    let net = fsm_read_done(inh);
    let newnet = fsm_construct_done(outh);
    fsm_destroy(net);
    newnet
}

/* _addfinalloop(L, "#":0) adds "#":0 at all final states */
/* _addnonfinalloop(L, "#":0) adds "#":0 at all nonfinal states */
/* _addloop(L, "#":0) adds "#":0 at all states */

/* Adds loops at finals = 0 nonfinals, finals = 1 finals, finals = 2, all */

// [spec:foma:def:constructions.fsm-add-loop-fn]
// [spec:foma:sem:constructions.fsm-add-loop-fn]
// [spec:foma:def:fomalib.fsm-add-loop-fn]
// [spec:foma:sem:fomalib.fsm-add-loop-fn]
pub fn fsm_add_loop(net: Box<Fsm>, marker: &Fsm, finals: i32) -> Box<Fsm> {
    let mut inh = fsm_read_init(net);
    /* C: the read handle borrows marker (which is NOT destroyed); the
    Rust handle owns a deep copy — read-only, observably equivalent */
    let mut minh = fsm_read_init(Box::new(marker.clone()));

    let name = inh
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .name
        .clone();
    let mut outh = fsm_construct_init(&name);
    fsm_construct_copy_sigma(
        &mut outh,
        &inh.net
            .as_ref()
            .expect("net present until fsm_read_done")
            .sigma,
    );

    while fsm_get_next_arc(&mut inh) != 0 {
        let (source, target, num_in, num_out) = (
            fsm_get_arc_source(&inh),
            fsm_get_arc_target(&inh),
            fsm_get_arc_num_in(&inh),
            fsm_get_arc_num_out(&inh),
        );
        fsm_construct_add_arc_nums(&mut outh, source, target, num_in, num_out);
    }
    /* Where to put the loops */
    if finals == 1 {
        loop {
            let i = fsm_get_next_final(&mut inh);
            if i == -1 {
                break;
            }
            fsm_construct_set_final(&mut outh, i);
            fsm_read_reset(Some(&mut minh));
            while fsm_get_next_arc(&mut minh) != 0 {
                let min_in = fsm_get_arc_in(&minh)
                    .expect("arc label present on the positioned cursor")
                    .to_string();
                let min_out = fsm_get_arc_out(&minh)
                    .expect("arc label present on the positioned cursor")
                    .to_string();
                fsm_construct_add_arc(&mut outh, i, i, &min_in, &min_out);
            }
        }
    } else if finals == 0 || finals == 2 {
        let statecount = inh
            .net
            .as_ref()
            .expect("net present until fsm_read_done")
            .statecount;
        for i in 0..statecount {
            if finals == 2 || !fsm_read_is_final(&inh, i) {
                fsm_read_reset(Some(&mut minh));
                while fsm_get_next_arc(&mut minh) != 0 {
                    let min_in = fsm_get_arc_in(&minh)
                        .expect("arc label present on the positioned cursor")
                        .to_string();
                    let min_out = fsm_get_arc_out(&minh)
                        .expect("arc label present on the positioned cursor")
                        .to_string();
                    fsm_construct_add_arc(&mut outh, i, i, &min_in, &min_out);
                }
            }
        }
    }
    for i in inh.finals() {
        fsm_construct_set_final(&mut outh, i);
    }
    fsm_construct_set_initial(&mut outh, 0);
    let net = fsm_read_done(inh);
    /* fsm_read_done(minh) — frees the handle; the marker copy is dropped
    with it (the C caller keeps the original marker) */
    let marker_copy = fsm_read_done(minh);
    drop(marker_copy);
    let newnet = fsm_construct_done(outh);
    fsm_destroy(net);
    newnet
}

// [spec:foma:def:constructions.fsm-context-restrict-fn]
// [spec:foma:sem:constructions.fsm-context-restrict-fn]
// [spec:foma:def:fomalib.fsm-context-restrict-fn]
// [spec:foma:sem:fomalib.fsm-context-restrict-fn]
pub fn fsm_context_restrict(
    opts: &FomaOptions,
    x: Box<Fsm>,
    lr: Option<Box<Fsmcontexts>>,
) -> Box<Fsm> {
    /* [.#. \.#.* .#.]-'[[ [\X* X C X \X*]&~[\X* [L1 X \X* X R1|...|Ln X \X* X Rn] \X*]],X,0] */
    /* Where X = variable symbol */

    let mut x = x;
    let mut lr = lr;

    let mut var = fsm_symbol("@VARX@");
    let mut notvar = fsm_minimize(
        opts,
        fsm_kleene_star(opts, fsm_term_negation(opts, fsm_symbol("@VARX@"))),
    );

    /* We add the variable symbol to all alphabets to avoid ? mathing it */
    /* which would cause extra nondeterminism */
    sigma_add("@VARX@", &mut x.sigma);
    sigma_sort(&mut x);

    /* Also, if any L or R is undeclared we add 0 */
    let mut pairs = lr.as_deref_mut();
    while let Some(p) = pairs {
        if let Some(left) = p.left.as_deref_mut() {
            sigma_add("@VARX@", &mut left.sigma);
            let _ = sigma_substitute(".#.", "@#@", &mut left.sigma);
            sigma_sort(left);
        } else {
            p.left = Some(fsm_empty_string());
        }
        if let Some(right) = p.right.as_deref_mut() {
            sigma_add("@VARX@", &mut right.sigma);
            let _ = sigma_substitute(".#.", "@#@", &mut right.sigma);
            sigma_sort(right);
        } else {
            p.right = Some(fsm_empty_string());
        }
        pairs = p.next.as_deref_mut();
    }

    let mut union_p = fsm_empty_set();

    let mut pairs = lr.as_deref_mut();
    while let Some(p) = pairs {
        union_p = fsm_minimize(
            opts,
            fsm_union(
                opts,
                fsm_minimize(
                    opts,
                    fsm_concat(
                        opts,
                        fsm_copy(
                            p.left
                                .as_deref_mut()
                                .expect("left filled by the preceding pass"),
                        ),
                        fsm_concat(
                            opts,
                            fsm_copy(&mut var),
                            fsm_concat(
                                opts,
                                fsm_copy(&mut notvar),
                                fsm_concat(
                                    opts,
                                    fsm_copy(&mut var),
                                    fsm_copy(
                                        p.right
                                            .as_deref_mut()
                                            .expect("right filled by the preceding pass"),
                                    ),
                                ),
                            ),
                        ),
                    ),
                ),
                union_p,
            ),
        );
        pairs = p.next.as_deref_mut();
    }

    let union_l = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_copy(&mut notvar),
            fsm_concat(
                opts,
                fsm_copy(&mut var),
                fsm_concat(
                    opts,
                    fsm_copy(&mut x),
                    fsm_concat(opts, fsm_copy(&mut var), fsm_copy(&mut notvar)),
                ),
            ),
        ),
    );

    let mut result = fsm_intersect(
        opts,
        union_l,
        fsm_complement(
            opts,
            fsm_concat(
                opts,
                fsm_copy(&mut notvar),
                fsm_minimize(
                    opts,
                    fsm_concat(opts, fsm_copy(&mut union_p), fsm_copy(&mut notvar)),
                ),
            ),
        ),
    );
    if sigma_find("@VARX@", &result.sigma).is_some() {
        result = fsm_complement(
            opts,
            fsm_substitute_symbol(result, "@VARX@", "@_EPSILON_SYMBOL_@"),
        );
    } else {
        result = fsm_complement(opts, result);
    }

    if sigma_find("@#@", &result.sigma).is_some() {
        let word = fsm_minimize(
            opts,
            fsm_concat(
                opts,
                fsm_symbol("@#@"),
                fsm_concat(
                    opts,
                    fsm_kleene_star(opts, fsm_term_negation(opts, fsm_symbol("@#@"))),
                    fsm_symbol("@#@"),
                ),
            ),
        );
        result = fsm_intersect(opts, word, result);
        result = fsm_substitute_symbol(result, "@#@", "@_EPSILON_SYMBOL_@");
    }
    fsm_destroy(union_p);
    fsm_destroy(var);
    fsm_destroy(notvar);
    fsm_destroy(x);
    /* C: fsm_clear_contexts(pairs) — pairs is the loop cursor, NULL after
    the loops, so the LR context list is never freed (latent leak;
    fsm_clear_contexts(LR) was clearly intended). Literal NULL call: */
    fsm_clear_contexts(None);
    drop(lr);
    result
}

// [spec:foma:def:constructions.fsm-flatten-fn]
// [spec:foma:sem:constructions.fsm-flatten-fn+1]
// [spec:foma:def:fomalib.fsm-flatten-fn]
// [spec:foma:sem:fomalib.fsm-flatten-fn+1]
pub fn fsm_flatten(opts: &FomaOptions, net: Box<Fsm>, epsilon: Box<Fsm>) -> Option<Box<Fsm>> {
    let net = fsm_minimize(opts, net);

    let mut inh = fsm_read_init(net);
    let mut eps = fsm_read_init(epsilon);
    // [spec:foma:sem:constructions.fsm-flatten-fn+1] no arc in the epsilon
    // machine (fsm_get_next_arc == 0, end-of-arcs) → return None. C tested == -1,
    // which fsm_get_next_arc never returns, so an arc-less epsilon machine fell
    // through and read an invalid arc below.
    if fsm_get_next_arc(&mut eps) == 0 {
        let net = fsm_read_done(inh);
        let epsilon = fsm_read_done(eps);
        fsm_destroy(net);
        fsm_destroy(epsilon);
        return None;
    }
    /* strdup(fsm_get_arc_in(eps)) */
    let epssym = fsm_get_arc_in(&eps)
        .expect("arc label present on the positioned cursor")
        .to_string();
    let epsilon = fsm_read_done(eps);

    let name = inh
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .name
        .clone();
    let mut outh = fsm_construct_init(&name);
    let mut maxstate = inh
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .statecount;

    fsm_construct_copy_sigma(
        &mut outh,
        &inh.net
            .as_ref()
            .expect("net present until fsm_read_done")
            .sigma,
    );

    while fsm_get_next_arc(&mut inh) != 0 {
        let target = fsm_get_arc_target(&inh);
        let r#in = fsm_get_arc_num_in(&inh);
        let out = fsm_get_arc_num_out(&inh);
        if r#in == EPSILON || out == EPSILON {
            let mut instring = fsm_get_arc_in(&inh)
                .expect("arc label present on the positioned cursor")
                .to_string();
            let mut outstring = fsm_get_arc_out(&inh)
                .expect("arc label present on the positioned cursor")
                .to_string();
            if r#in == EPSILON {
                instring = epssym.clone();
            }
            if out == EPSILON {
                outstring = epssym.clone();
            }

            let source = fsm_get_arc_source(&inh);
            fsm_construct_add_arc(&mut outh, source, maxstate, &instring, &instring);
            fsm_construct_add_arc(&mut outh, maxstate, target, &outstring, &outstring);
        } else {
            let source = fsm_get_arc_source(&inh);
            fsm_construct_add_arc_nums(&mut outh, source, maxstate, r#in, r#in);
            fsm_construct_add_arc_nums(&mut outh, maxstate, target, out, out);
        }
        maxstate += 1;
    }
    for i in inh.finals() {
        fsm_construct_set_final(&mut outh, i);
    }
    for i in inh.initials() {
        fsm_construct_set_initial(&mut outh, i);
    }

    let net = fsm_read_done(inh);
    let newnet = fsm_construct_done(outh);
    fsm_destroy(net);
    fsm_destroy(epsilon);
    /* free(epssym) */
    drop(epssym);
    Some(newnet)
}

/* Removes IDENTITY and UNKNOWN transitions. If mode = 1, only removes UNKNOWNs */
// [spec:foma:def:constructions.fsm-close-sigma-fn]
// [spec:foma:sem:constructions.fsm-close-sigma-fn]
// [spec:foma:def:fomalib.fsm-close-sigma-fn]
// [spec:foma:sem:fomalib.fsm-close-sigma-fn]
pub fn fsm_close_sigma(opts: &FomaOptions, net: Box<Fsm>, mode: i32) -> Box<Fsm> {
    let mut inh = fsm_read_init(net);
    let name = inh
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .name
        .clone();
    let mut newh = fsm_construct_init(&name);
    fsm_construct_copy_sigma(
        &mut newh,
        &inh.net
            .as_ref()
            .expect("net present until fsm_read_done")
            .sigma,
    );

    while fsm_get_next_arc(&mut inh) != 0 {
        let num_in = fsm_get_arc_num_in(&inh);
        let num_out = fsm_get_arc_num_out(&inh);
        if (num_in != UNKNOWN && num_in != IDENTITY && num_out != UNKNOWN && num_out != IDENTITY)
            || (mode == 1 && num_in != UNKNOWN && num_out != UNKNOWN)
        {
            let (source, target) = (fsm_get_arc_source(&inh), fsm_get_arc_target(&inh));
            fsm_construct_add_arc_nums(&mut newh, source, target, num_in, num_out);
        }
    }
    for i in inh.finals() {
        fsm_construct_set_final(&mut newh, i);
    }
    for i in inh.initials() {
        fsm_construct_set_initial(&mut newh, i);
    }
    let net = fsm_read_done(inh);
    let newnet = fsm_construct_done(newh);
    fsm_destroy(net);
    fsm_minimize(opts, newnet)
}
