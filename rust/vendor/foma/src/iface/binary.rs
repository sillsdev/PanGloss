//! foma/iface.c Wave-4 split: multi-net commands (compose/concat/union/
//! intersect/crossproduct/ignore/shuffle/substitute). See iface/mod.rs.
use super::*;
use crate::define::defined_networks_init;

// [spec:foma:def:iface.iface-compose-fn]
// [spec:foma:sem:iface.iface-compose-fn]
// [spec:foma:def:foma.iface-compose-fn]
// [spec:foma:sem:foma.iface-compose-fn]
pub fn iface_compose(session: &mut Session) {
    if iface_stack_check(session, 2) {
        while session.stack_size() > 1 {
            let Some(one) = session.stack_pop() else {
                break;
            };
            let Some(two) = session.stack_pop() else {
                break;
            };
            session.stack_add(fsm_topsort(fsm_minimize(
                &session.opts,
                fsm_compose(&session.opts, one, two),
            )));
        }
    }
}

// [spec:foma:def:iface.iface-conc-fn]
// [spec:foma:sem:iface.iface-conc-fn+1]
// [spec:foma:def:foma.iface-conc-fn]
// [spec:foma:sem:foma.iface-conc-fn+1]
pub fn iface_conc(session: &mut Session) {
    if iface_stack_check(session, 2) {
        while session.stack_size() > 1 {
            // Wave 4 fix: the C left a stray debug printf("dd") here — deleted.
            let Some(one) = session.stack_pop() else {
                break;
            };
            let Some(two) = session.stack_pop() else {
                break;
            };
            session.stack_add(fsm_topsort(fsm_minimize(
                &session.opts,
                fsm_concat(&session.opts, one, two),
            )));
        }
    }
}

// [spec:foma:def:iface.iface-crossproduct-fn]
// [spec:foma:sem:iface.iface-crossproduct-fn]
// [spec:foma:def:foma.iface-crossproduct-fn]
// [spec:foma:sem:foma.iface-crossproduct-fn]
pub fn iface_crossproduct(session: &mut Session) {
    if iface_stack_check(session, 2) {
        let Some(one) = session.stack_pop() else {
            return;
        };
        let Some(two) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_minimize(
            &session.opts,
            fsm_cross_product(&session.opts, one, two),
        )));
    }
}

// [spec:foma:def:iface.iface-ignore-fn]
// [spec:foma:sem:iface.iface-ignore-fn]
// [spec:foma:def:foma.iface-ignore-fn]
// [spec:foma:sem:foma.iface-ignore-fn]
pub fn iface_ignore(session: &mut Session) {
    if iface_stack_check(session, 2) {
        let Some(one) = session.stack_pop() else {
            return;
        };
        let Some(two) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_minimize(
            &session.opts,
            fsm_ignore(&session.opts, one, two, OP_IGNORE_ALL),
        )));
    }
}

// [spec:foma:def:iface.iface-intersect-fn]
// [spec:foma:sem:iface.iface-intersect-fn]
// [spec:foma:def:foma.iface-intersect-fn]
// [spec:foma:sem:foma.iface-intersect-fn]
pub fn iface_intersect(session: &mut Session) {
    if iface_stack_check(session, 2) {
        while session.stack_size() > 1 {
            // C: fsm_intersect(stack_pop(), stack_pop()) — the two pops are one
            // expression (C order unspecified); Rust evaluates arguments
            // left-to-right. Intersection is commutative, so the language matches.
            let Some(a) = session.stack_pop() else { break };
            let Some(b) = session.stack_pop() else { break };
            session.stack_add(fsm_topsort(fsm_minimize(
                &session.opts,
                fsm_intersect(&session.opts, a, b),
            )));
        }
    }
}

// [spec:foma:def:iface.iface-substitute-symbol-fn]
// [spec:foma:sem:iface.iface-substitute-symbol-fn]
// [spec:foma:def:foma.iface-substitute-symbol-fn]
// [spec:foma:sem:foma.iface-substitute-symbol-fn]
pub fn iface_substitute_symbol(session: &mut Session, original: &str, substitute: &str) {
    if iface_stack_check(session, 1) {
        // DEVIATION from C: C dequotes the caller's `char *` buffers in place; the
        // args are &str here, so local byte copies are dequoted instead (observably
        // identical — the printed strings and the fsm op both use the dequoted text).
        let mut original = original.as_bytes().to_vec();
        let mut substitute = substitute.as_bytes().to_vec();
        dequote_string(&mut original);
        dequote_string(&mut substitute);
        let original = String::from_utf8_lossy(&original).into_owned();
        let substitute = String::from_utf8_lossy(&substitute).into_owned();
        let Some(popped) = session.stack_pop() else {
            return;
        };
        session.stack_add(fsm_topsort(fsm_minimize(
            &session.opts,
            fsm_substitute_symbol(popped, &original, &substitute),
        )));
        println!("Substituted '{}' for '{}'.", substitute, original);
    }
}

