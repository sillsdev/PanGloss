//! Error type for the statistics cache.

use std::path::PathBuf;

use thiserror::Error;

/// Everything that can go wrong opening, writing, or reading the stats cache.
#[derive(Debug, Error)]
pub enum StatsError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error(
        "counter `{counter}` value {value} exceeds i64::MAX and cannot be stored in SQLite's \
         signed INTEGER column"
    )]
    CounterOverflow { counter: &'static str, value: u64 },

    #[error("stats cache already contains engine `{existing}`, cannot append engine `{requested}")]
    EngineMismatch { existing: String, requested: String },

    #[error(
        "could not determine a user-data directory for the stats cache (checked LOCALAPPDATA / \
         XDG_DATA_HOME / HOME)"
    )]
    NoUserDataDir,

    #[error("could not canonicalize fwdata path {path}: {source}")]
    CanonicalizeFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
