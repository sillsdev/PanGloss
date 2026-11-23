//! `pangloss serve`'s `generate` op: per-seed allomorphs and rule traces, typed stops, and the `--describe` contract.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");
const FWDATA: &str =
    include_str!("../../../../docs/divergences/evidence/071/kat-fixture-with-k.fwdata");
const ROOT: &str = r#"{"entryGuid":"00000000-0000-0000-0000-000000000040","msaGuid":"00000000-0000-0000-0000-000000000042","msaClass":"MoStemMsa"}"#;
const SNAPSHOT: &str = include_str!("../../../../docs/divergences/evidence/071/kat-snapshot.json");
const SNAPSHOT_ROOT: &str = r#"{"entryGuid":"00000000-0000-0000-0000-000000000030","msaGuid":"00000000-0000-0000-0000-000000000032","msaClass":"MoStemMsa"}"#;
const AFFIX: &str = r#"{"entryGuid":"00000000-0000-0000-0000-000000000050","msaGuid":"00000000-0000-0000-0000-000000000052","msaClass":"MoInflAffMsa"}"#;

struct Project {
    path: std::path::PathBuf,
}

impl Project {
    fn new(name: &str, text: &str) -> Project {
        let dir =
            std::env::temp_dir().join(format!("pangloss-serve-generate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        Project { path }
    }
}

/// The fixture with a second root allomorph, so the seed product is two.
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

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    fn start(project: &Project) -> Server {
        let mut child = Command::new(BIN)
            .args(["serve", "--project", project.path.to_str().unwrap()])
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

    fn generate(&mut self, morphemes: &[&str], extra: Value) -> Value {
        let request: Value = serde_json::from_str(&format!(
            r#"{{"schemaVersion":1,"root":{ROOT},"morphemes":[{}]}}"#,
            morphemes.join(",")
        ))
        .unwrap();
        let mut message = json!({"id": "g", "op": "generate", "request": request});
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

fn surfaces(seed: &Value) -> Vec<String> {
    seed["surfaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["surface"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn trace_names_the_allomorphs_and_each_rule_step() {
    let project = Project::new("kat.fwdata", FWDATA);
    let mut server = Server::start(&project);
    let r = server.generate(&[AFFIX], json!({"trace": true}));
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(r["op"], "generate");
    assert_eq!(r["id"], "g");
    assert_eq!(r["complete"], true, "{r}");
    assert_eq!(r["stopReason"], Value::Null);
    let seeds = r["seeds"].as_array().unwrap();
    assert_eq!(seeds.len(), 1, "{r}");
    let seed = &seeds[0];
    assert_eq!(seed["seedKey"], "r0/p0");
    assert_eq!(surfaces(seed), ["rannas"]);
    let allomorphs = seed["allomorphs"].as_array().unwrap();
    assert_eq!(allomorphs[0]["role"], "root");
    assert_eq!(
        allomorphs[0]["entryGuid"],
        "00000000-0000-0000-0000-000000000040"
    );
    assert_eq!(allomorphs[1]["role"], "rule");
    assert_eq!(
        allomorphs[1]["msaGuid"],
        "00000000-0000-0000-0000-000000000052"
    );
    let steps = seed["surfaces"][0]["steps"].as_array().unwrap();
    assert!(!steps.is_empty(), "{r}");
    for step in steps {
        for key in ["stratum", "rule", "input", "output", "subrule"] {
            assert!(step.get(key).is_some(), "step lacks {key}: {step}");
        }
    }
    assert_eq!(
        steps.last().unwrap()["output"]
            .as_str()
            .unwrap()
            .replace('+', ""),
        "rannas"
    );
}

#[test]
fn a_phonological_rule_appears_as_a_step_with_its_input_and_output() {
    let project = Project::new("kat.json", SNAPSHOT);
    let mut server = Server::start(&project);
    let request: Value = serde_json::from_str(&format!(
        r#"{{"schemaVersion":1,"root":{SNAPSHOT_ROOT},"morphemes":[]}}"#
    ))
    .unwrap();
    let r = server.ask(json!({"id": "p", "op": "generate", "request": request, "trace": true}));
    assert_eq!(r["complete"], true, "{r}");
    let surface = &r["seeds"][0]["surfaces"][0];
    assert_eq!(surface["surface"], "kad", "{r}");
    let steps = surface["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1, "{r}");
    assert_eq!(steps[0]["rule"]["kind"], "phon");
    assert_eq!(steps[0]["rule"]["name"], "Voicing");
    assert_eq!(steps[0]["input"], "kat");
    assert_eq!(steps[0]["output"], "kad");
}

#[test]
fn trace_off_returns_surfaces_without_steps() {
    let project = Project::new("kat.fwdata", FWDATA);
    let mut server = Server::start(&project);
    let r = server.generate(&[AFFIX], json!({"trace": false}));
    assert_eq!(r["complete"], true, "{r}");
    assert_eq!(surfaces(&r["seeds"][0]), ["rannas"]);
    assert!(r["seeds"][0]["surfaces"][0].get("steps").is_none());
    assert_eq!(r["traceSteps"], 0);
}

#[test]
fn one_seed_per_allomorph_choice_sorted_and_deterministic() {
    let project = Project::new("kat2.fwdata", &two_allomorph_fwdata());
    let mut server = Server::start(&project);
    let first = server.generate(&[AFFIX], json!({"trace": true}));
    let second = server.generate(&[AFFIX], json!({"trace": true}));
    assert_eq!(first, second, "repeat requests must match exactly");
    assert_eq!(first["complete"], true, "{first}");
    let seeds = first["seeds"].as_array().unwrap();
    assert_eq!(seeds.len(), 2, "{first}");
    assert_eq!(seeds[0]["seedKey"], "r0/p0");
    assert_eq!(seeds[1]["seedKey"], "r1/p0");
    assert_ne!(
        seeds[0]["allomorphs"][0]["allomorphId"],
        seeds[1]["allomorphs"][0]["allomorphId"]
    );
    for seed in seeds {
        let names = surfaces(seed);
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "surfaces are sorted");
    }
}

#[test]
fn seed_limit_returns_the_seeds_found_so_far() {
    let project = Project::new("kat2.fwdata", &two_allomorph_fwdata());
    let mut server = Server::start(&project);
    let r = server.generate(&[AFFIX], json!({"maxSeeds": 1}));
    assert_eq!(r["complete"], false, "{r}");
    assert_eq!(r["stopReason"], "seed_limit");
    assert_eq!(r["seeds"].as_array().unwrap().len(), 1);
}

#[test]
fn seed_product_over_the_work_limit_is_refused_before_enumeration() {
    let project = Project::new("kat2.fwdata", &two_allomorph_fwdata());
    let mut server = Server::start(&project);
    let r = server.generate(&[AFFIX], json!({"maxWork": 1}));
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(r["complete"], false);
    assert_eq!(r["stopReason"], "seed_product_exceeds_budget");
    assert!(r["seeds"].as_array().unwrap().is_empty());
}

#[test]
fn work_limit_stops_with_the_partial_result() {
    let project = Project::new("kat2.fwdata", &two_allomorph_fwdata());
    let mut server = Server::start(&project);
    // The seed product (2) fits, but synthesis needs more than two work units.
    let r = server.generate(&[AFFIX], json!({"maxWork": 2}));
    assert_eq!(r["complete"], false, "{r}");
    assert_eq!(r["stopReason"], "work_limit");
}

#[test]
fn deadline_stops_with_a_typed_reason() {
    let project = Project::new("kat.fwdata", FWDATA);
    let mut server = Server::start(&project);
    let r = server.generate(&[AFFIX], json!({"timeoutMs": 0}));
    assert_eq!(r["complete"], false, "{r}");
    assert_eq!(r["stopReason"], "deadline");
}

#[test]
fn trace_limit_marks_the_seed_truncated_and_stops() {
    let project = Project::new("kat2.fwdata", &two_allomorph_fwdata());
    let mut server = Server::start(&project);
    let r = server.generate(&[AFFIX], json!({"trace": true, "maxTraceSteps": 2}));
    assert_eq!(r["complete"], false, "{r}");
    assert_eq!(r["stopReason"], "trace_limit");
    assert_eq!(r["seeds"][0]["traceTruncated"], true);
    assert!(r["traceSteps"].as_u64().unwrap() <= 2, "{r}");
}

#[test]
fn a_grammar_without_authored_identity_is_refused() {
    let project = Project::new("mini.xml", include_str!("data/serve-generate-hc.xml"));
    let mut server = Server::start(&project);
    let r = server.generate(&[], json!({}));
    assert_eq!(r["type"], "error", "{r}");
    assert_eq!(r["code"], "authored_identity_unavailable");
}

#[test]
fn bad_requests_never_run() {
    let project = Project::new("kat.fwdata", FWDATA);
    let mut server = Server::start(&project);
    let r = server.ask(json!({"id": "a", "op": "generate"}));
    assert_eq!(r["code"], "bad_request", "{r}");
    let r = server.ask(json!({"id": "b", "op": "generate", "request": {"schemaVersion": 1}}));
    assert_eq!(r["code"], "bad_request", "{r}");
    let r = server.generate(&[], json!({"maxTraceSteps": 0}));
    assert_eq!(r["code"], "bad_request", "{r}");
    let r = server.generate(&[], json!({"surprise": 1}));
    assert_eq!(r["code"], "bad_request", "{r}");
    let unknown = ROOT.replace("000000000040", "0000000000ff");
    let request = json!({"schemaVersion": 1, "root": serde_json::from_str::<Value>(&unknown).unwrap(), "morphemes": []});
    let r = server.ask(json!({"id": "c", "op": "generate", "request": request}));
    assert_eq!(r["code"], "unknown_entry", "{r}");
    let r = server.generate(&[], json!({}));
    assert_eq!(r["type"], "result", "the server survives bad requests: {r}");
}

#[test]
fn describe_matches_what_the_op_emits() {
    let output = Command::new(BIN).arg("--describe").output().unwrap();
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
    let op = &doc["interfaces"]["serveOps"]["generate"];
    assert_eq!(op["version"], 1);
    assert_eq!(op["trace"], true);
    assert_eq!(op["requestSchema"], 1);

    let project = Project::new("kat2.fwdata", &two_allomorph_fwdata());
    let mut server = Server::start(&project);
    let r = server.generate(&[AFFIX], json!({"trace": true}));
    let keys = |value: &Value| -> Vec<String> {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    let declared = |field: &str| -> Vec<String> {
        let mut names: Vec<String> = op[field]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        names.sort();
        names
    };
    let mut result_keys = keys(&r);
    result_keys.retain(|k| !["id", "type", "op"].contains(&k.as_str()));
    assert_eq!(result_keys, declared("resultFields"));
    let seed = r["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|seed| !seed["surfaces"].as_array().unwrap().is_empty())
        .expect("a seed with a surface");
    assert_eq!(keys(seed), declared("seedFields"));
    let surface = &seed["surfaces"][0];
    assert_eq!(keys(surface), declared("surfaceFields"));
    assert_eq!(keys(&surface["steps"][0]), declared("stepFields"));

    let limits = &r["limits"];
    assert_eq!(limits["generationCap"], op["limits"]["generationCap"]);
    assert_eq!(limits["maxSeeds"], op["limits"]["maxSeeds"]["default"]);
    assert_eq!(
        limits["maxTraceSteps"],
        op["limits"]["maxTraceSteps"]["default"]
    );

    let stops = declared("stopReasons");
    let emitted: Vec<Value> = vec![
        server.generate(&[AFFIX], json!({"maxSeeds": 1})),
        server.generate(&[AFFIX], json!({"maxWork": 1})),
        server.generate(&[AFFIX], json!({"maxWork": 2})),
        server.generate(&[AFFIX], json!({"timeoutMs": 0})),
        server.generate(&[AFFIX], json!({"trace": true, "maxTraceSteps": 2})),
    ];
    let mut seen: Vec<String> = emitted
        .iter()
        .map(|r| {
            r["stopReason"]
                .as_str()
                .expect("every cap names its reason")
                .to_string()
        })
        .collect();
    seen.sort();
    assert_eq!(
        seen, stops,
        "describe lists exactly the stop reasons the op emits"
    );

    let errors = declared("errorCodes");
    let r = server.ask(json!({"id": "x", "op": "generate"}));
    assert!(
        errors.contains(&r["code"].as_str().unwrap().to_string()),
        "{r}"
    );
}
