//! foma/extract.c — literal (bug-for-bug) Wave-2 port per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/extract.md
//! (per-file ids) plus the fomalib.h prototype ids.
//!
//! fsm_lower / fsm_upper project a transducer onto its lower (output) /
//! upper (input) side in place, rebuilding the state array with the
//! fsm_state_* dynarray builder. A lone UNKNOWN (1) label becomes IDENTITY
//! (2) in the projection.

use crate::constructions::fsm_update_flags;
use crate::dynarray::{
    fsm_state_add_arc, fsm_state_close, fsm_state_end_state, fsm_state_init,
    fsm_state_set_current_state,
};
#[cfg(test)]
use crate::options::FomaOptions;
use crate::sigma::{sigma_cleanup, sigma_max};
use crate::types::{Fsm, IDENTITY, NO, UNK, UNKNOWN};

// [spec:foma:def:extract.fsm-lower-fn]
// [spec:foma:sem:extract.fsm-lower-fn]
// [spec:foma:def:fomalib.fsm-lower-fn]
// [spec:foma:sem:fomalib.fsm-lower-fn]
pub fn fsm_lower(net: Box<Fsm>) -> Box<Fsm> {
    let mut net = net;
    /* C: fsm = net->states — reads below index net.states directly */
    let mut builder = fsm_state_init(sigma_max(&net.sigma));
    let mut prevstate = -1;
    let mut i: i32 = 0;
    while net.states[i as usize].state_no != -1 {
        if prevstate != -1 && prevstate != net.states[i as usize].state_no {
            fsm_state_end_state(&mut builder);
        }
        if prevstate != net.states[i as usize].state_no {
            fsm_state_set_current_state(
                &mut builder,
                net.states[i as usize].state_no,
                net.states[i as usize].final_state as i32,
                net.states[i as usize].start_state as i32,
            );
        }
        if net.states[i as usize].target != -1 {
            let out = if net.states[i as usize].out as i32 == UNKNOWN {
                IDENTITY
            } else {
                net.states[i as usize].out as i32
            };
            fsm_state_add_arc(
                &mut builder,
                net.states[i as usize].state_no,
                out,
                out,
                net.states[i as usize].target,
                net.states[i as usize].final_state as i32,
                net.states[i as usize].start_state as i32,
            );
        }
        /* C for-loop increment clause: prevstate = (fsm+i)->state_no, i++ */
        prevstate = net.states[i as usize].state_no;
        i += 1;
    }
    fsm_state_end_state(&mut builder);
    /* drop the old line table; fsm_state_close installs the rebuilt one */
    net.states = Vec::new();
    fsm_state_close(&mut builder, &mut net);
    fsm_update_flags(&mut net, NO, NO, NO, UNK, UNK, UNK);
    sigma_cleanup(&mut net, 0);
    net
}

