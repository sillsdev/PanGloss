//! Phonological and morphological rules, affix templates, strata, and cascades (plan §5.5).
//!
//! Rules are enum dispatch (not trait objects) on the hot path; rule *data* lives in flat
//! per-grammar tables indexed by `RuleId(u32)`.
#![forbid(unsafe_code)]

pub mod bridge;
pub mod cache;
pub mod cascade;
pub mod metathesis;
pub mod morph;
pub mod rewrite;
pub mod shape_feat;
pub mod stats;
pub mod stats_calibrate;
pub mod stratum;
pub mod surface_probe;
pub mod trace;
pub mod validity;
pub mod word;

pub use word::{MorphRecord, Word, WordFlags, WordKey};

/// Flat index of a rule in the grammar's rule tables (plan §5.5).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct RuleId(pub u32);
