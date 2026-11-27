//! `parse --rule-costs`, the `serve` `ruleCosts` field and trace-details v6: the per-object `work_steps` rows of one word, on a grammar whose alpha deletion rule feeds itself so a low work cap stops the word inside that rule (#390).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");
const WORD: &str = "aaaaaaaaaaaa";
const CAP: &str = "100";

fn grammar() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/rule-costs-alpha-self-feed.xml")
        .to_string_lossy()
        .into_owned()
}

/// Runs `pangloss parse` and returns (exit success, stdout lines).
fn parse(extra: &[&str]) -> (bool, Vec<String>) {
    let output = Command::new(BIN)
        .args(["parse", &grammar(), WORD, "--work-cap", CAP])
        .args(extra)
        .output()
        .expect("pangloss parse must run");
    let stdout = String::from_utf8(output.stdout).unwrap();
    (
        output.status.success(),
        stdout.lines().map(str::to_owned).collect(),
    )
}

fn costs_of(lines: &[String]) -> Value {
    let line = lines
        .iter()
        .find_map(|l| l.strip_prefix("ruleCosts:\t"))
        .unwrap_or_else(|| panic!("no ruleCosts line in {lines:?}"));
    serde_json::from_str(line).unwrap()
}

/// Wall-clock self time is the one field that differs between runs.
fn without_timing(mut costs: Value) -> Value {
    for object in costs["objects"].as_array_mut().unwrap() {
        object["selfElapsedNs"] = Value::Null;
    }
    costs
}

fn assert_stopped_in_the_alpha_rule(costs: &Value) {
    assert_eq!(costs["stopped"], true, "{costs}");
    let objects = costs["objects"].as_array().unwrap();
    assert!(!objects.is_empty(), "{costs}");
    let top = &objects[0];
    assert_eq!(top["kind"], "phonRule", "{costs}");
    assert_eq!(top["label"], "alpha-vowel-deletion", "{costs}");
    let steps: Vec<u64> = objects
        .iter()
        .map(|o| o["work_steps"].as_u64().unwrap())
        .collect();
    assert!(steps.windows(2).all(|w| w[0] >= w[1]), "sorted: {steps:?}");
    let attributed: u64 = costs["attributedWorkSteps"].as_u64().unwrap();
    let unattributed: u64 = costs["unattributedWorkSteps"].as_u64().unwrap();
    assert_eq!(
        attributed + unattributed,
        costs["workSteps"].as_u64().unwrap()
    );
    assert!(
        top["work_steps"].as_u64().unwrap() * 2 > costs["workSteps"].as_u64().unwrap(),
        "the rule that stopped the word holds most of the budget: {costs}"
    );
}

#[test]
fn rule_costs_name_the_alpha_rule_that_spent_the_budget_of_a_stopped_word() {
    let (ok, lines) = parse(&["--rule-costs"]);
    assert!(!ok, "a capped word exits non-zero: {lines:?}");
    assert_stopped_in_the_alpha_rule(&costs_of(&lines));
}

#[test]
fn rule_costs_take_a_row_count_and_refuse_a_bad_one() {
    let (_, lines) = parse(&["--rule-costs=1"]);
    let costs = costs_of(&lines);
    assert_eq!(costs["topN"], 1);
    assert_eq!(costs["objects"].as_array().unwrap().len(), 1, "{costs}");
    for bad in ["--rule-costs=0", "--rule-costs=x"] {
        let output = Command::new(BIN)
            .args(["parse", &grammar(), WORD, bad])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{bad}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("--rule-costs"));
    }
}

#[test]
fn rule_costs_do_not_change_the_parse_or_its_work() {
    let (_, plain) = parse(&[]);
    let (_, with) = parse(&["--rule-costs"]);
    let without_costs: Vec<&String> = with
        .iter()
        .filter(|l| !l.starts_with("ruleCosts:"))
        .collect();
    assert_eq!(plain.iter().collect::<Vec<_>>(), without_costs);
    // The same work: the costs' total is the bare parse's work, read from serve below.
    let again = costs_of(&parse(&["--rule-costs"]).1);
    assert_eq!(without_timing(costs_of(&with)), without_timing(again));
}

fn serve(requests: &[Value]) -> Vec<Value> {
    let mut child = Command::new(BIN)
        .args(["serve", "--project", &grammar(), "--work-cap", CAP])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut read = || {
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str::<Value>(&line).unwrap()
    };
    assert_eq!(read()["type"], "ready");
    let mut out = Vec::new();
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
        out.push(read());
    }
    drop(stdin);
    child.wait().unwrap();
    out
}

