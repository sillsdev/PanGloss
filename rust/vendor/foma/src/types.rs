//! Types declared in the C headers (foma.h, fomalib.h, fomalibconf.h,
//! lexc.h), ported literally per docs/port/rust-conventions.md.
//!
//! Wave 2: types only — no methods, no trait impls. Field names and C
//! widths are preserved exactly (int → i32, short int → i16, char flag →
//! i8, unsigned char → u8, long long → i64, unsigned int → u32).
//! Malloc'd arrays → Vec, owned linked lists → Option<Box<...>>, owned
//! char* strings → Option<String>. Interior pointers into the
//! sentinel-terminated fsm line table are represented as indices
//! (pointer walks become index walks, per the conventions).
//!
//! Owned symbol/name/value strings are `smol_str::SmolStr` (immutable,
//! small-string-optimized, cheap to clone) rather than `String`, matching
//! nfst's SmolStr migration. Mutable accumulator buffers (Apply/MED
//! `instring`/`outstring`, built via push_str/truncate) stay `String`.

use smol_str::SmolStr;

/* ------------------------------------------------------------------ */
/* Constants from fomalib.h                                            */
/* ------------------------------------------------------------------ */

/* Library version */
pub const MAJOR_VERSION: i32 = 0;
pub const MINOR_VERSION: i32 = 10;
pub const BUILD_VERSION: i32 = 0;
pub const STATUS_VERSION: &str = "alpha";

/* Special symbols on arcs */
pub const EPSILON: i32 = 0;
pub const UNKNOWN: i32 = 1;
pub const IDENTITY: i32 = 2;

/* Variants of ignore operation */
pub const OP_IGNORE_ALL: i32 = 1;
pub const OP_IGNORE_INTERNAL: i32 = 2;

/* Replacement direction */
pub const OP_UPWARD_REPLACE: i32 = 1;
pub const OP_RIGHTWARD_REPLACE: i32 = 2;
pub const OP_LEFTWARD_REPLACE: i32 = 3;
pub const OP_DOWNWARD_REPLACE: i32 = 4;
pub const OP_TWO_LEVEL_REPLACE: i32 = 5;

/* Arrow types in fsmrules */
pub const ARROW_RIGHT: i32 = 1;
pub const ARROW_LEFT: i32 = 2;
pub const ARROW_OPTIONAL: i32 = 4;
/// This is for the [..] part of a dotted rule
pub const ARROW_DOTTED: i32 = 8;
pub const ARROW_LONGEST_MATCH: i32 = 16;
pub const ARROW_SHORTEST_MATCH: i32 = 32;
pub const ARROW_LEFT_TO_RIGHT: i32 = 64;
pub const ARROW_RIGHT_TO_LEFT: i32 = 128;

/* Flag types */
pub const FLAG_UNIFY: i32 = 1;
pub const FLAG_CLEAR: i32 = 2;
pub const FLAG_DISALLOW: i32 = 4;
pub const FLAG_NEGATIVE: i32 = 8;
pub const FLAG_POSITIVE: i32 = 16;
pub const FLAG_REQUIRE: i32 = 32;
pub const FLAG_EQUAL: i32 = 64;

pub const NO: i32 = 0;
pub const YES: i32 = 1;
pub const UNK: i32 = 2;

/* Compared against fsm.pathcount (long long in C) */
pub const PATHCOUNT_CYCLIC: i64 = -1;
pub const PATHCOUNT_OVERFLOW: i64 = -2;
pub const PATHCOUNT_UNKNOWN: i64 = -3;

pub const M_UPPER: i32 = 1;
pub const M_LOWER: i32 = 2;

pub const APPLY_INDEX_INPUT: i32 = 1;
pub const APPLY_INDEX_OUTPUT: i32 = 2;

/* ------------------------------------------------------------------ */
/* Constants from foma.h                                               */
/* ------------------------------------------------------------------ */

