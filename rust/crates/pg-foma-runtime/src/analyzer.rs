//! `FomaProposer`: the thin `emit + foma-compile + apply-up` wrapper for the propose half of
//! propose→confirm; confirm itself lives elsewhere.
//!
//! Compiles `crate::emit::emit`'s lexc source with the pure-Rust `foma` crate and exposes
//! `FomaProposer::propose`: normalize the query word the same way `crate::emit` normalized
//! surface text (NFD — see that module's doc), `apply_up` it, decode every resulting tag path,
//! and split each into `tags::Candidate`s, deduped by `(morphemes, root_index)` preserving
//! first-seen order. Allomorph IDs are not part of candidate identity.

use std::collections::HashSet;
use std::fmt;
use std::time::Instant;

use foma::apply::apply_init;
use foma::structures::fsm_sort_arcs;
use foma::types::{ApplyHandle, Fsm};

use pg_grammar::chardef::{CharDefKind, CharDefTable};

use crate::compose_budget::{ApplyBudget, ApplyDimension, ApplyOutcome};
use crate::emit_report::EmitReport;
use crate::tags::{self, Candidate};

fn render_refusal(diagnostics: &[pg_health::capability::CapabilityDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|d| {
            format!(
                "predicate={} construct={} witness={}",
                d.predicate, d.construct, d.witness
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}
/// Errors constructing a `FomaProposer`. Deliberately small (this stage doesn't need a rich
/// error hierarchy) — a grammar whose foma path fails to compile should fall back to the full
/// engine (plan §1's per-grammar tiering), which only needs to know THAT it failed.
#[derive(Debug)]
pub enum FomaError {
    /// `fsm_lexc_parse_string` returned `None`; carries the emitter's own report for diagnosis.
    LexcCompileFailed(Box<EmitReport>),
    /// The emitter proved that no complete Foma artifact can be built for this grammar.
    Unsupported(Box<EmitReport>),
    /// The emitter produced lexc material but also identified constructs which can contribute
    /// analyses that material does not propose. Confirmation cannot restore omitted candidates,
    /// so normal construction must refuse this report before compiling the partial network.
    Incomplete(Box<EmitReport>),
    /// The capability envelope refused this backend before any emission ran. Unlike the three
    /// above, this verdict costs no emission and names the construct rather than a tier.
    CapabilityRefused(Vec<pg_health::capability::CapabilityDiagnostic>),
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
            FomaError::Unsupported(report) => write!(
                f,
                "foma backend unsupported for this grammar (emit report: {} uncovered constructs, tier {:?}): {}",
                report.uncovered.len(),
                report.tier,
                uncovered_constructs(report)
            ),
            FomaError::Incomplete(report) => write!(
                f,
                "foma emission is incomplete and cannot be used as a trusted proposer (emit report: {} uncovered constructs, tier {:?}): {}",
                report.uncovered.len(),
                report.tier,
                uncovered_constructs(report)
            ),
            FomaError::CapabilityRefused(diagnostics) => write!(
                f,
                "the capability envelope refuses this backend for {} construct(s), so no emission ran: {}",
                diagnostics.len(),
                render_refusal(diagnostics)
            ),
        }
    }
}

/// The constructs a report could not cover; a tier says how bad the outcome was, never what.
fn uncovered_constructs(report: &EmitReport) -> String {
    if report.uncovered.is_empty() {
        return "no construct named; see the tier's own reason".to_owned();
    }
    report
        .uncovered
        .iter()
        .map(|item| format!("[{}] {} -- {}", item.kind, item.id, item.reason))
        .collect::<Vec<_>>()
        .join("; ")
}

impl std::error::Error for FomaError {}

pub type Result<T> = std::result::Result<T, FomaError>;

/// Opt-in per-word proposal measurements. These counters describe only paths actually pulled from
/// foma before completion or a cooperative `ApplyBudget` trip.
#[derive(Clone, Debug, Default)]
pub struct ProposalDiagnostics {
    pub raw_paths: usize,
    pub raw_bytes: usize,
    pub decoded_paths: usize,
    pub malformed_paths: usize,
    pub unique_candidates: usize,
    pub traversal_elapsed: std::time::Duration,
    pub decode_dedup_elapsed: std::time::Duration,
}

/// The two magnitudes an `ApplyBudget` is denominated in, and nothing else.
///
/// Distinct from `ProposalDiagnostics` on purpose: these are counters the decode loop already
/// keeps, so reporting them is free, whereas the diagnostics clock every path. A budgeted
/// production run needs the counters to carry one cumulative budget across several proposals; it
/// does not need the timings.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProposalCounts {
    pub raw_paths: usize,
    pub unique_candidates: usize,
}

