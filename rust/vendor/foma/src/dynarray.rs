//! foma/dynarray.c — Wave-4 idiomatization of the Wave-2 port per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/dynarray.md
//! (per-file ids) plus the fomalib.h / fomalibconf.h prototype ids.
//!
//! Two facilities live here:
//! - the fsm_state_* dynamic line-array builder. The C's family of file-static
//!   globals is folded into one owned `FsmBuilder` struct (methods take
//!   `&mut self`); the free `fsm_state_*` functions delegate to it. `fsm_state_init`
//!   returns the builder and the later `fsm_state_*` calls take `&mut FsmBuilder`,
//!   so each build is a self-contained handle threaded by its caller;
//! - the fsm_construct_* / fsm_read_* handle families for building and
//!   iterating networks.
//!
//! Interior pointers of the C (arcs_cursor, states_head entries, the
//! finals/initials cursors) are represented as indices per the conventions.
//! The fsm_get_next_state protocol parks arcs_cursor one line *before* the
//! state's first line (C: a pointer one element before the array position —
//! UB but works); here that park position is `index.wrapping_sub(1)` and
//! fsm_get_next_state_arc's pre-increment wraps it back.

use crate::mem::next_power_of_two;
use crate::sigma::{sigma_max, sigma_sort, sigma_to_list};
use crate::structures::{fsm_create, fsm_destroy, fsm_empty_set};
use crate::types::{
    EPSILON, Fsm, FsmConstructHandle, FsmReadHandle, FsmSigmaHash, FsmSigmaList, FsmState,
    FsmStateList, FsmTransList, IDENTITY, PATHCOUNT_UNKNOWN, Sigma, UNK, UNKNOWN,
};
use smol_str::SmolStr;

/* C: #define INITIAL_SIZE 16384 */
pub const INITIAL_SIZE: usize = 16384;
/* C: #define SIGMA_HASH_SIZE 1021 */
pub const SIGMA_HASH_SIZE: u32 = 1021;
/* C: #define MINSIGMA 3 */
pub const MINSIGMA: i32 = 3;

// [spec:foma:def:dynarray.foma-reserved-symbols]
pub struct FomaReservedSymbols {
    pub symbol: Option<&'static str>,
    pub number: i32,
    pub prints_as: Option<&'static str>,
}

/* C: the table is NULL-terminated; symbol == None is the terminator entry */
pub static FOMA_RESERVED_SYMBOLS: [FomaReservedSymbols; 4] = [
    FomaReservedSymbols {
        symbol: Some("@_EPSILON_SYMBOL_@"),
        number: EPSILON,
        prints_as: Some("0"),
    },
    FomaReservedSymbols {
        symbol: Some("@_UNKNOWN_SYMBOL_@"),
        number: UNKNOWN,
        prints_as: Some("?"),
    },
    FomaReservedSymbols {
        symbol: Some("@_IDENTITY_SYMBOL_@"),
        number: IDENTITY,
        prints_as: Some("@"),
    },
    FomaReservedSymbols {
        symbol: None,
        number: 0,
        prints_as: None,
    },
];

// [spec:foma:def:dynarray.sigma-lookup]
#[derive(Debug, Clone)]
pub struct SigmaLookup {
    pub target: i32,
    pub mainloop: u32,
}

/// Owns one in-progress `fsm_state_*` line-array build. The C kept a family
/// of file-static globals (`current_fsm_head`, `current_fsm_linecount`,
/// `slookup`, the arity/count/flag scratch, `mainloop`, ...); these are folded
/// into this struct with `&mut self` methods. The free `fsm_state_*` functions
/// delegate to it: `fsm_state_init` returns the builder and the rest take
/// `&mut FsmBuilder`, so each build is a self-contained handle.
#[derive(Debug)]
pub struct FsmBuilder {
    current_fsm_size: usize,
    current_fsm_linecount: u32,
    current_state_no: u32,
    current_final: u32,
    current_start: u32,
    current_trans: u32,
    num_finals: u32,
    num_initials: u32,
    arity: u32,
    statecount: u32,
    is_deterministic: bool,
    is_epsilon_free: bool,
    current_fsm_head: Vec<FsmState>,
    mainloop: u32,
    ssize: u32,
    arccount: u32,
    slookup: Vec<SigmaLookup>,
}

impl FsmBuilder {
    /// `fsm_state_init`: begin a build sized for symbol numbers `0..=sigma_size`.
    pub fn new(sigma_size: i32) -> Self {
        let ssize = (sigma_size + 1) as u32;
        FsmBuilder {
            current_fsm_head: Vec::with_capacity(INITIAL_SIZE),
            current_fsm_size: INITIAL_SIZE,
            current_fsm_linecount: 0,
            ssize,
            slookup: vec![
                SigmaLookup {
                    target: 0,
                    mainloop: 0,
                };
                (ssize as usize) * (ssize as usize)
            ],
            mainloop: 1,
            is_deterministic: true,
            is_epsilon_free: true,
            arccount: 0,
            num_finals: 0,
            num_initials: 0,
            statecount: 0,
            arity: 1,
            current_trans: 1,
            current_state_no: 0,
            current_final: 0,
            current_start: 0,
        }
    }

    /// `fsm_state_set_current_state`.
    pub fn set_current_state(&mut self, state_no: i32, final_state: i32, start_state: i32) {
        /* the counters are unsigned; C's int→unsigned conversion wraps */
        self.current_state_no = state_no as u32;
        self.current_final = final_state as u32;
        self.current_start = start_state as u32;
        self.current_trans = 0;
        /* counts only the exact value 1 — other nonzero flags are stored
        but not counted */
        if self.current_final == 1 {
            self.num_finals += 1;
        }
        if self.current_start == 1 {
            self.num_initials += 1;
        }
    }

    /// `fsm_state_end_state`: synthesize a placeholder line if the state
    /// emitted nothing, then advance the state/dedup bookkeeping.
    pub fn end_state(&mut self) {
        if self.current_trans == 0 {
            self.add_arc(
                self.current_state_no as i32,
                -1,
                -1,
                -1,
                self.current_final as i32,
                self.current_start as i32,
            );
        }
        self.statecount += 1;
        /* invalidates all slookup duplicate-detection stamps for the next state */
        self.mainloop += 1;
    }

    /// `fsm_state_add_arc`: append one arc (or sentinel) line.
    pub fn add_arc(
        &mut self,
        state_no: i32,
        r#in: i32,
        out: i32,
        target: i32,
        final_state: i32,
        start_state: i32,
    ) {
        if r#in != out {
            self.arity = 2;
        }
        /* Check epsilon moves */
        if r#in == EPSILON && out == EPSILON {
            if state_no == target {
                return;
            }
            self.is_deterministic = false;
            self.is_epsilon_free = false;
        }

        /* Check if we already added this particular arc and skip; also check
        if the net becomes non-deterministic. slookup cell at ssize*in + out,
        stamped per state via mainloop. Quirk (kept): a same-label arc with a
        *different* target overwrites the cell's target, so a third same-label
        arc repeating the FIRST target is no longer seen as a duplicate and is
        emitted twice. */
        if r#in != -1 && out != -1 {
            let idx = (self.ssize as usize) * (r#in as usize) + (out as usize);
            if self.slookup[idx].mainloop == self.mainloop {
                if self.slookup[idx].target == target {
                    /* exact duplicate (in,out,target): silently dropped */
                    return;
                }
                self.is_deterministic = false;
            }
            self.arccount += 1;
            self.slookup[idx].mainloop = self.mainloop;
            self.slookup[idx].target = target;
        }

        self.current_trans = 1;
        if self.current_fsm_linecount as usize >= self.current_fsm_size {
            /* C doubled a realloc here; Vec growth is implicit — the size
            counter is kept only to mirror the C bookkeeping. */
            self.current_fsm_size *= 2;
        }
        /* in/out truncate int→short, final/start truncate int→char as in C */
        self.current_fsm_head.push(FsmState {
            state_no,
            r#in: r#in as i16,
            out: out as i16,
            target,
            final_state: final_state as i8,
            start_state: start_state as i8,
        });
        self.current_fsm_linecount += 1;
    }

    /// `fsm_state_close`: append the sentinel line and install the built
    /// table and its counts/flags into `net`.
    pub fn close(&mut self, net: &mut Fsm) {
        /* array terminator line */
        self.add_arc(-1, -1, -1, -1, -1, -1);
        let mut states = std::mem::take(&mut self.current_fsm_head);
        states.shrink_to_fit();
        net.arity = self.arity as i32;
        net.arccount = self.arccount as i32;
        net.statecount = self.statecount as i32;
        net.linecount = self.current_fsm_linecount as i32;
        net.finalcount = self.num_finals as i32;
        net.pathcount = PATHCOUNT_UNKNOWN;
        if self.num_initials > 1 {
            self.is_deterministic = false;
        }
        net.is_deterministic = self.is_deterministic as i32;
        net.is_pruned = UNK;
        net.is_minimized = UNK;
        net.is_epsilon_free = self.is_epsilon_free as i32;
        net.is_loop_free = UNK;
        net.is_completed = UNK;
        net.arcs_sorted_in = 0;
        net.arcs_sorted_out = 0;
        net.states = states;
        /* free(slookup) */
        self.slookup = Vec::new();
    }
}

/// The C library `rand()`/`srand()` state (the ISO C sample LCG). C foma
/// relied on the single process-global libc state, seeded once by
/// `apply_init` with `srand(time(NULL))` and otherwise left at the ISO C
/// default of 1; the port has no libc dependency, so each owner (an
/// `ApplyHandle`, a `Session`, or a one-shot `fsm_construct_done` build)
/// carries its own `Lcg`. Only affects random enumeration order and the
/// random hex names given to unnamed constructed nets.
#[derive(Debug, Clone)]
pub struct Lcg {
    next: u64,
}

impl Lcg {
    /// ISO C default seed (equivalent to a program that never calls srand).
    pub fn new() -> Lcg {
        Lcg { next: 1 }
    }

    /// C library `srand`: reseed the LCG (`next = seed`).
    pub fn srand(&mut self, seed: u32) {
        self.next = seed as u64;
    }

    /// C library `rand` (ISO C sample implementation).
    pub fn rand(&mut self) -> i32 {
        self.next = self.next.wrapping_mul(1103515245).wrapping_add(12345);
        ((self.next / 65536) % 32768) as i32
    }
}

