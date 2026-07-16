//! foma/iface.c Wave-4 split: apply/enumeration commands (apply up/down/med/
//! file, random-*, words, pairs, shortest-string). See iface/mod.rs.
use super::*;

// [spec:foma:def:iface.iface-apply-set-params-fn]
// [spec:foma:sem:iface.iface-apply-set-params-fn]
// [spec:foma:def:foma.iface-apply-set-params-fn]
// [spec:foma:sem:foma.iface-apply-set-params-fn]
pub fn iface_apply_set_params(opts: &FomaOptions, h: &mut ApplyHandle) {
    apply_set_print_space(h, opts.print_space as i32);
    apply_set_print_pairs(h, opts.print_pairs as i32);
    apply_set_show_flags(h, opts.show_flags as i32);
    apply_set_obey_flags(h, opts.obey_flags as i32);
}

// [spec:foma:def:iface.iface-apply-med-fn]
// [spec:foma:sem:iface.iface-apply-med-fn]
// [spec:foma:def:foma.iface-apply-med-fn]
// [spec:foma:sem:foma.iface-apply-med-fn]
pub fn iface_apply_med(session: &mut Session, word: &str) {
    if !iface_stack_check(session, 1) {
        return;
    }
    // amedh = stack_get_med_ah() — arena index of the top entry (see module notes)
    let Some(amedh) = session.stack_get_med_ah() else {
        return;
    };

    session.stack_entry_amedh_with_opts(amedh, |opts, h| {
        apply_med_set_heap_max(h, 4194304 + 1);
        apply_med_set_med_limit(h, opts.med_limit);
        apply_med_set_med_cutoff(h, opts.med_cutoff);
    });

    let result = session.stack_entry_amedh(amedh, |h| apply_med(h, Some(word)));
    match result {
        None => {
            println!("???");
            return;
        }
        Some(r) => {
            println!("{}", r);
            let (instr, cost) = session.stack_entry_amedh(amedh, |h| {
                (apply_med_get_instring(h), apply_med_get_cost(h))
            });
            println!("{}", instr.unwrap_or_default());
            print!("Cost[f]: {}\n\n", cost);
        }
    }
    loop {
        let result = session.stack_entry_amedh(amedh, |h| apply_med(h, None));
        match result {
            None => break,
            Some(r) => {
                println!("{}", r);
                let (instr, cost) = session.stack_entry_amedh(amedh, |h| {
                    (apply_med_get_instring(h), apply_med_get_cost(h))
                });
                println!("{}", instr.unwrap_or_default());
                print!("Cost[f]: {}\n\n", cost);
            }
        }
    }
}