// [spec:foma:def:extract.fsm-upper-fn]
// [spec:foma:sem:extract.fsm-upper-fn]
// [spec:foma:def:fomalib.fsm-upper-fn]
// [spec:foma:sem:fomalib.fsm-upper-fn]
pub fn fsm_upper(net: Box<Fsm>) -> Box<Fsm> {
    let mut net = net;
    /* C: fsm = net->states — reads below index net.states directly */
    let mut builder = fsm_state_init(sigma_max(&net.sigma));
    let mut prevstate = -1;
    let mut i: i32 = 0;
    while net.states[i as usize].state_no != -1 {
        if prevstate != -1 && prevstate != net.states[i as usize].state_no {
            fsm_state_end_state(&mut builder);
        }
        if prevstate != net.states[i as usize].state_no {
            fsm_state_set_current_state(
                &mut builder,
                net.states[i as usize].state_no,
                net.states[i as usize].final_state as i32,
                net.states[i as usize].start_state as i32,
            );
        }
        if net.states[i as usize].target != -1 {
            let r#in = if net.states[i as usize].r#in as i32 == UNKNOWN {
                IDENTITY
            } else {
                net.states[i as usize].r#in as i32
            };
            fsm_state_add_arc(
                &mut builder,
                net.states[i as usize].state_no,
                r#in,
                r#in,
                net.states[i as usize].target,
                net.states[i as usize].final_state as i32,
                net.states[i as usize].start_state as i32,
            );
        }
        /* C for-loop increment clause: prevstate = (fsm+i)->state_no, i++ */
        prevstate = net.states[i as usize].state_no;
        i += 1;
    }
    fsm_state_end_state(&mut builder);
    /* drop the old line table; fsm_state_close installs the rebuilt one */
    net.states = Vec::new();
    fsm_state_close(&mut builder, &mut net);
    fsm_update_flags(&mut net, NO, NO, NO, UNK, UNK, UNK);
    sigma_cleanup(&mut net, 0);
    net
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{apply_down, apply_init};
    use crate::regex::fsm_parse_regex;
    use crate::types::EPSILON;

    fn sigma_syms(net: &Fsm) -> Vec<(i32, String)> {
        net.sigma
            .iter()
            .map(|node| (node.number, node.symbol.to_string()))
            .collect()
    }

    fn arc_labels(net: &Fsm) -> Vec<(i16, i16)> {
        net.states
            .iter()
            .take_while(|l| l.state_no != -1)
            .filter(|l| l.target != -1)
            .map(|l| (l.r#in, l.out))
            .collect()
    }

    // [spec:foma:sem:extract.fsm-lower-fn/test]
    // [spec:foma:sem:fomalib.fsm-lower-fn/test]
    #[test]
    fn lower_projects_transducer_to_output_side() {
        let opts = &FomaOptions::default();
        let net = fsm_lower(fsm_parse_regex(opts, "a:b", None, None).unwrap());
        /* acceptor: both labels are the old `out` symbol */
        for (i, o) in arc_labels(&net) {
            assert_eq!(i, o);
        }
        /* sigma_cleanup(net, 0): no UNKNOWN/IDENTITY left in sigma, so the
        now-unused "a" is purged */
        let syms: Vec<String> = sigma_syms(&net).into_iter().map(|(_, s)| s).collect();
        assert!(syms.contains(&"b".to_string()));
        assert!(!syms.contains(&"a".to_string()));
        /* fsm_update_flags(net, NO, NO, NO, UNK, UNK, UNK) */
        assert_eq!(net.is_deterministic, NO);
        assert_eq!(net.is_pruned, NO);
        assert_eq!(net.is_minimized, NO);
        assert_eq!(net.is_epsilon_free, UNK);
        assert_eq!(net.is_loop_free, UNK);
        assert_eq!(net.is_completed, UNK);
        /* resulting language is {b} */
        let mut h = apply_init(&net);
        assert_eq!(apply_down(&mut h, Some("b")), Some("b".to_string()));
        assert_eq!(apply_down(&mut h, Some("a")), None);
    }

    // [spec:foma:sem:extract.fsm-upper-fn/test]
    // [spec:foma:sem:fomalib.fsm-upper-fn/test]
    #[test]
    fn upper_projects_transducer_to_input_side() {
        let opts = &FomaOptions::default();
        let net = fsm_upper(fsm_parse_regex(opts, "a:b", None, None).unwrap());
        for (i, o) in arc_labels(&net) {
            assert_eq!(i, o);
        }
        /* the now-unused "b" is purged; "a" stays */
        let syms: Vec<String> = sigma_syms(&net).into_iter().map(|(_, s)| s).collect();
        assert!(syms.contains(&"a".to_string()));
        assert!(!syms.contains(&"b".to_string()));
        assert_eq!(net.is_deterministic, NO);
        assert_eq!(net.is_pruned, NO);
        assert_eq!(net.is_minimized, NO);
        assert_eq!(net.is_epsilon_free, UNK);
        assert_eq!(net.is_loop_free, UNK);
        assert_eq!(net.is_completed, UNK);
        /* resulting language is {a} */
        let mut h = apply_init(&net);
        assert_eq!(apply_down(&mut h, Some("a")), Some("a".to_string()));
        assert_eq!(apply_down(&mut h, Some("b")), None);
    }

    // [spec:foma:sem:extract.fsm-lower-fn/test]
    // [spec:foma:sem:fomalib.fsm-lower-fn/test]
    #[test]
    fn lower_maps_unknown_label_to_identity() {
        let opts = &FomaOptions::default();
        /* a:? has arcs a:UNKNOWN and a:a; on the lower side the lone
        UNKNOWN becomes an IDENTITY pair */
        let src = fsm_parse_regex(opts, "a:?", None, None).unwrap();
        assert!(
            arc_labels(&src).iter().any(|&(_, o)| o as i32 == UNKNOWN),
            "source transducer has an UNKNOWN on the lower side"
        );
        let a_num = sigma_syms(&src)
            .into_iter()
            .find(|(_, s)| s == "a")
            .unwrap()
            .0 as i16;
        let net = fsm_lower(src);
        let mut labels = arc_labels(&net);
        labels.sort();
        assert_eq!(
            labels,
            vec![(IDENTITY as i16, IDENTITY as i16), (a_num, a_num)]
        );
        /* UNKNOWN and IDENTITY remain in sigma, so sigma_cleanup(net, 0)
        purges nothing */
        let nums: Vec<i32> = sigma_syms(&net).into_iter().map(|(n, _)| n).collect();
        assert!(nums.contains(&UNKNOWN));
        assert!(nums.contains(&IDENTITY));
        /* language is ?: accepts known "a" and (via IDENTITY) unknown "z" */
        let mut h = apply_init(&net);
        assert_eq!(apply_down(&mut h, Some("a")), Some("a".to_string()));
        assert_eq!(apply_down(&mut h, Some("z")), Some("z".to_string()));
    }

    // [spec:foma:sem:extract.fsm-upper-fn/test]
    // [spec:foma:sem:fomalib.fsm-upper-fn/test]
    #[test]
    fn upper_of_unknown_transducer_projects_all_arcs_to_input_side() {
        let opts = &FomaOptions::default();
        /* a:? upper side: both arcs project to a:a with the same target,
        and the fsm_state_add_arc builder drops the exact duplicate */
        let src = fsm_parse_regex(opts, "a:?", None, None).unwrap();
        assert_eq!(arc_labels(&src).len(), 2, "source has a:UNKNOWN and a:a");
        let a_num = sigma_syms(&src)
            .into_iter()
            .find(|(_, s)| s == "a")
            .unwrap()
            .0 as i16;
        let net = fsm_upper(src);
        assert_eq!(arc_labels(&net), vec![(a_num, a_num)]);
        /* UNKNOWN stays in sigma (no purge with force = 0) */
        let nums: Vec<i32> = sigma_syms(&net).into_iter().map(|(n, _)| n).collect();
        assert!(nums.contains(&UNKNOWN));
        assert!(nums.contains(&IDENTITY));
        let mut h = apply_init(&net);
        assert_eq!(apply_down(&mut h, Some("a")), Some("a".to_string()));
        assert_eq!(apply_down(&mut h, Some("z")), None);
    }

    // [spec:foma:sem:extract.fsm-lower-fn/test]
    // [spec:foma:sem:fomalib.fsm-lower-fn/test]
    #[test]
    fn lower_epsilon_output_becomes_epsilon_arc() {
        let opts = &FomaOptions::default();
        /* a:0 lower side: the EPSILON output becomes an eps:eps arc and the
        language is the empty string */
        let net = fsm_lower(fsm_parse_regex(opts, "a:0", None, None).unwrap());
        assert_eq!(arc_labels(&net), vec![(EPSILON as i16, EPSILON as i16)]);
        let mut h = apply_init(&net);
        assert_eq!(apply_down(&mut h, Some("")), Some("".to_string()));
    }
}
