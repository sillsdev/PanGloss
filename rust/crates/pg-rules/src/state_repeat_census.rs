//! `HC_STATE_REPEAT_CENSUS=<path>` diagnostic (env-gated, measurement only, never changes a result):
//! how much analysis search work is spent expanding a state that an earlier node of the same
//! stratum call already expanded.
//!
//! A node is a word handed to `apply_mrules` or `apply_templates`, keyed by its
//! `AnalysisStateKey` (the order-independent key the stratum's fold uses). Within one stratum
//! `analyze` call the first node with a key is "first"; later ones are repeats, classed by how
//! their trail differs from the first's. The work an outermost repeat's expansion consumed is what
//! sharing equal-state expansions could at most remove. Each repeat's sink push count is compared
//! with the first's as a soundness probe: a mismatch means something outside the key decides the
//! subtree. One TSV row per `analyze` call is appended to the path.
//!
//! Zero cost when unset: [`enabled`] caches one env read per thread.

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use pg_grammar_model::model::{AllomorphId, MRuleId};
use rustc_hash::FxHashMap;

const ENV: &str = "HC_STATE_REPEAT_CENSUS";
const HEADER: &str = "word\tcall\tstratum\torder\tnodes_m\trepeats_m\tnodes_t\trepeats_t\t\
    rep_allo\trep_order\trep_both\trep_other\toutermost_repeats\twork_total\t\
    wur_allo\twur_order\twur_both\twur_other\tpush_compared\tpush_mismatch\t\
    cascade_words\tcascade_distinct_states\tcascade_work\tcapped";
/// Mismatch examples printed to stderr per thread; the counter column carries the rest.
const MAX_EXAMPLES: u32 = 5;

/// Which of the two expanding calls a node entered.
#[derive(Clone, Copy)]
pub enum Hook {
    Mrules = 0,
    Templates = 1,
}

/// How a repeat's trail differs from the first node's trail for the same key.
#[derive(Clone, Copy)]
enum Class {
    /// Same rule sequence, some allomorph differs.
    Allo = 0,
    /// Same rule and allomorph pairs, different sequence.
    Order = 1,
    /// Same rule multiset in a different sequence, with different allomorphs.
    Both = 2,
    /// Identical trails, or rule multisets that differ (saturated counts).
    Other = 3,
}

/// Order-sensitive and order-free fingerprints of a trail, so no trail is retained per node.
#[derive(Clone, Copy)]
struct Sig {
    full: u64,
    rules_seq: u64,
    rules_set: u64,
    pairs_set: u64,
}

struct First {
    sig: Sig,
    trail: Box<[(u32, u32)]>,
    /// Pushes of the first expansion; `None` until it ends, or when it was cut short.
    pushes: Option<u64>,
}

/// Handed from [`enter`] to [`exit`]; inert when no call is open.
pub struct Token {
    live: bool,
    hook: Hook,
    key: u64,
    work0: u64,
    pushes0: u64,
    first: bool,
    repeat: Option<(Class, bool)>,
    trail: Box<[(u32, u32)]>,
}

#[derive(Default)]
struct Call {
    call: u64,
    stratum: u32,
    linear: bool,
    work0: u64,
    nodes: [u64; 2],
    repeats: [u64; 2],
    class: [u64; 4],
    outermost: u64,
    work_under: [u64; 4],
    compared: u64,
    mismatch: u64,
    cascade_words: u64,
    cascade_distinct: u64,
    cascade_work: u64,
    depth: u32,
    seen: [FxHashMap<u64, First>; 2],
}

thread_local! {
    static ENABLED: Cell<Option<bool>> = const { Cell::new(None) };
    static CALL: RefCell<Option<Call>> = const { RefCell::new(None) };
    static WORD: RefCell<String> = const { RefCell::new(String::new()) };
    static CALLS_THIS_WORD: Cell<u64> = const { Cell::new(0) };
    static PUSHES: Cell<u64> = const { Cell::new(0) };
    static EXAMPLES: Cell<u32> = const { Cell::new(0) };
}

static OUT: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

/// Cached `HC_STATE_REPEAT_CENSUS` read (one env lookup per thread).
pub fn enabled() -> bool {
    ENABLED.with(|c| {
        if let Some(v) = c.get() {
            return v;
        }
        let v = std::env::var_os(ENV).is_some();
        c.set(Some(v));
        v
    })
}

/// Name the input word the following `analyze` calls belong to, and restart the call numbering.
pub fn set_word(word: &str) {
    if !enabled() {
        return;
    }
    WORD.with(|w| {
        let mut w = w.borrow_mut();
        w.clear();
        w.push_str(word);
    });
    CALLS_THIS_WORD.with(|c| c.set(0));
}

/// A stable hash of anything hashable, used to name a state without keeping it.
pub fn hash_of<T: Hash>(value: &T) -> u64 {
    let mut h = DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}

/// Count one word pushed into a stratum sink.
pub fn note_push() {
    PUSHES.with(|c| c.set(c.get() + 1));
}

/// Open the record for one stratum `analyze` call.
pub fn begin_call(stratum: u32, linear: bool, work_now: u64) {
    let call = CALLS_THIS_WORD.with(|c| {
        c.set(c.get() + 1);
        c.get()
    });
    CALL.with(|c| {
        *c.borrow_mut() = Some(Call {
            call,
            stratum,
            linear,
            work0: work_now,
            ..Call::default()
        });
    });
}

fn trail_pairs(rules: &[Option<MRuleId>], allomorphs: &[Option<AllomorphId>]) -> Box<[(u32, u32)]> {
    // `None` (an unknown compounding rule, or no allomorph) maps to a value no id takes.
    rules
        .iter()
        .zip(allomorphs)
        .map(|(r, a)| (r.map_or(u32::MAX, |r| r.0), a.map_or(u32::MAX, |a| a.0)))
        .collect()
}

