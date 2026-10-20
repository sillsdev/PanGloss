//! `pangloss serve` against `pangloss parse`: the warm `result` must equal the cold document, and the protocol must survive bad requests.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");

fn data(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    fn start(project: &str, extra: &[&str]) -> (Server, Value) {
        let mut child = Command::new(BIN)
            .args(["serve", "--project", project])
            .args(extra)
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
        let ready = server.read_line();
        (server, ready)
    }

    fn read_line(&mut self) -> Value {
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).expect("read stdout");
        assert!(n > 0, "server closed stdout before the expected line");
        assert!(line.ends_with('\n'), "protocol lines end with a newline");
        serde_json::from_str(line.trim_end()).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    fn send_raw(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    fn ask(&mut self, request: Value) -> Value {
        self.send_raw(&request.to_string());
        self.read_line()
    }

    fn parse(&mut self, id: &str, word: &str, details: bool) -> Value {
        self.ask(json!({"id": id, "op": "parse", "word": word, "traceDetails": details}))
    }

    fn finish(mut self) -> std::process::ExitStatus {
        drop(self.stdin.take());
        let mut rest = String::new();
        std::io::Read::read_to_string(&mut self.stdout, &mut rest).unwrap();
        assert_eq!(rest, "", "no protocol output after the last response");
        self.child.wait().unwrap()
    }
}

fn is_wall_clock_field(key: &str) -> bool {
    key.ends_with("ElapsedNs")
        || matches!(
            key,
            "elapsedNs" | "timedNs" | "timingOverrunNs" | "unattributedNs"
        )
}

/// Removes every wall-clock field, the only fields allowed to differ between a warm and a cold run.
fn strip_timing(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|key, _| !is_wall_clock_field(key));
            map.values_mut().for_each(strip_timing);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_timing),
        _ => {}
    }
}

fn cold(project: &str, word: &str, details: bool) -> Value {
    let mut args = vec!["parse", project, word, "--trace", "--trace-format", "json"];
    if details {
        args.push("--trace-details");
    }
    let output = Command::new(BIN).args(&args).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Without --trace-details the document follows the `word<TAB>signature` line.
    let body = if details {
        stdout.as_str()
    } else {
        stdout.split_once('\n').map_or("", |(_, rest)| rest)
    };
    if body.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(body).unwrap_or_else(|e| panic!("cold output not JSON ({e}): {body}"))
    }
}

fn assert_warm_equals_cold(server: &mut Server, project: &str, word: &str, details: bool) -> usize {
    let response = server.parse(word, word, details);
    assert_eq!(response["type"], "result", "{word}: {response}");
    assert_eq!(response["id"], word);
    let mut warm = response["result"].clone();
    let mut cold = cold(project, word, details);
    let size = cold.to_string().len();
    strip_timing(&mut warm);
    strip_timing(&mut cold);
    assert_eq!(
        warm, cold,
        "{word} (traceDetails={details}): warm differs from cold"
    );
    size
}

#[test]
fn ready_line_announces_protocol_one_and_load_time() {
    let (server, ready) = Server::start(&data("trace-family.xml"), &[]);
    assert_eq!(ready["type"], "ready");
    assert_eq!(ready["protocol"], 1);
    assert!(ready["loadMs"].is_u64(), "{ready}");
    assert!(ready.get("timing").is_none());
    assert!(server.finish().success());
}

#[test]
fn warm_trace_details_equal_cold_parse_for_words_and_non_words() {
    let project = data("trace-family.xml");
    let (mut server, _) = Server::start(&project, &[]);
    let mut largest = 0;
    for word in [
        "zodut",
        "vem",
        "kib",
        "tay",
        "fom",
        "xqzzkt",
        "q",
        "zodutzodutzodut",
    ] {
        largest = largest.max(assert_warm_equals_cold(&mut server, &project, word, true));
    }
    assert!(
        largest > 2_000,
        "the word set must include a sizeable trace: {largest} bytes"
    );
    assert!(server.finish().success());
}

#[test]
fn a_repeated_word_gives_the_same_result_each_time() {
    let (mut server, _) = Server::start(&data("trace-family.xml"), &[]);
    let mut first = server.parse("a", "zodut", true)["result"].clone();
    let mut second = server.parse("b", "zodut", true)["result"].clone();
    strip_timing(&mut first);
    strip_timing(&mut second);
    assert_eq!(first, second);
    assert!(server.finish().success());
}

