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

/// The cold command's stdout and exit code.
fn cold_raw(project: &str, word: &str, details: bool, extra: &[&str]) -> (String, i32) {
    let mut args = vec!["parse", project, word, "--trace", "--trace-format", "json"];
    if details {
        args.push("--trace-details");
    }
    args.extend_from_slice(extra);
    let output = Command::new(BIN).args(&args).output().unwrap();
    (
        String::from_utf8(output.stdout).unwrap(),
        output.status.code().unwrap(),
    )
}

/// Asserts the warm response is a result whose `exitCode` and `result` equal the cold command's, ignoring wall-clock fields; returns the cold stdout size.
fn assert_warm_equals_cold(
    server: &mut Server,
    project: &str,
    word: &str,
    details: bool,
    extra: &[&str],
) -> usize {
    let response = server.parse(word, word, details);
    assert_eq!(response["type"], "result", "{word}: {response}");
    assert_eq!(response["id"], word);
    let (stdout, code) = cold_raw(project, word, details, extra);
    assert_eq!(
        response["exitCode"], code,
        "{word} (traceDetails={details}): exit code"
    );
    if details {
        let mut warm = response["result"].clone();
        let mut cold: Value =
            serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("cold not JSON ({e})"));
        strip_timing(&mut warm);
        strip_timing(&mut cold);
        assert_eq!(warm, cold, "{word}: warm differs from cold");
    } else {
        assert_eq!(
            response["result"],
            Value::String(stdout.clone()),
            "{word}: stdout text"
        );
    }
    stdout.len()
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
        largest = largest.max(assert_warm_equals_cold(
            &mut server,
            &project,
            word,
            true,
            &[],
        ));
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
fn warm_plain_mode_equals_cold_stdout_exactly() {
    let project = data("trace-family.xml");
    let (mut server, _) = Server::start(&project, &[]);
    for word in ["zodut", "vem", "kib", "xqzzkt"] {
        assert_warm_equals_cold(&mut server, &project, word, false, &[]);
    }
    assert!(server.finish().success());
}

#[test]
fn an_incomplete_parse_is_a_result_with_the_cold_exit_code() {
    let project = data("trace-family.xml");
    let (mut server, _) = Server::start(&project, &["--step-cap", "3"]);
    assert_warm_equals_cold(&mut server, &project, "zodut", true, &["--step-cap", "3"]);
    let r = server.parse("p", "zodut", false);
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(r["exitCode"], 1);
    assert_warm_equals_cold(&mut server, &project, "zodut", false, &["--step-cap", "3"]);
    assert!(server.finish().success());
}

/// Both modes carry the word's `steps` and `workSteps` beside `result`, equal to the trace document's own search counts.
#[test]
fn results_carry_the_words_search_counts_in_both_modes() {
    for extra in [&[][..], &["--step-cap", "3"][..]] {
        let (mut server, _) = Server::start(&data("trace-family.xml"), extra);
        for word in ["zodut", "vem", "xqzzkt"] {
            let detailed = server.parse("d", word, true);
            let plain = server.parse("p", word, false);
            let search = &detailed["result"]["search"];
            assert!(
                search["workSteps"].as_u64().unwrap() > 0,
                "{word}: {search}"
            );
            for r in [&detailed, &plain] {
                assert_eq!(r["steps"], search["steps"], "{word} {extra:?}: {r}");
                assert_eq!(r["workSteps"], search["workSteps"], "{word} {extra:?}: {r}");
            }
        }
        assert!(server.finish().success());
    }
}

#[test]
fn guessed_words_match_cold_in_both_modes() {
    let project = data("trace-guess.xml");
    let (mut server, _) = Server::start(&project, &["--guess"]);
    assert_warm_equals_cold(&mut server, &project, "gag", true, &["--guess"]);
    assert_warm_equals_cold(&mut server, &project, "gag", false, &["--guess"]);
    let r = server.parse("g", "gag", false);
    assert!(
        r["result"].as_str().unwrap().contains("guessed:\ttrue\n"),
        "{r}"
    );
    assert!(server.finish().success());
}

#[test]
fn an_oversized_request_line_is_refused_and_the_server_keeps_running() {
    let (mut server, _) = Server::start(&data("trace-family.xml"), &[]);
    server.send_raw(&"x".repeat((1 << 20) + 10));
    let r = server.read_line();
    assert_eq!(
        (r["type"].as_str(), r["code"].as_str()),
        (Some("error"), Some("request_too_large"))
    );
    let r = server.parse("after", "vem", true);
    assert_eq!(r["type"], "result");
    assert!(server.finish().success());
}

#[test]
fn closing_stdin_during_a_long_parse_exits_promptly() {
    let (mut server, _) = Server::start(&data("trace-family.xml"), &[]);
    let word = "zodut".repeat(3000);
    server.send_raw(
        &json!({"id": "slow", "op": "parse", "word": word, "traceDetails": true}).to_string(),
    );
    std::thread::sleep(std::time::Duration::from_millis(300));
    let started = std::time::Instant::now();
    drop(server.stdin.take());
    let status = server.child.wait().unwrap();
    assert!(status.success());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
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
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["interfaces"]["batchTsv"], 2);
    assert!(doc["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["name"] == "serve"));
    let batch = doc["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "batch")
        .unwrap();
    assert!(batch["flags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["name"] == "--work-columns"));
}
