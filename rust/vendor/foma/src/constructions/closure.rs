//! Wave-4 split of constructions.c (see mod.rs). Cross-module and
//! external names come via `use super::*` (re-exported by mod.rs).
use super::*;

pub const KLEENE_STAR: i32 = 0;
pub const KLEENE_PLUS: i32 = 1;
pub const OPTIONALITY: i32 = 2;

// [spec:foma:def:constructions.fsm-kleene-star-fn]
// [spec:foma:sem:constructions.fsm-kleene-star-fn]
// [spec:foma:def:fomalib.fsm-kleene-star-fn]
// [spec:foma:sem:fomalib.fsm-kleene-star-fn]
pub fn fsm_kleene_star(opts: &FomaOptions, net: Box<Fsm>) -> Box<Fsm> {
    fsm_kleene_closure(opts, net, KLEENE_STAR)
}

// [spec:foma:def:constructions.fsm-kleene-plus-fn]
// [spec:foma:sem:constructions.fsm-kleene-plus-fn]
// [spec:foma:def:fomalib.fsm-kleene-plus-fn]
// [spec:foma:sem:fomalib.fsm-kleene-plus-fn]
pub fn fsm_kleene_plus(opts: &FomaOptions, net: Box<Fsm>) -> Box<Fsm> {
    fsm_kleene_closure(opts, net, KLEENE_PLUS)
}

// [spec:foma:def:constructions.fsm-optionality-fn]
// [spec:foma:sem:constructions.fsm-optionality-fn]
// [spec:foma:def:fomalib.fsm-optionality-fn]
// [spec:foma:sem:fomalib.fsm-optionality-fn]
pub fn fsm_optionality(opts: &FomaOptions, net: Box<Fsm>) -> Box<Fsm> {
    fsm_kleene_closure(opts, net, OPTIONALITY)
}

// [spec:foma:def:constructions.fsm-kleene-closure-fn]
// [spec:foma:sem:constructions.fsm-kleene-closure-fn]
pub fn fsm_kleene_closure(opts: &FomaOptions, net: Box<Fsm>, operation: i32) -> Box<Fsm> {
    if operation == OPTIONALITY {
        return fsm_union(opts, net, fsm_empty_string());
    }

    let mut net = fsm_minimize(opts, net);
    fsm_count(&mut net);

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
        (net.linecount + net.finalcount + 1) as usize
    ];

    let mut j: i32 = 0;
    if operation == KLEENE_STAR {
        add_fsm_arc(&mut new_fsm, j, 0, EPSILON, EPSILON, 1, 1, 1);
        j += 1;
    }
    if operation == KLEENE_PLUS {
        add_fsm_arc(&mut new_fsm, j, 0, EPSILON, EPSILON, 1, 0, 1);
        j += 1;
    }
    let mut laststate = 0;
    let mut arccount = 1;
    let mut i = 0usize;
    while net.states[i].state_no != -1 {
        let curr_state = net.states[i].state_no + 1;
        let curr_target = if net.states[i].target == -1 {
            -1
        } else {
            net.states[i].target + 1
        };
        if curr_target == -1 && net.states[i].final_state == 1 {
            add_fsm_arc(&mut new_fsm, j, curr_state, EPSILON, EPSILON, 0, 1, 0);
            j += 1;
            arccount += 1;
            i += 1;
            laststate = curr_state;
            continue;
        }
        if curr_state != laststate && net.states[i].final_state == 1 {
            arccount += 1;
            add_fsm_arc(&mut new_fsm, j, curr_state, EPSILON, EPSILON, 0, 1, 0);
            j += 1;
        }
        let (line_in, line_out, final_state) = (
            net.states[i].r#in as i32,
            net.states[i].out as i32,
            net.states[i].final_state as i32,
        );
        add_fsm_arc(
            &mut new_fsm,
            j,
            curr_state,
            line_in,
            line_out,
            curr_target,
            final_state,
            0,
        );
        j += 1;
        if curr_target != -1 {
            arccount += 1;
        }
        i += 1;
        laststate = curr_state;
    }
    add_fsm_arc(&mut new_fsm, j, -1, -1, -1, -1, -1, -1);
    j += 1;
    net.statecount += 1;
    net.linecount = j;
    net.finalcount = if operation == KLEENE_STAR {
        net.finalcount + 1
    } else {
        net.finalcount
    };
    net.arccount = arccount;
    net.pathcount = PATHCOUNT_UNKNOWN;
    /* free(net->states) */
    net.states = new_fsm;
    if sigma_find_number(EPSILON, &net.sigma).is_none() {
        sigma_add_special(EPSILON, &mut net.sigma);
    }
    fsm_update_flags(&mut net, NO, NO, NO, NO, UNK, NO);
    net
}
