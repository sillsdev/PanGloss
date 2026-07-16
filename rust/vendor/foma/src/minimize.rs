//! foma/minimize.c — literal (bug-for-bug) Wave-2 port per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/minimize.md
//! (per-file ids) plus the fomalib.h prototype ids.
//!
//! Hopcroft partition-refinement minimization plus the Brzozowski fallback.
//! The C's pointer pools (struct p / struct e / struct agenda and the
//! inverse-arc trans_array/trans_list index) become index-based pools
//! (Vec + usize indices) with the identical link discipline; NULL ↔ None.
//! A C `struct p *` argument into the block pool decomposes to a
//! (Minimizer, index) pair — see agenda_add.
//!
//! Wave 4: the C's file-static scratch (partition/agenda pools, the inverse-arc
//! index, sigma maps and the loop counters) is owned by a per-call `Minimizer`
//! struct threaded through the hop pipeline by `&mut` — nothing survives a
//! call. The shared int stack and the `add_fsm_arc` line writer belong to
//! other concerns and stay module-level.

use crate::coaccessible::fsm_coaccessible;
use crate::constructions::{add_fsm_arc, fsm_count, fsm_update_flags};
use crate::determinize::fsm_determinize;
use crate::options::FomaOptions;
use crate::reverse::fsm_reverse;
use crate::sigma::sigma_max;
use crate::structures::{fsm_destroy, fsm_empty_set};
use crate::types::{EPSILON, Fsm, UNK, UNKNOWN, YES};

// [spec:foma:def:minimize.statesym]
/* Declared in the C but never used (dead declaration) — kept literally. */
#[derive(Debug, Clone)]
pub struct Statesym {
    pub target: i32,
    pub symbol: u16,
    pub states: Option<Box<StateList>>,
    pub next: Option<Box<Statesym>>,
}

// [spec:foma:def:minimize.state-list]
/* Declared in the C but never used (dead declaration) — kept literally. */
#[derive(Debug, Clone)]
pub struct StateList {
    pub state: i32,
    pub next: Option<Box<StateList>>,
}

// [spec:foma:def:minimize.p]
/* Block of the partition; lives in the PHEAD pool. The C's struct e * /
struct p * / struct agenda * fields are indices into the E / PHEAD /
AGENDA_TOP pools (None ↔ NULL). calloc zero-fill ↔ Default. */
#[derive(Debug, Clone, Default)]
pub struct P {
    pub first_e: Option<usize>,
    pub last_e: Option<usize>,
    pub current_split: Option<usize>,
    pub next: Option<usize>,
    pub agenda: Option<usize>,
    pub count: i32,
    pub t_count: i32,
    pub inv_count: i32,
    pub inv_t_count: i32,
}

// [spec:foma:def:minimize.e]
/* Per-state record; E array index == state number ("temp_E - E" in the C
is the index itself). group is a PHEAD-pool index (calloc NULL ↔ 0 here;
always assigned by init_PE before use); left/right are E indices. */
#[derive(Debug, Clone, Default)]
pub struct E {
    pub group: usize,
    pub left: Option<usize>,
    pub right: Option<usize>,
    pub inv_count: i32,
}

// [spec:foma:def:minimize.agenda]
/* Agenda entry; lives in the AGENDA_TOP pool. p is a PHEAD-pool index
(calloc NULL ↔ 0 here; always assigned before read); next is an
AGENDA_TOP index. */
#[derive(Debug, Clone, Default)]
pub struct Agenda {
    pub p: usize,
    pub next: Option<usize>,
    pub index: bool,
}

// [spec:foma:def:minimize.trans-list]
#[derive(Debug, Clone, Default)]
pub struct TransList {
    pub inout: i32,
    pub source: i32,
}

// [spec:foma:def:minimize.trans-array]
/* transitions is the C's struct trans_list * interior pointer into the
trans_list_minimize pool — here the base offset of this state's slice. */
#[derive(Debug, Clone, Default)]
pub struct TransArray {
    pub transitions: usize,
    pub size: u32,
    pub tail: u32,
}

/// Per-call Hopcroft-minimization scratch. Every field mirrors a C
/// file-static; Wave 4 folds them into one owned struct created fresh in
/// `fsm_minimize_hop`, so nothing survives a call. `Default` gives the C's
/// zeroed BSS start.
#[derive(Debug, Default)]
pub(crate) struct Minimizer {
    // C: static int *single_sigma_array, *double_sigma_array, *memo_table,
    //    *temp_move, *temp_group, maxsigma, epsilon_symbol, num_states,
    //    num_symbols, num_finals, mainloop, total_states;
    single_sigma_array: Vec<i32>,
    double_sigma_array: Vec<i32>,
    memo_table: Vec<i32>,
    temp_move: Vec<i32>,
    temp_group: Vec<i32>,
    maxsigma: i32,
    epsilon_symbol: i32,
    num_states: i32,
    num_symbols: i32,
    num_finals: i32,
    mainloop: i32,
    total_states: i32,
    // C: static _Bool *finals;
    finals: Vec<bool>,
    // C: struct trans_list *trans_list_minimize; struct trans_array
    // *trans_array_minimize;
    trans_list_minimize: Vec<TransList>,
    trans_array_minimize: Vec<TransArray>,
    // C: static struct p *P, *Phead, *Pnext, *current_w;
    // phead owns the block pool; p is the block-chain head index, pnext the
    // bump-allocation cursor, current_w the block currently used as splitter.
    p: Option<usize>,
    phead: Vec<P>,
    pnext: usize,
    current_w: usize,
    // C: static struct e *E;
    e: Vec<E>,
    // C: static struct agenda *Agenda_head, *Agenda_top, *Agenda_next, *Agenda;
    // agenda_top owns the agenda pool; the other three are indices into it.
    agenda_head: Option<usize>,
    agenda_top: Vec<Agenda>,
    agenda_next: usize,
    agenda: Option<usize>,
}

/* C forward decl kept as a comment:
static void single_symbol_to_symbol_pair(int symbol, int *symbol_in, int *symbol_out);
(compiled out in minimize.c — the back-mapping is never needed here) */

