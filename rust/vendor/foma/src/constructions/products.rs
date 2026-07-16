//! Wave-4 split of constructions.c (see mod.rs). Cross-module and
//! external names come via `use super::*` (re-exported by mod.rs).
use super::*;

// [spec:foma:def:constructions.fsm-intersect-fn]
// [spec:foma:sem:constructions.fsm-intersect-fn]
// [spec:foma:def:fomalib.fsm-intersect-fn]
// [spec:foma:sem:fomalib.fsm-intersect-fn]
pub fn fsm_intersect(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    let mut int_stack = IntStack::new();
    /* C: struct blookup {int mainloop; int target; } *array, *bptr; —
    function-local type */
    #[derive(Clone)]
    struct Blookup {
        mainloop: i32,
        target: i32,
    }

    let mut net1 = fsm_minimize(opts, net1);
    let mut net2 = fsm_minimize(opts, net2);

    if fsm_isempty(opts, &mut net1) || fsm_isempty(opts, &mut net2) {
        fsm_destroy(net1);
        fsm_destroy(net2);
        return fsm_empty_set();
    }

    fsm_merge_sigma(opts, &mut net1, &mut net2);

    fsm_update_flags(&mut net1, YES, NO, UNK, YES, UNK, UNK);

    let sigma2size = sigma_max(&net2.sigma) + 1;
    /* calloc — zeroed entries; mainloop stamps start at 1 below, so all
    entries begin stale */
    let mut array: Vec<Blookup> = vec![
        Blookup {
            mainloop: 0,
            target: 0,
        };
        (sigma2size * sigma2size) as usize
    ];
    let mut mainloop = 0;

    /* Intersect two networks by the running-in-parallel method */
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

        /* Create a lookup index for machine b */
        /* array[in][out] holds the target for this state and the symbol pair in:out */
        /* Also, we keep track of whether an entry is fresh by the mainloop counter */
        /* so we don't mistakenly use an old entry and don't have to clear the table */
        /* between each state pair we encounter */

        mainloop += 1;
        let mut bi = point_b[b as usize].transitions;
        while net2.states[bi].state_no == b {
            if net2.states[bi].r#in < 0 {
                bi += 1;
                continue;
            }
            let bptr =
                ((net2.states[bi].r#in as i32) * sigma2size + net2.states[bi].out as i32) as usize;
            array[bptr].mainloop = mainloop;
            array[bptr].target = net2.states[bi].target;
            bi += 1;
        }

        /* The main loop where we run the machines in parallel */
        /* We look at each transition of a in this state, and consult the index of b */
        /* we just created */

        let mut ai = point_a[a as usize].transitions;
        while net1.states[ai].state_no == a {
            if net1.states[ai].r#in < 0 || net1.states[ai].out < 0 {
                ai += 1;
                continue;
            }
            let bptr =
                ((net1.states[ai].r#in as i32) * sigma2size + net1.states[ai].out as i32) as usize;

            if array[bptr].mainloop != mainloop {
                ai += 1;
                continue;
            }

            let atarget = net1.states[ai].target;
            let btarget = array[bptr].target;
            let target_number = match triplet_hash_find(&th, atarget, btarget, 0) {
                Some(n) => n,
                None => {
                    /* STACK_2_PUSH(bptr->target, machine_a->target) */
                    int_stack.push(btarget);
                    int_stack.push(atarget);
                    triplet_hash_insert(&mut th, atarget, btarget, 0)
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
        fsm_state_end_state(&mut builder);
    }
    let mut new_net = fsm_create("");
    fsm_sigma_destroy(core::mem::take(&mut new_net.sigma));
    new_net.sigma = core::mem::take(&mut net1.sigma);
    fsm_destroy(net2);
    fsm_destroy(net1);
    fsm_state_close(&mut builder, &mut new_net);
    /* free(point_a); free(point_b); free(array) */
    drop(point_a);
    drop(point_b);
    drop(array);
    triplet_hash_free(Some(th));
    fsm_coaccessible(new_net)
}

// [spec:foma:def:constructions.fsm-compose-fn]
// [spec:foma:sem:constructions.fsm-compose-fn]
// [spec:foma:def:fomalib.fsm-compose-fn]
// [spec:foma:sem:fomalib.fsm-compose-fn]
pub fn fsm_compose(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    let mut int_stack = IntStack::new();
    /* The composition algorithm is the basic naive composition where we lazily      */
    /* take the cross-product of states P and Q and move to a new state with symbols */
    /* ain, bout if the symbols aout = bin.  Also, if aout = 0 state p goes to       */
    /* its target, while q stays.  Similarly, if bin = 0, q goes to its target       */
    /* while p stays.                                                                */

    /* We have two variants of the algorithm to avoid creating multiple paths:       */
    /* 1) Bistate composition.  In this variant, when we create a new state, we call it */
    /*    (p,q,mode) where mode = 0 or 1, depending on what kind of an arc we followed  */
    /*    to get there.  If we followed an x:y arc where x and y are both real symbols  */
    /*    we always go to mode 0, however, if we followed an 0:y arc, we go to mode 1.  */
    /*    from mode 1, we do not follow x:0 arcs.  Each (p,q,mode) is unique, and       */
    /*    from (p,q,X) we always consider the transitions from p and q.                 */
    /*    We never create arcs (x:0 0:y) yielding x:y.                                  */

    /* 2) Tristate composition. Here we always go to mode 0 with a x:y arc.             */
    /*    (x:0,0:y) yielding x:y is allowed, but only in mode 0                         */
    /*    (x:y y:z) is always allowed and results in target = mode 0                    */
    /*    0:y arcs lead to mode 2, and from there we stay in mode 2 with 0:y            */
    /*    in mode 2 we only consider 0:y and x:y arcs                                   */
    /*    x:0 arcs lead to mode 1, and from there we stay in mode 1 with x:0            */
    /*    in mode 1 we only consider x:0 and x:y arcs                                   */

    /* It seems unsettled which type of composition is better.  Tristate is similar to  */
    /* the filter transducer given in Mohri, Pereira and Riley (1996) and works well    */
    /* for cases such as [a:0 b:0 c:0 .o. 0:d 0:e 0:f], yielding the shortest path.     */
    /* However, for generic cases, bistate seems to yield smaller transducers.          */
    /* The global variable g_compose_tristate is set to OFF by default                  */

    /* C: struct outarray { short int symin; short int symout; int target;
    int mainloop; } and struct index { struct outarray *tail; } —
    function-local types; tail is an index into outarray here */
    #[derive(Clone)]
    struct OutarrayEntry {
        symin: i16,
        symout: i16,
        target: i32,
        mainloop: i32,
    }
    #[derive(Clone)]
    struct Index {
        tail: usize,
    }

    let g_compose_tristate = opts.compose_tristate;
    let g_flag_is_epsilon = opts.flag_is_epsilon;

    let mut net1 = fsm_minimize(opts, net1);
    let mut net2 = fsm_minimize(opts, net2);

    if fsm_isempty(opts, &mut net1) || fsm_isempty(opts, &mut net2) {
        fsm_destroy(net1);
        fsm_destroy(net2);
        return fsm_empty_set();
    }

    /* If flag-is-epsilon is on, we need to add the flag symbols    */
    /* in both networks to each other's sigma so that UNKNOWN       */
    /* or IDENTITY symbols do not match these flags, since they are */
    /* supposed to have the behavior of EPSILON                     */
    /* And we need to do this before merging the sigmas, of course  */

    if g_flag_is_epsilon {
        let mut flags1 = 0;
        let mut flags2 = 0;
        let max2sigma = sigma_max(&net2.sigma);
        for idx in 0..net1.sigma.len() {
            let sym = net1.sigma[idx].symbol.clone();
            if flag_check(&sym) {
                flags1 = 1;
                if sigma_find(&sym, &net2.sigma).is_none() {
                    sigma_add(&sym, &mut net2.sigma);
                }
            }
        }

        for idx in 0..net2.sigma.len() {
            let s2num = net2.sigma[idx].number;
            let sym = net2.sigma[idx].symbol.clone();
            if flag_check(&sym) {
                if s2num <= max2sigma {
                    flags2 = 1;
                }
                if sigma_find(&sym, &net1.sigma).is_none() {
                    sigma_add(&sym, &mut net1.sigma);
                }
            }
        }
        sigma_sort(&mut net2);
        sigma_sort(&mut net1);
        if flags1 != 0 && flags2 != 0 {
            tracing::warn!(
                "flag-is-epsilon is ON and both networks contain flags in composition.  This may yield incorrect results.  Set flag-is-epsilon to OFF."
            );
        }
    }

    fsm_merge_sigma(opts, &mut net1, &mut net2);

    let mut is_flag: Vec<bool> = Vec::new();
    if g_flag_is_epsilon {
        /* Create lookup table for quickly checking if a symbol is a flag */
        /* C: malloc'd (uninitialized for numbers absent from the sigma);
        zero-initialized here */
        is_flag = vec![false; (sigma_max(&net1.sigma) + 1) as usize];
        for s1 in &net1.sigma {
            is_flag[s1.number as usize] = flag_check(&s1.symbol);
        }
    }

    fsm_update_flags(&mut net1, YES, NO, UNK, YES, UNK, UNK);

    let max2sigma = sigma_max(&net2.sigma);

    /* Create an index for looking up input symbols in machine b quickly */
    /* We store each machine_b->in symbol in outarray[symin][...] */
    /* the array index[symin] points to the tail of the current list in outarray */
    /* (we may have many entries for one input symbol as there may be many outputs */
    /* The field mainloop tells us whether the entry is current as we want to loop */
    /* UNKNOWN and IDENTITY are indexed as UNKNOWN because we need to find both */
    /* as they share some semantics */

    let mut index: Vec<Index> = vec![Index { tail: 0 }; (max2sigma + 1) as usize];
    let mut outarray: Vec<OutarrayEntry> = vec![
        OutarrayEntry {
            symin: 0,
            symout: 0,
            target: 0,
            mainloop: 0,
        };
        ((max2sigma + 2) * (max2sigma + 1)) as usize
    ];

    for i in 0..=max2sigma {
        index[i as usize].tail = ((max2sigma + 2) * i) as usize;
    }

    /* Mode, a, b */
    /* STACK_3_PUSH(0,0,0) */
    int_stack.push(0);
    int_stack.push(0);
    int_stack.push(0);

    let mut th = triplet_hash_init();
    triplet_hash_insert(&mut th, 0, 0, 0);

    let mut builder = fsm_state_init(sigma_max(&net1.sigma));

    let point_a = init_state_pointers(&net1.states);
    let point_b = init_state_pointers(&net2.states);

    let mut mainloop = 0;

    while !int_stack.is_empty() {
        /* Get a pair of states to examine */

        let a = int_stack.pop();
        let b = int_stack.pop();
        let mode = int_stack.pop();

        let current_state = triplet_hash_find(&th, a, b, mode)
            .expect("state triple popped off the work stack was inserted into the triplet hash");
        let current_start =
            if point_a[a as usize].start == 1 && point_b[b as usize].start == 1 && mode == 0 {
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

        /* Create the index for machine b in this state */
        mainloop += 1;
        let mut bi = point_b[b as usize].transitions;
        while net2.states[bi].state_no == b {
            /* Index b */
            let bindex = if net2.states[bi].r#in as i32 == IDENTITY {
                UNKNOWN
            } else {
                net2.states[bi].r#in as i32
            };
            if bindex < 0 || net2.states[bi].target < 0 {
                bi += 1;
                continue;
            }

            let mut iptr = index[bindex as usize].tail;
            if outarray[iptr].mainloop != mainloop {
                iptr = (bindex * (max2sigma + 2)) as usize;
                index[bindex as usize].tail = iptr;
            } else {
                iptr += 1;
            }
            outarray[iptr].symin = net2.states[bi].r#in;
            outarray[iptr].symout = net2.states[bi].out;
            outarray[iptr].mainloop = mainloop;
            outarray[iptr].target = net2.states[bi].target;
            index[bindex as usize].tail = iptr;
            bi += 1;
        }

        let mut ai = point_a[a as usize].transitions;
        while net1.states[ai].state_no == a {
            /* If we have the same transition from (a,b)-> some state */
            /* If we have x:y y:z trans to some state */
            let aout = net1.states[ai].out as i32;
            /* IDENTITY is indexed under UNKNOWN (see above) */
            let asearch = if aout == IDENTITY { UNKNOWN } else { aout };
            if aout < 0 {
                ai += 1;
                continue;
            }
            let mut iptr = (asearch * (max2sigma + 2)) as usize;
            let currtail = index[asearch as usize].tail + 1;
            while iptr != currtail && outarray[iptr].mainloop == mainloop {
                let mut ain = net1.states[ai].r#in as i32;
                let mut aout = net1.states[ai].out as i32;
                let mut bin = outarray[iptr].symin as i32;
                let mut bout = outarray[iptr].symout as i32;

                if aout == IDENTITY && bin == UNKNOWN {
                    ain = UNKNOWN;
                    aout = UNKNOWN;
                } else if aout == UNKNOWN && bin == IDENTITY {
                    bin = UNKNOWN;
                    bout = UNKNOWN;
                }

                /* The C branched on g_compose_tristate here, but the bistate
                and tristate arms were byte-identical over complementary
                conditions, so the match-and-emit runs unconditionally. */
                if bin == aout && bin != -1 && (bin != EPSILON || mode == 0) {
                    /* mode -> 0 */
                    let atarget = net1.states[ai].target;
                    let btarget = outarray[iptr].target;
                    let target_number = match triplet_hash_find(&th, atarget, btarget, 0) {
                        Some(n) => n,
                        None => {
                            /* STACK_3_PUSH(0, iptr->target, machine_a->target) */
                            int_stack.push(0);
                            int_stack.push(btarget);
                            int_stack.push(atarget);
                            triplet_hash_insert(&mut th, atarget, btarget, 0)
                        }
                    };

                    fsm_state_add_arc(
                        &mut builder,
                        current_state,
                        ain,
                        bout,
                        target_number,
                        current_final,
                        current_start,
                    );
                }

                iptr += 1;
            }
            ai += 1;
        }

        /* Treat epsilon outputs on machine a (may include flags) */
        let mut ai = point_a[a as usize].transitions;
        while net1.states[ai].state_no == a {
            let aout = net1.states[ai].out as i32;
            if aout != EPSILON && !g_flag_is_epsilon {
                ai += 1;
                continue;
            }
            let ain = net1.states[ai].r#in as i32;

            if g_flag_is_epsilon && aout != -1 && mode == 0 && is_flag[aout as usize] {
                let atarget = net1.states[ai].target;
                let target_number = match triplet_hash_find(&th, atarget, b, 0) {
                    Some(n) => n,
                    None => {
                        /* STACK_3_PUSH(0, b, machine_a->target) */
                        int_stack.push(0);
                        int_stack.push(b);
                        int_stack.push(atarget);
                        triplet_hash_insert(&mut th, atarget, b, 0)
                    }
                };
                fsm_state_add_arc(
                    &mut builder,
                    current_state,
                    ain,
                    aout,
                    target_number,
                    current_final,
                    current_start,
                );
            }

            if !g_compose_tristate {
                /* Check A:0 arcs on upper side */
                if aout == EPSILON && mode == 0 {
                    /* mode -> 0 */
                    let atarget = net1.states[ai].target;
                    let target_number = match triplet_hash_find(&th, atarget, b, 0) {
                        Some(n) => n,
                        None => {
                            /* STACK_3_PUSH(0, b, machine_a->target) */
                            int_stack.push(0);
                            int_stack.push(b);
                            int_stack.push(atarget);
                            triplet_hash_insert(&mut th, atarget, b, 0)
                        }
                    };

                    fsm_state_add_arc(
                        &mut builder,
                        current_state,
                        ain,
                        EPSILON,
                        target_number,
                        current_final,
                        current_start,
                    );
                }
            } else if g_compose_tristate && aout == EPSILON && mode != 2 {
                /* mode -> 1 */
                let atarget = net1.states[ai].target;
                let target_number = match triplet_hash_find(&th, atarget, b, 1) {
                    Some(n) => n,
                    None => {
                        /* STACK_3_PUSH(1, b, machine_a->target) */
                        int_stack.push(1);
                        int_stack.push(b);
                        int_stack.push(atarget);
                        triplet_hash_insert(&mut th, atarget, b, 1)
                    }
                };

                fsm_state_add_arc(
                    &mut builder,
                    current_state,
                    ain,
                    EPSILON,
                    target_number,
                    current_final,
                    current_start,
                );
            }

            ai += 1;
        }
        /* Treat epsilon inputs on machine b (may include flags) */
        let mut bi = point_b[b as usize].transitions;
        while net2.states[bi].state_no == b {
            let bin = net2.states[bi].r#in as i32;
            if bin != EPSILON && !g_flag_is_epsilon {
                bi += 1;
                continue;
            }

            let bout = net2.states[bi].out as i32;

            if g_flag_is_epsilon && bin != -1 && is_flag[bin as usize] {
                let btarget = net2.states[bi].target;
                let target_number = match triplet_hash_find(&th, a, btarget, 1) {
                    Some(n) => n,
                    None => {
                        /* STACK_3_PUSH(1, machine_b->target, a) */
                        int_stack.push(1);
                        int_stack.push(btarget);
                        int_stack.push(a);
                        triplet_hash_insert(&mut th, a, btarget, 1)
                    }
                };
                fsm_state_add_arc(
                    &mut builder,
                    current_state,
                    bin,
                    bout,
                    target_number,
                    current_final,
                    current_start,
                );
            }

            if !g_compose_tristate {
                /* Check 0:A arcs on lower side */
                if bin == EPSILON {
                    /* mode -> 1 */
                    let btarget = net2.states[bi].target;
                    let target_number = match triplet_hash_find(&th, a, btarget, 1) {
                        Some(n) => n,
                        None => {
                            /* STACK_3_PUSH(1, machine_b->target, a) */
                            int_stack.push(1);
                            int_stack.push(btarget);
                            int_stack.push(a);
                            triplet_hash_insert(&mut th, a, btarget, 1)
                        }
                    };

                    fsm_state_add_arc(
                        &mut builder,
                        current_state,
                        EPSILON,
                        bout,
                        target_number,
                        current_final,
                        current_start,
                    );
                }
            } else if g_compose_tristate {
                /* Check 0:A arcs on lower side */
                if bin == EPSILON && mode != 1 {
                    /* mode -> 1 */
                    let btarget = net2.states[bi].target;
                    let target_number = match triplet_hash_find(&th, a, btarget, 2) {
                        Some(n) => n,
                        None => {
                            /* STACK_3_PUSH(2, machine_b->target, a) */
                            int_stack.push(2);
                            int_stack.push(btarget);
                            int_stack.push(a);
                            triplet_hash_insert(&mut th, a, btarget, 2)
                        }
                    };

                    fsm_state_add_arc(
                        &mut builder,
                        current_state,
                        EPSILON,
                        bout,
                        target_number,
                        current_final,
                        current_start,
                    );
                }
            }
            bi += 1;
        }
        fsm_state_end_state(&mut builder);
    }

    /* free(net1->states) */
    net1.states = Vec::new();
    fsm_destroy(net2);
    fsm_state_close(&mut builder, &mut net1);
    /* free(point_a); free(point_b); free(index); free(outarray) */
    drop(point_a);
    drop(point_b);
    drop(index);
    drop(outarray);

    if g_flag_is_epsilon {
        /* free(is_flag) */
        drop(is_flag);
    }
    triplet_hash_free(Some(th));
    let net1 = fsm_topsort(fsm_coaccessible(net1));
    fsm_coaccessible(net1)
}

// [spec:foma:def:constructions.fsm-cross-product-fn]
// [spec:foma:sem:constructions.fsm-cross-product-fn]
// [spec:foma:def:fomalib.fsm-cross-product-fn]
// [spec:foma:sem:fomalib.fsm-cross-product-fn]
pub fn fsm_cross_product(opts: &FomaOptions, net1: Box<Fsm>, net2: Box<Fsm>) -> Box<Fsm> {
    let mut int_stack = IntStack::new();
    /* Perform a cross product by running two machines in parallel */
    /* The approach here allows a state to stay, creating a a:0 or 0:b transition */
    /* with the a/b-state waiting, and the arc going to {a,stay} or {stay,b} */
    /* the wait maneuver is only possible if the waiting state is final */

    /* For the rewrite rules compilation, a different cross-product is used:  */
    /* rewrite_cp() synchronizes A and B as long as possible to get a unique  */
    /* output match for each cross product.                                   */

    /* This behavior where we postpone zeroes on either side and perform */
    /* and equal length cross-product as long as possible and never intermix */
    /* ?:0 and 0:? arcs (i.e. we keep both machines synchronized as long as possible */
    /* can be done by [A .x. B] & ?:?* [?:0*|0:?*] at the cost of possibly */
    /* up to three times larger transducers. */
    /* This is very similar to the idea in "tristate composition" in fsm_compose() */

    /* This function is only used for explicit cross products */
    /* such as a:b or A.x.B, etc.  In rewrite rules, we use rewrite_cp() */

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

        let mut ai = point_a[a as usize].transitions;
        while net1.states[ai].state_no == a {
            let mut bi = point_b[b as usize].transitions;
            while net2.states[bi].state_no == b {
                if net1.states[ai].target == -1 && net2.states[bi].target == -1 {
                    bi += 1;
                    continue;
                }
                if net1.states[ai].target == -1 && net1.states[ai].final_state == 0 {
                    bi += 1;
                    continue;
                }
                if net2.states[bi].target == -1 && net2.states[bi].final_state == 0 {
                    bi += 1;
                    continue;
                }
                /* Main check */
                if !(net1.states[ai].target == -1 || net2.states[bi].target == -1) {
                    let atarget = net1.states[ai].target;
                    let btarget = net2.states[bi].target;
                    let mut target_number = triplet_hash_find(&th, atarget, btarget, 0);
                    if target_number.is_none() {
                        /* STACK_2_PUSH(machine_b->target, machine_a->target) */
                        int_stack.push(btarget);
                        int_stack.push(atarget);
                        target_number = Some(triplet_hash_insert(&mut th, atarget, btarget, 0));
                    }
                    let target_number = target_number.expect("found or just inserted");
                    let mut symbol1 = net1.states[ai].r#in as i32;
                    let mut symbol2 = net2.states[bi].r#in as i32;
                    if symbol1 == IDENTITY && symbol2 != IDENTITY {
                        symbol1 = UNKNOWN;
                    }
                    if symbol2 == IDENTITY && symbol1 != IDENTITY {
                        symbol2 = UNKNOWN;
                    }

                    fsm_state_add_arc(
                        &mut builder,
                        current_state,
                        symbol1,
                        symbol2,
                        target_number,
                        current_final,
                        current_start,
                    );
                    /* @:@ -> @:@ and also ?:? */
                    if net1.states[ai].r#in as i32 == IDENTITY
                        && net2.states[bi].r#in as i32 == IDENTITY
                    {
                        fsm_state_add_arc(
                            &mut builder,
                            current_state,
                            UNKNOWN,
                            UNKNOWN,
                            target_number,
                            current_final,
                            current_start,
                        );
                    }
                }
                if net1.states[ai].final_state == 1 && net2.states[bi].target != -1 {
                    /* Add 0:b i.e. stay in state A */
                    let astate = net1.states[ai].state_no;
                    let btarget = net2.states[bi].target;
                    let mut target_number = triplet_hash_find(&th, astate, btarget, 0);
                    if target_number.is_none() {
                        /* STACK_2_PUSH(machine_b->target, machine_a->state_no) */
                        int_stack.push(btarget);
                        int_stack.push(astate);
                        target_number = Some(triplet_hash_insert(&mut th, astate, btarget, 0));
                    }
                    let target_number = target_number.expect("found or just inserted");
                    /* @:0 becomes ?:0 */
                    let symbol2 = if net2.states[bi].r#in as i32 == IDENTITY {
                        UNKNOWN
                    } else {
                        net2.states[bi].r#in as i32
                    };
                    fsm_state_add_arc(
                        &mut builder,
                        current_state,
                        EPSILON,
                        symbol2,
                        target_number,
                        current_final,
                        current_start,
                    );
                }

                if net2.states[bi].final_state == 1 && net1.states[ai].target != -1 {
                    /* Add a:0 i.e. stay in state B */
                    let atarget = net1.states[ai].target;
                    let bstate = net2.states[bi].state_no;
                    let mut target_number = triplet_hash_find(&th, atarget, bstate, 0);
                    if target_number.is_none() {
                        /* STACK_2_PUSH(machine_b->state_no, machine_a->target) */
                        int_stack.push(bstate);
                        int_stack.push(atarget);
                        target_number = Some(triplet_hash_insert(&mut th, atarget, bstate, 0));
                    }
                    let target_number = target_number.expect("found or just inserted");
                    /* @:0 becomes ?:0 */
                    let symbol1 = if net1.states[ai].r#in as i32 == IDENTITY {
                        UNKNOWN
                    } else {
                        net1.states[ai].r#in as i32
                    };
                    fsm_state_add_arc(
                        &mut builder,
                        current_state,
                        symbol1,
                        EPSILON,
                        target_number,
                        current_final,
                        current_start,
                    );
                }
                bi += 1;
            }
            ai += 1;
        }
        /* Check arctrack */
        fsm_state_end_state(&mut builder);
    }

    /* free(net1->states) */
    net1.states = Vec::new();
    fsm_state_close(&mut builder, &mut net1);

    let mut epsilon = 0;
    let mut unknown = 0;
    let mut i = 0usize;
    while net1.states[i].state_no != -1 {
        if net1.states[i].r#in as i32 == EPSILON || net1.states[i].out as i32 == EPSILON {
            epsilon = 1;
        }
        if net1.states[i].r#in as i32 == UNKNOWN || net1.states[i].out as i32 == UNKNOWN {
            unknown = 1;
        }
        i += 1;
    }
    if epsilon == 1 && sigma_find_number(EPSILON, &net1.sigma).is_none() {
        sigma_add_special(EPSILON, &mut net1.sigma);
    }
    if unknown == 1 && sigma_find_number(UNKNOWN, &net1.sigma).is_none() {
        sigma_add_special(UNKNOWN, &mut net1.sigma);
    }
    /* free(point_a); free(point_b) */
    drop(point_a);
    drop(point_b);
    fsm_destroy(net2);
    triplet_hash_free(Some(th));
    fsm_coaccessible(net1)
}