/// Minimum arc count before `FomaProposer::new` pays `fsm_sort_arcs`'s cost to switch `apply_up` to its binary-search branch.
/// Why 10,000: `docs/research/pg-foma-analyzer-design-notes.md`, "`ARC_SORT_MIN_ARCS`".
const ARC_SORT_MIN_ARCS: i32 = 10_000;

/// Prepare a compiled network for repeated `apply_up` calls when its size clears the measured
/// break-even threshold for foma's binary-search traversal path. Direction 2 sorts outgoing arcs,
/// which is the direction `apply_up` checks through `net.arcs_sorted_out`.
pub fn prepare_network_for_apply(net: &mut Fsm) {
    if net.arccount >= ARC_SORT_MIN_ARCS {
        fsm_sort_arcs(net, 2);
    }
}

/// The compiled foma network for one grammar (as a live `ApplyHandle`, see below), plus the
/// emitter's own report (uncovered constructs, counts, tier — plan P1 gate F1's "counts are
/// plausible" assertions read this).
pub struct FomaProposer {
    // Fully owned/`'static` (a clone of the compiled `Fsm`), not a borrow this struct would also need to store.
    handle: Box<ApplyHandle>,
    /// Diagnostics from the compiler that built this proposer, when that compiler produces an
    /// `EmitReport`. Plan-composed networks deliberately carry `None`: asking the tuned-surface
    /// emitter to manufacture diagnostics for a different backend can itself be unbounded work.
    pub report: Option<EmitReport>,
    query_encoder: Option<SegmentQueryEncoder>,
}

/// Owned form of `crate::replace::SegAlphabet::encode_query`: a proposer must outlive the borrowed `SegAlphabet` used to build it.
struct SegmentQueryEncoder {
    /// NFD representations, longest first, paired with their PUA token.
    representations: Vec<(Vec<char>, char)>,
    /// Declared boundary representations, longest first, for explicit-query encoding before terminal cleanup.
    boundary_representations: Vec<Vec<char>>,
}

impl SegmentQueryEncoder {
    fn new(table: &CharDefTable) -> Self {
        let mut representations: Vec<(Vec<char>, char)> = Vec::new();
        let mut boundary_representations: Vec<Vec<char>> = Vec::new();
        for (id, definition) in table.iter() {
            for representation in definition.representations_nfd() {
                match definition.kind() {
                    CharDefKind::Segment => representations.push((
                        representation.chars().collect(),
                        char::from_u32(0xE000 + id.0)
                            .expect("char table too large for the PUA token scheme"),
                    )),
                    CharDefKind::Boundary => {
                        boundary_representations.push(representation.chars().collect())
                    }
                }
            }
        }
        representations.sort_by_key(|(representation, _)| std::cmp::Reverse(representation.len()));
        boundary_representations
            .sort_by_key(|representation| std::cmp::Reverse(representation.len()));
        SegmentQueryEncoder {
            representations,
            boundary_representations,
        }
    }

