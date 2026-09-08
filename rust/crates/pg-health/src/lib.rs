//! The compiler's report vocabulary, as plain data: health findings and severities, backend
//! compatibility-report shapes, and required-runtime-feature constants.
//!
//! `pg-foma` (the FST compiler), `pg-pack` (the `.pgpack` container format), and the Runtime
//! (`pg-wasm`) all read these same wire shapes. Before this crate existed they shared them by
//! everyone depending on `pg-foma` directly, which pulled the whole compiler — and `foma` itself
//! — into the Runtime's wasm32 build graph; excluding the compiler there relied on dead-code
//! elimination that nothing verified. This crate carries only the shapes those three sides need
//! in common, so the Runtime can read a compile's report without compiling anything.
//!
//! Every type here is data plus accessors and Serde impls. Nothing here builds a `foma::types::Fsm`
//! or drives any part of a compile — a constructor that needs the compiler's own capability
//! envelope or advice catalog stays in `pg-foma` as a free function over these types (see
//! `pg_foma::backend_selection`'s doc for the specific split).
//!
//! Only serde/serde_json are dependencies, so this crate builds for wasm32-unknown-unknown with
//! no `foma`/construction machinery anywhere in its graph — pinned by
//! `pg-wasm/tests/wasm_excludes_compiler.rs`'s `cargo metadata` walk.

pub mod advice;
pub mod backend_selection;
pub mod capability;
pub mod health;
pub mod runtime_features;
pub mod strategy;
