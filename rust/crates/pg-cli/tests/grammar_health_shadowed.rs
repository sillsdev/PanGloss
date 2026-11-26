//! `pangloss grammar-health --shadowed` states the same verdicts and GUIDs as `pangloss serve`'s `shadowedAllomorphs` op.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");
const CODE: &str = "grammar.allomorph.shadowed";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

fn serve_op(project: &PathBuf, include_selectable: bool) -> Value {
    let mut child = Command::new(BIN)
        .args(["serve", "--project"])
        .arg(project)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("pangloss serve must start");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert_ne!(
            stdout.read_line(&mut line).unwrap(),
            0,
            "serve exited before its ready banner"
        );
        if serde_json::from_str::<Value>(line.trim_end()).is_ok_and(|v| v["type"] == "ready") {
            break;
        }
    }
    let request =
        json!({"id": "s", "op": "shadowedAllomorphs", "includeSelectable": include_selectable});
    writeln!(stdin, "{request}").unwrap();
    stdin.flush().unwrap();
    line.clear();
    stdout.read_line(&mut line).unwrap();
    drop(stdin);
    let _ = child.wait();
    serde_json::from_str(line.trim_end()).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
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
fn the_command_line_report_equals_the_serve_op_on_every_fixture() {
    for name in [
        "shadowed-fixture.xml",
        "shadowed-copies-fixture.xml",
        "shadowed-passes-fixture.xml",
    ] {
        let project = fixture(name);
        let serve = serve_op(&project, true);
        assert_eq!(serve["type"], "result", "{name}: {serve}");
        let (report, output) = health(&project, &["--shadowed", "--include-selectable"]);
        assert!(output.status.success(), "{name}: {output:?}");
        let section = &report["shadowed_alternates"];
        for key in [
            "complete",
            "counts",
            "alternates",
            "deadCounts",
            "deadContinuations",
        ] {
            assert_eq!(
                section[key], serve[key],
                "{name}: {key} differs from the serve op"
            );
        }
        let shadowed: Vec<&Value> = serve["alternates"]
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