// [spec:foma:def:iface.iface-apply-file-fn]
// [spec:foma:sem:iface.iface-apply-file-fn]
// [spec:foma:def:foma.iface-apply-file-fn]
// [spec:foma:sem:foma.iface-apply-file-fn]
pub fn iface_apply_file(
    session: &mut Session,
    infilename: &str,
    outfilename: Option<&str>,
    direction: i32,
) -> bool {
    // C read input with fgets(line, 8192, INFILE); read_line below reads whole
    // logical lines instead, so there is no fixed buffer size to honor.
    if direction != AP_D && direction != AP_U {
        perror("Invalid direction in iface_apply_file().\n");
        return false;
    }
    if !iface_stack_check(session, 1) {
        return true;
    }
    let infile = match File::open(infilename) {
        Ok(f) => f,
        Err(_) => {
            eprint!("{}: ", infilename);
            perror("Error opening file");
            return false;
        }
    };

    // C: OUTFILE = fopen(outfilename, "w") happens BEFORE the "Writing output to
    // file" message, which is BEFORE the NULL check — so the message prints even
    // when the open fails.
    let mut outfile: Output = match outfilename {
        None => Output::Stdout(std::io::stdout()),
        Some(name) => {
            let res = File::create(name);
            println!("Writing output to file {}.", name);
            match res {
                Ok(f) => Output::File(f),
                Err(_) => {
                    eprint!("{}: ", name);
                    perror("Error opening output file.");
                    return false;
                }
            }
        }
    };

    let Some(ah) = session.stack_get_ah() else {
        return true;
    };
    session.stack_entry_ah_with_opts(ah, iface_apply_set_params);

    let mut reader = BufReader::new(infile);
    let mut inword = String::new();
    loop {
        inword.clear();
        // fgets: NULL at EOF. read_line returns Ok(0) at EOF.
        // DEVIATION from C: read_line requires UTF-8; a decode error is treated as
        // end-of-input here (C reads raw bytes).
        let n = reader.read_line(&mut inword).unwrap_or(0);
        if n == 0 {
            break;
        }
        // C: if (inword[strlen(inword)-1] == '\n') inword[strlen-1] = '\0';
        // DEVIATION from C (on a line whose first byte is NUL, strlen==0 and the C
        // indexes inword[-1] — OOB; guard non-empty and strip a trailing '\n').
        if !inword.is_empty() && inword.as_bytes()[inword.len() - 1] == b'\n' {
            inword.pop();
        }

        write!(outfile, "\n{}\n", inword).expect("writing apply-file output");
        let result = if direction == AP_D {
            session.stack_entry_ah(ah, |h| apply_down(h, Some(&inword)))
        } else {
            session.stack_entry_ah(ah, |h| apply_up(h, Some(&inword)))
        };

        let result = match result {
            None => {
                writeln!(outfile, "???").expect("writing apply-file output");
                continue;
            }
            Some(r) => r,
        };
        writeln!(outfile, "{}", result).expect("writing apply-file output");
        loop {
            let result = if direction == AP_D {
                session.stack_entry_ah(ah, |h| apply_down(h, None))
            } else {
                session.stack_entry_ah(ah, |h| apply_up(h, None))
            };
            match result {
                None => break,
                Some(r) => {
                    writeln!(outfile, "{}", r).expect("writing apply-file output");
                }
            }
        }
    }
    // C: fclose(OUTFILE) only when outfilename != NULL; the input file is never
    // fclose'd (latent leak). Rust drops (closes) both at scope end; stdout is not
    // closed. The observable difference (leak vs. drop) is unrepresentable safely.
    true
}

// [spec:foma:def:iface.iface-apply-down-fn]
// [spec:foma:sem:iface.iface-apply-down-fn]
// [spec:foma:def:foma.iface-apply-down-fn]
// [spec:foma:sem:foma.iface-apply-down-fn]
pub fn iface_apply_down(session: &mut Session, word: &str) {
    if !iface_stack_check(session, 1) {
        return;
    }
    let Some(ah) = session.stack_get_ah() else {
        return;
    };
    session.stack_entry_ah_with_opts(ah, iface_apply_set_params);
    let result = session.stack_entry_ah(ah, |h| apply_down(h, Some(word)));
    match result {
        None => {
            println!("???");
            return;
        }
        Some(r) => {
            println!("{}", r);
        }
    }
    let mut i = session.opts.list_limit;
    while i > 0 {
        let result = session.stack_entry_ah(ah, |h| apply_down(h, None));
        match result {
            None => break,
            Some(r) => println!("{}", r),
        }
        i -= 1;
    }
}

// [spec:foma:def:iface.iface-apply-up-fn]
// [spec:foma:sem:iface.iface-apply-up-fn]
// [spec:foma:def:foma.iface-apply-up-fn]
// [spec:foma:sem:foma.iface-apply-up-fn]
pub fn iface_apply_up(session: &mut Session, word: &str) {
    if !iface_stack_check(session, 1) {
        return;
    }
    let Some(ah) = session.stack_get_ah() else {
        return;
    };
    session.stack_entry_ah_with_opts(ah, iface_apply_set_params);
    let result = session.stack_entry_ah(ah, |h| apply_up(h, Some(word)));
    match result {
        None => {
            println!("???");
            return;
        }
        Some(r) => {
            println!("{}", r);
        }
    }
    let mut i = session.opts.list_limit;
    while i > 0 {
        let result = session.stack_entry_ah(ah, |h| apply_up(h, None));
        match result {
            None => break,
            Some(r) => println!("{}", r),
        }
        i -= 1;
    }
}

