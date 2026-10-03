//! Exercises compile refusals at the CLI output boundary used by hosts.

use std::process::Command;

#[test]
fn parse_refusal_is_a_json_stderr_line_and_health_still_writes_findings() {
    let scratch = tempfile::tempdir().unwrap();
    let grammar_path = scratch.path().join("unknown-provenance.json");
    let health_path = scratch.path().join("health.json");
    let mut snapshot = pg_snapshot::Snapshot::new(
        Default::default(),
        Default::default(),
        pg_snapshot::phonology::Phonology {
            boundary_markers: vec![pg_snapshot::phonology::BoundaryMarker {
                guid: "00000000-0000-0000-0000-000000000001".into(),
                name: "Morpheme boundary".into(),
                representations: vec![pg_snapshot::WsForm {
                    ws: "und".into(),
                    form: "+".into(),
                }],
            }],
            ..Default::default()
        },
        Default::default(),
        Default::default(),
    );
    snapshot.conversion_provenance = Default::default();
    std::fs::write(&grammar_path, snapshot.to_json()).unwrap();
    let parsed = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .arg("parse")
        .arg(&grammar_path)
        .arg("a")
        .output()
        .unwrap();
    assert!(!parsed.status.success());
    assert!(parsed.stdout.is_empty());
    let stderr = String::from_utf8(parsed.stderr).unwrap();
    let failure: serde_json::Value = stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|value| value["status"] == "compile_error")
        .expect("standalone compile JSON on stderr");
    assert_eq!(failure["schema_version"], 1);
    assert!(failure["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |issue| issue["code"] == "conversion.source-provenance-unknown"
                && issue["fatal"] == true
        ));
    assert!(!stderr.contains("ConversionError {"));

    let health = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .arg("grammar-health")
        .arg(&grammar_path)
        .arg(&health_path)
        .output()
        .unwrap();
    assert!(!health.status.success());
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(health_path).unwrap()).unwrap();
    assert_eq!(report["schema_version"], 4);
    assert!(report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |finding| finding["code"] == "conversion.source-provenance-unknown"
                && finding["level"] == "error"
        ));
}