/// Apply down
pub const AP_D: i32 = 1;
/// Apply up
pub const AP_U: i32 = 2;
/// Apply minimum edit distance
pub const AP_M: i32 = 3;

/// Regular prompt
pub const PROMPT_MAIN: i32 = 0;
/// Apply prompt
pub const PROMPT_A: i32 = 1;

/* ------------------------------------------------------------------ */
/* Type-shape constants from the .c files (no spec ids of their own)   */
/* ------------------------------------------------------------------ */

/* int_stack.c stack capacities */
pub const MAX_STACK: usize = 2097152;
pub const MAX_PTR_STACK: usize = 2097152;

/* apply.c mode bits (apply_handle.mode) */
pub const RANDOM: i32 = 1;
pub const ENUMERATE: i32 = 2;
pub const MATCH: i32 = 4;
pub const UP: i32 = 8;
pub const DOWN: i32 = 16;
pub const LOWER: i32 = 32;
pub const UPPER: i32 = 64;
pub const SPACE: i32 = 128;

/* apply.c traversal results */
pub const FAIL: i32 = 0;
pub const SUCCEED: i32 = 1;

/* apply.c buffer sizes */
pub const DEFAULT_OUTSTRING_SIZE: usize = 4096;
pub const DEFAULT_STACK_SIZE: usize = 128;
pub const APPLY_BINSEARCH_THRESHOLD: i32 = 10;

/* spelling.c minimum-edit-distance defaults */
/// Default max words to find
pub const MED_DEFAULT_LIMIT: i32 = 4;
/// Default MED cost cutoff
pub const MED_DEFAULT_CUTOFF: i32 = 15;
/// By default won't grow heap more than this
pub const MED_DEFAULT_MAX_HEAP_SIZE: i32 = 262145;

/* ------------------------------------------------------------------ */
/* fomalib.h types                                                     */
/* ------------------------------------------------------------------ */

/// Defined networks
// [spec:foma:def:fomalib.defined-networks]
#[derive(Debug, Clone)]
pub struct DefinedNetworks {
    pub name: Option<SmolStr>,
    pub net: Option<Box<Fsm>>,
    pub next: Option<Box<DefinedNetworks>>,
}

/// Defined functions
// [spec:foma:def:fomalib.defined-functions]
#[derive(Debug, Clone)]
pub struct DefinedFunctions {
    pub name: Option<SmolStr>,
    pub regex: Option<SmolStr>,
    pub numargs: i32,
    pub next: Option<Box<DefinedFunctions>>,
}

// [spec:foma:def:fomalib.defined-quantifiers]
#[derive(Debug, Clone)]
pub struct DefinedQuantifiers {
    pub name: Option<SmolStr>,
    pub next: Option<Box<DefinedQuantifiers>>,
}

/// Main automaton structure
// [spec:foma:def:fomalib.fsm]
#[derive(Debug, Clone)]
pub struct Fsm {
    /// C: `char name[FSM_NAME_LEN]` — a fixed 40-byte struct buffer. Here it is
    /// an unbounded heap `String`: the 40-byte cap (and its truncation) is gone.
    pub name: SmolStr,
    pub arity: i32,
    pub arccount: i32,
    pub statecount: i32,
    pub linecount: i32,
    pub finalcount: i32,
    pub pathcount: i64,
    pub is_deterministic: i32,
    pub is_pruned: i32,
    pub is_minimized: i32,
    pub is_epsilon_free: i32,
    pub is_loop_free: i32,
    pub is_completed: i32,
    pub arcs_sorted_in: i32,
    pub arcs_sorted_out: i32,
    /// The line table: sentinel-terminated (final line has state_no == -1),
    /// exactly as in C (pointer to first line). Empty vec ↔ NULL.
    pub states: Vec<FsmState>,
    pub sigma: Vec<Sigma>,
    // DEVIATION from C (aliased pointer; fsm_copy shares medlookup between copies and C double-frees)
    pub medlookup: Option<Box<Medlookup>>,
}