// [spec:foma:def:iface.iface-lower-words-fn]
// [spec:foma:sem:iface.iface-lower-words-fn]
// [spec:foma:def:foma.iface-lower-words-fn]
// [spec:foma:sem:foma.iface-lower-words-fn]
pub fn iface_lower_words(session: &mut Session, limit: i32) {
    if !iface_stack_check(session, 1) {
        return;
    }
    let limit = if limit == -1 {
        session.opts.list_limit
    } else {
        limit
    };
    if iface_stack_check(session, 1) {
        let Some(ah) = session.stack_get_ah() else {
            return;
        };
        session.stack_entry_ah_with_opts(ah, iface_apply_set_params);
        let mut i = limit;
        while i > 0 {
            let result = session.stack_entry_ah(ah, apply_lower_words);
            match result {
                None => break,
                Some(r) => println!("{}", r),
            }
            i -= 1;
        }
        session.stack_entry_ah(ah, apply_reset_enumerator);
    }
}

// [spec:foma:def:iface.iface-random-lower-fn]
// [spec:foma:sem:iface.iface-random-lower-fn]
// [spec:foma:def:foma.iface-random-lower-fn]
// [spec:foma:sem:foma.iface-random-lower-fn]
pub fn iface_random_lower(session: &mut Session, limit: i32) {
    iface_apply_random(session, apply_random_lower, limit);
}

// [spec:foma:def:iface.iface-random-upper-fn]
// [spec:foma:sem:iface.iface-random-upper-fn]
// [spec:foma:def:foma.iface-random-upper-fn]
// [spec:foma:sem:foma.iface-random-upper-fn]
pub fn iface_random_upper(session: &mut Session, limit: i32) {
    iface_apply_random(session, apply_random_upper, limit);
}

// [spec:foma:def:iface.iface-random-words-fn]
// [spec:foma:sem:iface.iface-random-words-fn]
// [spec:foma:def:foma.iface-random-words-fn]
// [spec:foma:sem:foma.iface-random-words-fn]
pub fn iface_random_words(session: &mut Session, limit: i32) {
    iface_apply_random(session, apply_random_words, limit);
}

// [spec:foma:def:iface.iface-apply-random-fn]
// [spec:foma:sem:iface.iface-apply-random-fn]
// [spec:foma:def:foma.iface-apply-random-fn]
// [spec:foma:sem:foma.iface-apply-random-fn]
// C: `void iface_apply_random(char *(*applyer)(struct apply_handle *h), int limit)` —
// the applyer function pointer becomes a Rust fn pointer of the same shape.
pub fn iface_apply_random(
    session: &mut Session,
    applyer: fn(&mut ApplyHandle) -> Option<String>,
    limit: i32,
) {
    let limit = if limit == -1 {
        session.opts.list_random_limit
    } else {
        limit
    };
    if iface_stack_check(session, 1) {
        // calloc(limit, sizeof(struct apply_results {char *string; int count;}))
        let mut results: Vec<(Option<String>, i32)> = vec![(None, 0); limit as usize];
        let Some(ah) = session.stack_get_ah() else {
            return;
        };
        session.stack_entry_ah_with_opts(ah, iface_apply_set_params);
        let mut i = limit;
        while i > 0 {
            let result = session.stack_entry_ah(ah, applyer);
            if let Some(result) = result {
                for slot in results.iter_mut() {
                    if slot.0.is_none() {
                        // strdup(result)
                        slot.0 = Some(result.clone());
                        slot.1 = 1;
                        break;
                    } else if slot.0.as_deref() == Some(result.as_str()) {
                        slot.1 += 1;
                        break;
                    }
                }
            }
            i -= 1;
        }
        for slot in results.iter() {
            if let Some(s) = &slot.0 {
                println!("[{}] {}", slot.1, s);
                // free(tempresults->string) — String dropped at scope end.
            }
        }
        // free(results) — Vec dropped at scope end.
        session.stack_entry_ah(ah, apply_reset_enumerator);
    }
}

