//! One module per synthetic-stress-grammar-plan.md §2 construct row (design doc §2's crate
//! layout). Stage 1 implemented [`tables`], [`circumfix`], and [`template`] -- what GATE 1
//! (multi-table) and GATE 2 (circumfix) need. The minimal root/segment scaffolding every grammar
//! requires regardless of construct lives in [`crate::render`] itself, not a separate module here
//! (it's generic glue, not a construct-specific builder). Stage 2 (design doc §6's priority order)
//! fills in every remaining module: [`gating`] (partition-k), [`alpha`] (alpha-variable scale),
//! [`strata`] (stratum-depth scale), [`compounding`] (compounding-rule scale), [`quantifier`]/
//! [`metathesis`]/[`simultaneous`]/[`right_to_left`] (the four HONEST-SKIP bail gates).

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
