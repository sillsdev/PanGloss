//! `import_file_measured` must change no behaviour versus `import_file`, for both input kinds.

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
    let opts = zip::write::SimpleFileOptions::default();
    z.start_file("Proj 1.fwdata", opts).unwrap();
    z.write_all(&fwdata).unwrap();
    z.start_file("WritingSystemStore/fx.ldml", opts).unwrap();
    z.write_all(br#"<ldml><identity><language type="fx"/></identity><characters><exemplarCharacters>[a-c{ch}]</exemplarCharacters></characters></ldml>"#).unwrap();
    z.finish().unwrap();
    out
}

fn assert_parity(path: &Path) {
    let (snapshot_plain, report_plain) = pg_fwdata::import_file(path).unwrap();
    let (snapshot_measured, report_measured, _delta) =
        pg_fwdata::import_file_measured(path).unwrap();

    assert_eq!(
        snapshot_plain,
        snapshot_measured,
        "{}: import_file_measured must produce the same Snapshot",
        path.display()
    );
    assert_eq!(
        report_plain.warnings.len(),
        report_measured.warnings.len(),
        "{}: warning count must match",
        path.display()
    );
    for (a, b) in report_plain.warnings.iter().zip(report_measured.warnings.iter()) {
        assert_eq!(
            a, b,
            "{}: warnings must be byte-identical element by element",
            path.display()
        );
    }
    assert_eq!(
        report_plain.provenance, report_measured.provenance,
        "{}: provenance must match",
        path.display()
    );
}

#[test]
fn import_file_measured_changes_no_behaviour_for_fwdata() {
    assert_parity(&fixture_path());
}

#[test]
fn import_file_measured_changes_no_behaviour_for_fwbackup() {
    let dir = tempfile::tempdir().unwrap();
    let backup = write_backup(dir.path());
    assert_parity(&backup);
}