// [spec:foma:def:iface.iface-print-shortest-string-fn]
// [spec:foma:sem:iface.iface-print-shortest-string-fn]
// [spec:foma:def:foma.iface-print-shortest-string-fn]
// [spec:foma:sem:foma.iface-print-shortest-string-fn]
pub fn iface_print_shortest_string(session: &mut Session) {
    /* L -  ?+  [[L .o. [?:"@TMP@"]*].l .o. "@TMP@":?*].l; */
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        let mut one = session.stack_entry_fsm(top, fsm_copy);
        if session.stack_entry_fsm(top, |f| f.arity) == 1 {
            let result = fsm_minimize(
                &session.opts,
                fsm_minus(
                    &session.opts,
                    fsm_copy(&mut one),
                    fsm_concat(
                        &session.opts,
                        fsm_kleene_plus(&session.opts, fsm_identity()),
                        fsm_lower(fsm_compose(
                            &session.opts,
                            fsm_lower(fsm_compose(
                                &session.opts,
                                fsm_copy(&mut one),
                                fsm_kleene_star(
                                    &session.opts,
                                    fsm_cross_product(
                                        &session.opts,
                                        fsm_identity(),
                                        fsm_symbol("@TMP@"),
                                    ),
                                ),
                            )),
                            fsm_kleene_star(
                                &session.opts,
                                fsm_cross_product(
                                    &session.opts,
                                    fsm_symbol("@TMP@"),
                                    fsm_identity(),
                                ),
                            ),
                        )),
                    ),
                ),
            );
            let mut ah = apply_init(&result);
            let word = apply_words(&mut ah);
            if let Some(w) = &word {
                println!("{}", w);
            }
            apply_clear(ah);
            fsm_destroy(result);
            // C leaks the initial fsm_copy `one` here; dropped (freed) at scope end.
        } else {
            let mut onel = fsm_lower(fsm_copy(&mut one));
            let mut oneu = fsm_upper(one);
            let result_u = fsm_minimize(
                &session.opts,
                fsm_minus(
                    &session.opts,
                    fsm_copy(&mut oneu),
                    fsm_concat(
                        &session.opts,
                        fsm_kleene_plus(&session.opts, fsm_identity()),
                        fsm_lower(fsm_compose(
                            &session.opts,
                            fsm_lower(fsm_compose(
                                &session.opts,
                                fsm_copy(&mut oneu),
                                fsm_kleene_star(
                                    &session.opts,
                                    fsm_cross_product(
                                        &session.opts,
                                        fsm_identity(),
                                        fsm_symbol("@TMP@"),
                                    ),
                                ),
                            )),
                            fsm_kleene_star(
                                &session.opts,
                                fsm_cross_product(
                                    &session.opts,
                                    fsm_symbol("@TMP@"),
                                    fsm_identity(),
                                ),
                            ),
                        )),
                    ),
                ),
            );
            let result_l = fsm_minimize(
                &session.opts,
                fsm_minus(
                    &session.opts,
                    fsm_copy(&mut onel),
                    fsm_concat(
                        &session.opts,
                        fsm_kleene_plus(&session.opts, fsm_identity()),
                        fsm_lower(fsm_compose(
                            &session.opts,
                            fsm_lower(fsm_compose(
                                &session.opts,
                                fsm_copy(&mut onel),
                                fsm_kleene_star(
                                    &session.opts,
                                    fsm_cross_product(
                                        &session.opts,
                                        fsm_identity(),
                                        fsm_symbol("@TMP@"),
                                    ),
                                ),
                            )),
                            fsm_kleene_star(
                                &session.opts,
                                fsm_cross_product(
                                    &session.opts,
                                    fsm_symbol("@TMP@"),
                                    fsm_identity(),
                                ),
                            ),
                        )),
                    ),
                ),
            );
            fsm_destroy(oneu);
            fsm_destroy(onel);
            let mut ah = apply_init(&result_u);
            let word = apply_words(&mut ah);
            // C: if (word == NULL) word = ""; printf("Upper: %s\n", word);
            println!("Upper: {}", word.as_deref().unwrap_or(""));
            apply_clear(ah);
            fsm_destroy(result_u);
            let mut ah = apply_init(&result_l);
            let word = apply_words(&mut ah);
            println!("Lower: {}", word.as_deref().unwrap_or(""));
            apply_clear(ah);
            fsm_destroy(result_l);
        }
    }
}

