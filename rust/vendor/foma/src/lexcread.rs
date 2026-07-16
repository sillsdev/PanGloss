//! foma/lexcread.c — literal (bug-for-bug) Wave-2 port per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/lexcread.md
//! (per-file ids) and docs/spec/port/foma/lexc.md (lexc.h header ids).
//!
//! The lexc compiler builds a raw `struct states` graph (shared, cyclic,
//! heavily aliased through statelist/trans/merge_with/lexstate pointers),
//! then converts it to a struct fsm. Safe Rust cannot hold the C's raw
//! pointer graph, so — exactly as the fsm line table becomes index walks in
//! types.rs — every C pointer becomes an index into a `Vec` arena, and NULL
//! becomes `Option<usize>` / `None`. All file-static globals plus the arenas
//! live in a single owned `LexcCompiler` context. Wave 4 removed the
//! thread_local: `fsm_lexc_parse_string`/`_file` create one `LexcCompiler` per
//! call and thread it through the helpers, so the compiler is now reentrant.
//! The C names are kept on the fields. The former lexc.h public helpers keep
//! their C names and take `&mut LexcCompiler` (the caller-owned-context
//! pattern), so the whole compile threads one borrow.

use crate::constructions::{add_fsm_arc, fsm_update_flags};
use crate::define::add_defined;
use crate::determinize::fsm_determinize;
use crate::io::file_to_mem;
use crate::minimize::fsm_minimize;
use crate::options::FomaOptions;
use crate::regex::fsm_parse_regex;
use crate::sigma::{
    sigma_add, sigma_add_special, sigma_cleanup, sigma_create, sigma_find, sigma_find_number,
    sigma_max, sigma_sort,
};
use crate::structures::{fsm_create, fsm_empty_set};
use crate::topsort::fsm_topsort;
use crate::types::{DefinedNetworks, EPSILON, Fsm, FsmState, IDENTITY, Sigma, UNK, UNKNOWN};

const SIGMA_HASH_TABLESIZE: usize = 3079;

const WORD_ENTRY: i32 = 1;
const REGEX_ENTRY: i32 = 2;

/* ------------------------------------------------------------------ */
/* File-local struct types (declared inside lexcread.c)               */
/* ------------------------------------------------------------------ */

// [spec:foma:def:lexcread.multichar-symbols]
// C: struct multichar_symbols { char *symbol; short int sigma_number;
// struct multichar_symbols *next; }. `next` is an arena index (None ↔ NULL).
#[derive(Debug, Clone)]
struct MulticharSymbols {
    symbol: Option<String>,
    sigma_number: i16,
    next: Option<usize>,
}

// [spec:foma:def:lexcread.lexstates]
// C: struct lexstates — separate list of LEXICON states. `state` is a state
// arena index; `next` a lexstates arena index (None ↔ NULL).
#[derive(Debug, Clone)]
struct Lexstates {
    name: Option<String>,
    state: usize,
    next: Option<usize>,
    targeted: u8,
    has_outgoing: u8,
}

// [spec:foma:def:lexcread.states.trans]
// C: struct trans { short int in; short int out; struct states *target;
// struct trans *next; }. `target` is a state arena index; `next` a trans
// arena index (None ↔ NULL).
#[derive(Debug, Clone)]
struct Trans {
    r#in: i16,
    out: i16,
    target: usize,
    next: Option<usize>,
}

// [spec:foma:def:lexcread.states]
// C: struct states { struct trans *trans; struct lexstates *lexstate;
// int number; unsigned int hashval; unsigned char mergeable;
// unsigned short int distance; struct states *merge_with; }.
// `trans` and `lexstate` are Option<arena index>; `merge_with` a state arena
// index (initialised to self).
#[derive(Debug, Clone)]
struct States {
    trans: Option<usize>,
    lexstate: Option<usize>,
    number: i32,
    hashval: u32,
    mergeable: u8,
    distance: u16,
    merge_with: usize,
}

// [spec:foma:def:lexcread.statelist]
// C: struct statelist { struct states *state; struct statelist *next;
// char start; char final; }. `state` is a state arena index; `next` a
// statelist arena index (None ↔ NULL).
#[derive(Debug, Clone)]
struct Statelist {
    state: usize,
    next: Option<usize>,
    start: i8,
    r#final: i8,
}

// [spec:foma:def:lexcread.lexc-hashtable]
// C: struct lexc_hashtable { char *symbol; struct lexc_hashtable *next;
// int sigma_number; }. Hash for looking up symbols in sigma quickly. The
// 3079 bucket heads live in a Vec; collision chains are owned Box nodes.
#[derive(Debug, Clone)]
struct LexcHashtable {
    symbol: Option<String>,
    next: Option<Box<LexcHashtable>>,
    sigma_number: i32,
}

/* C: static unsigned int primes[26] = { ... }; */
static PRIMES: [u32; 26] = [
    61, 127, 251, 509, 1021, 2039, 4093, 8191, 16381, 32749, 65521, 131071, 262139, 524287,
    1048573, 2097143, 4194301, 8388593, 16777213, 33554393, 67108859, 134217689, 268435399,
    536870909, 1073741789, 2147483647,
];

/* ------------------------------------------------------------------ */
/* The lexc compiler context (Wave 4: owned, reentrant — was a         */
/* thread_local; now created per fsm_lexc_parse_string/_file call)      */
/* ------------------------------------------------------------------ */

/// Holds every lexcread.c file-static (the C names are kept on the fields)
/// plus the arenas that back the C pointer graph. Pointer fields are arena
/// indices; NULL is `None` / an empty arena.
struct LexcCompiler {
    /* the compiling session's options (C read the g_* globals directly) */
    opts: FomaOptions,

    /* arenas backing the C `struct *` graph (never reclaimed mid-compile;
    the C frees are no-ops here — see the DEVIATION notes at each free site) */
    state_arena: Vec<States>,
    trans_arena: Vec<Trans>,
    statelist_arena: Vec<Statelist>,
    lexstates_arena: Vec<Lexstates>,
    mc_arena: Vec<MulticharSymbols>,

    /* C: static struct statelist *statelist / *mc / *lexstates — list heads */
    statelist: Option<usize>,
    mc: Option<usize>,
    lexstates: Option<usize>,

    /* C: static struct sigma *lexsigma */
    lexsigma: Vec<Sigma>,
    /* C: static struct lexc_hashtable *hashtable — 3079 calloc'd bucket heads */
    hashtable: Vec<LexcHashtable>,
    /* C: static struct fsm *current_regex_network */
    current_regex_network: Option<Box<Fsm>>,

    /* C: static int cwordin[1000], cwordout[1000], medcwordin[2000],
    medcwordout[2000]. Wave 4: growable Vecs (the C fixed buffers overflowed —
    and corrupted adjacent memory — on an entry side of 1000+ tokens). Reads
    stay within the -1-terminated prefix; writes past the current length grow
    the Vec via vset(). */
    cwordin: Vec<i32>,
    cwordout: Vec<i32>,
    medcwordin: Vec<i32>,
    medcwordout: Vec<i32>,

    /* C scalar file-statics */
    carity: i32,
    lexc_statecount: i32,
    maxlen: i32,
    hasfinal: i32,
    current_entry: i32,
    net_has_unknown: i32,

    /* C: static struct lexstates *clexicon, *ctarget — lexstates indices */
    clexicon: Option<usize>,
    ctarget: Option<usize>,
}

impl LexcCompiler {
    fn new_empty() -> LexcCompiler {
        LexcCompiler {
            opts: FomaOptions::default(),
            state_arena: Vec::new(),
            trans_arena: Vec::new(),
            statelist_arena: Vec::new(),
            lexstates_arena: Vec::new(),
            mc_arena: Vec::new(),
            statelist: None,
            mc: None,
            lexstates: None,
            lexsigma: Vec::new(),
            hashtable: Vec::new(),
            current_regex_network: None,
            cwordin: Vec::new(),
            cwordout: Vec::new(),
            medcwordin: Vec::new(),
            medcwordout: Vec::new(),
            carity: 0,
            lexc_statecount: 0,
            maxlen: 0,
            hasfinal: 0,
            current_entry: 0,
            net_has_unknown: 0,
            clexicon: None,
            ctarget: None,
        }
    }
}

/// Write `val` at `idx` in a token buffer, growing it with 0 fill so `idx` is
/// in bounds (the C wrote into fixed 1000/2000-int arrays; here the buffers
/// grow instead of overflowing).
fn vset(v: &mut Vec<i32>, idx: usize, val: i32) {
    if idx >= v.len() {
        v.resize(idx + 1, 0);
    }
    v[idx] = val;
}

/* ------------------------------------------------------------------ */
/* Hashing                                                             */
/* ------------------------------------------------------------------ */

// [spec:foma:def:lexcread.lexc-suffix-hash-fn]
// [spec:foma:sem:lexcread.lexc-suffix-hash-fn]
fn lexc_suffix_hash(lx: &LexcCompiler, offset: i32) -> u32 {
    let mut h: u32 = 0;
    let mut g: u32;
    /* Read suffixes in cwordin[] and cwordout[] and return a hash value */
    let mut p = offset as usize;
    while lx.cwordin[p] != -1 {
        h = (h << 4).wrapping_add((lx.cwordin[p] | (lx.cwordout[p] << 8)) as u32);
        g = h & 0xf000_0000;
        if g != 0 {
            h ^= g >> 24;
            h ^= g;
        }
        p += 1;
    }
    /* No tablemod here, we decide on the table size later */
    h
}

// [spec:foma:def:lexcread.lexc-symbol-hash-fn]
// [spec:foma:sem:lexcread.lexc-symbol-hash-fn]
fn lexc_symbol_hash(s: &str) -> u32 {
    let mut hash: u32 = 5381;
    /* while ((c = *s++)) — signed char sign-extension into int c, per the
    conventions (bytes >= 0x80 add a wrapped large value) */
    for &b in s.as_bytes() {
        let c = b as i8 as i32;
        hash = ((hash << 5).wrapping_add(hash)).wrapping_add(c as u32);
    }
    hash % SIGMA_HASH_TABLESIZE as u32
}

// [spec:foma:def:lexcread.lexc-find-sigma-hash-fn]
// [spec:foma:sem:lexcread.lexc-find-sigma-hash-fn+1]
fn lexc_find_sigma_hash(lx: &LexcCompiler, symbol: &str) -> Option<i32> {
    let ptr = lexc_symbol_hash(symbol) as usize;

    lx.hashtable[ptr].symbol.as_ref()?;
    /* for (h = head; h != NULL; h = h->next) */
    if lx.hashtable[ptr].symbol.as_deref() == Some(symbol) {
        return Some(lx.hashtable[ptr].sigma_number);
    }
    let mut h = lx.hashtable[ptr].next.as_deref();
    while let Some(node) = h {
        if node.symbol.as_deref() == Some(symbol) {
            return Some(node.sigma_number);
        }
        h = node.next.as_deref();
    }
    None
}

// [spec:foma:def:lexcread.lexc-add-sigma-hash-fn]
// [spec:foma:sem:lexcread.lexc-add-sigma-hash-fn]
fn lexc_add_sigma_hash(lx: &mut LexcCompiler, symbol: &str, number: i32) {
    let ptr = lexc_symbol_hash(symbol) as usize;

    if lx.net_has_unknown == 1 {
        lexc_update_unknowns(lx, number);
    }

    if lx.hashtable[ptr].symbol.is_none() {
        lx.hashtable[ptr].symbol = Some(symbol.to_string());
        lx.hashtable[ptr].sigma_number = number;
        return;
    }
    /* for (h = head; h->next != NULL; h = h->next) {} — walk to the tail */
    let mut tail_next = &mut lx.hashtable[ptr].next;
    while let Some(node) = tail_next {
        tail_next = &mut node.next;
    }
    *tail_next = Some(Box::new(LexcHashtable {
        symbol: Some(symbol.to_string()),
        sigma_number: number,
        next: None,
    }));
}

