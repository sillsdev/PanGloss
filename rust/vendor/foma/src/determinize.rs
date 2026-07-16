//! foma/determinize.c — literal (bug-for-bug) Wave-2 port per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/determinize.md
//! (per-file ids) plus the fomalib.h prototype ids for fsm_determinize /
//! fsm_epsilon_remove.
//!
//! Subset construction / epsilon removal engine. The C's pointer-chain
//! structures become index-based pools with the identical link discipline
//! (see minimize.rs for the established pattern):
//! - e_closure_memo: calloc'd head-node array + malloc'd chain nodes → one
//!   pool Vec (heads at indices 0..num_states, state number == index; chain
//!   nodes appended after); `target`/`next` pointers → Option<usize> pool
//!   indices; the DFS pushes pool indices on the ptr stack where the C
//!   pushes node pointers.
//! - nhash table: calloc'd bucket-head array → Vec<NhashList>, malloc'd
//!   collision nodes → owned Option<Box<NhashList>> chains with the same
//!   splice-after-head order.
//! - trans_array/trans_list: per-state interior pointers into the shared
//!   entry pool → base offsets (usize).
//!
//! The C's file-static scratch (subset-construction pools, nhash table, sigma
//! maps, the numss/mainloop counters, the int/ptr worklist stacks, …) is owned
//! by a per-call `Subset` struct — nothing survives a call. The `fsm_state_*`
//! line-array build is a `FsmBuilder` threaded through the call; this module
//! keeps no state of its own.

use crate::constructions::fsm_count;
use crate::dynarray::{
    fsm_state_add_arc, fsm_state_close, fsm_state_end_state, fsm_state_init,
    fsm_state_set_current_state,
};
use crate::int_stack::{IntStack, PtrStack};
use crate::mem::next_power_of_two;
use crate::sigma::sigma_max;
use crate::types::{EPSILON, Fsm, FsmState, UNKNOWN, YES};

/* C: #define SUBSET_EPSILON_REMOVE 1 / SUBSET_DETERMINIZE 2 /
SUBSET_TEST_STAR_FREE 3 */
pub const SUBSET_EPSILON_REMOVE: i32 = 1;
pub const SUBSET_DETERMINIZE: i32 = 2;
pub const SUBSET_TEST_STAR_FREE: i32 = 3;

/// load limit for nhash table size
pub const NHASH_LOAD_LIMIT: i32 = 2;

// [spec:foma:def:determinize.e-closure-memo]
/* target/next are pool indices into E_CLOSURE_MEMO (None ↔ NULL); head
nodes occupy indices 0..num_states (targets always reference head nodes, so
a target index is the target's state number); chain nodes are appended
after. Chain nodes' mark is malloc garbage in C and never read — 0 here. */
#[derive(Debug, Clone, Default)]
pub struct EClosureMemo {
    pub state: i32,
    pub mark: i32,
    pub target: Option<usize>,
    pub next: Option<usize>,
}

/* C: static unsigned int primes[26] = {...}; (never mutated) */
pub(crate) static PRIMES: [u32; 26] = [
    61, 127, 251, 509, 1021, 2039, 4093, 8191, 16381, 32749, 65521, 131071, 262139, 524287,
    1048573, 2097143, 4194301, 8388593, 16777213, 33554393, 67108859, 134217689, 268435399,
    536870909, 1073741789, 2147483647,
];

// [spec:foma:def:determinize.nhash-list]
/* size == 0 marks an empty bucket head (calloc); collision nodes hang off
`next` as an owned chain, spliced in directly after the head as in C. */
#[derive(Debug, Clone, Default)]
pub struct NhashList {
    pub setnum: i32,
    pub size: u32,
    pub set_offset: u32,
    pub next: Option<Box<NhashList>>,
}

// [spec:foma:def:determinize.t-memo]
#[derive(Debug, Clone, Default)]
pub struct TMemo {
    pub finalstart: u8,
    pub size: u32,
    pub set_offset: u32,
}

// [spec:foma:def:determinize.trans-list]
#[derive(Debug, Clone, Default)]
pub struct TransList {
    pub inout: i32,
    pub target: i32,
}

// [spec:foma:def:determinize.trans-array]
/* transitions is the C's struct trans_list * interior pointer into the
trans_list_determinize pool — here the base offset of this state's slice. */
#[derive(Debug, Clone, Default)]
pub struct TransArray {
    pub transitions: usize,
    pub size: u32,
    pub tail: u32,
}

/// Per-call subset-construction scratch. The C kept every field below as a
/// file-static; Wave 4 folds them into one owned struct created fresh in
/// `fsm_subset`, so a determinize/epsilon-remove call owns all its own state
/// and nothing persists between calls. `Default` gives the C's zeroed BSS
/// start (0 / false / empty Vec).
#[derive(Debug, Default)]
pub(crate) struct Subset {
    // C: static int fsm_linecount, num_states, num_symbols, epsilon_symbol,
    //    *single_sigma_array, *double_sigma_array, limit, num_start_states, op;
    // written by init to mirror the C statics but never read back in the port
    #[allow(dead_code)]
    fsm_linecount: i32,
    num_states: i32,
    num_symbols: i32,
    epsilon_symbol: i32,
    single_sigma_array: Vec<i32>,
    double_sigma_array: Vec<i32>,
    #[allow(dead_code)]
    limit: i32,
    num_start_states: i32,
    op: i32,

    // C: static _Bool *finals, deterministic, numss;
    finals: Vec<bool>,
    deterministic: bool,
    numss: bool,

    // C: static struct e_closure_memo *e_closure_memo; — head-node array
    // plus malloc'd chain nodes, all in one pool here (see module docs)
    e_closure_memo: Vec<EClosureMemo>,

    // C: int T_last_unmarked, T_limit;
    #[allow(dead_code)]
    t_last_unmarked: i32,
    t_limit: i32,

    // C: struct trans_list *trans_list_determinize; struct trans_array
    // *trans_array_determinize;
    trans_list_determinize: Vec<TransList>,
    trans_array_determinize: Vec<TransArray>,

    // C: static struct T_memo *T_ptr;
    t_ptr: Vec<TMemo>,

    // C: static int nhash_tablesize, nhash_load, current_setnum, *e_table,
    //    *marktable, *temp_move, mainloop, maxsigma, *set_table,
    //    set_table_size, star_free_mark; unsigned int set_table_offset;
    nhash_tablesize: i32,
    nhash_load: i32,
    current_setnum: i32,
    e_table: Vec<i32>,
    marktable: Vec<i32>,
    temp_move: Vec<i32>,
    mainloop: i32,
    maxsigma: i32,
    set_table: Vec<i32>,
    set_table_size: i32,
    star_free_mark: i32,
    set_table_offset: u32,
    // C: static struct nhash_list *table;
    table: Vec<NhashList>,

    // C used the shared int_stack/ptr_stack scratch; the subset build owns
    // its own here (worklist agenda of subset numbers; DFS stack of
    // e_closure_memo pool indices).
    int_stack: IntStack,
    ptr_stack: PtrStack,
}

// [spec:foma:def:determinize.add-fsm-arc-fn]
// [spec:foma:sem:determinize.add-fsm-arc-fn]
/* C: extern int add_fsm_arc(struct fsm_state *fsm, int offset, int state_no,
int in, int out, int target, int final_state, int start_state); — an extern
declaration only; the definition lives in constructions.c (constructions.rs
here). determinize.c never calls it; this re-export mirrors the extern
declaration's visibility. */
pub use crate::constructions::add_fsm_arc;