/// Length (in symbols) of the SHORTEST accepted string of a minimized unary
/// acceptor — a BFS over the state graph counting arcs from the start state to
/// the nearest final. The C source reported `statecount - 1`, i.e. the *longest*
/// acyclic path (the minimal unary DFA of {a^k} is a chain of max_len+1 states).
/// Returns 0 for the empty language (no final reachable).
fn shortest_acyclic_length(net: &crate::types::Fsm) -> i32 {
    use std::collections::VecDeque;
    let n = net.statecount as usize;
    let mut adj: Vec<Vec<i32>> = vec![Vec::new(); n];
    let mut is_final = vec![false; n];
    let mut start: i32 = -1;
    let mut i = 0usize;
    while net.states[i].state_no != -1 {
        let s = net.states[i].state_no;
        if net.states[i].start_state != 0 {
            start = s;
        }
        if net.states[i].final_state != 0 {
            is_final[s as usize] = true;
        }
        let t = net.states[i].target;
        if t != -1 {
            adj[s as usize].push(t);
        }
        i += 1;
    }
    if start < 0 {
        return 0;
    }
    let mut dist = vec![-1i32; n];
    let mut q: VecDeque<i32> = VecDeque::new();
    dist[start as usize] = 0;
    q.push_back(start);
    while let Some(u) = q.pop_front() {
        if is_final[u as usize] {
            return dist[u as usize];
        }
        for &v in &adj[u as usize] {
            if dist[v as usize] == -1 {
                dist[v as usize] = dist[u as usize] + 1;
                q.push_back(v);
            }
        }
    }
    0
}

// [spec:foma:def:iface.iface-print-shortest-string-size-fn]
// [spec:foma:sem:iface.iface-print-shortest-string-size-fn+1]
// [spec:foma:def:foma.iface-print-shortest-string-size-fn]
// [spec:foma:sem:foma.iface-print-shortest-string-size-fn+1]
pub fn iface_print_shortest_string_size(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        let mut one = session.stack_entry_fsm(top, fsm_copy);
        /* [L .o. [?:a]*].l; */
        if session.stack_entry_fsm(top, |f| f.arity) == 1 {
            let result = fsm_minimize(
                &session.opts,
                fsm_lower(fsm_compose(
                    &session.opts,
                    one,
                    fsm_kleene_star(
                        &session.opts,
                        fsm_cross_product(&session.opts, fsm_identity(), fsm_symbol("a")),
                    ),
                )),
            );
            println!(
                "Shortest acyclic path length: {}",
                shortest_acyclic_length(&result)
            );
            // Result net never fsm_destroy'd in C (leak); dropped at scope end.
        } else {
            let onel = fsm_lower(fsm_copy(&mut one));
            let oneu = fsm_upper(one);
            let result_u = fsm_minimize(
                &session.opts,
                fsm_lower(fsm_compose(
                    &session.opts,
                    oneu,
                    fsm_kleene_star(
                        &session.opts,
                        fsm_cross_product(&session.opts, fsm_identity(), fsm_symbol("a")),
                    ),
                )),
            );
            let result_l = fsm_minimize(
                &session.opts,
                fsm_lower(fsm_compose(
                    &session.opts,
                    onel,
                    fsm_kleene_star(
                        &session.opts,
                        fsm_cross_product(&session.opts, fsm_identity(), fsm_symbol("a")),
                    ),
                )),
            );
            println!(
                "Shortest acyclic upper path length: {}",
                shortest_acyclic_length(&result_u)
            );
            println!(
                "Shortest acyclic lower path length: {}",
                shortest_acyclic_length(&result_l)
            );
        }
    }
}

// [spec:foma:def:iface.iface-upper-words-fn]
// [spec:foma:sem:iface.iface-upper-words-fn]
// [spec:foma:def:foma.iface-upper-words-fn]
// [spec:foma:sem:foma.iface-upper-words-fn]
pub fn iface_upper_words(session: &mut Session, limit: i32) {
    let limit = if limit == -1 {
        session.opts.list_limit
    } else {
        limit
    };
    if iface_stack_check(session, 1) {
        let Some(ah) = session.stack_get_ah() else {
            return;
        };
        session.stack_entry_ah_with_opts(ah, iface_apply_set_params);
        let mut i = limit;
        while i > 0 {
            let result = session.stack_entry_ah(ah, apply_upper_words);
            match result {
                None => break,
                Some(r) => println!("{}", r),
            }
            i -= 1;
        }
        session.stack_entry_ah(ah, apply_reset_enumerator);
    }
}

