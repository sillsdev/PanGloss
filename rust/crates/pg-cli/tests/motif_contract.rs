//! Runs each Motif invocation in `tests/contract/motif/*.json` against the shipped binary and compares its output shape (see that directory's README).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");
/// Setting this to a reason rewrites every `expect` and appends the reason and fixture digest to `CHANGES.md`.
const REGENERATE: &str = "MOTIF_CONTRACT_REGENERATE";
/// String values under these keys are part of the contract, not just their type.
const PINNED_STRING_KEYS: &[&str] = &["schema", "type", "op", "binary"];

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn contract_dir() -> PathBuf {
    crate_dir().join("tests/contract/motif")
}

fn evidence(name: &str) -> PathBuf {
    crate_dir()
        .join("../../../docs/divergences/evidence/071")
        .join(name)
}

fn scenario_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(contract_dir())
        .expect("contract fixture directory")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
}

fn shape(value: &Value, key: Option<&str>) -> Value {
    match value {
        Value::Null => json!("null"),
        Value::Bool(_) => json!("boolean"),
        Value::Number(n) => {
            if key.is_some_and(|k| k == "protocol" || k.to_ascii_lowercase().contains("version")) {
                value.clone()
            } else if n.is_i64() || n.is_u64() {
                json!("integer")
            } else {
                json!("number")
            }
        }
        Value::String(s) => {
            if key.is_some_and(|k| PINNED_STRING_KEYS.contains(&k)) {
                Value::String(s.clone())
            } else {
                json!("string")
            }
        }
        Value::Array(items) => {
            let mut merged: Option<Value> = None;
            for item in items {
                let s = shape(item, None);
                merged = Some(match merged {
                    None => s,
                    Some(m) => merge(m, s),
                });
            }
            Value::Array(merged.into_iter().collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), shape(v, Some(k))))
                .collect(),
        ),
    }
}

fn merge(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::Object(mut x), Value::Object(y)) => {
            for (k, v) in y {
                let merged = match x.remove(&k) {
                    Some(old) => merge(old, v),
                    None => v,
                };
                x.insert(k, merged);
            }
            Value::Object(x)
        }
        (Value::Array(x), Value::Array(y)) => {
            let mut all = x.into_iter().chain(y);
            match all.next() {
                None => Value::Array(vec![]),
                Some(first) => Value::Array(vec![all.fold(first, merge)]),
            }
        }
        (Value::String(x), Value::String(y)) if x == y => Value::String(x),
        (Value::String(x), Value::String(y)) => {
            let mut parts: Vec<&str> = x.split('|').chain(y.split('|')).collect();
            parts.sort_unstable();
            parts.dedup();
            Value::String(parts.join("|"))
        }
        (x, y) if x == y => x,
        _ => json!("mixed"),
    }
}

/// Every key, element and value of `expected` is present in `observed`; `observed` may carry more.
fn contains(expected: &Value, observed: &Value, path: &str, problems: &mut Vec<String>) {
    match (expected, observed) {
        (Value::Object(e), Value::Object(o)) => {
            for (k, ev) in e {
                match o.get(k) {
                    Some(ov) => contains(ev, ov, &format!("{path}/{k}"), problems),
                    None => problems.push(format!(
                        "{path}/{k}: missing (a key Motif reads was removed or renamed)"
                    )),
                }
            }
        }
        (Value::Array(e), Value::Array(o)) => {
            for ev in e {
                let mut best: Option<Vec<String>> = None;
                let ok = o.iter().any(|ov| {
                    let mut p = Vec::new();
                    contains(ev, ov, &format!("{path}[]"), &mut p);
                    let ok = p.is_empty();
                    if !ok && best.as_ref().is_none_or(|b| p.len() < b.len()) {
                        best = Some(p);
                    }
                    ok
                });
                if !ok {
                    match best {
                        Some(p) => problems.extend(p),
                        None => problems.push(format!("{path}[]: expected an element, found none")),
                    }
                }
            }
        }
        (e, o) if e == o => {}
        (e, o) => problems.push(format!("{path}: expected {e}, found {o}")),
    }
}

struct Workspace {
    dir: tempfile::TempDir,
}

