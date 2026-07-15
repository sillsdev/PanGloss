//! Phase P0 viability spike (docs/fst-plan/foma-fst-plan.md §P0, gate F0) plus P1 stage 1
//! (emitter core + Sena, gate F1) and P1 stage 2 (junction-aware phonology + Indonesian, gate F1).
//!
//! - [`tags`]: the `<R:nnnn>`/`<M:nnnn>` tag codec (D2) — escaped lexc spellings, decoded literal
//!   text, an `apply_up`-output decoder, and the `Candidate` split for compound (multi-root) paths.
//! - [`emit`]: `Grammar -> lexc source` (D3) — see that module's doc for the full design.
//! - [`junctions`]: [`junctions::PhonologyProbe`], the pre-probed surface-variant/deletion-junction
//!   machinery `emit` drives for a grammar with real phonological rules (stage 2) — a `None`-safe
//!   no-op for a grammar without any (stage 1's Sena stays byte-identical).
//! - [`analyzer`]: `FomaProposer`, the thin `emit + compile + apply-up` wrapper.
//!
//! `tests/f0_viability.rs` remains the P0 gate's record (proves the pure-Rust `foma` crate,
//! crates.io v0.1.1, github.com/divvun/foma-rs, compiles and behaves correctly on Windows and
//! wasm32); `tests/f1_sena_gate.rs` is the P1 stage-1 gate (emit+compile Sena, recall vs. the full
//! engine, `mbali`, overgeneration sanity); `tests/f2_indonesian_gate.rs` is the P1 stage-2 gate
//! (emit+compile Indonesian, recall minus the reduplication exclusion list, junction spot-checks,
//! overgeneration sanity, plus a Sena regression re-run).
//!
//! This crate's lib target intentionally never depends on `hc-parse` (the verifier engine) — only
//! `hc-grammar`/`hc-featstruct` and, as of stage 2, `hc-rules`/`hc-shape` for [`junctions`]'s probe
//! machinery (both already load-bearing in `hc-wasm`'s own dependency graph via `hc-parse`, so this
//! adds no new wasm32 risk — see `tests/f2_indonesian_gate.rs`'s wasm32 `cargo check`).
//! `hc-parse` itself stays a dev-dependency ONLY, needed by the recall gates as an oracle, so
//! `hc-wasm` linking this crate directly does not pull in the whole engine.
#![forbid(unsafe_code)]

pub mod analyzer;
pub mod emit;
pub mod junctions;
pub mod tags;

/// Re-exported so downstream crates (and the P0 tests) have a single, versioned door into the
/// `foma` runtime rather than depending on it directly.
pub use foma as foma_runtime;