/// Minimum edit distance structure
// [spec:foma:def:fomalib.medlookup]
#[derive(Debug, Clone)]
pub struct Medlookup {
    /// Confusion matrix
    pub confusion_matrix: Vec<i32>,
}

/// Array of states (one line of the fsm line table)
// [spec:foma:def:fomalib.fsm-state]
#[derive(Debug, Clone)]
pub struct FsmState {
    /// State number
    pub state_no: i32,
    pub r#in: i16,
    pub out: i16,
    pub target: i32,
    pub final_state: i8,
    pub start_state: i8,
}

// [spec:foma:def:fomalib.fsmcontexts]
#[derive(Debug, Clone)]
pub struct Fsmcontexts {
    pub left: Option<Box<Fsm>>,
    pub right: Option<Box<Fsm>>,
    pub next: Option<Box<Fsmcontexts>>,
    /// Only used internally when compiling rewrite rules
    pub cpleft: Option<Box<Fsm>>,
    /// ditto
    pub cpright: Option<Box<Fsm>>,
}

// [spec:foma:def:fomalib.fsmrules]
#[derive(Debug, Clone)]
pub struct Fsmrules {
    pub left: Option<Box<Fsm>>,
    pub right: Option<Box<Fsm>>,
    /// Only needed for A -> B ... C rules
    pub right2: Option<Box<Fsm>>,
    pub cross_product: Option<Box<Fsm>>,
    pub next: Option<Box<Fsmrules>>,
    pub arrow_type: i32,
    /// [.A.] rule
    pub dotted: i32,
}

// [spec:foma:def:fomalib.rewrite-set]
#[derive(Debug, Clone)]
pub struct RewriteSet {
    pub rewrite_rules: Option<Box<Fsmrules>>,
    pub rewrite_contexts: Option<Box<Fsmcontexts>>,
    pub next: Option<Box<RewriteSet>>,
    /// || \\ // \/
    pub rule_direction: i32,
}

/// One sigma alphabet entry; number < IDENTITY is reserved for special
/// symbols. The alphabet as a whole is a `Vec<Sigma>` in insertion order.
// [spec:foma:def:fomalib.sigma+1]
#[derive(Debug, Clone)]
pub struct Sigma {
    pub number: i32,
    pub symbol: SmolStr,
}

// [spec:foma:def:fomalib.fsm-options]
// C: typedef enum { ... } FSM_OPTIONS; — names kept literally.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
pub enum FSM_OPTIONS {
    /// _Bool
    FSMO_SKIP_WORD_BOUNDARY_MARKER,
    FSMO_NUM_OPTIONS,
}

/* String hashing */

// [spec:foma:def:fomalib.sh-handle]
#[derive(Debug, Clone)]
pub struct ShHandle {
    /// C: pointer to a calloc'd array of STRING_HASH_SIZE bucket heads
    /// (chained on collision).
    pub hash: Vec<ShHashtable>,
    pub lastvalue: i32,
}

// [spec:foma:def:fomalib.sh-hashtable]
#[derive(Debug, Clone)]
pub struct ShHashtable {
    /// NULL ↔ empty bucket head
    pub string: Option<SmolStr>,
    pub value: i32,
    pub next: Option<Box<ShHashtable>>,
}

/* Trie construction */

// [spec:foma:def:fomalib.trie-hash]
#[derive(Debug, Clone)]
pub struct TrieHash {
    // DEVIATION from C (insym/outsym alias strings interned in the sh_hash; owned copies here)
    pub insym: Option<SmolStr>,
    pub outsym: Option<SmolStr>,
    pub sourcestate: u32,
    pub targetstate: u32,
    pub next: Option<Box<TrieHash>>,
}

// [spec:foma:def:fomalib.trie-states]
#[derive(Debug, Clone)]
pub struct TrieStates {
    pub is_final: bool,
}