// [spec:foma:def:determinize.fsm-epsilon-remove-fn]
// [spec:foma:sem:determinize.fsm-epsilon-remove-fn]
// [spec:foma:def:fomalib.fsm-epsilon-remove-fn]
// [spec:foma:sem:fomalib.fsm-epsilon-remove-fn]
pub fn fsm_epsilon_remove(net: Box<Fsm>) -> Box<Fsm> {
    fsm_subset(net, SUBSET_EPSILON_REMOVE)
}

// [spec:foma:def:determinize.fsm-determinize-fn]
// [spec:foma:sem:determinize.fsm-determinize-fn]
// [spec:foma:def:fomalib.fsm-determinize-fn]
// [spec:foma:sem:fomalib.fsm-determinize-fn]
pub fn fsm_determinize(net: Box<Fsm>) -> Box<Fsm> {
    fsm_subset(net, SUBSET_DETERMINIZE)
}

// [spec:foma:def:determinize.fsm-subset-fn]
// [spec:foma:sem:determinize.fsm-subset-fn]
#[allow(non_snake_case)]
pub(crate) fn fsm_subset(net: Box<Fsm>, operation: i32) -> Box<Fsm> {
    let mut net = net;
    let mut T: i32;
    let mut U: i32;

    if net.is_deterministic == YES && operation != SUBSET_TEST_STAR_FREE {
        return net;
    }
    /* all subset-construction scratch is owned here and dropped on return */
    let mut s = Subset {
        /* Export this var */
        op: operation,
        ..Default::default()
    };
    fsm_count(&mut net);
    s.num_states = net.statecount;
    s.deterministic = true;
    init(&mut s, &mut net);
    let num_states = s.num_states;
    nhash_init(&mut s, if num_states < 12 { 6 } else { num_states / 2 });

    T = initial_e_closure(&mut s, &net);

    s.int_stack.clear();

    /* numss is a C _Bool holding the truncated last-seen start state number,
    so numss == 0 really means "the single start state is state 0". Benign
    quirk kept: numss mis-tests any single start state != 0 as "not state 0",
    but it only gates the already-deterministic shortcut — never the result
    (the full path below produces the same language). */
    if s.deterministic && s.epsilon_symbol == -1 && s.num_start_states == 1 && !s.numss {
        net.is_deterministic = YES;
        net.is_epsilon_free = YES;
        /* iterative free of the nhash Box chains; dropping `s` frees the rest */
        nhash_free(std::mem::take(&mut s.table), s.nhash_tablesize);
        return net;
    }

    if operation == SUBSET_EPSILON_REMOVE && s.epsilon_symbol == -1 {
        net.is_epsilon_free = YES;
        nhash_free(std::mem::take(&mut s.table), s.nhash_tablesize);
        return net;
    }

    let mut builder = if operation == SUBSET_TEST_STAR_FREE {
        let sm = sigma_max(&net.sigma);
        let builder = fsm_state_init(sm + 1);
        s.star_free_mark = 0;
        builder
    } else {
        let sm = sigma_max(&net.sigma);
        let builder = fsm_state_init(sm);
        /* consume the old line table; fsm_state_close installs the rebuilt
        one at the end */
        net.states = Vec::new();
        builder
    };

    /* init */

    loop {
        'stateloop: {
            let mut symbol_in: i32 = 0;
            let mut symbol_out: i32 = 0;

            let finalstart = s.t_ptr[T as usize].finalstart;
            fsm_state_set_current_state(
                &mut builder,
                T,
                finalstart as i32,
                if T == 0 { 1 } else { 0 },
            );

            /* Prepare set */
            let setsize = s.t_ptr[T as usize].size as i32;
            let mut theset = s.t_ptr[T as usize].set_offset as usize;
            let mut minsym: i32 = i32::MAX; /* INT_MAX */
            let mut has_trans = 0;
            for i in 0..setsize {
                let stateno = s.set_table[theset + i as usize];
                let tptr = &mut s.trans_array_determinize[stateno as usize];
                tptr.tail = 0;
                let size0 = tptr.size;
                let tbase = tptr.transitions;
                if size0 == 0 {
                    continue;
                }
                let inout0 = s.trans_list_determinize[tbase].inout;
                if inout0 < minsym {
                    minsym = inout0;
                    has_trans = 1;
                }
            }
            if has_trans == 0 {
                /* close state */
                fsm_state_end_state(&mut builder);
                break 'stateloop; /* continue */
            }

            /* While set not empty */

            let mut next_minsym: i32 = i32::MAX;
            while minsym != i32::MAX {
                /* re-read each round (matches the C's set_table re-fetch) */
                theset = s.t_ptr[T as usize].set_offset as usize;

                let mut j: i32 = 0;
                for i in 0..setsize {
                    let stateno = s.set_table[theset + i as usize];
                    /* tail is a local copy; transitions walks the pool from
                    tptr.transitions + tail */
                    let tptr = &s.trans_array_determinize[stateno as usize];
                    let (mut tail, tbase, tsize) = (tptr.tail, tptr.transitions, tptr.size);

                    while tail < tsize {
                        let transitions = &s.trans_list_determinize[tbase + tail as usize];
                        let (inout, trgt) = (transitions.inout, transitions.target);
                        if inout != minsym {
                            break;
                        }
                        let marked = s.e_table[trgt as usize];
                        if marked != s.mainloop {
                            s.e_table[trgt as usize] = s.mainloop;
                            s.temp_move[j as usize] = trgt;
                            j += 1;

                            if operation == SUBSET_EPSILON_REMOVE {
                                s.mainloop += 1;
                                U = e_closure(&mut s, j);
                                if U != -1 {
                                    single_symbol_to_symbol_pair(
                                        &s,
                                        minsym,
                                        &mut symbol_in,
                                        &mut symbol_out,
                                    );
                                    let fs = s.t_ptr[T as usize].finalstart;
                                    fsm_state_add_arc(
                                        &mut builder,
                                        T,
                                        symbol_in,
                                        symbol_out,
                                        U,
                                        fs as i32,
                                        if T == 0 { 1 } else { 0 },
                                    );
                                    j = 0;
                                }
                            }
                        }
                        tail += 1;
                    }

                    s.trans_array_determinize[stateno as usize].tail = tail;

                    if tail == tsize {
                        continue;
                    }
                    /* Check next minsym */
                    let inout = s.trans_list_determinize[tbase + tail as usize].inout;
                    if inout < next_minsym {
                        next_minsym = inout;
                    }
                }
                if operation == SUBSET_DETERMINIZE {
                    s.mainloop += 1;
                    U = e_closure(&mut s, j);
                    if U != -1 {
                        single_symbol_to_symbol_pair(&s, minsym, &mut symbol_in, &mut symbol_out);
                        let fs = s.t_ptr[T as usize].finalstart;
                        fsm_state_add_arc(
                            &mut builder,
                            T,
                            symbol_in,
                            symbol_out,
                            U,
                            fs as i32,
                            if T == 0 { 1 } else { 0 },
                        );
                    }
                }
                if operation == SUBSET_TEST_STAR_FREE {
                    s.mainloop += 1;
                    U = e_closure(&mut s, j);
                    if U != -1 {
                        single_symbol_to_symbol_pair(&s, minsym, &mut symbol_in, &mut symbol_out);
                        let fs = s.t_ptr[T as usize].finalstart;
                        fsm_state_add_arc(
                            &mut builder,
                            T,
                            symbol_in,
                            symbol_out,
                            U,
                            fs as i32,
                            if T == 0 { 1 } else { 0 },
                        );
                        if s.star_free_mark == 1 {
                            //fsm_state_add_arc(T, maxsigma, maxsigma, U, (T_ptr+T)->finalstart, T == 0 ? 1 : 0);
                            s.star_free_mark = 0;
                        }
                    }
                }
                minsym = next_minsym;
                next_minsym = i32::MAX;
            }
            /* end state */
            fsm_state_end_state(&mut builder);
        }
        T = next_unmarked(&mut s);
        if T == -1 {
            break;
        }
    }

    /* wrapup(): iterative free of the nhash Box chains and the e-closure
    memo; the rest of the scratch is freed when `s` drops */
    nhash_free(std::mem::take(&mut s.table), s.nhash_tablesize);
    if s.epsilon_symbol != -1 {
        e_closure_free(&mut s);
    }
    fsm_state_close(&mut builder, &mut net);
    net
}

