//! Fuzz harness for the cascade backend: seeded grammar generator plus the gate against plain HC.

// Some switches and candidate hooks are read only by candidates that implement them.
#![allow(dead_code, clippy::enum_variant_names)]

pub mod alphabet;
pub mod features;
pub mod gate;
pub mod gen;
pub mod hc;
pub mod rng;
