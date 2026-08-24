//! Per-project statistics cache: attribution and aggregation for `pangloss batch --stats`.
//!
//! A gated collector (not part of this crate) records, per analyzed word, seven counters for
//! every `(object, stratum, allomorph, direction)` combination that participated in the search.
//! This crate
//! is everything downstream of that: where the cache file lives, how it is opened, wiped, and
//! accumulated into, how facts are written, and the reports that read them back.
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
    CoverageState, Direction, FactRecord, IdentityQuality, ObjectKind, RunMetadata,
    StructuralLocator, UnknownVariant, WordRecord,
};
pub use path::{default_cache_dir, default_cache_path};
pub use report::{
    coverage_rows, kind_has_any_recorded_object, mixed_settings, never_fires_report,
    per_allomorph_report, per_direction_report, per_kind_report, per_morpheme_report,
    per_object_report, per_stratum_report, per_word_report, word_elapsed_ns_total, CoverageRow,
    MixedSettings, NeverFiresFilter, NeverFiresRow, PerAllomorphFilter, PerAllomorphRow,
    PerDirectionFilter, PerDirectionRow, PerKindFilter, PerKindRow, PerMorphemeFilter,
    PerMorphemeRow, PerObjectFilter, PerObjectRow, PerStratumFilter, PerStratumRow, PerWordRow,
    SortKey, NEVER_FIRES_DEFAULT_MIN_ATTEMPTS,
};
pub use schema::{COUNTER_SEMANTICS_VERSION, SCHEMA_VERSION};