// [spec:foma:def:determinize.init-fn]
// [spec:foma:sem:determinize.init-fn]
pub(crate) fn init(s: &mut Subset, net: &mut Fsm) {
    /* A temporary table for handling epsilon closure */
    /* to avoid doubles */

    s.e_table = vec![0; net.statecount as usize];

    /* Counter for our access tables */

    s.mainloop = 1;

    /* Temporary table for storing sets and */
    /* passing to hash function */

    /* Table for listing current results of move & e-closure
    (write-before-read scratch) */
    s.temp_move = vec![0; (net.statecount + 1) as usize];

    /* We malloc this much memory to begin with for the new fsm */
    /* Then grow it by the double as needed */

    s.limit = next_power_of_two(net.linecount);
    s.fsm_linecount = 0;
    sigma_to_pairs(s, net);

    /* Optimistically malloc T_ptr array */
    /* We allocate memory for a number of pointers to a set of states */
    /* To handle fast lookup in array */
    /* Optimistically, we choose the initial size to be the number of */
    /* states in the non-deterministic fsm */

    s.t_last_unmarked = 0;
    s.t_limit = next_power_of_two(s.num_states);

    /* T_ptr = calloc(T_limit,sizeof(struct T_memo)); */
    s.t_ptr = vec![TMemo::default(); s.t_limit as usize];

    /* Stores all sets consecutively in one table */
    /* T_ptr->set_offset and size                 */
    /* are used to retrieve the set               */

    s.set_table_size = next_power_of_two(s.num_states);
    s.set_table = vec![0; s.set_table_size as usize];
    s.set_table_offset = 0;

    init_trans_array(s, net);
}

// [spec:foma:def:determinize.trans-sort-cmp-fn]
// [spec:foma:sem:determinize.trans-sort-cmp-fn+1]
/* C: qsort comparator over const void * — typed references here.
Ascending on the composite `inout` symbol. */
pub(crate) fn trans_sort_cmp(a: &TransList, b: &TransList) -> core::cmp::Ordering {
    a.inout.cmp(&b.inout)
}

// [spec:foma:def:determinize.init-trans-array-fn]
// [spec:foma:sem:determinize.init-trans-array-fn]
pub(crate) fn init_trans_array(s: &mut Subset, net: &Fsm) {
    /* one entry pool sized to the line count, one per-state index array
    (Default-filled) */
    s.trans_list_determinize = vec![TransList::default(); net.linecount as usize];
    s.trans_array_determinize = vec![TransArray::default(); net.statecount as usize];

    let fsm = &net.states;

    let mut laststate: i32 = -1;
    /* arrptr walks the shared entry pool — an index here */
    let mut arrptr: usize = 0;
    let mut size: u32 = 0;

    let mut i = 0usize;
    while fsm[i].state_no != -1 {
        let state = fsm[i].state_no;
        if state != laststate {
            if laststate != -1 {
                s.trans_array_determinize[laststate as usize].size = size;
            }
            s.trans_array_determinize[state as usize].transitions = arrptr;
            size = 0;
        }
        laststate = state;

        if fsm[i].target == -1 {
            i += 1;
            continue;
        }
        let inout = symbol_pair_to_single_symbol(s, fsm[i].r#in as i32, fsm[i].out as i32);
        if inout == s.epsilon_symbol {
            i += 1;
            continue;
        }

        s.trans_list_determinize[arrptr].inout = inout;
        s.trans_list_determinize[arrptr].target = fsm[i].target;
        arrptr += 1;
        size += 1;
        i += 1;
    }

    if laststate != -1 {
        s.trans_array_determinize[laststate as usize].size = size;
    }

    for i in 0..net.statecount as usize {
        let arrptr = s.trans_array_determinize[i].transitions;
        let size = s.trans_array_determinize[i].size;
        if size > 1 {
            /* unstable sort by symbol; equal keys keep an unspecified order */
            s.trans_list_determinize[arrptr..arrptr + size as usize]
                .sort_unstable_by(trans_sort_cmp);
            let mut lastsym = -1;
            /* Figure out if we're already deterministic */
            for j in 0..size as usize {
                if s.trans_list_determinize[arrptr + j].inout == lastsym {
                    s.deterministic = false;
                }
                lastsym = s.trans_list_determinize[arrptr + j].inout;
            }
        }
    }
}

// [spec:foma:def:determinize.e-closure-fn]
// [spec:foma:sem:determinize.e-closure-fn]
/* C: INLINE static int e_closure(int states) */
pub(crate) fn e_closure(s: &mut Subset, states: i32) -> i32 {
    /* e_closure extends the list of states which are reachable */
    /* and appends these to e_table                             */

    if s.epsilon_symbol == -1 {
        /* set_lookup reads only e_table/set_table/table/finals, never
        temp_move, so lend the set out and hand it straight back */
        let tm = std::mem::take(&mut s.temp_move);
        let r = set_lookup(s, &tm, states);
        s.temp_move = tm;
        return r;
    }

    if states == 0 {
        return -1;
    }

    s.mainloop -= 1;
    let mainloop = s.mainloop;

    let mut set_size = states;

    for i in 0..states {
        /* State number we want to do e-closure on
        (ptr = e_closure_memo + temp_move[i] — a pool index) */
        let mut ptr = s.temp_move[i as usize] as usize;
        if s.e_closure_memo[ptr].target.is_none() {
            continue;
        }
        s.ptr_stack.push(ptr);

        while !s.ptr_stack.is_empty() {
            ptr = s.ptr_stack.pop();
            let state = s.e_closure_memo[ptr].state as usize;
            /* Don't follow if already seen */
            if s.marktable[state] == mainloop {
                continue;
            }

            s.e_closure_memo[ptr].mark = mainloop;
            s.marktable[state] = mainloop;
            /* Add to tail of list */
            if s.e_table[state] != mainloop {
                s.temp_move[set_size as usize] = state as i32;
                s.e_table[state] = mainloop;
                set_size += 1;
            }

            if s.e_closure_memo[ptr].target.is_none() {
                continue;
            }
            /* Traverse chain */

            let mut p: Option<usize> = Some(ptr);
            while let Some(pi) = p {
                /* chain nodes always carry a target (head targets checked above) */
                let tgt = s.e_closure_memo[pi]
                    .target
                    .expect("chain node always carries a target");
                if s.e_closure_memo[tgt].mark != mainloop {
                    /* Push */
                    s.e_closure_memo[tgt].mark = mainloop;
                    s.ptr_stack.push(tgt);
                }
                p = s.e_closure_memo[pi].next;
            }
        }
    }

    s.mainloop += 1;
    let tm = std::mem::take(&mut s.temp_move);
    let r = set_lookup(s, &tm, set_size);
    s.temp_move = tm;
    r
}

// [spec:foma:def:determinize.set-lookup-fn]
// [spec:foma:sem:determinize.set-lookup-fn]
/* C: INLINE static int set_lookup (int *lookup_table, int size) */
pub(crate) fn set_lookup(s: &mut Subset, lookup_table: &[i32], size: i32) -> i32 {
    /* Look up a set and its corresponding state number */
    /* if it doesn't exist from before, assign a state number */

    nhash_find_insert(s, lookup_table, size)
}

// [spec:foma:def:determinize.add-t-ptr-fn]
// [spec:foma:sem:determinize.add-t-ptr-fn]
/* External linkage in C (not static) even though internal to the module */
#[allow(non_snake_case)]
pub(crate) fn add_T_ptr(s: &mut Subset, setnum: i32, setsize: i32, theset: u32, fs: i32) {
    if setnum >= s.t_limit {
        s.t_limit *= 2;
        let t_limit = s.t_limit;
        /* the grown region only needs .size == 0 (the "unused" sentinel);
        Default gives that */
        s.t_ptr.resize(t_limit as usize, TMemo::default());
        for i in setnum..t_limit {
            s.t_ptr[i as usize].size = 0;
        }
    }

    s.t_ptr[setnum as usize].size = setsize as u32;
    s.t_ptr[setnum as usize].set_offset = theset;
    /* int → unsigned char truncation */
    s.t_ptr[setnum as usize].finalstart = fs as u8;
    s.int_stack.push(setnum);
}

// [spec:foma:def:determinize.initial-e-closure-fn]
// [spec:foma:sem:determinize.initial-e-closure-fn]
pub(crate) fn initial_e_closure(s: &mut Subset, net: &Fsm) -> i32 {
    /* finals = calloc(num_states, sizeof(_Bool)); */
    let num_states = s.num_states;
    s.finals = vec![false; num_states as usize];

    s.num_start_states = 0;
    let fsm = &net.states;

    /* Create lookups for each state */
    let mut j: i32 = 0;
    let mut i = 0usize;
    while fsm[i].state_no != -1 {
        let state = fsm[i].state_no as usize;
        if fsm[i].final_state != 0 {
            s.finals[state] = true;
        }
        /* Add the start states as the initial set */
        if ((s.op == SUBSET_TEST_STAR_FREE) || fsm[i].start_state != 0)
            && s.e_table[state] != s.mainloop
        {
            s.num_start_states += 1;
            /* numss = (fsm+i)->state_no; — numss is a C _Bool, so the
            assignment truncates to state_no != 0 */
            s.numss = fsm[i].state_no != 0;
            s.e_table[state] = s.mainloop;
            s.temp_move[j as usize] = fsm[i].state_no;
            j += 1;
        }
        i += 1;
    }
    s.mainloop += 1;
    /* Memoize e-closure(u) */
    if s.epsilon_symbol != -1 {
        memoize_e_closure(s, fsm);
    }
    e_closure(s, j)
}

// [spec:foma:def:determinize.memoize-e-closure-fn]
// [spec:foma:sem:determinize.memoize-e-closure-fn]
pub(crate) fn memoize_e_closure(s: &mut Subset, fsm: &[FsmState]) {
    let num_states = s.num_states;

    /* e_closure_memo = calloc(num_states,...); marktable = calloc(...) */
    s.e_closure_memo = vec![EClosureMemo::default(); num_states as usize];
    s.marktable = vec![0; num_states as usize];
    /* Table for avoiding redundant epsilon arcs in closure (set to -1 below) */
    let mut redcheck: Vec<i32> = vec![-1; num_states as usize];

    for i in 0..num_states as usize {
        s.e_closure_memo[i].state = i as i32;
        s.e_closure_memo[i].target = None;
    }

    let mut laststate: i32 = -1;

    let mut i = 0usize;
    loop {
        let state = fsm[i].state_no;

        if state != laststate && !s.int_stack.is_empty() {
            s.deterministic = false;
            /* ptr = e_closure_memo+laststate; */
            let mut ptr = laststate as usize;
            /* ptr->target = e_closure_memo+s.int_stack.pop(); — target
            indices are head-node indices (state numbers) */
            s.e_closure_memo[ptr].target = Some(s.int_stack.pop() as usize);
            while !s.int_stack.is_empty() {
                /* append a chain node to the pool (its mark is never read
                on chain nodes; 0 here) */
                s.e_closure_memo.push(EClosureMemo {
                    state: laststate,
                    mark: 0,
                    target: Some(s.int_stack.pop() as usize),
                    next: None,
                });
                let ni = s.e_closure_memo.len() - 1;
                s.e_closure_memo[ptr].next = Some(ni);
                ptr = ni;
            }
        }
        if state == -1 {
            break;
        }
        if fsm[i].target == -1 {
            i += 1;
            continue;
        }
        /* Check if we have a redundant epsilon arc */
        if fsm[i].r#in as i32 == EPSILON && fsm[i].out as i32 == EPSILON {
            if redcheck[fsm[i].target as usize] != fsm[i].state_no
                && fsm[i].target != fsm[i].state_no
            {
                s.int_stack.push(fsm[i].target);
                redcheck[fsm[i].target as usize] = fsm[i].state_no;
            }
            laststate = state;
        }
        i += 1;
    }
    /* redcheck dropped here */
}

