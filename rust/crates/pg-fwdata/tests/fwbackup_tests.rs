//! Verifies `.fwbackup` imports preserve embedded project data and vernacular exemplars.

use std::io::Write;

fn fixture_fwdata() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fixture.fwdata")
}

fn write_backup(dir: &std::path::Path) -> std::path::PathBuf {
    let fwdata = std::fs::read(fixture_fwdata()).unwrap();
    write_backup_with_fwdata(dir, &fwdata)
}

fn write_backup_with_fwdata(
    dir: &std::path::Path,
    fwdata: &[u8],
) -> std::path::PathBuf {
    let out = dir.join("Proj 1.fwbackup");
    let file = std::fs::File::create(&out).unwrap();
    let mut z = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    z.start_file("Proj 1.fwdata", opts).unwrap();
    z.write_all(fwdata).unwrap();
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

#[test]
fn malformed_xample_cap_reaches_fwbackup_import_report() {
    let dir = tempfile::tempdir().unwrap();
    let source = std::fs::read_to_string(fixture_fwdata()).unwrap();
    let needle = "&lt;MaxPrefixes&gt;2&lt;/MaxPrefixes&gt;";
    assert!(source.contains(needle), "fixture text to replace must be present");
    let variant = source.replacen(
        needle,
        "&lt;MaxPrefixes&gt;many&lt;/MaxPrefixes&gt;",
        1,
    );
    let backup = write_backup_with_fwdata(dir.path(), variant.as_bytes());
    let (snapshot, report) = pg_fwdata::import_file(&backup).unwrap();
    assert_eq!(snapshot.morphology.parser_parameters.xample.max_prefixes, None);
    let warning = report
        .warnings
        .iter()
        .find(|warning| warning.code == "fwdata.invalid-parser-parameter")
        .expect("invalid cap must be reported from a backup import");
    assert!(warning.message.contains("MaxPrefixes"));
    assert_eq!(
        report
            .warnings
            .iter()
            .filter(|warning| warning.code == "fwdata.invalid-parser-parameter")
            .count(),
        1
    );
}

#[test]
fn fwbackup_report_provenance_matches_the_snapshot_and_the_direct_fwdata_import() {
    let dir = tempfile::tempdir().unwrap();
    let backup = write_backup(dir.path());
    let (snapshot, report) = pg_fwdata::import_file(&backup).unwrap();
    assert_eq!(report.provenance, snapshot.conversion_provenance);

    let (direct, _direct_report) = pg_fwdata::import_file(&fixture_fwdata()).unwrap();
    assert_eq!(report.provenance, direct.conversion_provenance);
}
