//! foma/iface.c Wave-4 split: test-predicate commands (iface_test_*).
//! See iface/mod.rs.
use super::*;

// [spec:foma:def:iface.iface-test-equivalent-fn]
// [spec:foma:sem:iface.iface-test-equivalent-fn]
// [spec:foma:def:foma.iface-test-equivalent-fn]
// [spec:foma:sem:foma.iface-test-equivalent-fn]
pub fn iface_test_equivalent(session: &mut Session) {
    if iface_stack_check(session, 2) {
        let top = session.stack_find_top().unwrap();
        let second = session.stack_find_second().unwrap();
        let mut one = session.stack_entry_fsm(top, fsm_copy);
        let mut two = session.stack_entry_fsm(second, fsm_copy);
        fsm_count(&mut one);
        fsm_count(&mut two);
        // Latent leak in C: the two copies are never fsm_destroy'd; here they are
        // consumed (freed) by fsm_equivalent — no-op observable difference.
        iface_print_bool(fsm_equivalent(&session.opts, one, two));
    }
}

// [spec:foma:def:iface.iface-test-functional-fn]
// [spec:foma:sem:iface.iface-test-functional-fn]
// [spec:foma:def:foma.iface-test-functional-fn]
// [spec:foma:sem:foma.iface-test-functional-fn]
pub fn iface_test_functional(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let top = session.stack_find_top().unwrap();
        let r = session.stack_entry_fsm_with_opts(top, fsm_isfunctional);
        iface_print_bool(r);
    }
}

// [spec:foma:def:iface.iface-test-identity-fn]
// [spec:foma:sem:iface.iface-test-identity-fn]
// [spec:foma:def:foma.iface-test-identity-fn]
// [spec:foma:sem:foma.iface-test-identity-fn]
pub fn iface_test_identity(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let top = session.stack_find_top().unwrap();
        let r = session.stack_entry_fsm_with_opts(top, fsm_isidentity);
        iface_print_bool(r);
    }
}

// [spec:foma:def:iface.iface-test-nonnull-fn]
// [spec:foma:sem:iface.iface-test-nonnull-fn]
// [spec:foma:def:foma.iface-test-nonnull-fn]
// [spec:foma:sem:foma.iface-test-nonnull-fn]
pub fn iface_test_nonnull(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let top = session.stack_find_top().unwrap();
        // C: iface_print_bool(!fsm_isempty(...)) — logical NOT of the int result.
        let e = session.stack_entry_fsm_with_opts(top, fsm_isempty);
        iface_print_bool(!e);
    }
}

// [spec:foma:def:iface.iface-test-null-fn]
// [spec:foma:sem:iface.iface-test-null-fn]
// [spec:foma:def:foma.iface-test-null-fn]
// [spec:foma:sem:foma.iface-test-null-fn]
pub fn iface_test_null(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let top = session.stack_find_top().unwrap();
        let r = session.stack_entry_fsm_with_opts(top, fsm_isempty);
        iface_print_bool(r);
    }
}

// [spec:foma:def:iface.iface-test-unambiguous-fn]
// [spec:foma:sem:iface.iface-test-unambiguous-fn]
// [spec:foma:def:foma.iface-test-unambiguous-fn]
// [spec:foma:sem:foma.iface-test-unambiguous-fn]
pub fn iface_test_unambiguous(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let top = session.stack_find_top().unwrap();
        let r = session.stack_entry_fsm_with_opts(top, fsm_isunambiguous);
        iface_print_bool(r);
    }
}

// [spec:foma:def:iface.iface-test-lower-universal-fn]
// [spec:foma:sem:iface.iface-test-lower-universal-fn]
// [spec:foma:def:foma.iface-test-lower-universal-fn]
// [spec:foma:sem:foma.iface-test-lower-universal-fn]
pub fn iface_test_lower_universal(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let top = session.stack_find_top().unwrap();
        let copy = session.stack_entry_fsm(top, fsm_copy);
        let mut tmp = fsm_complement(&session.opts, fsm_lower(copy));
        iface_print_bool(fsm_isempty(&session.opts, &mut tmp));
        fsm_destroy(tmp);
    }
}

// [spec:foma:def:iface.iface-test-sequential-fn]
// [spec:foma:sem:iface.iface-test-sequential-fn]
// [spec:foma:def:foma.iface-test-sequential-fn]
// [spec:foma:sem:foma.iface-test-sequential-fn]
pub fn iface_test_sequential(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let top = session.stack_find_top().unwrap();
        let r = session.stack_entry_fsm(top, |f| fsm_issequential(f));
        iface_print_bool(r);
    }
}

// [spec:foma:def:iface.iface-test-upper-universal-fn]
// [spec:foma:sem:iface.iface-test-upper-universal-fn]
// [spec:foma:def:foma.iface-test-upper-universal-fn]
// [spec:foma:sem:foma.iface-test-upper-universal-fn]
pub fn iface_test_upper_universal(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let top = session.stack_find_top().unwrap();
        let copy = session.stack_entry_fsm(top, fsm_copy);
        let mut tmp = fsm_complement(&session.opts, fsm_upper(copy));
        iface_print_bool(fsm_isempty(&session.opts, &mut tmp));
        fsm_destroy(tmp);
    }
}