impl Default for Lcg {
    fn default() -> Self {
        Lcg::new()
    }
}

/* Functions for directly building a fsm_state structure */
/* dynamically. */

/* fsm_state_init() is called when a new machine is constructed */

/* fsm_state_add_arc(&mut b, ) adds an arc and possibly reallocs the array */

/* fsm_state_close() adds the sentinel entry and clears values */

// [spec:foma:def:dynarray.fsm-state-init-fn]
// [spec:foma:sem:dynarray.fsm-state-init-fn]
// [spec:foma:def:fomalibconf.fsm-state-init-fn]
// [spec:foma:sem:fomalibconf.fsm-state-init-fn]
pub fn fsm_state_init(sigma_size: i32) -> FsmBuilder {
    // C returned the malloc'd array pointer (also retained in a family of
    // file-static globals); the port hands the caller the owned builder that
    // the subsequent fsm_state_* calls thread as `&mut`.
    FsmBuilder::new(sigma_size)
}

// [spec:foma:def:dynarray.fsm-state-set-current-state-fn]
// [spec:foma:sem:dynarray.fsm-state-set-current-state-fn]
// [spec:foma:def:fomalibconf.fsm-state-set-current-state-fn]
// [spec:foma:sem:fomalibconf.fsm-state-set-current-state-fn]
pub fn fsm_state_set_current_state(
    b: &mut FsmBuilder,
    state_no: i32,
    final_state: i32,
    start_state: i32,
) {
    b.set_current_state(state_no, final_state, start_state);
}

/* Add sentinel if needed */
// [spec:foma:def:dynarray.fsm-state-end-state-fn]
// [spec:foma:sem:dynarray.fsm-state-end-state-fn]
// [spec:foma:def:fomalibconf.fsm-state-end-state-fn]
// [spec:foma:sem:fomalibconf.fsm-state-end-state-fn]
pub fn fsm_state_end_state(b: &mut FsmBuilder) {
    b.end_state();
}

