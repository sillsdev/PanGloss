//! Shared test support for the Phase C per-construct gates (`docs/fst-plan/
//! phase-c-generator-design.md`). Standard Rust integration-test convention: `tests/common/mod.rs`
//! is not itself picked up as a separate test binary (only files directly under `tests/` are);
//! each real gate file does `mod common;` to pull this in (mirrors `pg-rules/tests/common/mod.rs`,
//! the existing precedent for this pattern in this workspace).

pub mod gate_template;
