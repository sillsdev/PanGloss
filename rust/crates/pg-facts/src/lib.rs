#![forbid(unsafe_code)]

mod adhoc;
mod compiled;
mod context;
mod features;
mod guid;
mod lexicon;
mod load;
mod metadata;
mod morphology;
mod phonology;
mod settings;
mod stats;
mod variants;
mod write;

use std::path::Path;

use serde::Serialize;
use thiserror::Error;

pub use metadata::{
    ProducerIdentity, SectionStatus, APPLICATION_ID, FACTS_CONTEXT_FORMAT, FACTS_CONTEXT_VERSION,
    FACT_FORMAT, FACT_SCHEMA_VERSION, STATS_MANIFEST_FORMAT_VERSION,
};
pub use stats::StatsInput;
pub use write::write_facts;
pub use write::write_facts_with_stats;

/// The result of publishing a complete facts database.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FactsResult {
    pub application_id: i64,
    pub schema_version: u32,
    pub format: String,
    pub source_sha256: String,
    pub grammar_hash: String,
    pub model_fingerprint: String,
    pub baseline_key: String,
    pub input_kind: String,
    pub dry_run_digest: Option<String>,
    pub compile_status: String,
    pub sections: Vec<SectionStatus>,
    pub output_bytes: u64,
    pub output_sha256: String,
}

/// Failures that prevent a facts database from being published.
#[derive(Debug, Error)]
pub enum FactsError {
    #[error("invalid context: {0}")]
    InvalidContext(String),
    #[error("unsupported context version {0}")]
    UnsupportedContextVersion(u64),
    #[error("invalid Snapshot: {0}")]
    InvalidSnapshot(String),
    #[error("unsupported Snapshot version {0}")]
    UnsupportedSnapshotVersion(u32),
    #[error("unsupported facts source {0:?}")]
    UnsupportedFactsSource(String),
    #[error("expected model fingerprint {expected}, found {actual}")]
    ModelMismatch { expected: String, actual: String },
    #[error("compile failed before a structured conversion result: {0}")]
    Compile(String),
    #[error("output already exists")]
    OutputExists,
    #[error("output path resolves to an input file")]
    InputOutputCollision,
    #[error("stats manifest does not match the supplied Snapshot: {0}")]
    StatsContextMismatch(String),
    #[error("frozen stats run is unavailable: {0}")]
    StatsRunUnavailable(String),
    #[error("SQLite integrity verification failed: {0}")]
    Integrity(String),
    #[error("serialization failed: {0}")]
    Serialization(String),
    #[error("I/O failed: {0}")]
    Io(String),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

impl FactsError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidContext(_) => "invalid_context",
            Self::UnsupportedContextVersion(_) => "unsupported_context_version",
            Self::InvalidSnapshot(_) => "invalid_snapshot",
            Self::UnsupportedSnapshotVersion(_) => "unsupported_snapshot_version",
            Self::UnsupportedFactsSource(_) => "unsupported_facts_source",
            Self::ModelMismatch { .. } => "model_mismatch",
            Self::Compile(_) => "compile_failed",
            Self::OutputExists => "output_exists",
            Self::InputOutputCollision => "input_output_collision",
            Self::StatsContextMismatch(_) => "stats_context_mismatch",
            Self::StatsRunUnavailable(_) => "stats_run_unavailable",
            Self::Integrity(_) | Self::Serialization(_) | Self::Io(_) | Self::Sqlite(_) => {
                "facts_write_failed"
            }
        }
    }
}

/// Builds a facts file from exact Snapshot bytes and a closed identity context.
pub fn write_facts_from_paths(
    source_path: impl AsRef<Path>,
    context_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    producer: ProducerIdentity<'_>,
) -> Result<FactsResult, FactsError> {
    let source = std::fs::read(source_path).map_err(|error| FactsError::Io(error.to_string()))?;
    let context = std::fs::read(context_path).map_err(|error| FactsError::Io(error.to_string()))?;
    write_facts(&source, &context, output_path, producer)
}
