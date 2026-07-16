//! foma/reverse.c — literal (bug-for-bug) Wave-2 port per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/reverse.md
//! (per-file id) plus the fomalib.h prototype id.
//!
//! fsm_reverse builds the reversal via the read/construct handle APIs: all
//! original state numbers are shifted up by 1, a brand-new state 0 becomes
//! the sole initial state, with EPSILON:EPSILON arcs to every (old) final
//! state; label sides are NOT swapped. The input net is consumed
//! (fsm_destroy'd).

use crate::dynarray::{
    fsm_construct_add_arc_nums, fsm_construct_copy_sigma, fsm_construct_done, fsm_construct_init,
    fsm_construct_set_final, fsm_construct_set_initial, fsm_get_arc_num_in, fsm_get_arc_num_out,
    fsm_get_arc_source, fsm_get_arc_target, fsm_get_next_arc, fsm_read_done, fsm_read_init,
};
#[cfg(test)]
use crate::options::FomaOptions;
use crate::structures::fsm_destroy;
use crate::types::{EPSILON, Fsm};

// [spec:foma:def:reverse.fsm-reverse-fn]
// [spec:foma:sem:reverse.fsm-reverse-fn]
// [spec:foma:def:fomalib.fsm-reverse-fn]
// [spec:foma:sem:fomalib.fsm-reverse-fn]
pub fn fsm_reverse(net: Box<Fsm>) -> Box<Fsm> {
    /* C: net stays a caller pointer alongside the read handle; here the
    handle owns the net until fsm_read_done returns it, so net->name /
    net->sigma are reached through inh (observably equivalent) */
    let mut inh = fsm_read_init(net);
    let name = inh
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .name
        .clone();
    let mut revh = fsm_construct_init(&name);
    fsm_construct_copy_sigma(
        &mut revh,
        &inh.net
            .as_ref()
            .expect("net present until fsm_read_done")
            .sigma,
    );

    while fsm_get_next_arc(&mut inh) != 0 {
        let (target, source) = (fsm_get_arc_target(&inh), fsm_get_arc_source(&inh));
        let (num_in, num_out) = (fsm_get_arc_num_in(&inh), fsm_get_arc_num_out(&inh));
        fsm_construct_add_arc_nums(&mut revh, target + 1, source + 1, num_in, num_out);
    }

    for i in inh.finals() {
        fsm_construct_add_arc_nums(&mut revh, 0, i + 1, EPSILON, EPSILON);
    }
    for i in inh.initials() {
        fsm_construct_set_final(&mut revh, i + 1);
    }
    fsm_construct_set_initial(&mut revh, 0);
    let net = fsm_read_done(inh);
    let mut revnet = fsm_construct_done(revh);
    revnet.is_deterministic = 0;
    revnet.is_epsilon_free = 0;
    fsm_destroy(net);
    revnet
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{apply_down, apply_init, apply_up};
    use crate::regex::fsm_parse_regex;
    use std::collections::BTreeSet;

    fn real_lines(net: &Fsm) -> Vec<crate::types::FsmState> {
        net.states
            .iter()
            .take_while(|l| l.state_no != -1)
            .cloned()
            .collect()
    }

    // [spec:foma:sem:reverse.fsm-reverse-fn/test]
    // [spec:foma:sem:fomalib.fsm-reverse-fn/test]
    #[test]
    fn reverse_shifts_states_and_adds_new_initial_with_epsilon_arcs() {
        let opts = &FomaOptions::default();
        let net = fsm_parse_regex(opts, "a b c", None, None).unwrap();
        let old_statecount = net.statecount;
        let old = real_lines(&net);
        let old_arcs: Vec<(i32, i16, i16, i32)> = old
            .iter()
            .filter(|l| l.target != -1)
            .map(|l| (l.state_no, l.r#in, l.out, l.target))
            .collect();
        let old_finals: BTreeSet<i32> = old
            .iter()
            .filter(|l| l.final_state == 1)
            .map(|l| l.state_no)
            .collect();
        let old_initials: BTreeSet<i32> = old
            .iter()
            .filter(|l| l.start_state == 1)
            .map(|l| l.state_no)
            .collect();

        let rev = fsm_reverse(net);
        /* exact state+1 shift: one brand-new state 0 */
        assert_eq!(rev.statecount, old_statecount + 1);
        assert_eq!(rev.is_deterministic, 0);
        assert_eq!(rev.is_epsilon_free, 0);

        let rev_lines = real_lines(&rev);
        let rev_arcs: Vec<(i32, i16, i16, i32)> = rev_lines
            .iter()
            .filter(|l| l.target != -1)
            .map(|l| (l.state_no, l.r#in, l.out, l.target))
            .collect();
        /* every old arc (s, in, out, t) appears as (t+1, in, out, s+1):
        label sides NOT swapped */
        for (s, i, o, t) in &old_arcs {
            assert!(rev_arcs.contains(&(t + 1, *i, *o, s + 1)), "arc {s}->{t}");
        }
        /* one EPSILON:EPSILON arc from new state 0 per old final state */
        for f in &old_finals {
            assert!(
                rev_arcs.contains(&(0, EPSILON as i16, EPSILON as i16, f + 1)),
                "epsilon arc to old final {f}"
            );
        }
        assert_eq!(rev_arcs.len(), old_arcs.len() + old_finals.len());
        /* old initials (shifted) are exactly the finals of the result */
        let rev_finals: BTreeSet<i32> = rev_lines
            .iter()
            .filter(|l| l.final_state == 1)
            .map(|l| l.state_no)
            .collect();
        assert_eq!(rev_finals, old_initials.iter().map(|i| i + 1).collect());
        /* state 0 is the sole initial state */
        for l in &rev_lines {
            assert_eq!(l.start_state == 1, l.state_no == 0);
        }
    }

    // [spec:foma:sem:reverse.fsm-reverse-fn/test]
    // [spec:foma:sem:fomalib.fsm-reverse-fn/test]
    #[test]
    fn reverse_accepts_reversed_words() {
        let opts = &FomaOptions::default();
        let rev = fsm_reverse(fsm_parse_regex(opts, "a b c", None, None).unwrap());
        let mut h = apply_init(&rev);
        assert_eq!(apply_down(&mut h, Some("cba")), Some("cba".to_string()));
        assert_eq!(apply_down(&mut h, Some("abc")), None);
    }

    // [spec:foma:sem:reverse.fsm-reverse-fn/test]
    // [spec:foma:sem:fomalib.fsm-reverse-fn/test]
    #[test]
    fn reverse_keeps_transducer_label_sides_unswapped() {
        let opts = &FomaOptions::default();
        /* a:x b:y reversed maps upper "ba" to lower "yx" (and back) */
        let rev = fsm_reverse(fsm_parse_regex(opts, "a:x b:y", None, None).unwrap());
        let mut h = apply_init(&rev);
        assert_eq!(apply_down(&mut h, Some("ba")), Some("yx".to_string()));
        assert_eq!(apply_down(&mut h, Some("ab")), None);
        let mut h = apply_init(&rev);
        assert_eq!(apply_up(&mut h, Some("yx")), Some("ba".to_string()));
    }
}
