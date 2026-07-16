//! foma/spelling.c — literal Wave-2 (bug-for-bug) port per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/spelling.md
//! (per-file ids) plus the fomalib.h prototype ids for the exported functions
//! (apply_med*, cmatrix_*, fsm_create_letter_lookup).
//!
//! Interior `struct fsm_state *` pointers (curr_ptr, ptr_stack tokens,
//! state_array[..].transitions) are represented as indices into the net's
//! sentinel-terminated line table, exactly as the conventions prescribe. The
//! agenda is append-only and addressed by index; astarnode "pointers" become
//! agenda indices so the parent-chain walk in print_match survives reallocs.

use crate::int_stack::{IntStack, PtrStack};
#[cfg(test)]
use crate::options::FomaOptions;
use crate::sigma::{sigma_find, sigma_max, sigma_string};
use crate::stringhash::{sh_add_string, sh_find_string, sh_get_value, sh_init};
use crate::structures::map_firstlines;
use crate::types::{
    ApplyMedHandle, Astarnode, Fsm, IDENTITY, MED_DEFAULT_CUTOFF, MED_DEFAULT_LIMIT,
    MED_DEFAULT_MAX_HEAP_SIZE, Medlookup, Sigma,
};

/* C #defines local to spelling.c */
const INITIAL_AGENDA_SIZE: i32 = 256;
const INITIAL_HEAP_SIZE: i32 = 256;

/* C: #define CHAR_BIT 8 (<limits.h>); used by BITNSLOTS */
const CHAR_BIT: i32 = 8;

/* For keeping track of the strongly connected components when doing the DFS */
// [spec:foma:def:spelling.sccinfo]
#[derive(Debug, Clone)]
struct Sccinfo {
    index: i32,
    lowlink: i32,
    on_t_stack: i32,
}

/* (net + off) line-table field accessors. `off` is an index into the net's
sentinel-terminated line table (a `struct fsm_state *` in C). */
fn ls_state_no(net: &Fsm, off: usize) -> i32 {
    net.states[off].state_no
}
fn ls_in(net: &Fsm, off: usize) -> i16 {
    net.states[off].r#in
}
fn ls_target(net: &Fsm, off: usize) -> i32 {
    net.states[off].target
}

/* medh->curr_ptr-> field accessors (curr_ptr is the persisted resume cursor). */
fn med_net(medh: &ApplyMedHandle) -> &Fsm {
    medh.net
        .as_ref()
        .expect("med net present during an active walk")
}
fn med_curr_ptr(medh: &ApplyMedHandle) -> usize {
    medh.curr_ptr
        .expect("curr_ptr positioned during an active walk")
}
fn cur_state_no(medh: &ApplyMedHandle) -> i32 {
    med_net(medh).states[med_curr_ptr(medh)].state_no
}
fn cur_in(medh: &ApplyMedHandle) -> i16 {
    med_net(medh).states[med_curr_ptr(medh)].r#in
}
fn cur_target(medh: &ApplyMedHandle) -> i32 {
    med_net(medh).states[med_curr_ptr(medh)].target
}
fn cur_final(medh: &ApplyMedHandle) -> i8 {
    med_net(medh).states[med_curr_ptr(medh)].final_state
}
fn next_state_no(medh: &ApplyMedHandle) -> i32 {
    net_line_state_no(medh, med_curr_ptr(medh) + 1)
}
fn net_line_state_no(medh: &ApplyMedHandle, off: usize) -> i32 {
    med_net(medh).states[off].state_no
}

// [spec:foma:def:spelling.print-sym-fn]
// [spec:foma:sem:spelling.print-sym-fn]
fn print_sym(sym: i32, sigma: &[Sigma]) -> Option<&str> {
    sigma
        .iter()
        .find(|s| s.number == sym)
        .map(|s| s.symbol.as_str())
}

// [spec:foma:def:spelling.apply-med-set-heap-max-fn]
// [spec:foma:sem:spelling.apply-med-set-heap-max-fn]
// [spec:foma:def:fomalib.apply-med-set-heap-max-fn]
// [spec:foma:sem:fomalib.apply-med-set-heap-max-fn]
pub fn apply_med_set_heap_max(medh: &mut ApplyMedHandle, max: i32) {
    /* C guards on medh != NULL; a &mut can never be null here. */
    medh.med_max_heap_size = max;
}

// [spec:foma:def:spelling.apply-med-set-align-symbol-fn]
// [spec:foma:sem:spelling.apply-med-set-align-symbol-fn]
// [spec:foma:def:fomalib.apply-med-set-align-symbol-fn]
// [spec:foma:sem:fomalib.apply-med-set-align-symbol-fn]
pub fn apply_med_set_align_symbol(medh: &mut ApplyMedHandle, align: &str) {
    medh.align_symbol = Some(align.into()); /* C: strdup(align) */
}

// [spec:foma:def:spelling.apply-med-set-med-limit-fn]
// [spec:foma:sem:spelling.apply-med-set-med-limit-fn]
// [spec:foma:def:fomalib.apply-med-set-med-limit-fn]
// [spec:foma:sem:fomalib.apply-med-set-med-limit-fn]
pub fn apply_med_set_med_limit(medh: &mut ApplyMedHandle, max: i32) {
    medh.med_limit = max;
}

// [spec:foma:def:spelling.apply-med-set-med-cutoff-fn]
// [spec:foma:sem:spelling.apply-med-set-med-cutoff-fn]
// [spec:foma:def:fomalib.apply-med-set-med-cutoff-fn]
// [spec:foma:sem:fomalib.apply-med-set-med-cutoff-fn]
pub fn apply_med_set_med_cutoff(medh: &mut ApplyMedHandle, max: i32) {
    medh.med_cutoff = max;
}

// [spec:foma:def:spelling.apply-med-get-cost-fn]
// [spec:foma:sem:spelling.apply-med-get-cost-fn]
// [spec:foma:def:fomalib.apply-med-get-cost-fn]
// [spec:foma:sem:fomalib.apply-med-get-cost-fn]
pub fn apply_med_get_cost(medh: &ApplyMedHandle) -> i32 {
    medh.cost
}

// [spec:foma:def:spelling.apply-med-get-instring-fn]
// [spec:foma:sem:spelling.apply-med-get-instring-fn]
// [spec:foma:def:fomalib.apply-med-get-instring-fn]
// [spec:foma:sem:fomalib.apply-med-get-instring-fn]
pub fn apply_med_get_instring(medh: &ApplyMedHandle) -> Option<String> {
    Some(medh.instring.clone())
}

// [spec:foma:def:spelling.apply-med-get-outstring-fn]
// [spec:foma:sem:spelling.apply-med-get-outstring-fn]
// [spec:foma:def:fomalib.apply-med-get-outstring-fn]
// [spec:foma:sem:fomalib.apply-med-get-outstring-fn]
pub fn apply_med_get_outstring(medh: &ApplyMedHandle) -> Option<String> {
    Some(medh.outstring.clone())
}

// [spec:foma:def:spelling.apply-med-clear-fn]
// [spec:foma:sem:spelling.apply-med-clear-fn]
// [spec:foma:def:fomalib.apply-med-clear-fn]
// [spec:foma:sem:fomalib.apply-med-clear-fn]
// Consumes the handle (C frees agenda/instring/outstring/heap/state_array/
// align_symbol/letterbits/nletterbits/intword, sh_done's the sigmahash, then
// frees the handle — all handled by drop here). net/cm are borrowed, not owned.
pub fn apply_med_clear(medh: Option<Box<ApplyMedHandle>>) {
    if medh.is_none() {
        return;
    }
    drop(medh);
}