// [spec:foma:def:fomalib.fsm-trie-handle]
#[derive(Debug, Clone)]
pub struct FsmTrieHandle {
    /// C: array of `statesize` trie_states (grown on demand)
    pub trie_states: Vec<TrieStates>,
    pub trie_cursor: u32,
    /// C: calloc'd array of THASH_TABLESIZE bucket heads (chained)
    pub trie_hash: Vec<TrieHash>,
    pub used_states: u32,
    pub statesize: u32,
    /// Per-trie string-intern table; fsm_trie_done consumes it with the handle.
    pub sh_hash: Box<ShHandle>,
}

/* Extraction routines */

// [spec:foma:def:fomalib.fsm-read-handle]
#[derive(Debug, Clone)]
pub struct FsmReadHandle {
    // DEVIATION from C (interior pointer to net->states; represented as a base index into net's line table)
    pub arcs_head: usize,
    /// C: malloc'd array mapping state number → pointer to its first line;
    /// here indices into the net's line table (None ↔ NULL).
    pub states_head: Vec<Option<usize>>,
    /// Iteration cursor: index into the net's line table (None ↔ NULL)
    pub arcs_cursor: Option<usize>,
    /// -1-terminated array of final state numbers, as in C
    pub finals_head: Vec<i32>,
    /// Cursor: index into finals_head (None ↔ NULL)
    pub finals_cursor: Option<usize>,
    /// Cursor: index into states_head (None ↔ NULL)
    pub states_cursor: Option<usize>,
    /// -1-terminated array of initial state numbers, as in C
    pub initials_head: Vec<i32>,
    /// Cursor: index into initials_head (None ↔ NULL)
    pub initials_cursor: Option<usize>,
    pub current_state: i32,
    pub fsm_sigma_list: Vec<FsmSigmaList>,
    pub sigma_list_size: i32,
    // DEVIATION from C (borrowed pointer — the handle never owns the net; see fsm-read-done sem)
    pub net: Option<Box<Fsm>>,
    /// Per-state byte table: bit 0 = initial, bit 1 = final
    pub lookuptable: Vec<u8>,
    pub has_unknowns: bool,
}

/* ------------------------------------------------------------------ */
/* fomalibconf.h types                                                 */
/* ------------------------------------------------------------------ */

// [spec:foma:def:fomalibconf.state-array]
#[derive(Debug, Clone)]
pub struct StateArray {
    // DEVIATION from C (interior pointer to a state's first line in the fsm line table; represented as an index)
    pub transitions: usize,
}

// [spec:foma:def:fomalibconf.fsm-trans-list]
#[derive(Debug, Clone)]
pub struct FsmTransList {
    pub r#in: i16,
    pub out: i16,
    pub target: i32,
    pub next: Option<Box<FsmTransList>>,
}

// [spec:foma:def:fomalibconf.fsm-state-list]
#[derive(Debug, Clone)]
pub struct FsmStateList {
    pub used: bool,
    pub is_final: bool,
    pub is_initial: bool,
    pub num_trans: i16,
    pub state_number: i32,
    pub fsm_trans_list: Option<Box<FsmTransList>>,
}

// [spec:foma:def:fomalibconf.fsm-sigma-list]
#[derive(Debug, Clone)]
pub struct FsmSigmaList {
    pub symbol: Option<SmolStr>,
}

// [spec:foma:def:fomalibconf.fsm-sigma-hash]
#[derive(Debug, Clone)]
pub struct FsmSigmaHash {
    /// NULL ↔ empty bucket head. In C this aliases the fsm_sigma_list entry's
    /// string; owned copy here (observably equivalent).
    pub symbol: Option<SmolStr>,
    pub sym: i16,
    pub next: Option<Box<FsmSigmaHash>>,
}

// [spec:foma:def:fomalibconf.fsm-read-binary-handle]
// C: typedef void *fsm_read_binary_handle — an opaque handle that, at every
// foma call site, points to an io.c io_buf_handle. The literal port (owned by
// the io concern) refines the raw void* into a thin owning wrapper around that
// handle. io_buf_handle is declared inside io.c, so its Rust twin
// (crate::io::IoBufHandle) lives in the io module.
#[derive(Debug)]
pub struct FsmReadBinaryHandle {
    pub iobh: Box<crate::io::IoBufHandle>,
}

