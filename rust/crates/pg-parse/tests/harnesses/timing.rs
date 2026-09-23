// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../perf_cold_warm_probe.rs"]
mod perf_cold_warm_probe;
#[path = "../word_timeout_gate.rs"]
mod word_timeout_gate;
#[path = "../word_timeout_pathological_gate.rs"]
mod word_timeout_pathological_gate;
