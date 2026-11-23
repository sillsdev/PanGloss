//! Fuzz harness: seeded grammars compared word by word with plain HC; replay with `CASCADE_FUZZ_REPLAY=<seed>`.

// Some switches and candidate hooks are read only by candidates that implement them.
#![allow(dead_code, clippy::enum_variant_names)]

pub mod alphabet;
pub mod features;
pub mod gate;
pub mod gen;
pub mod hc;
pub mod rng;