// [spec:foma:def:spelling.print-match-fn]
// [spec:foma:sem:spelling.print-match-fn+1]
fn print_match(medh: &mut ApplyMedHandle, node: usize, sigma: &[Sigma], word: &str) {
    let mut int_stack = IntStack::new();
    let wordlen = medh.wordlen;
    /* Pass 1: walk the parent chain pushing each n->in, terminating at the root.
    [spec:foma:sem:spelling.print-match-fn+1] the root is the sole node with
    parent == -1, so that alone marks it; C additionally broke on in==0 && out==0,
    which also truncated the walk at any interior epsilon:epsilon step. */
    let mut n = node;
    loop {
        if medh.agenda[n].parent == -1 {
            break;
        }
        int_stack.push(medh.agenda[n].r#in);
        n = medh.agenda[n].parent as usize;
    }
    /* Each pass rebuilds the buffer from scratch (C reset printptr to 0). */
    medh.outstring.clear();
    while !int_stack.is_empty() {
        let s = int_stack.pop();
        if s > 2 {
            medh.outstring
                .push_str(print_sym(s, sigma).expect("symbol > 2 resolves in the med sigma"));
        }
        if s == 0 {
            if let Some(al) = &medh.align_symbol {
                medh.outstring.push_str(al);
            }
        }
        if s == 2 {
            medh.outstring.push('@');
        }
    }
    /* Pass 2: same walk, pushing each n->out. [spec:foma:sem:spelling.print-match-fn+1] */
    let mut n = node;
    loop {
        if medh.agenda[n].parent == -1 {
            break;
        }
        int_stack.push(medh.agenda[n].out);
        n = medh.agenda[n].parent as usize;
    }
    medh.instring.clear();
    let mut i: usize = 0; // byte offset into `word`
    while !int_stack.is_empty() {
        let sym = int_stack.pop();
        if sym > 2 {
            medh.instring
                .push_str(print_sym(sym, sigma).expect("symbol > 2 resolves in the med sigma"));
            i += word[i..].chars().next().map_or(0, |c| c.len_utf8());
        }
        if sym == 0 {
            if let Some(al) = &medh.align_symbol {
                medh.instring.push_str(al);
            }
        }
        if sym == 2 {
            if i as i32 > wordlen {
                medh.instring.push('*');
            } else {
                let w = word[i..].chars().next().map_or(0, |c| c.len_utf8());
                let end = (i + w).min(word.len());
                let chunk = word[i..end].to_string();
                medh.instring.push_str(&chunk);
                i += w;
            }
        }
    }
    medh.cost = medh.agenda[node].g as i32;
}

/* f/wordpos of agenda[agidx] (short ints promoted to int, as C does). */
fn ag_f(medh: &ApplyMedHandle, agidx: i32) -> i32 {
    medh.agenda[agidx as usize].f as i32
}
fn ag_wordpos(medh: &ApplyMedHandle, agidx: i32) -> i32 {
    medh.agenda[agidx as usize].wordpos as i32
}

// [spec:foma:def:spelling.calculate-h-fn]
// [spec:foma:sem:spelling.calculate-h-fn]
fn calculate_h(medh: &ApplyMedHandle, intword: &[i32], currpos: i32, state: i32) -> i32 {
    let mut i: i32;
    let mut j: i32;
    let mut hinf: i32;
    let mut hn: i32;
    let mut curr_sym: i32;
    hinf = 0;
    hn = 0;

    let bpla = medh.bytes_per_letter_array;
    /* bitptr = state*bpla into letterbits; nbitptr = state*bpla into nletterbits */

    /* For n = inf */
    if intword[currpos as usize] == -1 {
        return 0;
    }
    i = currpos;
    while intword[i as usize] != -1 {
        curr_sym = intword[i as usize];
        /* !BITTEST(bitptr, curr_sym) */
        if (medh.letterbits[(state * bpla + (curr_sym >> 3)) as usize] & (1u8 << (curr_sym & 7)))
            == 0
        {
            hinf += 1;
        }
        i += 1;
    }
    /* For n = maxdepth */
    if intword[currpos as usize] == -1 {
        return 0;
    }
    i = currpos;
    j = 0;
    while j < medh.maxdepth && intword[i as usize] != -1 {
        curr_sym = intword[i as usize];
        if (medh.nletterbits[(state * bpla + (curr_sym >> 3)) as usize] & (1u8 << (curr_sym & 7)))
            == 0
        {
            hn += 1;
        }
        i += 1;
        j += 1;
    }
    if hinf > hn { hinf } else { hn }
}

// [spec:foma:def:spelling.node-delete-min-fn]
// [spec:foma:sem:spelling.node-delete-min-fn]
// Returns the agenda index of the popped min node (= curr_node - agenda), or
// None when the heap is empty.
fn node_delete_min(medh: &mut ApplyMedHandle) -> Option<i32> {
    let mut i: i32;
    let mut child: i32;
    if medh.heapcount == 0 {
        return None;
    }

    /* We find the min from the heap */
    let firstptr = medh.heap[1];
    let lastptr = medh.heap[medh.heapcount as usize];
    medh.heapcount -= 1;

    /* Adjust heap */
    i = 1;
    while (i << 1) <= medh.heapcount {
        child = i << 1;

        /* If right child is smaller (higher priority) than left child */
        if child != medh.heapcount
            && (ag_f(medh, medh.heap[(child + 1) as usize]) < ag_f(medh, medh.heap[child as usize])
                || (ag_f(medh, medh.heap[(child + 1) as usize])
                    <= ag_f(medh, medh.heap[child as usize])
                    && ag_wordpos(medh, medh.heap[(child + 1) as usize])
                        > ag_wordpos(medh, medh.heap[child as usize])))
        {
            child += 1;
        }

        /* If child has lower priority than last element */
        if ag_f(medh, medh.heap[child as usize]) < ag_f(medh, lastptr)
            || (ag_f(medh, medh.heap[child as usize]) <= ag_f(medh, lastptr)
                && ag_wordpos(medh, medh.heap[child as usize]) > ag_wordpos(medh, lastptr))
        {
            medh.heap[i as usize] = medh.heap[child as usize];
        } else {
            break;
        }
        i = child;
    }
    medh.heap[i as usize] = lastptr;
    Some(firstptr)
}

// [spec:foma:def:spelling.node-insert-fn]
// [spec:foma:sem:spelling.node-insert-fn]
#[allow(clippy::too_many_arguments)]
fn node_insert(
    medh: &mut ApplyMedHandle,
    wordpos: i32,
    fsmstate: i32,
    g: i32,
    h: i32,
    r#in: i32,
    out: i32,
    parent: i32,
) -> bool {
    let mut j: i32;

    /* We add the node in the array */
    let i = medh.astarcount;
    if i >= medh.agenda_size - 1 {
        if medh.agenda_size * 2 >= medh.med_max_heap_size {
            return false;
        }
        medh.agenda_size *= 2;
        /* Grow the agenda pool (every new slot is written before it is read). */
        medh.agenda.resize(
            medh.agenda_size as usize,
            Astarnode {
                wordpos: 0,
                fsmstate: 0,
                f: 0,
                g: 0,
                h: 0,
                r#in: 0,
                out: 0,
                parent: 0,
            },
        );
    }
    let f: i32 = g + h;
    medh.agenda[i as usize].wordpos = wordpos as i16;
    medh.agenda[i as usize].fsmstate = fsmstate;
    medh.agenda[i as usize].f = f as i16;
    medh.agenda[i as usize].g = g as i16;
    medh.agenda[i as usize].h = h as i16;
    medh.agenda[i as usize].r#in = r#in;
    medh.agenda[i as usize].out = out;
    medh.agenda[i as usize].parent = parent;
    medh.astarcount += 1;

    /* We also put the ptr on the heap */
    medh.heapcount += 1;

    if medh.heapcount == medh.heap_size - 1 {
        medh.heap.resize((medh.heap_size * 2) as usize, 0);
        medh.heap_size *= 2;
    }
    /*                                     >= makes fifo */
    j = medh.heapcount;
    while (ag_f(medh, medh.heap[(j >> 1) as usize]) > f)
        || (ag_f(medh, medh.heap[(j >> 1) as usize]) >= f
            && ag_wordpos(medh, medh.heap[(j >> 1) as usize]) <= wordpos)
    {
        medh.heap[j as usize] = medh.heap[(j >> 1) as usize];
        j >>= 1;
    }
    medh.heap[j as usize] = i;
    true
}

// [spec:foma:def:spelling.letterbits-union-fn]
// [spec:foma:sem:spelling.letterbits-union-fn]
fn letterbits_union(v: i32, vp: i32, ptr: &mut [u8], bytes_per_letter_array: i32) {
    let vbase = (v * bytes_per_letter_array) as usize;
    let vpbase = (vp * bytes_per_letter_array) as usize;
    for i in 0..bytes_per_letter_array as usize {
        ptr[vbase + i] |= ptr[vpbase + i];
    }
}

// [spec:foma:def:spelling.letterbits-copy-fn]
// [spec:foma:sem:spelling.letterbits-copy-fn]
fn letterbits_copy(source: i32, target: i32, ptr: &mut [u8], bytes_per_letter_array: i32) {
    let sourcebase = (source * bytes_per_letter_array) as usize;
    let targetbase = (target * bytes_per_letter_array) as usize;
    let bpla = bytes_per_letter_array as usize;
    ptr.copy_within(sourcebase..sourcebase + bpla, targetbase);
}

// [spec:foma:def:spelling.letterbits-add-fn]
// [spec:foma:sem:spelling.letterbits-add-fn]
fn letterbits_add(v: i32, symbol: i32, ptr: &mut [u8], bytes_per_letter_array: i32) {
    let vbase = (v * bytes_per_letter_array) as usize;
    /* BITSET(vptr, symbol) */
    ptr[vbase + (symbol >> 3) as usize] |= 1u8 << (symbol & 7);
}

/* Program-counter labels for the goto-based iterative Tarjan DFS below. */
#[derive(Clone, Copy)]
enum Pc {
    Loop,
    L1,
    L2,
    L3,
    L4,
}

// [spec:foma:def:spelling.fsm-create-letter-lookup-fn]
// [spec:foma:sem:spelling.fsm-create-letter-lookup-fn]
// [spec:foma:def:fomalib.fsm-create-letter-lookup-fn]
// [spec:foma:sem:fomalib.fsm-create-letter-lookup-fn]
pub fn fsm_create_letter_lookup(medh: &mut ApplyMedHandle, net: &Fsm) {
    let mut int_stack = IntStack::new();
    let mut ptr_stack = PtrStack::new();

    let mut index: i32;
    let mut v: i32;
    let mut vp: i32;
    let mut copystate: i32;
    /* curr_ptr is an index into net->states (a struct fsm_state * in C). */
    let mut curr_ptr: usize;
    let mut depth: i32;
    medh.maxdepth = 2;

    let num_states: i32 = net.statecount;
    let num_symbols: i32 = sigma_max(&net.sigma);

    /* BITNSLOTS(num_symbols+1) */
    medh.bytes_per_letter_array = ((num_symbols + 1) + CHAR_BIT - 1) / CHAR_BIT;
    let bpla = medh.bytes_per_letter_array;
    medh.letterbits = vec![0u8; (bpla * num_states) as usize];
    medh.nletterbits = vec![0u8; (bpla * num_states) as usize];

    let mut sccinfo: Vec<Sccinfo> = vec![
        Sccinfo {
            index: 0,
            lowlink: 0,
            on_t_stack: 0
        };
        num_states as usize
    ];

    index = 1;
    curr_ptr = 0; /* net->states */
    v = 0;
    vp = 0;

    /* Iterative Tarjan-SCC DFS (C: gotos l1..l4 inside a while loop). goto l1. */
    let mut pc = Pc::L1;
    loop {
        match pc {
            Pc::Loop => {
                if ptr_stack.is_empty() {
                    break;
                }
                curr_ptr = ptr_stack.pop();
                v = ls_state_no(net, curr_ptr); /* source state number */
                vp = ls_target(net, curr_ptr); /* target state number */

                /* T: v.letterlist = list_union(v'->list, current edge label) */
                letterbits_union(v, vp, &mut medh.letterbits, bpla);
                letterbits_add(v, ls_in(net, curr_ptr) as i32, &mut medh.letterbits, bpla);

                sccinfo[v as usize].lowlink = sccinfo[v as usize]
                    .lowlink
                    .min(sccinfo[vp as usize].lowlink);

                if ls_state_no(net, curr_ptr + 1) != ls_state_no(net, curr_ptr) {
                    pc = Pc::L4;
                } else {
                    pc = Pc::L3;
                }
            }
            Pc::L1 => {
                v = ls_state_no(net, curr_ptr);
                vp = ls_target(net, curr_ptr); /* target */
                /* T: v.lowlink = index, index++, Tpush(v) */
                sccinfo[v as usize].index = index;
                sccinfo[v as usize].lowlink = index;
                index += 1;
                int_stack.push(v);
                sccinfo[v as usize].on_t_stack = 1;

                if vp == -1 {
                    pc = Pc::L4;
                } else {
                    pc = Pc::L2;
                }
            }
            Pc::L2 => {
                letterbits_add(v, ls_in(net, curr_ptr) as i32, &mut medh.letterbits, bpla);
                if sccinfo[vp as usize].index == 0 {
                    /* push (v,e) ptr on stack */
                    ptr_stack.push(curr_ptr);
                    let tgt = ls_target(net, curr_ptr) as usize;
                    curr_ptr = medh.state_array[tgt].transitions;
                    /* (v,e) = (v',firstedge), goto init */
                    pc = Pc::L1;
                } else {
                    if sccinfo[vp as usize].on_t_stack != 0 {
                        sccinfo[v as usize].lowlink = sccinfo[v as usize]
                            .lowlink
                            .min(sccinfo[vp as usize].lowlink);
                    }
                    /* If node is visited, copy its bits */
                    letterbits_union(v, vp, &mut medh.letterbits, bpla);
                    pc = Pc::L3;
                }
            }
            Pc::L3 => {
                if ls_state_no(net, curr_ptr + 1) == ls_state_no(net, curr_ptr) {
                    curr_ptr += 1;
                    v = ls_state_no(net, curr_ptr);
                    vp = ls_target(net, curr_ptr); /* target */
                    pc = Pc::L2;
                } else {
                    pc = Pc::L4;
                }
            }
            Pc::L4 => {
                /* Copy all bits from root of SCC to descendants */
                if sccinfo[v as usize].lowlink == sccinfo[v as usize].index {
                    loop {
                        copystate = int_stack.pop();
                        if copystate == v {
                            break;
                        }
                        sccinfo[copystate as usize].on_t_stack = 0;
                        letterbits_copy(v, copystate, &mut medh.letterbits, bpla);
                    }
                    sccinfo[v as usize].on_t_stack = 0;
                }
                pc = Pc::Loop;
            }
        }
    }

    /* (two commented-out debug loops in C have no effect) */
    int_stack.clear();

    /* We do the same thing for some finite n (up to maxdepth) in nletterbits */
    v = 0;
    while v < num_states {
        ptr_stack.push(medh.state_array[v as usize].transitions);
        int_stack.push(0);
        while !ptr_stack.is_empty() {
            curr_ptr = ptr_stack.pop();
            depth = int_stack.pop();
            /* looper: */
            loop {
                if depth == medh.maxdepth {
                    break; /* continue outer while */
                }
                if ls_in(net, curr_ptr) != -1 {
                    letterbits_add(v, ls_in(net, curr_ptr) as i32, &mut medh.nletterbits, bpla);
                }
                if ls_target(net, curr_ptr) != -1 {
                    if ls_state_no(net, curr_ptr) == ls_state_no(net, curr_ptr + 1) {
                        ptr_stack.push(curr_ptr + 1);
                        int_stack.push(depth);
                    }
                    depth += 1;
                    let tgt = ls_target(net, curr_ptr) as usize;
                    curr_ptr = medh.state_array[tgt].transitions;
                    /* goto looper */
                } else {
                    break;
                }
            }
        }
        v += 1;
    }
    /* free(sccinfo) */
    drop(sccinfo);
}

// [spec:foma:def:spelling.apply-med-init-fn]
// [spec:foma:sem:spelling.apply-med-init-fn]
// [spec:foma:def:fomalib.apply-med-init-fn]
// [spec:foma:sem:fomalib.apply-med-init-fn]
// Net is borrowed in C (must outlive the handle); the handle keeps an owned
// copy here (see the ApplyMedHandle.net deviation in types.rs).
pub fn apply_med_init(net: &Fsm) -> Box<ApplyMedHandle> {
    /* calloc(1, ...): every field starts zero/None/empty/false. */
    let mut medh: Box<ApplyMedHandle> = Box::new(ApplyMedHandle {
        agenda: Vec::new(),
        bytes_per_letter_array: 0,
        letterbits: Vec::new(),
        nletterbits: Vec::new(),
        astarcount: 0,
        heapcount: 0,
        heap_size: 0,
        agenda_size: 0,
        maxdepth: 0,
        maxsigma: 0,
        wordlen: 0,
        utf8len: 0,
        cost: 0,
        nummatches: 0,
        curr_state: 0,
        curr_g: 0,
        curr_pos: 0,
        lines: 0,
        curr_agenda_offset: 0,
        curr_node_has_match: 0,
        med_limit: 0,
        med_cutoff: 0,
        med_max_heap_size: 0,
        nodes_expanded: 0,
        cm: Vec::new(),
        word: None,
        instring: String::new(),
        outstring: String::new(),
        align_symbol: None,
        heap: Vec::new(),
        intword: Vec::new(),
        sigmahash: None,
        state_array: Vec::new(),
        net: None,
        curr_ptr: None,
        hascm: false,
    });
    medh.net = Some(Box::new(net.clone())); /* DEVIATION: owned copy of borrowed net */
    medh.agenda = vec![
        Astarnode {
            wordpos: 0,
            fsmstate: 0,
            f: 0,
            g: 0,
            h: 0,
            r#in: 0,
            out: 0,
            parent: 0,
        };
        INITIAL_AGENDA_SIZE as usize
    ];
    medh.agenda[0].f = -1; /* slot 0 is the permanent heap sentinel */
    medh.agenda_size = INITIAL_AGENDA_SIZE;

    medh.heap = vec![0i32; INITIAL_HEAP_SIZE as usize];
    medh.heap_size = INITIAL_HEAP_SIZE;
    medh.heap[0] = 0; /* Points to sentinel */
    medh.astarcount = 1;
    medh.heapcount = 0;
    medh.state_array = map_firstlines(net);
    if let Some(ml) = &net.medlookup {
        if !ml.confusion_matrix.is_empty() {
            medh.hascm = true;
            medh.cm = ml.confusion_matrix.clone(); /* DEVIATION: owned copy of borrowed matrix */
        }
    }
    medh.maxsigma = sigma_max(&net.sigma) + 1;
    let sigmahash = medh.sigmahash.insert(sh_init());
    for s in &net.sigma {
        if s.number > IDENTITY {
            let _ = sh_add_string(sigmahash, &s.symbol, s.number);
        }
    }

    fsm_create_letter_lookup(&mut medh, net);

    medh.instring = String::new();
    medh.outstring = String::new();

    medh.med_limit = MED_DEFAULT_LIMIT;
    medh.med_cutoff = MED_DEFAULT_CUTOFF;
    medh.med_max_heap_size = MED_DEFAULT_MAX_HEAP_SIZE;
    medh
}

// [spec:foma:def:spelling.apply-med-fn]
// [spec:foma:sem:spelling.apply-med-fn]
// [spec:foma:def:fomalib.apply-med-fn]
// [spec:foma:sem:fomalib.apply-med-fn]
// Non-None `word` starts a fresh search; None resumes the previous one (jumps
// straight to the `resume` point in the expansion loop). Returns the matched
// dictionary-side word (medh.outstring) on each match, None when exhausted.
pub fn apply_med(medh: &mut ApplyMedHandle, word: Option<&str>) -> Option<String> {
    /* local ok: target, in, out, g, h, curr_node
    not ok: curr_ptr, curr_pos, lines, nummatches, nodes_expanded, curr_state */

    let mut target: i32;
    let mut r#in: i32;
    let mut out: i32;
    let mut g: i32;
    let mut h: i32;

    let delcost: i32 = 1;
    let subscost: i32 = 1;
    let inscost: i32 = 1;

    /* DEVIATION from C (resume-before-any-search is UB in C — the handle's
    resume state is uninitialized; here medh.curr_ptr is None from calloc and
    the first line access below panics rather than dereferencing garbage). */
    let mut resuming = false;
    match word {
        None => {
            resuming = true; /* goto resume */
        }
        Some(w) => {
            let wbytes = w.as_bytes();
            medh.word = Some(w.into()); /* DEVIATION: owned copy (C: medh->word = word) */

            medh.nodes_expanded = 0;
            medh.astarcount = 1;
            medh.heapcount = 0;

            medh.wordlen = wbytes.len() as i32; /* strlen(word) */
            medh.utf8len = w.chars().count() as i32;
            /* free previous intword + malloc utf8len+1 ints */
            medh.intword = vec![0i32; (medh.utf8len + 1) as usize];

            /* intword -> sigma numbers of word, one entry per input character */
            let mut j = 0usize;
            let mut cbuf = [0u8; 4];
            for c in w.chars() {
                let ch = c.encode_utf8(&mut cbuf);
                let sigmahash = medh
                    .sigmahash
                    .as_mut()
                    .expect("sigmahash built in med init");
                medh.intword[j] = match sh_find_string(sigmahash, ch) {
                    Some(_) => sh_get_value(sigmahash),
                    None => IDENTITY,
                };
                j += 1;
            }
            medh.intword[j] = -1; /* sentinel */

            /* Insert (0,0) g = 0 */
            h = calculate_h(medh, &medh.intword, 0, 0);

            /* Root node */
            if !node_insert(medh, 0, 0, 0, h, 0, 0, -1) {
                return None; /* goto out */
            }
            medh.nummatches = 0;
        }
    }

    'outer: loop {
        if !resuming {
            let curr_node = node_delete_min(medh);
            /* Save this in case we realloc and print_match(); computed before
            the None check as in C (benign — unused when None). */
            medh.curr_agenda_offset = curr_node.unwrap_or(0);
            let Some(curr_node_idx) = curr_node else {
                break 'outer; /* goto out */
            };
            medh.curr_state = medh.agenda[curr_node_idx as usize].fsmstate;
            medh.curr_ptr = Some(medh.state_array[medh.curr_state as usize].transitions);
            /* leftover conditional with an empty body (dead code) */
            if cur_final(medh) == 0
                || (medh.agenda[curr_node_idx as usize].wordpos as i32 != medh.utf8len)
            {
                //continue;
            }

            medh.nodes_expanded += 1;

            if (medh.agenda[curr_node_idx as usize].f as i32) > medh.med_cutoff {
                break 'outer; /* goto out */
            }

            medh.curr_pos = medh.agenda[curr_node_idx as usize].wordpos as i32;
            medh.curr_state = medh.agenda[curr_node_idx as usize].fsmstate;
            medh.curr_g = medh.agenda[curr_node_idx as usize].g as i32;

            medh.lines = 0;
            medh.curr_node_has_match = 0;

            medh.curr_ptr = Some(medh.state_array[medh.curr_state as usize].transitions);
        }

        'inner: loop {
            if !resuming {
                if cur_state_no(medh) == -1 {
                    break 'inner;
                }
                medh.lines += 1;
                if cur_final(medh) != 0
                    && medh.curr_pos == medh.utf8len
                    && medh.curr_node_has_match == 0
                {
                    /* Found a match */
                    medh.curr_node_has_match = 1;
                    let sigma = med_net(medh).sigma.clone();
                    let word = medh
                        .word
                        .clone()
                        .expect("word set for the current med lookup");
                    let off = medh.curr_agenda_offset as usize;
                    print_match(medh, off, &sigma, &word);
                    medh.nummatches += 1;
                    return Some(medh.outstring.clone());
                }
            }
            resuming = false;

            /* resume: */
            'skip_block: {
                'insert_block: {
                    if medh.nummatches == medh.med_limit {
                        break 'outer; /* goto out */
                    }

                    if cur_target(medh) == -1 && medh.curr_pos == medh.utf8len {
                        break 'inner;
                    }
                    if cur_target(medh) == -1 && medh.lines == 1 {
                        break 'insert_block; /* goto insert */
                    }
                    if cur_target(medh) == -1 {
                        break 'inner;
                    }

                    target = cur_target(medh);
                    /* Add nodes to edge:0, edge:input, 0:edge */

                    /* Delete a symbol from input */
                    r#in = cur_in(medh) as i32;
                    out = 0;
                    g = if medh.hascm {
                        medh.curr_g + medh.cm[(r#in * medh.maxsigma) as usize]
                    } else {
                        medh.curr_g + delcost
                    };
                    h = calculate_h(medh, &medh.intword, medh.curr_pos, cur_target(medh));

                    if (medh.curr_pos == medh.utf8len) && (cur_final(medh) == 0) && (h == 0) {
                        // h = 1;
                    }

                    if g + h <= medh.med_cutoff
                        && !node_insert(
                            medh,
                            medh.curr_pos,
                            target,
                            g,
                            h,
                            r#in,
                            out,
                            medh.curr_agenda_offset,
                        )
                    {
                        break 'outer; /* goto out */
                    }
                    if medh.curr_pos == medh.utf8len {
                        break 'skip_block; /* goto skip */
                    }

                    /* Match/substitute */
                    r#in = cur_in(medh) as i32;
                    out = medh.intword[medh.curr_pos as usize];
                    if r#in != out {
                        g = if medh.hascm {
                            medh.curr_g + medh.cm[(r#in * medh.maxsigma + out) as usize]
                        } else {
                            medh.curr_g + subscost
                        };
                    } else {
                        g = medh.curr_g;
                    }

                    h = calculate_h(medh, &medh.intword, medh.curr_pos + 1, cur_target(medh));
                    if (g + h) <= medh.med_cutoff
                        && !node_insert(
                            medh,
                            medh.curr_pos + 1,
                            target,
                            g,
                            h,
                            r#in,
                            out,
                            medh.curr_agenda_offset,
                        )
                    {
                        break 'outer; /* goto out */
                    }
                } /* insert: */

                /* Insert a symbol into input — can only be done once per state */
                if medh.lines == 1 {
                    r#in = 0;
                    out = medh.intword[medh.curr_pos as usize];

                    g = if medh.hascm {
                        medh.curr_g + medh.cm[out as usize]
                    } else {
                        medh.curr_g + inscost
                    };
                    h = calculate_h(medh, &medh.intword, medh.curr_pos + 1, medh.curr_state);

                    if g + h <= medh.med_cutoff
                        && !node_insert(
                            medh,
                            medh.curr_pos + 1,
                            medh.curr_state,
                            g,
                            h,
                            r#in,
                            out,
                            medh.curr_agenda_offset,
                        )
                    {
                        break 'outer; /* goto out */
                    }
                }
                if cur_target(medh) == -1 {
                    break 'inner;
                }
            } /* skip: */

            if next_state_no(medh) == cur_state_no(medh) {
                medh.curr_ptr = Some(med_curr_ptr(medh) + 1);
            } else {
                break 'inner;
            }
        }
    }
    /* out: */
    None
}