#[test]
fn serve_parse_returns_rule_costs_only_when_asked_and_leaves_work_steps_alone() {
    let parse_req = |id: &str, extra: Value| {
        let mut r = json!({"id": id, "op": "parse", "word": WORD});
        for (k, v) in extra.as_object().unwrap() {
            r[k] = v.clone();
        }
        r
    };
    let responses = serve(&[
        parse_req("plain", json!({})),
        parse_req("costs", json!({"ruleCosts": true})),
        parse_req("two", json!({"ruleCosts": 2})),
        parse_req("traced", json!({"ruleCosts": true, "ruleTrace": true})),
        parse_req("details", json!({"traceDetails": true, "ruleCosts": true})),
        parse_req("plaindetails", json!({"traceDetails": true})),
        parse_req("zero", json!({"ruleCosts": 0})),
        parse_req("text", json!({"ruleCosts": "yes"})),
    ]);
    let [plain, costs, two, traced, details, plain_details, zero, text] = &responses[..] else {
        panic!("{responses:?}")
    };
    assert_eq!(plain["status"], "CAP", "{plain}");
    assert!(plain.get("ruleCosts").is_none(), "{plain}");
    assert_eq!(costs["status"], plain["status"]);
    assert_eq!(costs["workSteps"], plain["workSteps"]);
    assert_eq!(costs["result"], plain["result"]);
    assert_stopped_in_the_alpha_rule(&costs["ruleCosts"]);
    assert_eq!(costs["ruleCosts"]["workSteps"], plain["workSteps"]);
    assert_eq!(two["ruleCosts"]["objects"].as_array().unwrap().len(), 2);
    assert_eq!(traced["ruleCosts"]["workSteps"], traced["workSteps"]);
    let doc = &details["result"];
    assert_eq!(doc["schemaVersion"], "pangloss.trace-details.v6");
    assert_stopped_in_the_alpha_rule(&doc["ruleCosts"]);
    assert_eq!(
        doc["ruleCosts"]["workSteps"], doc["search"]["workSteps"],
        "{doc}"
    );
    let default_doc = &plain_details["result"];
    assert_eq!(default_doc["schemaVersion"], "pangloss.trace-details.v5");
    assert_eq!(
        default_doc["provenance"]["parser"]["traceProfile"],
        "pangloss.trace-details.v5"
    );
    assert!(default_doc.get("ruleCosts").is_none(), "{default_doc}");
    assert!(plain_details.get("ruleCosts").is_none());
    for bad in [zero, text] {
        assert_eq!(bad["type"], "error", "{bad}");
        assert_eq!(bad["code"], "bad_request", "{bad}");
    }
}

#[test]
fn describe_declares_rule_costs_for_parse_and_serve() {
    let output = Command::new(BIN).arg("--describe").output().unwrap();
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
    let fields: Vec<&str> = doc["interfaces"]["serveOps"]["parse"]["request"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(fields.contains(&"ruleCosts"), "{fields:?}");
    assert!(doc.to_string().contains("--rule-costs"));
}

fn cli_stderr(extra: &[&str]) -> String {
    let output = Command::new(BIN)
        .args(["parse", &grammar(), WORD])
        .args(extra)
        .output()
        .unwrap();
    assert!(!output.status.success(), "{extra:?} must be refused");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn cli_trace_details_is_v5_unless_rule_costs_are_requested() {
    let details = |extra: &[&str]| -> Value {
        let output = Command::new(BIN)
            .args([
                "parse",
                &grammar(),
                WORD,
                "--trace",
                "--trace-format=json",
                "--trace-details",
            ])
            .args(extra)
            .output()
            .unwrap();
        serde_json::from_slice(&output.stdout).unwrap()
    };
    let default = details(&[]);
    assert_eq!(default["schemaVersion"], "pangloss.trace-details.v5");
    assert!(default.get("ruleCosts").is_none());
    let costs = details(&["--rule-costs=3"]);
    assert_eq!(costs["schemaVersion"], "pangloss.trace-details.v6");
    assert_eq!(costs["ruleCosts"]["topN"], 3);
    assert_eq!(
        costs["ruleCosts"]["workSteps"],
        costs["search"]["workSteps"]
    );
}

#[test]
fn rule_costs_is_refused_with_trace_without_and_a_rejection_trace() {
    for extra in [
        &["--rule-costs", "--trace"][..],
        &[
            "--rule-costs",
            "--without",
            "00000000-0000-0000-0000-000000000000",
        ][..],
        &[
            "--rule-costs",
            "--trace-rejections=rule-costs-refusal.jsonl",
        ][..],
    ] {
        let stderr = cli_stderr(extra);
        assert!(
            stderr.contains("--rule-costs cannot be combined"),
            "{extra:?}: {stderr}"
        );
    }
}
