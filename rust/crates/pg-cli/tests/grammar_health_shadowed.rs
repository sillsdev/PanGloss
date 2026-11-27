//! `pangloss grammar-health --shadowed` reports one finding per proven row, with the row's GUIDs and key.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");
const CODE: &str = "grammar.allomorph.shadowed";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

fn health(project: &PathBuf, extra: &[&str]) -> (Value, std::process::Output) {
    let out = std::env::temp_dir().join(format!(
        "pangloss-shadowed-cli-{}-{}.json",
        std::process::id(),
        extra.join("")
    ));
    let output = Command::new(BIN)
        .arg("grammar-health")
        .arg(project)
        .arg(&out)
        .args(extra)
        .output()
        .expect("pangloss grammar-health must run");
    let report = std::fs::read_to_string(&out)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    let _ = std::fs::remove_file(out);
    (report, output)
}

#[test]
fn every_proven_row_is_one_finding_on_every_fixture() {
    for name in [
        "shadowed-fixture.xml",
        "shadowed-copies-fixture.xml",
        "shadowed-passes-fixture.xml",
    ] {
        let project = fixture(name);
        let (report, output) = health(&project, &["--shadowed", "--include-selectable"]);
        assert!(output.status.success(), "{name}: {output:?}");
        let section = &report["shadowed_alternates"];
        assert_eq!(section["complete"], true, "{name}");
        let shadowed: Vec<&Value> = section["alternates"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["verdict"] == "shadowed")
            .collect();
        let findings: Vec<&Value> = report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["code"] == CODE)
            .collect();
        assert_eq!(
            findings.len(),
            shadowed.len(),
            "{name}: one finding per proven row"
        );
        for (finding, row) in findings.iter().zip(&shadowed) {
            let message = finding["description"].as_str().unwrap();
            let removable = row["removableGuids"].as_array().unwrap();
            assert!(!removable.is_empty(), "{name}: {row}");
            for guid in removable {
                assert!(
                    message.contains(guid.as_str().unwrap()),
                    "{name}: {message}"
                );
            }
            assert!(
                message.contains(row["xmlKey"].as_str().unwrap()),
                "{name}: {message}"
            );
        }
    }
}

#[test]
fn the_proven_rows_of_the_default_fixture_are_reported_as_findings() {
    let (report, output) = health(&fixture("shadowed-fixture.xml"), &["--shadowed"]);
    assert!(output.status.success(), "{output:?}");
    let findings = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == CODE)
        .count();
    assert_eq!(findings, 2, "{report}");
    assert_eq!(report["shadowed_alternates"]["counts"]["shadowed"], 2);
    // Without --include-selectable the selectable rows are counted, not listed.
    assert!(report["shadowed_alternates"]["alternates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["verdict"] != "selectable"));
}

#[test]
fn the_report_is_absent_unless_asked_for() {
    let (report, output) = health(&fixture("shadowed-fixture.xml"), &[]);
    assert!(output.status.success(), "{output:?}");
    assert!(report.get("shadowed_alternates").is_none(), "{report}");
    assert!(report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .all(|d| d["code"] != CODE));
}

#[test]
fn include_selectable_needs_shadowed() {
    let (_, output) = health(&fixture("shadowed-fixture.xml"), &["--include-selectable"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("--include-selectable requires --shadowed"));
}