    fn encode(&self, word: &str) -> Option<String> {
        let normalized: Vec<char> = pg_grammar::nfd::nfd(word).chars().collect();
        let mut encoded = String::with_capacity(normalized.len());
        let mut position = 0;
        while position < normalized.len() {
            if let Some((representation, token)) = self
                .representations
                .iter()
                .find(|(representation, _)| normalized[position..].starts_with(representation))
            {
                encoded.push(*token);
                position += representation.len();
                continue;
            }
            let representation = self
                .boundary_representations
                .iter()
                .find(|representation| normalized[position..].starts_with(representation))?;
            position += representation.len();
        }
        Some(encoded)
    }
}

impl FomaProposer {
    /// The backend `Self::new` realizes, and so the one a capability gate in front of this
    /// constructor has to consult.
    ///
    /// A whole-grammar verdict cannot answer that question: it is the best any backend offers, and
    /// this constructor offers exactly one of them. Named here rather than at the call site so the
    /// fact lives next to the emitter it describes, and moves with it. Pinned by
    /// `the_named_backend_is_the_one_this_constructor_builds`.
    pub const EMISSION_STRATEGY: pg_health::strategy::EmissionStrategy =
        pg_health::strategy::EmissionStrategy::TunedSurfaceProbed;

    /// Build a proposer around an already-compiled network. This constructor performs exactly one
    /// `apply_init` and does not emit, compile, sort, compose, or minimize the supplied network.
    pub fn from_precompiled_network(net: &foma::types::Fsm, report: EmitReport) -> Self {
        FomaProposer {
            handle: apply_init(net),
            query_encoder: None,
            report: Some(report),
        }
    }

    /// Build around a network produced by a compiler whose diagnostics are not an `EmitReport`.
    /// The caller must publish that compiler's own backend report separately; this constructor
    /// never runs another compiler merely to fill an unrelated diagnostics field.
    pub fn from_precompiled_network_without_emit_report(net: &foma::types::Fsm) -> Self {
        FomaProposer {
            handle: apply_init(net),
            query_encoder: None,
            report: None,
        }
    }

    /// Attach P6's representation-to-token query encoding to a precompiled proposer.
    pub fn with_segment_query_encoder(mut self, table: &CharDefTable) -> Self {
        self.query_encoder = Some(SegmentQueryEncoder::new(table));
        self
    }

    fn encode_query(&self, word: &str) -> Option<String> {
        match &self.query_encoder {
            Some(encoder) => encoder.encode(word),
            None => Some(pg_grammar::nfd::nfd(word)),
        }
    }

