//! Verifies `.fwbackup` imports preserve embedded project data and vernacular exemplars.

use std::io::Write;

fn fixture_fwdata() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fixture.fwdata")
}

fn write_backup(dir: &std::path::Path) -> std::path::PathBuf {
    let out = dir.join("Proj 1.fwbackup");
    let file = std::fs::File::create(&out).unwrap();
    let mut z = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    z.start_file("Proj 1.fwdata", opts).unwrap();
    z.write_all(&std::fs::read(fixture_fwdata()).unwrap())
        .unwrap();
    z.start_file("WritingSystemStore/fx.ldml", opts).unwrap();
    z.write_all(br#"<ldml><identity><language type="fx"/></identity><characters><exemplarCharacters>[a-c{ch}]</exemplarCharacters></characters></ldml>"#).unwrap();
    z.start_file("BackupSettings/BackupSettings.xml", opts)
        .unwrap();
    z.write_all(b"<BackupSettings/>").unwrap();
    z.finish().unwrap();
    out
}

#[test]
fn fwbackup_imports_the_embedded_fwdata_and_exemplars() {
    let dir = tempfile::tempdir().unwrap();
    let backup = write_backup(dir.path());
    let (snapshot, _report) = pg_fwdata::import_file(&backup).unwrap();
    assert_eq!(snapshot.project.name, "Proj 1");
    // Same lexicon as importing the .fwdata directly.
    let (direct, _) = pg_fwdata::import_file(&fixture_fwdata()).unwrap();
    assert_eq!(snapshot.lexicon, direct.lexicon);
    // The fixture's default vernacular writing system is "fx", matching the LDML entry.
    assert_eq!(direct.project.vernacular_writing_systems.first().map(String::as_str), Some("fx"));
    assert_eq!(
        snapshot.project.exemplar_characters,
        vec!["a", "b", "c", "ch"]
    );
}

#[test]
fn fwbackup_without_fwdata_entry_is_a_hard_error() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("empty.fwbackup");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&out).unwrap());
    z.start_file(
        "BackupSettings/BackupSettings.xml",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    z.write_all(b"<BackupSettings/>").unwrap();
    z.finish().unwrap();
    let err = pg_fwdata::import_file(&out).unwrap_err();
    assert!(matches!(err, pg_fwdata::ImportError::Backup(_)));
}
