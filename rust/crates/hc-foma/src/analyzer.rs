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
use foma::types::Fsm;

use hc_grammar::model::Grammar;

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

/// The compiled foma network for one grammar, plus the emitter's own report (uncovered
/// constructs, counts, tier — plan P1 gate F1's "counts are plausible" assertions read this).
pub struct FomaProposer {
    net: Box<Fsm>,
    pub report: EmitReport,
}

impl FomaProposer {
    /// Emit `g`'s lexc source and compile it. `Err` iff `foma`'s lexc compiler itself rejects the
    /// source (a bug in this crate's emitter, not a grammar-content problem — the emitter's own
    /// `uncovered` list is how grammar CONTENT gaps are reported, always alongside `Ok`).
    pub fn new(g: &Grammar) -> Result<Self> {
        let result = emit::emit(g);
        let opts = FomaOptions::default();
        match fsm_lexc_parse_string(&opts, None, &result.lexc_source) {
            Some(net) => Ok(FomaProposer {
                net,
                report: result.report,
            }),
            None => Err(FomaError::LexcCompileFailed(result.report)),
        }
    }

    /// Propose every candidate analysis for `word`. NFD-normalizes first (matching
    /// [`crate::emit::kept_surface_text`]'s own normalization — see that function's doc for why
    /// this must be consistent on both sides regardless of the caller's on-disk encoding).
    /// Dedups by `(morphemes, root_index)`, preserving first-seen order across BOTH the
    /// `apply_up` path order and, within one path, the compound-split order (`tags::to_candidates`
    /// already yields ascending root-position order for a single path).
    pub fn propose(&mut self, word: &str) -> Vec<Candidate> {
        let normalized = hc_grammar::nfd::nfd(word);
        let mut handle = apply_init(&self.net);
        let mut seen: HashSet<(Vec<u32>, i32)> = HashSet::new();
        let mut out = Vec::new();
        for s in handle.up(&normalized) {
            let Some(path) = tags::decode_path(&s) else {
                continue;
            };
            for c in tags::to_candidates(&path) {
                let key: (Vec<u32>, i32) = (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
                if seen.insert(key) {
                    out.push(c);
                }
            }
        }
        out
    }
}