// [spec:foma:def:minimize.fsm-minimize-fn]
// [spec:foma:sem:minimize.fsm-minimize-fn]
// [spec:foma:def:fomalib.fsm-minimize-fn]
// [spec:foma:sem:fomalib.fsm-minimize-fn]
pub fn fsm_minimize(opts: &FomaOptions, net: Box<Fsm>) -> Box<Fsm> {
    /* C: extern int g_minimal; extern int g_minimize_hopcroft; — the session
    options threaded in as `opts` */

    /* C: if (net == NULL) return NULL — a Box argument cannot be NULL;
    NULL-able callers keep the check at the call site */
    let mut net = net;
    /* The network needs to be deterministic and trim before we minimize */
    if net.is_deterministic != YES {
        net = fsm_determinize(net);
    }
    if net.is_pruned != YES {
        net = fsm_coaccessible(net);
    }
    if net.is_minimized != YES && opts.minimal {
        if opts.minimize_hopcroft {
            net = fsm_minimize_hop(net);
        } else {
            net = fsm_minimize_brz(net);
        }
        fsm_update_flags(&mut net, YES, YES, YES, YES, UNK, UNK);
    }
    net
}

// [spec:foma:def:minimize.fsm-minimize-brz-fn]
// [spec:foma:sem:minimize.fsm-minimize-brz-fn]
pub(crate) fn fsm_minimize_brz(net: Box<Fsm>) -> Box<Fsm> {
    fsm_determinize(fsm_reverse(fsm_determinize(fsm_reverse(net))))
}

// [spec:foma:def:minimize.fsm-minimize-hop-fn]
// [spec:foma:sem:minimize.fsm-minimize-hop-fn]
#[allow(non_snake_case)]
pub(crate) fn fsm_minimize_hop(net: Box<Fsm>) -> Box<Fsm> {
    let mut net = net;

    fsm_count(&mut net);
    if net.finalcount == 0 {
        fsm_destroy(net);
        return fsm_empty_set();
    }

    /* all partition-refinement scratch is owned here and dropped on return */
    let mut m = Minimizer {
        num_states: net.statecount,
        p: None,
        ..Default::default()
    };

    /*
       1. generate the inverse lookup table
       2. generate P and E (partitions, states linked list)
       3. Init Agenda = {Q, Q-F}
       4. Split until Agenda is empty
    */

    sigma_to_pairs(&mut m, &mut net);

    init_PE(&mut m);

    'bail: {
        if m.total_states == m.num_states {
            break 'bail; /* goto bail */
        }

        generate_inverse(&mut m, &net);

        /* C: Agenda_head->index = 0; — unconditional deref (the head exists
        here because num_finals > 0) */
        let head = m
            .agenda_head
            .expect("agenda head exists here because num_finals > 0");
        m.agenda_top[head].index = false;
        if let Some(next) = m.agenda_top[head].next {
            m.agenda_top[next].index = false;
        }

        /* for (Agenda = Agenda_head; Agenda != NULL; ) */
        m.agenda = m.agenda_head;
        while let Some(agptr) = m.agenda {
            /* Remove current_w from agenda */
            let current_w = m.agenda_top[agptr].p;
            let current_i = m.agenda_top[agptr].index as i32;
            let agenda_next_entry = m.agenda_top[agptr].next;
            m.current_w = current_w;
            m.phead[current_w].agenda = None;
            m.agenda = agenda_next_entry;

            /* Store current group state number in tmp_group */
            /* And figure out minsym */
            /* If index is 0 we start splitting from the first symbol */
            /* Otherwise we split from where we left off last time */

            let mut thissize: i32 = 0;
            let mut minsym: i32 = i32::MAX; /* INT_MAX */
            let mut temp_E = m.phead[current_w].first_e;
            while let Some(te) = temp_E {
                let stateno = te; /* temp_E - E */
                m.temp_group[thissize as usize] = stateno as i32;
                thissize += 1;
                /* Clear tails if symloop should start from 0 */
                if current_i == 0 {
                    m.trans_array_minimize[stateno].tail = 0;
                }
                let tail = m.trans_array_minimize[stateno].tail;
                let size = m.trans_array_minimize[stateno].size;
                let transitions = m.trans_array_minimize[stateno].transitions + tail as usize;
                if tail < size && m.trans_list_minimize[transitions].inout < minsym {
                    minsym = m.trans_list_minimize[transitions].inout;
                }
                temp_E = m.e[te].right;
            }

            /* for (next_minsym = INT_MAX; minsym != INT_MAX;
            minsym = next_minsym, next_minsym = INT_MAX) */
            let mut next_minsym: i32 = i32::MAX;
            'symloop: while minsym != i32::MAX {
                'cont: {
                    /* Add states to temp_move */
                    let mut j: i32 = 0;
                    let mut i: i32 = 0;
                    while i < thissize {
                        let stateno = m.temp_group[i as usize] as usize;
                        let mut tail = m.trans_array_minimize[stateno].tail;
                        let base = m.trans_array_minimize[stateno].transitions;
                        let size = m.trans_array_minimize[stateno].size;
                        let mut transitions = base + tail as usize;
                        while tail < size && m.trans_list_minimize[transitions].inout == minsym {
                            let source = m.trans_list_minimize[transitions].source;
                            if m.memo_table[source as usize] != m.mainloop {
                                m.memo_table[source as usize] = m.mainloop;
                                m.temp_move[j as usize] = source;
                                j += 1;
                            }
                            tail += 1;
                            transitions += 1;
                        }
                        m.trans_array_minimize[stateno].tail = tail;
                        if tail < size && m.trans_list_minimize[transitions].inout < next_minsym {
                            next_minsym = m.trans_list_minimize[transitions].inout;
                        }
                        i += 1;
                    }
                    if j == 0 {
                        break 'cont; /* continue */
                    }
                    m.mainloop += 1;
                    if refine_states(&mut m, j) {
                        break 'symloop; /* break loop if we split current_w */
                    }
                }
                minsym = next_minsym;
                next_minsym = i32::MAX;
            }
            if m.total_states == m.num_states {
                break;
            }
        }

        net = rebuild_machine(&mut m, net);
    }

    /* bail: `m` drops here, freeing the agenda/partition/inverse pools and
    the sigma maps (no Box chains, so no recursion) */

    net
}