// [spec:foma:def:lexcread.lexc-init-fn]
// [spec:foma:sem:lexcread.lexc-init-fn]
// [spec:foma:def:lexc.lexc-init-fn]
// [spec:foma:sem:lexc.lexc-init-fn]
fn lexc_init(lx: &mut LexcCompiler) {
    lx.lexsigma = sigma_create();
    lx.mc = None;
    lx.lexstates = None;
    lx.clexicon = None;
    lx.ctarget = None;
    lx.statelist = None;
    lx.lexc_statecount = 0;
    lx.net_has_unknown = 0;
    lexc_clear_current_word(lx);
    /* calloc(SIGMA_HASH_TABLESIZE) then the loop below sets each bucket
    head to {symbol=NULL, sigma_number=-1, next=NULL} */
    lx.hashtable = vec![
        LexcHashtable {
            symbol: None,
            next: None,
            sigma_number: -1,
        };
        SIGMA_HASH_TABLESIZE
    ];

    lx.maxlen = 0;

    /* Does not free structures from a previous run (that is lexc_cleanup's
    job); calling lexc_init twice without an intervening cleanup leaks the old
    tables — reproduced by not clearing the arenas here (they are cleared in
    lexc_cleanup). current_regex_network is not reset. */
}

// [spec:foma:def:lexcread.lexc-clear-current-word-fn]
// [spec:foma:sem:lexcread.lexc-clear-current-word-fn]
// [spec:foma:def:lexc.lexc-clear-current-word-fn]
// [spec:foma:sem:lexc.lexc-clear-current-word-fn]
fn lexc_clear_current_word(lx: &mut LexcCompiler) {
    /* cwordin/cwordout = [EPSILON(0), -1] (the -1-terminated empty word) */
    lx.cwordin = vec![0, -1];
    lx.cwordout = vec![0, -1];
    lx.current_entry = WORD_ENTRY;
}

// [spec:foma:def:lexcread.lexc-add-state-fn]
// [spec:foma:sem:lexcread.lexc-add-state-fn]
fn lexc_add_state(lx: &mut LexcCompiler, s: usize) {
    /* sl = malloc(struct statelist); sl->state = s; s->number = -1;
    sl->next = statelist; sl->start = 0; sl->final = 0; statelist = sl; */
    let slidx = lx.statelist_arena.len();
    lx.statelist_arena.push(Statelist {
        state: s,
        next: lx.statelist,
        start: 0,
        r#final: 0,
    });
    lx.state_arena[s].number = -1;
    lx.statelist = Some(slidx);
    lx.lexc_statecount += 1;
}

/* Go through the net built so far and add new transitions for @ */
/* to reflect the new symbols we now have in sigma */

// [spec:foma:def:lexcread.lexc-update-unknowns-fn]
// [spec:foma:sem:lexcread.lexc-update-unknowns-fn]
fn lexc_update_unknowns(lx: &mut LexcCompiler, sigma_number: i32) {
    /* for (s = statelist; s != NULL; s = s->next) */
    let mut s = lx.statelist;
    while let Some(sidx) = s {
        let stateidx = lx.statelist_arena[sidx].state;
        if lx.state_arena[stateidx].mergeable != 2 {
            /* for (t = s->state->trans; t != NULL; t = t->next) */
            let mut t = lx.state_arena[stateidx].trans;
            while let Some(tidx) = t {
                if lx.trans_arena[tidx].r#in as i32 == IDENTITY
                    || lx.trans_arena[tidx].out as i32 == IDENTITY
                {
                    let target = lx.trans_arena[tidx].target;
                    let tnext = lx.trans_arena[tidx].next;
                    /* newtrans->next = t->next; t->next = newtrans */
                    let newidx = lx.trans_arena.len();
                    lx.trans_arena.push(Trans {
                        r#in: sigma_number as i16,
                        out: sigma_number as i16,
                        target,
                        next: tnext,
                    });
                    lx.trans_arena[tidx].next = Some(newidx);
                }
                /* t = t->next: after an insertion this visits the new arc next
                (labels are the new symbol, not IDENTITY, so no recursion) */
                t = lx.trans_arena[tidx].next;
            }
        }
        s = lx.statelist_arena[sidx].next;
    }
}

// [spec:foma:def:lexcread.lexc-add-network-fn]
// [spec:foma:sem:lexcread.lexc-add-network-fn]
fn lexc_add_network(lx: &mut LexcCompiler) {
    let mut unknown_symbols = 0;
    let mut first_new_sigma = 0;
    let sourcestate = lx.lexstates_arena[lx
        .clexicon
        .expect("current lexicon set during a lexicon parse")]
    .state;
    let deststate = lx.lexstates_arena[lx
        .ctarget
        .expect("current target set during an entry parse")]
    .state;

    /* net = current_regex_network; taken out so the &mut lx calls below do not
    conflict with reading net->states / net->sigma. Put back at the end (C
    never frees it and leaves current_regex_network pointing at the mutated
    net). */
    let mut net = lx
        .current_regex_network
        .take()
        .expect("regex network present for this entry");

    /* sigreplace = calloc(sigma_max(net->sigma)+1, sizeof(int)) */
    let mut sigreplace: Vec<i32> = vec![0; (sigma_max(&net.sigma) + 1) as usize];

    /* for (sigma = net->sigma; sigma != NULL && sigma->number != -1; ...) */
    for idx in 0..net.sigma.len() {
        let s_number = net.sigma[idx].number;
        let sym = net.sigma[idx].symbol.clone();
        match lexc_find_sigma_hash(lx, &sym) {
            None => {
                /* Add to existing lexc sigma */
                let signumber = sigma_add(&sym, &mut lx.lexsigma);
                first_new_sigma = if first_new_sigma > 0 {
                    first_new_sigma
                } else {
                    signumber
                };
                lexc_add_sigma_hash(lx, &sym, signumber);
                sigreplace[s_number as usize] = signumber;
            }
            Some(signumber) => {
                /* We already have it, add to conversion table */
                sigreplace[s_number as usize] = signumber;
            }
        }
    }

    /* Renum arcs — net->states is mutated in place */
    let mut maxstate = 0i32;
    {
        let fsm = &mut net.states;
        let mut i = 0usize;
        while fsm[i].state_no != -1 {
            if fsm[i].r#in != -1 {
                fsm[i].r#in = sigreplace[fsm[i].r#in as usize] as i16;
            }
            if fsm[i].out != -1 {
                fsm[i].out = sigreplace[fsm[i].out as usize] as i16;
            }
            if fsm[i].state_no > maxstate {
                maxstate = fsm[i].state_no;
            }
            if fsm[i].r#in as i32 == IDENTITY
                || fsm[i].r#in as i32 == UNKNOWN
                || fsm[i].out as i32 == UNKNOWN
            {
                unknown_symbols = 1;
            }
            i += 1;
        }
    }

    /* unk: 0-terminated list of concrete lexsigma symbols absent from net */
    let mut unk: Vec<i32> = Vec::new();
    if unknown_symbols == 1 {
        unk = vec![0; (sigma_max(&lx.lexsigma) + 2) as usize];
        let mut i = 0usize;
        for s in &lx.lexsigma {
            if s.number > 2 && sigma_find(&s.symbol, &net.sigma).is_none() {
                unk[i] = s.number;
                i += 1;
            }
        }
    }

    /* slist[state_no] -> fresh state index; finals[state_no] -> final flag */
    let mut slist: Vec<usize> = vec![0; (maxstate + 1) as usize];
    let mut finals: Vec<i32> = vec![0; (maxstate + 1) as usize];

    for slot in slist.iter_mut().take(maxstate as usize + 1) {
        let newidx = lx.state_arena.len();
        lx.state_arena.push(States {
            trans: None,
            lexstate: None,
            number: -1,
            hashval: u32::MAX, /* C: newstate->hashval = -1 (unsigned int) */
            mergeable: 0,
            distance: 0,
            merge_with: 0, /* set to self below */
        });
        lx.state_arena[newidx].merge_with = newidx;
        *slot = newidx;
        /* Prepend a statelist cell directly (NOT via lexc_add_state, so
        lexc_statecount is not bumped — harmless; recomputed later) */
        let slidx = lx.statelist_arena.len();
        lx.statelist_arena.push(Statelist {
            state: newidx,
            next: lx.statelist,
            start: 0,
            r#final: 0,
        });
        lx.statelist = Some(slidx);
    }

    /* Add an EPSILON transition from sourcestate to state 0 */
    {
        let newtrans = lx.trans_arena.len();
        lx.trans_arena.push(Trans {
            r#in: EPSILON as i16,
            out: EPSILON as i16,
            target: slist[0],
            next: lx.state_arena[sourcestate].trans,
        });
        lx.state_arena[sourcestate].trans = Some(newtrans);
    }

    {
        let mut i = 0usize;
        while net.states[i].state_no != -1 {
            if net.states[i].target != -1 {
                let newstate = slist[net.states[i].state_no as usize];
                let newtrans = lx.trans_arena.len();
                lx.trans_arena.push(Trans {
                    r#in: net.states[i].r#in,
                    out: net.states[i].out,
                    target: slist[net.states[i].target as usize],
                    next: lx.state_arena[newstate].trans,
                });
                lx.state_arena[newstate].trans = Some(newtrans);
                /* Add new symbols for @:@ transitions */
                /* TODO: make this work for ?: or :? trans as well */
                if unknown_symbols == 1
                    && (net.states[i].r#in as i32 == IDENTITY
                        || net.states[i].out as i32 == IDENTITY)
                {
                    let mut j = 0usize;
                    while unk[j] != 0 {
                        let nt = lx.trans_arena.len();
                        lx.trans_arena.push(Trans {
                            r#in: unk[j] as i16,
                            out: unk[j] as i16,
                            target: slist[net.states[i].target as usize],
                            next: lx.state_arena[newstate].trans,
                        });
                        lx.state_arena[newstate].trans = Some(nt);
                        j += 1;
                    }
                }
            }
            finals[net.states[i].state_no as usize] = net.states[i].final_state as i32;
            i += 1;
        }
    }

    /* Add an EPSILON transition from all final states to deststate */
    for i in 0..=(maxstate as usize) {
        if finals[i] == 1 {
            let newstate = slist[i];
            let nt = lx.trans_arena.len();
            lx.trans_arena.push(Trans {
                r#in: EPSILON as i16,
                out: EPSILON as i16,
                target: deststate,
                next: lx.state_arena[newstate].trans,
            });
            lx.state_arena[newstate].trans = Some(nt);
        }
    }

    if unknown_symbols == 1 {
        /* free(unk) — no-op (local Vec) */
        lx.net_has_unknown = 1;
    }
    /* free(slist); free(finals) — no-op. sigreplace is never freed in C
    (leak); the local Vecs drop here. */
    lx.current_regex_network = Some(net);
    let _ = first_new_sigma; /* recorded but never read (dead code in C) */
}

// [spec:foma:def:lexcread.lexc-set-network-fn]
// [spec:foma:sem:lexcread.lexc-set-network-fn]
// [spec:foma:def:lexc.lexc-set-network-fn]
// [spec:foma:sem:lexc.lexc-set-network-fn]
fn lexc_set_network(lx: &mut LexcCompiler, net: Box<Fsm>) {
    lx.current_regex_network = Some(net);
    lx.current_entry = REGEX_ENTRY;
}

// [spec:foma:def:lexcread.lexc-set-current-lexicon-fn]
// [spec:foma:sem:lexcread.lexc-set-current-lexicon-fn]
// [spec:foma:def:lexc.lexc-set-current-lexicon-fn]
// [spec:foma:sem:lexc.lexc-set-current-lexicon-fn]
fn lexc_set_current_lexicon(lx: &mut LexcCompiler, name: &str, which: i32) {
    /* Sets the global lexicon variable to point to a new lexicon */
    /* which == 0 indicates source, which == 1 indicates target */
    let mut l = lx.lexstates;
    while let Some(lidx) = l {
        if lx.lexstates_arena[lidx].name.as_deref() == Some(name) {
            if which == 0 {
                lx.lexstates_arena[lidx].has_outgoing = 1;
                lx.clexicon = Some(lidx);
            } else {
                lx.ctarget = Some(lidx);
            }
            return;
        }
        l = lx.lexstates_arena[lidx].next;
    }
    let lidx = lx.lexstates_arena.len();
    lx.lexstates_arena.push(Lexstates {
        name: Some(name.to_string()),
        /* state assigned below after lexc_add_state */
        state: 0,
        next: lx.lexstates,
        has_outgoing: 0,
        targeted: 0,
    });
    lx.lexstates = Some(lidx);
    let newidx = lx.state_arena.len();
    lx.state_arena.push(States {
        trans: None,
        lexstate: None,
        number: -1,
        /* C leaves hashval and distance uninitialised — never read while
        mergeable == 0; zeroed here */
        hashval: 0,
        mergeable: 0,
        distance: 0,
        merge_with: 0,
    });
    lexc_add_state(lx, newidx);
    lx.state_arena[newidx].lexstate = Some(lidx);
    lx.state_arena[newidx].trans = None;
    lx.state_arena[newidx].mergeable = 0;
    lx.state_arena[newidx].merge_with = newidx;
    lx.lexstates_arena[lidx].state = newidx;
    if which == 0 {
        lx.clexicon = Some(lidx);
        lx.lexstates_arena[lidx].has_outgoing = 1;
    } else {
        lx.ctarget = Some(lidx);
    }
}