// [spec:foma:def:spelling.cmatrix-print-att-fn]
// [spec:foma:sem:spelling.cmatrix-print-att-fn]
// [spec:foma:def:fomalib.cmatrix-print-att-fn]
// [spec:foma:sem:fomalib.cmatrix-print-att-fn]
pub fn cmatrix_print_att<W: std::io::Write + ?Sized>(net: &Fsm, outfile: &mut W) {
    let maxsigma = sigma_max(&net.sigma) + 1;
    let cm = &net
        .medlookup
        .as_ref()
        .expect("confusion matrix present (caller checked)")
        .confusion_matrix;

    for i in 0..maxsigma {
        for j in 0..maxsigma {
            if (i != 0 && i < 3) || (j != 0 && j < 3) {
                continue;
            }
            if i == 0 && j != 0 {
                writeln!(
                    outfile,
                    "0\t0\t@0@\t{}\t{}",
                    sigma_string(j, &net.sigma).expect("symbol number in matrix range resolves"),
                    cm[(i * maxsigma + j) as usize]
                )
                .expect("writing the confusion matrix");
            } else if j == 0 && i != 0 {
                writeln!(
                    outfile,
                    "0\t0\t{}\t@0@\t{}",
                    sigma_string(i, &net.sigma).expect("symbol number in matrix range resolves"),
                    cm[(i * maxsigma + j) as usize]
                )
                .expect("writing the confusion matrix");
            } else if j != 0 && i != 0 {
                writeln!(
                    outfile,
                    "0\t0\t{}\t{}\t{}",
                    sigma_string(i, &net.sigma).expect("symbol number in matrix range resolves"),
                    sigma_string(j, &net.sigma).expect("symbol number in matrix range resolves"),
                    cm[(i * maxsigma + j) as usize]
                )
                .expect("writing the confusion matrix");
            }
        }
    }
    writeln!(outfile, "0").expect("writing the confusion matrix");
}