// [spec:foma:def:minimize.rebuild-machine-fn]
// [spec:foma:sem:minimize.rebuild-machine-fn]
pub(crate) fn rebuild_machine(m: &mut Minimizer, net: Box<Fsm>) -> Box<Fsm> {
    let mut net = net;
    let mut new_linecount: i32 = 0;
    let mut arccount: i32 = 0;

    if net.statecount == m.total_states {
        return net;
    }
    /* the line table is rewritten in place below */

    /* We need to make sure state 0 is first in its group */
    /* to get the proper numbering of states */

    /* if (E->group->first_e != E) E->group->first_e = E; — pointer overwrite
    only; the member list is not re-woven */
    let g0 = m.e[0].group;
    if m.phead[g0].first_e != Some(0) {
        m.phead[g0].first_e = Some(0);
    }

    /* Recycling t_count for group numbering use here */

    let mut group_num: i32 = 1;
    let mut myp = m.p;
    while let Some(pi) = myp {
        m.phead[pi].count = 0;
        myp = m.phead[pi].next;
    }

    let mut i: usize = 0;
    while net.states[i].state_no != -1 {
        let thise = net.states[i].state_no as usize; /* thise = E+state_no */
        let g = m.e[thise].group;
        if m.phead[g].first_e == Some(thise) {
            new_linecount += 1;
            if net.states[i].start_state == 1 {
                m.phead[g].t_count = 0;
                m.phead[g].count = 1;
            } else if m.phead[g].count == 0 {
                m.phead[g].t_count = group_num;
                group_num += 1;
                m.phead[g].count = 1;
            }
        }
        i += 1;
    }

    let mut j: i32 = 0;
    let mut i: usize = 0;
    while net.states[i].state_no != -1 {
        let thise = net.states[i].state_no as usize;
        let g = m.e[thise].group;
        if m.phead[g].first_e == Some(thise) {
            let source = m.phead[g].t_count;
            let target = if net.states[i].target == -1 {
                -1
            } else {
                m.phead[m.e[net.states[i].target as usize].group].t_count
            };
            let r#in = net.states[i].r#in as i32;
            let out = net.states[i].out as i32;
            let start_state = net.states[i].start_state as i32;
            let final_flag = m.finals[thise] as i32;
            add_fsm_arc(
                &mut net.states,
                j,
                source,
                r#in,
                out,
                target,
                final_flag,
                start_state,
            );
            /* C reads (fsm+i)->target again AFTER the write to (fsm+j); when
            j == i the rewritten target's -1-ness matches the original's */
            arccount = if net.states[i].target == -1 {
                arccount
            } else {
                arccount + 1
            };
            j += 1;
        }
        i += 1;
    }

    add_fsm_arc(&mut net.states, j, -1, -1, -1, -1, -1, -1);
    /* truncate to (new_linecount + 1) lines (the +1 counts the sentinel) */
    net.states.truncate((new_linecount + 1) as usize);
    net.linecount = j + 1;
    net.arccount = arccount;
    net.statecount = m.total_states;
    net
}

