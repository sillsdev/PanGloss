//! Per-project statistics cache: attribution and aggregation for `pangloss batch --stats`.
//!
//! A gated collector (not part of this crate) records, per analyzed word, seven counters for
//! every `(object, stratum, allomorph)` combination that participated in the search. This crate
//! is everything downstream of that: where the cache file lives, how it is opened, wiped, and
//! accumulated into, how facts are written, and the two v1 reports that read them back.
//!
//! The schema (`schema.sql`, embedded via `include_str!`) is a documented public escape hatch —
//! `run.schema_version` is a compatibility promise once a caller queries the file directly, so
//! this crate never migrates it. A version or grammar-hash mismatch wipes and starts over; see
//! `cache::StatsCache::open`.

pub mod cache;
pub mod error;
pub mod model;
pub mod path;
pub mod report;
mod schema;
#[cfg(test)]
mod test_support;
mod util;

pub use cache::{OpenOutcome, StatsCache};
pub use error::StatsError;
pub use model::{
    CoverageState, FactRecord, IdentityQuality, ObjectKind, RunMetadata, StructuralLocator,
    UnknownVariant, WordRecord,
};
pub use path::{default_cache_dir, default_cache_path};
pub use report::{
    coverage_rows, mixed_settings, per_allomorph_report, per_object_report, per_stratum_report,
    per_word_report, CoverageRow, MixedSettings, PerAllomorphFilter, PerAllomorphRow,
    PerObjectFilter, PerObjectRow, PerStratumFilter, PerStratumRow, PerWordRow, SortKey,
};
pub use schema::{COUNTER_SEMANTICS_VERSION, SCHEMA_VERSION};