// [spec:foma:def:spelling.cmatrix-print-fn]
// [spec:foma:sem:spelling.cmatrix-print-fn+1]
// [spec:foma:def:fomalib.cmatrix-print-fn]
// [spec:foma:sem:fomalib.cmatrix-print-fn+1]
pub fn cmatrix_print(net: &Fsm) {
    cmatrix_print_to(net, &mut std::io::stdout());
}

/// Writer-generic core of [cmatrix_print]. The diagonal cell (i == j) is filled
/// to the full column width (strlen(sym_j)+1); the C source used that count as a
/// `%.*s` *precision*, truncating "*" to one char and under-filling the column,
/// which misaligned every row past the diagonal.
fn cmatrix_print_to<W: std::io::Write + ?Sized>(net: &Fsm, out: &mut W) {
    let maxsigma = sigma_max(&net.sigma) + 1;
    let cm = &net
        .medlookup
        .as_ref()
        .expect("confusion matrix present (caller checked)")
        .confusion_matrix;

    let mut lsymbol: i32 = 0;
    for s in &net.sigma {
        if s.number < 3 {
            continue;
        }
        /* strlen(sigma->symbol) — byte length, as in C */
        let l = s.symbol.len() as i32;
        lsymbol = if l > lsymbol { l } else { lsymbol };
    }
    write!(out, "{:>w$}", "", w = (lsymbol + 2) as usize).expect("writing the confusion matrix");
    write!(out, "0 ").expect("writing the confusion matrix");

    let mut i = 3;
    while let Some(thisstring) = sigma_string(i, &net.sigma) {
        write!(out, "{} ", thisstring).expect("writing the confusion matrix");
        i += 1;
    }

    writeln!(out).expect("writing the confusion matrix");

    let mut i = 0i32;
    while i < maxsigma {
        let mut j = 0i32;
        while j < maxsigma {
            if j == 0 {
                if i == 0 {
                    write!(out, "{:>w$}", "0", w = (lsymbol + 1) as usize)
                        .expect("writing the confusion matrix");
                    write!(out, "{:>2}", "*").expect("writing the confusion matrix");
                } else {
                    write!(
                        out,
                        "{:>w$}",
                        sigma_string(i, &net.sigma)
                            .expect("symbol number in matrix range resolves"),
                        w = (lsymbol + 1) as usize
                    )
                    .expect("writing the confusion matrix");
                    write!(out, "{:>2}", cm[(i * maxsigma + j) as usize])
                        .expect("writing the confusion matrix");
                }
                j += 1;
                j += 1;
                j += 1; /* the for-loop's own j++ (C: continue) */
                continue;
            }
            if i == j {
                let width = sigma_string(j, &net.sigma)
                    .expect("symbol number in matrix range resolves")
                    .len()
                    + 1;
                write!(out, "{:>w$}", "*", w = width).expect("writing the confusion matrix");
            } else {
                /* printf("%.*d", strlen(sym_j)+1, cost) — zero-padded width */
                let width = sigma_string(j, &net.sigma)
                    .expect("symbol number in matrix range resolves")
                    .len()
                    + 1;
                write!(out, "{:0w$}", cm[(i * maxsigma + j) as usize], w = width)
                    .expect("writing the confusion matrix");
            }
            j += 1;
        }
        writeln!(out).expect("writing the confusion matrix");
        if i == 0 {
            i += 1;
            i += 1;
        }
        i += 1;
    }
}