// [spec:foma:def:fomalibconf.fsm-construct-handle]
#[derive(Debug, Clone)]
pub struct FsmConstructHandle {
    /// C: malloc'd array of fsm_state_list_size entries
    pub fsm_state_list: Vec<FsmStateList>,
    pub fsm_state_list_size: i32,
    /// C: malloc'd array of fsm_sigma_list_size entries
    pub fsm_sigma_list: Vec<FsmSigmaList>,
    pub fsm_sigma_list_size: i32,
    /// C: calloc'd array of fsm_sigma_hash_size bucket heads (chained)
    pub fsm_sigma_hash: Vec<FsmSigmaHash>,
    pub fsm_sigma_hash_size: i32,
    pub maxstate: i32,
    pub maxsigma: i32,
    pub numfinals: i32,
    pub hasinitial: i32,
    pub name: Option<SmolStr>,
}

/// A* agenda node (declared inside struct apply_med_handle in C)
// [spec:foma:def:fomalibconf.apply-med-handle.astarnode]
#[derive(Debug, Clone)]
pub struct Astarnode {
    pub wordpos: i16,
    pub fsmstate: i32,
    pub f: i16,
    pub g: i16,
    pub h: i16,
    pub r#in: i32,
    pub out: i32,
    pub parent: i32,
}

// [spec:foma:def:fomalibconf.apply-med-handle]
#[derive(Debug, Clone)]
pub struct ApplyMedHandle {
    /// C: malloc'd array of agenda_size astarnodes
    pub agenda: Vec<Astarnode>,
    pub bytes_per_letter_array: i32,
    pub letterbits: Vec<u8>,
    pub nletterbits: Vec<u8>,
    pub astarcount: i32,
    pub heapcount: i32,
    pub heap_size: i32,
    pub agenda_size: i32,
    pub maxdepth: i32,
    pub maxsigma: i32,
    pub wordlen: i32,
    pub utf8len: i32,
    pub cost: i32,
    pub nummatches: i32,
    pub curr_state: i32,
    pub curr_g: i32,
    pub curr_pos: i32,
    pub lines: i32,
    pub curr_agenda_offset: i32,
    pub curr_node_has_match: i32,
    pub med_limit: i32,
    pub med_cutoff: i32,
    pub med_max_heap_size: i32,
    pub nodes_expanded: i32,
    // DEVIATION from C (aliases net->medlookup->confusion_matrix; owned copy here)
    pub cm: Vec<i32>,
    // DEVIATION from C (aliases the caller's word — medh->word = word, no strdup; owned copy here)
    pub word: Option<SmolStr>,
    pub instring: String,
    pub outstring: String,
    pub align_symbol: Option<SmolStr>,
    /// C: malloc'd array of heap_size ints
    pub heap: Vec<i32>,
    pub intword: Vec<i32>,
    pub sigmahash: Option<Box<ShHandle>>,
    /// C: malloc'd array (map_firstlines), one entry per state
    pub state_array: Vec<StateArray>,
    // DEVIATION from C (borrowed pointer to the stack-owned net; the handle never owns it)
    pub net: Option<Box<Fsm>>,
    /// Cursor: index into net's line table (C: struct fsm_state *; None ↔ NULL)
    pub curr_ptr: Option<usize>,
    pub hascm: bool,
}

/// Byte-trie node over symbol strings (declared inside struct apply_handle in C)
// [spec:foma:def:fomalibconf.apply-handle.sigma-trie]
#[derive(Debug, Clone)]
pub struct SigmaTrie {
    pub signum: i32,
    // DEVIATION from C (next points into 256-entry node arrays owned by sigma_trie_arrays; owned chain here)
    pub next: Option<Box<SigmaTrie>>,
}