// [spec:foma:def:minimize.refine-states-fn]
// [spec:foma:sem:minimize.refine-states-fn]
#[allow(non_snake_case)]
pub(crate) fn refine_states(m: &mut Minimizer, invstates: i32) -> bool {
    /*
       1. add inverse(P,a) to table of inverses, disallowing duplicates
       2. first pass on S, touch each state once, increasing P->t_count
       3. for each P where counter != count, split and add to agenda
    */
    /* Inverse to table of inverses */
    let mut selfsplit = false;

    /* touch and increase P->counter */
    for i in 0..invstates as usize {
        let s = m.temp_move[i] as usize;
        let g = m.e[s].group;
        m.phead[g].t_count += 1;
        m.phead[g].inv_t_count += m.e[s].inv_count;
        assert!(m.phead[g].t_count <= m.phead[g].count);
    }

    /* Split (this is the tricky part) */

    for i in 0..invstates as usize {
        let thise = m.temp_move[i] as usize; /* thise = E+*(temp_move+i) */
        let tP = m.e[thise].group;

        /* Do we need to split?
        if we've touched as many states as there are in the partition
        we don't split */

        if m.phead[tP].t_count == m.phead[tP].count {
            m.phead[tP].t_count = 0;
            m.phead[tP].inv_t_count = 0;
            continue;
        }

        if (m.phead[tP].t_count != m.phead[tP].count)
            && (m.phead[tP].count > 1)
            && (m.phead[tP].t_count > 0)
        {
            /* Check if we already split this */
            let mut newP = m.phead[tP].current_split;
            if newP.is_none() {
                /* Create new group newP */
                m.total_states += 1;
                if m.total_states == m.num_states {
                    return true; /* Abort now, machine is already minimal */
                }
                /* tP->current_split = Pnext++; */
                let np = m.pnext;
                m.pnext = np + 1;
                m.phead[tP].current_split = Some(np);
                newP = m.phead[tP].current_split;
                m.phead[np].first_e = Some(thise);
                m.phead[np].last_e = Some(thise);
                m.phead[np].count = 0;
                m.phead[np].inv_count = m.phead[tP].inv_t_count;
                m.phead[np].inv_t_count = 0;
                m.phead[np].t_count = 0;
                m.phead[np].current_split = None;
                m.phead[np].agenda = None;

                /* Add to agenda */

                /* If the current block (tP) is on the agenda, we add both back */
                /* to the agenda */
                /* In practice we need only add newP since tP stays where it is */
                /* However, we mark the larger one as not starting the symloop */
                /* from zero */
                if let Some(tp_agenda) = m.phead[tP].agenda {
                    /* Is tP smaller */
                    /* (latent quirk kept: inv_t_count sums a subset of tP's
                    members' inv_counts, so inv_count < inv_t_count is always
                    false and the else branch always runs) */
                    if m.phead[tP].inv_count < m.phead[tP].inv_t_count {
                        agenda_add(m, np, 1);
                        let ag = tp_agenda;
                        m.agenda_top[ag].index = false;
                    } else {
                        agenda_add(m, np, 0);
                    }
                    /* In the event that we're splitting the partition we're currently */
                    /* splitting with, we can simply add both new partitions to the agenda */
                    /* and break out of the entire sym loop after we're */
                    /* done with the current sym and move on with the agenda */
                    /* We process the larger one for all symbols */
                    /* and the smaller one for only the ones remaining in this symloop */
                } else if tP == m.current_w {
                    let smaller = if m.phead[tP].inv_count < m.phead[tP].inv_t_count {
                        tP
                    } else {
                        np
                    };
                    let larger = if m.phead[tP].inv_count >= m.phead[tP].inv_t_count {
                        tP
                    } else {
                        np
                    };
                    agenda_add(m, smaller, 0);
                    agenda_add(m, larger, 1);
                    selfsplit = true;
                } else {
                    /* If the block is not on the agenda, we add */
                    /* the smaller of tP, newP and start the symloop from 0 */
                    let smaller = if m.phead[tP].inv_count < m.phead[tP].inv_t_count {
                        tP
                    } else {
                        np
                    };
                    agenda_add(m, smaller, 0);
                }
                /* Add to middle of P-chain */
                /* newP->next = P->next; P->next = newP; — C derefs the chain
                head P unconditionally */
                let p = m.p.expect("P-chain head set before splitting");
                m.phead[np].next = m.phead[p].next;
                m.phead[p].next = Some(np);
            }

            let newP = newP.expect("newP set to current_split above");
            m.e[thise].group = newP;
            m.phead[newP].count += 1;

            /* need to make tP->last_e point to the last untouched e */
            if m.phead[tP].last_e == Some(thise) {
                m.phead[tP].last_e = m.e[thise].left;
            }
            if m.phead[tP].first_e == Some(thise) {
                m.phead[tP].first_e = m.e[thise].right;
            }

            /* Adjust links */
            if let Some(left) = m.e[thise].left {
                m.e[left].right = m.e[thise].right;
            }
            if let Some(right) = m.e[thise].right {
                m.e[right].left = m.e[thise].left;
            }

            if m.phead[newP].last_e != Some(thise) {
                let last = m.phead[newP]
                    .last_e
                    .expect("newP always has a last_e once populated");
                m.e[last].right = Some(thise);
                m.e[thise].left = Some(last);
                m.phead[newP].last_e = Some(thise);
            }

            m.e[thise].right = None;
            if m.phead[newP].first_e == Some(thise) {
                m.e[thise].left = None;
            }

            /* Are we done for this block? Adjust counters */
            if m.phead[newP].count == m.phead[tP].t_count {
                m.phead[tP].count -= m.phead[newP].count;
                m.phead[tP].inv_count -= m.phead[tP].inv_t_count;
                m.phead[tP].current_split = None;
                m.phead[tP].t_count = 0;
                m.phead[tP].inv_t_count = 0;
            }
        }
    }
    /* We return 1 if we just split the partition we were working with */
    selfsplit
}

// [spec:foma:def:minimize.agenda-add-fn]
// [spec:foma:sem:minimize.agenda-add-fn]
/* C: static void agenda_add(struct p *pptr, int start) — the struct p *
argument becomes the block-pool index `pptr` into the Minimizer */
pub(crate) fn agenda_add(m: &mut Minimizer, pptr: usize, start: i32) {
    /* Use FILO strategy here */

    let ag = m.agenda_next; /* ag = Agenda_next++ (no bounds check in C) */
    m.agenda_next = ag + 1;
    if m.agenda.is_some() {
        m.agenda_top[ag].next = m.agenda;
    } else {
        m.agenda_top[ag].next = None;
    }
    m.agenda_top[ag].p = pptr;
    m.agenda_top[ag].index = start != 0; /* int → _Bool */
    m.agenda = Some(ag);
    m.phead[pptr].agenda = Some(ag);
}