// [spec:foma:def:spelling.cmatrix-init-fn]
// [spec:foma:sem:spelling.cmatrix-init-fn]
// [spec:foma:def:fomalib.cmatrix-init-fn]
// [spec:foma:sem:fomalib.cmatrix-init-fn]
pub fn cmatrix_init(net: &mut Fsm) {
    if net.medlookup.is_none() {
        net.medlookup = Some(Box::new(Medlookup {
            confusion_matrix: Vec::new(),
        }));
    }
    let maxsigma: i32 = sigma_max(&net.sigma) + 1;
    let mut cm = vec![0i32; (maxsigma * maxsigma) as usize];
    for i in 0..maxsigma {
        for j in 0..maxsigma {
            if i == j {
                cm[(i * maxsigma + j) as usize] = 0;
            } else {
                cm[(i * maxsigma + j) as usize] = 1;
            }
        }
    }
    net.medlookup
        .as_mut()
        .expect("confusion matrix present (caller checked)")
        .confusion_matrix = cm;
}

// [spec:foma:def:spelling.cmatrix-default-substitute-fn]
// [spec:foma:sem:spelling.cmatrix-default-substitute-fn]
// [spec:foma:def:fomalib.cmatrix-default-substitute-fn]
// [spec:foma:sem:fomalib.cmatrix-default-substitute-fn]
pub fn cmatrix_default_substitute(net: &mut Fsm, cost: i32) {
    let maxsigma = sigma_max(&net.sigma) + 1;
    let cm = &mut net
        .medlookup
        .as_mut()
        .expect("confusion matrix present (caller checked)")
        .confusion_matrix;
    for i in 1..maxsigma {
        for j in 1..maxsigma {
            if i == j {
                cm[(i * maxsigma + j) as usize] = 0;
            } else {
                cm[(i * maxsigma + j) as usize] = cost;
            }
        }
    }
}