    /// Propose every candidate analysis for `word`. NFD-normalizes first (matching
    /// `crate::emit::kept_surface_text`'s own normalization — see that function's doc for why
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
        match self.propose_budgeted(word, &ApplyBudget::unbounded()) {
            ApplyOutcome::Complete(candidates) => candidates,
            ApplyOutcome::Incomplete { .. } => {
                unreachable!("ApplyBudget::unbounded() can never report Incomplete")
            }
        }
    }

    /// `Self::propose`'s core, plus in-process cooperative magnitude containment
    /// (`crate::compose_budget`'s own "Apply-path dimension" section doc): checks `budget`'s two
    /// magnitude dimensions -- raw decoded-path count, distinct-candidate count -- as this word's
    /// `apply_up` result iterator is walked, returning `ApplyOutcome::Incomplete` the instant
    /// either cap is exceeded rather than continuing to decode/allocate further for this word. This
    /// is deliberately NOT a watchdog: there is no worker process to spawn or kill here (a native
    /// thread cannot be safely hard-killed in Rust; this method runs entirely in the
    /// caller's own process, on `self.handle`, exactly like `Self::propose` always has) -- the
    /// containment is a plain deterministic counter, checked cooperatively, the same discipline
    /// `ComposeBudget::check_chain_depth` already uses one call stack over in the compile-time
    /// composition path.
    ///
    /// `ApplyBudget::unbounded` (what `Self::propose` passes) can never report `Incomplete` --
    /// every check below is `Some(cap) if count > cap`, so a `None` cap is always `false` -- which
    /// is exactly how `Self::propose` proves its own behavior is unchanged by this addition
    /// without duplicating the decode loop.
    pub fn propose_budgeted(
        &mut self,
        word: &str,
        budget: &ApplyBudget,
    ) -> ApplyOutcome<Vec<Candidate>> {
        self.propose_budgeted_counted(word, budget).0
    }

    /// `Self::propose_budgeted` with the two magnitudes it consumed, and nothing else.
    ///
    /// A budgeted *production* run needs one cumulative budget spanning the direct proposal and
    /// every proposal reduplication peeling requests, which means each call has to report what it
    /// spent. `Self::propose_with_diagnostics_budgeted` already reports that, but it calls
    /// `Instant::now()` twice per raw path — on a word that decodes a hundred thousand paths that
    /// is two hundred thousand clock reads bought for a number nobody asked for. These are plain
    /// counters the decode loop was already keeping.
    ///
    /// `unique_candidates` is the count at the point of return, so a trip reports the magnitude
    /// that tripped rather than a truncated set's length.
    pub fn propose_budgeted_counted(
        &mut self,
        word: &str,
        budget: &ApplyBudget,
    ) -> (ApplyOutcome<Vec<Candidate>>, ProposalCounts) {
        let Some(normalized) = self.encode_query(word) else {
            return (
                ApplyOutcome::Complete(Vec::new()),
                ProposalCounts::default(),
            );
        };
        let mut seen: HashSet<(Vec<u32>, i32)> = HashSet::new();
        let mut out = Vec::new();
        let mut counts = ProposalCounts::default();
        for s in self.handle.up(&normalized) {
            counts.raw_paths += 1;
            if let Some(limit) = budget.path_cap() {
                if counts.raw_paths > limit {
                    return (
                        ApplyOutcome::Incomplete {
                            dimension: ApplyDimension::DecodedPaths,
                            value: counts.raw_paths,
                            limit,
                        },
                        counts,
                    );
                }
            }
            let Some(path) = tags::decode_path(&s) else {
                continue;
            };
            for c in tags::to_candidates(&path) {
                let key: (Vec<u32>, i32) =
                    (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
                if seen.insert(key) {
                    out.push(c);
                    counts.unique_candidates = out.len();
                    if let Some(limit) = budget.candidate_cap() {
                        if out.len() > limit {
                            return (
                                ApplyOutcome::Incomplete {
                                    dimension: ApplyDimension::Candidates,
                                    value: out.len(),
                                    limit,
                                },
                                counts,
                            );
                        }
                    }
                }
            }
        }
        (ApplyOutcome::Complete(out), counts)
    }

    /// `Self::propose_budgeted` with opt-in path, byte, decode, dedup, and timing diagnostics.
    /// Budget dimensions and first-seen candidate order are identical to the ordinary path.
    pub fn propose_with_diagnostics_budgeted(
        &mut self,
        word: &str,
        budget: &ApplyBudget,
    ) -> (ApplyOutcome<Vec<Candidate>>, ProposalDiagnostics) {
        let Some(normalized) = self.encode_query(word) else {
            return (
                ApplyOutcome::Complete(Vec::new()),
                ProposalDiagnostics::default(),
            );
        };
        let mut seen: HashSet<(Vec<u32>, i32)> = HashSet::new();
        let mut out = Vec::new();
        let mut diagnostics = ProposalDiagnostics::default();
        let mut paths = self.handle.up(&normalized);

        loop {
            let traversal_start = Instant::now();
            let raw = paths.next();
            diagnostics.traversal_elapsed += traversal_start.elapsed();
            let Some(raw) = raw else { break };

            let decode_start = Instant::now();
            diagnostics.raw_paths += 1;
            diagnostics.raw_bytes += raw.len();
            let path = match tags::decode_path(&raw) {
                Some(path) => {
                    diagnostics.decoded_paths += 1;
                    Some(path)
                }
                None => {
                    diagnostics.malformed_paths += 1;
                    None
                }
            };

            if let Some(limit) = budget.path_cap() {
                if diagnostics.raw_paths > limit {
                    diagnostics.decode_dedup_elapsed += decode_start.elapsed();
                    return (
                        ApplyOutcome::Incomplete {
                            dimension: ApplyDimension::DecodedPaths,
                            value: diagnostics.raw_paths,
                            limit,
                        },
                        diagnostics,
                    );
                }
            }

            let Some(path) = path else {
                diagnostics.decode_dedup_elapsed += decode_start.elapsed();
                continue;
            };
            for candidate in tags::to_candidates(&path) {
                let key = (
                    candidate.morphemes.iter().map(|m| m.0).collect(),
                    candidate.root_index,
                );
                if seen.insert(key) {
                    out.push(candidate);
                    diagnostics.unique_candidates = out.len();
                    if let Some(limit) = budget.candidate_cap() {
                        if out.len() > limit {
                            diagnostics.decode_dedup_elapsed += decode_start.elapsed();
                            return (
                                ApplyOutcome::Incomplete {
                                    dimension: ApplyDimension::Candidates,
                                    value: out.len(),
                                    limit,
                                },
                                diagnostics,
                            );
                        }
                    }
                }
            }
            diagnostics.decode_dedup_elapsed += decode_start.elapsed();
        }

        (ApplyOutcome::Complete(out), diagnostics)
    }

    /// Serializes this proposer's own compiled network to foma's existing binary-memory encoding
    /// (`foma::io::fsm_write_binary` — the same gzip'd format `fsm_read_binary_mem` reads):
    /// foma's tested binary-memory representation is reused
    /// inside a PanGloss envelope rather than inventing another network encoding. This is the
    /// REAL foma payload a `.pgpack` container carries (nothing writes one today) — no second network
    /// format, no fabricated bytes.
    ///
    /// `self.handle.last_net` is always `Some` here: `apply_init` (called by every constructor
    /// above, immediately after a successful `fsm_lexc_parse_string`) unconditionally populates it
    /// with a clone of the just-compiled network before returning the handle — see `apply_init`'s
    /// own doc, "C: h->last_net = net (borrowed). DEVIATION from C (owns a clone...)". There is no
    /// code path that constructs a `FomaProposer` without going through `apply_init` first.
    pub fn foma_binary_payload(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        foma::io::fsm_write_binary(self.network(), &mut bytes)?;
        Ok(bytes)
    }

    /// `(statecount, arccount)` of this proposer's own compiled network — a cheap struct-field
    /// read (both are `Copy` `i32` fields), exposed so a caller can compare a freshly-compiled
    /// network's shape against one reconstructed from a serialized payload
    /// (`read_foma_binary_payload`) without either side needing its own `foma` crate dependency.
    pub fn network_counts(&self) -> (i32, i32) {
        let net = self.network();
        (net.statecount, net.arccount)
    }

    /// This proposer's own compiled network, as built by `apply_init` (`Self::foma_binary_payload`'s doc explains why `last_net` is always `Some` here).
    fn network(&self) -> &foma::types::Fsm {
        self.handle.last_net.as_ref().expect(
            "FomaProposer::handle is always built by apply_init, which unconditionally sets \
             ApplyHandle::last_net to a clone of the compiled network",
        )
    }
}

/// Reads a foma binary-memory payload back into a live `foma::types::Fsm` — the read side of
/// `FomaProposer::foma_binary_payload`, exposed here (rather than requiring every caller to add
/// its own direct `foma` crate dependency) so `pg-pack`/`pg-cli` round-trip tests, and eventually a
/// packaged-analyzer loader, can reconstruct the compiled network from `.pgpack` bytes using the
/// SAME entry point (`fsm_read_binary_mem`), never a
/// second parser.
pub fn read_foma_binary_payload(
    bytes: &[u8],
) -> std::result::Result<foma::types::Fsm, foma::error::FomaError> {
    foma::io::fsm_read_binary_mem(bytes)
}
