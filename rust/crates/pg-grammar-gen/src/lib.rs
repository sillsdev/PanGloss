//! Synthetic-grammar generator for per-construct gates. Emits HermitCrab XML the production
//! loader (`pg_grammar::load`) accepts -- NOT snapshot JSON (JSON silently drops/collapses half
//! the checklist constructs this generator exists to exercise: always one char table,
//! circumfix/metathesis entries dropped, strata hardcoded to exactly 3).
//!
//! Determinism contract: [`render::render`] is a pure function of `(recipe.name, recipe.seed,
//! recipe.scale, recipe.construct)` -- the same recipe rendered twice must produce byte-identical
//! XML (`tests/self_check.rs` pins this for every builder). Recipes are Rust literals
//! checked into the gate files that use them, never generated blobs on disk.
//!
//! ## Module map
//! - [`rng`]: in-house SplitMix64, seeded from `hash(name, seed)` -- no `rand` dependency for a
//!   dev-only tool.
//! - [`recipe`]: [`recipe::Recipe`]/[`recipe::ScaleKnobs`]/[`recipe::ConstructKnobs`], the knobs
//!   every builder reads.
//! - [`ids`]: deterministic per-prefix XML id minting, shared across every builder so two
//!   builders never collide on an id within one render.
//! - [`mod@render`]: assembles the full `<HermitCrabInput>` document from a [`recipe::Recipe`].
//! - `oracle` (feature `oracle`, needs the optional `pg-parse` dependency): bounded
//!   Morpher-as-generator sweep -- ground truth for the recall-parity gates (GATE 2).
//! - [`build`]: one submodule per construct row this crate synthesizes a stress grammar for --
//!   see `build`'s own doc for what each implements.

pub mod build;
pub mod ids;
pub mod recipe;
pub mod render;
pub mod rng;

#[cfg(feature = "oracle")]
pub mod oracle;

pub use recipe::{ConstructKnobs, Recipe, ScaleKnobs};
pub use render::{
    render, render_indexed, AlphaIndex, ChainIndex, CompoundingIndex, ExtraStratumIndex,
    GatingIndex, MetathesisIndex, QuantifierIndex, RenderedGrammar, RightToLeftIndex, RootIndex,
    SimultaneousIndex, TableIndex,
};