// [spec:foma:def:spelling.cmatrix-default-insert-fn]
// [spec:foma:sem:spelling.cmatrix-default-insert-fn]
// [spec:foma:def:fomalib.cmatrix-default-insert-fn]
// [spec:foma:sem:fomalib.cmatrix-default-insert-fn]
pub fn cmatrix_default_insert(net: &mut Fsm, cost: i32) {
    let maxsigma = sigma_max(&net.sigma) + 1;
    let cm = &mut net
        .medlookup
        .as_mut()
        .expect("confusion matrix present (caller checked)")
        .confusion_matrix;
    for i in 0..maxsigma {
        cm[i as usize] = cost;
    }
}

// [spec:foma:def:spelling.cmatrix-default-delete-fn]
// [spec:foma:sem:spelling.cmatrix-default-delete-fn]
// [spec:foma:def:fomalib.cmatrix-default-delete-fn]
// [spec:foma:sem:fomalib.cmatrix-default-delete-fn]
pub fn cmatrix_default_delete(net: &mut Fsm, cost: i32) {
    let maxsigma = sigma_max(&net.sigma) + 1;
    let cm = &mut net
        .medlookup
        .as_mut()
        .expect("confusion matrix present (caller checked)")
        .confusion_matrix;
    for i in 0..maxsigma {
        cm[(i * maxsigma) as usize] = cost;
    }
}