impl Workspace {
    fn new() -> Workspace {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace { dir };
        let copy = |from: PathBuf, to: &str| {
            std::fs::copy(&from, ws.dir.path().join(to))
                .unwrap_or_else(|e| panic!("copy {}: {e}", from.display()));
        };
        copy(evidence("kat-snapshot.json"), "kat-snapshot.json");
        copy(evidence("kat-fixture-with-k.fwdata"), "kat.fwdata");
        copy(
            crate_dir().join("tests/data/match-fixture.xml"),
            "match-fixture.xml",
        );
        ws
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn fill(&self, text: &str) -> String {
        let scratch = self.path().to_string_lossy().into_owned();
        text.replace("{scratch}", &scratch)
    }
}

fn read_line(out: &mut BufReader<std::process::ChildStdout>) -> Value {
    let mut line = String::new();
    assert!(
        out.read_line(&mut line).unwrap() > 0,
        "serve closed its output"
    );
    serde_json::from_str(line.trim_end())
        .unwrap_or_else(|e| panic!("serve line is not JSON ({e}): {line}"))
}

/// Masks the elapsed milliseconds and the work-step count of a batch row, which vary with timing and tuning.
fn normalise_tsv(text: &str) -> String {
    text.replace("\r\n", "\n")
        .lines()
        .map(|line| {
            let mut fields: Vec<&str> = line.split('\t').collect();
            if fields.len() >= 5 {
                fields[2] = "<ms>";
            }
            if fields.len() >= 6 {
                fields[5] = "<work>";
            }
            fields.join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn describe_projection(doc: &Value) -> Value {
    // What `PanGlossSurface` reads: schema_version, binary, commands[].name/positionals/hidden/flags[].name/takes_value, interfaces.
    let commands: Vec<Value> = doc["commands"]
        .as_array()
        .expect("describe commands")
        .iter()
        .map(|c| {
            let mut out = Map::new();
            out.insert("name".into(), c["name"].clone());
            out.insert("positionals".into(), c["positionals"].clone());
            if let Some(h) = c.get("hidden") {
                out.insert("hidden".into(), h.clone());
            }
            let flags: Vec<Value> = c["flags"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| json!({"name": f["name"], "takes_value": f["takes_value"]}))
                .collect();
            out.insert("flags".into(), Value::Array(flags));
            Value::Object(out)
        })
        .collect();
    // Motif never reads the retired serve ops `health` and `shadowedAllomorphs`, so they are not pinned.
    let mut interfaces = doc["interfaces"].clone();
    if let Some(ops) = interfaces
        .get_mut("serveOps")
        .and_then(Value::as_object_mut)
    {
        ops.remove("health");
        ops.remove("shadowedAllomorphs");
    }
    json!({
        "schema_version": doc["schema_version"],
        "binary": doc["binary"],
        "commands": commands,
        "interfaces": interfaces,
    })
}

/// Reads one captured file or stream the way Motif reads it, as the value an `expect` records.
fn capture(kind: &str, bytes: &[u8]) -> Value {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_start_matches('\u{feff}');
    match kind {
        "json" => shape(
            &serde_json::from_str(text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}")),
            None,
        ),
        "describe" => shape_exact_describe(&serde_json::from_str(text).unwrap()),
        "jsonl" => {
            // Lines are grouped by their `type` (or `schema`), so a stream with several kinds of line pins each.
            let mut groups: BTreeMap<String, Value> = BTreeMap::new();
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let v: Value =
                    serde_json::from_str(line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"));
                let key = v["type"]
                    .as_str()
                    .or_else(|| v["schema"].as_str())
                    .unwrap_or("-")
                    .to_string();
                let s = shape(&v, None);
                let merged = match groups.remove(&key) {
                    Some(old) => merge(old, s),
                    None => s,
                };
                groups.insert(key, merged);
            }
            Value::Object(groups.into_iter().collect())
        }
        "tsv" => json!({ "tsv": normalise_tsv(text) }),
        "text" => json!({ "nonEmpty": !text.trim().is_empty() }),
        "exists" => json!({ "nonEmpty": !bytes.is_empty() }),
        other => panic!("unknown capture kind {other}"),
    }
}

/// `--describe` pins values (interface versions, flag spellings) as well as keys, restricted to what Motif reads.
fn shape_exact_describe(doc: &Value) -> Value {
    describe_projection(doc)
}

fn run_step(step: &Value, ws: &Workspace) -> Value {
    if let Some(files) = step.get("writeFiles").and_then(Value::as_object) {
        for (name, text) in files {
            std::fs::write(ws.path().join(name), ws.fill(text.as_str().unwrap())).unwrap();
        }
    }
    let argv: Vec<String> = step["argv"]
        .as_array()
        .expect("argv")
        .iter()
        .map(|a| ws.fill(a.as_str().unwrap()))
        .collect();
    let mut out = Map::new();
    if let Some(requests) = step.get("serveRequests").and_then(Value::as_array) {
        let mut child = Command::new(BIN)
            .args(&argv)
            .current_dir(ws.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("pangloss serve must start");
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut replies = Map::new();
        replies.insert("ready".into(), shape(&read_line(&mut stdout), None));
        for request in requests {
            let name = request["name"].as_str().unwrap();
            let line = request["line"].as_str().unwrap();
            stdin.write_all(ws.fill(line).as_bytes()).unwrap();
            stdin.write_all(b"\n").unwrap();
            stdin.flush().unwrap();
            replies.insert(name.into(), shape(&read_line(&mut stdout), None));
        }
        drop(stdin);
        child.wait().unwrap();
        out.insert("replies".into(), Value::Object(replies));
    } else {
        let result = Command::new(BIN)
            .args(&argv)
            .current_dir(ws.path())
            .output()
            .expect("run pangloss");
        let exit = result.status.code().unwrap_or(-1);
        let accepted: Vec<i64> = step["exits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_i64().unwrap())
            .collect();
        assert!(
            accepted.contains(&i64::from(exit)),
            "step {}: exit {exit} is not one Motif accepts {accepted:?}; stderr:\n{}",
            step["name"],
            String::from_utf8_lossy(&result.stderr)
        );
        if let Some(kind) = step.get("stdout").and_then(Value::as_str) {
            out.insert("stdout".into(), capture(kind, &result.stdout));
        }
        if let Some(files) = step.get("files").and_then(Value::as_object) {
            let mut captured = Map::new();
            for (name, kind) in files {
                let bytes = std::fs::read(ws.path().join(name))
                    .unwrap_or_else(|e| panic!("step {} wrote no {name}: {e}", step["name"]));
                captured.insert(name.clone(), capture(kind.as_str().unwrap(), &bytes));
            }
            out.insert("files".into(), Value::Object(captured));
        }
    }
    Value::Object(out)
}

fn fixture_digest() -> String {
    let mut hasher = Sha256::new();
    for file in scenario_files() {
        hasher.update(file.file_name().unwrap().to_string_lossy().as_bytes());
        hasher.update([0]);
        let text = std::fs::read_to_string(&file)
            .unwrap()
            .replace("\r\n", "\n");
        hasher.update(text.as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn load(file: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(file).unwrap())
        .unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

fn regenerate(reason: &str) {
    for file in scenario_files() {
        let mut doc = load(&file);
        let ws = Workspace::new();
        for step in doc["steps"].as_array_mut().unwrap() {
            let observed = run_step(step, &ws);
            step["expect"] = observed;
        }
        let mut text = serde_json::to_string_pretty(&doc).unwrap();
        text.push('\n');
        std::fs::write(&file, text).unwrap();
    }
    let ledger = contract_dir().join("CHANGES.md");
    let mut text = std::fs::read_to_string(&ledger).unwrap();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!(
        "- {reason} (fixtures sha256 {})\n",
        fixture_digest()
    ));
    std::fs::write(&ledger, text).unwrap();
}

#[test]
fn motif_contract_fixtures_match_the_shipped_binary() {
    if let Ok(reason) = std::env::var(REGENERATE) {
        assert!(
            !reason.trim().is_empty(),
            "{REGENERATE} must say why the contract changes (and name the changes/unreleased fragment)"
        );
        regenerate(reason.trim());
        return;
    }
    let mut problems = Vec::new();
    for file in scenario_files() {
        let doc = load(&file);
        let scenario = file.file_stem().unwrap().to_string_lossy().into_owned();
        let ws = Workspace::new();
        for step in doc["steps"].as_array().unwrap() {
            let observed = run_step(step, &ws);
            let mut found = Vec::new();
            contains(&step["expect"], &observed, "", &mut found);
            for f in found {
                problems.push(format!(
                    "{scenario}/{}: {f}",
                    step["name"].as_str().unwrap()
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "pangloss no longer produces what Motif reads ({} differences). If the change is intended, it is a \
         contract change: follow tests/contract/motif/README.md.\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn fixtures_were_changed_only_through_the_regenerate_command() {
    let ledger = std::fs::read_to_string(contract_dir().join("CHANGES.md")).unwrap();
    let last = ledger
        .lines()
        .rev()
        .find(|l| l.starts_with("- "))
        .expect("CHANGES.md records at least the initial fixtures");
    let digest = fixture_digest();
    assert!(
        last.contains(&format!("sha256 {digest}")),
        "the fixtures under tests/contract/motif/ do not match the last CHANGES.md entry (digest {digest}). \
         Edit them only by running `{REGENERATE}=\"<reason>\" pg.ps1 -Mode test -Package pg-cli -TestTarget motif_contract`, \
         which records the reason; see tests/contract/motif/README.md."
    );
}

#[test]
fn every_scenario_step_names_its_motif_source_and_accepted_exits() {
    for file in scenario_files() {
        let doc = load(&file);
        for step in doc["steps"].as_array().unwrap() {
            let name = step["name"].as_str().expect("step name");
            assert!(
                step["motifSource"]
                    .as_str()
                    .is_some_and(|s| s.contains(".cs")),
                "{name}: motifSource must name the Motif file and member that builds this request"
            );
            let argv = step["argv"].as_array().expect("argv");
            assert!(!argv.is_empty(), "{name}: empty argv");
            if step.get("serveRequests").is_none() {
                assert!(
                    step["exits"].as_array().is_some_and(|e| !e.is_empty()),
                    "{name}: exits"
                );
            }
        }
    }
}
