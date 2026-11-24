//! `pangloss serve` op `effects`: which phonological rules can change a morpheme, on the authored-identity fixture.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");
const FWDATA: &str =
    include_str!("../../../../docs/divergences/evidence/071/kat-fixture-with-k.fwdata");
const ROOT_ENTRY: &str = "00000000-0000-0000-0000-000000000040";
const ROOT_MSA: &str = "00000000-0000-0000-0000-000000000042";
const AFFIX_ENTRY: &str = "00000000-0000-0000-0000-000000000050";
const AFFIX_MSA: &str = "00000000-0000-0000-0000-000000000052";

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    _dir: tempfile::TempDir,
}

impl Server {
    fn start() -> Server {
        Server::start_with(FWDATA)
    }

    fn start_with(project: &str) -> Server {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kat.fwdata");
        std::fs::write(&path, project).unwrap();
        let mut child = Command::new(BIN)
            .args(["serve", "--project"])
            .arg(&path)
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
            _dir: dir,
        };
        let ready = server.read_line();
        assert_eq!(ready["type"], "ready", "{ready}");
        server
    }

    fn read_line(&mut self) -> Value {
        let mut line = String::new();
        assert!(
            self.stdout.read_line(&mut line).unwrap() > 0,
            "server closed stdout"
        );
        serde_json::from_str(line.trim_end()).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    fn ask(&mut self, request: Value) -> Value {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(request.to_string().as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
        self.read_line()
    }

    fn effects(&mut self, morpheme: Value, extra: Value) -> Value {
        let mut message = json!({"id": "e1", "op": "effects", "morpheme": morpheme});
        for (key, value) in extra.as_object().unwrap() {
            message[key] = value.clone();
        }
        self.ask(message)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

/// The fixture with a second root allomorph on the root entry.
fn two_allomorph_fwdata() -> String {
    FWDATA
        .replacen(
            "<AUni ws=\"fx\">xxx</AUni>",
            "<AUni ws=\"fx\">ranni</AUni>",
            1,
        )
        .replacen(
            "guid=\"00000000-0000-0000-0000-00000000abcd\" t=\"r\"",
            "guid=\"d7f713e8-e8cf-11d3-9764-00c04f186933\" t=\"r\"",
            1,
        )
}

fn root() -> Value {
    json!({"entryGuid": ROOT_ENTRY, "msaGuid": ROOT_MSA})
}

fn affix() -> Value {
    json!({"entryGuid": AFFIX_ENTRY, "msaGuid": AFFIX_MSA})
}

fn error_code(response: &Value) -> &str {
    assert_eq!(response["type"], "error", "{response}");
    response["code"].as_str().unwrap()
}

#[test]
fn a_root_lists_its_allomorph_and_every_rule_as_unknown_for_each_edge() {
    let mut s = Server::start();
    let r = s.effects(root(), json!({}));
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(r["op"], "effects");
    assert_eq!(r["id"], "e1");
    assert_eq!(r["complete"], true, "{r}");
    assert_eq!(r["stopReason"], Value::Null);
    assert_eq!(r["morpheme"]["entryGuid"], ROOT_ENTRY);
    assert_eq!(r["morpheme"]["msaGuid"], ROOT_MSA);
    let allomorphs = r["allomorphs"].as_array().unwrap();
    assert_eq!(allomorphs.len(), 1, "{r}");
    assert_eq!(allomorphs[0]["form"], "ranna");
    assert_eq!(allomorphs[0]["segments"], json!(["r", "a", "n", "n", "a"]));
    assert_eq!(allomorphs[0]["formPartial"], false);
    let id = allomorphs[0]["allomorphId"].clone();
    let effects = r["effects"].as_array().unwrap();
    assert_eq!(effects.len(), 2, "one rule, one subrule, two edges: {r}");
    let edges: Vec<&str> = effects
        .iter()
        .map(|e| e["edge"].as_str().unwrap())
        .collect();
    assert_eq!(edges, ["left", "right"]);
    for e in effects {
        assert_eq!(e["rule"]["kind"], "rewrite");
        assert_eq!(e["rule"]["name"], "Voicing");
        assert_eq!(e["rule"]["ordinal"], 0);
        assert!(e["rule"]["stratum"].is_string(), "{e}");
        assert_eq!(e["subrule"], 0);
        assert_eq!(e["allomorphId"], id);
        assert_eq!(e["mayApply"], "unknown");
        assert_eq!(e["reason"], "not_analysed");
    }
}

#[test]
fn an_edge_list_and_an_allomorph_id_narrow_the_scan() {
    let mut s = Server::start();
    let all = s.effects(root(), json!({}));
    let id = all["allomorphs"][0]["allomorphId"].clone();
    let mut morpheme = root();
    morpheme["allomorphId"] = id.clone();
    let r = s.effects(morpheme, json!({"edges": ["right"]}));
    assert_eq!(r["type"], "result", "{r}");
    let effects = r["effects"].as_array().unwrap();
    assert_eq!(effects.len(), 1, "{r}");
    assert_eq!(effects[0]["edge"], "right");
    assert_eq!(effects[0]["allomorphId"], id);
}

#[test]
fn an_affix_form_is_its_inserted_segments() {
    let mut s = Server::start();
    let r = s.effects(affix(), json!({}));
    assert_eq!(r["type"], "result", "{r}");
    let allomorphs = r["allomorphs"].as_array().unwrap();
    assert_eq!(allomorphs.len(), 1, "{r}");
    assert_eq!(
        allomorphs[0]["form"], "+s",
        "the form keeps its morph boundary"
    );
    assert_eq!(allomorphs[0]["segments"], json!(["s"]));
    assert_eq!(
        allomorphs[0]["formPartial"], true,
        "a suffix process copies the stem"
    );
    assert_eq!(r["effects"].as_array().unwrap().len(), 2);
}

#[test]
fn a_work_limit_returns_the_effects_so_far_flagged_partial() {
    let mut s = Server::start_with(&two_allomorph_fwdata());
    let r = s.effects(root(), json!({"maxWork": 1}));
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(r["complete"], false, "{r}");
    assert_eq!(r["stopReason"], "work_limit");
    assert_eq!(r["partial"], true);
    assert_eq!(r["allomorphs"].as_array().unwrap().len(), 2, "{r}");
    assert_eq!(
        r["effects"].as_array().unwrap().len(),
        2,
        "one unit scanned: {r}"
    );
}

#[test]
fn a_short_deadline_names_itself_when_it_stops() {
    let mut s = Server::start();
    let r = s.effects(root(), json!({"wordTimeoutMs": 1, "maxWork": 1000000}));
    assert_eq!(r["type"], "result", "{r}");
    // A one millisecond deadline may or may not fire on a tiny scan; when it stops, it says so.
    if r["complete"] == false {
        assert_eq!(r["stopReason"], "deadline", "{r}");
        assert_eq!(r["partial"], true);
    }
}

#[test]
fn typed_errors_for_requests_that_never_ran() {
    let mut s = Server::start();
    let unknown_entry =
        json!({"entryGuid": "00000000-0000-0000-0000-0000000000ff", "msaGuid": ROOT_MSA});
    assert_eq!(
        error_code(&s.effects(unknown_entry, json!({}))),
        "unknown_morpheme"
    );
    let wrong_msa = json!({"entryGuid": ROOT_ENTRY, "msaGuid": AFFIX_MSA});
    assert_eq!(
        error_code(&s.effects(wrong_msa, json!({}))),
        "unknown_morpheme"
    );
    let mut bad_allomorph = root();
    bad_allomorph["allomorphId"] = json!(4_000_000);
    assert_eq!(
        error_code(&s.effects(bad_allomorph, json!({}))),
        "unknown_morpheme"
    );
    for extra in [
        json!({"edges": ["middle"]}),
        json!({"edges": []}),
        json!({"rootsPerEnvironment": 0}),
        json!({"rootsPerEnvironment": 51}),
        json!({"maxWork": 0}),
        json!({"bogus": 1}),
    ] {
        assert_eq!(
            error_code(&s.effects(root(), extra.clone())),
            "bad_request",
            "{extra}"
        );
    }
    let missing = s.ask(json!({"id": "e2", "op": "effects"}));
    assert_eq!(error_code(&missing), "bad_request");
    let no_guid =
        s.ask(json!({"id": "e3", "op": "effects", "morpheme": {"entryGuid": ROOT_ENTRY}}));
    assert_eq!(error_code(&no_guid), "bad_request");
}

#[test]
fn describe_declares_what_the_op_emits() {
    let out = Command::new(BIN).arg("--describe").output().unwrap();
    let describe: Value = serde_json::from_slice(&out.stdout).unwrap();
    let cap = &describe["interfaces"]["serveOps"]["effects"];
    assert_eq!(cap["version"], 1, "{cap}");
    assert_eq!(cap["limits"]["rootsPerEnvironmentMax"], 50);
    assert_eq!(
        cap["nonLocalCriteria"],
        json!([
            "unboundedQuantifier",
            "alphaAgreement",
            "iterativeSpreading"
        ])
    );
    let mut s = Server::start();
    let r = s.effects(root(), json!({}));
    let declared = |key: &str| -> Vec<String> {
        cap[key]
            .as_array()
            .unwrap_or_else(|| panic!("{key} missing in {cap}"))
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    };
    let effect_fields = declared("effectFields");
    for e in r["effects"].as_array().unwrap() {
        for key in e.as_object().unwrap().keys() {
            assert!(effect_fields.contains(key), "undeclared effect field {key}");
        }
        assert!(declared("mayApply").contains(&e["mayApply"].as_str().unwrap().to_string()));
    }
    let result_fields = declared("resultFields");
    for key in r.as_object().unwrap().keys() {
        if !["id", "type", "op"].contains(&key.as_str()) {
            assert!(result_fields.contains(key), "undeclared result field {key}");
        }
    }
    let codes = declared("errors");
    for code in [
        "unknown_morpheme",
        "ambiguous_identity",
        "authored_identity_unavailable",
        "bad_request",
    ] {
        assert!(codes.contains(&code.to_string()), "{code}");
    }
    assert_eq!(declared("stopReasons"), ["work_limit", "deadline"]);
}

#[test]
fn a_grammar_without_authored_identity_is_a_typed_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("grammar.xml");
    std::fs::write(&path, include_str!("data/match-fixture.xml")).unwrap();
    let mut child = Command::new(BIN)
        .args(["serve", "--project"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let request = json!({"id": "e", "op": "effects", "morpheme": root()});
    writeln!(stdin, "{request}").unwrap();
    line.clear();
    stdout.read_line(&mut line).unwrap();
    let r: Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_eq!(error_code(&r), "authored_identity_unavailable", "{r}");
    drop(stdin);
    let _ = child.wait();
}
