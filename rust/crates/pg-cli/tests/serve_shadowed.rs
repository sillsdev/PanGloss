//! `pangloss serve` op `shadowedAllomorphs`: shadowed affix alternates and dead stem continuations, and the `--describe` declaration of both.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    fn start() -> Server {
        Server::start_with("shadowed-fixture.xml")
    }

    fn start_with(fixture: &str) -> Server {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(fixture);
        let mut child = Command::new(BIN)
            .args(["serve", "--project"])
            .arg(project)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("pangloss serve must start");
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut server = Server {
            child,
            stdin,
            stdout,
        };
        assert_eq!(server.ask_raw()["type"], "ready");
        server
    }

    fn ask_raw(&mut self) -> Value {
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).expect("read stdout");
        assert!(n > 0, "server closed stdout");
        serde_json::from_str(line.trim_end()).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    fn ask(&mut self, request: Value) -> Value {
        let stdin = self.stdin.as_mut().expect("stdin open");
        stdin.write_all(request.to_string().as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
        self.ask_raw()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

#[test]
fn reports_the_shadowed_passive_alternate_and_the_dead_vc_a_root() {
    let mut s = Server::start();
    let r = s.ask(json!({"id": "s", "op": "shadowedAllomorphs"}));
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(r["op"], "shadowedAllomorphs");
    let rows = r["alternates"].as_array().unwrap();
    // The unconstrained root alternate `bal` also covers the environment-bound `ba`.
    assert_eq!(rows.len(), 2, "{r}");
    assert!(rows.iter().all(|row| row["verdict"] == "shadowed"));
    let pass = rows.iter().find(|row| row["ruleName"] == "PASS").unwrap();
    assert_eq!(pass["index"], 1);
    assert_eq!(pass["blockers"][0]["index"], 0);
    assert_eq!(r["counts"]["shadowed"], 2);

    let dead = r["deadContinuations"].as_array().unwrap();
    assert_eq!(dead.len(), 2, "{r}");
    let proven = dead.iter().find(|d| d["continuation"] == "proven").unwrap();
    assert_eq!(proven["form"], "ba");
    assert_eq!(proven["features"], json!(["VC-A"]));
    assert_eq!(proven["blockedBy"][0]["kind"], "required_mpr");
    assert_eq!(proven["blockedBy"][0]["names"], json!(["VC-B"]));
    assert!(dead
        .iter()
        .any(|d| d["form"] == "bal" && d["continuation"] == "maybe"));
    assert_eq!(r["deadCounts"]["proven"], 1);
}

#[test]
fn include_selectable_lists_the_rest_and_bad_requests_are_typed() {
    let mut s = Server::start();
    let all = s.ask(json!({"id": "a", "op": "shadowedAllomorphs", "includeSelectable": true}));
    assert!(all["alternates"].as_array().unwrap().len() > 1, "{all}");
    assert!(
        all["deadContinuations"].as_array().unwrap().len() > 2,
        "{all}"
    );
    let bad = s.ask(json!({"id": "b", "op": "shadowedAllomorphs", "nope": 1}));
    assert_eq!(bad["code"], "bad_request");
    let bad = s.ask(json!({"id": "c", "op": "shadowedAllomorphs", "includeSelectable": 1}));
    assert_eq!(bad["code"], "bad_request");
    assert_eq!(
        all,
        s.ask(json!({"id": "a", "op": "shadowedAllomorphs", "includeSelectable": true}))
    );
}

#[test]
fn describe_declares_the_op() {
    let output = Command::new(BIN).arg("--describe").output().unwrap();
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
    let decl = &doc["interfaces"]["serveOps"]["shadowedAllomorphs"];
    assert_eq!(decl["version"], 1);
    assert_eq!(decl["readOnly"], true);
    assert_eq!(
        decl["verdicts"],
        json!(["shadowed", "maybe_shadowed", "selectable"])
    );
    assert_eq!(decl["continuations"], json!(["proven", "maybe", "live"]));

    let mut s = Server::start();
    let r = s.ask(json!({"id": "s", "op": "shadowedAllomorphs", "includeSelectable": true}));
    let keys = |v: &Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    let declared = |v: &Value| -> Vec<String> {
        let mut k: Vec<String> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect();
        k.sort();
        k
    };
    for row in r["alternates"].as_array().unwrap() {
        assert_eq!(keys(row), declared(&decl["rowFields"]), "{row}");
    }
    for row in r["deadContinuations"].as_array().unwrap() {
        assert_eq!(
            keys(row),
            declared(&decl["deadContinuationFields"]),
            "{row}"
        );
    }
}

#[test]
fn a_source_allomorph_is_proven_only_when_every_copy_is() {
    let mut s = Server::start_with("shadowed-passes-fixture.xml");
    let r = s.ask(json!({"id": "p", "op": "shadowedAllomorphs", "includeSelectable": true}));
    let row = |key: &str| {
        r["alternates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["xmlKey"] == key)
            .unwrap_or_else(|| panic!("no row {key}: {r}"))
    };
    // `z#0` is covered by `s`, but its sibling pass `z#1` (another input pattern) is not.
    assert_eq!(row("z#0")["verdict"], "maybe_shadowed");
    assert_eq!(
        row("z#0")["reasons"],
        json!(["source_allomorph_selectable_elsewhere"])
    );
    assert_eq!(row("z#0")["siblings"][0]["xmlKey"], "z#1");
    assert_eq!(row("z#1")["verdict"], "selectable");
    // Both passes of `y` are covered, so the source allomorph is removable.
    assert_eq!(row("y#0")["verdict"], "shadowed");
    assert_eq!(row("y#1")["verdict"], "shadowed");
    assert_eq!(row("y#0")["siblings"], json!([]));

    let dead = r["deadContinuations"].as_array().unwrap();
    let copy = |key: &str| dead.iter().find(|d| d["xmlKey"] == key).unwrap();
    assert_eq!(copy("g#1")["continuation"], "live");
    assert_eq!(copy("g#0")["continuation"], "maybe");
    assert_eq!(
        copy("g#0")["reasons"],
        json!(["source_allomorph_live_elsewhere"])
    );
    assert_eq!(copy("g#0")["siblings"][0]["xmlKey"], "g#1");
    assert_eq!(r["deadCounts"]["proven"], 0);
}

#[test]
fn a_covered_copy_beside_a_live_copy_is_not_removable() {
    // The shape C# hc.dll parses `damz` through `/ m _` (the `z_1` copy), so `z` must not be shadowed.
    let mut s = Server::start_with("shadowed-copies-fixture.xml");
    let r = s.ask(json!({"id": "c", "op": "shadowedAllomorphs", "includeSelectable": true}));
    let rows: Vec<&Value> = r["alternates"].as_array().unwrap().iter().collect();
    assert!(
        rows.iter()
            .all(|a| a["verdict"] != "shadowed" || a["xmlKey"] != "z_0"),
        "{r}"
    );
    assert!(
        r["deadContinuations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|d| d["continuation"] != "proven"),
        "{r}"
    );
}
