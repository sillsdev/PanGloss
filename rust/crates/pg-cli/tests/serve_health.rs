//! `pangloss serve` op `health`: the `grammar-health` report from the grammar a serve process already holds, and the `--describe` declaration of it.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    fn start(project: &Path) -> Server {
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
        assert_eq!(server.read()["type"], "ready");
        server
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).expect("read stdout");
        assert!(n > 0, "server closed stdout");
        serde_json::from_str(line.trim_end()).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    fn ask(&mut self, request: Value) -> Value {
        let stdin = self.stdin.as_mut().expect("stdin open");
        writeln!(stdin, "{request}").unwrap();
        stdin.flush().unwrap();
        self.read()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

fn cli_report(project: &Path, extra: &[&str]) -> Value {
    let output = Command::new(BIN)
        .arg("grammar-health")
        .arg(project)
        .args(extra)
        .output()
        .expect("pangloss grammar-health must run");
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("report is not JSON ({e}): {output:?}"))
}

/// Drops wall-clock fields and sorts every array of objects, so only content is compared.
fn normalize(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|key, _| !(key.ends_with("Ms") || key.ends_with("Ns") || key == "elapsed"));
            map.values_mut().for_each(normalize);
        }
        Value::Array(items) => {
            items.iter_mut().for_each(normalize);
            items.sort_by_key(|item| item.to_string());
        }
        _ => {}
    }
}

fn assert_same_report(serve: &Value, cli: &Value, what: &str) {
    assert_eq!(serve["type"], "result", "{what}: {serve}");
    assert_eq!(serve["op"], "health", "{what}");
    let (mut serve, mut cli) = (serve["report"].clone(), cli.clone());
    normalize(&mut serve);
    normalize(&mut cli);
    assert_eq!(serve, cli, "{what}: serve report differs from the command");
}

/// The synthetic FieldWorks fixture plus two approved analyses of `kat`: one PanGloss parses, one naming a root it does not.
fn stored_analysis_project() -> PathBuf {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pg-fwdata/tests/data/fixture.fwdata");
    let mut xml = std::fs::read_to_string(fixture).unwrap().replace(
        "guid=\"00000000-0000-0000-0000-0000000000ff\"",
        "guid=\"00000000-0000-0000-0000-000000000017\"",
    );
    let mut records = String::new();
    for (index, (allomorph, msa)) in [
        (
            "00000000-0000-0000-0000-000000000031",
            "00000000-0000-0000-0000-000000000032",
        ),
        (
            "00000000-0000-0000-0000-000000000041",
            "00000000-0000-0000-0000-000000000042",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let (w, a, b) = (
            format!("e0000000-0000-0000-0000-{index:012x}"),
            format!("f0000000-0000-0000-0000-{index:012x}"),
            format!("c0000000-0000-0000-0000-{index:012x}"),
        );
        records.push_str(&format!(
            "<rt class=\"WfiWordform\" guid=\"{w}\"><Form><AUni ws=\"fx\">kat</AUni></Form><Analyses><objsur guid=\"{a}\" t=\"r\" /></Analyses></rt><rt class=\"WfiAnalysis\" guid=\"{a}\"><MorphBundles><objsur guid=\"{b}\" t=\"o\" /></MorphBundles></rt><rt class=\"WfiMorphBundle\" guid=\"{b}\"><Morph><objsur guid=\"{allomorph}\" t=\"r\" /></Morph><Msa><objsur guid=\"{msa}\" t=\"r\" /></Msa></rt>"
        ));
    }
    let end = xml.rfind("</languageproject>").unwrap();
    xml.insert_str(end, &records);
    let dir = std::env::temp_dir().join(format!("pangloss-serve-health-fw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("project.fwdata");
    std::fs::write(&path, xml).unwrap();
    path
}

#[test]
fn health_then_parse_in_one_session_without_reloading() {
    // The grammar file is gone before the first request, so only a held grammar can answer.
    let dir = std::env::temp_dir().join(format!("pangloss-serve-health-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let project = dir.join("grammar.xml");
    std::fs::copy(data("shadowed-fixture.xml"), &project).unwrap();
    let mut s = Server::start(&project);
    std::fs::remove_file(&project).unwrap();
    let health = s.ask(json!({"id": "h", "op": "health"}));
    assert_eq!(health["type"], "result", "{health}");
    assert_eq!(health["id"], "h");
    assert!(health["report"]["diagnostics"].is_array(), "{health}");
    let parse = s.ask(json!({"id": "p", "op": "parse", "word": "a"}));
    assert_eq!(parse["type"], "result", "{parse}");
    let again = s.ask(json!({"id": "h2", "op": "health"}));
    assert_eq!(again["report"], health["report"], "health is repeatable");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn health_equals_the_grammar_health_command() {
    for (name, request, extra) in [
        ("shadowed-fixture.xml", json!({}), vec![]),
        (
            "shadowed-fixture.xml",
            json!({"shadowed": true, "includeSelectable": true}),
            vec!["--shadowed", "--include-selectable"],
        ),
        (
            "shadowed-copies-fixture.xml",
            json!({"shadowed": true}),
            vec!["--shadowed"],
        ),
    ] {
        let project = data(name);
        let mut s = Server::start(&project);
        let mut request = request;
        request["id"] = json!("h");
        request["op"] = json!("health");
        let serve = s.ask(request);
        assert_same_report(&serve, &cli_report(&project, &extra), name);
    }
}

#[test]
fn health_equals_the_command_on_a_fieldworks_project_with_stored_analyses_options() {
    let project = stored_analysis_project();
    let mut s = Server::start(&project);
    for (mode, extra) in [
        ("bounded", vec![]),
        ("all", vec!["--stored-analyses", "all"]),
        ("off", vec!["--stored-analyses", "off"]),
    ] {
        let serve = s.ask(json!({"id": mode, "op": "health", "storedAnalyses": mode}));
        let cli = cli_report(&project, &extra);
        if mode != "off" {
            assert!(
                cli["stored_analysis_words"]
                    .as_array()
                    .is_some_and(|w| !w.is_empty()),
                "the fixture must exercise the stored-analysis section: {cli}"
            );
        }
        assert_same_report(&serve, &cli, mode);
    }
    let bad = s.ask(json!({"id": "x", "op": "health", "storedAnalyses": "some"}));
    assert_eq!(bad["code"], "bad_request", "{bad}");
}

#[test]
fn unknown_or_malformed_options_are_bad_requests() {
    let mut s = Server::start(&data("shadowed-fixture.xml"));
    for request in [
        json!({"id": "a", "op": "health", "fst": true}),
        json!({"id": "b", "op": "health", "shadowed": "yes"}),
        json!({"id": "c", "op": "health", "includeSelectable": true}),
        json!({"id": "d", "op": "health", "storedAnalyses": 1}),
    ] {
        let r = s.ask(request.clone());
        assert_eq!(r["type"], "error", "{request}: {r}");
        assert_eq!(r["code"], "bad_request", "{request}: {r}");
    }
    let ok = s.ask(json!({"id": "ok", "op": "health"}));
    assert_eq!(ok["type"], "result", "the session survives bad requests");
}

#[test]
fn describe_declares_the_health_op() {
    let output = Command::new(BIN).arg("--describe").output().unwrap();
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
    let decl = &doc["interfaces"]["serveOps"]["health"];
    assert_eq!(
        decl["request"],
        json!(["shadowed", "includeSelectable", "storedAnalyses"])
    );
    assert_eq!(decl["notRun"], json!(["fst", "snapshotOut", "factsOut"]));
}