/// (declared inside struct apply_handle in C)
// [spec:foma:def:fomalibconf.apply-handle.sigmatch-array]
#[derive(Debug, Clone)]
pub struct SigmatchArray {
    pub signumber: i32,
    pub consumes: i32,
}

/// Bookkeeping list of the 256-entry sigma_trie node arrays, kept for
/// freeing in C (declared inside struct apply_handle)
// [spec:foma:def:fomalibconf.apply-handle.sigma-trie-arrays]
#[derive(Debug, Clone)]
pub struct SigmaTrieArrays {
    /// C: pointer to a calloc'd array of 256 sigma_trie nodes
    pub arr: Vec<SigmaTrie>,
    pub next: Option<Box<SigmaTrieArrays>>,
}

/// Indexed sigma symbols (declared inside struct apply_handle in C)
// [spec:foma:def:fomalibconf.apply-handle.sigs]
#[derive(Debug, Clone)]
pub struct Sigs {
    // DEVIATION from C (aliases sigma symbol strings / the handle's epsilon_symbol / static "?" and "@"; owned copies here)
    pub symbol: Option<SmolStr>,
    pub length: i32,
}

/// Per-state arc index chain node (declared inside struct apply_handle in C)
// [spec:foma:def:fomalibconf.apply-handle.apply-state-index]
#[derive(Debug, Clone)]
pub struct ApplyStateIndex {
    pub fsmptr: i32,
    pub next: Option<Box<ApplyStateIndex>>,
}

/// (declared inside struct apply_handle in C)
// [spec:foma:def:fomalibconf.apply-handle.flag-list]
// DEVIATION from C (struct flag_list is a name-keyed linked list walked with
// `next`; the port stores the same per-feature (value, neg) state in a
// HashMap<String, FlagState> keyed by feature name — O(1) lookup, no node
// plumbing. Feature order never affects apply output, so the map is
// observably equivalent to the list.)
#[derive(Debug, Clone, Default)]
pub struct FlagState {
    pub value: Option<SmolStr>,
    pub neg: i16,
}

/// (declared inside struct apply_handle in C)
// [spec:foma:def:fomalibconf.apply-handle.flag-lookup]
#[derive(Debug, Clone)]
pub struct FlagLookup {
    pub r#type: i32,
    pub name: Option<SmolStr>,
    pub value: Option<SmolStr>,
}

/// (declared inside struct apply_handle in C)
// [spec:foma:def:fomalibconf.apply-handle.searchstack]
#[derive(Debug, Clone)]
pub struct Searchstack {
    pub offset: i32,
    // DEVIATION from C (aliases a node inside index_in/index_out chains; owned copy here)
    pub iptr: Option<Box<ApplyStateIndex>>,
    pub state_has_index: i32,
    pub opos: i32,
    pub ipos: i32,
    pub visitmark: i32,
    // DEVIATION from C (flagname/flagvalue alias flag strings owned elsewhere in the handle; owned copies here)
    pub flagname: Option<SmolStr>,
    pub flagvalue: Option<SmolStr>,
    pub flagneg: i32,
}

/// One upper:lower arc segment recorded by apply_append during two-sided
/// enumeration. `offset` is the outstring byte position the segment was
/// appended at, used to discard segments a backtracked branch abandoned.
/// No C counterpart (C round-tripped pair output through in-band control
/// bytes that collided with symbol text containing those bytes).
#[derive(Debug, Clone)]
pub struct PairSegment {
    pub offset: u32,
    pub upper: String,
    /// None = identity: the upper text counts for both sides.
    pub lower: Option<String>,
}

// [spec:foma:def:fomalibconf.apply-handle]
#[derive(Debug, Clone)]
pub struct ApplyHandle {
    pub ptr: i32,
    pub curr_ptr: i32,
    pub ipos: i32,
    pub opos: i32,
    pub mode: i32,
    pub printcount: i32,
    pub numlines: Vec<i32>,
    pub statemap: Vec<i32>,
    pub marks: Vec<i32>,