/* Read a string and fill cwordin, cwordout arrays */
/* with the sigma numbers of the current word, -1 terminated */

// [spec:foma:def:lexcread.lexc-set-current-word-fn]
// [spec:foma:sem:lexcread.lexc-set-current-word-fn]
// [spec:foma:def:lexc.lexc-set-current-word-fn]
// [spec:foma:sem:lexc.lexc-set-current-word-fn]
fn lexc_set_current_word(lx: &mut LexcCompiler, upper: &str, lower: Option<&str>) {
    /* nfst-lexc already de-escaped %X and split upper:lower pairs, so the two
    sides arrive as plain &str. The tokenizer decodes the remaining conventions
    (@ZERO@ -> the literal "0" symbol, a bare '0' -> alignment EPSILON). */
    lx.carity = if lower.is_some() { 2 } else { 1 };

    /* lexc_string_to_tokens(upper, cwordin) — cwordin moved out so it can be
    a &mut param disjoint from &mut lx, then moved back */
    let mut intarr = std::mem::take(&mut lx.cwordin);
    lexc_string_to_tokens(lx, upper, &mut intarr);
    lx.cwordin = intarr;

    if let Some(lower) = lower {
        let mut intarr = std::mem::take(&mut lx.cwordout);
        lexc_string_to_tokens(lx, lower, &mut intarr);
        lx.cwordout = intarr;
        if lx.opts.lexc_align {
            lexc_medpad(lx);
        } else {
            lexc_pad(lx);
        }
    } else {
        let mut i = 0usize;
        while lx.cwordin[i] != -1 {
            let v = lx.cwordin[i];
            vset(&mut lx.cwordout, i, v);
            i += 1;
        }
        vset(&mut lx.cwordout, i, -1);
    }
    lx.current_entry = WORD_ENTRY;
}

const LEV_DOWN: i32 = 0;
const LEV_LEFT: i32 = 1;
const LEV_DIAG: i32 = 2;

// [spec:foma:def:lexcread.lexc-medpad-fn]
// [spec:foma:sem:lexcread.lexc-medpad-fn]
fn lexc_medpad(lx: &mut LexcCompiler) {
    if lx.cwordin[0] == -1 && lx.cwordout[0] == -1 {
        vset(&mut lx.cwordin, 0, EPSILON);
        vset(&mut lx.cwordout, 0, EPSILON);
        vset(&mut lx.cwordin, 1, -1);
        vset(&mut lx.cwordout, 1, -1);
        return;
    }

    /* compact cwordin, deleting every EPSILON token */
    {
        let mut i = 0usize;
        let mut j = 0usize;
        while lx.cwordin[i] != -1 {
            if lx.cwordin[i] == EPSILON {
                i += 1;
                continue;
            }
            lx.cwordin[j] = lx.cwordin[i];
            j += 1;
            i += 1;
        }
        lx.cwordin[j] = -1;
    }
    /* compact cwordout */
    {
        let mut i = 0usize;
        let mut j = 0usize;
        while lx.cwordout[i] != -1 {
            if lx.cwordout[i] == EPSILON {
                i += 1;
                continue;
            }
            lx.cwordout[j] = lx.cwordout[i];
            j += 1;
            i += 1;
        }
        lx.cwordout[j] = -1;
    }

    let mut s1len = 0usize;
    while lx.cwordin[s1len] != -1 {
        s1len += 1;
    }
    let mut s2len = 0usize;
    while lx.cwordout[s2len] != -1 {
        s2len += 1;
    }

    /* Grow the word/scratch buffers so the backtrace and copy-back writes stay
    in bounds (the C wrote into fixed 1000/2000-int arrays; an alignment can be
    up to s1len+s2len pairs long). 0-fill past the -1 is never read. */
    let size = s1len + s2len + 2;
    if lx.cwordin.len() < size {
        lx.cwordin.resize(size, 0);
    }
    if lx.cwordout.len() < size {
        lx.cwordout.resize(size, 0);
    }
    if lx.medcwordin.len() < size {
        lx.medcwordin.resize(size, 0);
    }
    if lx.medcwordout.len() < size {
        lx.medcwordout.resize(size, 0);
    }

    /* calloc (s1len+2) x (s2len+2) int matrices */
    let mut matrix: Vec<Vec<i32>> = vec![vec![0i32; s2len + 2]; s1len + 2];
    let mut dirmatrix: Vec<Vec<i32>> = vec![vec![0i32; s2len + 2]; s1len + 2];

    matrix[0][0] = 0;
    dirmatrix[0][0] = 0;
    for x in 1..=s1len {
        matrix[x][0] = matrix[x - 1][0] + 1;
        dirmatrix[x][0] = LEV_LEFT;
    }
    for y in 1..=s2len {
        matrix[0][y] = matrix[0][y - 1] + 1;
        dirmatrix[0][y] = LEV_DOWN;
    }
    for x in 1..=s1len {
        for y in 1..=s2len {
            let diag = matrix[x - 1][y - 1]
                + if lx.cwordin[x - 1] == lx.cwordout[y - 1] {
                    0
                } else {
                    100
                };
            let down = matrix[x][y - 1] + 1;
            let left = matrix[x - 1][y] + 1;
            if diag <= left && diag <= down {
                matrix[x][y] = diag;
                dirmatrix[x][y] = LEV_DIAG;
            } else if left <= diag && left <= down {
                matrix[x][y] = left;
                dirmatrix[x][y] = LEV_LEFT;
            } else {
                matrix[x][y] = down;
                dirmatrix[x][y] = LEV_DOWN;
            }
        }
    }

    let mut x = s1len;
    let mut y = s2len;
    let mut i = 0usize;
    while x > 0 || y > 0 {
        let dir = dirmatrix[x][y];
        if dir == LEV_DIAG {
            lx.medcwordin[i] = lx.cwordin[x - 1];
            lx.medcwordout[i] = lx.cwordout[y - 1];
            x -= 1;
            y -= 1;
        } else if dir == LEV_DOWN {
            lx.medcwordin[i] = EPSILON;
            lx.medcwordout[i] = lx.cwordout[y - 1];
            y -= 1;
        } else {
            lx.medcwordin[i] = lx.cwordin[x - 1];
            lx.medcwordout[i] = EPSILON;
            x -= 1;
        }
        i += 1;
    }
    /* for (j = 0, i -= 1; i >= 0; j++, i--) — copy the reversed scratch back */
    let mut j = 0usize;
    let mut k: isize = i as isize - 1;
    while k >= 0 {
        lx.cwordin[j] = lx.medcwordin[k as usize];
        lx.cwordout[j] = lx.medcwordout[k as usize];
        j += 1;
        k -= 1;
    }
    lx.cwordin[j] = -1;
    lx.cwordout[j] = -1;
    /* free matrices — Vecs drop here */
}

// [spec:foma:def:lexcread.lexc-pad-fn]
// [spec:foma:sem:lexcread.lexc-pad-fn]
fn lexc_pad(lx: &mut LexcCompiler) {
    /* Pad the shorter of current in, out words with EPSILON */
    /* Grow both buffers to a common working length (0-fill past the -1 is only
    read as a non-terminator during padding, never after) so every write below
    stays in bounds — the C wrote into fixed 1000-int arrays. */
    let need = lx.cwordin.len().max(lx.cwordout.len()) + 2;
    if lx.cwordin.len() < need {
        lx.cwordin.resize(need, 0);
    }
    if lx.cwordout.len() < need {
        lx.cwordout.resize(need, 0);
    }
    if lx.cwordin[0] == -1 && lx.cwordout[0] == -1 {
        lx.cwordin[0] = EPSILON;
        lx.cwordout[0] = EPSILON;
        lx.cwordin[1] = -1;
        lx.cwordout[1] = -1;
        return;
    }

    let mut i = 0usize;
    let mut pad = 0;
    loop {
        if pad == 1 && lx.cwordout[i] == -1 {
            lx.cwordin[i] = -1;
            break;
        }
        if pad == 2 && lx.cwordin[i] == -1 {
            lx.cwordout[i] = -1;
            break;
        }
        if lx.cwordin[i] == -1 && lx.cwordout[i] != -1 {
            pad = 1; /* Pad upper */
        } else if lx.cwordin[i] != -1 && lx.cwordout[i] == -1 {
            pad = 2; /* Pad lower */
        }
        if pad == 1 {
            lx.cwordin[i] = EPSILON;
        }
        if pad == 2 {
            lx.cwordout[i] = EPSILON;
        }
        if pad == 0 && lx.cwordin[i] == -1 {
            break;
        }
        i += 1;
    }
}

// [spec:foma:def:lexcread.lexc-string-to-tokens-fn]
// [spec:foma:sem:lexcread.lexc-string-to-tokens-fn+1]
fn lexc_string_to_tokens(lx: &mut LexcCompiler, string: &str, intarr: &mut Vec<i32>) {
    let mut pos = 0usize;
    let mut rest = string;
    while let Some(c) = rest.chars().next() {
        // nfst-lexc encodes an escaped literal zero (%0) as the marker @ZERO@,
        // which denotes the literal "0" symbol.
        if let Some(r) = rest.strip_prefix("@ZERO@") {
            vset(intarr, pos, intern_symbol(lx, "0"));
            pos += 1;
            rest = r;
            continue;
        }
        // A bare '0' is the alignment EPSILON.
        if c == '0' {
            vset(intarr, pos, EPSILON);
            pos += 1;
            rest = &rest[c.len_utf8()..];
            continue;
        }
        // The longest matching multichar symbol (the chain is kept longest-first,
        // so the first prefix hit is the longest).
        if let Some(m) = first_mc_prefix(lx, rest) {
            vset(intarr, pos, lx.mc_arena[m].sigma_number as i32);
            pos += 1;
            let mclen = lx.mc_arena[m]
                .symbol
                .as_deref()
                .expect("multichar arena entry has a symbol")
                .len();
            rest = &rest[mclen..];
            continue;
        }
        // A single character.
        let sym = &rest[..c.len_utf8()];
        let n = intern_symbol(lx, sym);
        vset(intarr, pos, n);
        pos += 1;
        rest = &rest[c.len_utf8()..];
    }
    vset(intarr, pos, -1);
}

/// Look `sym` up in the lex sigma hash, adding it (and registering the number)
/// on a miss; returns the sigma number. Mirrors the C find -> sigma_add ->
/// add_sigma_hash order exactly, so symbol numbering is preserved.
fn intern_symbol(lx: &mut LexcCompiler, sym: &str) -> i32 {
    if let Some(n) = lexc_find_sigma_hash(lx, sym) {
        return n;
    }
    let n = sigma_add(sym, &mut lx.lexsigma);
    lexc_add_sigma_hash(lx, sym, n);
    n
}

/// The first (hence longest, since the chain is length-sorted) multichar symbol
/// that is a prefix of `rest`.
fn first_mc_prefix(lx: &LexcCompiler, rest: &str) -> Option<usize> {
    let mut m = lx.mc;
    while let Some(i) = m {
        if rest.starts_with(
            lx.mc_arena[i]
                .symbol
                .as_deref()
                .expect("multichar arena entry has a symbol"),
        ) {
            return Some(i);
        }
        m = lx.mc_arena[i].next;
    }
    None
}

/* Add MC to front of chain */
/* In decreasing order of length */

