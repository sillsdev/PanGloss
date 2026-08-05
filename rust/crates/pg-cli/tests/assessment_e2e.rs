//! End-to-end gate for the grammar-assessment evidence layer (`add-grammar-assessment` unit 7).
//!
//! Drives the real `pangloss` binary over a real conformance grammar and asserts the properties the
//! artifact contract rests on — the ones a unit test on a hand-built fixture cannot reach, because
//! they involve an actual compiled model, an actual foma network, and an actual process exit code.
//!
//! Synthetic data only: the grammar is `machine/conformance/edge-cases/deep-optional-affix-nesting`,
//! whose all-optional 12-slot prefix chain yields exactly `C(12,k)` analyses for a word with `k`
//! leading `x`s. That known-by-construction count is the arithmetic this file checks against, so a
//! projection bug cannot hide behind "whatever the parser said".

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn repo_root() -> PathBuf {
    // `crates/pg-cli` -> `crates` -> `rust` -> repo root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root above crates/pg-cli")
        .to_path_buf()
}

fn grammar() -> PathBuf {
    repo_root()
        .join("machine/conformance/edge-cases/deep-optional-affix-nesting/grammar.xml")
        .to_path_buf()
}

fn binary() -> PathBuf {
    // The integration-test executable lives beside the binary under test.
    let mut dir = std::env::current_exe().expect("test executable path");
    dir.pop();
    if dir.ends_with("deps") {
        dir.pop();
    }
    dir.join(format!("pangloss{}", std::env::consts::EXE_SUFFIX))
}

struct Workspace {
    dir: PathBuf,
}

impl Workspace {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("pg-assessment-e2e-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create workspace");
        Workspace { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.path(name);
        std::fs::write(&path, contents).expect("write fixture");
        path
    }

    fn read_json(&self, name: &str) -> Value {
        let text = std::fs::read_to_string(self.path(name)).expect("read artifact");
        serde_json::from_str(&text).expect("artifact is JSON")
    }
}

/// Run the binary, returning its exit code.
fn run(args: &[&str]) -> i32 {
    Command::new(binary())
        .args(args)
        .output()
        .expect("spawn pangloss")
        .status
        .code()
        .expect("process exited with a code")
}

