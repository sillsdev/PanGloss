//! One module per construct row this crate synthesizes a stress grammar for. The minimal
//! root/segment scaffolding every grammar requires regardless of construct lives in
//! [`crate::render`] itself, not a separate module here (it's generic glue, not a
//! construct-specific builder). [`tables`]/[`circumfix`]/[`template`] serve GATE 1 (multi-table)
//! and GATE 2 (circumfix); [`gating`] (partition-k), [`alpha`] (alpha-variable scale),
//! [`strata`] (stratum-depth scale), [`compounding`] (compounding-rule scale), and
//! [`quantifier`]/[`metathesis`]/[`simultaneous`]/[`right_to_left`] (the four HONEST-SKIP bail
//! gates) serve later construct-specific gates -- see each module's own doc for what it builds.
//! [`chain`] is a deep STANDALONE (non-template) affix chain, reproducing a known deep-truncation
//! root cause synthetically.

pub mod circumfix;
pub mod tables;
pub mod template;

pub mod alpha;
pub mod chain;
pub mod compounding;
pub mod gating;
pub mod metathesis;
pub mod quantifier;
pub mod right_to_left;
pub mod simultaneous;
pub mod strata;
