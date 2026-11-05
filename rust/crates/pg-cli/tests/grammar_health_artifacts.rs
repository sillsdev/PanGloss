//! `grammar-health --snapshot-out/--facts-out` output equals `import` then `facts` (#176).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run(args: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args(args)
        .output()
        .unwrap()
}

fn text(arg: &str) -> PathBuf {
    PathBuf::from(arg)
}

fn broken_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../pg-fwdata/tests/data/fixture.fwdata")
}

/// The synthetic project without its one dangling environment reference, so it imports cleanly.
fn fixture(dir: &Path) -> PathBuf {
    let source = std::fs::read_to_string(broken_fixture()).unwrap();
    let dangling = "<objsur guid=\"00000000-0000-0000-0000-0000000000ff\" t=\"r\" />";
    assert_eq!(source.matches(dangling).count(), 1);
    let path = dir.join("project.fwdata");
    std::fs::write(&path, source.replace(dangling, "")).unwrap();
    path
}

fn write_context(path: &Path) {
    std::fs::write(
        path,
        br#"{"format":"pangloss-facts-context","version":1,"baselineToken":{"id":"health-test"},"inputKind":"baseline","dryRunDigest":null}"#,
    )
    .unwrap();
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The facts file embeds its producer and a stamp, so compare what `facts` computes, byte for byte.
#[test]
fn snapshot_and_facts_equal_import_then_facts() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let context = dir.join("context.json");
    write_context(&context);
    let project = fixture(dir);

    let import_snapshot = dir.join("import.json");
    let imported = run(&[&text("import"), &project, &import_snapshot]);
    assert!(imported.status.success(), "{}", stderr(&imported));
    let facts_reference = dir.join("reference.sqlite");
    let facts = run(&[
        &text("facts"),
        &import_snapshot,
        &text("--out"),
        &facts_reference,
        &text("--context"),
        &context,
    ]);
    assert!(facts.status.success(), "{}", stderr(&facts));

    let health_snapshot = dir.join("health.json");
    let health_facts = dir.join("health.sqlite");
    let health_report = dir.join("health-report.json");
    let health = run(&[
        &text("grammar-health"),
        &project,
        &health_report,
        &text("--snapshot-out"),
        &health_snapshot,
        &text("--facts-out"),
        &health_facts,
        &text("--facts-context"),
        &context,
    ]);
    assert!(health.status.success(), "{}", stderr(&health));
    assert!(
        stderr(&health).contains("reused from this run"),
        "facts must reuse the health compile: {}",
        stderr(&health)
    );

    assert_eq!(
        std::fs::read(&import_snapshot).unwrap(),
        std::fs::read(&health_snapshot).unwrap(),
        "snapshot must be byte-identical to `import`"
    );
    assert_eq!(
        std::fs::read(&facts_reference).unwrap(),
        std::fs::read(&health_facts).unwrap(),
        "facts must be byte-identical to `facts` on that snapshot"
    );

    // The report is the one a plain run writes.
    let plain_report = dir.join("plain-report.json");
    let plain = run(&[&text("grammar-health"), &project, &plain_report]);
    assert!(plain.status.success(), "{}", stderr(&plain));
    assert_eq!(
        std::fs::read(&plain_report).unwrap(),
        std::fs::read(&health_report).unwrap()
    );
}

#[test]
fn facts_record_the_hash_of_the_snapshot_file_written() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let context = dir.join("context.json");
    write_context(&context);
    let snapshot = dir.join("s.json");
    let facts = dir.join("f.sqlite");
    let health = run(&[
        &text("grammar-health"),
        &fixture(dir),
        &dir.join("r.json"),
        &text("--snapshot-out"),
        &snapshot,
        &text("--facts-out"),
        &facts,
        &text("--facts-context"),
        &context,
    ]);
    assert!(health.status.success(), "{}", stderr(&health));
    let expected = pg_assess::source_sha256(&std::fs::read(&snapshot).unwrap());
    let connection = rusqlite::Connection::open(&facts).unwrap();
    let mut found = false;
    let tables: Vec<String> = connection
        .prepare("select name from sqlite_master where type='table'")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for table in tables {
        let sql = format!("select * from \"{table}\"");
        let mut statement = connection.prepare(&sql).unwrap();
        let columns = statement.column_count();
        let mut rows = statement.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            for column in 0..columns {
                if let Ok(value) = row.get::<_, String>(column) {
                    found |= value == expected;
                }
            }
        }
    }
    assert!(found, "facts must record source_sha256 {expected}");
}

#[test]
fn snapshot_only_and_bad_flag_combinations() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let snapshot = dir.join("s.json");
    let only = run(&[
        &text("grammar-health"),
        &fixture(dir),
        &dir.join("r.json"),
        &text("--snapshot-out"),
        &snapshot,
    ]);
    assert!(only.status.success(), "{}", stderr(&only));
    let reference = dir.join("i.json");
    assert!(run(&[&text("import"), &fixture(dir), &reference])
        .status
        .success());
    assert_eq!(
        std::fs::read(&reference).unwrap(),
        std::fs::read(&snapshot).unwrap()
    );

    let facts_without_context = run(&[
        &text("grammar-health"),
        &fixture(dir),
        &text("--facts-out"),
        &dir.join("f.sqlite"),
    ]);
    assert!(!facts_without_context.status.success());
    assert!(stderr(&facts_without_context).contains("--facts-context"));

    let collision = run(&[
        &text("grammar-health"),
        &fixture(dir),
        &text("--snapshot-out"),
        &fixture(dir),
    ]);
    assert!(!collision.status.success());
    assert!(stderr(&collision).contains("must differ"));

    let not_a_project = dir.join("grammar.xml");
    std::fs::write(&not_a_project, "<x/>").unwrap();
    let wrong_input = run(&[
        &text("grammar-health"),
        &not_a_project,
        &text("--snapshot-out"),
        &dir.join("never.json"),
    ]);
    assert!(!wrong_input.status.success());
    assert!(stderr(&wrong_input).contains(".fwdata"));
    assert!(!dir.join("never.json").exists());
}

#[test]
fn a_refused_import_writes_the_report_but_no_snapshot_or_facts() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let context = dir.join("context.json");
    write_context(&context);
    let report = dir.join("r.json");
    let snapshot = dir.join("s.json");
    let facts = dir.join("f.sqlite");
    let broken = broken_fixture();
    let reference = run(&[&text("import"), &broken, &dir.join("i.json")]);
    assert!(
        !reference.status.success(),
        "fixture must be refused by import"
    );
    let health = run(&[
        &text("grammar-health"),
        &broken,
        &report,
        &text("--snapshot-out"),
        &snapshot,
        &text("--facts-out"),
        &facts,
        &text("--facts-context"),
        &context,
    ]);
    assert!(!health.status.success());
    assert!(report.exists(), "the health report is still written");
    assert!(!snapshot.exists() && !facts.exists());
    assert!(!dir.join("i.json").exists());
}