// [spec:foma:def:iface.iface-words-file-fn]
// [spec:foma:sem:iface.iface-words-file-fn+1]
// [spec:foma:def:foma.iface-words-file-fn]
// [spec:foma:sem:foma.iface-words-file-fn+1]
pub fn iface_words_file(session: &mut Session, filename: &str, r#type: i32) {
    /* type 0 (words), 1 (upper-words), 2 (lower-words) */
    // Wave 4 fix: the C kept the applyer in a function-local `static`, so a type-0
    // call after any type-1/2 call reused the stale upper/lower enumerator. Select
    // the enumerator fresh from `type` on every call instead.
    let applyer: fn(&mut ApplyHandle) -> Option<String> = if r#type == 1 {
        apply_upper_words
    } else if r#type == 2 {
        apply_lower_words
    } else {
        apply_words
    };
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        if session.stack_entry_fsm(top, |f| f.pathcount) == PATHCOUNT_CYCLIC {
            println!("FSM is cyclic: can't write all words to file.");
            return;
        }
        println!("Writing to {}.", filename);
        let mut outfile = match File::create(filename) {
            Ok(f) => f,
            Err(_) => {
                perror("Error opening file");
                return;
            }
        };
        let Some(ah) = session.stack_get_ah() else {
            return;
        };
        session.stack_entry_ah_with_opts(ah, iface_apply_set_params);
        loop {
            let result = session.stack_entry_ah(ah, applyer);
            match result {
                None => break,
                Some(r) => {
                    writeln!(outfile, "{}", r).expect("writing words-file output");
                }
            }
        }
        session.stack_entry_ah(ah, apply_reset_enumerator);
        // fclose(outfile) — dropped at scope end.
    }
}

// [spec:foma:def:iface.iface-words-fn]
// [spec:foma:sem:iface.iface-words-fn]
// [spec:foma:def:foma.iface-words-fn]
// [spec:foma:sem:foma.iface-words-fn]
pub fn iface_words(session: &mut Session, limit: i32) {
    let limit = if limit == -1 {
        session.opts.list_limit
    } else {
        limit
    };
    if iface_stack_check(session, 1) {
        let Some(ah) = session.stack_get_ah() else {
            return;
        };
        session.stack_entry_ah_with_opts(ah, iface_apply_set_params);
        let mut i = limit;
        while i > 0 {
            let result = session.stack_entry_ah(ah, apply_words);
            match result {
                None => break,
                Some(r) => println!("{}", r),
            }
            i -= 1;
        }
        session.stack_entry_ah(ah, apply_reset_enumerator);
    }
}

// [spec:foma:def:iface.iface-pairs-call-fn]
// [spec:foma:sem:iface.iface-pairs-call-fn]
pub fn iface_pairs_call(session: &mut Session, limit: i32, random: i32) {
    let limit = if limit == -1 {
        session.opts.list_limit
    } else {
        limit
    };
    if iface_stack_check(session, 1) {
        let Some(ah) = session.stack_get_ah() else {
            return;
        };
        session.stack_entry_ah_with_opts(ah, |opts, h| {
            apply_set_show_flags(h, opts.show_flags as i32)
        });
        session.stack_entry_ah_with_opts(ah, |opts, h| {
            apply_set_obey_flags(h, opts.obey_flags as i32)
        });
        /* C encoded the pair split in-band by setting space/epsilon/separator
        to bytes 0x01/0x02/0x03 and re-splitting the rendered string; the
        handle records structured segments instead. */
        session.stack_entry_ah(ah, |h| apply_set_collect_pairs(h, true));
        let mut i = limit;
        while i > 0 {
            let result = if random == 1 {
                session.stack_entry_ah(ah, apply_random_words)
            } else {
                session.stack_entry_ah(ah, apply_words)
            };
            if result.is_none() {
                break;
            }
            let (upper, lower) = session.stack_entry_ah(ah, |h| apply_last_pairs(h));
            println!("{}\t{}", upper, lower);
            i -= 1;
        }
        session.stack_entry_ah(ah, |h| apply_set_collect_pairs(h, false));
        session.stack_entry_ah(ah, apply_reset_enumerator);
    }
}