#[test]
fn warm_trace_tree_equals_cold_parse_without_trace_details() {
    let project = data("trace-family.xml");
    let (mut server, _) = Server::start(&project, &[]);
    let mut seen = 0;
    for word in ["zodut", "vem", "kib"] {
        let response = server.parse(word, word, false);
        if response["type"] == "error" {
            assert_eq!(response["code"], "parse_incomplete", "{response}");
            continue;
        }
        let mut warm = response["result"].clone();
        let mut cold = cold(&project, word, false);
        strip_timing(&mut warm);
        strip_timing(&mut cold);
        assert_eq!(warm, cold, "{word}");
        seen += 1;
    }
    assert!(seen > 0, "at least one word must produce a trace tree");
    assert!(server.finish().success());
}

#[test]
fn bad_requests_get_an_error_and_the_server_keeps_running() {
    let (mut server, _) = Server::start(&data("trace-family.xml"), &[]);

    server.send_raw("{not json");
    let r = server.read_line();
    assert_eq!(
        (r["type"].as_str(), r["code"].as_str()),
        (Some("error"), Some("malformed_json"))
    );
    assert_eq!(r["id"], Value::Null);

    let r = server.ask(json!({"id": "u", "op": "frobnicate"}));
    assert_eq!(
        (r["type"].as_str(), r["code"].as_str()),
        (Some("error"), Some("unknown_op"))
    );
    assert_eq!(r["id"], "u");

    let r = server.ask(json!({"id": "w", "op": "parse"}));
    assert_eq!(r["code"], "bad_request");
    assert_eq!(r["id"], "w");

    let r = server.ask(json!({"id": "t", "op": "parse", "word": "vem", "traceDetails": "yes"}));
    assert_eq!(r["code"], "bad_request");

    let r = server.ask(json!([1, 2]));
    assert_eq!(r["code"], "bad_request");

    let r = server.ask(json!({"op": "parse", "word": "vem"}));
    assert_eq!(r["code"], "bad_request");
    assert_eq!(r["id"], Value::Null);

    // Blank lines are skipped, and the server still parses afterwards.
    server.send_raw("");
    let r = server.parse("ok", "vem", true);
    assert_eq!(
        (r["type"].as_str(), r["id"].as_str()),
        (Some("result"), Some("ok"))
    );
    assert!(server.finish().success());
}

#[test]
fn shutdown_answers_bye_and_exits_zero() {
    let (mut server, _) = Server::start(&data("trace-family.xml"), &[]);
    let r = server.ask(json!({"id": "bye-1", "op": "shutdown"}));
    assert_eq!(r, json!({"id": "bye-1", "type": "bye"}));
    assert!(server.finish().success());
}

#[test]
fn eof_on_stdin_exits_zero_without_output() {
    let (server, _) = Server::start(&data("trace-family.xml"), &[]);
    assert!(server.finish().success());
}

#[test]
fn load_failure_reports_fatal_and_exits_non_zero() {
    let (server, fatal) = Server::start("no-such-project.fwdata", &[]);
    assert_eq!(fatal["type"], "fatal");
    assert_eq!(fatal["code"], "load_failed");
    assert!(fatal["message"].as_str().is_some_and(|m| !m.is_empty()));
    assert!(!server.finish().success());
}

#[test]
fn a_missing_project_flag_is_fatal_bad_arguments() {
    let output = Command::new(BIN).arg("serve").output().unwrap();
    assert!(!output.status.success());
    let line: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        (line["type"].as_str(), line["code"].as_str()),
        (Some("fatal"), Some("bad_arguments"))
    );
}

#[test]
fn timing_flag_adds_phase_timings() {
    let (mut server, ready) = Server::start(&data("trace-family.xml"), &["--timing"]);
    assert!(ready["timing"]["compileMs"].is_u64(), "{ready}");
    assert!(ready["timing"]["importMs"].is_u64(), "{ready}");
    let r = server.parse("t", "vem", true);
    assert!(
        r["timing"]["parseMs"].is_u64() && r["timing"]["renderMs"].is_u64(),
        "{r}"
    );
    assert!(server.finish().success());
}

#[test]
fn describe_publishes_the_serve_protocol_version() {
    let output = Command::new(BIN).arg("--describe").output().unwrap();
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(doc["interfaces"]["serveProtocol"], 1);
    assert!(doc["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["name"] == "serve"));
}
