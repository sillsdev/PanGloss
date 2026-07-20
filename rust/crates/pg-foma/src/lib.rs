//! Phase P0 viability spike (docs/fst-plan/foma-fst-plan.md §P0, gate F0), P1 stage 1 (emitter core
//! + Sena, gate F1), P1 stage 2 (junction-aware phonology + Indonesian, gate F1), P1d (Amharic
//! capability stage — rule-application pre-expansion + boundary-fusion composites), and P2
//! (propose→confirm composite, gate F2).
//!
//! - [`tags`]: the `<R:nnnn>`/`<M:nnnn>` tag codec (D2) — escaped lexc spellings, decoded literal
//!   text, an `apply_up`-output decoder, and the `Candidate` split for compound (multi-root) paths.
//! - [`emit`]: `Grammar -> lexc source` (D3) — see that module's doc for the full design.
//! - [`junctions`]: [`junctions::PhonologyProbe`], the pre-probed surface-variant/deletion-junction
//!   machinery `emit` drives for a grammar with real phonological rules (stage 2) — a `None`-safe
//!   no-op for a grammar without any (stage 1's Sena stays byte-identical).
//! - [`morphotactics`] (crate-internal, `docs/fst-plan/morphotactic-composite-pruning.md`): the
//!   `MorphotacticIndex`/`ChainState` subset-construction automaton over the engine's own
//!   `pg-rules/src/stratum.rs` morphotactics, shared by `preexpand::extend`/`emit::struct_extend`
//!   to prune their composite-chain recursion to engine-legal rule adjacencies (the Aweti scale
//!   fix — see that module's doc for the full design).
//! - [`preexpand`] (P1d, crate-internal): rule-application pre-expansion (interdigitation —
//!   `Role::Infix` rules applied to each root via the engine's own `pg_rules::morph::synthesize`)
//!   and boundary-fusion composite probing (Ge'ez glyph coalescence), emitted as multi-tag
//!   composite entries in the engine's own morph order and wired into `emit`'s shared `Composites`
//!   lexicon — see `tests/f3_amharic_gate.rs` (Amharic recall 100%, asserted).
//! - [`analyzer`]: `FomaProposer`, the thin `emit + compile + apply-up` wrapper.
//! - [`confirm`] (P2): a fresh port of `hc-hybrid/src/replay.rs`'s confirm half — `MorphemeOwner`,
//!   `build_morpheme_owners`, and `confirm_all` (D4's multiplicity recovery: every matching analysis
//!   in the pinned `parse_word_selected` outcome, not just the first).
//! - [`peel`] (P2, D6): a fresh port of `hc-hybrid/src/proposers.rs::ReduplicationProposer`, its
//!   recursion target swapped to the foma proposer (`ReduplicationPeeler::peel_candidates`).
//! - [`composite`] (P2): `FomaAnalyzer`, the public propose→confirm product API — `analyze_word`
//!   mirrors `pg_parse::ParseOutcome`'s `analyses`/`structured` shape, plus diagnostics.
//! - [`precision`] (P6 step 1, `docs/superpowers/specs/2026-07-15-fst-precision-knob-design.md`):
//!   the FST precision knob's `ConstraintCatalog`/`PrecisionAction`/`PrecisionConfig` for the
//!   GATE-CONSTRAINT ENVIRONMENT family, plus the `AllFlags` preset's flag-emission runtime
//!   (`crate::emit::emit_with_precision` is the opt-in entry point; `crate::emit::emit` always
//!   passes `PrecisionConfig::Strip`, byte-identical to before this step).
//!
//! `tests/f0_viability.rs` remains the P0 gate's record (proves the pure-Rust `foma` crate,
//! crates.io v0.1.1, github.com/divvun/foma-rs, compiles and behaves correctly on Windows and
//! wasm32); `tests/f1_sena_gate.rs` is the P1 stage-1 gate (emit+compile Sena, recall vs. the full
//! engine, `mbali`, overgeneration sanity); `tests/f2_indonesian_gate.rs` is the P1 stage-2 gate
//! (emit+compile Indonesian, recall minus the reduplication exclusion list, junction spot-checks,
//! overgeneration sanity, plus a Sena regression re-run); `tests/f3_amharic_gate.rs` is the P1d
//! gate (Amharic: emit+compile with the infix items gone from `uncovered`, recall asserted 100%,
//! end-to-end multiset parity vs the full engine, overgeneration sanity);
//! `tests/f4_composite_gate.rs` is the P2 gate (over-generation pruning, `mbali` multiplicity,
//! Indonesian redup round-trip, empty-on-miss, and a mini-parity smoke pass).
//!
//! ## `pg-parse` is now a NORMAL dependency (P2)
//! Through P1, this crate's lib target never depended on `pg-parse` (the verifier engine) — only
//! `pg-grammar`/`pg-featstruct` and (stage 2) `pg-rules`/`pg-shape` for [`junctions`]'s probe
//! machinery. P2's whole point is *confirm* — pinning `pg_parse::Morpher::parse_word_selected` to a
//! candidate's root(s)/rules (plan §2) — so [`confirm`] and [`composite`] necessarily depend on
//! `pg_parse::{Morpher, ParseOptions, WordAnalysis}` for real, not just as a dev-dependency oracle.
//! This is not a new wasm32 risk: `pg-wasm` already links `pg-parse` directly (it IS the verifier
//! engine `pg-wasm`'s own demo runs today), so this crate depending on it too adds no new transitive
//! dependency to that build — see `tests/f2_indonesian_gate.rs`'s wasm32 `cargo check`, still green
//! with `pg-parse` promoted, and this crate's own wasm32 check in CI/`README`.
#![forbid(unsafe_code)]

pub mod analyzer;
pub mod composite;
pub mod confirm;
/// Phase B composition-path budget guards (`docs/fst-plan/phase-b-compose-budget-design.md`):
/// [`morphotactics::EnumerationBudget`]'s sibling for the P6 composition path ([`replace`],
/// [`gate`], [`uflexc`]) -- size/count caps plus an opt-in wall-clock deadline for every
/// compose/union/minimize call on that path. See that module's own doc for the full design.
pub mod compose_budget;
/// E2 feasibility probe (not mainline; see that module's doc): does token-space Infix-rule
/// splicing (Amharic root-and-pattern interdigitation) reach 100% recall composed with
/// [`replace`]'s rule cascade? Standalone, additive, same status as [`replace`]/[`uflexc`].
pub mod e2_infix_probe;
pub mod emit;
/// P6 feasibility prototype sibling of [`replace`]/[`uflexc`]: static MPR/POS subrule gating (the
/// `docs/fst-plan/p6-prototype-report.md` §6 item 4 gap). See that module's doc for the design and
/// why it is a flag-free static partition rather than a flag-diacritics encoding.
pub mod gate;
pub mod junctions;
pub(crate) mod morphotactics;
pub mod peel;
pub(crate) mod preexpand;
pub mod precision;
/// P6 feasibility prototype (docs/fst-plan/p6-prototype-report.md): replace-rule compilation +
/// underlying-form lexc, NOT wired into the mainline `emit`/`analyzer` path. See that module's doc.
pub mod replace;
pub mod tags;
/// P6 feasibility prototype sibling of [`replace`]: the underlying-form lexc emitter.
pub mod uflexc;

/// Re-exported so downstream crates (and the P0 tests) have a single, versioned door into the
/// `foma` runtime rather than depending on it directly.
pub use foma as foma_runtime;
