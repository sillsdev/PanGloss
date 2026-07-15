//! Phase P0 viability spike (docs/fst-plan/foma-fst-plan.md §P0, gate F0) plus P1 stage 1
//! (emitter core + Sena, gate F1).
//!
//! - [`tags`]: the `<R:nnnn>`/`<M:nnnn>` tag codec (D2) — escaped lexc spellings, decoded literal
//!   text, an `apply_up`-output decoder, and the `Candidate` split for compound (multi-root) paths.
//! - [`emit`]: `Grammar -> lexc source` (D3) — see that module's doc for the full design.
//! - [`analyzer`]: `FomaProposer`, the thin `emit + compile + apply-up` wrapper.
//!
//! `tests/f0_viability.rs` remains the P0 gate's record (proves the pure-Rust `foma` crate,
//! crates.io v0.1.1, github.com/divvun/foma-rs, compiles and behaves correctly on Windows and
//! wasm32); `tests/f1_sena_gate.rs` is the P1 stage-1 gate (emit+compile Sena, recall vs. the full
//! engine, `mbali`, overgeneration sanity).
//!
//! This crate's lib target intentionally never depends on `hc-parse` (the verifier engine) — only
//! `hc-grammar`/`hc-featstruct`. `hc-parse` is a dev-dependency ONLY, needed by the recall gate
//! test as an oracle, so `hc-wasm` linking this crate directly does not pull in the whole engine.
#![forbid(unsafe_code)]

pub mod analyzer;
pub mod emit;
pub mod tags;

/// Re-exported so downstream crates (and the P0 tests) have a single, versioned door into the
/// `foma` runtime rather than depending on it directly.
pub use foma as foma_runtime;
