//! Stage 1 (`docs/fst-plan/phase-c-generator-design.md`): synthetic-grammar generator for the
//! Phase C per-construct gates. Emits HermitCrab XML the production loader (`pg_grammar::load`)
//! accepts -- NOT snapshot JSON (design doc §1: JSON silently drops/collapses half the checklist
//! constructs this generator exists to exercise: always one char table, circumfix/metathesis
//! entries dropped, strata hardcoded to exactly 3).
//!
//! Determinism contract: [`render::render`] is a pure function of `(recipe.name, recipe.seed,
//! recipe.scale, recipe.construct)` -- the same recipe rendered twice must produce byte-identical
//! XML (`tests/self_check.rs` pins this for every stage-1 builder). Recipes are Rust literals
//! checked into the gate files that use them, never generated blobs on disk.
//!
//! ## Module map (design doc §2)
//! - [`rng`]: in-house SplitMix64, seeded from `hash(name, seed)` -- no `rand` dependency for a
//!   dev-only tool.
//! - [`recipe`]: [`recipe::Recipe`]/[`recipe::ScaleKnobs`]/[`recipe::ConstructKnobs`], the knobs
//!   every builder reads (or, for stage 2's reserved fields, will read).
//! - [`ids`]: deterministic per-prefix XML id minting, shared across every builder so two
//!   builders never collide on an id within one render.
//! - [`render`]: assembles the full `<HermitCrabInput>` document from a [`recipe::Recipe`].
//! - [`oracle`] (feature `oracle`, needs the optional `pg-parse` dependency): bounded
//!   Morpher-as-generator sweep -- ground truth for the recall-parity gates (GATE 2).
//! - [`build`]: one submodule per synthetic-stress-grammar-plan.md §2 construct row. Stage 1
//!   implements `tables`/`circumfix`/`template`; everything else is a compile-clean stub (see
//!   `build`'s own doc).

pub mod build;
pub mod ids;
pub mod recipe;
pub mod render;
pub mod rng;

#[cfg(feature = "oracle")]
pub mod oracle;

pub use recipe::{ConstructKnobs, Recipe, ScaleKnobs};
pub use render::{
    render, render_indexed, AlphaIndex, CompoundingIndex, ExtraStratumIndex, GatingIndex,
    MetathesisIndex, QuantifierIndex, RenderedGrammar, RightToLeftIndex, RootIndex,
    SimultaneousIndex, TableIndex,
};
