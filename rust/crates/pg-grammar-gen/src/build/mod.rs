//! One module per synthetic-stress-grammar-plan.md §2 construct row (design doc §2's crate
//! layout). Stage 1 implements [`tables`], [`circumfix`], and [`template`] -- what GATE 1
//! (multi-table) and GATE 2 (circumfix) need. The minimal root/segment scaffolding every grammar
//! requires regardless of construct lives in [`crate::render`] itself, not a separate module here
//! (it's generic glue, not a construct-specific builder). Every other module below is a
//! compile-clean, empty stub: its own doc names the stage-2 gate (design doc §6's priority order)
//! that will fill it in.

pub mod circumfix;
pub mod tables;
pub mod template;

pub mod alpha;
pub mod compounding;
pub mod gating;
pub mod metathesis;
pub mod quantifier;
pub mod right_to_left;
pub mod simultaneous;
pub mod strata;
