//! `pg-fwdata` imports FieldWorks `.fwdata` project files and `.fwbackup` archives into
//! `pg_snapshot::Snapshot` values. Vernacular exemplar characters come from
//! `WritingSystemStore/*.ldml`: a backup's own embedded copy, or -- for a plain `.fwdata` --
//! a sibling `WritingSystemStore/` directory next to it on disk, if one exists.
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

use pg_snapshot::{ConversionProvenance, FwClass, FwObjectRef, InventoryDelta, Snapshot, Warning};
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
/// of anything tolerated along the way. Vernacular exemplar characters come from
/// `WritingSystemStore/*.ldml` -- a backup's embedded copy, or a plain `.fwdata`'s sibling
/// directory on disk, if either is present; absent either way, `exemplar_characters` stays empty.
/// For a direct `.fwdata` import, the input file stem becomes `project.name`; for a `.fwbackup`
/// import, the embedded top-level `.fwdata` entry stem becomes `project.name`.
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
    let (mut snapshot, mut warnings) = extract::extract(&graph, &filename_stem)?;
    let provenance = snapshot.conversion_provenance.clone();
    let (ldml, ldml_warning) = read_sibling_writing_system_store(path);
    fwbackup::apply_exemplars(&mut snapshot, &ldml);
    warnings.extend(ldml_warning);
    Ok((
        snapshot,
        ImportReport {
            warnings,
            provenance,
        },
    ))
}

/// As [`import_file`], but also returns the [`InventoryDelta`] derived from the same import's
/// [`pg_snapshot::SelectionRecorder`] -- a violated invariant is this crate's own bookkeeping bug,
/// so it panics naming the violation rather than returning an untrustworthy measurement.
pub fn import_file_measured(
    path: &Path,
) -> Result<(Snapshot, ImportReport, InventoryDelta), ImportError> {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("fwbackup"))
    {
        return fwbackup::import_fwbackup_measured(path);
    }
    let graph = xml::parse_fwdata(path)?;
    let filename_stem = file_stem(path);
    let (mut snapshot, mut warnings, recorder) =
        extract::extract_recording(&graph, &filename_stem)?;
    let provenance = snapshot.conversion_provenance.clone();
    let (ldml, ldml_warning) = read_sibling_writing_system_store(path);
    fwbackup::apply_exemplars(&mut snapshot, &ldml);
    warnings.extend(ldml_warning);
    if let Err(violation) = recorder.check_invariants() {
        panic!("import_file_measured: selection recorder invariant violated: {violation}");
    }
    let (inventory, issues) = recorder.finish();
    Ok((
        snapshot,
        ImportReport {
            warnings,
            provenance,
        },
        InventoryDelta::from_stage(inventory, issues),
    ))
}

/// Reads every `WritingSystemStore/*.ldml` file next to `fwdata_path`; a missing directory is silent (matches `.fwbackup`'s tolerant absence of embedded LDML), but an existing, unreadable one -- a different fact from "never shipped" -- returns a warning rather than looking identical to it.
fn read_sibling_writing_system_store(
    fwdata_path: &Path,
) -> (Vec<(String, String)>, Option<Warning>) {
    let Some(parent) = fwdata_path.parent() else {
        return (Vec::new(), None);
    };
    let project_name = file_stem(fwdata_path);
    let dir = parent.join("WritingSystemStore");
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Vec::new(), None),
        Err(_) => {
            return (
                Vec::new(),
                Some(writing_system_store_warning(&project_name)),
            )
        }
    };
    let mut pairs = Vec::new();
    let mut warning = None;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                warning.get_or_insert_with(|| writing_system_store_warning(&project_name));
                continue;
            }
        };
        let entry_path = entry.path();
        if entry_path.extension().and_then(|e| e.to_str()) != Some("ldml") {
            continue;
        }
        let Some(tag) = entry_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        match std::fs::read_to_string(&entry_path) {
            Ok(text) => pairs.push((tag, text)),
            Err(_) => {
                warning.get_or_insert_with(|| writing_system_store_warning(&project_name));
            }
        }
    }
    (pairs, warning)
}

fn writing_system_store_warning(project_name: &str) -> Warning {
    Warning::new(
        extract::codes::WRITING_SYSTEM_STORE_UNREADABLE,
        format!("Writing-system data for FieldWorks project '{project_name}' could not be read."),
    )
    .with_subject(FwObjectRef::new(FwClass::Project).name(project_name))
}

pub(crate) fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}
