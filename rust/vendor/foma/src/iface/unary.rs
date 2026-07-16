//! foma/iface.c Wave-4 split: single-net commands (minimize/determinize/
//! invert/reverse/…/extract/sort). See iface/mod.rs.
use super::*;

// [spec:foma:def:iface.iface-ambiguous-upper-fn]
// [spec:foma:sem:iface.iface-ambiguous-upper-fn]
// [spec:foma:def:foma.iface-ambiguous-upper-fn]
// [spec:foma:sem:foma.iface-ambiguous-upper-fn]
pub fn iface_ambiguous_upper(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_extract_ambiguous_domain(&session.opts, popped));
    }
}

// [spec:foma:def:iface.iface-close-fn]
// [spec:foma:sem:iface.iface-close-fn]
// [spec:foma:def:foma.iface-close-fn]
// [spec:foma:sem:foma.iface-close-fn]
pub fn iface_close(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_minimize(
            &session.opts,
            fsm_close_sigma(&session.opts, popped, 0),
        )));
    }
}

// [spec:foma:def:iface.iface-compact-fn]
// [spec:foma:sem:iface.iface-compact-fn]
// [spec:foma:def:foma.iface-compact-fn]
// [spec:foma:sem:foma.iface-compact-fn]
pub fn iface_compact(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        session.stack_entry_fsm(top, fsm_compact);
        session.stack_entry_fsm(top, sigma_sort);
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_minimize(&session.opts, popped)));
    }
}

// [spec:foma:def:iface.iface-complete-fn]
// [spec:foma:sem:iface.iface-complete-fn]
// [spec:foma:def:foma.iface-complete-fn]
// [spec:foma:sem:foma.iface-complete-fn]
pub fn iface_complete(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_complete(&session.opts, popped));
    }
}

// [spec:foma:def:iface.iface-determinize-fn]
// [spec:foma:sem:iface.iface-determinize-fn]
// [spec:foma:def:foma.iface-determinize-fn]
// [spec:foma:sem:foma.iface-determinize-fn]
pub fn iface_determinize(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_determinize(popped));
    }
}

// [spec:foma:def:iface.iface-eliminate-flags-fn]
// [spec:foma:sem:iface.iface-eliminate-flags-fn]
// [spec:foma:def:foma.iface-eliminate-flags-fn]
// [spec:foma:sem:foma.iface-eliminate-flags-fn]
pub fn iface_eliminate_flags(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(flag_eliminate(&session.opts, popped, None));
    }
}

// [spec:foma:def:iface.iface-extract-ambiguous-fn]
// [spec:foma:sem:iface.iface-extract-ambiguous-fn]
// [spec:foma:def:foma.iface-extract-ambiguous-fn]
// [spec:foma:sem:foma.iface-extract-ambiguous-fn]
pub fn iface_extract_ambiguous(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_extract_ambiguous(&session.opts, popped));
    }
}

// [spec:foma:def:iface.iface-extract-unambiguous-fn]
// [spec:foma:sem:iface.iface-extract-unambiguous-fn]
// [spec:foma:def:foma.iface-extract-unambiguous-fn]
// [spec:foma:sem:foma.iface-extract-unambiguous-fn]
pub fn iface_extract_unambiguous(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_extract_unambiguous(&session.opts, popped));
    }
}

// [spec:foma:def:iface.iface-extract-number-fn]
// [spec:foma:sem:iface.iface-extract-number-fn+1]
// [spec:foma:def:foma.iface-extract-number-fn]
// [spec:foma:sem:foma.iface-extract-number-fn+1]
pub fn iface_extract_number(s: &str) -> i32 {
    // Scan to the first ASCII digit (compared as unsigned char), then atoi.
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && (bytes[i] < b'0' || bytes[i] > b'9') {
        i += 1;
    }
    // Wave 4 fix: the C scan stopped AT the first digit, dropping a leading '-'
    // so negatives read positive ("abc-5" -> 5). Include a '-' immediately before
    // the first digit so the sign is parsed ("abc-5" -> -5).
    if i > 0 && i < bytes.len() && bytes[i - 1] == b'-' {
        i -= 1;
    }
    crate::io::parse_leading_i32(&s[i..])
}

// [spec:foma:def:iface.iface-eliminate-flag-fn]
// [spec:foma:sem:iface.iface-eliminate-flag-fn]
// [spec:foma:def:foma.iface-eliminate-flag-fn]
// [spec:foma:sem:foma.iface-eliminate-flag-fn]
pub fn iface_eliminate_flag(session: &mut Session, name: &str) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(flag_eliminate(&session.opts, popped, Some(name)));
    }
}

// [spec:foma:def:iface.iface-factorize-fn]
// [spec:foma:sem:iface.iface-factorize-fn]
// [spec:foma:def:foma.iface-factorize-fn]
// [spec:foma:sem:foma.iface-factorize-fn]
pub fn iface_factorize(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_bimachine(popped));
    }
}

// [spec:foma:def:iface.iface-sequentialize-fn]
// [spec:foma:sem:iface.iface-sequentialize-fn]
// [spec:foma:def:foma.iface-sequentialize-fn]
// [spec:foma:sem:foma.iface-sequentialize-fn]
pub fn iface_sequentialize(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_sequentialize(popped));
    }
}

// [spec:foma:def:iface.iface-invert-fn]
// [spec:foma:sem:iface.iface-invert-fn]
// [spec:foma:def:foma.iface-invert-fn]
// [spec:foma:sem:foma.iface-invert-fn]
pub fn iface_invert(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_invert(popped));
    }
}