// [spec:foma:def:spelling.cmatrix-set-cost-fn]
// [spec:foma:sem:spelling.cmatrix-set-cost-fn]
// [spec:foma:def:fomalib.cmatrix-set-cost-fn]
// [spec:foma:sem:fomalib.cmatrix-set-cost-fn]
pub fn cmatrix_set_cost(net: &mut Fsm, r#in: Option<&str>, out: Option<&str>, cost: i32) {
    let maxsigma = sigma_max(&net.sigma) + 1;
    // A symbol lookup only fails for a Some(s) that isn't in the alphabet; None
    // maps to 0, so the -1 case always carries the offending symbol.
    let i: i32 = match r#in {
        None => 0,
        Some(s) => match sigma_find(s, &net.sigma) {
            Some(found) => found,
            None => {
                tracing::warn!("symbol '{}' not in alphabet", s);
                return;
            }
        },
    };
    let o: i32 = match out {
        None => 0,
        Some(s) => match sigma_find(s, &net.sigma) {
            Some(found) => found,
            None => {
                tracing::warn!("symbol '{}' not in alphabet", s);
                return;
            }
        },
    };
    let cm = &mut net
        .medlookup
        .as_mut()
        .expect("confusion matrix present (caller checked)")
        .confusion_matrix;
    cm[(i * maxsigma + o) as usize] = cost;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regex::fsm_parse_regex;
    use crate::structures::fsm_sort_arcs;
    use crate::types::Sigma;

    /* Minimized net from a regex, arcs sorted so each state's lines are
    contiguous (apply_med requires it). */
    fn parse_sorted(rx: &str) -> Box<Fsm> {
        let opts = &FomaOptions::default();
        let mut net = fsm_parse_regex(opts, rx, None, None).expect("regex should compile");
        fsm_sort_arcs(&mut net, 1);
        net
    }

    /* Full A* result-set: first call passes the word, NULL-resume drains the
    remaining matches. Returns (dictionary-side, aligned-input, cost) triples. */
    fn med_all(h: &mut ApplyMedHandle, word: &str) -> Vec<(String, String, i32)> {
        let mut out = Vec::new();
        let mut r = apply_med(h, Some(word));
        while let Some(s) = r {
            let ins = apply_med_get_instring(h).unwrap();
            let cost = apply_med_get_cost(h);
            out.push((s, ins, cost));
            r = apply_med(h, None);
        }
        out
    }

    // [spec:foma:sem:spelling.print-sym-fn/test]
    #[test]
    fn print_sym_linear_scan() {
        let sig = vec![
            Sigma {
                number: 4,
                symbol: "abc".into(),
            },
            Sigma {
                number: 3,
                symbol: "a".into(),
            },
        ];
        assert_eq!(print_sym(4, &sig), Some("abc"));
        assert_eq!(print_sym(3, &sig), Some("a"));
        assert_eq!(print_sym(99, &sig), None);
    }

    // [spec:foma:sem:spelling.letterbits-add-fn/test]
    // [spec:foma:sem:spelling.letterbits-union-fn/test]
    // [spec:foma:sem:spelling.letterbits-copy-fn/test]
    #[test]
    fn letterbits_primitives() {
        // 2 states, 1 byte per state.
        let mut buf = vec![0u8; 2];
        letterbits_add(0, 3, &mut buf, 1);
        letterbits_add(0, 5, &mut buf, 1);
        assert_eq!(buf[0], (1 << 3) | (1 << 5)); // 40
        // union OR's state 0 into state 1.
        letterbits_union(1, 0, &mut buf, 1);
        assert_eq!(buf[1], 40);
        // add another bit to state 0, then copy overwrites state 1 entirely.
        letterbits_add(0, 0, &mut buf, 1);
        assert_eq!(buf[0], 41);
        letterbits_copy(0, 1, &mut buf, 1);
        assert_eq!(buf[1], 41);
    }

    // [spec:foma:sem:spelling.apply-med-init-fn/test]
    // [spec:foma:sem:fomalib.apply-med-init-fn/test]
    #[test]
    fn med_init_defaults() {
        let net = parse_sorted("{cat}");
        let medh = apply_med_init(&net);
        assert_eq!(medh.med_limit, MED_DEFAULT_LIMIT); // 4
        assert_eq!(medh.med_cutoff, MED_DEFAULT_CUTOFF); // 15
        assert_eq!(medh.med_max_heap_size, MED_DEFAULT_MAX_HEAP_SIZE);
        assert_eq!(medh.agenda[0].f, -1); // permanent heap sentinel
        assert_eq!(medh.astarcount, 1);
        assert_eq!(medh.heapcount, 0);
        // sigma is {a,c,t} => sigma_max 5, maxsigma 6.
        assert_eq!(medh.maxsigma, 6);
        assert!(!medh.state_array.is_empty());
        assert!(medh.sigmahash.is_some());
        // no confusion matrix on a bare net.
        assert!(!medh.hascm);
    }

    // [spec:foma:sem:spelling.fsm-create-letter-lookup-fn/test]
    // [spec:foma:sem:fomalib.fsm-create-letter-lookup-fn/test]
    #[test]
    fn letter_lookup_bitsets() {
        // {cat}: state 0 -c-> 1 -a-> 2 -t-> 3(final). sigma a=3,c=4,t=5.
        let net = parse_sorted("{cat}");
        let medh = apply_med_init(&net);
        assert_eq!(medh.maxdepth, 2);
        assert_eq!(medh.bytes_per_letter_array, 1);
        // letterbits[v] = all labels reachable from v (n = infinity):
        //  state 0 can still see a,c,t; state 1 a,t; state 2 t; final none.
        assert_eq!(medh.letterbits, vec![56u8, 40, 32, 0]);
        // nletterbits[v] = labels within maxdepth (2) transitions.
        assert_eq!(medh.nletterbits, vec![24u8, 40, 32, 0]);
    }

    // [spec:foma:sem:spelling.calculate-h-fn/test]
    #[test]
    fn calculate_h_heuristic() {
        let net = parse_sorted("{cat}");
        let medh = apply_med_init(&net);
        // sentinel at currpos -> 0.
        assert_eq!(calculate_h(&medh, &[-1], 0, 0), 0);
        // 'c' (sigma 4) is reachable from state 0 -> costs nothing extra.
        assert_eq!(calculate_h(&medh, &[4, -1], 0, 0), 0);
        // from the final state 3 nothing is reachable -> each suffix symbol costs 1.
        assert_eq!(calculate_h(&medh, &[3, -1], 0, 3), 1);
        // every occurrence counts.
        assert_eq!(calculate_h(&medh, &[3, 3, -1], 0, 3), 2);
    }

    // [spec:foma:sem:spelling.node-insert-fn/test]
    // [spec:foma:sem:spelling.node-delete-min-fn/test]
    #[test]
    fn heap_ordering() {
        let net = parse_sorted("{cat}");
        let mut medh = apply_med_init(&net);
        // Insert three nodes: (f=2,wp=0), (f=1,wp=1), (f=1,wp=3).
        assert!(node_insert(&mut medh, 0, 0, 2, 0, 0, 0, -1)); // agenda idx 1
        assert!(node_insert(&mut medh, 1, 0, 1, 0, 0, 0, -1)); // agenda idx 2
        assert!(node_insert(&mut medh, 3, 0, 1, 0, 0, 0, -1)); // agenda idx 3
        // Priority: smaller f first; ties prefer larger wordpos.
        assert_eq!(node_delete_min(&mut medh), Some(3)); // f1,wp3
        assert_eq!(node_delete_min(&mut medh), Some(2)); // f1,wp1
        assert_eq!(node_delete_min(&mut medh), Some(1)); // f2
        assert_eq!(node_delete_min(&mut medh), None); // exhausted
    }

    // [spec:foma:sem:spelling.apply-med-fn/test]
    // [spec:foma:sem:fomalib.apply-med-fn/test]
    // [spec:foma:sem:spelling.print-match-fn/test]
    // [spec:foma:sem:spelling.apply-med-get-cost-fn/test]
    // [spec:foma:sem:fomalib.apply-med-get-cost-fn/test]
    // [spec:foma:sem:spelling.apply-med-get-instring-fn/test]
    // [spec:foma:sem:fomalib.apply-med-get-instring-fn/test]
    // [spec:foma:sem:spelling.apply-med-get-outstring-fn/test]
    // [spec:foma:sem:fomalib.apply-med-get-outstring-fn/test]
    #[test]
    fn med_matches_and_costs() {
        let net = parse_sorted("{cat}|{car}|{dog}");
        let mut h = apply_med_init(&net);
        // Exact match cost 0 first, then unit-cost edits (C-foma-verified).
        let r = med_all(&mut h, "cat");
        assert_eq!(
            r,
            vec![
                ("cat".to_string(), "cat".to_string(), 0), // exact
                ("car".to_string(), "cat".to_string(), 1), // 1 substitution
                ("car".to_string(), "cat".to_string(), 2),
                ("cat".to_string(), "cat".to_string(), 2),
            ]
        );
        // getters reflect the most recent (last) match.
        assert_eq!(apply_med_get_outstring(&h).unwrap(), "cat");
        assert_eq!(apply_med_get_instring(&h).unwrap(), "cat");

        // Deletion: "ca" -> cat/car at cost 1 (delete a trailing symbol).
        let mut h2 = apply_med_init(&net);
        let best = &med_all(&mut h2, "ca")[0];
        assert_eq!(best.2, 1);
        assert_eq!(best.1, "ca");
        assert!(best.0 == "cat" || best.0 == "car");

        // Insertion: "cats" -> cat at cost 1 (insert one word symbol).
        let mut h3 = apply_med_init(&net);
        let best = &med_all(&mut h3, "cats")[0];
        assert_eq!(best.0, "cat");
        assert_eq!(best.2, 1);
    }

    // [spec:foma:sem:spelling.apply-med-set-med-limit-fn/test]
    // [spec:foma:sem:fomalib.apply-med-set-med-limit-fn/test]
    #[test]
    fn med_limit_enforced() {
        let net = parse_sorted("{cat}|{car}|{dog}");
        let mut h = apply_med_init(&net);
        apply_med_set_med_limit(&mut h, 1);
        assert_eq!(h.med_limit, 1);
        assert_eq!(med_all(&mut h, "cat").len(), 1);
        let mut h2 = apply_med_init(&net);
        apply_med_set_med_limit(&mut h2, 2);
        assert_eq!(med_all(&mut h2, "cat").len(), 2);
    }

    // [spec:foma:sem:spelling.apply-med-set-med-cutoff-fn/test]
    // [spec:foma:sem:fomalib.apply-med-set-med-cutoff-fn/test]
    #[test]
    fn med_cutoff_enforced() {
        let net = parse_sorted("{cat}|{car}|{dog}");
        let mut h = apply_med_init(&net);
        apply_med_set_med_cutoff(&mut h, 1);
        assert_eq!(h.med_cutoff, 1);
        // No dictionary word is within total cost 1 of "zzzzzz".
        assert!(med_all(&mut h, "zzzzzz").is_empty());
    }

    // [spec:foma:sem:spelling.apply-med-set-align-symbol-fn/test]
    // [spec:foma:sem:fomalib.apply-med-set-align-symbol-fn/test]
    #[test]
    fn med_align_symbol_in_output() {
        let net = parse_sorted("{cat}");
        let mut h = apply_med_init(&net);
        apply_med_set_align_symbol(&mut h, "-");
        assert_eq!(h.align_symbol.as_deref(), Some("-"));
        // "bat" vs "cat": the cost-2 alignments carry an epsilon slot rendered
        // as the align symbol on one side.
        let r = med_all(&mut h, "bat");
        assert!(
            r.iter().any(|(o, i, _)| o.contains('-') || i.contains('-')),
            "expected an alignment dash in {:?}",
            r
        );
    }

    // [spec:foma:sem:spelling.apply-med-set-heap-max-fn/test]
    // [spec:foma:sem:fomalib.apply-med-set-heap-max-fn/test]
    #[test]
    fn med_set_heap_max() {
        let net = parse_sorted("{cat}");
        let mut h = apply_med_init(&net);
        apply_med_set_heap_max(&mut h, 99);
        assert_eq!(h.med_max_heap_size, 99);
    }

    // [spec:foma:sem:spelling.apply-med-clear-fn/test]
    // [spec:foma:sem:fomalib.apply-med-clear-fn/test]
    #[test]
    fn med_clear_consumes_handle() {
        let net = parse_sorted("{cat}");
        let h = apply_med_init(&net);
        apply_med_clear(Some(h)); // frees everything
        apply_med_clear(None); // NULL handle is a no-op
    }

    // [spec:foma:sem:spelling.apply-med-fn/test]
    // [spec:foma:sem:fomalib.apply-med-fn/test]
    #[test]
    #[should_panic]
    fn med_null_resume_before_search_panics() {
        // DEVIATION: resume-before-any-search is UB in C; the port panics because
        // curr_ptr is None from calloc.
        let net = parse_sorted("{cat}");
        let mut h = apply_med_init(&net);
        let _ = apply_med(&mut h, None);
    }

    // [spec:foma:sem:spelling.cmatrix-init-fn/test]
    // [spec:foma:sem:fomalib.cmatrix-init-fn/test]
    #[test]
    fn cmatrix_init_costs() {
        let mut net = parse_sorted("{cat}");
        cmatrix_init(&mut net);
        let cm = &net.medlookup.as_ref().unwrap().confusion_matrix;
        // maxsigma = 6; identity cells 0, all others 1.
        assert_eq!(cm.len(), 36);
        assert_eq!(cm[3 * 6 + 3], 0); // a->a
        assert_eq!(cm[3 * 6 + 4], 1); // a->c
        assert_eq!(cm[0], 0); // 0->0 diagonal
        assert_eq!(cm[4], 1); // insertion of c
    }

    // [spec:foma:sem:spelling.cmatrix-set-cost-fn/test]
    // [spec:foma:sem:fomalib.cmatrix-set-cost-fn/test]
    #[test]
    fn cmatrix_set_cost_cell_and_warning() {
        let mut net = parse_sorted("{cat}");
        cmatrix_init(&mut net);
        // substitution c -> a costs 4.
        cmatrix_set_cost(&mut net, Some("c"), Some("a"), 4);
        {
            let cm = &net.medlookup.as_ref().unwrap().confusion_matrix;
            // row = dictionary symbol c(4), col = word symbol a(3).
            assert_eq!(cm[4 * 6 + 3], 4);
        }
        // an unknown symbol warns and leaves the matrix unchanged.
        cmatrix_set_cost(&mut net, Some("Z"), None, 9);
        let cm = &net.medlookup.as_ref().unwrap().confusion_matrix;
        assert_eq!(cm[4 * 6 + 3], 4);
    }

    // [spec:foma:sem:spelling.cmatrix-default-substitute-fn/test]
    // [spec:foma:sem:fomalib.cmatrix-default-substitute-fn/test]
    #[test]
    fn cmatrix_default_substitute_costs() {
        let mut net = parse_sorted("{cat}");
        cmatrix_init(&mut net);
        cmatrix_default_substitute(&mut net, 7);
        let cm = &net.medlookup.as_ref().unwrap().confusion_matrix;
        assert_eq!(cm[3 * 6 + 4], 7); // off-diagonal substitution
        assert_eq!(cm[3 * 6 + 3], 0); // diagonal stays free
        assert_eq!(cm[4], 1); // row 0 (insertion) untouched
    }

    // [spec:foma:sem:spelling.cmatrix-default-insert-fn/test]
    // [spec:foma:sem:fomalib.cmatrix-default-insert-fn/test]
    #[test]
    fn cmatrix_default_insert_costs() {
        let mut net = parse_sorted("{cat}");
        cmatrix_init(&mut net);
        cmatrix_default_insert(&mut net, 9);
        let cm = &net.medlookup.as_ref().unwrap().confusion_matrix;
        // row 0 = insertion costs.
        assert_eq!(cm[3], 9);
        assert_eq!(cm[0], 9);
    }

    // [spec:foma:sem:spelling.cmatrix-default-delete-fn/test]
    // [spec:foma:sem:fomalib.cmatrix-default-delete-fn/test]
    #[test]
    fn cmatrix_default_delete_costs() {
        let mut net = parse_sorted("{cat}");
        cmatrix_init(&mut net);
        cmatrix_default_delete(&mut net, 8);
        let cm = &net.medlookup.as_ref().unwrap().confusion_matrix;
        // column 0 = deletion costs.
        assert_eq!(cm[3 * 6], 8);
        assert_eq!(cm[0], 8);
    }

    // [spec:foma:sem:spelling.apply-med-fn/test]
    #[test]
    fn cmatrix_affects_med_cost() {
        // Raising substitution cost pushes "bat" beyond a small cutoff.
        let mut net = parse_sorted("{cat}");
        cmatrix_init(&mut net);
        cmatrix_default_substitute(&mut net, 5);
        let mut h = apply_med_init(&net);
        assert!(h.hascm);
        apply_med_set_med_cutoff(&mut h, 3);
        // best "bat" match now needs 2 edits under cm cost model.
        let r = med_all(&mut h, "bat");
        assert!(r.iter().all(|(_, _, c)| *c >= 2));
    }

    // [spec:foma:sem:spelling.cmatrix-print-att-fn/test]
    // [spec:foma:sem:fomalib.cmatrix-print-att-fn/test]
    #[test]
    fn cmatrix_print_att_format() {
        let mut net = parse_sorted("{ab}"); // sigma a=3,b=4; maxsigma 5
        cmatrix_init(&mut net);
        let mut buf: Vec<u8> = Vec::new();
        cmatrix_print_att(&net, &mut buf);
        let got = String::from_utf8(buf).unwrap();
        let expected = "\
0\t0\t@0@\ta\t1
0\t0\t@0@\tb\t1
0\t0\ta\t@0@\t1
0\t0\ta\ta\t0
0\t0\ta\tb\t1
0\t0\tb\t@0@\t1
0\t0\tb\ta\t1
0\t0\tb\tb\t0
0
";
        assert_eq!(got, expected);
    }

    // [spec:foma:sem:spelling.cmatrix-print-fn/test]
    // [spec:foma:sem:fomalib.cmatrix-print-fn/test]
    #[test]
    fn cmatrix_print_runs() {
        // Writes to stdout; assert only that it renders without panicking.
        let mut net = parse_sorted("{ab}");
        cmatrix_init(&mut net);
        cmatrix_print(&net);
    }

    // [spec:foma:sem:spelling.print-match-fn/test]
    #[test]
    fn print_match_via_med_alignment() {
        // Exercises print_match's two-pass parent-chain walk end to end.
        let net = parse_sorted("{cat}");
        let mut h = apply_med_init(&net);
        let r = med_all(&mut h, "cat");
        // exact match: both aligned strings equal "cat", cost 0.
        assert_eq!(r[0].0, "cat");
        assert_eq!(r[0].1, "cat");
        assert_eq!(r[0].2, 0);
    }

    // [spec:foma:sem:spelling.print-match-fn+1/test]
    #[test]
    fn print_match_walks_through_interior_epsilon_node() {
        // Seed the parent chain root(parent=-1) <- interior epsilon:epsilon <- leaf.
        // C broke the walk at the interior in==0 && out==0 node, dropping every
        // ancestor; now only parent == -1 (the true root) terminates it, so the
        // interior epsilon contributes its align symbol.
        let net = parse_sorted("{cat}");
        let mut medh = apply_med_init(&net);
        medh.align_symbol = Some("-".into());
        medh.wordlen = 1;
        let c = 4; // 'c' has sigma number 4 (see calculate_h_heuristic).
        medh.agenda[1].r#in = 0;
        medh.agenda[1].out = 0;
        medh.agenda[1].parent = -1; // root
        medh.agenda[2].r#in = 0;
        medh.agenda[2].out = 0;
        medh.agenda[2].parent = 1; // interior epsilon:epsilon
        medh.agenda[3].r#in = c;
        medh.agenda[3].out = c;
        medh.agenda[3].parent = 2; // leaf carrying 'c'
        print_match(&mut medh, 3, &net.sigma, "c");
        // pass 1 pushes ins [4, 0]; popped LIFO -> 0 (align "-") then 4 ("c").
        assert_eq!(medh.outstring, "-c");
    }

    // [spec:foma:sem:spelling.cmatrix-print-fn+1/test]
    // [spec:foma:sem:fomalib.cmatrix-print-fn+1/test]
    #[test]
    fn cmatrix_print_aligns_diagonal_to_column_width() {
        // sigma has a 2-char symbol "ab" (column width 3). The diagonal star
        // must fill that width ("  *"); C emitted a bare "*", under-filling the
        // column and misaligning the row.
        let mut net = parse_sorted("\"ab\" | c");
        cmatrix_init(&mut net);
        let mut buf: Vec<u8> = Vec::new();
        cmatrix_print_to(&net, &mut buf);
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("  *"), "diagonal not width-padded:\n{}", out);
    }
}