fn signature(trail: &[(u32, u32)]) -> Sig {
    let mut sig = Sig {
        full: hash_of(&trail),
        rules_seq: 0,
        rules_set: 0,
        pairs_set: 0,
    };
    let rules: Vec<u32> = trail.iter().map(|p| p.0).collect();
    sig.rules_seq = hash_of(&rules);
    for pair in trail {
        sig.rules_set = sig.rules_set.wrapping_add(hash_of(&pair.0));
        sig.pairs_set = sig.pairs_set.wrapping_add(hash_of(pair));
    }
    sig
}

fn classify(first: &Sig, repeat: &Sig) -> Class {
    if first.full == repeat.full {
        Class::Other
    } else if first.rules_seq == repeat.rules_seq {
        Class::Allo
    } else if first.rules_set == repeat.rules_set {
        if first.pairs_set == repeat.pairs_set {
            Class::Order
        } else {
            Class::Both
        }
    } else {
        Class::Other
    }
}

/// Register one node about to be expanded; `key` is the hash of its state key.
pub fn enter(
    hook: Hook,
    key: u64,
    rules: &[Option<MRuleId>],
    allomorphs: &[Option<AllomorphId>],
    work_now: u64,
) -> Token {
    let trail = trail_pairs(rules, allomorphs);
    let mut token = Token {
        live: false,
        hook,
        key,
        work0: work_now,
        pushes0: PUSHES.with(Cell::get),
        first: false,
        repeat: None,
        trail: Box::default(),
    };
    CALL.with(|c| {
        let mut c = c.borrow_mut();
        let Some(call) = c.as_mut() else { return };
        token.live = true;
        let h = hook as usize;
        call.nodes[h] += 1;
        let sig = signature(&trail);
        match call.seen[h].get(&key) {
            None => {
                call.seen[h].insert(
                    key,
                    First {
                        sig,
                        trail,
                        pushes: None,
                    },
                );
                token.first = true;
            }
            Some(first) => {
                let class = classify(&first.sig, &sig);
                call.repeats[h] += 1;
                call.class[class as usize] += 1;
                token.repeat = Some((class, call.depth == 0));
                call.depth += 1;
                token.trail = trail;
            }
        }
    });
    token
}

/// Close a node's expansion; `truncated` is true when a cap, stop or full sink cut it short.
pub fn exit(token: Token, work_now: u64, truncated: bool) {
    if !token.live {
        return;
    }
    let pushes = PUSHES.with(Cell::get) - token.pushes0;
    let h = token.hook as usize;
    CALL.with(|c| {
        let mut c = c.borrow_mut();
        let Some(call) = c.as_mut() else { return };
        if token.first {
            if let (false, Some(first)) = (truncated, call.seen[h].get_mut(&token.key)) {
                first.pushes = Some(pushes);
            }
            return;
        }
        let Some((class, outermost)) = token.repeat else {
            return;
        };
        call.depth = call.depth.saturating_sub(1);
        if outermost {
            call.outermost += 1;
            call.work_under[class as usize] += work_now.saturating_sub(token.work0);
        }
        let first = call.seen[h].get(&token.key);
        if let (false, Some(first)) = (truncated, first) {
            if let Some(expected) = first.pushes {
                call.compared += 1;
                if expected != pushes {
                    call.mismatch += 1;
                    let n = EXAMPLES.with(|e| {
                        e.set(e.get() + 1);
                        e.get()
                    });
                    if n <= MAX_EXAMPLES {
                        let word = WORD.with(|w| w.borrow().clone());
                        eprintln!(
                            "STATE_REPEAT_MISMATCH\tword={word}\tstratum={}\thook={}\t\
                             first_pushes={expected}\trepeat_pushes={pushes}\t\
                             first_trail={:?}\trepeat_trail={:?}",
                            call.stratum, h, first.trail, token.trail
                        );
                    }
                }
            }
        }
    });
}

/// Record one mrule cascade's output: its word count, its distinct state keys, and its own work.
pub fn cascade(words: u64, distinct_states: u64, work: u64) {
    CALL.with(|c| {
        if let Some(call) = c.borrow_mut().as_mut() {
            call.cascade_words += words;
            call.cascade_distinct += distinct_states;
            call.cascade_work += work;
        }
    });
}

/// Close the open `analyze` call and append its row.
pub fn end_call(work_now: u64, capped: bool) {
    let Some(call) = CALL.with(|c| c.borrow_mut().take()) else {
        return;
    };
    let word = WORD.with(|w| w.borrow().clone());
    let row = format!(
        "{word}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        call.call,
        call.stratum,
        if call.linear { "linear" } else { "unordered" },
        call.nodes[0],
        call.repeats[0],
        call.nodes[1],
        call.repeats[1],
        call.class[0],
        call.class[1],
        call.class[2],
        call.class[3],
        call.outermost,
        work_now.saturating_sub(call.work0),
        call.work_under[0],
        call.work_under[1],
        call.work_under[2],
        call.work_under[3],
        call.compared,
        call.mismatch,
        call.cascade_words,
        call.cascade_distinct,
        call.cascade_work,
        u8::from(capped),
    );
    let out = OUT.get_or_init(|| {
        let path = std::env::var_os(ENV).unwrap_or_default();
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap_or_else(|e| panic!("{ENV}: cannot open {}: {e}", path.to_string_lossy()));
        if file.metadata().map(|m| m.len() == 0).unwrap_or(false) {
            let _ = writeln!(file, "{HEADER}");
        }
        Mutex::new(file)
    });
    if let Ok(mut file) = out.lock() {
        let _ = file.write_all(row.as_bytes());
    }
}
