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
    _dir: tempfile::TempDir,
}

impl Project {
    /// Each project gets its own directory, so tests sharing a file name never overwrite one another's grammar.
    fn new(name: &str, text: &str) -> Project {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        Project { path, _dir: dir }
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

/// The kat snapshot with a suffix of the given allomorph forms, each allowed after a vowel so every one matches `ranna`.
fn suffix_snapshot(forms: &[&str]) -> String {
    let mut doc: Value = serde_json::from_str(SNAPSHOT).unwrap();
    let allomorphs: Vec<Value> = forms
        .iter()
        .enumerate()
        .map(|(i, form)| {
            json!({
                "guid": format!("00000000-0000-0000-0000-0000000006{i}0"),
                "morphType": "suffix",
                "isAbstract": false,
                "forms": [{"ws": "fx", "form": form}],
                "environments": ["00000000-0000-0000-0000-000000000017"],
            })
        })
        .collect();
    let template = doc["lexicon"]["entries"][2].clone();
    let mut entry = template;
    entry["guid"] = json!("00000000-0000-0000-0000-000000000060");
    entry["allomorphs"] = Value::Array(allomorphs);
    entry["msas"][0]["guid"] = json!("00000000-0000-0000-0000-000000000062");
    entry["senses"][0]["guid"] = json!("00000000-0000-0000-0000-000000000063");
    entry["senses"][0]["msa"] = json!("00000000-0000-0000-0000-000000000062");
    doc["lexicon"]["entries"]
        .as_array_mut()
        .unwrap()
        .push(entry);
    doc.to_string()
}

const SUFFIX_RANNA: &str = r#"{"schemaVersion":1,"root":{"entryGuid":"00000000-0000-0000-0000-000000000040","msaGuid":"00000000-0000-0000-0000-000000000042","msaClass":"MoStemMsa"},"morphemes":[{"entryGuid":"00000000-0000-0000-0000-000000000060","msaGuid":"00000000-0000-0000-0000-000000000062","msaClass":"MoInflAffMsa"}]}"#;

fn generate_suffix(forms: &[&str], extra: Value) -> Value {
    let project = Project::new("suffix.json", &suffix_snapshot(forms));
    let mut server = Server::start(&project);
    let request: Value = serde_json::from_str(SUFFIX_RANNA).unwrap();
    let mut message = json!({"id": "s", "op": "generate", "request": request, "trace": true});
    for (key, value) in extra.as_object().unwrap() {
        message[key] = value.clone();
    }
    server.ask(message)
}

/// A derivation must start at the seed, chain input to output, and end at the surface.
fn assert_chain(steps: &Value, seed: &str, surface: &str) {
    let steps = steps.as_array().unwrap();
    assert!(!steps.is_empty());
    assert_eq!(steps[0]["input"], seed, "{steps:?}");
    for pair in steps.windows(2) {
        assert_eq!(pair[0]["output"], pair[1]["input"], "{steps:?}");
    }
    let last = steps.last().unwrap()["output"].as_str().unwrap();
    assert_eq!(last.replace('+', ""), surface, "{steps:?}");
}

#[test]
fn two_allomorphs_neutralised_to_one_surface_report_both_derivations_not_a_guess() {
    let r = generate_suffix(&["t", "d"], json!({}));
    assert_eq!(r["complete"], true, "{r}");
    let seeds = r["seeds"].as_array().unwrap();
    let surfaces: Vec<&Value> = seeds
        .iter()
        .flat_map(|seed| seed["surfaces"].as_array().unwrap())
        .collect();
    assert_eq!(surfaces.len(), 1, "{r}");
    let surface = surfaces[0];
    assert_eq!(surface["surface"], "rannad", "{r}");
    assert_eq!(surface["derivations"], 2, "{r}");
    assert_eq!(
        surface["steps"],
        json!([]),
        "several derivations give no single steps: {r}"
    );
    assert_eq!(surface["stepsUnresolved"], false, "{r}");
    let alternatives = surface["alternatives"].as_array().unwrap();
    assert_eq!(alternatives.len(), 2, "{r}");
    let mut kinds: Vec<Vec<String>> = alternatives
        .iter()
        .map(|steps| {
            assert_chain(steps, "ranna", "rannad");
            steps
                .as_array()
                .unwrap()
                .iter()
                .map(|step| step["rule"]["kind"].as_str().unwrap().to_string())
                .collect()
        })
        .collect();
    kinds.sort();
    assert_eq!(kinds, [vec!["morph"], vec!["morph", "phon"]], "{r}");
    for steps in alternatives {
        let morph = &steps[0];
        let expected = if steps.as_array().unwrap().len() == 2 {
            "ranna+t"
        } else {
            "ranna+d"
        };
        assert_eq!(morph["output"], expected, "{r}");
    }
}

#[test]
fn sibling_branches_keep_their_own_steps() {
    let r = generate_suffix(&["t", "s"], json!({}));
    assert_eq!(r["complete"], true, "{r}");
    let mut by_surface = std::collections::BTreeMap::new();
    for seed in r["seeds"].as_array().unwrap() {
        for surface in seed["surfaces"].as_array().unwrap() {
            by_surface.insert(
                surface["surface"].as_str().unwrap().to_string(),
                surface.clone(),
            );
        }
    }
    assert_eq!(
        by_surface.keys().cloned().collect::<Vec<_>>(),
        ["rannad", "rannas"],
        "{r}"
    );
    for (surface, value) in &by_surface {
        assert_eq!(value["derivations"], 1, "{r}");
        assert_eq!(value["stepsUnresolved"], false, "{r}");
        assert_chain(&value["steps"], "ranna", surface);
    }
    assert_eq!(
        by_surface["rannad"]["steps"].as_array().unwrap().len(),
        2,
        "{r}"
    );
    assert_eq!(
        by_surface["rannas"]["steps"].as_array().unwrap().len(),
        1,
        "{r}"
    );
}

#[test]
fn a_truncated_trace_reports_its_surfaces_without_guessing_steps() {
    let full = generate_suffix(&["t"], json!({}));
    let nodes = full["traceSteps"].as_u64().unwrap();
    assert!(nodes > 3, "{full}");
    for cap in 1..nodes {
        let r = generate_suffix(&["t"], json!({"maxTraceSteps": cap}));
        for seed in r["seeds"].as_array().unwrap() {
            assert_eq!(seed["traceTruncated"], true, "cap {cap}: {r}");
            for surface in seed["surfaces"].as_array().unwrap() {
                assert_eq!(surface["stepsUnresolved"], true, "cap {cap}: {r}");
                assert_eq!(surface["steps"], json!([]), "cap {cap}: {r}");
            }
        }
    }
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
    let r = server.generate(&[AFFIX], json!({"wordTimeoutMs": 0}));
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
fn trace_gives_each_morpheme_its_surface_spans() {
    let project = Project::new("kat.fwdata", FWDATA);
    let mut server = Server::start(&project);
    let r = server.generate(&[AFFIX], json!({"trace": true}));
    let surface = &r["seeds"][0]["surfaces"][0];
    assert_eq!(surface["surface"], "rannas", "{r}");
    assert_eq!(surface["normalized"], Value::Null);
    let analyses = surface["morphSpans"].as_array().unwrap();
    assert_eq!(analyses.len(), 1, "{r}");
    assert_eq!(analyses[0]["derivations"], 1);
    assert_eq!(analyses[0]["segmentCount"], 6);
    assert_eq!(analyses[0]["unattributed"], json!([]));
    assert_eq!(
        analyses[0]["morphemes"],
        json!([
            {"item": 0, "spans": [{"segments": [0, 5], "offsets": [0, 5]}], "reason": null, "rule": null},
            {"item": 1, "spans": [{"segments": [5, 6], "offsets": [5, 6]}], "reason": null, "rule": null},
        ]),
        "{r}"
    );
    let r = server.generate(&[AFFIX], json!({}));
    let plain = &r["seeds"][0]["surfaces"][0];
    assert!(
        plain.get("morphSpans").is_none(),
        "spans are a trace field: {r}"
    );
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
    let r = server.generate(
        &[AFFIX],
        json!({"trace": true, "attestedWords": ["rannas"]}),
    );
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
    let analysis = &surface["morphSpans"][0];
    assert_eq!(keys(analysis), declared("morphSpanFields"));
    for morpheme in analysis["morphemes"].as_array().unwrap() {
        assert_eq!(keys(morpheme), declared("morphemeSpanFields"));
        for span in morpheme["spans"].as_array().unwrap() {
            assert_eq!(keys(span), declared("spanFields"));
        }
    }
    assert_eq!(keys(&surface["steps"][0]["rule"]), declared("ruleFields"));
    for allomorph in seed["allomorphs"].as_array().unwrap() {
        assert_eq!(keys(allomorph), declared("allomorphFields"));
    }

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
        server.generate(&[AFFIX], json!({"wordTimeoutMs": 0})),
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

/// One `generate` request line, as `pangloss serve` and `generate --requests` both take it.
fn batch_line(id: &str, morphemes: &[&str], extra: Value) -> String {
    let request: Value = serde_json::from_str(&format!(
        r#"{{"schemaVersion":1,"root":{ROOT},"morphemes":[{}]}}"#,
        morphemes.join(",")
    ))
    .unwrap();
    let mut message = json!({"id": id, "op": "generate", "request": request});
    for (key, value) in extra.as_object().unwrap() {
        message[key] = value.clone();
    }
    message.to_string()
}

/// Runs `pangloss generate <project> --requests <source>`, feeding `lines` through a file or, for `-`, stdin; returns the exit success and the parsed output rows.
fn run_requests(project: &Project, lines: &[String], via_stdin: bool) -> (bool, Vec<Value>) {
    let text = lines.join("\n") + "\n";
    let mut command = Command::new(BIN);
    command.arg("generate").arg(&project.path).arg("--requests");
    let dir = tempfile::tempdir().unwrap();
    let output = if via_stdin {
        let mut child = command
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("pangloss generate must start");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    } else {
        let path = dir.path().join("requests.jsonl");
        std::fs::write(&path, text).unwrap();
        command.arg(&path).output().unwrap()
    };
    let stdout = String::from_utf8(output.stdout).unwrap();
    let rows = stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l}")))
        .collect();
    (output.status.success(), rows)
}

#[test]
fn requests_batch_answers_every_line_in_order_and_a_bad_line_is_an_error_row() {
    let project = Project::new("kat.fwdata", FWDATA);
    let lines = [
        batch_line("first", &[AFFIX], json!({})),
        "{not json".to_string(),
        batch_line("third", &[], json!({})),
    ];
    let (ok, rows) = run_requests(&project, &lines, false);
    assert!(ok, "a bad line must not fail the batch: {rows:?}");
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert_eq!(rows[0]["id"], "first");
    let numbers: Vec<&Value> = rows.iter().map(|r| &r["line"]).collect();
    assert_eq!(numbers, [1, 2, 3], "{rows:?}");
    assert_eq!(rows[0]["type"], "result", "{}", rows[0]);
    assert_eq!(surfaces(&rows[0]["seeds"][0]), ["rannas"]);
    assert_eq!(rows[1]["type"], "error", "{}", rows[1]);
    assert_eq!(rows[1]["code"], "malformed_json");
    assert_eq!(rows[1]["id"], Value::Null);
    assert_eq!(rows[2]["id"], "third");
    assert_eq!(rows[2]["type"], "result", "{}", rows[2]);
}

#[test]
fn requests_batch_row_equals_the_serve_op_answer_for_the_same_request() {
    let project = Project::new("kat.fwdata", FWDATA);
    let traced = batch_line("t", &[AFFIX], json!({"trace": true}));
    let plain = batch_line("p", &[], json!({"maxSeeds": 5}));
    let (_, rows) = run_requests(&project, &[traced.clone(), plain.clone()], false);
    let mut server = Server::start(&project);
    let expected_traced = server.ask(serde_json::from_str(&traced).unwrap());
    let expected_plain = server.ask(serde_json::from_str(&plain).unwrap());
    let without_line: Vec<Value> = rows
        .into_iter()
        .map(|mut row| {
            row.as_object_mut().unwrap().remove("line").unwrap();
            row
        })
        .collect();
    assert_eq!(without_line, [expected_traced, expected_plain]);
}

#[test]
fn requests_batch_reads_stdin_for_a_dash() {
    let project = Project::new("kat.fwdata", FWDATA);
    let lines = [batch_line("in", &[AFFIX], json!({}))];
    let (ok, rows) = run_requests(&project, &lines, true);
    assert!(ok);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["id"], "in");
    assert_eq!(surfaces(&rows[0]["seeds"][0]), ["rannas"]);
}

#[test]
fn requests_batch_keeps_refusing_form_selection_and_other_ops_per_row() {
    let project = Project::new("kat.fwdata", FWDATA);
    let forced = AFFIX.replace(
        r#""msaClass""#,
        r#""formGuid":"00000000-0000-0000-0000-000000000051","msaClass""#,
    );
    let lines = [
        batch_line("forced", &[&forced], json!({})),
        r#"{"id":"w","op":"parse","word":"x"}"#.to_string(),
        String::new(),
        batch_line("after", &[], json!({})),
    ];
    let (ok, rows) = run_requests(&project, &lines, false);
    assert!(ok);
    assert_eq!(rows.len(), 3, "a blank line has no row: {rows:?}");
    let numbers: Vec<&Value> = rows.iter().map(|r| &r["line"]).collect();
    assert_eq!(numbers, [1, 2, 4], "line counts the blank line: {rows:?}");
    assert_eq!(rows[0]["code"], "unsupported_form_selection", "{}", rows[0]);
    assert_eq!(rows[1]["code"], "unknown_op", "{}", rows[1]);
    assert_eq!(rows[1]["id"], "w");
    assert_eq!(rows[2]["type"], "result");
}

/// Runs `generate --requests` over raw bytes through a file.
fn run_raw_requests(project: &Project, bytes: &[u8]) -> Vec<Value> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("requests.jsonl");
    std::fs::write(&path, bytes).unwrap();
    let output = Command::new(BIN)
        .arg("generate")
        .arg(&project.path)
        .arg("--requests")
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn requests_batch_gives_an_error_row_for_an_oversize_and_a_non_utf8_line() {
    let project = Project::new("kat.fwdata", FWDATA);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(batch_line("a", &[], json!({})).as_bytes());
    bytes.push(b'\n');
    bytes.extend_from_slice(format!("{{\"id\":\"{}\"}}\n", "x".repeat(1 << 20)).as_bytes());
    bytes.extend_from_slice(b"{\"id\":\"\xff\"}\n");
    bytes.extend_from_slice(b"  \t\n");
    bytes.extend_from_slice(batch_line("e", &[], json!({})).as_bytes());
    bytes.push(b'\n');
    let rows = run_raw_requests(&project, &bytes);
    assert_eq!(rows.len(), 4, "{rows:?}");
    assert_eq!(rows[0]["line"], 1);
    assert_eq!(rows[1]["code"], "request_too_large", "{}", rows[1]);
    assert_eq!(rows[1]["line"], 2);
    assert_eq!(rows[2]["code"], "malformed_json", "{}", rows[2]);
    assert_eq!(rows[2]["line"], 3);
    assert_eq!(rows[3]["id"], "e");
    assert_eq!(rows[3]["line"], 5);
}

fn seed_form_guids(result: &Value) -> Vec<(String, Value)> {
    result["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|seed| {
            (
                seed["seedKey"].as_str().unwrap().to_string(),
                seed["allomorphs"][0]["formGuid"].clone(),
            )
        })
        .collect()
}

/// `ROOT` with a `formGuid` added.
fn root_with_form(form: &str) -> String {
    ROOT.replace(
        r#""msaClass""#,
        &format!(r#""formGuid":"{form}","msaClass""#),
    )
}

fn generate_with_root(server: &mut Server, root: &str, extra: Value) -> Value {
    let request: Value = serde_json::from_str(&format!(
        r#"{{"schemaVersion":1,"root":{root},"morphemes":[{AFFIX}]}}"#
    ))
    .unwrap();
    let mut message = json!({"id": "f", "op": "generate", "request": request});
    for (key, value) in extra.as_object().unwrap() {
        message[key] = value.clone();
    }
    server.ask(message)
}

#[test]
fn a_root_form_guid_runs_only_that_forms_seeds_and_the_forms_together_equal_the_unforced_run() {
    let project = Project::new("kat2.fwdata", &two_allomorph_fwdata());
    let mut server = Server::start(&project);
    let all = server.generate(&[AFFIX], json!({}));
    let forms = seed_form_guids(&all);
    assert_eq!(forms.len(), 2, "{all}");
    let mut union: Vec<Value> = Vec::new();
    for (key, form) in &forms {
        let form = form.as_str().expect("the fixture authors each root form");
        let forced = generate_with_root(&mut server, &root_with_form(form), json!({}));
        assert_eq!(forced["complete"], true, "{forced}");
        let seeds = forced["seeds"].as_array().unwrap();
        assert_eq!(seeds.len(), 1, "only the forced form's seed runs: {forced}");
        assert_eq!(
            seeds[0]["seedKey"], *key,
            "a forced seed keeps its unforced key"
        );
        assert_eq!(forced["seedChoices"], 1);
        let unforced = all["seeds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|seed| seed["seedKey"] == *key)
            .unwrap();
        assert_eq!(
            seeds[0], *unforced,
            "a forced seed equals the unforced seed"
        );
        union.push(seeds[0].clone());
    }
    let mut expected: Vec<Value> = all["seeds"].as_array().unwrap().clone();
    union.sort_by_key(|seed| seed["seedKey"].as_str().unwrap().to_string());
    expected.sort_by_key(|seed| seed["seedKey"].as_str().unwrap().to_string());
    assert_eq!(
        union, expected,
        "the forms together give the unforced seeds, keys and surfaces"
    );
}

#[test]
fn an_unknown_root_form_guid_is_unknown_form_and_an_affix_form_stays_refused() {
    let project = Project::new("kat.fwdata", FWDATA);
    let mut server = Server::start(&project);
    let stranger = generate_with_root(
        &mut server,
        &root_with_form("00000000-0000-0000-0000-0000000000ee"),
        json!({}),
    );
    assert_eq!(stranger["type"], "error", "{stranger}");
    assert_eq!(stranger["code"], "unknown_form", "{stranger}");
    let affix_form = AFFIX.replace(
        r#""msaClass""#,
        r#""formGuid":"00000000-0000-0000-0000-000000000051","msaClass""#,
    );
    let refused = server.generate(&[&affix_form], json!({}));
    assert_eq!(refused["code"], "unsupported_form_selection", "{refused}");
}

#[test]
fn attested_marks_each_surface_against_an_inline_list_and_adds_nothing_without_one() {
    let project = Project::new("kat.fwdata", FWDATA);
    let mut server = Server::start(&project);
    let plain = server.generate(&[AFFIX], json!({}));
    assert!(
        plain["seeds"][0]["surfaces"][0].get("attested").is_none(),
        "no list, no attested field: {plain}"
    );
    let yes = server.generate(&[AFFIX], json!({"attestedWords": ["rannas", "other"]}));
    assert_eq!(yes["seeds"][0]["surfaces"][0]["attested"], true, "{yes}");
    let no = server.generate(&[AFFIX], json!({"attestedWords": ["other"]}));
    assert_eq!(no["seeds"][0]["surfaces"][0]["attested"], false, "{no}");
    let mut stripped = yes.clone();
    for seed in stripped["seeds"].as_array_mut().unwrap() {
        for surface in seed["surfaces"].as_array_mut().unwrap() {
            surface.as_object_mut().unwrap().remove("attested");
        }
    }
    assert_eq!(stripped, plain, "attested is the only difference");
}

#[test]
fn attested_reads_a_file_and_refuses_both_sources_or_a_bad_type() {
    let project = Project::new("kat.fwdata", FWDATA);
    let mut server = Server::start(&project);
    let dir = tempfile::tempdir().unwrap();
    let list = dir.path().join("words.txt");
    std::fs::write(&list, "\nother\nrannas\n").unwrap();
    let r = server.generate(
        &[AFFIX],
        json!({"attestedWordsFile": list.to_str().unwrap()}),
    );
    assert_eq!(r["seeds"][0]["surfaces"][0]["attested"], true, "{r}");

    let both = server.generate(
        &[AFFIX],
        json!({"attestedWords": [], "attestedWordsFile": list.to_str().unwrap()}),
    );
    assert_eq!(both["code"], "bad_request", "{both}");
    let typed = server.generate(&[AFFIX], json!({"attestedWords": [1]}));
    assert_eq!(typed["code"], "bad_request", "{typed}");
    let missing = server.generate(
        &[AFFIX],
        json!({"attestedWordsFile": dir.path().join("none.txt").to_str().unwrap()}),
    );
    assert_eq!(missing["code"], "bad_request", "{missing}");
}

#[test]
fn requests_batch_attested_flag_marks_rows_and_a_row_list_overrides_it() {
    let project = Project::new("kat.fwdata", FWDATA);
    let dir = tempfile::tempdir().unwrap();
    let list = dir.path().join("words.txt");
    std::fs::write(&list, "rannas\n").unwrap();
    let requests = dir.path().join("requests.jsonl");
    std::fs::write(
        &requests,
        [
            batch_line("a", &[AFFIX], json!({})),
            batch_line("b", &[AFFIX], json!({"attestedWords": ["zzz"]})),
        ]
        .join("\n")
            + "\n",
    )
    .unwrap();
    let output = Command::new(BIN)
        .arg("generate")
        .arg(&project.path)
        .arg("--requests")
        .arg(&requests)
        .arg("--attested")
        .arg(&list)
        .output()
        .unwrap();
    assert!(output.status.success());
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        rows[0]["seeds"][0]["surfaces"][0]["attested"], true,
        "{}",
        rows[0]
    );
    assert_eq!(
        rows[1]["seeds"][0]["surfaces"][0]["attested"], false,
        "{}",
        rows[1]
    );

    let missing = Command::new(BIN)
        .arg("generate")
        .arg(&project.path)
        .arg("--requests")
        .arg(&requests)
        .arg("--attested")
        .arg(dir.path().join("none.txt"))
        .output()
        .unwrap();
    assert!(!missing.status.success(), "an unreadable list is exit 1");
    let single = Command::new(BIN)
        .arg("generate")
        .arg(&project.path)
        .arg("--attested")
        .arg(&list)
        .output()
        .unwrap();
    assert!(!single.status.success());
}
