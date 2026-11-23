//! Fuzz harness for the cascade backend: a seeded synthetic-grammar generator and the gate that
//! compares a candidate analyzer against plain HermitCrab word by word.
//!
//! # Replay by seed
//!
//! Every grammar is a pure function of its seed (`gen::Plan::for_seed(seed)` then
//! `gen::generate`), so a failure line that names a seed reproduces exactly:
//!
//! ```text
//! CASCADE_FUZZ_REPLAY=<seed> cargo test -p pg-foma-backend --test cascade_fuzz_gate -- --nocapture
//! ```
//!
//! prints the grammar's XML, the switches it realises, and each word's comparison. A range of
//! seeds is `CASCADE_FUZZ_START=<first> CASCADE_FUZZ_SEEDS=<count>`. The default run is 250
//! seeds, enough for the per-switch coverage minimums; shorter runs skip those minimums and say
//! so.
//!
//! Words are drawn from the grammar's own generation (root and rule subsets through
//! `Morpher::generate_words`) plus random strings and one-edit variants, using a generator seeded
//! from the grammar seed, so the word list replays too.

// Some switches and candidate hooks are read only by candidates that implement them.
#![allow(dead_code, clippy::enum_variant_names)]

pub mod alphabet;
pub mod features;
pub mod gate;
pub mod gen;
pub mod hc;
pub mod rng;
