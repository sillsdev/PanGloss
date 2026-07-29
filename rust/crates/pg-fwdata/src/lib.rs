//! `pg-fwdata`: reads a FieldWorks `.fwdata` project file directly into a
//! [`pg_snapshot::Snapshot`] — layer 1 of `docs/fwdata-import-plan.md`'s three-layer pipeline
//! (`.fwdata → pg-fwdata → Snapshot → pg_grammar::compile → Grammar`). See that plan's §2 for the
//! overall architecture and §6 (task T2) for this crate's scope.
//!
//! # Two layers, one crate
//!
//! 1. **Parser** ([`xml`]/[`node`]): a streaming `quick_xml` reader (`.fwdata` is a flat sequence
//!    of `<rt class="..." guid="..." [ownerguid="..."]>` records; Sena 3 is ~54MB, so this never
//!    builds a DOM of the whole document — only records whose class the extractor needs are
//!    parsed into a small per-record [`node::Node`] tree at all; everything else is skipped
//!    without being parsed).
//! 2. **Extractor** ([`extract`]): object graph → [`pg_snapshot::Snapshot`], one function per
//!    snapshot section.
//!
//! # Robustness
//!
//! Per `docs/fwdata-import-plan.md` §1, this crate must tolerate the kind of stale/dangling data
//! real FieldWorks projects contain (the motivating example: a stale `MoMorphAdhocProhib` that
//! crashes FieldWorks' own HC exporter) — dangling `objsur` targets, unrecognized morph-type
//! GUIDs, and missing expected fields are reported as warnings in the returned [`ImportReport`],
//! never a panic or a hard [`ImportError`]. Hard errors are reserved for I/O failures and input
//! that isn't XML / isn't a `.fwdata` document at all.
#![forbid(unsafe_code)]

mod extract;
mod morphtype;
mod node;
mod parser_params;
mod xml;

use std::path::Path;

use pg_snapshot::{Snapshot, Warning};
use thiserror::Error;

/// Hard errors from [`import_file`] — I/O and "this isn't a `.fwdata` file at all", never data
/// quality issues within an otherwise-valid `.fwdata` document (those become [`ImportReport`]
/// warnings; see the crate-level docs' "Robustness" section).
#[derive(Debug, Error)]
pub enum ImportError {
    #[error("failed to read {0}")]
    Io(#[source] std::io::Error),
    #[error("malformed XML: {0}")]
    Xml(String),
    #[error("not a .fwdata file: no <rt> records found")]
    NotFwdata,
}

/// Everything worth telling a caller about how the import went, beyond the `Snapshot` itself.
/// Never a reason to fail the import (see the crate-level docs). Each warning carries a stable
/// short code alongside its prose (`openspec/changes/add-grammar-assessment` task 3.8) — see
/// [`pg_snapshot::Warning`]'s doc for the `code`/`message` contract `pangloss compare` relies on.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub warnings: Vec<Warning>,
}

/// Import a `.fwdata` project file into a [`Snapshot`] plus an [`ImportReport`] of anything
/// tolerated along the way. `path`'s file stem (e.g. `"Sena 3"` for `Sena 3.fwdata`) becomes
/// `project.name`, matching how FieldWorks itself derives `LcmCache.ProjectId.Name` from the
/// project folder/file name rather than anything stored in the XML.
pub fn import_file(path: &Path) -> Result<(Snapshot, ImportReport), ImportError> {
    let graph = xml::parse_fwdata(path)?;
    let filename_stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (snapshot, warnings) = extract::extract(&graph, &filename_stem);
    Ok((snapshot, ImportReport { warnings }))
}