fn run_ok(args: &[&str]) {
    let output = Command::new(binary())
        .args(args)
        .output()
        .expect("spawn pangloss");
    assert!(
        output.status.success(),
        "pangloss {args:?} failed ({:?}): {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn skip_if_unbuilt() -> bool {
    if binary().exists() {
        return false;
    }
    eprintln!(
        "skipping: {} is not built; run the managed build for pg-cli first",
        binary().display()
    );
    true
}

fn analyses(report: &Value, case_id: &str) -> usize {
    report["cases"]
        .as_array()
        .expect("cases array")
        .iter()
        .find(|c| c["caseId"] == case_id)
        .unwrap_or_else(|| panic!("case {case_id} missing"))["analyses"]
        .as_array()
        .expect("a complete case carries analyses")
        .len()
}

#[test]
fn an_assessment_is_reproducible_and_only_its_report_id_moves() {
    if skip_if_unbuilt() {
        return;
    }
    let ws = Workspace::new("reproducible");
    let words = ws.write("words.txt", "k\nxk\nxxk\n");
    let grammar = grammar();
    let (g, w) = (grammar.to_str().unwrap(), words.to_str().unwrap());

    run_ok(&[
        "assess",
        g,
        "--words",
        w,
        "--report",
        ws.path("a.json").to_str().unwrap(),
    ]);
    run_ok(&[
        "assess",
        g,
        "--words",
        w,
        "--report",
        ws.path("b.json").to_str().unwrap(),
    ]);
    let (a, b) = (ws.read_json("a.json"), ws.read_json("b.json"));

    assert_eq!(a["schema"], "pangloss.assessment-report");
    assert_eq!(a["status"], "complete");
    assert_eq!(a["reproducible"], true);
    assert_eq!(
        a["semanticDigest"], b["semanticDigest"],
        "identical inputs must reproduce the semantic digest"
    );
    assert_eq!(a["outcomeDigest"], b["outcomeDigest"]);

    // The grammar's own combinatorics: k leading `x`s yield exactly C(12,k) distinct analyses. A
    // projection that collapsed distinct morpheme chains would show fewer.
    assert_eq!(analyses(&a, "w0:k"), 1);
    assert_eq!(analyses(&a, "w1:xk"), 12);
    assert_eq!(analyses(&a, "w2:xxk"), 66);
}

#[test]
fn both_pipelines_agree_on_complete_cases() {
    if skip_if_unbuilt() {
        return;
    }
    let ws = Workspace::new("pipelines");
    let words = ws.write("words.txt", "k\nxk\nxxk\n");
    let grammar = grammar();
    let (g, w) = (grammar.to_str().unwrap(), words.to_str().unwrap());

    run_ok(&[
        "assess",
        g,
        "--words",
        w,
        "--report",
        ws.path("foma.json").to_str().unwrap(),
    ]);
    run_ok(&[
        "assess",
        g,
        "--words",
        w,
        "--pipeline",
        "hermitcrab",
        "--report",
        ws.path("hc.json").to_str().unwrap(),
    ]);
    let (foma, hc) = (ws.read_json("foma.json"), ws.read_json("hc.json"));

    assert_eq!(foma["execution"]["pipeline"], "foma-confirm", "the default");
    assert_eq!(hc["execution"]["pipeline"], "hermitcrab");
    // The propose-and-confirm invariant, observed at the artifact level: the two pipelines must
    // produce equal analysis sets on complete cases, so their outcome digests must match.
    assert_eq!(
        foma["outcomeDigest"], hc["outcomeDigest"],
        "the pipelines disagree about what the grammar does"
    );
    // ...while the semantic digest correctly records that different work was done.
    assert_ne!(foma["semanticDigest"], hc["semanticDigest"]);
}

#[test]
fn compare_reports_a_dropped_case_as_one_sided_rather_than_changed() {
    if skip_if_unbuilt() {
        return;
    }
    let ws = Workspace::new("compare");
    let grammar = grammar();
    let g = grammar.to_str().unwrap();
    let all = ws.write("all.txt", "k\nxk\nxxk\n");
    let fewer = ws.write("fewer.txt", "k\nxk\n");

    run_ok(&[
        "assess",
        g,
        "--words",
        all.to_str().unwrap(),
        "--report",
        ws.path("baseline.json").to_str().unwrap(),
    ]);
    run_ok(&[
        "assess",
        g,
        "--words",
        fewer.to_str().unwrap(),
        "--report",
        ws.path("candidate.json").to_str().unwrap(),
    ]);
    run_ok(&[
        "compare",
        ws.path("baseline.json").to_str().unwrap(),
        ws.path("candidate.json").to_str().unwrap(),
        "--report",
        ws.path("delta.json").to_str().unwrap(),
    ]);

    let delta = ws.read_json("delta.json");
    assert_eq!(delta["schema"], "pangloss.grammar-delta");
    assert_eq!(delta["summary"]["totalCases"], 3);
    assert_eq!(delta["summary"]["byCategory"]["unchanged"], 2);
    assert_eq!(delta["summary"]["byCategory"]["baseline_only"], 1);
    // A case the candidate suite simply does not contain is inventory movement, not a grammar
    // change — forcing investigation on it would generate noise on every suite edit.
    assert_eq!(delta["summary"]["changedCases"], 0);
}

#[test]
fn golden_diff_evaluates_only_adjudicated_complete_cases() {
    if skip_if_unbuilt() {
        return;
    }
    let ws = Workspace::new("golden");
    let grammar = grammar();
    let g = grammar.to_str().unwrap();
    let words = ws.write("words.txt", "k\n");
    run_ok(&[
        "assess",
        g,
        "--words",
        words.to_str().unwrap(),
        "--report",
        ws.path("probe.json").to_str().unwrap(),
    ]);

    // Build a suite from identities the grammar actually produced, expanding the report's own
    // interned key table — the same thing any downstream consumer has to do.
    let probe = ws.read_json("probe.json");
    let keys: Vec<String> = probe["keyTable"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().to_string())
        .collect();
    let expand = |identity: &Value| -> Value {
        serde_json::json!({
            "morphemes": identity["morphemes"].as_array().unwrap().iter().map(|m| match m.as_u64() {
                None => Value::Null,
                Some(index) => Value::String(keys[index as usize].clone()),
            }).collect::<Vec<_>>(),
            "rootIndex": identity["rootIndex"],
            "category": identity["category"].as_u64()
                .map(|i| Value::String(keys[i as usize].clone()))
                .unwrap_or(Value::Null),
        })
    };
    let real = expand(&probe["cases"][0]["analyses"][0]["identity"]);
    let unproducible = serde_json::json!({
        "morphemes": ["no-such-morpheme"], "rootIndex": 0, "category": Value::Null
    });

    let suite = serde_json::json!({
        "schema": "pangloss.assessment-suite",
        "schemaVersion": 1,
        "suiteId": "e2e",
        "suiteRevision": "r1",
        "analysisIdentityProfile": "pangloss.machine-word-analysis/v1",
        "cases": [
            { "caseId": "agrees", "input": "k",
              "expectation": { "status": "adjudicated", "closedWorld": true, "required": [real] } },
            { "caseId": "disagrees", "input": "k",
              "expectation": { "status": "adjudicated", "required": [unproducible] } },
            { "caseId": "unruled", "input": "k",
              "expectation": { "status": "unresolved" } },
        ]
    });
    let suite_path = ws.write("suite.json", &serde_json::to_string_pretty(&suite).unwrap());

    run_ok(&[
        "assess",
        g,
        "--suite",
        suite_path.to_str().unwrap(),
        "--report",
        ws.path("suited.json").to_str().unwrap(),
    ]);
    run_ok(&[
        "golden-diff",
        ws.path("suited.json").to_str().unwrap(),
        "--suite",
        suite_path.to_str().unwrap(),
        "--report",
        ws.path("golden.json").to_str().unwrap(),
    ]);

    let golden = ws.read_json("golden.json");
    assert_eq!(golden["schema"], "pangloss.golden-set-diff");
    assert_eq!(golden["summary"]["agrees"], 1);
    assert_eq!(golden["summary"]["disagrees"], 1);
    assert_eq!(golden["summary"]["notAdjudicated"], 1);
    // Every aggregate carries its denominator.
    assert_eq!(golden["summary"]["totalCases"], 3);
    assert_eq!(golden["summary"]["adjudicatedAndEvaluable"], 2);
    // Three cases share the surface form "k" and stay distinct — a word-keyed report cannot do this.
    assert_eq!(golden["cases"].as_array().unwrap().len(), 3);
}

#[test]
fn investigate_binds_evidence_to_the_run_that_produced_it() {
    if skip_if_unbuilt() {
        return;
    }
    let ws = Workspace::new("investigate");
    let grammar = grammar();
    let words = ws.write("words.txt", "xk\n");
    run_ok(&[
        "assess",
        grammar.to_str().unwrap(),
        "--words",
        words.to_str().unwrap(),
        "--report",
        ws.path("report.json").to_str().unwrap(),
    ]);
    run_ok(&[
        "investigate",
        ws.path("report.json").to_str().unwrap(),
        "--case",
        "w0:xk",
        "--report",
        ws.path("handoff.json").to_str().unwrap(),
    ]);

    let report = ws.read_json("report.json");
    let handoff = ws.read_json("handoff.json");
    assert_eq!(handoff["schema"], "pangloss.investigation-handoff");
    assert_eq!(handoff["binding"]["reportId"], report["reportId"]);
    assert_eq!(
        handoff["binding"]["modelFingerprint"],
        report["provenance"]["modelFingerprint"]
    );
    assert_eq!(handoff["binding"]["caseId"], "w0:xk");
    // Absent evidence is labelled, not omitted.
    assert_eq!(handoff["evidence"]["availability"], "unavailable");
    assert!(handoff["caveat"]["text"]
        .as_str()
        .unwrap()
        .contains("not necessarily a grammar defect"));
}

#[test]
fn setup_failure_produces_a_failed_artifact_rather_than_an_error_exit() {
    if skip_if_unbuilt() {
        return;
    }
    let ws = Workspace::new("setup-failure");
    let broken = ws.write("broken.xml", "<Not-A-Grammar/>");
    let words = ws.write("words.txt", "k\nxk\n");

    // A caller that asked for evidence gets evidence: exiting non-zero with nothing to
    // read would tell CI only that something went wrong, and `compare` could not join the run.
    run_ok(&[
        "assess",
        broken.to_str().unwrap(),
        "--words",
        words.to_str().unwrap(),
        "--report",
        ws.path("failed.json").to_str().unwrap(),
    ]);

    let failed = ws.read_json("failed.json");
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["diagnostics"][0]["code"], "assessment.setup_failed");
    for case in failed["cases"].as_array().unwrap() {
        assert_eq!(case["outcome"], "not_attempted");
        assert_eq!(case["notAttempted"]["kind"], "assessmentSetupFailed");
        assert!(
            case.get("analyses").is_none(),
            "a case that never ran must not carry an analysis set"
        );
    }
}

