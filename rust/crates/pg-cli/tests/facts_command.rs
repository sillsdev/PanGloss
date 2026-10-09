use std::path::Path;
use std::process::Command;

use pg_snapshot::phonology::Phonology;
use pg_snapshot::Snapshot;
use pg_snapshot::{BoundaryMarker, Project, WsForm};

fn executable() -> &'static str {
    env!("CARGO_BIN_EXE_pangloss")
}

fn minimal_snapshot() -> Snapshot {
    let project = Project {
        vernacular_writing_systems: vec!["qaa".into()],
        ..Project::default()
    };
    Snapshot::new(
        project,
        pg_snapshot::FeatureSystems::default(),
        Phonology {
            boundary_markers: vec![BoundaryMarker {
                guid: "00000000-0000-0000-0000-000000000001".into(),
                name: "Morpheme boundary".into(),
                representations: vec![WsForm {
                    ws: "qaa".into(),
                    form: "+".into(),
                }],
            }],
            ..Phonology::default()
        },
        pg_snapshot::Morphology::default(),
        pg_snapshot::Lexicon::default(),
    )
}

fn write_context(path: &Path) {
    std::fs::write(
        path,
        br#"{"format":"pangloss-facts-context","version":1,"baselineToken":{"id":"cli-test"},"inputKind":"baseline","dryRunDigest":null}"#,
    )
    .unwrap();
}

#[test]
fn facts_command_writes_one_json_result_and_rejects_input_collision() {
    let temp = tempfile::tempdir().unwrap();
    let snapshot_path = temp.path().join("snapshot.json");
    let context_path = temp.path().join("context.json");
    let output_path = temp.path().join("facts.sqlite");
    let source = minimal_snapshot().to_json();
    std::fs::write(&snapshot_path, &source).unwrap();
    write_context(&context_path);

    let result = Command::new(executable())
        .arg("facts")
        .arg(&snapshot_path)
        .arg("--out")
        .arg(&output_path)
        .arg("--context")
        .arg(&context_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(response["applicationId"], 1_346_848_321);
    assert_eq!(response["schemaVersion"], 8);
    assert_eq!(response["compileStatus"], "completed");
    assert!(response["outputBytes"].as_u64().unwrap() > 0);
    assert!(output_path.exists());

    let missing_stats_manifest = Command::new(executable())
        .arg("facts")
        .arg(&snapshot_path)
        .arg("--out")
        .arg(temp.path().join("missing-manifest.sqlite"))
        .arg("--context")
        .arg(&context_path)
        .arg("--stats")
        .arg(temp.path().join("stats.sqlite"))
        .output()
        .unwrap();
    assert!(!missing_stats_manifest.status.success());
    assert!(String::from_utf8_lossy(&missing_stats_manifest.stderr)
        .contains("--stats and --stats-manifest must be supplied together"));

    let source_before = std::fs::read(&snapshot_path).unwrap();
    let collision = Command::new(executable())
        .arg("facts")
        .arg(&snapshot_path)
        .arg("--out")
        .arg(&snapshot_path)
        .arg("--context")
        .arg(&context_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!collision.status.success());
    assert!(String::from_utf8_lossy(&collision.stderr).contains("input_output_collision"));
    assert_eq!(source_before, std::fs::read(&snapshot_path).unwrap());

    let context_before = std::fs::read(&context_path).unwrap();
    let context_collision = Command::new(executable())
        .arg("facts")
        .arg(&snapshot_path)
        .arg("--out")
        .arg(&context_path)
        .arg("--context")
        .arg(&context_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!context_collision.status.success());
    assert!(String::from_utf8_lossy(&context_collision.stderr).contains("input_output_collision"));
    assert_eq!(context_before, std::fs::read(&context_path).unwrap());
}
