//! `import_file_lean` must equal `import_file` except for the import-stage inventory and decisions it omits.

use std::io::Write;
use std::path::{Path, PathBuf};

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fixture.fwdata")
}

fn write_backup(dir: &Path) -> PathBuf {
    let fwdata = std::fs::read(fixture_path()).unwrap();
    let out = dir.join("Proj 1.fwbackup");
    let file = std::fs::File::create(&out).unwrap();
    let mut z = zip::ZipWriter::new(file);
    z.start_file("Proj 1.fwdata", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(&fwdata).unwrap();
    z.finish().unwrap();
    out
}

fn assert_lean_matches_full(path: &Path) {
    let (mut full, full_report) = pg_fwdata::import_file(path).unwrap();
    let (lean, lean_report) = pg_fwdata::import_file_lean(path).unwrap();

    assert!(
        !full.conversion_provenance.import_load_decisions.is_empty(),
        "the full import must have decisions to omit"
    );
    assert!(lean.conversion_provenance.import_load_decisions.is_empty());
    assert_eq!(
        lean.conversion_provenance.graph_to_snapshot,
        Default::default()
    );
    full.conversion_provenance.import_load_decisions = Vec::new();
    full.conversion_provenance.graph_to_snapshot = Default::default();
    assert_eq!(lean, full, "{}", path.display());
    assert_eq!(lean_report, full_report, "{}", path.display());
}

#[test]
fn lean_import_matches_full_import_for_fwdata() {
    assert_lean_matches_full(&fixture_path());
}

#[test]
fn lean_import_matches_full_import_for_fwbackup() {
    let dir = tempfile::tempdir().unwrap();
    assert_lean_matches_full(&write_backup(dir.path()));
}