// [spec:foma:def:iface.iface-random-pairs-fn]
// [spec:foma:sem:iface.iface-random-pairs-fn+1]
// [spec:foma:def:foma.iface-random-pairs-fn]
// [spec:foma:sem:foma.iface-random-pairs-fn+1]
pub fn iface_random_pairs(session: &mut Session, limit: i32) {
    // Wave 4 fix: the C passed limit straight through, so limit == -1 resolved to
    // g_list_limit inside iface_pairs_call instead of g_list_random_limit like the
    // other random commands. Resolve -1 to g_list_random_limit here first.
    let limit = if limit == -1 {
        session.opts.list_random_limit
    } else {
        limit
    };
    iface_pairs_call(session, limit, 1);
}

// [spec:foma:def:iface.iface-pairs-fn]
// [spec:foma:sem:iface.iface-pairs-fn]
// [spec:foma:def:foma.iface-pairs-fn]
// [spec:foma:sem:foma.iface-pairs-fn]
pub fn iface_pairs(session: &mut Session, limit: i32) {
    iface_pairs_call(session, limit, 0);
}

// [spec:foma:def:iface.iface-pairs-file-fn]
// [spec:foma:sem:iface.iface-pairs-file-fn]
// [spec:foma:def:foma.iface-pairs-file-fn]
// [spec:foma:sem:foma.iface-pairs-file-fn]
pub fn iface_pairs_file(session: &mut Session, filename: &str) {
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        if session.stack_entry_fsm(top, |f| f.pathcount) == PATHCOUNT_CYCLIC {
            println!("FSM is cyclic: can't write all pairs to file.");
            return;
        }
        println!("Writing to {}.", filename);
        let mut outfile = match File::create(filename) {
            Ok(f) => f,
            Err(_) => {
                perror("Error opening file");
                return;
            }
        };
        let Some(ah) = session.stack_get_ah() else {
            return;
        };
        session.stack_entry_ah_with_opts(ah, |opts, h| {
            apply_set_show_flags(h, opts.show_flags as i32)
        });
        session.stack_entry_ah_with_opts(ah, |opts, h| {
            apply_set_obey_flags(h, opts.obey_flags as i32)
        });
        session.stack_entry_ah(ah, |h| apply_set_collect_pairs(h, true));
        loop {
            let result = session.stack_entry_ah(ah, apply_words);
            if result.is_none() {
                break;
            }
            let (upper, lower) = session.stack_entry_ah(ah, |h| apply_last_pairs(h));
            writeln!(outfile, "{}\t{}", upper, lower).expect("writing pairs to file");
        }
        session.stack_entry_ah(ah, |h| apply_set_collect_pairs(h, false));
        session.stack_entry_ah(ah, apply_reset_enumerator);
        // fclose(outfile) — dropped at scope end.
    }
}

#[cfg(test)]
mod tests {
    use super::shortest_acyclic_length;
    use crate::dynarray::{
        fsm_construct_add_arc, fsm_construct_done, fsm_construct_init, fsm_construct_set_final,
        fsm_construct_set_initial,
    };

    // [spec:foma:sem:iface.iface-print-shortest-string-size-fn+1/test]
    // [spec:foma:sem:foma.iface-print-shortest-string-size-fn+1/test]
    #[test]
    fn shortest_acyclic_length_returns_the_shortest_not_longest() {
        // Final state 1 is reachable in 1 arc (0 -a-> 1) and in 3 arcs
        // (0 -b-> 2 -b-> 3 -b-> 1). C reported statecount-1 (the LONGEST acyclic
        // path); the BFS reports the shortest.
        let mut h = fsm_construct_init("s");
        fsm_construct_add_arc(&mut h, 0, 1, "a", "a");
        fsm_construct_add_arc(&mut h, 0, 2, "b", "b");
        fsm_construct_add_arc(&mut h, 2, 3, "b", "b");
        fsm_construct_add_arc(&mut h, 3, 1, "b", "b");
        fsm_construct_set_final(&mut h, 1);
        fsm_construct_set_initial(&mut h, 0);
        let net = fsm_construct_done(h);
        assert_eq!(shortest_acyclic_length(&net), 1);
    }
}