// [spec:foma:def:minimize.init-pe-fn]
// [spec:foma:sem:minimize.init-pe-fn]
#[allow(non_snake_case)]
pub(crate) fn init_PE(m: &mut Minimizer) {
    /* Create two members of P
       (nonfinals,finals)
       and put both of them on the agenda
    */

    let num_states = m.num_states;
    let num_finals = m.num_finals;

    m.mainloop = 1;
    m.memo_table = vec![0; num_states as usize];
    m.temp_move = vec![0; num_states as usize];
    m.temp_group = vec![0; num_states as usize];
    /* Phead = P = Pnext = calloc(num_states+1, sizeof(struct p)); */
    m.phead = vec![P::default(); (num_states + 1) as usize];
    m.p = Some(0);
    m.pnext = 0;
    /* nonFP = Pnext++; FP = Pnext++; */
    let nonFP = m.pnext;
    m.pnext = nonFP + 1;
    let FP = m.pnext;
    m.pnext = FP + 1;
    m.phead[nonFP].next = Some(FP);
    m.phead[nonFP].count = num_states - num_finals;
    m.phead[FP].next = None;
    m.phead[FP].count = num_finals;
    m.phead[FP].t_count = 0;
    m.phead[nonFP].t_count = 0;
    m.phead[FP].current_split = None;
    m.phead[nonFP].current_split = None;
    m.phead[FP].inv_count = 0;
    m.phead[nonFP].inv_count = 0;
    m.phead[FP].inv_t_count = 0;
    m.phead[nonFP].inv_t_count = 0;

    /* How many groups can we put on the agenda? */
    m.agenda_top = vec![Agenda::default(); (num_states * 2) as usize];
    m.agenda_next = 0;
    m.agenda_head = None;

    m.p = None;
    m.total_states = 0;

    if num_finals > 0 {
        let ag = m.agenda_next;
        m.agenda_next = ag + 1;
        m.phead[FP].agenda = Some(ag);
        m.p = Some(FP);
        m.phead[FP].next = None; /* P->next = NULL */
        m.agenda_top[ag].p = FP;
        m.agenda_head = Some(ag);
        m.agenda_top[ag].next = None;
        m.total_states += 1;
    }
    if num_states - num_finals > 0 {
        let ag = m.agenda_next;
        m.agenda_next = ag + 1;
        m.phead[nonFP].agenda = Some(ag);
        m.agenda_top[ag].p = nonFP;
        m.agenda_top[ag].next = None;
        m.total_states += 1;
        if let Some(head) = m.agenda_head {
            m.agenda_top[head].next = Some(ag);
            let p = m.p.expect("m.p set once the agenda head exists");
            m.phead[p].next = Some(nonFP);
            /* P->next->next = NULL; */
            m.phead[nonFP].next = None;
        } else {
            m.p = Some(nonFP);
            m.phead[nonFP].next = None;
            m.agenda_head = Some(ag);
        }
    }

    /* Initialize doubly linked list E */
    m.e = vec![E::default(); num_states as usize];

    let mut last_f: Option<usize> = None;
    let mut last_nonf: Option<usize> = None;

    for i in 0..num_states as usize {
        if m.finals[i] {
            m.e[i].group = FP;
            m.e[i].left = last_f;
            match last_f {
                Some(prev) if i > 0 => m.e[prev].right = Some(i),
                None => m.phead[FP].first_e = Some(i),
                _ => {}
            }
            last_f = Some(i);
            m.phead[FP].last_e = Some(i);
        } else {
            m.e[i].group = nonFP;
            m.e[i].left = last_nonf;
            match last_nonf {
                Some(prev) if i > 0 => m.e[prev].right = Some(i),
                None => m.phead[nonFP].first_e = Some(i),
                _ => {}
            }
            last_nonf = Some(i);
            m.phead[nonFP].last_e = Some(i);
        }
        m.e[i].inv_count = 0;
    }

    if let Some(lf) = last_f {
        m.e[lf].right = None;
    }
    if let Some(lnf) = last_nonf {
        m.e[lnf].right = None;
    }
}

// [spec:foma:def:minimize.trans-sort-cmp-fn]
// [spec:foma:sem:minimize.trans-sort-cmp-fn+1]
/* C: qsort comparator over const void * — typed slice elements here.
Ascending on the composite `inout` symbol. */
pub(crate) fn trans_sort_cmp(a: &TransList, b: &TransList) -> core::cmp::Ordering {
    a.inout.cmp(&b.inout)
}