// [spec:foma:def:lexcread.lexc-add-mc-fn]
// [spec:foma:sem:lexcread.lexc-add-mc-fn]
// [spec:foma:def:lexc.lexc-add-mc-fn]
// [spec:foma:sem:lexc.lexc-add-mc-fn]
fn lexc_add_mc(lx: &mut LexcCompiler, raw: &str) {
    // nfst-lexc already de-escaped %X; decode the remaining conventions the way
    // the C mode-0 de-escape did: @ZERO@ -> the literal "0"; a bare '0' dropped.
    let symbol = normalize_mc_symbol(raw);
    if !lexc_find_mc(lx, &symbol) {
        let len = symbol.chars().count();
        let mut mcprev: Option<usize> = None;
        /* for (mcs = mc; mcs != NULL && utf8strlen(mcs->symbol) > len; ...) */
        let mut mcs = lx.mc;
        while let Some(m) = mcs {
            if lx.mc_arena[m]
                .symbol
                .as_deref()
                .expect("multichar arena entry has a symbol")
                .chars()
                .count()
                <= len
            {
                break;
            }
            mcprev = Some(m);
            mcs = lx.mc_arena[m].next;
        }
        let mcnew = lx.mc_arena.len();
        lx.mc_arena.push(MulticharSymbols {
            symbol: Some(symbol.clone()),
            sigma_number: 0, /* set below */
            next: mcs,
        });
        if lx.mc.is_none() || (mcs.is_some() && mcprev.is_none()) {
            lx.mc = Some(mcnew);
        }
        if let Some(p) = mcprev {
            lx.mc_arena[p].next = Some(mcnew);
        }

        let s = sigma_add(&symbol, &mut lx.lexsigma);
        lexc_add_sigma_hash(lx, &symbol, s);
        lx.mc_arena[mcnew].sigma_number = s as i16;
    }
}