// [spec:foma:def:dynarray.fsm-state-add-arc-fn]
// [spec:foma:sem:dynarray.fsm-state-add-arc-fn]
// [spec:foma:def:fomalibconf.fsm-state-add-arc-fn]
// [spec:foma:sem:fomalibconf.fsm-state-add-arc-fn]
pub fn fsm_state_add_arc(
    b: &mut FsmBuilder,
    state_no: i32,
    r#in: i32,
    out: i32,
    target: i32,
    final_state: i32,
    start_state: i32,
) {
    b.add_arc(state_no, r#in, out, target, final_state, start_state);
}

// [spec:foma:def:dynarray.fsm-state-close-fn]
// [spec:foma:sem:dynarray.fsm-state-close-fn]
// [spec:foma:def:fomalibconf.fsm-state-close-fn]
// [spec:foma:sem:fomalibconf.fsm-state-close-fn]
pub fn fsm_state_close(b: &mut FsmBuilder, net: &mut Fsm) {
    b.close(net);
}

/* Construction functions */

// [spec:foma:def:dynarray.fsm-construct-init-fn]
// [spec:foma:sem:dynarray.fsm-construct-init-fn]
// [spec:foma:def:fomalib.fsm-construct-init-fn]
// [spec:foma:sem:fomalib.fsm-construct-init-fn]
pub fn fsm_construct_init(name: &str) -> Box<FsmConstructHandle> {
    Box::new(FsmConstructHandle {
        /* calloc(1024, ...) — zeroed entries */
        fsm_state_list: vec![
            FsmStateList {
                used: false,
                is_final: false,
                is_initial: false,
                num_trans: 0,
                state_number: 0,
                fsm_trans_list: None,
            };
            1024
        ],
        fsm_state_list_size: 1024,
        fsm_sigma_list: vec![FsmSigmaList { symbol: None }; 1024],
        fsm_sigma_list_size: 1024,
        /* calloc(SIGMA_HASH_SIZE, ...) — symbol == None marks an empty bucket */
        fsm_sigma_hash: vec![
            FsmSigmaHash {
                symbol: None,
                sym: 0,
                next: None,
            };
            SIGMA_HASH_SIZE as usize
        ],
        /* C never initializes this field (malloc'd handle → garbage; the
        field is never read anywhere) */
        fsm_sigma_hash_size: 0,
        maxstate: -1,
        maxsigma: -1,
        numfinals: 0,
        /* C: name == NULL → handle->name = NULL; a &str cannot be NULL and
        no in-tree caller passes NULL */
        name: Some(name.into()),
        hasinitial: 0,
    })
}

// [spec:foma:def:dynarray.fsm-construct-check-size-fn]
// [spec:foma:sem:dynarray.fsm-construct-check-size-fn]
pub fn fsm_construct_check_size(handle: &mut FsmConstructHandle, state_no: i32) {
    let oldsize = handle.fsm_state_list_size;
    if oldsize <= state_no {
        let newsize = next_power_of_two(state_no);
        /* C: realloc leaves the grown region uninitialized; the loop below
        then initializes exactly oldsize..newsize (Vec::resize fills the
        same defaults first — observably identical) */
        handle.fsm_state_list.resize(
            newsize as usize,
            FsmStateList {
                used: false,
                is_final: false,
                is_initial: false,
                num_trans: 0,
                state_number: 0,
                fsm_trans_list: None,
            },
        );
        handle.fsm_state_list_size = newsize;
        for i in oldsize..newsize {
            let sl = &mut handle.fsm_state_list[i as usize];
            sl.is_final = false;
            sl.is_initial = false;
            sl.used = false;
            sl.num_trans = 0;
            sl.fsm_trans_list = None;
        }
    }
}

// [spec:foma:def:dynarray.fsm-construct-set-final-fn]
// [spec:foma:sem:dynarray.fsm-construct-set-final-fn]
// [spec:foma:def:fomalib.fsm-construct-set-final-fn]
// [spec:foma:sem:fomalib.fsm-construct-set-final-fn]
pub fn fsm_construct_set_final(handle: &mut FsmConstructHandle, state_no: i32) {
    fsm_construct_check_size(handle, state_no);

    if state_no > handle.maxstate {
        handle.maxstate = state_no;
    }

    if !handle.fsm_state_list[state_no as usize].is_final {
        handle.fsm_state_list[state_no as usize].is_final = true;
        handle.numfinals += 1;
    }
}

// [spec:foma:def:dynarray.fsm-construct-set-initial-fn]
// [spec:foma:sem:dynarray.fsm-construct-set-initial-fn]
// [spec:foma:def:fomalib.fsm-construct-set-initial-fn]
// [spec:foma:sem:fomalib.fsm-construct-set-initial-fn]
pub fn fsm_construct_set_initial(handle: &mut FsmConstructHandle, state_no: i32) {
    fsm_construct_check_size(handle, state_no);

    if state_no > handle.maxstate {
        handle.maxstate = state_no;
    }

    handle.fsm_state_list[state_no as usize].is_initial = true;
    handle.hasinitial = 1;
}

// [spec:foma:def:dynarray.fsm-construct-add-arc-fn]
// [spec:foma:sem:dynarray.fsm-construct-add-arc-fn]
// [spec:foma:def:fomalib.fsm-construct-add-arc-fn]
// [spec:foma:sem:fomalib.fsm-construct-add-arc-fn]
pub fn fsm_construct_add_arc(
    handle: &mut FsmConstructHandle,
    source: i32,
    target: i32,
    r#in: &str,
    out: &str,
) {
    fsm_construct_check_size(handle, source);
    fsm_construct_check_size(handle, target);

    if source > handle.maxstate {
        handle.maxstate = source;
    }
    if target > handle.maxstate {
        handle.maxstate = target;
    }

    handle.fsm_state_list[target as usize].used = true;
    handle.fsm_state_list[source as usize].used = true;
    /* C mallocs the node and prepends it to source's list *before*
    resolving the labels, filling the fields afterwards; the labels are
    resolved first here (check/add only touch the sigma list/hash —
    observably equivalent). num_trans is not updated, as in C. */
    let mut symin = fsm_construct_check_symbol(handle, r#in);
    if symin == -1 {
        symin = fsm_construct_add_symbol(handle, r#in);
    }
    let mut symout = fsm_construct_check_symbol(handle, out);
    if symout == -1 {
        symout = fsm_construct_add_symbol(handle, out);
    }
    let sl = &mut handle.fsm_state_list[source as usize];
    let tl = Box::new(FsmTransList {
        /* int→short truncation as in C */
        r#in: symin as i16,
        out: symout as i16,
        target,
        next: sl.fsm_trans_list.take(),
    });
    sl.fsm_trans_list = Some(tl);
}

// [spec:foma:def:dynarray.fsm-construct-hash-sym-fn]
// [spec:foma:sem:dynarray.fsm-construct-hash-sym-fn]
pub fn fsm_construct_hash_sym(symbol: &str) -> u32 {
    let mut hash: u32 = 0;

    /* C sums plain `char` values: on signed-char platforms bytes >= 0x80
    add sign-extended negative values, wrapping the unsigned sum */
    for b in symbol.as_bytes() {
        hash = hash.wrapping_add((*b as i8 as i32) as u32);
    }
    hash % SIGMA_HASH_SIZE
}

// [spec:foma:def:dynarray.fsm-construct-add-arc-nums-fn]
// [spec:foma:sem:dynarray.fsm-construct-add-arc-nums-fn]
// [spec:foma:def:fomalib.fsm-construct-add-arc-nums-fn]
// [spec:foma:sem:fomalib.fsm-construct-add-arc-nums-fn]
pub fn fsm_construct_add_arc_nums(
    handle: &mut FsmConstructHandle,
    source: i32,
    target: i32,
    r#in: i32,
    out: i32,
) {
    fsm_construct_check_size(handle, source);
    fsm_construct_check_size(handle, target);

    if source > handle.maxstate {
        handle.maxstate = source;
    }
    if target > handle.maxstate {
        handle.maxstate = target;
    }

    handle.fsm_state_list[target as usize].used = true;
    let sl = &mut handle.fsm_state_list[source as usize];
    sl.used = true;
    /* no sigma lookup or insertion: the caller must guarantee the numbers
    have symbol entries. num_trans is not updated, as in C. */
    let tl = Box::new(FsmTransList {
        /* int→short truncation as in C */
        r#in: r#in as i16,
        out: out as i16,
        target,
        next: sl.fsm_trans_list.take(),
    });
    sl.fsm_trans_list = Some(tl);
}

/* Copies entire alphabet from existing network */

// [spec:foma:def:dynarray.fsm-construct-copy-sigma-fn]
// [spec:foma:sem:dynarray.fsm-construct-copy-sigma-fn+1]
// [spec:foma:def:fomalib.fsm-construct-copy-sigma-fn]
// [spec:foma:sem:fomalib.fsm-construct-copy-sigma-fn+1]
pub fn fsm_construct_copy_sigma(handle: &mut FsmConstructHandle, sigma: &[Sigma]) {
    for s in sigma {
        let symnum = s.number;
        if symnum > handle.maxsigma {
            handle.maxsigma = symnum;
        }
        let symbol = s.symbol.as_str();
        if symnum >= handle.fsm_sigma_list_size {
            // [spec:foma:sem:dynarray.fsm-construct-copy-sigma-fn+1] grow until the
            // slot fits. C did a single doubling keyed on the current size, so a
            // number >= twice the size still overflowed the array (OOB write in C,
            // index panic here). New slots are not zero-initialized in C (None here).
            while symnum >= handle.fsm_sigma_list_size {
                handle.fsm_sigma_list_size = next_power_of_two(handle.fsm_sigma_list_size);
            }
            handle.fsm_sigma_list.resize(
                handle.fsm_sigma_list_size as usize,
                FsmSigmaList { symbol: None },
            );
        }
        /* Insert into list */
        /* C shares one strdup between the list slot and the hash node;
        cheap SmolStr clones of one copy here (observably equivalent) */
        let symdup: SmolStr = symbol.into();
        handle.fsm_sigma_list[symnum as usize].symbol = Some(symdup.clone());

        /* Insert into hashtable */
        let hash = fsm_construct_hash_sym(symbol);
        let fh = &mut handle.fsm_sigma_hash[hash as usize];
        if fh.symbol.is_none() {
            fh.symbol = Some(symdup);
            fh.sym = symnum as i16;
        } else {
            /* calloc'd chain node spliced directly after the head */
            let newfh = Box::new(FsmSigmaHash {
                symbol: Some(symdup),
                sym: symnum as i16,
                next: fh.next.take(),
            });
            fh.next = Some(newfh);
        }
    }
}

// [spec:foma:def:dynarray.fsm-construct-add-symbol-fn]
// [spec:foma:sem:dynarray.fsm-construct-add-symbol-fn]
// [spec:foma:def:fomalib.fsm-construct-add-symbol-fn]
// [spec:foma:sem:fomalib.fsm-construct-add-symbol-fn]
pub fn fsm_construct_add_symbol(handle: &mut FsmConstructHandle, symbol: &str) -> i32 {
    /* no duplicate check: adding an existing symbol allocates a fresh
    number — callers probe with fsm_construct_check_symbol first */
    let mut symnum = 0;
    let mut reserved = 0;

    /* Is symbol reserved? */
    let mut i = 0;
    while let Some(reserved_sym) = FOMA_RESERVED_SYMBOLS[i].symbol {
        if symbol == reserved_sym {
            symnum = FOMA_RESERVED_SYMBOLS[i].number;
            reserved = 1;
            if handle.maxsigma < symnum {
                handle.maxsigma = symnum;
            }
            break;
        }
        i += 1;
    }

    if reserved == 0 {
        symnum = handle.maxsigma + 1;
        if symnum < MINSIGMA {
            symnum = MINSIGMA;
        }
        handle.maxsigma = symnum;
    }

    if symnum >= handle.fsm_sigma_list_size {
        /* single growth step keyed on the current size (doubles a
        power-of-two size); new slots are not zero-initialized in C */
        handle.fsm_sigma_list_size = next_power_of_two(handle.fsm_sigma_list_size);
        // DEVIATION from C (OOB write when symnum >= the doubled size; Rust panics on the index below)
        handle.fsm_sigma_list.resize(
            handle.fsm_sigma_list_size as usize,
            FsmSigmaList { symbol: None },
        );
    }
    /* Insert into list */
    /* C shares one strdup between the list slot and the hash node;
    cheap SmolStr clones of one copy here (observably equivalent) */
    let symdup: SmolStr = symbol.into();
    handle.fsm_sigma_list[symnum as usize].symbol = Some(symdup.clone());

    /* Insert into hashtable */
    let hash = fsm_construct_hash_sym(symbol);
    let fh = &mut handle.fsm_sigma_hash[hash as usize];
    if fh.symbol.is_none() {
        fh.symbol = Some(symdup);
        fh.sym = symnum as i16;
    } else {
        /* calloc'd chain node spliced directly after the head */
        let newfh = Box::new(FsmSigmaHash {
            symbol: Some(symdup),
            sym: symnum as i16,
            next: fh.next.take(),
        });
        fh.next = Some(newfh);
    }
    symnum
}

// [spec:foma:def:dynarray.fsm-construct-check-symbol-fn]
// [spec:foma:sem:dynarray.fsm-construct-check-symbol-fn]
// [spec:foma:def:fomalib.fsm-construct-check-symbol-fn]
// [spec:foma:sem:fomalib.fsm-construct-check-symbol-fn]
pub fn fsm_construct_check_symbol(handle: &FsmConstructHandle, symbol: &str) -> i32 {
    /* C: int hash (the unsigned return converted to int) */
    let hash = fsm_construct_hash_sym(symbol) as i32;
    if handle.fsm_sigma_hash[hash as usize].symbol.is_none() {
        return -1;
    }
    let mut fh = Some(&handle.fsm_sigma_hash[hash as usize]);
    while let Some(node) = fh {
        if node.symbol.as_deref() == Some(symbol) {
            /* short→int promotion */
            return node.sym as i32;
        }
        fh = node.next.as_deref();
    }
    -1
}

// [spec:foma:def:dynarray.fsm-construct-convert-sigma-fn]
// [spec:foma:sem:dynarray.fsm-construct-convert-sigma-fn]
pub fn fsm_construct_convert_sigma(handle: &FsmConstructHandle) -> Vec<Sigma> {
    /* builds the alphabet in ascending symbol-number order, appending at the
    tail; NULL-symbol slots are skipped */
    let mut sigma: Vec<Sigma> = Vec::new();
    for i in 0..=handle.maxsigma {
        if let Some(symbol) = &handle.fsm_sigma_list[i as usize].symbol {
            /* C moves the char* out of fsm_sigma_list (no strdup) —
            ownership transfers to the sigma; cloned here since the handle
            is not mutable (observably equivalent: the handle's list is
            freed without freeing the strings) */
            sigma.push(Sigma {
                number: i,
                symbol: symbol.clone(),
            });
        }
    }
    sigma
}

// [spec:foma:def:dynarray.fsm-construct-done-fn]
// [spec:foma:sem:dynarray.fsm-construct-done-fn+1]
// [spec:foma:def:fomalib.fsm-construct-done-fn]
// [spec:foma:sem:fomalib.fsm-construct-done-fn+1]
pub fn fsm_construct_done(handle: Box<FsmConstructHandle>) -> Box<Fsm> {
    let mut handle = handle;
    if handle.maxstate == -1 || handle.numfinals == 0 || handle.hasinitial == 0 {
        // C leaked the handle and its contents on this early-return path;
        // Rust drops them.
        return fsm_empty_set();
    }
    let mut b = fsm_state_init(handle.maxsigma + 1);

    /* C read the process-global libc rand() state here (seeded only if an
    apply_init ran earlier). A one-shot LCG at the ISO C default stands in;
    the value it names is always overwritten below by handle->name. */
    let mut lcg = Lcg::new();

    /* emptyfsm tracks whether the FSM has (a) something outgoing from an
    initial state, or (b) an initial state that is final */
    let mut emptyfsm = 1;
    for i in 0..=handle.maxstate {
        let sl = &handle.fsm_state_list[i as usize];
        fsm_state_set_current_state(&mut b, i, sl.is_final as i32, sl.is_initial as i32);
        if sl.is_initial && sl.is_final {
            emptyfsm = 0;
        }
        /* transition list is walked in reverse insertion order (LIFO) */
        let mut trans = sl.fsm_trans_list.as_deref();
        while let Some(t) = trans {
            if sl.is_initial {
                emptyfsm = 0;
            }
            /* short→int promotion on in/out */
            fsm_state_add_arc(
                &mut b,
                i,
                t.r#in as i32,
                t.out as i32,
                t.target,
                sl.is_final as i32,
                sl.is_initial as i32,
            );
            trans = t.next.as_deref();
        }
        fsm_state_end_state(&mut b);
    }
    let mut net = fsm_create("");
    net.name = format!("{:X}", lcg.rand()).into();
    /* free(net->sigma) */
    net.sigma = Vec::new();
    fsm_state_close(&mut b, &mut net);

    net.sigma = fsm_construct_convert_sigma(&handle);
    if let Some(name) = handle.name.take() {
        /* C: strncpy(net->name, handle->name, 40) — the 40-byte cap was the
        struct name-buffer size, gone now that names are heap Strings. */
        net.name = name;
        /* free(handle->name) — dropped with the take() above */
    } else {
        net.name = format!("{:X}", lcg.rand()).into();
    }

    /* Free transitions (all fsm_state_list_size slots), the sigma-hash
    chain nodes, fsm_sigma_list, fsm_sigma_hash, fsm_state_list, and the
    handle itself — all dropped with `handle` here. The symbol strings
    now belong to net->sigma. */
    drop(handle);
    sigma_sort(&mut net);
    if emptyfsm != 0 {
        fsm_destroy(net);
        return fsm_empty_set();
    }
    net
}

/* Reading functions */

// [spec:foma:def:dynarray.fsm-read-is-final-fn]
// [spec:foma:sem:dynarray.fsm-read-is-final-fn]
// [spec:foma:def:fomalib.fsm-read-is-final-fn]
// [spec:foma:sem:fomalib.fsm-read-is-final-fn]
pub fn fsm_read_is_final(h: &FsmReadHandle, state: i32) -> bool {
    /* no bounds check on state in C (OOB read); Rust panics */
    (h.lookuptable[state as usize] & 2) != 0
}

// [spec:foma:def:dynarray.fsm-read-is-initial-fn]
// [spec:foma:sem:dynarray.fsm-read-is-initial-fn]
// [spec:foma:def:fomalib.fsm-read-is-initial-fn]
// [spec:foma:sem:fomalib.fsm-read-is-initial-fn]
pub fn fsm_read_is_initial(h: &FsmReadHandle, state: i32) -> bool {
    /* no bounds check on state in C (OOB read); Rust panics */
    (h.lookuptable[state as usize] & 1) != 0
}

// [spec:foma:def:dynarray.fsm-read-init-fn]
// [spec:foma:sem:dynarray.fsm-read-init-fn]
// [spec:foma:def:fomalib.fsm-read-init-fn]
// [spec:foma:sem:fomalib.fsm-read-init-fn]
pub fn fsm_read_init(net: Box<Fsm>) -> Box<FsmReadHandle> {
    // DEVIATION from C (the C handle borrows the caller's net pointer; the
    // Rust handle owns the net for its lifetime and fsm_read_done returns it)
    let num_states = net.statecount;
    let mut lookuptable: Vec<u8> = vec![0; num_states as usize];

    let mut num_initials = 0;
    let mut num_finals = 0;

    /* calloc(num_states+1, sizeof(struct fsm **)) */
    let mut states_head: Vec<Option<usize>> = vec![None; (num_states + 1) as usize];
    let mut has_unknowns = false;

    let mut laststate = -1;
    let fsm = &net.states;
    let mut i = 0usize;
    while fsm[i].state_no != -1 {
        let sno = fsm[i].state_no;
        if fsm[i].start_state != 0 {
            /* lookuptable and states_head are sized by statecount but
            indexed by state_no: sparse state numbering writes OOB in C.
            DEVIATION from C (OOB write; Rust panics) */
            if lookuptable[sno as usize] & 1 == 0 {
                lookuptable[sno as usize] |= 1;
                num_initials += 1;
            }
        }
        if fsm[i].final_state != 0 && lookuptable[sno as usize] & 2 == 0 {
            lookuptable[sno as usize] |= 2;
            num_finals += 1;
        }
        if fsm[i].r#in as i32 == UNKNOWN
            || fsm[i].out as i32 == UNKNOWN
            || fsm[i].r#in as i32 == IDENTITY
            || fsm[i].out as i32 == IDENTITY
        {
            has_unknowns = true;
        }
        if fsm[i].state_no != laststate {
            /* pointer to the state's first line → index */
            states_head[fsm[i].state_no as usize] = Some(i);
        }
        laststate = fsm[i].state_no;
        i += 1;
    }

    let mut finals_head: Vec<i32> = vec![0; (num_finals + 1) as usize];
    let mut initials_head: Vec<i32> = vec![0; (num_initials + 1) as usize];

    let mut j = 0usize;
    let mut k = 0usize;
    for i in 0..num_states {
        if lookuptable[i as usize] & 1 != 0 {
            initials_head[j] = i;
            j += 1;
        }
        if lookuptable[i as usize] & 2 != 0 {
            finals_head[k] = i;
            k += 1;
        }
    }
    initials_head[j] = -1;
    finals_head[k] = -1;

    let fsm_sigma_list = sigma_to_list(&net.sigma);
    let sigma_list_size = sigma_max(&net.sigma) + 1;

    /* handle = calloc(1, ...): all cursors NULL, current_state 0 */
    Box::new(FsmReadHandle {
        finals_head,
        initials_head,
        states_head,
        fsm_sigma_list,
        sigma_list_size,
        /* arcs_head = fsm (base of net->states) → index 0 */
        arcs_head: 0,
        arcs_cursor: None,
        finals_cursor: None,
        states_cursor: None,
        initials_cursor: None,
        current_state: 0,
        lookuptable,
        has_unknowns,
        net: Some(net),
    })
}

// [spec:foma:def:dynarray.fsm-read-reset-fn]
// [spec:foma:sem:dynarray.fsm-read-reset-fn]
// [spec:foma:def:fomalib.fsm-read-reset-fn]
// [spec:foma:sem:fomalib.fsm-read-reset-fn]
pub fn fsm_read_reset(handle: Option<&mut FsmReadHandle>) {
    let Some(handle) = handle else {
        return;
    };
    handle.arcs_cursor = None;
    handle.initials_cursor = None;
    handle.finals_cursor = None;
    handle.states_cursor = None;
}

// [spec:foma:def:dynarray.fsm-get-next-state-arc-fn]
// [spec:foma:sem:dynarray.fsm-get-next-state-arc-fn]
// [spec:foma:def:fomalib.fsm-get-next-state-arc-fn]
// [spec:foma:sem:fomalib.fsm-get-next-state-arc-fn]
pub fn fsm_get_next_state_arc(handle: &mut FsmReadHandle) -> i32 {
    /* pre-increment: fsm_get_next_state parked the cursor one line before
    the state's first line (wrapping_sub(1); see module docs). Calling
    this with a NULL cursor is a crash in C — unwrap panics. */
    let cursor = handle
        .arcs_cursor
        .expect("arcs_cursor parked by fsm_get_next_state")
        .wrapping_add(1);
    handle.arcs_cursor = Some(cursor);
    let states = &handle
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .states;
    if states[cursor].state_no != handle.current_state || states[cursor].target == -1 {
        handle.arcs_cursor = Some(cursor.wrapping_sub(1));
        return 0;
    }
    1
}

// [spec:foma:def:dynarray.fsm-get-next-arc-fn]
// [spec:foma:sem:dynarray.fsm-get-next-arc-fn]
// [spec:foma:def:fomalib.fsm-get-next-arc-fn]
// [spec:foma:sem:fomalib.fsm-get-next-arc-fn]
pub fn fsm_get_next_arc(handle: &mut FsmReadHandle) -> i32 {
    let states = &handle
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .states;
    if handle.arcs_cursor.is_none() {
        let mut cursor = handle.arcs_head;
        /* skip sentinel lines (target == -1) */
        while states[cursor].state_no != -1 && states[cursor].target == -1 {
            cursor += 1;
        }
        handle.arcs_cursor = Some(cursor);
        if states[cursor].state_no == -1 {
            return 0;
        }
    } else if let Some(mut cursor) = handle.arcs_cursor {
        /* sticky terminator: once on the state_no == -1 line, keep
        returning 0 without moving */
        if states[cursor].state_no == -1 {
            return 0;
        }
        loop {
            cursor += 1;
            if !(states[cursor].state_no != -1 && states[cursor].target == -1) {
                break;
            }
        }
        handle.arcs_cursor = Some(cursor);
        if states[cursor].state_no == -1 {
            return 0;
        }
    }
    1
}

// [spec:foma:def:dynarray.fsm-get-arc-source-fn]
// [spec:foma:sem:dynarray.fsm-get-arc-source-fn]
// [spec:foma:def:fomalib.fsm-get-arc-source-fn]
// [spec:foma:sem:fomalib.fsm-get-arc-source-fn]
pub fn fsm_get_arc_source(handle: &FsmReadHandle) -> i32 {
    let Some(cursor) = handle.arcs_cursor else {
        return -1;
    };
    handle
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .states[cursor]
        .state_no
}

// [spec:foma:def:dynarray.fsm-get-arc-target-fn]
// [spec:foma:sem:dynarray.fsm-get-arc-target-fn]
// [spec:foma:def:fomalib.fsm-get-arc-target-fn]
// [spec:foma:sem:fomalib.fsm-get-arc-target-fn]
pub fn fsm_get_arc_target(handle: &FsmReadHandle) -> i32 {
    let Some(cursor) = handle.arcs_cursor else {
        return -1;
    };
    handle
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .states[cursor]
        .target
}

// [spec:foma:def:dynarray.fsm-get-symbol-number-fn]
// [spec:foma:sem:dynarray.fsm-get-symbol-number-fn]
// [spec:foma:def:fomalib.fsm-get-symbol-number-fn]
// [spec:foma:sem:fomalib.fsm-get-symbol-number-fn]
pub fn fsm_get_symbol_number(handle: &FsmReadHandle, symbol: &str) -> i32 {
    for i in 0..handle.sigma_list_size {
        if handle.fsm_sigma_list[i as usize].symbol.is_none() {
            continue;
        }
        if handle.fsm_sigma_list[i as usize].symbol.as_deref() == Some(symbol) {
            return i;
        }
    }
    -1
}

// [spec:foma:def:dynarray.fsm-get-arc-in-fn]
// [spec:foma:sem:dynarray.fsm-get-arc-in-fn]
// [spec:foma:def:fomalib.fsm-get-arc-in-fn]
// [spec:foma:sem:fomalib.fsm-get-arc-in-fn]
pub fn fsm_get_arc_in(handle: &FsmReadHandle) -> Option<&str> {
    /* C returns a borrowed char* into the handle's sigma list, or NULL
    when the cursor is NULL */
    let cursor = handle.arcs_cursor?;
    let index = handle
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .states[cursor]
        .r#in;
    /* no sentinel check: in == -1 indexes out of bounds in C.
    DEVIATION from C (OOB read; Rust panics) */
    handle.fsm_sigma_list[index as usize].symbol.as_deref()
}

// [spec:foma:def:dynarray.fsm-get-arc-num-in-fn]
// [spec:foma:sem:dynarray.fsm-get-arc-num-in-fn]
// [spec:foma:def:fomalib.fsm-get-arc-num-in-fn]
// [spec:foma:sem:fomalib.fsm-get-arc-num-in-fn]
pub fn fsm_get_arc_num_in(handle: &FsmReadHandle) -> i32 {
    let Some(cursor) = handle.arcs_cursor else {
        return -1;
    };
    /* short→int promotion; a sentinel line's stored -1 returns as-is */
    handle
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .states[cursor]
        .r#in as i32
}

// [spec:foma:def:dynarray.fsm-get-arc-num-out-fn]
// [spec:foma:sem:dynarray.fsm-get-arc-num-out-fn]
// [spec:foma:def:fomalib.fsm-get-arc-num-out-fn]
// [spec:foma:sem:fomalib.fsm-get-arc-num-out-fn]
pub fn fsm_get_arc_num_out(handle: &FsmReadHandle) -> i32 {
    let Some(cursor) = handle.arcs_cursor else {
        return -1;
    };
    /* short→int promotion; a sentinel line's stored -1 returns as-is */
    handle
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .states[cursor]
        .out as i32
}

// [spec:foma:def:dynarray.fsm-get-arc-out-fn]
// [spec:foma:sem:dynarray.fsm-get-arc-out-fn]
// [spec:foma:def:fomalib.fsm-get-arc-out-fn]
// [spec:foma:sem:fomalib.fsm-get-arc-out-fn]
pub fn fsm_get_arc_out(handle: &FsmReadHandle) -> Option<&str> {
    /* C returns a borrowed char* into the handle's sigma list, or NULL
    when the cursor is NULL */
    let cursor = handle.arcs_cursor?;
    let index = handle
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .states[cursor]
        .out;
    /* no sentinel check: out == -1 indexes out of bounds in C.
    DEVIATION from C (OOB read; Rust panics) */
    handle.fsm_sigma_list[index as usize].symbol.as_deref()
}

// [spec:foma:def:dynarray.fsm-get-next-initial-fn]
// [spec:foma:sem:dynarray.fsm-get-next-initial-fn]
// [spec:foma:def:fomalib.fsm-get-next-initial-fn]
// [spec:foma:sem:fomalib.fsm-get-next-initial-fn]
pub fn fsm_get_next_initial(handle: &mut FsmReadHandle) -> i32 {
    let cursor = match handle.initials_cursor {
        None => 0,
        Some(cur) => {
            /* sticky -1 terminator: the end returns -1 without advancing */
            if handle.initials_head[cur] == -1 {
                return -1;
            }
            cur + 1
        }
    };
    handle.initials_cursor = Some(cursor);
    handle.initials_head[cursor]
}

// [spec:foma:def:dynarray.fsm-get-next-final-fn]
// [spec:foma:sem:dynarray.fsm-get-next-final-fn]
// [spec:foma:def:fomalib.fsm-get-next-final-fn]
// [spec:foma:sem:fomalib.fsm-get-next-final-fn]
pub fn fsm_get_next_final(handle: &mut FsmReadHandle) -> i32 {
    let cursor = match handle.finals_cursor {
        None => 0,
        Some(cur) => {
            /* sticky -1 terminator: the end returns -1 without advancing */
            if handle.finals_head[cur] == -1 {
                return -1;
            }
            cur + 1
        }
    };
    handle.finals_cursor = Some(cursor);
    handle.finals_head[cursor]
}

/* ------------------------------------------------------------------ */
/* Idiomatic iterator front-ends (additive sugar)                     */
/* ------------------------------------------------------------------ */

/* Which -1-terminated cursor family a ReadIter drives. */
#[derive(Clone, Copy)]
enum ReadWhich {
    Finals,
    Initials,
}

/// Lazy iterator over a read handle's final (or initial) state numbers.
/// Each `next()` drives the existing C-shaped `fsm_get_next_final` /
/// `fsm_get_next_initial` cursor protocol, yielding state numbers until the
/// underlying function returns the `-1` terminator. Pure sugar over the
/// cursor walk — it adds no new traversal behaviour, so
/// `for s in handle.finals() { ... }` is exactly the `loop { let s =
/// fsm_get_next_final(h); if s == -1 { break } ... }` idiom.
pub struct ReadIter<'a> {
    handle: &'a mut FsmReadHandle,
    which: ReadWhich,
}

impl Iterator for ReadIter<'_> {
    type Item = i32;
    fn next(&mut self) -> Option<i32> {
        let s = match self.which {
            ReadWhich::Finals => fsm_get_next_final(self.handle),
            ReadWhich::Initials => fsm_get_next_initial(self.handle),
        };
        if s == -1 { None } else { Some(s) }
    }
}

impl FsmReadHandle {
    /// Yield each final state number (drives `fsm_get_next_final`).
    pub fn finals(&mut self) -> ReadIter<'_> {
        ReadIter {
            handle: self,
            which: ReadWhich::Finals,
        }
    }

    /// Yield each initial state number (drives `fsm_get_next_initial`).
    pub fn initials(&mut self) -> ReadIter<'_> {
        ReadIter {
            handle: self,
            which: ReadWhich::Initials,
        }
    }
}

