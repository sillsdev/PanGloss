//! `FomaProposer`: the thin `emit + foma-compile + apply-up` wrapper (plan §1's "propose" half of
//! propose→confirm; confirm itself is P2's job, not built here).
//!
//! Compiles [`crate::emit::emit`]'s lexc source with the pure-Rust `foma` crate (gate F0) and
//! exposes [`FomaProposer::propose`]: normalize the query word the SAME way [`crate::emit`]
//! normalized surface text (NFD — see that module's doc), `apply_up` it, decode every resulting
//! tag path, and split each into [`tags::Candidate`]s, deduped by `(morphemes, root_index)`
//! preserving first-seen order (matching the propose→verify contract, plan §2: "Allomorph IDs are
//! NOT part of candidate identity").

use std::collections::HashSet;
use std::fmt;

use foma::apply::apply_init;
use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use foma::structures::fsm_sort_arcs;
use foma::types::ApplyHandle;

use pg_grammar::model::Grammar;

use crate::emit::{self, EmitReport};
use crate::tags::{self, Candidate};

/// Errors constructing a [`FomaProposer`]. Deliberately small (this stage doesn't need a rich
/// error hierarchy) — a grammar whose foma path fails to compile should fall back to the full
/// engine (plan §1's per-grammar tiering), which only needs to know THAT it failed.
#[derive(Debug)]
pub enum FomaError {
    /// `fsm_lexc_parse_string` returned `None` — the emitted lexc source failed to compile. Carries
    /// the emitter's own report (uncovered constructs, counts) since that is the first place to
    /// look when this happens.
    LexcCompileFailed(EmitReport),
}

impl fmt::Display for FomaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FomaError::LexcCompileFailed(report) => write!(
                f,
                "foma lexc compile failed (emit report: {} uncovered constructs, tier {:?})",
                report.uncovered.len(),
                report.tier
            ),
        }
    }
}

impl std::error::Error for FomaError {}

pub type Result<T> = std::result::Result<T, FomaError>;

/// Minimum arc count before `FomaProposer::new` pays `fsm_sort_arcs`'s one-time cost to switch
/// `apply_up`'s per-word traversal from foma's linear arc-scan branch to its binary-search branch
/// (gated on `net.arcs_sorted_out`, apply.rs's `apply_up`/`apply_follow_next_arc`).
///
/// Measured (prototype tracer, `examples/sort_probe.rs`): sorting is a clear win on real grammars
/// — sena (85,763 arcs) 1.49x propose speedup, amharic (177,177 arcs) 2.05x — with traversal-
/// identical results (states-entered and candidate sets identical, sorted vs. unsorted). But on a
/// tiny network (indonesian, 3,263 arcs, ~337 arcs examined/word) the binary-search bookkeeping
/// OUTWEIGHS the win: propose throughput regressed ~30%. This constant gates the sort so small
/// grammars stay on the (cheaper, for them) linear scan while large ones get the binary-search
/// speedup. 10,000 sits comfortably between indonesian's 3,263 (stays unsorted) and sena's 85,763
/// (gets sorted).
const ARC_SORT_MIN_ARCS: i32 = 10_000;

/// The compiled foma network for one grammar (as a live [`ApplyHandle`], see below), plus the
/// emitter's own report (uncovered constructs, counts, tier — plan P1 gate F1's "counts are
/// plausible" assertions read this).
pub struct FomaProposer {
    // Built ONCE in `new` via `apply_init` and reused across every `propose` call (see that
    // method's doc for why this is sound). `ApplyHandle` owns a full clone of the compiled `Fsm`
    // (`foma::apply::apply_init`'s doc: "DEVIATION from C (owns a clone; the handle never mutates
    // it, so observably equivalent for application)") plus its own grammar-static index tables
    // (`apply_create_statemap`/`apply_create_sigarray`, built once inside `apply_init` itself) —
    // it is fully owned/`'static`, not a borrow of any `Fsm` this struct would also need to store,
    // so there is no self-referential-struct trap here: the original `Box<Fsm>` `fsm_lexc_parse_string`
    // returned is consumed by `apply_init` and can be (is) dropped once the handle exists.
    handle: Box<ApplyHandle>,
    pub report: EmitReport,
}

impl FomaProposer {
    /// Emit `g`'s lexc source, compile it, and build the (word-independent) `ApplyHandle` once.
    /// `Err` iff `foma`'s lexc compiler itself rejects the source (a bug in this crate's emitter,
    /// not a grammar-content problem — the emitter's own `uncovered` list is how grammar CONTENT
    /// gaps are reported, always alongside `Ok`).
    pub fn new(g: &Grammar) -> Result<Self> {
        let result = emit::emit(g);
        let opts = FomaOptions::default();
        match fsm_lexc_parse_string(&opts, None, &result.lexc_source) {
            Some(mut net) => {
                // direction 2 = "out": apply_up (propose's entry point) gates its binsearch
                // branch on `net.arcs_sorted_out` (apply.rs's `apply_up`, ~line 469). See
                // `ARC_SORT_MIN_ARCS`'s doc for why this is gated on network size rather than
                // called unconditionally.
                if net.arccount >= ARC_SORT_MIN_ARCS {
                    fsm_sort_arcs(&mut net, 2);
                }
                Ok(FomaProposer {
                    handle: apply_init(&net),
                    report: result.report,
                })
            }
            None => Err(FomaError::LexcCompileFailed(result.report)),
        }
    }

    /// Propose every candidate analysis for `word`. NFD-normalizes first (matching
    /// [`crate::emit::kept_surface_text`]'s own normalization — see that function's doc for why
    /// this must be consistent on both sides regardless of the caller's on-disk encoding).
    /// Dedups by `(morphemes, root_index)`, preserving first-seen order across BOTH the
    /// `apply_up` path order and, within one path, the compound-split order (`tags::to_candidates`
    /// already yields ascending root-position order for a single path).
    ///
    /// Reuses `self.handle` across calls rather than rebuilding it per word (vendored
    /// `foma::apply::apply_init`, ~apply.rs:481-577, unconditionally deep-clones the whole
    /// compiled `Fsm` and rebuilds `apply_create_statemap`/`apply_create_sigarray` — all a
    /// function of the NETWORK only, never the word). The per-word entry point,
    /// `foma::apply::apply_up` (apply.rs:462-475, reached via `ApplyHandle::up`, apply.rs:667-669),
    /// resets only per-word state — `h.instring`, `apply_create_sigmatch` (word-derived sigma
    /// matches), and `apply_force_clear_stack` (apply.rs:424-433's `apply_updown`, the `Some(w)`
    /// arm) — leaving `last_net`/`statemap`/`sigmatch_array`/`sigma_trie` (the grammar-static
    /// tables) untouched, so repeated `up` calls on one handle are exactly the reuse this needs.
    pub fn propose(&mut self, word: &str) -> Vec<Candidate> {
        let normalized = pg_grammar::nfd::nfd(word);
        let mut seen: HashSet<(Vec<u32>, i32)> = HashSet::new();
        let mut out = Vec::new();
        for s in self.handle.up(&normalized) {
            let Some(path) = tags::decode_path(&s) else {
                continue;
            };
            for c in tags::to_candidates(&path) {
                let key: (Vec<u32>, i32) =
                    (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
                if seen.insert(key) {
                    out.push(c);
                }
            }
        }
        out
    }
}
