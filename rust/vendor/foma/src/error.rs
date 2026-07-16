//! Library-wide error type for the foma port (Wave 4 idiomatization).
//!
//! The Wave-2 library reproduced the C's fatal paths literally: `exit(1)`
//! on stack overflow, panics on malformed input, dead prototypes that were
//! link errors in C. Per docs/port/rust-conventions.md (Wave 4), library
//! code returns a `Result` instead so binaries can translate failures into
//! exit codes and messages. The enum is hand-rolled (no `thiserror`
//! dependency): `Display`, `Error` and `From` impls are written out below.

use std::fmt;

/// Errors surfaced by the foma library instead of `exit()`/panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FomaError {
    /// A C prototype that was never defined (a link error in C), or an
    /// algorithm the port has not implemented yet. Carries a static label
    /// naming the entry point.
    Unimplemented(&'static str),
    /// Input bytes were malformed for the operation — e.g. an invalid
    /// UTF-8 lead byte where a well-formed sequence was expected.
    MalformedInput(String),
    /// A bounded resource was exhausted (e.g. a fixed-capacity stack in the
    /// C sources that called `exit(1)` on overflow).
    CapacityExceeded(&'static str),
    /// A filesystem operation failed (open/read/write) where the C printed a
    /// diagnostic and returned a NULL/sentinel. Carries a human-readable detail.
    Io(String),
    /// A serialized network or input file was structurally malformed for the
    /// operation (bad header/field, or a rejected byte-order mark).
    Format(String),
}

impl fmt::Display for FomaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FomaError::Unimplemented(what) => write!(f, "unimplemented: {what}"),
            FomaError::MalformedInput(msg) => write!(f, "malformed input: {msg}"),
            FomaError::CapacityExceeded(what) => write!(f, "capacity exceeded: {what}"),
            FomaError::Io(msg) => write!(f, "io error: {msg}"),
            FomaError::Format(msg) => write!(f, "format error: {msg}"),
        }
    }
}

impl std::error::Error for FomaError {}

impl From<std::io::Error> for FomaError {
    /// An `io::Error` from a write/read at a boundary that flows `FomaError`
    /// becomes an `Io` variant carrying its message (dropping the non-`Eq`
    /// source so `FomaError` stays `PartialEq`/`Eq`).
    fn from(e: std::io::Error) -> Self {
        FomaError::Io(e.to_string())
    }
}