// [spec:foma:def:determinize.next-unmarked-fn]
// [spec:foma:sem:determinize.next-unmarked-fn]
pub(crate) fn next_unmarked(s: &mut Subset) -> i32 {
    if s.int_stack.is_empty() {
        return -1;
    }
    s.int_stack.pop()

    /* Everything after the return in the C (a sequential T_last_unmarked
    scan terminating on T_limit or a zero-size T_ptr entry) is unreachable
    dead code from an earlier FIFO design — not ported per the sem rule. */
}

// [spec:foma:def:determinize.single-symbol-to-symbol-pair-fn]
// [spec:foma:sem:determinize.single-symbol-to-symbol-pair-fn]
pub(crate) fn single_symbol_to_symbol_pair(
    s: &Subset,
    symbol: i32,
    symbol_in: &mut i32,
    symbol_out: &mut i32,
) {
    *symbol_in = s.single_sigma_array[(symbol * 2) as usize];
    *symbol_out = s.single_sigma_array[(symbol * 2 + 1) as usize];
}

// [spec:foma:def:determinize.symbol-pair-to-single-symbol-fn]
// [spec:foma:sem:determinize.symbol-pair-to-single-symbol-fn]
pub(crate) fn symbol_pair_to_single_symbol(s: &Subset, r#in: i32, out: i32) -> i32 {
    s.double_sigma_array[(s.maxsigma * r#in + out) as usize]
}

// [spec:foma:def:determinize.sigma-to-pairs-fn]
// [spec:foma:sem:determinize.sigma-to-pairs-fn]
pub(crate) fn sigma_to_pairs(s: &mut Subset, net: &mut Fsm) {
    let mut next_x: i32 = 0;

    s.epsilon_symbol = -1;
    s.maxsigma = sigma_max(&net.sigma) + 1;
    let maxsigma = s.maxsigma;

    /* two flat lookup tables: single (back-map, only read where written) and
    double (forward map, initialized to -1 below) */
    s.single_sigma_array = vec![0; (2 * maxsigma * maxsigma) as usize];
    s.double_sigma_array = vec![-1; (maxsigma * maxsigma) as usize];

    /* f(x) -> y,z sigma pair */
    /* f(y,z) -> x simple entry */
    /* if exists f(n) <-> EPSILON, EPSILON, save n */
    /* symbol(x) x>=1 */

    /* Forward mapping: */
    /* *(double_sigma_array+maxsigma*in+out) */

    /* Backmapping: */
    /* *(single_sigma_array+(symbol*2) = in(symbol) */
    /* *(single_sigma_array+(symbol*2+1) = out(symbol) */

    let mut x: i32 = 0;
    net.arity = 1;
    let mut i = 0usize;
    while net.states[i].state_no != -1 {
        let y = net.states[i].r#in as i32;
        let z = net.states[i].out as i32;
        if (y == -1) || (z == -1) {
            i += 1;
            continue;
        }
        if y != z || y == UNKNOWN || z == UNKNOWN {
            net.arity = 2;
        }
        if s.double_sigma_array[(maxsigma * y + z) as usize] == -1 {
            s.double_sigma_array[(maxsigma * y + z) as usize] = x;
            s.single_sigma_array[next_x as usize] = y;
            next_x += 1;
            s.single_sigma_array[next_x as usize] = z;
            next_x += 1;
            if y == EPSILON && z == EPSILON {
                s.epsilon_symbol = x;
            }
            x += 1;
        }
        i += 1;
    }
    s.num_symbols = x;
}

/* Functions for hashing n integers */
/* with permutations hashing to the same value */
/* necessary for subset construction */

// [spec:foma:def:determinize.nhash-find-insert-fn]
// [spec:foma:sem:determinize.nhash-find-insert-fn]
pub(crate) fn nhash_find_insert(s: &mut Subset, set: &[i32], setsize: i32) -> i32 {
    /* hashf's int return; values stay below nhash_tablesize */
    let mut hashval = hashf(s, set, setsize);
    let head_size = s.table[hashval as usize].size;
    if head_size == 0 {
        nhash_insert(s, hashval, set, setsize)
    } else {
        let mainloop = s.mainloop;
        let op = s.op;
        let mut found_setnum: Option<i32> = None;
        let mut star_mark = false;
        {
            let mut tableptr: Option<&NhashList> = Some(&s.table[hashval as usize]);
            while let Some(tp) = tableptr {
                if tp.size as i32 != setsize {
                    tableptr = tp.next.as_deref();
                    continue;
                }
                /* Compare the list at this bucket to the current set by
                looking at e_table entries */
                let mut found = 1;
                let currlist = tp.set_offset as usize;
                for j in 0..setsize as usize {
                    if s.e_table[s.set_table[currlist + j] as usize] != mainloop - 1 {
                        found = 0;
                        break;
                    }
                }
                if op == SUBSET_TEST_STAR_FREE && found == 1 {
                    for (j, &set_j) in set.iter().enumerate().take(setsize as usize) {
                        if set_j != s.set_table[currlist + j] {
                            /* Set mark (applied after the walk to keep the
                            table borrow immutable) */
                            star_mark = true;
                        }
                    }
                }
                if found == 1 {
                    found_setnum = Some(tp.setnum);
                    break;
                }
                tableptr = tp.next.as_deref();
            }
        }
        if star_mark {
            s.star_free_mark = 1;
        }
        if let Some(setnum) = found_setnum {
            return setnum;
        }

        /* Growth check only runs on this collision-miss path — inserting
        into an empty bucket never triggers a rebuild */
        if s.nhash_load / NHASH_LOAD_LIMIT > s.nhash_tablesize {
            nhash_rebuild_table(s);
            hashval = hashf(s, set, setsize);
        }
        nhash_insert(s, hashval, set, setsize)
    }
}

// [spec:foma:def:determinize.hashf-fn]
// [spec:foma:sem:determinize.hashf-fn]
/* C: INLINE static int hashf(int *set, int setsize) */
pub(crate) fn hashf(s: &Subset, set: &[i32], setsize: i32) -> i32 {
    let mut hashval: u32;
    let mut sum: u32 = 0;
    hashval = 6703271;
    for i in 0..setsize {
        /* C: hashval = (unsigned int) (*(set+i) + 1103 * setsize) * hashval;
        — the int addition wraps, then the unsigned multiply wraps */
        hashval = (set[i as usize].wrapping_add(1103_i32.wrapping_mul(setsize)) as u32)
            .wrapping_mul(hashval);
        /* C: sum += *(set+i) + i; — int add converted to unsigned */
        sum = sum.wrapping_add(set[i as usize].wrapping_add(i) as u32);
    }
    hashval = hashval.wrapping_add(sum.wrapping_mul(31));
    hashval %= s.nhash_tablesize as u32;
    hashval as i32
}

// [spec:foma:def:determinize.move-set-fn]
// [spec:foma:sem:determinize.move-set-fn]
pub(crate) fn move_set(s: &mut Subset, set: &[i32], setsize: i32) -> u32 {
    /* C compares set_table_offset + setsize >= set_table_size in unsigned
    arithmetic (note >=: growth also triggers on an exact fit) */
    if s.set_table_offset.wrapping_add(setsize as u32) >= s.set_table_size as u32 {
        while s.set_table_offset.wrapping_add(setsize as u32) >= s.set_table_size as u32 {
            s.set_table_size *= 2;
        }
        /* grow (zero-filled; only written offsets are ever read) */
        s.set_table.resize(s.set_table_size as usize, 0);
    }
    /* memcpy(set_table+set_table_offset, set, setsize * sizeof(int)); */
    let old_offset = s.set_table_offset;
    let off = old_offset as usize;
    s.set_table[off..off + setsize as usize].copy_from_slice(&set[..setsize as usize]);
    s.set_table_offset = old_offset + setsize as u32;
    old_offset
}

// [spec:foma:def:determinize.nhash-insert-fn]
// [spec:foma:sem:determinize.nhash-insert-fn]
pub(crate) fn nhash_insert(s: &mut Subset, hashval: i32, set: &[i32], setsize: i32) -> i32 {
    let mut fs = 0;

    s.current_setnum += 1;
    let current_setnum = s.current_setnum;

    s.nhash_load += 1;
    for i in 0..setsize {
        if s.finals[set[i as usize] as usize] {
            fs = 1;
        }
    }
    let head_empty = s.table[hashval as usize].size == 0;
    if head_empty {
        let set_offset = move_set(s, set, setsize);
        let tableptr = &mut s.table[hashval as usize];
        tableptr.set_offset = set_offset;
        tableptr.size = setsize as u32;
        tableptr.setnum = current_setnum;

        add_T_ptr(s, current_setnum, setsize, set_offset, fs);
        return current_setnum;
    }

    /* spliced in as the second chain element. (C assigns set_offset =
    move_set(...) after the splice; move_set only touches the set_table
    fields, so computing it first is unobservable) */
    let set_offset = move_set(s, set, setsize);
    let head = &mut s.table[hashval as usize];
    let tableptr = Box::new(NhashList {
        setnum: current_setnum,
        size: setsize as u32,
        set_offset,
        next: head.next.take(),
    });
    head.next = Some(tableptr);

    add_T_ptr(s, current_setnum, setsize, set_offset, fs);
    current_setnum
}

// [spec:foma:def:determinize.nhash-rebuild-table-fn]
// [spec:foma:sem:determinize.nhash-rebuild-table-fn]
pub(crate) fn nhash_rebuild_table(s: &mut Subset) {
    let oldtable = std::mem::take(&mut s.table);
    let oldsize = s.nhash_tablesize;

    s.nhash_load = 0;
    /* C: for (i=0; primes[i] < nhash_tablesize; i++) {} — lands exactly on
    the current prime, then takes the following entry. If already at the
    last prime, primes[i+1] reads past the array in C — panics here
    (practically unreachable). */
    let mut i = 0usize;
    while PRIMES[i] < s.nhash_tablesize as u32 {
        i += 1;
    }
    s.nhash_tablesize = PRIMES[i + 1] as i32;

    s.table = vec![NhashList::default(); s.nhash_tablesize as usize];
    for bucket in oldtable.iter().take(oldsize as usize) {
        if bucket.size == 0 {
            continue;
        }
        let mut tableptr: Option<&NhashList> = Some(bucket);
        while let Some(tp) = tableptr {
            /* rehash */
            let hashval = hashf(s, &s.set_table[tp.set_offset as usize..], tp.size as i32);
            let ntableptr = &mut s.table[hashval as usize];
            if ntableptr.size == 0 {
                /* quirk kept: nhash_load only counts occupied buckets here,
                understating the load factor for later checks */
                s.nhash_load += 1;
                ntableptr.size = tp.size;
                ntableptr.set_offset = tp.set_offset;
                ntableptr.setnum = tp.setnum;
                ntableptr.next = None;
            } else {
                let newptr = Box::new(NhashList {
                    setnum: tp.setnum,
                    size: tp.size,
                    set_offset: tp.set_offset,
                    next: ntableptr.next.take(),
                });
                ntableptr.next = Some(newptr);
            }
            tableptr = tp.next.as_deref();
        }
    }
    nhash_free(oldtable, oldsize);
}

// [spec:foma:def:determinize.nhash-init-fn]
// [spec:foma:sem:determinize.nhash-init-fn]
pub(crate) fn nhash_init(s: &mut Subset, initial_size: i32) {
    /* C: for (i=0; primes[i] < initial_size; i++) {} — unsigned comparison;
    minimum table size is primes[0] == 61 */
    let mut i = 0usize;
    while PRIMES[i] < initial_size as u32 {
        i += 1;
    }
    s.nhash_load = 0;
    s.nhash_tablesize = PRIMES[i] as i32;
    /* zeroed table so size == 0 marks an empty bucket */
    s.table = vec![NhashList::default(); s.nhash_tablesize as usize];
    s.current_setnum = -1;
}

// [spec:foma:def:determinize.e-closure-free-fn]
// [spec:foma:sem:determinize.e-closure-free-fn]
pub(crate) fn e_closure_free(s: &mut Subset) {
    /* the head array and its chain nodes share the E_CLOSURE_MEMO pool
    (index-linked, so clearing the Vec frees everything with no recursion) */
    s.marktable = Vec::new();
    s.e_closure_memo = Vec::new();
}

// [spec:foma:def:determinize.nhash-free-fn]
// [spec:foma:sem:determinize.nhash-free-fn]
pub(crate) fn nhash_free(mut nptr: Vec<NhashList>, size: i32) {
    /* for each bucket, free every chained node reachable from ->next (the
    heads are elements of the array itself); iterative take()s mirror the
    node-by-node frees and avoid recursive Box drops on long chains */
    for bucket in nptr.iter_mut().take(size as usize) {
        let mut nptr2 = bucket.next.take();
        while let Some(mut node) = nptr2 {
            let nnext = node.next.take();
            drop(node); /* free(nptr2) */
            nptr2 = nnext;
        }
    }
    /* free(nptr) — the bucket array is dropped on return */
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{apply_clear, apply_down, apply_init};
    use crate::dynarray::{
        fsm_construct_add_arc, fsm_construct_done, fsm_construct_init, fsm_construct_set_final,
        fsm_construct_set_initial,
    };
    use crate::types::{Fsm, NO, UNK};

    fn accepts(net: &Fsm, word: &str) -> Option<String> {
        let mut h = apply_init(net);
        let r = apply_down(&mut h, Some(word));
        apply_clear(h);
        r
    }

    fn lines(net: &Fsm) -> Vec<(i32, i16, i16, i32, i8, i8)> {
        net.states
            .iter()
            .map(|s| {
                (
                    s.state_no,
                    s.r#in,
                    s.out,
                    s.target,
                    s.final_state,
                    s.start_state,
                )
            })
            .collect()
    }

    /* NFA over {a}: 0 start, 0-a->0, 0-a->1, 1-a->2 (final). L = a^n, n >= 2. */
    fn build_a_ge2() -> Box<Fsm> {
        let mut hc = fsm_construct_init("d");
        fsm_construct_set_initial(&mut hc, 0);
        fsm_construct_add_arc(&mut hc, 0, 0, "a", "a");
        fsm_construct_add_arc(&mut hc, 0, 1, "a", "a");
        fsm_construct_add_arc(&mut hc, 1, 2, "a", "a");
        fsm_construct_set_final(&mut hc, 2);
        fsm_construct_done(hc)
    }

    // Full subset construction over the whole engine: fsm_determinize drives
    // fsm_subset, init, init_trans_array (+ trans_sort_cmp on the state with two
    // a-arcs), initial_e_closure, e_closure (no-epsilon branch), set_lookup and
    // the do-loop's next_unmarked agenda pops.
    // [spec:foma:sem:determinize.fsm-determinize-fn/test]
    // [spec:foma:sem:fomalib.fsm-determinize-fn/test]
    // [spec:foma:sem:determinize.fsm-subset-fn/test]
    // [spec:foma:sem:determinize.init-fn/test]
    // [spec:foma:sem:determinize.init-trans-array-fn/test]
    // [spec:foma:sem:determinize.initial-e-closure-fn/test]
    // [spec:foma:sem:determinize.e-closure-fn/test]
    // [spec:foma:sem:determinize.set-lookup-fn/test]
    #[test]
    fn determinize_subset_construction_shape() {
        let net = build_a_ge2();
        assert_ne!(net.is_deterministic, YES, "input NFA is nondeterministic");
        let d = fsm_determinize(net);
        /* subsets {0}, {0,1}, {0,1,2 final}: 3 states, 3 arcs, 1 final */
        assert_eq!(d.statecount, 3);
        assert_eq!(d.arccount, 3);
        assert_eq!(d.finalcount, 1);
        assert_eq!(d.is_deterministic, YES);
        assert_eq!(d.is_epsilon_free, YES);
        /* start state renumbered to 0, densely numbered result */
        assert_eq!(
            d.states
                .iter()
                .filter(|s| s.state_no != -1 && s.start_state != 0)
                .count(),
            1
        );
        assert_eq!(accepts(&d, ""), None);
        assert_eq!(accepts(&d, "a"), None);
        assert_eq!(accepts(&d, "aa"), Some("aa".to_string()));
        assert_eq!(accepts(&d, "aaaa"), Some("aaaa".to_string()));
    }

    // fsm_epsilon_remove drives fsm_subset with epsilon memoization: memoize is
    // exercised on the input's eps arc and the closure DFS runs the epsilon
    // branch of e_closure; e_closure_free tears the memo down in wrapup.
    // [spec:foma:sem:determinize.fsm-epsilon-remove-fn/test]
    // [spec:foma:sem:fomalib.fsm-epsilon-remove-fn/test]
    // [spec:foma:sem:determinize.e-closure-fn/test]
    #[test]
    fn epsilon_remove_eliminates_epsilon_arcs() {
        /* eps-NFA: 0 start -eps-> 1, 1 -a-> 1 (loop), 1 final. L = a*. */
        let mut hc = fsm_construct_init("e");
        fsm_construct_set_initial(&mut hc, 0);
        fsm_construct_add_arc(&mut hc, 0, 1, "@_EPSILON_SYMBOL_@", "@_EPSILON_SYMBOL_@");
        fsm_construct_add_arc(&mut hc, 1, 1, "a", "a");
        fsm_construct_set_final(&mut hc, 1);
        let net = fsm_construct_done(hc);
        assert_eq!(net.is_epsilon_free, NO);
        let er = fsm_epsilon_remove(net);
        assert_eq!(er.is_epsilon_free, YES);
        /* no (EPSILON:EPSILON) arc survives */
        assert!(
            !er.states
                .iter()
                .any(|s| s.r#in == 0 && s.out == 0 && s.target != -1)
        );
        /* state 0's epsilon closure reaches final state 1 -> language a* kept */
        assert_eq!(accepts(&er, ""), Some("".to_string()));
        assert_eq!(accepts(&er, "a"), Some("a".to_string()));
        assert_eq!(accepts(&er, "aaa"), Some("aaa".to_string()));
    }

    // fsm_subset's second fast path: EPSILON_REMOVE on a net with no epsilon arc
    // sets is_epsilon_free and returns the net unmodified (NOT determinized).
    // [spec:foma:sem:determinize.fsm-subset-fn/test]
    #[test]
    fn epsilon_remove_no_epsilon_fast_path() {
        /* nondeterministic (skips the top-level det early return) but eps-free */
        let mut hc = fsm_construct_init("f");
        fsm_construct_set_initial(&mut hc, 0);
        fsm_construct_add_arc(&mut hc, 0, 1, "a", "a");
        fsm_construct_add_arc(&mut hc, 0, 2, "a", "a"); /* two a-arcs -> nondet */
        fsm_construct_set_final(&mut hc, 1);
        fsm_construct_set_final(&mut hc, 2);
        let net = fsm_construct_done(hc);
        assert_ne!(net.is_deterministic, YES);
        let sc = net.statecount;
        let before = lines(&net);
        let er = fsm_epsilon_remove(net);
        assert_eq!(er.is_epsilon_free, YES);
        assert_eq!(er.statecount, sc);
        assert_ne!(
            er.is_deterministic, YES,
            "not determinized on the eps-free path"
        );
        assert_eq!(lines(&er), before, "line table returned untouched");
    }

    // Internal already-deterministic shortcut: forced past the top-level
    // is_deterministic==YES early return, a structurally-deterministic single-
    // start-at-0 net takes the shortcut, which sets det/eps flags but does NOT
    // rebuild (is_pruned/is_minimized preserved, line table intact).
    // [spec:foma:sem:determinize.fsm-subset-fn/test]
    #[test]
    fn already_deterministic_shortcut_preserves_flags() {
        let mut hc = fsm_construct_init("A");
        fsm_construct_set_initial(&mut hc, 0);
        fsm_construct_set_final(&mut hc, 0);
        fsm_construct_add_arc(&mut hc, 0, 1, "a", "a");
        fsm_construct_set_final(&mut hc, 1);
        fsm_construct_add_arc(&mut hc, 1, 1, "a", "a");
        let mut net = fsm_construct_done(hc);
        net.is_deterministic = UNK; /* skip the top-level early return */
        net.is_pruned = YES;
        net.is_minimized = YES;
        let before = lines(&net);
        let d = fsm_determinize(net);
        assert_eq!(d.is_deterministic, YES);
        assert_eq!(d.is_epsilon_free, YES);
        assert_eq!(d.is_pruned, YES, "shortcut does not touch is_pruned");
        assert_eq!(d.is_minimized, YES, "shortcut does not touch is_minimized");
        assert_eq!(lines(&d), before, "line table not rebuilt");
    }

    // numss _Bool truncation quirk: a deterministic net whose single start state
    // is NOT state 0 has numss = (state_no != 0) = true, so the shortcut is
    // skipped and the full subset construction runs (rebuild via fsm_state_close
    // clears is_pruned/is_minimized and renumbers the start state to 0).
    // [spec:foma:sem:determinize.fsm-subset-fn/test]
    // [spec:foma:sem:determinize.initial-e-closure-fn/test]
    #[test]
    fn numss_bool_truncation_forces_full_path() {
        let mut hc = fsm_construct_init("B");
        fsm_construct_set_initial(&mut hc, 1);
        fsm_construct_add_arc(&mut hc, 1, 0, "a", "a");
        fsm_construct_set_final(&mut hc, 0);
        let mut net = fsm_construct_done(hc);
        net.is_deterministic = UNK;
        net.is_pruned = YES;
        net.is_minimized = YES;
        let d = fsm_determinize(net);
        /* full path taken -> the builder close resets these to UNK */
        assert_eq!(d.is_pruned, UNK);
        assert_eq!(d.is_minimized, UNK);
        let start_states: Vec<i32> = d
            .states
            .iter()
            .filter(|s| s.state_no != -1 && s.start_state != 0)
            .map(|s| s.state_no)
            .collect();
        assert_eq!(start_states, vec![0], "start state renumbered to 0");
        assert_eq!(accepts(&d, "a"), Some("a".to_string()));
        assert_eq!(accepts(&d, ""), None);
    }

    // hashf: the fixed 6703271 seed (observable on the empty set) and the
    // permutation invariance the subset hashing relies on.
    // [spec:foma:sem:determinize.hashf-fn/test]
    #[test]
    fn hashf_seed_and_permutation() {
        let mut s = Subset {
            nhash_tablesize: 61,
            ..Default::default()
        };
        assert_eq!(hashf(&s, &[], 0), (6703271u32 % 61) as i32);
        /* large prime table: modulo does not mask the permutation equality */
        s.nhash_tablesize = 2147483647;
        let base = hashf(&s, &[7, 3, 19, 2], 4);
        assert_eq!(base, hashf(&s, &[2, 19, 3, 7], 4));
        assert_eq!(base, hashf(&s, &[19, 2, 7, 3], 4));
    }

    // nhash_init picks the smallest prime >= initial_size off the ladder
    // (minimum 61) and resets load / current_setnum.
    // [spec:foma:sem:determinize.nhash-init-fn/test]
    #[test]
    fn nhash_init_prime_ladder() {
        let mut s = Subset::default();
        nhash_init(&mut s, 6);
        assert_eq!(s.nhash_tablesize, 61);
        assert_eq!(s.current_setnum, -1);
        assert_eq!(s.nhash_load, 0);
        nhash_init(&mut s, 61);
        assert_eq!(s.nhash_tablesize, 61);
        nhash_init(&mut s, 62);
        assert_eq!(s.nhash_tablesize, 127);
        nhash_init(&mut s, 0);
        assert_eq!(s.nhash_tablesize, 61);
        nhash_init(&mut s, 2000);
        assert_eq!(s.nhash_tablesize, 2039);
    }

    // nhash_rebuild_table advances to the next prime and rehashes (empty here).
    // [spec:foma:sem:determinize.nhash-rebuild-table-fn/test]
    #[test]
    fn nhash_rebuild_advances_prime() {
        let mut s = Subset::default();
        nhash_init(&mut s, 6); /* 61, empty */
        nhash_rebuild_table(&mut s);
        assert_eq!(s.nhash_tablesize, 127);
        assert_eq!(s.nhash_load, 0);
    }

    // Round-trip through the subset canonicaliser: set_lookup -> nhash_find_insert
    // -> nhash_insert -> move_set + add_T_ptr. A first set is numbered 0 and its
    // members copied into set_table; a permutation of it canonicalises back to 0
    // (order-insensitive membership test via e_table); a distinct set gets 1.
    // add_T_ptr pushed both onto the agenda (next_unmarked pops LIFO).
    // [spec:foma:sem:determinize.set-lookup-fn/test]
    // [spec:foma:sem:determinize.nhash-find-insert-fn/test]
    // [spec:foma:sem:determinize.nhash-insert-fn/test]
    // [spec:foma:sem:determinize.move-set-fn/test]
    // [spec:foma:sem:determinize.add-t-ptr-fn/test]
    #[test]
    fn nhash_insert_find_roundtrip() {
        let n = 5usize;
        let mut s = Subset {
            finals: vec![false; n],
            e_table: vec![0; n],
            mainloop: 1,
            set_table_size: 64,
            set_table: vec![0; 64],
            set_table_offset: 0,
            t_limit: 8,
            t_ptr: vec![TMemo::default(); 8],
            op: SUBSET_DETERMINIZE,
            ..Default::default()
        };
        nhash_init(&mut s, 6);

        /* first insert of {2,0,1} -> subset 0, members copied to set_table */
        assert_eq!(set_lookup(&mut s, &[2, 0, 1], 3), 0);
        assert_eq!(s.set_table_offset, 3);
        assert_eq!(&s.set_table[0..3], &[2, 0, 1]);
        assert_eq!((s.t_ptr[0].size, s.t_ptr[0].set_offset), (3, 0));

        /* find a permutation: mark members e_table == mainloop-1, bump mainloop */
        s.e_table[0] = 1;
        s.e_table[1] = 1;
        s.e_table[2] = 1;
        s.mainloop = 2;
        assert_eq!(
            set_lookup(&mut s, &[0, 1, 2], 3),
            0,
            "permutation canonicalises to 0"
        );

        /* a distinct set gets the next number */
        assert_eq!(set_lookup(&mut s, &[3, 4], 2), 1);
        assert_eq!(s.set_table_offset, 5);

        /* both subsets were pushed on the agenda by add_T_ptr (LIFO) */
        assert_eq!(next_unmarked(&mut s), 1);
        assert_eq!(next_unmarked(&mut s), 0);
        assert_eq!(next_unmarked(&mut s), -1);
    }

    // next_unmarked pops the agenda LIFO, -1 when empty.
    // [spec:foma:sem:determinize.next-unmarked-fn/test]
    #[test]
    fn next_unmarked_pops_lifo() {
        let mut s = Subset::default();
        s.int_stack.push(3);
        s.int_stack.push(7);
        assert_eq!(next_unmarked(&mut s), 7);
        assert_eq!(next_unmarked(&mut s), 3);
        assert_eq!(next_unmarked(&mut s), -1);
    }

    // sigma_to_pairs builds the (in,out)<->composite bijection, flags a
    // transducer (arity 2), records epsilon_symbol for the (0,0) pair, and the
    // two mapping functions invert each other for every registered pair.
    // [spec:foma:sem:determinize.sigma-to-pairs-fn/test]
    // [spec:foma:sem:determinize.symbol-pair-to-single-symbol-fn/test]
    // [spec:foma:sem:determinize.single-symbol-to-symbol-pair-fn/test]
    #[test]
    fn sigma_to_pairs_and_symbol_mappings() {
        let mut hc = fsm_construct_init("s");
        fsm_construct_set_initial(&mut hc, 0);
        fsm_construct_add_arc(&mut hc, 0, 1, "a", "b"); /* a:b -> arity 2 */
        fsm_construct_add_arc(&mut hc, 1, 2, "a", "a"); /* a:a */
        fsm_construct_add_arc(&mut hc, 0, 2, "@_EPSILON_SYMBOL_@", "@_EPSILON_SYMBOL_@");
        fsm_construct_set_final(&mut hc, 2);
        let mut net = fsm_construct_done(hc);
        let mut s = Subset::default();
        sigma_to_pairs(&mut s, &mut net);
        assert_eq!(net.arity, 2);
        assert_ne!(s.epsilon_symbol, -1);
        assert_eq!(
            s.epsilon_symbol,
            symbol_pair_to_single_symbol(&s, EPSILON, EPSILON)
        );
        for st in net.states.iter() {
            let (i, o) = (st.r#in as i32, st.out as i32);
            if i < 0 || o < 0 {
                continue;
            }
            let c = symbol_pair_to_single_symbol(&s, i, o);
            assert!(c >= 0 && c < s.num_symbols);
            let (mut si, mut so) = (0, 0);
            single_symbol_to_symbol_pair(&s, c, &mut si, &mut so);
            assert_eq!((si, so), (i, o), "back-map inverts forward-map");
        }
    }

    // memoize_e_closure builds the per-state epsilon adjacency graph, skipping
    // self-loops and duplicate (source,target) pairs; fanout is a head ->target
    // plus a ->next chain (LIFO of int_stack pops).
    // [spec:foma:sem:determinize.memoize-e-closure-fn/test]
    #[test]
    fn memoize_e_closure_builds_epsilon_graph() {
        let mut s = Subset {
            num_states: 3,
            ..Default::default()
        };
        let e = EPSILON as i16;
        let fsm = vec![
            FsmState {
                state_no: 0,
                r#in: e,
                out: e,
                target: 1,
                final_state: 0,
                start_state: 1,
            },
            FsmState {
                state_no: 0,
                r#in: e,
                out: e,
                target: 2,
                final_state: 0,
                start_state: 1,
            },
            FsmState {
                state_no: 0,
                r#in: e,
                out: e,
                target: 1,
                final_state: 0,
                start_state: 1,
            }, /* dup */
            FsmState {
                state_no: 0,
                r#in: e,
                out: e,
                target: 0,
                final_state: 0,
                start_state: 1,
            }, /* self */
            FsmState {
                state_no: 1,
                r#in: -1,
                out: -1,
                target: -1,
                final_state: 0,
                start_state: 0,
            },
            FsmState {
                state_no: 2,
                r#in: -1,
                out: -1,
                target: -1,
                final_state: 1,
                start_state: 0,
            },
            FsmState {
                state_no: -1,
                r#in: -1,
                out: -1,
                target: -1,
                final_state: -1,
                start_state: -1,
            },
        ];
        memoize_e_closure(&mut s, &fsm);
        let em = &s.e_closure_memo;
        /* head 0 -> successors {2,1} (LIFO), heads 1,2 have none */
        assert_eq!(em[0].state, 0);
        assert_eq!(em[0].target, Some(2));
        let chain = em[0].next.expect("fanout chain node");
        assert_eq!(em[chain].target, Some(1));
        assert_eq!(em[chain].next, None);
        assert_eq!(em[1].target, None);
        assert_eq!(em[2].target, None);
    }

    // e_closure_free drops marktable and the memo pool.
    // [spec:foma:sem:determinize.e-closure-free-fn/test]
    #[test]
    fn e_closure_free_clears_memo() {
        let mut s = Subset {
            marktable: vec![1, 2, 3],
            e_closure_memo: vec![EClosureMemo::default(); 4],
            ..Default::default()
        };
        e_closure_free(&mut s);
        assert!(s.marktable.is_empty());
        assert!(s.e_closure_memo.is_empty());
    }

    // nhash_free walks each bucket's ->next chain without panicking.
    // [spec:foma:sem:determinize.nhash-free-fn/test]
    #[test]
    fn nhash_free_walks_chains() {
        let mut table = vec![NhashList::default(); 2];
        table[0].size = 1;
        table[0].next = Some(Box::new(NhashList {
            setnum: 1,
            size: 1,
            set_offset: 0,
            next: Some(Box::new(NhashList::default())),
        }));
        nhash_free(table, 2);
    }

    // trans_sort_cmp: ascending by composite symbol (a->inout vs b->inout).
    // [spec:foma:sem:determinize.trans-sort-cmp-fn+1/test]
    #[test]
    fn trans_sort_cmp_orders_by_inout() {
        use core::cmp::Ordering;
        let a = TransList {
            inout: 5,
            target: 0,
        };
        let b = TransList {
            inout: 2,
            target: 9,
        };
        assert_eq!(trans_sort_cmp(&a, &b), Ordering::Greater);
        assert_eq!(trans_sort_cmp(&b, &a), Ordering::Less);
        assert_eq!(trans_sort_cmp(&a, &a), Ordering::Equal);
    }

    // The extern add_fsm_arc re-export writes one flat line and returns offset+1.
    // [spec:foma:sem:determinize.add-fsm-arc-fn/test]
    #[test]
    fn add_fsm_arc_reexport_writes_line() {
        let mut fsm = vec![
            FsmState {
                state_no: 0,
                r#in: 0,
                out: 0,
                target: 0,
                final_state: 0,
                start_state: 0
            };
            2
        ];
        let r = add_fsm_arc(&mut fsm, 0, 5, 1, 2, 3, 1, 1);
        assert_eq!(r, 1);
        assert_eq!(
            (
                fsm[0].state_no,
                fsm[0].r#in,
                fsm[0].out,
                fsm[0].target,
                fsm[0].final_state,
                fsm[0].start_state
            ),
            (5, 1, 2, 3, 1, 1)
        );
    }
}