// [spec:foma:def:dynarray.fsm-get-num-states-fn]
// [spec:foma:sem:dynarray.fsm-get-num-states-fn]
// [spec:foma:def:fomalib.fsm-get-num-states-fn]
// [spec:foma:sem:fomalib.fsm-get-num-states-fn]
pub fn fsm_get_num_states(handle: &FsmReadHandle) -> i32 {
    handle
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .statecount
}

// [spec:foma:def:dynarray.fsm-get-has-unknowns-fn]
// [spec:foma:sem:dynarray.fsm-get-has-unknowns-fn]
// [spec:foma:def:fomalib.fsm-get-has-unknowns-fn]
// [spec:foma:sem:fomalib.fsm-get-has-unknowns-fn]
pub fn fsm_get_has_unknowns(handle: &FsmReadHandle) -> i32 {
    handle.has_unknowns as i32
}

// [spec:foma:def:dynarray.fsm-get-next-state-fn]
// [spec:foma:sem:dynarray.fsm-get-next-state-fn]
// [spec:foma:def:fomalib.fsm-get-next-state-fn]
// [spec:foma:sem:fomalib.fsm-get-next-state-fn]
pub fn fsm_get_next_state(handle: &mut FsmReadHandle) -> i32 {
    let cursor = match handle.states_cursor {
        None => 0,
        Some(cur) => cur + 1,
    };
    handle.states_cursor = Some(cursor);
    /* C: states_cursor - states_head >= fsm_get_num_states(handle) —
    ptrdiff vs int comparison, done in i64 here */
    if cursor as i64 >= fsm_get_num_states(handle) as i64 {
        return -1;
    }
    /* the state's first line; a NULL entry (state number gap) is a crash
    in C — expect panics (DEVIATION pin) */
    let first =
        handle.states_head[cursor].expect("no state-number gap in a read handle's states_head");
    let stateno = handle
        .net
        .as_ref()
        .expect("net present until fsm_read_done")
        .states[first]
        .state_no;
    /* park arcs_cursor one line before the state's first line so that
    fsm_get_next_state_arc's pre-increment lands on it (C decrements the
    pointer below the array base for first == 0 — UB; wrapping index here) */
    handle.arcs_cursor = Some(first.wrapping_sub(1));
    handle.current_state = stateno;
    stateno
}

