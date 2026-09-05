//! `pg-fwdata` imports FieldWorks `.fwdata` project files and `.fwbackup` archives into
//! `pg_snapshot::Snapshot` values. Backup imports extract the embedded `.fwdata` and use
//! `WritingSystemStore/*.ldml` data for vernacular exemplar characters.
//!
//! # Two layers, one crate
//!
//! 1. **Parser** (`xml`/`node`): a streaming `quick_xml` reader (`.fwdata` is a flat sequence
//!    of `<rt class="..." guid="..." [ownerguid="..."]>` records; Sena 3 is ~54MB, so this never
//!    builds a DOM of the whole document — only records whose class the extractor needs are
//!    parsed into a small per-record `node::Node` tree at all; everything else is skipped
//!    without being parsed).
//! 2. **Extractor** (`extract`): object graph → `pg_snapshot::Snapshot`, one function per
//!    snapshot section.
//!
//! # Robustness
//!
//! This crate tolerates stale or dangling data in otherwise-valid projects: dangling `objsur`
//! targets, unrecognized morph-type GUIDs, and missing expected fields become warnings in the
//! returned `ImportReport`. Hard errors cover I/O failures, invalid XML, non-`.fwdata` input,
//! and malformed parser-source metadata that cannot be represented safely.
//! Backup archive structure and member-access failures are reported separately as `Backup`.
#![forbid(unsafe_code)]

mod extract;
mod fwbackup;
mod morphtype;
mod node;
mod parser_params;
mod xml;

use std::path::Path;

use pg_snapshot::{ConversionProvenance, Snapshot, Warning};
use thiserror::Error;

/// Hard errors from `import_file`: I/O failures, invalid XML or non-`.fwdata` input, and source
/// metadata whose parser selector is malformed or unsupported. Data quality issues within an
/// otherwise-valid project become `ImportReport` warnings. `Backup` covers malformed ZIP
/// structure, member lookup, and LDML/member access failures; parsing the embedded `.fwdata` can
/// instead return `Xml`, `NotFwdata`, or `InvalidSource`.
#[derive(Debug, Error)]
pub enum ImportError {
    #[error("failed to read {0}")]
    Io(#[source] std::io::Error),
    #[error("malformed XML: {0}")]
    Xml(String),
    #[error("not a .fwdata file: no <rt> records found")]
    NotFwdata,
    #[error("not a .fwbackup: {0}")]
    Backup(String),
    #[error("{code}: {message}")]
    InvalidSource { code: &'static str, message: String },
}

/// Everything worth telling a caller about how the import went, beyond the `Snapshot` itself.
/// Never a reason to fail the import (see the crate-level docs). Each warning carries a stable
/// short code alongside its prose — see `pg_snapshot::Warning`'s doc for the `code`/`message`
/// contract `pangloss compare` relies on. `provenance` is the same value as the returned
/// `Snapshot`'s own `conversion_provenance`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub warnings: Vec<Warning>,
    pub provenance: ConversionProvenance,
}

/// Import a `.fwdata` project file or `.fwbackup` archive into a `Snapshot` plus an `ImportReport`
/// of anything tolerated along the way. Backup archives provide the embedded `.fwdata` and may
/// provide vernacular exemplar characters through `WritingSystemStore/*.ldml`. For a direct
/// `.fwdata` import, the input file stem becomes `project.name`; for a `.fwbackup` import, the
/// embedded top-level `.fwdata` entry stem becomes `project.name`.
pub fn import_file(path: &Path) -> Result<(Snapshot, ImportReport), ImportError> {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("fwbackup"))
    {
        return fwbackup::import_fwbackup(path);
    }
    let graph = xml::parse_fwdata(path)?;
    let filename_stem = file_stem(path);
    let (snapshot, warnings) = extract::extract(&graph, &filename_stem)?;
    let provenance = snapshot.conversion_provenance.clone();
    Ok((snapshot, ImportReport { warnings, provenance }))
}

pub(crate) fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}