    // DEVIATION from C (root pointer to a 256-entry node array also tracked in sigma_trie_arrays; owned root array here)
    pub sigma_trie: Vec<SigmaTrie>,
    /// C: malloc'd array of sigmatch_array_size entries
    pub sigmatch_array: Vec<SigmatchArray>,
    pub sigma_trie_arrays: Option<Box<SigmaTrieArrays>>,

    pub binsearch: i32,
    pub indexed: i32,
    pub state_has_index: i32,
    pub sigma_size: i32,
    pub sigmatch_array_size: i32,
    pub current_instring_length: i32,
    pub has_flags: i32,
    pub obey_flags: i32,
    pub show_flags: i32,
    pub print_space: i32,
    pub space_symbol: Option<SmolStr>,
    pub separator: Option<SmolStr>,
    pub epsilon_symbol: Option<SmolStr>,
    pub print_pairs: i32,
    /// Record PairSegments during two-sided enumeration (apply_set_collect_pairs).
    pub collect_pairs: bool,
    pub pair_segments: Vec<PairSegment>,
    pub apply_stack_ptr: i32,
    pub apply_stack_top: i32,
    pub oldflagneg: i32,
    pub iterate_old: i32,
    pub iterator: i32,
    /// Bit array: one bit per state that has a flag transition
    pub flagstates: Vec<u8>,
    pub outstring: String,
    pub instring: String,
    /// C: malloc'd array of sigma_size entries
    pub sigs: Vec<Sigs>,
    // DEVIATION from C (aliases a flag_list value string; owned copy here)
    pub oldflagvalue: Option<SmolStr>,

    // DEVIATION from C (borrowed pointer to the stack-owned net; the handle never owns it)
    pub last_net: Option<Box<Fsm>>,
    // DEVIATION from C (gstates = net->states, an interior pointer; base index into last_net's line table)
    pub gstates: usize,
    // DEVIATION from C (gsigma = net->sigma, an aliased pointer; owned copy here)
    pub gsigma: Vec<Sigma>,
    /// C: struct apply_state_index ** — malloc'd array of chain heads, one per state
    pub index_in: Vec<Option<Box<ApplyStateIndex>>>,
    /// C: struct apply_state_index ** — malloc'd array of chain heads, one per state
    pub index_out: Vec<Option<Box<ApplyStateIndex>>>,
    // DEVIATION from C (aliases a node inside index_in/index_out chains; owned copy here)
    pub iptr: Option<Box<ApplyStateIndex>>,

    /// Per-feature flag state, keyed by feature name (C: struct flag_list *).
    pub flag_state: std::collections::HashMap<SmolStr, FlagState>,
    /// C: malloc'd array indexed by sigma number
    pub flag_lookup: Vec<FlagLookup>,

    /// C: malloc'd array of apply_stack_top entries
    pub searchstack: Vec<Searchstack>,

    /// C read the process-global libc `rand()` state, which `apply_init`
    /// seeds with `srand(time(NULL))`; the handle owns that LCG here.
    pub lcg: crate::dynarray::Lcg,
}

/* ------------------------------------------------------------------ */
/* foma.h types                                                        */
/* ------------------------------------------------------------------ */

/// User stack entry (doubly-linked list with a sentinel)
// [spec:foma:def:foma.stack-entry]
#[derive(Debug, Clone)]
pub struct StackEntry {
    pub number: i32,
    pub ah: Option<Box<ApplyHandle>>,
    pub amedh: Option<Box<ApplyMedHandle>>,
    pub fsm: Option<Box<Fsm>>,
    // DEVIATION from C (the doubly-linked list is stored in a thread_local arena
    // in crate::stack; `next`/`previous` are arena indices — "struct stack_entry *"
    // pointer walks become index walks — not owning Box pointers, since safe Rust
    // cannot express a Box that is both forward-owned via `next` and back-aliased
    // via `previous`. None ↔ NULL. See crate::stack for the sentinel discipline.)
    pub next: Option<usize>,
    pub previous: Option<usize>,
}