// [spec:foma:def:iface.iface-substitute-defined-fn]
// [spec:foma:sem:iface.iface-substitute-defined-fn]
// [spec:foma:def:foma.iface-substitute-defined-fn]
// [spec:foma:sem:foma.iface-substitute-defined-fn]
pub fn iface_substitute_defined(session: &mut Session, original: &str, substitute: &str) {
    if iface_stack_check(session, 1) {
        // DEVIATION from C: see iface_substitute_symbol — dequote on local copies.
        let mut original = original.as_bytes().to_vec();
        let mut substitute = substitute.as_bytes().to_vec();
        dequote_string(&mut original);
        dequote_string(&mut substitute);
        let original = String::from_utf8_lossy(&original).into_owned();
        let substitute = String::from_utf8_lossy(&substitute).into_owned();
        // Take the registry out of the session for the duration: the found
        // subnet stays mutably borrowed from it across the stack calls below
        // (fsm_substitute_label merges sigmas into the registry's live net, as
        // in C, so a copy would observably diverge).
        let mut defines = std::mem::replace(&mut session.defines, defined_networks_init());
        match find_defined(&mut defines, &substitute) {
            None => {
                println!("No defined network '{}'.", substitute);
            }
            Some(subnet) => {
                let top = session
                    .stack_find_top()
                    .expect("nonempty stack: iface_stack_check(1) passed above");
                if !session
                    .stack_entry_fsm(top, |f| fsm_symbol_occurs(f, &original, M_UPPER + M_LOWER))
                {
                    println!("Symbol '{}' does not occur.", original);
                } else {
                    let newnet = session.stack_entry_fsm_with_opts(top, |opts, f| {
                        fsm_substitute_label(opts, f, &original, subnet)
                    });
                    // C: stack_pop() — the popped net is NOT fsm_destroy'd (latent
                    // leak); here the returned Box is dropped (freed) instead.
                    let _ = session.stack_pop();
                    println!("Substituted network '{}' for '{}'.", substitute, original);
                    session.stack_add(fsm_topsort(fsm_minimize(&session.opts, newnet)));
                }
            }
        }
        session.defines = defines;
    }
}

// [spec:foma:def:iface.iface-shuffle-fn]
// [spec:foma:sem:iface.iface-shuffle-fn]
// [spec:foma:def:foma.iface-shuffle-fn]
// [spec:foma:sem:foma.iface-shuffle-fn]
pub fn iface_shuffle(session: &mut Session) {
    if iface_stack_check(session, 2) {
        while session.stack_size() > 1 {
            // C: fsm_shuffle(stack_pop(), stack_pop()) — two pops in one expression
            // (C order unspecified); Rust evaluates left-to-right. Shuffle is
            // commutative, so the resulting language is the same.
            let Some(a) = session.stack_pop() else { break };
            let Some(b) = session.stack_pop() else { break };
            session.stack_add(fsm_minimize(
                &session.opts,
                fsm_shuffle(&session.opts, a, b),
            ));
        }
    }
}

// [spec:foma:def:iface.iface-union-fn]
// [spec:foma:sem:iface.iface-union-fn]
// [spec:foma:def:foma.iface-union-fn]
// [spec:foma:sem:foma.iface-union-fn]
pub fn iface_union(session: &mut Session) {
    if iface_stack_check(session, 2) {
        while session.stack_size() > 1 {
            // C: fsm_union(&session.opts, stack_pop(), stack_pop()) — pops in one expression (C
            // order unspecified); union is commutative. Minimized, NOT topsorted.
            let Some(a) = session.stack_pop() else { break };
            let Some(b) = session.stack_pop() else { break };
            session.stack_add(fsm_minimize(&session.opts, fsm_union(&session.opts, a, b)));
        }
    }
}