// [spec:foma:def:minimize.generate-inverse-fn]
// [spec:foma:sem:minimize.generate-inverse-fn]
pub(crate) fn generate_inverse(m: &mut Minimizer, net: &Fsm) {
    m.trans_array_minimize = vec![TransArray::default(); net.statecount as usize];
    m.trans_list_minimize = vec![TransList::default(); net.arccount as usize];

    /* Figure out the number of transitions each one has */
    let mut i: usize = 0;
    while net.states[i].state_no != -1 {
        if net.states[i].target == -1 {
            i += 1;
            continue;
        }
        let target = net.states[i].target as usize;
        m.e[target].inv_count += 1;
        let g = m.e[target].group;
        m.phead[g].inv_count += 1;
        m.trans_array_minimize[target].size += 1;
        i += 1;
    }

    let mut offsetcount: i32 = 0;
    for i in 0..net.statecount as usize {
        m.trans_array_minimize[i].transitions = offsetcount as usize;
        offsetcount += m.trans_array_minimize[i].size as i32;
    }

    let mut i: usize = 0;
    while net.states[i].state_no != -1 {
        if net.states[i].target == -1 {
            i += 1;
            continue;
        }
        let symbol =
            symbol_pair_to_single_symbol(m, net.states[i].r#in as i32, net.states[i].out as i32);
        let source = net.states[i].state_no;
        let target = net.states[i].target as usize;
        let slot = m.trans_array_minimize[target].transitions
            + m.trans_array_minimize[target].tail as usize;
        m.trans_list_minimize[slot].inout = symbol;
        m.trans_list_minimize[slot].source = source;
        m.trans_array_minimize[target].tail += 1;
        i += 1;
    }

    /* Sort arcs (unstable; equal keys keep an unspecified relative order) */
    for i in 0..net.statecount as usize {
        let listptr = m.trans_array_minimize[i].transitions;
        let size = m.trans_array_minimize[i].size as i32;
        if size > 1 {
            m.trans_list_minimize[listptr..listptr + size as usize]
                .sort_unstable_by(trans_sort_cmp);
        }
    }
}

// [spec:foma:def:minimize.sigma-to-pairs-fn]
// [spec:foma:sem:minimize.sigma-to-pairs-fn]
pub(crate) fn sigma_to_pairs(m: &mut Minimizer, net: &mut Fsm) {
    let mut next_x: i32 = 0;

    m.epsilon_symbol = -1;
    m.maxsigma = sigma_max(&net.sigma) + 1;
    let maxsigma = m.maxsigma;

    /* two flat lookup tables: single (back-map, only read where written) and
    double (forward map, initialized to -1 below) */
    m.single_sigma_array = vec![0; (2 * maxsigma * maxsigma) as usize];
    m.double_sigma_array = vec![-1; (maxsigma * maxsigma) as usize];

    /* f(x) -> y,z sigma pair */
    /* f(y,z) -> x simple entry */
    /* if exists f(n) <-> EPSILON, EPSILON, save n */
    /* symbol(x) x>=1 */

    /* Forward mapping: */
    /* *(double_sigma_array+maxsigma*in+out) */

    /* Backmapping: */
    /* *(single_sigma_array+(symbol*2) = in(symbol) */
    /* *(single_sigma_array+(symbol*2+1) = out(symbol) */

    /* Table for checking whether a state is final */

    m.finals = vec![false; m.num_states as usize];
    let mut x: i32 = 0;
    m.num_finals = 0;
    net.arity = 1;
    let mut i: usize = 0;
    while net.states[i].state_no != -1 {
        let sno = net.states[i].state_no as usize;
        /* C: finals[state_no] != 1 on a _Bool */
        if net.states[i].final_state == 1 && !m.finals[sno] {
            m.num_finals += 1;
            m.finals[sno] = true;
        }
        let y = net.states[i].r#in as i32;
        let z = net.states[i].out as i32;
        if y != z || y == UNKNOWN || z == UNKNOWN {
            net.arity = 2;
        }
        if (y == -1) || (z == -1) {
            i += 1;
            continue;
        }
        if m.double_sigma_array[(maxsigma * y + z) as usize] == -1 {
            m.double_sigma_array[(maxsigma * y + z) as usize] = x;
            m.single_sigma_array[next_x as usize] = y;
            next_x += 1;
            m.single_sigma_array[next_x as usize] = z;
            next_x += 1;
            if y == EPSILON && z == EPSILON {
                m.epsilon_symbol = x;
            }
            x += 1;
        }
        i += 1;
    }
    m.num_symbols = x;
}

// [spec:foma:def:minimize.symbol-pair-to-single-symbol-fn]
// [spec:foma:sem:minimize.symbol-pair-to-single-symbol-fn]
pub(crate) fn symbol_pair_to_single_symbol(m: &Minimizer, r#in: i32, out: i32) -> i32 {
    m.double_sigma_array[(m.maxsigma * r#in + out) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{apply_clear, apply_down, apply_init};
    use crate::dynarray::{
        fsm_construct_add_arc, fsm_construct_done, fsm_construct_init, fsm_construct_set_final,
        fsm_construct_set_initial,
    };

    fn accepts(net: &Fsm, word: &str) -> Option<String> {
        let mut h = apply_init(net);
        let r = apply_down(&mut h, Some(word));
        apply_clear(h);
        r
    }

    /* Deterministic 3-state input for (a|b)+: states {1,2} are equivalent and
    must merge, exercising rebuild_machine's compaction. */
    fn build_ab_plus() -> Box<Fsm> {
        let mut hc = fsm_construct_init("m");
        fsm_construct_set_initial(&mut hc, 0);
        fsm_construct_add_arc(&mut hc, 0, 1, "a", "a");
        fsm_construct_add_arc(&mut hc, 0, 2, "b", "b");
        fsm_construct_add_arc(&mut hc, 1, 1, "a", "a");
        fsm_construct_add_arc(&mut hc, 1, 2, "b", "b");
        fsm_construct_add_arc(&mut hc, 2, 1, "a", "a");
        fsm_construct_add_arc(&mut hc, 2, 2, "b", "b");
        fsm_construct_set_final(&mut hc, 1);
        fsm_construct_set_final(&mut hc, 2);
        fsm_construct_done(hc)
    }

    /* NFA over {a}: L = a^n, n >= 2. */
    fn build_a_ge2() -> Box<Fsm> {
        let mut hc = fsm_construct_init("d");
        fsm_construct_set_initial(&mut hc, 0);
        fsm_construct_add_arc(&mut hc, 0, 0, "a", "a");
        fsm_construct_add_arc(&mut hc, 0, 1, "a", "a");
        fsm_construct_add_arc(&mut hc, 1, 2, "a", "a");
        fsm_construct_set_final(&mut hc, 2);
        fsm_construct_done(hc)
    }

    /* NFA over {a,b}: strings ending in 'a' (0-a->0, 0-b->0, 0-a->1 final). */
    fn build_ends_a() -> Box<Fsm> {
        let mut hc = fsm_construct_init("t");
        fsm_construct_set_initial(&mut hc, 0);
        fsm_construct_add_arc(&mut hc, 0, 0, "a", "a");
        fsm_construct_add_arc(&mut hc, 0, 0, "b", "b");
        fsm_construct_add_arc(&mut hc, 0, 1, "a", "a");
        fsm_construct_set_final(&mut hc, 1);
        fsm_construct_done(hc)
    }

    // End-to-end Hopcroft minimization: the whole hop pipeline (sigma_to_pairs,
    // init_PE, generate_inverse, trans_sort_cmp, refine_states no-split touch
    // pass, rebuild_machine's in-place compaction) collapses the equivalent
    // pair {1,2} to one state.
    // [spec:foma:sem:minimize.fsm-minimize-fn/test]
    // [spec:foma:sem:fomalib.fsm-minimize-fn/test]
    // [spec:foma:sem:minimize.fsm-minimize-hop-fn/test]
    // [spec:foma:sem:minimize.rebuild-machine-fn/test]
    // [spec:foma:sem:minimize.refine-states-fn/test]
    // [spec:foma:sem:minimize.symbol-pair-to-single-symbol-fn/test]
    #[test]
    fn minimize_hop_reduces_and_preserves_language() {
        let opts = &FomaOptions::default();
        let net = build_ab_plus();
        let m = fsm_minimize(opts, net);
        assert_eq!(m.statecount, 2, "3-state DFA minimizes to 2");
        assert_eq!(m.arccount, 4);
        assert_eq!(m.linecount, 5);
        assert_eq!(m.is_deterministic, YES);
        assert_eq!(m.is_minimized, YES);
        /* exactly one structural final state (the merged {1,2}) ... */
        let finals: Vec<i32> = m
            .states
            .iter()
            .filter(|s| s.state_no != -1 && s.final_state != 0)
            .map(|s| s.state_no)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        assert_eq!(finals, vec![1]);
        /* ... but finalcount stays 2: rebuild_machine does NOT recompute it
        (stale-count quirk preserved from the pre-merge fsm_count). */
        assert_eq!(m.finalcount, 2);
        assert_eq!(accepts(&m, ""), None);
        assert_eq!(accepts(&m, "a"), Some("a".to_string()));
        assert_eq!(accepts(&m, "ab"), Some("ab".to_string()));
        assert_eq!(accepts(&m, "bbaa"), Some("bbaa".to_string()));
    }

    // Brzozowski dispatch: with g_minimize_hopcroft == 0, fsm_minimize routes to
    // fsm_minimize_brz = determinize(reverse(determinize(reverse(net)))).
    // [spec:foma:sem:minimize.fsm-minimize-fn/test]
    // [spec:foma:sem:minimize.fsm-minimize-brz-fn/test]
    #[test]
    fn minimize_brzozowski_path() {
        let net = build_ab_plus();
        let opts = FomaOptions {
            minimize_hopcroft: false,
            ..FomaOptions::default()
        };
        let m = fsm_minimize(&opts, net);
        assert_eq!(m.statecount, 2);
        assert_eq!(m.is_deterministic, YES);
        assert_eq!(m.is_minimized, YES);
        assert_eq!(accepts(&m, ""), None);
        assert_eq!(accepts(&m, "a"), Some("a".to_string()));
        assert_eq!(accepts(&m, "abba"), Some("abba".to_string()));
    }

    // fsm_minimize_brz called directly yields the minimal deterministic core.
    // [spec:foma:sem:minimize.fsm-minimize-brz-fn/test]
    #[test]
    fn fsm_minimize_brz_direct() {
        let m = fsm_minimize_brz(build_ab_plus());
        assert_eq!(m.is_deterministic, YES);
        assert_eq!(m.statecount, 2);
        assert_eq!(accepts(&m, "a"), Some("a".to_string()));
        assert_eq!(accepts(&m, ""), None);
    }

    // fsm_minimize_hop empty-language shortcut: finalcount == 0 destroys net and
    // returns a fresh canonical empty set.
    // [spec:foma:sem:minimize.fsm-minimize-hop-fn/test]
    #[test]
    fn hop_empty_language_returns_empty_set() {
        let mut hc = fsm_construct_init("z");
        fsm_construct_set_initial(&mut hc, 0);
        fsm_construct_add_arc(&mut hc, 0, 1, "a", "a");
        fsm_construct_set_final(&mut hc, 1);
        let mut net = fsm_construct_done(hc);
        for s in net.states.iter_mut() {
            s.final_state = 0; /* strip all finals -> finalcount 0 after fsm_count */
        }
        let e = fsm_minimize_hop(net);
        assert_eq!(e.statecount, 1);
        assert_eq!(e.finalcount, 0);
        assert_eq!(e.arccount, 0);
        assert_eq!(e.linecount, 2);
        assert_eq!(e.is_deterministic, YES);
    }

    // Property: minimize(net) and determinize(net) accept the same word set
    // across several small NFAs (checked with apply_down on sample words).
    // [spec:foma:sem:minimize.fsm-minimize-fn/test]
    // [spec:foma:sem:fomalib.fsm-minimize-fn/test]
    #[test]
    fn minimize_determinize_language_equivalence() {
        fn check(net: Box<Fsm>, samples: &[(&str, bool)]) {
            let opts = &FomaOptions::default();
            let d = fsm_determinize(net.clone());
            let m = fsm_minimize(opts, net);
            for (w, exp) in samples {
                assert_eq!(
                    accepts(&d, w).is_some(),
                    *exp,
                    "determinize accepts {:?}",
                    w
                );
                assert_eq!(accepts(&m, w).is_some(), *exp, "minimize accepts {:?}", w);
            }
        }
        check(
            build_a_ge2(),
            &[
                ("", false),
                ("a", false),
                ("aa", true),
                ("aaa", true),
                ("aaaa", true),
            ],
        );
        check(
            build_ab_plus(),
            &[
                ("", false),
                ("a", true),
                ("b", true),
                ("ab", true),
                ("bba", true),
            ],
        );
        check(
            build_ends_a(),
            &[
                ("", false),
                ("a", true),
                ("b", false),
                ("ba", true),
                ("ab", false),
                ("aba", true),
            ],
        );
    }

    // init_PE builds the initial {nonfinals, finals} partition, weaves each
    // block's doubly linked member list in state order, seeds the agenda
    // (finals first) and the P chain.
    // [spec:foma:sem:minimize.init-pe-fn/test]
    #[test]
    fn init_pe_builds_initial_partition() {
        let mut m = Minimizer {
            num_states: 3,
            num_finals: 1,
            finals: vec![false, false, true],
            ..Default::default()
        };
        init_PE(&mut m);
        assert_eq!(m.total_states, 2);
        /* nonFP == block 0 (count 2), FP == block 1 (count 1) */
        assert_eq!(m.phead[0].count, 2);
        assert_eq!(m.phead[1].count, 1);
        assert_eq!(m.phead[0].first_e, Some(0));
        assert_eq!(m.phead[0].last_e, Some(1));
        assert_eq!(m.phead[1].first_e, Some(2));
        assert_eq!(m.phead[1].last_e, Some(2));
        /* P chain head is FP -> nonFP */
        assert_eq!(m.phead[1].next, Some(0));
        assert_eq!(m.phead[0].next, None);
        assert_eq!(m.phead[1].agenda, Some(0));
        assert_eq!(m.phead[0].agenda, Some(1));
        assert_eq!(m.p, Some(1));
        assert_eq!((m.e[0].group, m.e[1].group, m.e[2].group), (0, 0, 1));
        assert_eq!(m.e[0].left, None);
        assert_eq!(m.e[0].right, Some(1));
        assert_eq!(m.e[1].left, Some(0));
        assert_eq!(m.e[1].right, None);
        assert_eq!(m.e[2].left, None);
        assert_eq!(m.e[2].right, None);
        /* agenda: finals (FP) first, then nonfinals (nonFP) */
        assert_eq!(m.agenda_head, Some(0));
        assert_eq!(m.agenda_top[0].p, 1);
        assert_eq!(m.agenda_top[0].next, Some(1));
        assert_eq!(m.agenda_top[1].p, 0);
        assert_eq!(m.agenda_top[1].next, None);
    }

    // sigma_to_pairs (minimize variant) also fills the finals bitmap and counts
    // distinct final states, flags arity, and forward-maps every real pair.
    // [spec:foma:sem:minimize.sigma-to-pairs-fn/test]
    #[test]
    fn sigma_to_pairs_sets_finals_and_arity() {
        let mut hc = fsm_construct_init("s");
        fsm_construct_set_initial(&mut hc, 0);
        fsm_construct_add_arc(&mut hc, 0, 1, "a", "b"); /* a:b -> arity 2 */
        fsm_construct_add_arc(&mut hc, 1, 1, "a", "a");
        fsm_construct_set_final(&mut hc, 1);
        let mut net = fsm_construct_done(hc);
        let mut m = Minimizer {
            num_states: net.statecount,
            ..Default::default()
        };
        sigma_to_pairs(&mut m, &mut net);
        assert_eq!(net.arity, 2);
        assert_eq!(m.epsilon_symbol, -1);
        assert_eq!(m.num_finals, 1);
        assert!(m.finals[1] && !m.finals[0]);
        for st in net.states.iter() {
            let (i, o) = (st.r#in as i32, st.out as i32);
            if i < 0 || o < 0 {
                continue;
            }
            let c = symbol_pair_to_single_symbol(&m, i, o);
            assert!(c >= 0 && c < m.num_symbols);
        }
    }

    // generate_inverse: inverse in-degree per state (trans_array size,
    // E.inv_count) and inverse-arc source lists over a 2-state cycle.
    // [spec:foma:sem:minimize.generate-inverse-fn/test]
    #[test]
    fn generate_inverse_counts_and_sources() {
        let mut hc = fsm_construct_init("g");
        fsm_construct_set_initial(&mut hc, 0);
        fsm_construct_set_final(&mut hc, 0);
        fsm_construct_add_arc(&mut hc, 0, 1, "a", "a");
        fsm_construct_add_arc(&mut hc, 1, 0, "a", "a");
        let mut net = fsm_construct_done(hc);
        fsm_count(&mut net);
        let mut m = Minimizer {
            num_states: net.statecount,
            ..Default::default()
        };
        sigma_to_pairs(&mut m, &mut net);
        init_PE(&mut m);
        generate_inverse(&mut m, &net);
        assert_eq!(m.trans_array_minimize[0].size, 1);
        assert_eq!(m.trans_array_minimize[1].size, 1);
        assert_eq!(m.e[0].inv_count, 1);
        assert_eq!(m.e[1].inv_count, 1);
        /* inverse arc of state 0 comes from state 1 and vice versa; both carry
        the single composite symbol 0 */
        let t0 = m.trans_array_minimize[0].transitions;
        let t1 = m.trans_array_minimize[1].transitions;
        assert_eq!(
            (
                m.trans_list_minimize[t0].source,
                m.trans_list_minimize[t1].source
            ),
            (1, 0)
        );
        assert_eq!(m.trans_list_minimize[t0].inout, 0);
        assert_eq!(m.trans_list_minimize[t1].inout, 0);
    }

    // refine_states splits a block when only some members reach the splitter.
    // Hand-built partition: block 1 = {0,1,2}, S = {0,1} touched. The block
    // splits into newP = {0,1} (touched) and {2} (remainder). Verifies the
    // always-false inv_count < inv_t_count quirk: the touched half (newP) is the
    // one enqueued, with index 0.
    // [spec:foma:sem:minimize.refine-states-fn/test]
    // [spec:foma:sem:minimize.agenda-add-fn/test]
    #[test]
    fn refine_states_splits_and_enqueues_touched_half() {
        let mut m = Minimizer {
            num_states: 10, /* large: TOTAL never reaches it -> no abort */
            total_states: 3,
            /* block pool: index 1 is the splittable block tP, index 3 is free */
            phead: vec![P::default(); 6],
            ..Default::default()
        };
        m.phead[1].count = 3;
        m.phead[1].first_e = Some(0);
        m.phead[1].last_e = Some(2);
        m.phead[1].current_split = None;
        m.phead[1].agenda = None;
        m.phead[1].next = None;
        m.p = Some(1); /* chain head */
        m.pnext = 3;
        m.current_w = 5; /* tP (1) is NOT current_w */
        m.e = vec![E::default(); 3];
        for (i, ent) in m.e.iter_mut().enumerate() {
            ent.group = 1;
            ent.inv_count = 0;
            ent.left = if i == 0 { None } else { Some(i - 1) };
            ent.right = if i == 2 { None } else { Some(i + 1) };
        }
        m.temp_move = vec![0, 1, 0];
        m.agenda_top = vec![Agenda::default(); 4];
        m.agenda_next = 0;
        m.agenda = None;
        m.mainloop = 1;

        let selfsplit = refine_states(&mut m, 2);
        assert!(!selfsplit, "tP is not current_w");
        assert_eq!(m.total_states, 4, "one new block created");
        /* touched states 0,1 moved to newP (block 3); state 2 stays in tP */
        assert_eq!((m.e[0].group, m.e[1].group, m.e[2].group), (3, 3, 1));
        assert_eq!(m.phead[3].count, 2, "newP holds the two touched states");
        assert_eq!(m.phead[1].count, 1, "tP reduced to the untouched remainder");
        assert_eq!(m.phead[1].next, Some(3), "newP linked after the chain head");
        /* tP's remaining member list is just {2} */
        assert_eq!(m.phead[1].first_e, Some(2));
        assert_eq!(m.phead[1].last_e, Some(2));
        /* the touched half (newP == 3) is the one enqueued, index 0 (false) */
        assert_eq!(m.agenda, Some(0));
        assert_eq!(m.agenda_top[0].p, 3);
        assert!(!m.agenda_top[0].index);
        assert_eq!(m.phead[3].agenda, Some(0));
    }

    // trans_sort_cmp: ascending by composite symbol.
    // [spec:foma:sem:minimize.trans-sort-cmp-fn+1/test]
    #[test]
    fn trans_sort_cmp_orders_by_inout() {
        use core::cmp::Ordering;
        let a = TransList {
            inout: 9,
            source: 0,
        };
        let b = TransList {
            inout: 4,
            source: 1,
        };
        assert_eq!(trans_sort_cmp(&a, &b), Ordering::Greater);
        assert_eq!(trans_sort_cmp(&b, &a), Ordering::Less);
        assert_eq!(trans_sort_cmp(&a, &a), Ordering::Equal);
    }
}