/// Decode nfst-lexc's already-de-escaped multichar symbol: @ZERO@ -> "0"; a bare
/// '0' is dropped (the C mode-0 de-escape silently deleted an unescaped '0').
fn normalize_mc_symbol(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(c) = rest.chars().next() {
        if let Some(r) = rest.strip_prefix("@ZERO@") {
            out.push('0');
            rest = r;
        } else if c == '0' {
            rest = &rest[1..];
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

// [spec:foma:def:lexcread.lexc-find-mc-fn]
// [spec:foma:sem:lexcread.lexc-find-mc-fn+1]
// [spec:foma:def:lexc.lexc-find-mc-fn]
// [spec:foma:sem:lexc.lexc-find-mc-fn+1]
fn lexc_find_mc(lx: &LexcCompiler, symbol: &str) -> bool {
    /* Membership test over the multichar-symbol chain: true iff `symbol` is
    already registered (C returned 1/0). */
    let mut mcs = lx.mc;
    while let Some(m) = mcs {
        if lx.mc_arena[m].symbol.as_deref() == Some(symbol) {
            return true;
        }
        mcs = lx.mc_arena[m].next;
    }
    false
}

// [spec:foma:def:lexcread.lexc-find-lex-state-fn]
// [spec:foma:sem:lexcread.lexc-find-lex-state-fn]
// [spec:foma:def:lexc.lexc-find-lex-state-fn]
// [spec:foma:sem:lexc.lexc-find-lex-state-fn]
// Returns the lexicon's state (a private `struct states` — exposed here as its
// arena index, since this is dead API with no callers in the C tree).
#[allow(dead_code)] // dead API (no C callers); kept + test-pinned per the port
fn lexc_find_lex_state(lx: &LexcCompiler, name: &str) -> Option<usize> {
    let mut l = lx.lexstates;
    while let Some(lidx) = l {
        if lx.lexstates_arena[lidx].name.as_deref() == Some(name) {
            return Some(lx.lexstates_arena[lidx].state);
        }
        l = lx.lexstates_arena[lidx].next;
    }
    None
}

// [spec:foma:def:lexcread.lexc-add-word-fn]
// [spec:foma:sem:lexcread.lexc-add-word-fn]
// [spec:foma:def:lexc.lexc-add-word-fn]
// [spec:foma:sem:lexc.lexc-add-word-fn]
fn lexc_add_word(lx: &mut LexcCompiler) {
    /* Add a word from source state to destination state */
    if lx.current_entry == REGEX_ENTRY {
        lexc_add_network(lx);
        return;
    }

    /* find source, dest */
    let mut sourcestate = lx.lexstates_arena[lx
        .clexicon
        .expect("current lexicon set during a lexicon parse")]
    .state;
    let deststate = lx.lexstates_arena[lx
        .ctarget
        .expect("current target set during an entry parse")]
    .state;

    let mut li = 0usize;
    while lx.cwordin[li] != -1 {
        li += 1;
    }
    let len = li as i32;
    if len > lx.maxlen {
        lx.maxlen = len;
    }

    /* We follow the source state if the symbols are the same (merge prefixes) */
    let mut follow = 1;
    let mut i = 0usize;
    while lx.cwordin[i] != -1 {
        let mut followed = false;
        if follow == 1 {
            let mut trans = lx.state_arena[sourcestate].trans;
            while let Some(tidx) = trans {
                let t_in = lx.trans_arena[tidx].r#in as i32;
                let t_out = lx.trans_arena[tidx].out as i32;
                let t_target = lx.trans_arena[tidx].target;
                if t_in == lx.cwordin[i]
                    && t_out == lx.cwordout[i]
                    && lx.state_arena[t_target].lexstate.is_none()
                {
                    /* Can't follow if target needs to be lexstate */
                    if lx.cwordin[i + 1] == -1 && t_target != deststate {
                        trans = lx.trans_arena[tidx].next;
                        continue;
                    }
                    sourcestate = t_target;
                    lx.state_arena[sourcestate].mergeable = 0;
                    followed = true; /* goto breakout */
                    break;
                }
                trans = lx.trans_arena[tidx].next;
            }
        }
        if followed {
            i += 1;
            continue;
        }
        follow = 0;

        let target;
        if lx.cwordin[i + 1] == -1 {
            target = deststate;
        } else {
            let newstate = lx.state_arena.len();
            lx.state_arena.push(States {
                trans: None,
                lexstate: None,
                number: -1,
                hashval: 0,
                mergeable: 1,
                distance: 0,
                merge_with: 0,
            });
            lexc_add_state(lx, newstate);
            lx.state_arena[newstate].trans = None;
            lx.state_arena[newstate].lexstate = None;
            lx.state_arena[newstate].mergeable = 1;
            lx.state_arena[newstate].hashval = lexc_suffix_hash(lx, (i + 1) as i32);
            lx.state_arena[newstate].distance = (len - i as i32 - 1) as u16;
            lx.state_arena[newstate].merge_with = newstate;
            target = newstate;
        }
        let newtrans = lx.trans_arena.len();
        lx.trans_arena.push(Trans {
            r#in: lx.cwordin[i] as i16,
            out: lx.cwordout[i] as i16,
            target,
            next: lx.state_arena[sourcestate].trans,
        });
        lx.state_arena[sourcestate].trans = Some(newtrans);
        sourcestate = target;
        i += 1;
    }
}

// [spec:foma:def:lexcread.lexc-number-states-fn]
// [spec:foma:sem:lexcread.lexc-number-states-fn+1]
fn lexc_number_states(lx: &mut LexcCompiler) {
    let mut smax = 0i32;
    let mut n = 0i32;
    lx.hasfinal = 0;

    /* Wave 4 fix: smax = the total number of states, so "#" (numbered smax-1
    below) always gets the true last number. The C computed smax by counting
    only up to Root and stopping, which equals the total count only when Root
    was the first state created — otherwise "#" collided with a state numbered
    sequentially in the final pass, corrupting the number-indexed array in
    lexc_to_fsm. */
    {
        let mut s = lx.statelist;
        while let Some(sidx) = s {
            smax += 1;
            s = lx.statelist_arena[sidx].next;
        }
    }

    let mut hasroot = 0;
    {
        let mut s = lx.statelist;
        while let Some(sidx) = s {
            let state = lx.statelist_arena[sidx].state;
            let is_root = match lx.state_arena[state].lexstate {
                Some(lidx) => lx.lexstates_arena[lidx].name.as_deref() == Some("Root"),
                None => false,
            };
            if is_root {
                lx.state_arena[state].number = 0;
                lx.statelist_arena[sidx].start = 1;
                n += 1;
                hasroot = 1;
                break;
            }
            s = lx.statelist_arena[sidx].next;
        }
    }
    /* If there is no Root lexicon, the first lexicon mentioned is Root */
    if hasroot == 0 {
        let mut s = lx.statelist;
        while let Some(sidx) = s {
            if lx.statelist_arena[sidx].next.is_none() {
                let state = lx.statelist_arena[sidx].state;
                lx.state_arena[state].number = 0;
                if lx.opts.verbose {
                    let lidx = lx.state_arena[state]
                        .lexstate
                        .expect("state carries a lexstate here");
                    let name = lx.lexstates_arena[lidx].name.as_deref().unwrap_or("");
                    tracing::warn!("no Root lexicon, using '{}' as Root.", name);
                }
                lx.statelist_arena[sidx].start = 1;
                n += 1;
            }
            s = lx.statelist_arena[sidx].next;
        }
    }
    /* Mark # as the last state */
    {
        let mut s = lx.statelist;
        while let Some(sidx) = s {
            let state = lx.statelist_arena[sidx].state;
            if let Some(lidx) = lx.state_arena[state].lexstate {
                if lx.lexstates_arena[lidx].name.as_deref() == Some("#") {
                    lx.state_arena[state].number = smax - 1;
                    lx.statelist_arena[sidx].r#final = 1;
                    lx.hasfinal = 1;
                } else if lx.lexstates_arena[lidx].has_outgoing == 0 {
                    /* Also mark uncontinued states as final */
                    lx.statelist_arena[sidx].r#final = 1;
                }
            }
            s = lx.statelist_arena[sidx].next;
        }
    }

    {
        let mut s = lx.statelist;
        while let Some(sidx) = s {
            let state = lx.statelist_arena[sidx].state;
            if lx.state_arena[state].number == -1 {
                lx.state_arena[state].number = n;
                n += 1;
            }
            s = lx.statelist_arena[sidx].next;
        }
    }
    lx.lexc_statecount = n + 1;
    {
        let mut l = lx.lexstates;
        while let Some(lidx) = l {
            let state = lx.lexstates_arena[lidx].state;
            if lx.lexstates_arena[lidx].targeted == 0
                && lx.state_arena[state].number != 0
                && lx.opts.verbose
            {
                let name = lx.lexstates_arena[lidx].name.as_deref().unwrap_or("");
                tracing::warn!("lexicon '{}' defined but not used", name);
            }
            if lx.lexstates_arena[lidx].has_outgoing == 0
                && lx.lexstates_arena[lidx].name.as_deref() != Some("#")
                && lx.opts.verbose
            {
                let name = lx.lexstates_arena[lidx].name.as_deref().unwrap_or("");
                tracing::warn!("lexicon '{}' used but never defined", name);
            }
            l = lx.lexstates_arena[lidx].next;
        }
    }
}

// [spec:foma:def:lexcread.lexc-eq-paths-fn]
// [spec:foma:sem:lexcread.lexc-eq-paths-fn]
fn lexc_eq_paths(lx: &LexcCompiler, mut one: usize, mut two: usize) -> bool {
    while lx.state_arena[one].lexstate.is_none() && lx.state_arena[two].lexstate.is_none() {
        /* dereferences trans without a NULL check (unwrap → panic on None,
        the nearest safe behavior to C's crash) */
        let ot = lx.state_arena[one]
            .trans
            .expect("non-lexstate node carries a trans");
        let tt = lx.state_arena[two]
            .trans
            .expect("non-lexstate node carries a trans");
        if lx.trans_arena[ot].r#in != lx.trans_arena[tt].r#in
            || lx.trans_arena[ot].out != lx.trans_arena[tt].out
        {
            return false;
        }
        one = lx.trans_arena[ot].target;
        two = lx.trans_arena[tt].target;
    }
    lx.state_arena[one].lexstate == lx.state_arena[two].lexstate
}

/* Local chained bucket cell for lexc_merge_states (lenlist / hashstates).
Heads occupy the first maxlen+1 / tablesize entries of their Vec; chained
cells are appended, linked via `next` into the same Vec. */
#[derive(Debug, Clone, Copy)]
struct MergeNode {
    state: Option<usize>,
    next: Option<usize>,
}

// [spec:foma:def:lexcread.lexc-merge-states-fn]
// [spec:foma:sem:lexcread.lexc-merge-states-fn]
fn lexc_merge_states(lx: &mut LexcCompiler) {
    /* Array of ptrs to states depending on string length */
    let mut lenlist: Vec<MergeNode> = vec![
        MergeNode {
            state: None,
            next: None
        };
        (lx.maxlen + 1) as usize
    ];
    let mut numstates = 0i32;
    {
        let mut s = lx.statelist;
        while let Some(sidx) = s {
            if lx.state_arena[lx.statelist_arena[sidx].state].mergeable != 0 {
                numstates += 1;
            }
            s = lx.statelist_arena[sidx].next;
        }
    }

    /* Find a suitable prime proportional to the number of mergeable states */
    let mut pi = 0usize;
    while PRIMES[pi] < (numstates / 4) as u32 {
        pi += 1;
    }
    let tablesize = PRIMES[pi];
    let mut hashstates: Vec<MergeNode> = vec![
        MergeNode {
            state: None,
            next: None
        };
        tablesize as usize
    ];

    {
        let mut s = lx.statelist;
        while let Some(sidx) = s {
            let state = lx.statelist_arena[sidx].state;
            if lx.state_arena[state].mergeable != 0 {
                #[allow(unused_assignments)]
                {
                    numstates += 1; /* dead second count, as in C */
                }
                let distance = lx.state_arena[state].distance as usize;
                if lenlist[distance].state.is_none() {
                    lenlist[distance].state = Some(state);
                } else {
                    let newl = lenlist.len();
                    let oldnext = lenlist[distance].next;
                    lenlist.push(MergeNode {
                        state: Some(state),
                        next: oldnext,
                    });
                    lenlist[distance].next = Some(newl);
                }
                lx.state_arena[state].hashval %= tablesize;
                let h = lx.state_arena[state].hashval as usize;
                if hashstates[h].state.is_none() {
                    hashstates[h].state = Some(state);
                } else {
                    let newh = hashstates.len();
                    let oldnext = hashstates[h].next;
                    hashstates.push(MergeNode {
                        state: Some(state),
                        next: oldnext,
                    });
                    hashstates[h].next = Some(newh);
                }
            }
            s = lx.statelist_arena[sidx].next;
        }
    }

    let mut i = lx.maxlen;
    while i >= 1 {
        let mut cl = Some(i as usize);
        while let Some(clidx) = cl {
            match lenlist[clidx].state {
                None => break,
                Some(cstate) => {
                    if lx.state_arena[cstate].mergeable != 1 {
                        cl = lenlist[clidx].next;
                        continue;
                    }
                    let state = cstate;
                    let hash = lx.state_arena[state].hashval as usize;
                    let mut ch = Some(hash);
                    while let Some(chidx) = ch {
                        if let Some(hstate) = hashstates[chidx].state {
                            if hstate != state
                                && lx.state_arena[hstate].mergeable == 1
                                && lx.state_arena[hstate].distance == lx.state_arena[state].distance
                                && lexc_eq_paths(lx, hstate, state)
                            {
                                lx.state_arena[hstate].merge_with = state;
                                let mut purge = hstate;
                                while lx.state_arena[purge].lexstate.is_none() {
                                    lx.state_arena[purge].mergeable = 2;
                                    let t = lx.state_arena[purge]
                                        .trans
                                        .expect("non-lexstate node carries a trans");
                                    purge = lx.trans_arena[t].target;
                                }
                            }
                        }
                        ch = hashstates[chidx].next;
                    }
                    cl = lenlist[clidx].next;
                }
            }
        }
        i -= 1;
    }

    /* Rewrite pass: redirect targets through merge_with; free merged states'
    trans cells (no-op in the arena); set lexstate->targeted for survivors */
    {
        let mut s = lx.statelist;
        while let Some(sidx) = s {
            let state = lx.statelist_arena[sidx].state;
            let merged = lx.state_arena[state].mergeable == 2;
            let mut t = lx.state_arena[state].trans;
            let mut tprev: Option<usize> = None;
            while let Some(tidx) = t {
                let tgt = lx.trans_arena[tidx].target;
                lx.trans_arena[tidx].target = lx.state_arena[tgt].merge_with;
                if tprev.is_some() && merged {
                    /* free(tprev) — no-op (arena) */
                } else {
                    let newtgt = lx.trans_arena[tidx].target;
                    if let Some(lidx) = lx.state_arena[newtgt].lexstate {
                        lx.lexstates_arena[lidx].targeted = 1;
                    }
                }
                tprev = Some(tidx);
                t = lx.trans_arena[tidx].next;
            }
            /* if (tprev != NULL && merged) free(tprev) — no-op */
            s = lx.statelist_arena[sidx].next;
        }
    }

    /* Removal pass: unlink and free cells (and states) with mergeable == 2 */
    {
        let mut s = lx.statelist;
        let mut sprev: Option<usize> = None;
        while let Some(sidx) = s {
            let state = lx.statelist_arena[sidx].state;
            if lx.state_arena[state].mergeable == 2 {
                match sprev {
                    Some(p) => {
                        lx.statelist_arena[p].next = lx.statelist_arena[sidx].next;
                    }
                    None => {
                        /* Latent quirk kept as-is: the C sets statelist = s (the
                        removed cell) instead of s->next, so one deleted (mergeable
                        == 2) cell survives as the list head with its `next` intact.
                        This is benign, not a memory hazard in the arena: the stray
                        state has no incoming arcs (they were redirected through
                        merge_with), so lexc_to_fsm emits it as an unreachable
                        component that fsm_determinize/minimize prune — output is
                        unaffected. */
                        lx.statelist = Some(sidx);
                    }
                }
                /* free(s->state); free(sf) — no-op (arena) */
                s = lx.statelist_arena[sidx].next;
            } else {
                sprev = Some(sidx);
                s = lx.statelist_arena[sidx].next;
            }
        }
    }

    /* Cleanup of the index chains is memory-only (the local Vecs drop here);
    the C off-by-one leak of lenlist[maxlen]'s chain has no observable effect. */
}

// [spec:foma:def:lexcread.lexc-to-fsm-fn]
// [spec:foma:sem:lexcread.lexc-to-fsm-fn]
// [spec:foma:def:lexc.lexc-to-fsm-fn]
// [spec:foma:sem:lexc.lexc-to-fsm-fn]
fn lexc_to_fsm(lx: &mut LexcCompiler) -> Box<Fsm> {
    if lx.opts.verbose {
        tracing::info!("Building lexicon...");
    }
    lexc_merge_states(lx);
    let mut net = fsm_create("");
    /* free(net->sigma); net->sigma = lexsigma (ownership transfer) */
    net.sigma = core::mem::take(&mut lx.lexsigma);
    lexc_number_states(lx);
    if lx.hasfinal == 0 {
        if lx.opts.verbose {
            tracing::warn!("# is never reached!!!");
        }
        /* Leak path: lexc_cleanup is not called; the state graph and hash
        tables persist in the arenas until the next lexc_init. */
        return fsm_empty_set();
    }
    /* sa is indexed by state number. With the lexc_number_states "#"
    collision fixed, every surviving state has a distinct number in
    [0, statecount) so each sa entry is written exactly once (the unreachable
    stray head cell from lexc_merge_states also lands in its own slot and is
    pruned downstream). */
    let statecount = lx.lexc_statecount as usize;
    let mut sa: Vec<(usize, i8, i8)> = vec![(0usize, 0i8, 0i8); statecount];
    {
        let mut s = lx.statelist;
        while let Some(sidx) = s {
            let state = lx.statelist_arena[sidx].state;
            let num = lx.state_arena[state].number as usize;
            sa[num] = (
                state,
                lx.statelist_arena[sidx].start,
                lx.statelist_arena[sidx].r#final,
            );
            s = lx.statelist_arena[sidx].next;
        }
    }
    let mut linecount = 0i32;
    {
        let mut s = lx.statelist;
        while let Some(sidx) = s {
            linecount += 1;
            let state = lx.statelist_arena[sidx].state;
            let mut t = lx.state_arena[state].trans;
            while let Some(tidx) = t {
                linecount += 1;
                t = lx.trans_arena[tidx].next;
            }
            s = lx.statelist_arena[sidx].next;
        }
    }
    let default_line = FsmState {
        state_no: 0,
        r#in: 0,
        out: 0,
        target: 0,
        final_state: 0,
        start_state: 0,
    };
    let mut fsm: Vec<FsmState> = vec![default_line; (linecount + 1) as usize];
    let mut i = 0i32;
    for &(state, sstart, sfinal) in sa.iter().take(statecount) {
        /* sa[num] was stored as (state, start, final); C calls
        add_fsm_arc(..., s[j].final, s[j].start), so bind so that
        `sfinal` = the final flag and `sstart` = the start flag. */
        if lx.state_arena[state].trans.is_none() {
            add_fsm_arc(
                &mut fsm,
                i,
                lx.state_arena[state].number,
                -1,
                -1,
                -1,
                sfinal as i32,
                sstart as i32,
            );
            i += 1;
        } else {
            let mut t = lx.state_arena[state].trans;
            while let Some(tidx) = t {
                let tgt = lx.trans_arena[tidx].target;
                add_fsm_arc(
                    &mut fsm,
                    i,
                    lx.state_arena[state].number,
                    lx.trans_arena[tidx].r#in as i32,
                    lx.trans_arena[tidx].out as i32,
                    lx.state_arena[tgt].number,
                    sfinal as i32,
                    sstart as i32,
                );
                i += 1;
                t = lx.trans_arena[tidx].next;
            }
        }
    }
    add_fsm_arc(&mut fsm, i, -1, -1, -1, -1, -1, -1);
    net.states = fsm;
    net.statecount = lx.lexc_statecount;
    fsm_update_flags(&mut net, UNK, UNK, UNK, UNK, UNK, UNK);
    /* lexsigma is now net.sigma (aliased in C); operate on net.sigma */
    if sigma_find_number(EPSILON, &net.sigma).is_none() {
        sigma_add_special(EPSILON, &mut net.sigma);
    }
    /* free(s): C frees the sa array here (s == sa after the build loop);
    the sa Vec drops at scope end, observably identical */
    lexc_cleanup(lx);
    sigma_cleanup(&mut net, 0);
    sigma_sort(&mut net);

    if lx.opts.verbose {
        tracing::info!("Determinizing...");
    }
    let net = fsm_determinize(net);
    if lx.opts.verbose {
        tracing::info!("Minimizing...");
    }
    let net = fsm_topsort(fsm_minimize(&lx.opts, net));
    if lx.opts.verbose {
        tracing::info!("Done!");
    }
    net
}

// [spec:foma:def:lexcread.lexc-cleanup-fn]
// [spec:foma:sem:lexcread.lexc-cleanup-fn]
fn lexc_cleanup(lx: &mut LexcCompiler) {
    /* free every hashtable chain node + symbol, then the bucket array */
    lx.hashtable = Vec::new();
    /* free the mc list (symbols + nodes) */
    lx.mc_arena = Vec::new();
    lx.mc = None;
    /* free the lexstates list (names + nodes; states freed via statelist) */
    lx.lexstates_arena = Vec::new();
    lx.lexstates = None;
    /* free each state's trans, then each state, then the statelist cells */
    lx.trans_arena = Vec::new();
    lx.state_arena = Vec::new();
    lx.statelist_arena = Vec::new();
    lx.statelist = None;
    /* lexsigma is not freed here — ownership was transferred to the net. The
    static pointers are left dangling in C; reset to None here (lexc_init must
    still run before any further lexc use). */
    lx.clexicon = None;
    lx.ctarget = None;
}

// [spec:foma:def:lexc.lexc-trim-fn]
// [spec:foma:sem:lexc.lexc-trim-fn]
// Implemented in foma/lexc.l (not lexcread.c); ported here per the concern.
pub fn lexc_trim(s: &str) -> &str {
    /* Remove trailing ; = space tab, then leading space tab newline. The C's
    asymmetric character sets are preserved (this is not str::trim). */
    s.trim_end_matches([';', '=', ' ', '\t'])
        .trim_start_matches([' ', '\t', '\n'])
}

/* ------------------------------------------------------------------ */
/* lexc lexer driver (foma/lexc.l): fsm_lexc_parse_string / _file      */
/* ------------------------------------------------------------------ */

/* lexc.l: #define SOURCE_LEXICON 0 / #define TARGET_LEXICON 1 */
const SOURCE_LEXICON: i32 = 0;
const TARGET_LEXICON: i32 = 1;

// [spec:foma:def:fomalib.fsm-lexc-parse-string-fn]
// [spec:foma:sem:fomalib.fsm-lexc-parse-string-fn]
pub fn fsm_lexc_parse_string(
    opts: &FomaOptions,
    mut defines: Option<&mut DefinedNetworks>,
    string: &str,
) -> Option<Box<Fsm>> {
    /* C took an (ignored) `verbose` int parameter; the warnings keyed off the
    global g_verbose instead. Both collapse into `opts.verbose` here. C read the
    `g_defines` registry global ("olddefines"); it is the `defines` parameter
    now — Definitions-section nets are added to it and persist after the call. */

    /* lexentries = -1; lexclineno = 1; lexc_init(). Wave 4: the compiler state
    is a local LexcCompiler (was a thread_local), created and threaded per call
    so fsm_lexc_parse_string is reentrant. */
    let mut lexentries: i32 = -1;
    let mut lx = LexcCompiler::new_empty();
    lx.opts = opts.clone();
    lexc_init(&mut lx);

    /* lexclex(): the flex scanner is replaced by nfst-lexc; its return code is
    modelled as `syntax_error` (the C returns 1 on the <*>(.) error rule). */
    let mut syntax_error = false;

    match nfst_lexc::parse(string) {
        Ok(file) => {
            let f = &file.value;

            /* foma's lexc.l has no NOFLAGS section: a "NOFLAGS" token hits the
            <*>(.) syntax-error rule. nfst-lexc accepts it (an HFST feature), so
            reject here to stay faithful (see report). */
            if !f.noflags.is_empty() {
                tracing::error!("lexc: NOFLAGS section is not supported by foma");
                return None;
            }

            /* <MCS>{NONRESERVED}+ { lexc_add_mc(lexctext); } */
            for m in &f.multichars {
                lexc_add_mc(&mut lx, &m.value.0);
            }

            /* <DEFREGEX>[\073] { if (my_yyparse(...,g_defines,...)==0)
            add_defined(g_defines, fsm_topsort(fsm_minimize(&lx.opts, current_parse)),
                        tempstr); } */
            for d in &f.definitions {
                let body = nfst_xre::pretty_print(&d.value.body);
                if let Some(net) = fsm_parse_regex(opts, &body, defines.as_deref_mut(), None) {
                    let net = fsm_topsort(net);
                    if let Some(defs) = defines.as_deref_mut() {
                        add_defined(defs, Some(net), &d.value.name);
                    }
                }
            }

            for lex in &f.lexicons {
                /* <*>(LEXICON|Lexicon){SPACE}+{NONRESERVED}+ */
                if lexentries != -1 {
                    tracing::info!("{} entries", lexentries);
                }
                tracing::info!("building lexicon '{}'...", lex.value.name);
                lexentries = 0;
                lexc_set_current_lexicon(&mut lx, &lex.value.name, SOURCE_LEXICON);

                for entry in &lex.value.entries {
                    /* The gloss ("info" string) is discarded by the C lexer
                    (EATUPINFO state), so entry.value.gloss is ignored. */
                    match &entry.value.spec {
                        nfst_lexc::EntrySpec::Empty => {
                            /* No word token: current word stays the epsilon left
                            by the preceding lexc_clear_current_word. */
                        }
                        nfst_lexc::EntrySpec::String(s) => {
                            lexc_set_current_word(&mut lx, s, None);
                        }
                        nfst_lexc::EntrySpec::Pair { upper, lower } => {
                            /* nfst-lexc already split the unescaped `:`; feed the
                            two de-escaped sides straight in. */
                            lexc_set_current_word(&mut lx, upper, Some(lower));
                        }
                        nfst_lexc::EntrySpec::Regex(xre) => {
                            /* <REGEX>[\076] { if (my_yyparse(...)==0)
                            lexc_set_network(current_parse); } */
                            let r = nfst_xre::pretty_print(xre);
                            if let Some(net) =
                                fsm_parse_regex(opts, &r, defines.as_deref_mut(), None)
                            {
                                lexc_set_network(&mut lx, net);
                            }
                        }
                    }

                    /* The continuation token drives the target lexicon + word:
                    lexc_trim; lexc_set_current_lexicon(TARGET); lexc_add_word();
                    lexc_clear_current_word(); lexentries++. */
                    lexc_set_current_lexicon(&mut lx, &entry.value.continuation, TARGET_LEXICON);
                    lexc_add_word(&mut lx);
                    lexc_clear_current_word(&mut lx);
                    lexentries += 1;
                    if lexentries % 10000 == 0 {
                        tracing::info!("{}...", lexentries);
                    }
                }
            }
        }
        Err(e) => {
            /* The C prints "\n***Syntax error on line %i column %i at '%s'\n"
            and returns 1; line/column/text are not recoverable from nfst-lexc,
            so emit its first diagnostic instead (see report). lexc_to_fsm is
            still called below, as in C. */
            syntax_error = true;
            let msg = e
                .diagnostics
                .first()
                .map(|d| d.message.clone())
                .unwrap_or_else(|| "syntax error".to_string());
            tracing::error!("Syntax error: {}", msg);
        }
    }

    /* if (lexclex() != 1) { if (lexentries != -1) printf("%i\n", lexentries); } */
    if !syntax_error && lexentries != -1 {
        tracing::info!("{} entries", lexentries);
    }

    /* return lexc_to_fsm() */
    Some(lexc_to_fsm(&mut lx))
}

// [spec:foma:def:fomalib.fsm-lexc-parse-file-fn]
// [spec:foma:sem:fomalib.fsm-lexc-parse-file-fn]
pub fn fsm_lexc_parse_file(
    opts: &FomaOptions,
    defines: Option<&mut DefinedNetworks>,
    filename: &str,
) -> Option<Box<Fsm>> {
    /* mystring = file_to_mem(filename); return fsm_lexc_parse_string(mystring,
    verbose). The C never frees mystring (documented leak); here the buffer is a
    Vec that drops at scope end — an observable no-op. */
    let mystring = file_to_mem(filename).ok()?;
    /* file_to_mem appends a terminating NUL; strip it (and any BOM-free tail of
    trailing NULs) before handing the text to the parser. */
    let end = mystring
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(mystring.len());
    let text = String::from_utf8_lossy(&mystring[..end]);
    fsm_lexc_parse_string(opts, defines, &text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{
        apply_down, apply_init, apply_lower_words, apply_up, apply_upper_words, apply_words,
    };

    /* ---- helpers -------------------------------------------------------- */

    /// Compile a lexc source string (verbose off is irrelevant — g_verbose is a
    /// separate global; the parse string ignores its own `verbose` arg).
    fn compile(src: &str) -> Box<Fsm> {
        fsm_lexc_parse_string(&FomaOptions::default(), None, src).expect("compile produced a net")
    }

    fn lower_all(net: &Fsm) -> Vec<String> {
        let mut h = apply_init(net);
        let mut v = Vec::new();
        let mut r = apply_lower_words(&mut h);
        while let Some(s) = r {
            v.push(s);
            r = apply_lower_words(&mut h);
        }
        v.sort();
        v.dedup();
        v
    }

    fn upper_all(net: &Fsm) -> Vec<String> {
        let mut h = apply_init(net);
        let mut v = Vec::new();
        let mut r = apply_upper_words(&mut h);
        while let Some(s) = r {
            v.push(s);
            r = apply_upper_words(&mut h);
        }
        v.sort();
        v.dedup();
        v
    }

    fn words_all(net: &Fsm) -> Vec<String> {
        let mut h = apply_init(net);
        let mut v = Vec::new();
        let mut r = apply_words(&mut h);
        while let Some(s) = r {
            v.push(s);
            r = apply_words(&mut h);
        }
        v.sort();
        v.dedup();
        v
    }

    fn down_one(net: &Fsm, w: &str) -> Option<String> {
        let mut h = apply_init(net);
        apply_down(&mut h, Some(w))
    }

    fn up_one(net: &Fsm, w: &str) -> Option<String> {
        let mut h = apply_init(net);
        apply_up(&mut h, Some(w))
    }

    /* Drive one word entry Root -> "#" through the public trie API. */
    fn add_word_entry(lx: &mut LexcCompiler, word: &str) {
        lexc_set_current_word(lx, word, None);
        lexc_set_current_lexicon(lx, "#", TARGET_LEXICON);
        lexc_add_word(lx);
        lexc_clear_current_word(lx);
    }

    /* ---- end-to-end (fsm_lexc_parse_string + apply) --------------------- */

    // Root + `#` termination, identity words, continuation-class LIFO trie:
    // exercises the whole driver (init -> set_current_lexicon/word -> add_word
    // -> clear -> to_fsm -> number_states -> merge_states -> cleanup).
    // [spec:foma:sem:fomalib.fsm-lexc-parse-string-fn/test]
    // [spec:foma:sem:lexcread.lexc-to-fsm-fn/test]
    // [spec:foma:sem:lexc.lexc-to-fsm-fn/test]
    #[test]
    fn e2e_basic_root_and_hash() {
        let net = compile("LEXICON Root\ncat # ;\ndog # ;\n");
        assert_eq!(lower_all(&net), vec!["cat", "dog"]);
        assert_eq!(upper_all(&net), vec!["cat", "dog"]);
    }

    // A cross-lexicon continuation (Root -> N) concatenates prefixes.
    // [spec:foma:sem:lexcread.lexc-set-current-lexicon-fn/test]
    // [spec:foma:sem:lexc.lexc-set-current-lexicon-fn/test]
    #[test]
    fn e2e_continuation_class() {
        let net = compile("LEXICON Root\nbig N ;\nLEXICON N\ncat # ;\ndog # ;\n");
        assert_eq!(lower_all(&net), vec!["bigcat", "bigdog"]);
    }

    // Pair entry `cat:dog`: upper and lower projections differ.
    // [spec:foma:sem:lexcread.lexc-set-current-word-fn/test]
    // [spec:foma:sem:lexc.lexc-set-current-word-fn/test]
    #[test]
    fn e2e_pair_entry() {
        let net = compile("LEXICON Root\ncat:dog # ;\n");
        assert_eq!(upper_all(&net), vec!["cat"]);
        assert_eq!(lower_all(&net), vec!["dog"]);
        assert_eq!(down_one(&net, "cat").as_deref(), Some("dog"));
        assert_eq!(up_one(&net, "dog").as_deref(), Some("cat"));
    }

    // Multichar symbols declared + longest-first tokenization: `+PlPoss` must
    // win over its prefix `+Pl` (mc list is kept in decreasing length order).
    // [spec:foma:sem:lexcread.lexc-add-mc-fn/test]
    // [spec:foma:sem:lexc.lexc-add-mc-fn/test]
    #[test]
    fn e2e_multichar_longest_first() {
        let net = compile("Multichar_Symbols +Pl +PlPoss\nLEXICON Root\nx+Pl # ;\ny+PlPoss # ;\n");
        assert_eq!(upper_all(&net), vec!["x+Pl", "y+PlPoss"]);
    }

    // `%`-escape: `a%:b` is the literal three-symbol string a : b (identity),
    // not a pair split at the colon.
    // [spec:foma:sem:lexcread.lexc-find-delim-fn/test]
    // [spec:foma:sem:lexcread.lexc-deescape-string-fn/test]
    #[test]
    fn e2e_percent_escape_colon() {
        let net = compile("LEXICON Root\na%:b # ;\n");
        assert_eq!(upper_all(&net), vec!["a:b"]);
        assert_eq!(lower_all(&net), vec!["a:b"]);
    }

    // `0` as epsilon: `a:0` = a:eps, `0:b` = eps:b — projections drop epsilon.
    // [spec:foma:sem:lexcread.lexc-string-to-tokens-fn+1/test]
    #[test]
    fn e2e_zero_is_epsilon() {
        let net = compile("LEXICON Root\na:0 # ;\n0:b # ;\n");
        assert_eq!(upper_all(&net), vec!["", "a"]);
        assert_eq!(lower_all(&net), vec!["", "b"]);
    }

    // `< regex >` entry: the parsed net is spliced between source and target.
    // [spec:foma:sem:lexcread.lexc-set-network-fn/test]
    // [spec:foma:sem:lexc.lexc-set-network-fn/test]
    // [spec:foma:sem:lexcread.lexc-add-network-fn/test]
    #[test]
    fn e2e_regex_entry() {
        let net = compile("LEXICON Root\n< a (b) c > # ;\n");
        assert_eq!(words_all(&net), vec!["abc", "ac"]);
    }

    // `< a ? >` (identity/unknown) plus a later word `b` introducing a new
    // symbol: lexc_update_unknowns patches the @-arc so `ab` is also matched.
    // [spec:foma:sem:lexcread.lexc-update-unknowns-fn/test]
    #[test]
    fn e2e_regex_unknown_symbols() {
        let net = compile("LEXICON Root\n< a ? > # ;\nb # ;\n");
        let low = lower_all(&net);
        // C `read lexc` gives 4 paths: b, aa, ab, a@ (@ = other/unknown).
        assert_eq!(low.len(), 4);
        assert!(low.contains(&"aa".to_string()));
        assert!(low.contains(&"ab".to_string()));
        assert!(low.contains(&"b".to_string()));
    }

    // Definitions section + use inside a `< regex >` entry. As in the REPL,
    // the registry is an initialized (dummy-head) list before parsing.
    // [spec:foma:sem:fomalib.fsm-lexc-parse-string-fn/test]
    #[test]
    fn e2e_definitions_section() {
        let mut defs = crate::define::defined_networks_init();
        let net = fsm_lexc_parse_string(
            &FomaOptions::default(),
            Some(&mut defs),
            "Definitions\nV = a | e | i | o | u ;\nLEXICON Root\n< V V > # ;\n",
        )
        .expect("compile produced a net");
        assert_eq!(words_all(&net).len(), 25);
    }

    // Info ("gloss") string is discarded by the lexer (EATUPINFO).
    // [spec:foma:sem:lexc.lexc-add-word-fn/test]
    #[test]
    fn e2e_infostring_discarded() {
        let net = compile("LEXICON Root\ncat # \"a feline\" ;\n");
        assert_eq!(lower_all(&net), vec!["cat"]);
    }

    // Undefined continuation class: the net is still built (the dead-end
    // lexicon has has_outgoing==0 so number_states makes it final).
    // [spec:foma:sem:lexcread.lexc-number-states-fn+1/test]
    #[test]
    fn e2e_undefined_continuation() {
        let net = compile("LEXICON Root\ncat Nonexistent ;\ndog # ;\n");
        assert_eq!(lower_all(&net), vec!["cat", "dog"]);
    }

    // No Root lexicon: the first-mentioned lexicon becomes the start state.
    // [spec:foma:sem:lexcread.lexc-number-states-fn+1/test]
    #[test]
    fn e2e_no_root_lexicon() {
        let net = compile("LEXICON First\nhi # ;\n");
        assert_eq!(lower_all(&net), vec!["hi"]);
    }

    // `#` never reached -> fsm_empty_set() (0 paths).
    // [spec:foma:sem:lexcread.lexc-to-fsm-fn/test]
    // [spec:foma:sem:lexc.lexc-to-fsm-fn/test]
    #[test]
    fn e2e_hash_never_reached_empty() {
        let net = compile("LEXICON Root\ncat Root ;\n");
        assert_eq!(net.finalcount, 0);
        assert_eq!(lower_all(&net), Vec::<String>::new());
    }

    /* ---- fsm_lexc_parse_file -------------------------------------------- */

    // Parse via a temp file; a nonexistent path returns None (DEVIATION: C
    // hands NULL to the scanner).
    // [spec:foma:sem:fomalib.fsm-lexc-parse-file-fn/test]
    #[test]
    fn parse_file_roundtrip_and_missing() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("foma_lexc_test_{}.lexc", std::process::id()));
        std::fs::write(&path, "LEXICON Root\ncat # ;\n").unwrap();
        let net = fsm_lexc_parse_file(&FomaOptions::default(), None, path.to_str().unwrap())
            .expect("file parsed");
        assert_eq!(lower_all(&net), vec!["cat"]);
        std::fs::remove_file(&path).ok();

        assert!(
            fsm_lexc_parse_file(&FomaOptions::default(), None, "/no/such/foma/lexc/file.xyz")
                .is_none()
        );
    }

    /* ---- direct API: hashing -------------------------------------------- */

    // djb2 with signed-char sign extension, % 3079. Values derived offline.
    // [spec:foma:sem:lexcread.lexc-symbol-hash-fn/test]
    #[test]
    fn symbol_hash_signed_char() {
        assert_eq!(lexc_symbol_hash("a"), 2167);
        assert_eq!(lexc_symbol_hash("cat"), 686);
        assert_eq!(lexc_symbol_hash("+Noun"), 211);
        assert_eq!(lexc_symbol_hash(""), 2302);
        // High byte (0xC3 0xA9 = "é"): signed-char extension gives 1551, not
        // the unsigned-byte value 1018 — pins the sign-extension quirk.
        assert_eq!(lexc_symbol_hash("é"), 1551);
    }

    // PJW-style suffix fold over cwordin|cwordout<<8, no table modulus.
    // [spec:foma:sem:lexcread.lexc-suffix-hash-fn/test]
    #[test]
    fn suffix_hash_values() {
        let mut lx = LexcCompiler::new_empty();
        lx.cwordin = vec![3, 4, -1];
        lx.cwordout = vec![3, 4, -1];
        assert_eq!(lexc_suffix_hash(&lx, 0), 13364);
        assert_eq!(lexc_suffix_hash(&lx, 1), 1028);
        lx.cwordin = vec![5, -1];
        lx.cwordout = vec![7, -1];
        assert_eq!(lexc_suffix_hash(&lx, 0), 1797);
    }

    // Add/find in the sigma hashtable: head fill, chain append (no dedup ->
    // shadowed entry), find returns the head match, miss -> None.
    // [spec:foma:sem:lexcread.lexc-add-sigma-hash-fn/test]
    // [spec:foma:sem:lexcread.lexc-find-sigma-hash-fn+1/test]
    #[test]
    fn sigma_hash_add_find_shadow() {
        let mut lx = LexcCompiler::new_empty();
        lexc_init(&mut lx);
        assert_eq!(lexc_find_sigma_hash(&lx, "a"), None);
        lexc_add_sigma_hash(&mut lx, "a", 7);
        lexc_add_sigma_hash(&mut lx, "a", 99); // shadow appended to tail
        let bucket = lexc_symbol_hash("a") as usize;
        assert_eq!(lx.hashtable[bucket].symbol.as_deref(), Some("a"));
        assert_eq!(lx.hashtable[bucket].sigma_number, 7);
        assert!(lx.hashtable[bucket].next.is_some());
        assert_eq!(lexc_find_sigma_hash(&lx, "a"), Some(7)); // head wins
        assert_eq!(lexc_find_sigma_hash(&lx, "zzz"), None);
    }

    /* ---- direct API: string helpers ------------------------------------- */

    // lexc_trim: strip trailing ;/=/space/tab, then leading space/tab/nl;
    // empty / all-trimmable input must not underrun.
    // [spec:foma:sem:lexc.lexc-trim-fn/test]
    #[test]
    fn trim_cases() {
        assert_eq!(lexc_trim("  cat ;;  "), "cat");
        assert_eq!(lexc_trim(""), "");
        assert_eq!(lexc_trim("==="), "");
    }

    /* ---- direct API: tokenization --------------------------------------- */

    // Multichar longest-first, a bare '0' -> alignment EPSILON, and fresh-sigma
    // numbering (first regular symbol = 3).
    // [spec:foma:sem:lexcread.lexc-string-to-tokens-fn+1/test]
    // [spec:foma:sem:lexcread.lexc-find-mc-fn+1/test]
    // [spec:foma:sem:lexc.lexc-find-mc-fn+1/test]
    #[test]
    fn string_to_tokens_multichar_and_epsilon() {
        let mut lx = LexcCompiler::new_empty();
        lexc_init(&mut lx);
        // register +Pl (len 3) then +PlPoss (len 7): mc list -> longest first
        lexc_add_mc(&mut lx, "+Pl"); // sigma 3
        lexc_add_mc(&mut lx, "+PlPoss"); // sigma 4
        assert!(lexc_find_mc(&lx, "+Pl"));
        assert!(!lexc_find_mc(&lx, "+Nope"));
        // mc list head is the longest symbol
        let head = lx.mc.unwrap();
        assert_eq!(lx.mc_arena[head].symbol.as_deref(), Some("+PlPoss"));

        let mut arr: Vec<i32> = Vec::new();
        lexc_string_to_tokens(&mut lx, "+PlPoss", &mut arr);
        assert_eq!(&arr[..2], &[4, -1]); // longest match wins

        let mut arr2: Vec<i32> = Vec::new();
        lexc_string_to_tokens(&mut lx, "+Plx", &mut arr2); // +Pl then new 'x'=5
        assert_eq!(&arr2[..3], &[3, 5, -1]);

        // a bare '0' between two symbols -> EPSILON(0)
        let mut arr3: Vec<i32> = Vec::new();
        lexc_string_to_tokens(&mut lx, "a0a", &mut arr3);
        assert_eq!(arr3[1], EPSILON);
    }

    /* ---- direct API: alignment ------------------------------------------ */

    // Tail-pad the shorter side with EPSILON; empty:empty -> single eps pair.
    // [spec:foma:sem:lexcread.lexc-pad-fn/test]
    #[test]
    fn pad_shapes() {
        let mut lx = LexcCompiler::new_empty();
        lx.cwordin = vec![3, 4, -1];
        lx.cwordout = vec![5, -1];
        lexc_pad(&mut lx);
        assert_eq!(&lx.cwordin[..3], &[3, 4, -1]);
        assert_eq!(&lx.cwordout[..3], &[5, EPSILON, -1]);

        lx.cwordin = vec![-1];
        lx.cwordout = vec![-1];
        lexc_pad(&mut lx);
        assert_eq!(&lx.cwordin[..2], &[EPSILON, -1]);
        assert_eq!(&lx.cwordout[..2], &[EPSILON, -1]);
    }

    // Min-edit alignment: unequal symbols never substitute (cost 100), so
    // a->b becomes eps:b, a:eps; equal symbols align on the diagonal.
    // [spec:foma:sem:lexcread.lexc-medpad-fn/test]
    #[test]
    fn medpad_shapes() {
        let mut lx = LexcCompiler::new_empty();
        lx.cwordin = vec![3, -1];
        lx.cwordout = vec![4, -1];
        lexc_medpad(&mut lx);
        assert_eq!(&lx.cwordin[..3], &[EPSILON, 3, -1]);
        assert_eq!(&lx.cwordout[..3], &[4, EPSILON, -1]);

        lx.cwordin = vec![3, 4, -1];
        lx.cwordout = vec![3, 4, -1];
        lexc_medpad(&mut lx);
        assert_eq!(&lx.cwordin[..3], &[3, 4, -1]);
        assert_eq!(&lx.cwordout[..3], &[3, 4, -1]);
    }

    /* ---- direct API: set_current_word ----------------------------------- */

    // Pair split at unescaped ':', carity=2, identity copy for carity=1.
    // [spec:foma:sem:lexcread.lexc-set-current-word-fn/test]
    // [spec:foma:sem:lexc.lexc-set-current-word-fn/test]
    // [spec:foma:sem:lexcread.lexc-clear-current-word-fn/test]
    // [spec:foma:sem:lexc.lexc-clear-current-word-fn/test]
    #[test]
    fn set_current_word_pair_and_identity() {
        let mut lx = LexcCompiler::new_empty();
        lexc_init(&mut lx);
        lexc_set_current_word(&mut lx, "cat", Some("dog"));
        assert_eq!(lx.carity, 2);
        assert_eq!(&lx.cwordin[..4], &[3, 4, 5, -1]); // c a t
        assert_eq!(&lx.cwordout[..4], &[6, 7, 8, -1]); // d o g
        lexc_clear_current_word(&mut lx);
        assert_eq!(&lx.cwordin[..2], &[EPSILON, -1]);
        assert_eq!(lx.current_entry, WORD_ENTRY);
        lexc_set_current_word(&mut lx, "cat", None);
        assert_eq!(lx.carity, 1);
        assert_eq!(&lx.cwordin[..4], &[3, 4, 5, -1]);
        assert_eq!(&lx.cwordout[..4], &[3, 4, 5, -1]); // identity copy
    }

    /* ---- direct API: lexicons & states ---------------------------------- */

    // init resets the tables; create/reuse a lexicon; source vs target; a dead
    // lexicon lookup.
    // [spec:foma:sem:lexcread.lexc-init-fn/test]
    // [spec:foma:sem:lexc.lexc-init-fn/test]
    // [spec:foma:sem:lexcread.lexc-set-current-lexicon-fn/test]
    // [spec:foma:sem:lexc.lexc-set-current-lexicon-fn/test]
    // [spec:foma:sem:lexcread.lexc-add-state-fn/test]
    // [spec:foma:sem:lexcread.lexc-find-lex-state-fn/test]
    // [spec:foma:sem:lexc.lexc-find-lex-state-fn/test]
    #[test]
    fn init_and_set_current_lexicon() {
        let mut lx = LexcCompiler::new_empty();
        lexc_init(&mut lx);
        assert_eq!(lx.hashtable.len(), SIGMA_HASH_TABLESIZE);
        /* lexc_init resets the alphabet to empty */
        assert!(lx.lexsigma.is_empty());
        assert_eq!(lx.lexc_statecount, 0);
        assert_eq!(lx.hashtable[0].sigma_number, -1);
        assert!(lexc_find_lex_state(&lx, "Root").is_none());
        lexc_set_current_lexicon(&mut lx, "Root", SOURCE_LEXICON);
        // one lexstate + one state registered; clexicon set, has_outgoing=1
        assert_eq!(lx.lexstates_arena.len(), 1);
        assert_eq!(lx.lexc_statecount, 1);
        let l = lx.clexicon.unwrap();
        assert_eq!(lx.lexstates_arena[l].has_outgoing, 1);
        assert_eq!(lx.lexstates_arena[l].name.as_deref(), Some("Root"));
        assert!(lexc_find_lex_state(&lx, "Root").is_some());
        // target reuse of a fresh name creates a second lexicon
        lexc_set_current_lexicon(&mut lx, "N", TARGET_LEXICON);
        assert_eq!(lx.lexstates_arena.len(), 2);
        assert_eq!(lx.lexstates_arena[lx.ctarget.unwrap()].has_outgoing, 0);
        // re-selecting Root as source reuses the same lexstate (no growth)
        lexc_set_current_lexicon(&mut lx, "Root", SOURCE_LEXICON);
        assert_eq!(lx.lexstates_arena.len(), 2);
    }

    // Prefix sharing (trie): a second word sharing a prefix follows existing
    // transitions and adds no new state.
    // [spec:foma:sem:lexcread.lexc-add-word-fn/test]
    // [spec:foma:sem:lexc.lexc-add-word-fn/test]
    #[test]
    fn add_word_prefix_sharing() {
        let mut lx = LexcCompiler::new_empty();
        lexc_init(&mut lx);
        lexc_set_current_lexicon(&mut lx, "Root", SOURCE_LEXICON);
        add_word_entry(&mut lx, "cat");
        let after_cat = lx.state_arena.len();
        add_word_entry(&mut lx, "car"); // shares "ca" prefix -> no new state
        let after_car = lx.state_arena.len();
        assert_eq!(after_cat, after_car);
        // followed prefix states were demoted to mergeable == 0
        let root = lx.lexstates_arena[lx.clexicon.unwrap()].state;
        let t = lx.state_arena[root].trans.unwrap();
        let s1 = lx.trans_arena[t].target; // 'c' target
        assert_eq!(lx.state_arena[s1].mergeable, 0);
    }

    /* ---- direct API: eq_paths ------------------------------------------- */

    // Identical single-arc suffix chains ending in the same lexstate compare
    // equal; a label mismatch compares unequal.
    // [spec:foma:sem:lexcread.lexc-eq-paths-fn/test]
    #[test]
    fn eq_paths_match_and_mismatch() {
        let mut lx = LexcCompiler::new_empty();
        // lexicon terminal state (index 0)
        lx.lexstates_arena.push(Lexstates {
            name: Some("#".into()),
            state: 0,
            next: None,
            targeted: 0,
            has_outgoing: 0,
        });
        let dest = push_state(&mut lx, Some(0));
        let a = push_chain(&mut lx, 5, 5, dest); // (5:5) -> dest
        let b = push_chain(&mut lx, 5, 5, dest);
        let c = push_chain(&mut lx, 6, 6, dest); // different label
        assert!(lexc_eq_paths(&lx, a, b));
        assert!(!lexc_eq_paths(&lx, a, c));
    }

    // Null trans deref: eq_paths on two non-lexicon states with no transition
    // panics (DEVIATION: C dereferences NULL).
    // [spec:foma:sem:lexcread.lexc-eq-paths-fn/test]
    #[test]
    #[should_panic]
    fn eq_paths_null_trans_panics() {
        let mut lx = LexcCompiler::new_empty();
        let a = push_state(&mut lx, None);
        let b = push_state(&mut lx, None);
        lexc_eq_paths(&lx, a, b);
    }

    /* ---- direct API: merge_states --------------------------------------- */

    // Suffix merge shrinks the trie: cat + bat share tail "at", so two suffix
    // states are marked deleted (mergeable == 2); language preserved.
    // [spec:foma:sem:lexcread.lexc-merge-states-fn/test]
    #[test]
    fn merge_states_shared_suffix() {
        let mut lx = LexcCompiler::new_empty();
        lexc_init(&mut lx);
        lexc_set_current_lexicon(&mut lx, "Root", SOURCE_LEXICON);
        add_word_entry(&mut lx, "cat");
        add_word_entry(&mut lx, "bat");
        lexc_merge_states(&mut lx);
        let deleted = lx.state_arena.iter().filter(|s| s.mergeable == 2).count();
        assert_eq!(deleted, 2); // the "at" and "t" states of one branch
        // whole compile still yields both words
        let net = compile("LEXICON Root\ncat # ;\nbat # ;\n");
        assert_eq!(lower_all(&net), vec!["bat", "cat"]);
    }

    // Latent quirk repro (kept, benign): when the deleted cell is the statelist
    // head, the C keeps `statelist = s` (the removed cell) instead of s->next;
    // the arena keeps that head deterministically with its `next` intact. The
    // stray deleted state has no incoming arcs, so it is pruned downstream.
    // [spec:foma:sem:lexcread.lexc-merge-states-fn/test]
    #[test]
    fn merge_states_head_deleted_cell_kept() {
        let mut lx = LexcCompiler::new_empty();
        // lexicon terminal DEST = state 0
        lx.lexstates_arena.push(Lexstates {
            name: Some("#".into()),
            state: 0,
            next: None,
            targeted: 0,
            has_outgoing: 0,
        });
        let dest = push_state(&mut lx, Some(0)); // 0
        // survivor chain X0 -(a)-> X1 -(b)-> DEST
        let x1 = push_suffix(&mut lx, 20, 1, dest, 2, 2); // state 2, dist 1
        let x0 = push_suffix(&mut lx, 10, 2, x1, 1, 1); // state 1, dist 2
        // loser chain Y0 -(a)-> Y1 -(b)-> DEST (same labels/hashes)
        let y1 = push_suffix(&mut lx, 20, 1, dest, 2, 2); // dist 1
        let y0 = push_suffix(&mut lx, 10, 2, y1, 1, 1); // dist 2

        // statelist head = Y1 (a loser-chain interior node)
        let head = push_sl(&mut lx, y1); // cell 0
        let c_x0 = push_sl(&mut lx, x0); // cell 1
        let c_x1 = push_sl(&mut lx, x1); // cell 2
        let c_y0 = push_sl(&mut lx, y0); // cell 3
        let c_dest = push_sl(&mut lx, dest); // cell 4
        lx.statelist_arena[head].next = Some(c_x0);
        lx.statelist_arena[c_x0].next = Some(c_x1);
        lx.statelist_arena[c_x1].next = Some(c_y0);
        lx.statelist_arena[c_y0].next = Some(c_dest);
        lx.statelist = Some(head);
        lx.maxlen = 2;

        lexc_merge_states(&mut lx);

        // loser chain deleted, survivor kept, redirect recorded
        assert_eq!(lx.state_arena[y0].mergeable, 2);
        assert_eq!(lx.state_arena[y1].mergeable, 2);
        assert_eq!(lx.state_arena[x0].mergeable, 1);
        assert_eq!(lx.state_arena[y0].merge_with, x0);
        // the head still points at the removed cell, next intact
        assert_eq!(lx.statelist, Some(head));
        assert_eq!(lx.statelist_arena[head].next, Some(c_x0));
        // the interior loser cell was unlinked (X1 -> DEST)
        assert_eq!(lx.statelist_arena[c_x1].next, Some(c_dest));
    }

    /* ---- direct API: number_states -------------------------------------- */

    // Wave 4 fix: smax is now the total state count, so "#" gets the true last
    // number (smax-1) with no collision even when Root is not the first-created
    // state (the C computed smax as Root's list position and "#" collided with a
    // sequentially numbered state).
    // [spec:foma:sem:lexcread.lexc-number-states-fn+1/test]
    #[test]
    fn number_states_hash_no_collision() {
        let mut lx = LexcCompiler::new_empty();
        // three lexicon states: First(0), Root(1), #(2)
        for (i, nm) in ["First", "Root", "#"].iter().enumerate() {
            lx.lexstates_arena.push(Lexstates {
                name: Some((*nm).into()),
                state: i,
                next: None,
                targeted: 1,
                has_outgoing: 1,
            });
            let s = push_state(&mut lx, Some(i));
            assert_eq!(s, i);
        }
        lx.lexstates_arena[2].has_outgoing = 0; // '#' is a target-only leaf
        // statelist order [#, Root, First] (reverse creation)
        let c_h = push_sl(&mut lx, 2);
        let c_r = push_sl(&mut lx, 1);
        let c_f = push_sl(&mut lx, 0);
        lx.statelist_arena[c_h].next = Some(c_r);
        lx.statelist_arena[c_r].next = Some(c_f);
        lx.statelist = Some(c_h);
        // link lexstates list so the warning loop has something to walk
        lx.lexstates = Some(0);

        lexc_number_states(&mut lx);

        assert_eq!(lx.hasfinal, 1);
        assert_eq!(lx.state_arena[1].number, 0); // Root
        // smax = total state count = 3, so '#' = smax-1 = 2 (the true last number)
        assert_eq!(lx.state_arena[2].number, 2);
        // First is numbered sequentially to a DISTINCT value (no collision)
        assert_eq!(lx.state_arena[0].number, 1);
        assert_ne!(lx.state_arena[0].number, lx.state_arena[2].number);
    }

    /* ---- direct API: cleanup -------------------------------------------- */

    // Frees every compile table (arenas emptied), lexsigma left for the net.
    // [spec:foma:sem:lexcread.lexc-cleanup-fn/test]
    #[test]
    fn cleanup_empties_arenas() {
        let mut lx = LexcCompiler::new_empty();
        lexc_init(&mut lx);
        lexc_set_current_lexicon(&mut lx, "Root", SOURCE_LEXICON);
        add_word_entry(&mut lx, "cat");
        assert!(!lx.state_arena.is_empty());
        lexc_cleanup(&mut lx);
        assert!(lx.state_arena.is_empty());
        assert!(lx.trans_arena.is_empty());
        assert!(lx.statelist_arena.is_empty());
        assert!(lx.lexstates_arena.is_empty());
        assert!(lx.mc_arena.is_empty());
        assert!(lx.hashtable.is_empty());
        assert_eq!(lx.statelist, None);
    }

    /* ---- arena construction helpers (tests only) ------------------------ */

    fn push_state(lx: &mut LexcCompiler, lexstate: Option<usize>) -> usize {
        let idx = lx.state_arena.len();
        lx.state_arena.push(States {
            trans: None,
            lexstate,
            number: -1,
            hashval: 0,
            mergeable: 0,
            distance: 0,
            merge_with: idx,
        });
        idx
    }

    /* One mergeable suffix state carrying a single (in:out) arc to `target`. */
    fn push_suffix(
        lx: &mut LexcCompiler,
        hashval: u32,
        distance: u16,
        target: usize,
        r#in: i16,
        out: i16,
    ) -> usize {
        let sidx = lx.state_arena.len();
        lx.state_arena.push(States {
            trans: None,
            lexstate: None,
            number: -1,
            hashval,
            mergeable: 1,
            distance,
            merge_with: sidx,
        });
        let tidx = lx.trans_arena.len();
        lx.trans_arena.push(Trans {
            r#in,
            out,
            target,
            next: None,
        });
        lx.state_arena[sidx].trans = Some(tidx);
        sidx
    }

    /* A one-arc chain state (mergeable, distance 1) to a lexicon `target`. */
    fn push_chain(lx: &mut LexcCompiler, r#in: i16, out: i16, target: usize) -> usize {
        push_suffix(lx, 0, 1, target, r#in, out)
    }

    fn push_sl(lx: &mut LexcCompiler, state: usize) -> usize {
        let idx = lx.statelist_arena.len();
        lx.statelist_arena.push(Statelist {
            state,
            next: None,
            start: 0,
            r#final: 0,
        });
        idx
    }
}