// [spec:foma:def:iface.iface-label-net-fn]
// [spec:foma:sem:iface.iface-label-net-fn]
// [spec:foma:def:foma.iface-label-net-fn]
// [spec:foma:sem:foma.iface-label-net-fn]
pub fn iface_label_net(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_sigma_pairs_net(popped));
    }
}

// [spec:foma:def:iface.iface-letter-machine-fn]
// [spec:foma:sem:iface.iface-letter-machine-fn]
// [spec:foma:def:foma.iface-letter-machine-fn]
// [spec:foma:sem:foma.iface-letter-machine-fn]
pub fn iface_letter_machine(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_minimize(
            &session.opts,
            fsm_letter_machine(&session.opts, popped),
        )));
    }
}

// [spec:foma:def:iface.iface-lower-side-fn]
// [spec:foma:sem:iface.iface-lower-side-fn]
// [spec:foma:def:foma.iface-lower-side-fn]
// [spec:foma:sem:foma.iface-lower-side-fn]
pub fn iface_lower_side(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_minimize(&session.opts, fsm_lower(popped))));
    }
}

// [spec:foma:def:iface.iface-minimize-fn]
// [spec:foma:sem:iface.iface-minimize-fn]
// [spec:foma:def:foma.iface-minimize-fn]
// [spec:foma:sem:foma.iface-minimize-fn]
pub fn iface_minimize(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let store_minimal_var = session.opts.minimal;
        session.opts.minimal = true;
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_minimize(&session.opts, popped)));
        session.opts.minimal = store_minimal_var;
    }
}

// [spec:foma:def:iface.iface-one-plus-fn]
// [spec:foma:sem:iface.iface-one-plus-fn]
// [spec:foma:def:foma.iface-one-plus-fn]
// [spec:foma:sem:foma.iface-one-plus-fn]
pub fn iface_one_plus(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_minimize(
            &session.opts,
            fsm_kleene_plus(&session.opts, popped),
        )));
    }
}

// [spec:foma:def:iface.iface-negate-fn]
// [spec:foma:sem:iface.iface-negate-fn]
// [spec:foma:def:foma.iface-negate-fn]
// [spec:foma:sem:foma.iface-negate-fn]
pub fn iface_negate(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_minimize(
            &session.opts,
            fsm_complement(&session.opts, popped),
        )));
    }
}

// [spec:foma:def:iface.iface-prune-fn]
// [spec:foma:sem:iface.iface-prune-fn]
// [spec:foma:def:foma.iface-prune-fn]
// [spec:foma:sem:foma.iface-prune-fn]
pub fn iface_prune(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_coaccessible(popped)));
    }
}

// [spec:foma:def:iface.iface-reverse-fn]
// [spec:foma:sem:iface.iface-reverse-fn]
// [spec:foma:def:foma.iface-reverse-fn]
// [spec:foma:sem:foma.iface-reverse-fn]
pub fn iface_reverse(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_determinize(fsm_reverse(popped))));
    }
}

// [spec:foma:def:iface.iface-sigma-net-fn]
// [spec:foma:sem:iface.iface-sigma-net-fn]
// [spec:foma:def:foma.iface-sigma-net-fn]
// [spec:foma:sem:foma.iface-sigma-net-fn]
pub fn iface_sigma_net(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_sigma_net(popped));
    }
}

// [spec:foma:def:iface.iface-sort-input-fn]
// [spec:foma:sem:iface.iface-sort-input-fn]
// [spec:foma:def:foma.iface-sort-input-fn]
// [spec:foma:sem:foma.iface-sort-input-fn]
pub fn iface_sort_input(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        session.stack_entry_fsm(top, |f| fsm_sort_arcs(f, 1));
    }
}

// [spec:foma:def:iface.iface-sort-output-fn]
// [spec:foma:sem:iface.iface-sort-output-fn]
// [spec:foma:def:foma.iface-sort-output-fn]
// [spec:foma:sem:foma.iface-sort-output-fn]
pub fn iface_sort_output(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        session.stack_entry_fsm(top, |f| fsm_sort_arcs(f, 2));
    }
}

// [spec:foma:def:iface.iface-sort-fn]
// [spec:foma:sem:iface.iface-sort-fn]
// [spec:foma:def:foma.iface-sort-fn]
// [spec:foma:sem:foma.iface-sort-fn]
pub fn iface_sort(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        session.stack_entry_fsm(top, sigma_sort);
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(popped));
    }
}

// [spec:foma:def:iface.iface-twosided-flags-fn]
// [spec:foma:sem:iface.iface-twosided-flags-fn]
// [spec:foma:def:foma.iface-twosided-flags-fn]
// [spec:foma:sem:foma.iface-twosided-flags-fn]
pub fn iface_twosided_flags(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(flag_twosided(&session.opts, popped));
    }
}

// [spec:foma:def:iface.iface-upper-side-fn]
// [spec:foma:sem:iface.iface-upper-side-fn]
// [spec:foma:def:foma.iface-upper-side-fn]
// [spec:foma:sem:foma.iface-upper-side-fn]
pub fn iface_upper_side(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_minimize(&session.opts, fsm_upper(popped))));
    }
}

// [spec:foma:def:iface.iface-zero-plus-fn]
// [spec:foma:sem:iface.iface-zero-plus-fn]
// [spec:foma:def:foma.iface-zero-plus-fn]
// [spec:foma:sem:foma.iface-zero-plus-fn]
pub fn iface_zero_plus(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_minimize(
            &session.opts,
            fsm_kleene_star(&session.opts, popped),
        )));
    }
}