// [spec:foma:def:dynarray.fsm-read-done-fn]
// [spec:foma:sem:dynarray.fsm-read-done-fn]
// [spec:foma:def:fomalib.fsm-read-done-fn]
// [spec:foma:sem:fomalib.fsm-read-done-fn]
pub fn fsm_read_done(handle: Box<FsmReadHandle>) -> Box<Fsm> {
    /* frees lookuptable, fsm_sigma_list (array only — the symbol strings
    are copies here where C borrows net->sigma's), finals_head,
    initials_head, states_head, and the handle — all dropped here.
    DEVIATION from C (C leaves the caller's net pointer untouched; the
    Rust handle owns the net, so it is returned to the caller) */
    let mut handle = handle;
    handle.net.take().expect("net present until fsm_read_done")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Fsm, FsmState, Sigma};

    /* FsmState has no PartialEq (types.rs is out of scope); compare the six
    fields as a tuple, which does. */
    fn line(l: &FsmState) -> (i32, i16, i16, i32, i8, i8) {
        (
            l.state_no,
            l.r#in,
            l.out,
            l.target,
            l.final_state,
            l.start_state,
        )
    }

    /* ---- builder family --------------------------------------------- */

    // [spec:foma:sem:dynarray.fsm-state-init-fn/test]
    // [spec:foma:sem:fomalibconf.fsm-state-init-fn/test]
    // [spec:foma:sem:dynarray.fsm-state-set-current-state-fn/test]
    // [spec:foma:sem:fomalibconf.fsm-state-set-current-state-fn/test]
    // [spec:foma:sem:dynarray.fsm-state-add-arc-fn/test]
    // [spec:foma:sem:fomalibconf.fsm-state-add-arc-fn/test]
    // [spec:foma:sem:dynarray.fsm-state-end-state-fn/test]
    // [spec:foma:sem:fomalibconf.fsm-state-end-state-fn/test]
    // [spec:foma:sem:dynarray.fsm-state-close-fn/test]
    // [spec:foma:sem:fomalibconf.fsm-state-close-fn/test]
    #[test]
    fn fsm_state_build_line_table_and_sentinel() {
        /* state 0: initial, one arc 0 -3:3-> 1; state 1: final, no arcs. */
        let mut b = fsm_state_init(4);
        fsm_state_set_current_state(&mut b, 0, 0, 1);
        fsm_state_add_arc(&mut b, 0, 3, 3, 1, 0, 1);
        fsm_state_end_state(&mut b);
        fsm_state_set_current_state(&mut b, 1, 1, 0);
        /* no arc emitted -> end_state must synthesize a placeholder line */
        fsm_state_end_state(&mut b);
        let mut net = fsm_create("");
        fsm_state_close(&mut b, &mut net);

        /* exact line table incl. the sentinel terminator */
        assert_eq!(net.states.len(), 3);
        assert_eq!(line(&net.states[0]), (0, 3, 3, 1, 0, 1));
        assert_eq!(line(&net.states[1]), (1, -1, -1, -1, 1, 0));
        assert_eq!(line(&net.states[2]), (-1, -1, -1, -1, -1, -1));

        /* counts and heuristic flags copied out by fsm_state_close */
        assert_eq!(net.arity, 1);
        assert_eq!(net.arccount, 1);
        assert_eq!(net.statecount, 2);
        assert_eq!(net.linecount, 3);
        assert_eq!(net.finalcount, 1);
        assert_eq!(net.pathcount, PATHCOUNT_UNKNOWN);
        assert_eq!(net.is_deterministic, 1);
        assert_eq!(net.is_epsilon_free, 1);
        assert_eq!(net.is_pruned, UNK);
        assert_eq!(net.is_minimized, UNK);
        assert_eq!(net.is_loop_free, UNK);
        assert_eq!(net.is_completed, UNK);
        assert_eq!(net.arcs_sorted_in, 0);
        assert_eq!(net.arcs_sorted_out, 0);
    }

    // [spec:foma:sem:dynarray.fsm-state-set-current-state-fn/test]
    #[test]
    fn fsm_state_set_current_state_counts_only_exact_one() {
        let mut b = fsm_state_init(4);
        /* final/start flags of 2 and 5 are nonzero but not exactly 1 */
        fsm_state_set_current_state(&mut b, 0, 2, 5);
        assert_eq!(b.num_finals, 0);
        assert_eq!(b.num_initials, 0);
        fsm_state_set_current_state(&mut b, 1, 1, 1);
        assert_eq!(b.num_finals, 1);
        assert_eq!(b.num_initials, 1);
    }

    // [spec:foma:sem:dynarray.fsm-state-add-arc-fn/test]
    #[test]
    fn fsm_state_add_arc_sets_arity_on_asymmetric_label() {
        let mut b = fsm_state_init(4);
        fsm_state_set_current_state(&mut b, 0, 0, 1);
        assert_eq!(b.arity, 1);
        fsm_state_add_arc(&mut b, 0, 3, 4, 1, 0, 1); /* in != out */
        assert_eq!(b.arity, 2);
    }

    // [spec:foma:sem:dynarray.fsm-state-add-arc-fn/test]
    #[test]
    fn fsm_state_add_arc_epsilon_self_loop_dropped() {
        let mut b = fsm_state_init(4);
        fsm_state_set_current_state(&mut b, 0, 0, 1);
        /* EPSILON:EPSILON self-loop -> nothing appended, flags untouched */
        fsm_state_add_arc(&mut b, 0, EPSILON, EPSILON, 0, 0, 1);
        assert_eq!(b.current_fsm_linecount, 0);
        assert!(b.is_epsilon_free);
        assert!(b.is_deterministic);
        /* EPSILON:EPSILON to a different target -> emitted, clears both flags */
        fsm_state_add_arc(&mut b, 0, EPSILON, EPSILON, 1, 0, 1);
        assert_eq!(b.current_fsm_linecount, 1);
        assert!(!b.is_epsilon_free);
        assert!(!b.is_deterministic);
    }

    // [spec:foma:sem:dynarray.fsm-state-add-arc-fn/test]
    // [spec:foma:sem:dynarray.fsm-state-end-state-fn/test]
    #[test]
    fn fsm_state_add_arc_slookup_dedup_quirks() {
        let mut b = fsm_state_init(4);
        fsm_state_set_current_state(&mut b, 0, 0, 1);
        /* 1: first (3,3)->1 emitted */
        fsm_state_add_arc(&mut b, 0, 3, 3, 1, 0, 1);
        /* 2: exact duplicate (3,3)->1 silently dropped */
        fsm_state_add_arc(&mut b, 0, 3, 3, 1, 0, 1);
        assert_eq!(b.current_fsm_linecount, 1);
        assert_eq!(b.arccount, 1);
        assert!(b.is_deterministic);
        /* 3: same label, different target -> emitted, clears determinism,
        overwrites the cell's recorded target to 2 */
        fsm_state_add_arc(&mut b, 0, 3, 3, 2, 0, 1);
        assert!(!b.is_deterministic);
        /* 4: repeats the FIRST target (1); the cell now records 2, so this is
        no longer seen as a duplicate and is emitted a second time (the quirk) */
        fsm_state_add_arc(&mut b, 0, 3, 3, 1, 0, 1);

        assert_eq!(b.current_fsm_linecount, 3);
        assert_eq!(b.arccount, 3);
        let targets: Vec<i32> = b.current_fsm_head.iter().map(|l| l.target).collect();
        assert_eq!(targets, vec![1, 2, 1]);

        /* end_state bumps mainloop, invalidating the stamps for the next state */
        let before = b.mainloop;
        fsm_state_end_state(&mut b);
        assert_eq!(b.mainloop, before + 1);
        assert_eq!(b.statecount, 1);
    }

    // [spec:foma:sem:dynarray.fsm-state-close-fn/test]
    #[test]
    fn fsm_state_close_multiple_initials_clears_determinism() {
        let mut b = fsm_state_init(4);
        fsm_state_set_current_state(&mut b, 0, 0, 1);
        fsm_state_end_state(&mut b);
        fsm_state_set_current_state(&mut b, 1, 1, 1); /* second initial state */
        fsm_state_end_state(&mut b);
        let mut net = fsm_create("");
        fsm_state_close(&mut b, &mut net);
        /* num_initials > 1 forces is_deterministic = 0 even with no dup arcs */
        assert_eq!(net.is_deterministic, 0);
        /* and slookup is freed */
        assert!(b.slookup.is_empty());
    }

    /* ---- construction family ---------------------------------------- */

    // [spec:foma:sem:dynarray.fsm-construct-init-fn/test]
    // [spec:foma:sem:fomalib.fsm-construct-init-fn/test]
    #[test]
    fn fsm_construct_init_shape() {
        let h = fsm_construct_init("mynet");
        assert_eq!(h.fsm_state_list.len(), 1024);
        assert_eq!(h.fsm_state_list_size, 1024);
        assert_eq!(h.fsm_sigma_list.len(), 1024);
        assert_eq!(h.fsm_sigma_list_size, 1024);
        assert_eq!(h.fsm_sigma_hash.len(), SIGMA_HASH_SIZE as usize);
        /* C never initializes fsm_sigma_hash_size; the port pins it to 0 */
        assert_eq!(h.fsm_sigma_hash_size, 0);
        assert_eq!(h.maxstate, -1);
        assert_eq!(h.maxsigma, -1);
        assert_eq!(h.numfinals, 0);
        assert_eq!(h.hasinitial, 0);
        assert_eq!(h.name.as_deref(), Some("mynet"));
        /* zeroed state slot */
        let s = &h.fsm_state_list[0];
        assert!(!s.used && !s.is_final && !s.is_initial);
        assert_eq!(s.num_trans, 0);
        assert!(s.fsm_trans_list.is_none());
        /* empty sigma-hash bucket head */
        assert!(h.fsm_sigma_hash[0].symbol.is_none());
    }

    // [spec:foma:sem:dynarray.fsm-construct-hash-sym-fn/test]
    #[test]
    fn fsm_construct_hash_sym_signed_char() {
        assert_eq!(fsm_construct_hash_sym(""), 0);
        assert_eq!(fsm_construct_hash_sym("a"), 97);
        assert_eq!(fsm_construct_hash_sym("01"), 97);
        /* "é" is UTF-8 0xC3 0xA9; as signed chars -61 + -87 = -148, which
        wraps to 0xFFFFFF6C before % 1021 = 981 (unsigned chars would give
        364, so this pins the signed-char sign extension). */
        assert_eq!(fsm_construct_hash_sym("é"), 981);
    }

    // [spec:foma:sem:dynarray.fsm-construct-check-size-fn/test]
    #[test]
    fn fsm_construct_check_size_grows_to_next_power_of_two() {
        let mut h = fsm_construct_init("n");
        fsm_construct_check_size(&mut h, 2000);
        assert_eq!(h.fsm_state_list_size, 2048);
        assert_eq!(h.fsm_state_list.len(), 2048);
        /* new slots default-initialized; maxstate untouched by check_size */
        assert!(!h.fsm_state_list[2000].used);
        assert_eq!(h.maxstate, -1);
        /* a no-op when already big enough */
        fsm_construct_check_size(&mut h, 10);
        assert_eq!(h.fsm_state_list_size, 2048);
    }

    // [spec:foma:sem:dynarray.fsm-construct-add-symbol-fn/test]
    // [spec:foma:sem:fomalib.fsm-construct-add-symbol-fn/test]
    // [spec:foma:sem:dynarray.fsm-construct-check-symbol-fn/test]
    // [spec:foma:sem:fomalib.fsm-construct-check-symbol-fn/test]
    #[test]
    fn fsm_construct_add_and_check_symbol_numbering() {
        let mut h = fsm_construct_init("n");
        assert_eq!(fsm_construct_check_symbol(&h, "cat"), -1);
        /* first non-reserved symbol is floored to MINSIGMA = 3 */
        assert_eq!(fsm_construct_add_symbol(&mut h, "cat"), 3);
        assert_eq!(h.maxsigma, 3);
        assert_eq!(fsm_construct_add_symbol(&mut h, "dog"), 4);
        /* reserved symbols keep their fixed numbers, maxsigma not lowered */
        assert_eq!(
            fsm_construct_add_symbol(&mut h, "@_EPSILON_SYMBOL_@"),
            EPSILON
        );
        assert_eq!(
            fsm_construct_add_symbol(&mut h, "@_IDENTITY_SYMBOL_@"),
            IDENTITY
        );
        assert_eq!(h.maxsigma, 4);
        /* now findable via the hash */
        assert_eq!(fsm_construct_check_symbol(&h, "cat"), 3);
        assert_eq!(fsm_construct_check_symbol(&h, "dog"), 4);
        assert_eq!(fsm_construct_check_symbol(&h, "missing"), -1);
    }

    // [spec:foma:sem:dynarray.fsm-construct-add-symbol-fn/test]
    // [spec:foma:sem:fomalib.fsm-construct-add-symbol-fn/test]
    #[test]
    fn fsm_construct_add_symbol_hash_bucket_chain() {
        let mut h = fsm_construct_init("n");
        /* "a" and "01" both sum to 97 -> same signed-char bucket 97 */
        assert_eq!(fsm_construct_add_symbol(&mut h, "a"), 3);
        assert_eq!(fsm_construct_add_symbol(&mut h, "01"), 4);
        let head = &h.fsm_sigma_hash[97];
        assert_eq!(head.symbol.as_deref(), Some("a"));
        assert_eq!(head.sym, 3);
        /* second colliding symbol spliced directly after the head */
        let next = head.next.as_deref().unwrap();
        assert_eq!(next.symbol.as_deref(), Some("01"));
        assert_eq!(next.sym, 4);
        assert!(next.next.is_none());
    }

    // [spec:foma:sem:dynarray.fsm-construct-set-final-fn/test]
    // [spec:foma:sem:fomalib.fsm-construct-set-final-fn/test]
    #[test]
    fn fsm_construct_set_final_idempotent() {
        let mut h = fsm_construct_init("n");
        fsm_construct_set_final(&mut h, 5);
        assert_eq!(h.maxstate, 5);
        assert_eq!(h.numfinals, 1);
        assert!(h.fsm_state_list[5].is_final);
        /* does not set `used` */
        assert!(!h.fsm_state_list[5].used);
        /* repeated call does not recount */
        fsm_construct_set_final(&mut h, 5);
        assert_eq!(h.numfinals, 1);
    }

    // [spec:foma:sem:dynarray.fsm-construct-set-initial-fn/test]
    // [spec:foma:sem:fomalib.fsm-construct-set-initial-fn/test]
    #[test]
    fn fsm_construct_set_initial_sets_hasinitial() {
        let mut h = fsm_construct_init("n");
        fsm_construct_set_initial(&mut h, 2);
        assert_eq!(h.maxstate, 2);
        assert_eq!(h.hasinitial, 1);
        assert!(h.fsm_state_list[2].is_initial);
        assert!(!h.fsm_state_list[2].used);
    }

    // [spec:foma:sem:dynarray.fsm-construct-add-arc-fn/test]
    // [spec:foma:sem:fomalib.fsm-construct-add-arc-fn/test]
    #[test]
    fn fsm_construct_add_arc_prepends_and_interns() {
        let mut h = fsm_construct_init("n");
        fsm_construct_add_arc(&mut h, 0, 1, "a", "a");
        fsm_construct_add_arc(&mut h, 0, 2, "b", "c");
        assert_eq!(h.maxstate, 2);
        assert!(h.fsm_state_list[0].used);
        assert!(h.fsm_state_list[1].used);
        assert!(h.fsm_state_list[2].used);
        /* num_trans is not maintained */
        assert_eq!(h.fsm_state_list[0].num_trans, 0);
        /* newest-first: (b,c)->2 then (a,a)->1 */
        let head = h.fsm_state_list[0].fsm_trans_list.as_deref().unwrap();
        assert_eq!((head.r#in, head.out, head.target), (4, 5, 2));
        let next = head.next.as_deref().unwrap();
        assert_eq!((next.r#in, next.out, next.target), (3, 3, 1));
        assert!(next.next.is_none());
    }

    // [spec:foma:sem:dynarray.fsm-construct-add-arc-nums-fn/test]
    // [spec:foma:sem:fomalib.fsm-construct-add-arc-nums-fn/test]
    #[test]
    fn fsm_construct_add_arc_nums_no_sigma_touch() {
        let mut h = fsm_construct_init("n");
        fsm_construct_add_arc_nums(&mut h, 0, 1, 7, 8);
        assert_eq!(h.maxstate, 1);
        assert_eq!(h.maxsigma, -1); /* untouched */
        assert!(h.fsm_state_list[0].used && h.fsm_state_list[1].used);
        assert_eq!(h.fsm_state_list[0].num_trans, 0);
        let head = h.fsm_state_list[0].fsm_trans_list.as_deref().unwrap();
        assert_eq!((head.r#in, head.out, head.target), (7, 8, 1));
    }

    // [spec:foma:sem:dynarray.fsm-construct-copy-sigma-fn/test]
    // [spec:foma:sem:fomalib.fsm-construct-copy-sigma-fn/test]
    #[test]
    fn fsm_construct_copy_sigma_bulk_loads() {
        let sigma = vec![
            Sigma {
                number: 3,
                symbol: "x".into(),
            },
            Sigma {
                number: 5,
                symbol: "y".into(),
            },
        ];
        let mut h = fsm_construct_init("n");
        fsm_construct_copy_sigma(&mut h, &sigma);
        assert_eq!(h.maxsigma, 5);
        assert_eq!(h.fsm_sigma_list[3].symbol.as_deref(), Some("x"));
        assert_eq!(h.fsm_sigma_list[5].symbol.as_deref(), Some("y"));
        assert_eq!(fsm_construct_check_symbol(&h, "x"), 3);
        assert_eq!(fsm_construct_check_symbol(&h, "y"), 5);
        assert_eq!(fsm_construct_check_symbol(&h, "z"), -1);
    }

    // [spec:foma:sem:dynarray.fsm-construct-convert-sigma-fn/test]
    #[test]
    fn fsm_construct_convert_sigma_ascending() {
        let mut h = fsm_construct_init("n");
        fsm_construct_add_symbol(&mut h, "@_EPSILON_SYMBOL_@"); /* 0 */
        fsm_construct_add_symbol(&mut h, "cat"); /* 3 */
        fsm_construct_add_symbol(&mut h, "dog"); /* 4 */
        let sigma = fsm_construct_convert_sigma(&h);
        let seen: Vec<(i32, String)> = sigma
            .iter()
            .map(|s| (s.number, s.symbol.to_string()))
            .collect();
        assert_eq!(
            seen,
            vec![
                (0, "@_EPSILON_SYMBOL_@".to_string()),
                (3, "cat".to_string()),
                (4, "dog".to_string()),
            ]
        );
    }

    // [spec:foma:sem:dynarray.fsm-construct-done-fn/test]
    // [spec:foma:sem:fomalib.fsm-construct-done-fn/test]
    #[test]
    fn fsm_construct_done_builds_net() {
        let mut h = fsm_construct_init("mynet");
        fsm_construct_set_initial(&mut h, 0);
        fsm_construct_set_final(&mut h, 1);
        fsm_construct_add_arc(&mut h, 0, 1, "a", "a");
        let net = fsm_construct_done(h);

        assert_eq!(net.name, "mynet");
        assert_eq!(net.statecount, 2);
        assert_eq!(net.finalcount, 1);
        assert_eq!(net.arccount, 1);
        assert_eq!(net.linecount, 3);
        assert_eq!(net.pathcount, PATHCOUNT_UNKNOWN);
        assert_eq!(net.arity, 1);
        assert_eq!(net.is_deterministic, 1);
        assert_eq!(net.is_epsilon_free, 1);
        /* line table: arc line, state-1 placeholder, sentinel */
        assert_eq!(line(&net.states[0]), (0, 3, 3, 1, 0, 1));
        assert_eq!(line(&net.states[1]), (1, -1, -1, -1, 1, 0));
        assert_eq!(line(&net.states[2]), (-1, -1, -1, -1, -1, -1));
        /* sigma survived (single symbol, number 3 after sigma_sort) */
        assert_eq!(net.sigma.len(), 1);
        assert_eq!(net.sigma[0].number, 3);
        assert_eq!(net.sigma[0].symbol, "a");
    }

    // [spec:foma:sem:dynarray.fsm-construct-done-fn/test]
    // [spec:foma:sem:fomalib.fsm-construct-done-fn/test]
    #[test]
    fn fsm_construct_done_early_empty_set_when_no_final() {
        let mut h = fsm_construct_init("x");
        fsm_construct_set_initial(&mut h, 0);
        fsm_construct_add_arc(&mut h, 0, 0, "a", "a");
        /* numfinals == 0 -> immediate fsm_empty_set() */
        let net = fsm_construct_done(h);
        assert_eq!(net.statecount, 1);
        assert_eq!(net.finalcount, 0);
        assert_eq!(net.linecount, 2);
        assert_eq!(net.pathcount, 0);
    }

    // [spec:foma:sem:dynarray.fsm-construct-done-fn/test]
    // [spec:foma:sem:fomalib.fsm-construct-done-fn/test]
    #[test]
    fn fsm_construct_done_emptyfsm_detection_returns_empty_set() {
        /* valid handle (initial+final present) but no initial state has an
        outgoing arc and none is both initial and final -> emptyfsm path */
        let mut h = fsm_construct_init("x");
        fsm_construct_set_initial(&mut h, 0);
        fsm_construct_set_final(&mut h, 1);
        let net = fsm_construct_done(h);
        assert_eq!(net.statecount, 1);
        assert_eq!(net.finalcount, 0);
        assert_eq!(net.linecount, 2);
        assert_eq!(net.pathcount, 0);
    }

    // [spec:foma:sem:dynarray.fsm-construct-done-fn+1/test]
    // [spec:foma:sem:fomalib.fsm-construct-done-fn+1/test]
    #[test]
    fn fsm_construct_done_keeps_full_length_name() {
        let build = |name: &str| {
            let mut h = fsm_construct_init(name);
            fsm_construct_set_initial(&mut h, 0);
            fsm_construct_set_final(&mut h, 1);
            fsm_construct_add_arc(&mut h, 0, 1, "a", "a");
            fsm_construct_done(h)
        };
        // Names are heap Strings now — the old 40-byte struct cap is gone, so
        // the name is kept verbatim however long it is.
        let net = build(&"a".repeat(50));
        assert_eq!(net.name, "a".repeat(50));
        // Multi-byte names survive intact (no mid-codepoint truncation).
        let net = build(&format!("{}é", "a".repeat(39)));
        assert_eq!(net.name, format!("{}é", "a".repeat(39)));
        assert_eq!(net.name.chars().count(), 40);
    }

    /* ---- reading family --------------------------------------------- */

    /* Builds a 3-state net directly: state 0 initial with arcs 0-3:3->1 and
    0-4:4->2; states 1 and 2 final. sigma: 3="a", 4="b". */
    fn build_read_net() -> Box<Fsm> {
        let mut b = fsm_state_init(4);
        fsm_state_set_current_state(&mut b, 0, 0, 1);
        fsm_state_add_arc(&mut b, 0, 3, 3, 1, 0, 1);
        fsm_state_add_arc(&mut b, 0, 4, 4, 2, 0, 1);
        fsm_state_end_state(&mut b);
        fsm_state_set_current_state(&mut b, 1, 1, 0);
        fsm_state_end_state(&mut b);
        fsm_state_set_current_state(&mut b, 2, 1, 0);
        fsm_state_end_state(&mut b);
        let mut net = fsm_create("read");
        net.sigma = Vec::new();
        fsm_state_close(&mut b, &mut net);
        net.sigma = vec![
            Sigma {
                number: 3,
                symbol: "a".into(),
            },
            Sigma {
                number: 4,
                symbol: "b".into(),
            },
        ];
        net
    }

    // [spec:foma:sem:dynarray.fsm-read-init-fn/test]
    // [spec:foma:sem:fomalib.fsm-read-init-fn/test]
    // [spec:foma:sem:dynarray.fsm-read-is-final-fn/test]
    // [spec:foma:sem:fomalib.fsm-read-is-final-fn/test]
    // [spec:foma:sem:dynarray.fsm-read-is-initial-fn/test]
    // [spec:foma:sem:fomalib.fsm-read-is-initial-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-num-states-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-num-states-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-has-unknowns-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-has-unknowns-fn/test]
    #[test]
    fn fsm_read_init_and_lookup_tables() {
        let h = fsm_read_init(build_read_net());
        assert_eq!(fsm_get_num_states(&h), 3);
        assert_eq!(fsm_get_has_unknowns(&h), 0);
        /* is_initial returns bit 0 (1), is_final returns bit 1 (the value 2) */
        assert!(fsm_read_is_initial(&h, 0));
        assert!(!(fsm_read_is_initial(&h, 1)));
        assert!(!(fsm_read_is_final(&h, 0)));
        assert!(fsm_read_is_final(&h, 1));
        assert!(fsm_read_is_final(&h, 2));
        /* the -1-terminated finals/initials arrays */
        assert_eq!(h.initials_head, vec![0, -1]);
        assert_eq!(h.finals_head, vec![1, 2, -1]);
    }

    // [spec:foma:sem:dynarray.fsm-read-init-fn/test]
    // [spec:foma:sem:fomalib.fsm-read-init-fn/test]
    #[test]
    fn fsm_read_init_none_returns_none() {}

    // [spec:foma:sem:dynarray.fsm-get-has-unknowns-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-has-unknowns-fn/test]
    #[test]
    fn fsm_read_detects_identity_label_as_unknown() {
        let mut b = fsm_state_init(4);
        fsm_state_set_current_state(&mut b, 0, 1, 1);
        fsm_state_add_arc(&mut b, 0, IDENTITY, IDENTITY, 0, 1, 1);
        fsm_state_end_state(&mut b);
        let mut net = fsm_create("id");
        net.sigma = Vec::new();
        fsm_state_close(&mut b, &mut net);
        net.sigma = vec![Sigma {
            number: IDENTITY,
            symbol: "@_IDENTITY_SYMBOL_@".into(),
        }];
        let h = fsm_read_init(net);
        assert_eq!(fsm_get_has_unknowns(&h), 1);
    }

    // [spec:foma:sem:dynarray.fsm-get-next-initial-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-next-initial-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-next-final-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-next-final-fn/test]
    // [spec:foma:sem:dynarray.fsm-read-reset-fn/test]
    // [spec:foma:sem:fomalib.fsm-read-reset-fn/test]
    #[test]
    fn fsm_read_initials_finals_iterators_and_reset() {
        let mut h = fsm_read_init(build_read_net());
        /* initials: 0 then sticky -1 */
        assert_eq!(fsm_get_next_initial(&mut h), 0);
        assert_eq!(fsm_get_next_initial(&mut h), -1);
        assert_eq!(fsm_get_next_initial(&mut h), -1);
        /* finals: 1, 2 then sticky -1 */
        assert_eq!(fsm_get_next_final(&mut h), 1);
        assert_eq!(fsm_get_next_final(&mut h), 2);
        assert_eq!(fsm_get_next_final(&mut h), -1);
        assert_eq!(fsm_get_next_final(&mut h), -1);
        /* reset restarts every iterator */
        fsm_read_reset(Some(&mut *h));
        assert_eq!(fsm_get_next_initial(&mut h), 0);
        assert_eq!(fsm_get_next_final(&mut h), 1);
        /* reset(None) is a no-op, not a crash */
        fsm_read_reset(None);
    }

    // [spec:foma:sem:dynarray.fsm-get-next-state-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-next-state-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-next-state-arc-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-next-state-arc-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-arc-source-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-arc-source-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-arc-target-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-arc-target-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-arc-num-in-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-arc-num-in-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-arc-num-out-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-arc-num-out-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-arc-in-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-arc-in-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-arc-out-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-arc-out-fn/test]
    #[test]
    fn fsm_get_next_state_and_arc_walk() {
        let mut h = fsm_read_init(build_read_net());
        /* state 0 */
        assert_eq!(fsm_get_next_state(&mut h), 0);
        /* first arc 0 -3:3-> 1 (cursor parked one before, pre-incremented) */
        assert_eq!(fsm_get_next_state_arc(&mut h), 1);
        assert_eq!(fsm_get_arc_source(&h), 0);
        assert_eq!(fsm_get_arc_target(&h), 1);
        assert_eq!(fsm_get_arc_num_in(&h), 3);
        assert_eq!(fsm_get_arc_num_out(&h), 3);
        assert_eq!(fsm_get_arc_in(&h), Some("a"));
        assert_eq!(fsm_get_arc_out(&h), Some("a"));
        /* second arc 0 -4:4-> 2 */
        assert_eq!(fsm_get_next_state_arc(&mut h), 1);
        assert_eq!(fsm_get_arc_target(&h), 2);
        assert_eq!(fsm_get_arc_in(&h), Some("b"));
        /* no more arcs for state 0 */
        assert_eq!(fsm_get_next_state_arc(&mut h), 0);
        /* state 1: final, placeholder line has target == -1 -> zero arcs */
        assert_eq!(fsm_get_next_state(&mut h), 1);
        assert_eq!(fsm_get_next_state_arc(&mut h), 0);
        /* state 2, then exhaustion */
        assert_eq!(fsm_get_next_state(&mut h), 2);
        assert_eq!(fsm_get_next_state(&mut h), -1);
    }

    // [spec:foma:sem:dynarray.fsm-get-next-arc-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-next-arc-fn/test]
    #[test]
    fn fsm_get_next_arc_skips_sentinels_and_sticks() {
        let mut h = fsm_read_init(build_read_net());
        /* whole-machine walk visits only the two real arcs, skipping the
        placeholder lines of states 1 and 2, then sticks at 0 */
        assert_eq!(fsm_get_next_arc(&mut h), 1);
        assert_eq!(fsm_get_arc_target(&h), 1);
        assert_eq!(fsm_get_next_arc(&mut h), 1);
        assert_eq!(fsm_get_arc_target(&h), 2);
        assert_eq!(fsm_get_next_arc(&mut h), 0);
        assert_eq!(fsm_get_next_arc(&mut h), 0);
    }

    // [spec:foma:sem:dynarray.fsm-get-symbol-number-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-symbol-number-fn/test]
    #[test]
    fn fsm_get_symbol_number_linear_scan() {
        let h = fsm_read_init(build_read_net());
        assert_eq!(fsm_get_symbol_number(&h, "a"), 3);
        assert_eq!(fsm_get_symbol_number(&h, "b"), 4);
        assert_eq!(fsm_get_symbol_number(&h, "z"), -1);
    }

    // [spec:foma:sem:dynarray.fsm-get-arc-source-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-arc-source-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-arc-target-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-arc-target-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-arc-num-in-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-arc-num-in-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-arc-num-out-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-arc-num-out-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-arc-in-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-arc-in-fn/test]
    // [spec:foma:sem:dynarray.fsm-get-arc-out-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-arc-out-fn/test]
    #[test]
    fn fsm_get_arc_accessors_null_cursor() {
        /* fresh handle: arcs_cursor is NULL -> the documented sentinel values */
        let h = fsm_read_init(build_read_net());
        assert_eq!(fsm_get_arc_source(&h), -1);
        assert_eq!(fsm_get_arc_target(&h), -1);
        assert_eq!(fsm_get_arc_num_in(&h), -1);
        assert_eq!(fsm_get_arc_num_out(&h), -1);
        assert_eq!(fsm_get_arc_in(&h), None);
        assert_eq!(fsm_get_arc_out(&h), None);
    }

    // [spec:foma:sem:dynarray.fsm-get-next-state-arc-fn/test]
    // [spec:foma:sem:fomalib.fsm-get-next-state-arc-fn/test]
    #[test]
    #[should_panic]
    fn fsm_get_next_state_arc_null_cursor_panics() {
        /* C dereferences a NULL cursor (crash); the port unwraps and panics */
        let mut h = fsm_read_init(build_read_net());
        fsm_get_next_state_arc(&mut h);
    }

    // [spec:foma:sem:dynarray.fsm-read-done-fn/test]
    // [spec:foma:sem:fomalib.fsm-read-done-fn/test]
    #[test]
    fn fsm_read_done_returns_net() {
        let h = fsm_read_init(build_read_net());
        /* the Rust handle owns the net and hands it back on done */
        let net = fsm_read_done(h);
        assert_eq!(net.statecount, 3);
    }

    /* ---- module types ----------------------------------------------- */

    // [spec:foma:def:dynarray.foma-reserved-symbols/test]
    #[test]
    fn foma_reserved_symbols_table() {
        assert_eq!(FOMA_RESERVED_SYMBOLS[0].symbol, Some("@_EPSILON_SYMBOL_@"));
        assert_eq!(FOMA_RESERVED_SYMBOLS[0].number, EPSILON);
        assert_eq!(FOMA_RESERVED_SYMBOLS[0].prints_as, Some("0"));
        assert_eq!(FOMA_RESERVED_SYMBOLS[1].number, UNKNOWN);
        assert_eq!(FOMA_RESERVED_SYMBOLS[2].number, IDENTITY);
        /* NULL-terminator entry */
        assert!(FOMA_RESERVED_SYMBOLS[3].symbol.is_none());
    }

    // [spec:foma:def:dynarray.sigma-lookup/test]
    #[test]
    fn sigma_lookup_zeroed_by_init() {
        let b = fsm_state_init(2);
        /* fsm_state_init callocs ssize*ssize zeroed sigma_lookup cells */
        assert_eq!(b.slookup.len(), 9); /* ssize = sigma_size+1 = 3; 3*3 */
        assert_eq!(b.slookup[0].target, 0);
        assert_eq!(b.slookup[0].mainloop, 0);
    }

    // [spec:foma:sem:dynarray.fsm-construct-copy-sigma-fn+1/test]
    // [spec:foma:sem:fomalib.fsm-construct-copy-sigma-fn+1/test]
    #[test]
    fn fsm_construct_copy_sigma_grows_past_double_initial_size() {
        // Symbol number 3000 exceeds twice the initial fsm_sigma_list_size (1024),
        // so C's single-doubling growth left the slot out of range (OOB write in C,
        // index panic here). The growth loop must resize until the slot fits.
        let sigma = vec![Sigma {
            number: 3000,
            symbol: "z".into(),
        }];
        let mut h = fsm_construct_init("c");
        fsm_construct_copy_sigma(&mut h, &sigma);
        assert!(h.fsm_sigma_list_size > 3000);
        assert_eq!(h.fsm_sigma_list[3000].symbol.as_deref(), Some("z"));
        assert!(h.maxsigma >= 3000);
    }
}