#[test]
fn a_report_is_published_atomically_and_leaves_no_debris() {
    if skip_if_unbuilt() {
        return;
    }
    let ws = Workspace::new("atomic-write");
    let grammar = grammar();
    let g = grammar.to_str().unwrap();
    let words = ws.write("words.txt", "k\nxk\n");
    let report = ws.path("report.json");

    // Pre-existing content that must be replaced wholesale, never partially overwritten. It is
    // much longer than the real artifact, so a writer that truncated in place and re-streamed could
    // leave a tail of it behind. The sentinel is a string the artifact cannot contain, so this
    // cannot pass or fail by accident on the fixture's own vocabulary.
    const SENTINEL: &str = "PREVIOUS-REPORT-CONTENT-THAT-MUST-NOT-SURVIVE";
    std::fs::write(&report, SENTINEL.repeat(5_000)).unwrap();

    run_ok(&[
        "assess",
        g,
        "--words",
        words.to_str().unwrap(),
        "--report",
        report.to_str().unwrap(),
    ]);

    let written = std::fs::read_to_string(&report).expect("read published report");
    let parsed: Value = serde_json::from_str(&written).expect("published report is complete JSON");
    assert_eq!(parsed["schema"], "pangloss.assessment-report");
    assert!(
        !written.contains(SENTINEL),
        "the previous file's content survived, so the write was not a replacement"
    );

    // The temp sibling is an implementation detail that must not outlive the run: a leftover
    // `.report.json.<pid>.tmp` would accumulate in the caller's directory on every invocation.
    let debris: Vec<_> = std::fs::read_dir(&ws.dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(debris.is_empty(), "temp files left behind: {debris:?}");
}

#[test]
fn exit_codes_are_typed() {
    if skip_if_unbuilt() {
        return;
    }
    let ws = Workspace::new("exit-codes");
    let grammar = grammar();
    let g = grammar.to_str().unwrap();
    let words = ws.write("words.txt", "k\n");
    run_ok(&[
        "assess",
        g,
        "--words",
        words.to_str().unwrap(),
        "--report",
        ws.path("report.json").to_str().unwrap(),
    ]);
    let report_path = ws.path("report.json");
    let report = report_path.to_str().unwrap();

    assert_eq!(
        run(&[
            "assess",
            g,
            "--words",
            words.to_str().unwrap(),
            "--pipeline",
            "xample"
        ]),
        2,
        "an unknown pipeline is invalid input, never a silent fallback"
    );
    assert_eq!(run(&["assess", g]), 2, "no suite and no word list");
    assert_eq!(
        run(&["compare", ws.path("absent.json").to_str().unwrap(), report]),
        2
    );
    assert_eq!(run(&["investigate", report, "--case", "ghost"]), 2);

    let junk = ws.write("junk.json", "not json at all");
    assert_eq!(run(&["compare", junk.to_str().unwrap(), report]), 2);

    // A report from another identity profile is well formed; this build just cannot read its
    // encoding, so it is an unsupported capability rather than invalid input.
    let mut foreign = ws.read_json("report.json");
    foreign["suite"]["analysisIdentityProfile"] =
        Value::String("pangloss.machine-word-analysis/v2".into());
    let foreign_path = ws.write("foreign.json", &serde_json::to_string(&foreign).unwrap());
    assert_eq!(run(&["compare", foreign_path.to_str().unwrap(), report]), 3);
}
