//! Phase P0 viability spike (docs/fst-plan/foma-fst-plan.md §P0, gate F0).
//!
//! This crate will eventually host the emitter (`Grammar -> FomaSource`, D3), the tag codec
//! (D2), and the propose→confirm composite (P2) that replace `hc-hybrid` as the FST proposer
//! layer. For P0 the substance is entirely in `tests/f0_viability.rs`, which proves the
//! pure-Rust `foma` crate (crates.io v0.1.1, github.com/divvun/foma-rs) compiles and behaves
//! correctly on Windows and wasm32 before any of that is built.
//!
//! This placeholder module is deliberately thin — no emitter logic yet.
#![forbid(unsafe_code)]

/// Re-exported so downstream crates (and the P0 tests) have a single, versioned door into the
/// `foma` runtime rather than depending on it directly.
pub use foma as foma_runtime;
