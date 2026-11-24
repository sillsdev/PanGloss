//! `pangloss serve` op `inventory`: the natural classes and phonological features of the loaded grammar, and the spelling `match` accepts for each class.

use std::collections::BTreeSet;
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
        let project =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/match-fixture.xml");
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
        assert_eq!(server.read_line()["type"], "ready");
        server
    }

    fn read_line(&mut self) -> Value {
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
        self.read_line()
    }

    fn inventory(&mut self) -> Value {
        self.ask(json!({"id": "i", "op": "inventory"}))
    }

    /// Tokens are checked against the members a class lists, so the exact test is asked for; the engine test also takes the segments a class's feature union admits.
    fn matching(&mut self, pattern: &str, words: &[&str]) -> Value {
        self.ask(json!({"id": "m", "op": "match", "pattern": pattern, "words": words, "segmentTest": "identity"}))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

fn class<'a>(r: &'a Value, id: &str) -> &'a Value {
    r["classes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("no class {id}: {r}"))
}

#[test]
fn lists_every_class_with_token_name_id_and_kind_and_every_feature_with_its_values() {
    let mut s = Server::start();
    let r = s.inventory();
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(r["op"], "inventory");
    assert_eq!(r["complete"], true);
    assert!(r["stopReason"].is_null());
    assert_eq!(
        class(&r, "ncV"),
        &json!({"token": "V", "name": "V", "fullName": null, "id": "ncV", "kind": "segment"})
    );
    assert_eq!(
        class(&r, "ncVoiced"),
        &json!({"token": "Voiced", "name": "Voiced", "fullName": null, "id": "ncVoiced", "kind": "feature"})
    );
    assert_eq!(
        r["features"],
        json!([
            {"name": "voice", "id": "featVoice", "values": ["+", "-"]},
            {"name": "son", "id": "featSon", "values": ["+", "-"]},
            {"name": "tone", "id": "featTone", "values": ["hi", "lo"]},
        ]),
        "the engine's own Type feature is not listed"
    );
}

/// The letters of the fixture word each class selects, written down independently of the engine: a class whose token resolved to another class would select other letters.
const WORD: &str = "pbtdkgaiunŋ";
const SELECTS: [(&str, &str); 14] = [
    ("ncV", "aiu"),
    ("ncStop", "pbtdkg"),
    ("ncVoiced", "bdgaiunŋ"),
    ("ncV2", "ptk"),
    ("ncW", "ptk"),
    ("W", "bdgaiunŋ"),
    ("ncAnon", "ptk"),
    ("ncPlus", "ptk"),
    ("ncNas", "nŋ"),
    ("ncEng", "ŋ"),
    ("ncBr", "p"),
    ("ncOpen", "g"),
    ("no]way", "k"),
    ("__none__", ""),
];

#[test]
fn every_token_selects_the_class_it_was_listed_for_and_a_null_token_reaches_nothing() {
    let mut s = Server::start();
    let r = s.inventory();
    let selects: std::collections::BTreeMap<&str, &str> = SELECTS.into_iter().collect();
    let letters: Vec<char> = WORD.chars().collect();
    let mut seen = BTreeSet::new();
    let mut nulls = 0;
    for c in r["classes"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        let expected = selects
            .get(id)
            .unwrap_or_else(|| panic!("class {id} needs an entry in SELECTS: {c}"));
        match c["token"].as_str() {
            Some(token) => {
                assert!(seen.insert(token.to_string()), "tokens are unique: {token}");
                let m = s.matching(&format!("[{token}]"), &[WORD]);
                assert_eq!(m["type"], "result", "[{token}] must parse: {m}");
                let got: String = m["words"][0]["matches"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| letters[x["segments"][0].as_u64().unwrap() as usize])
                    .collect();
                assert_eq!(&got, expected, "[{token}] must select class {id}: {m}");
            }
            None => {
                nulls += 1;
                // No spelling reaches it: neither its name nor its id parses to a class that selects it.
                for spelling in [c["name"].as_str().unwrap_or(""), id] {
                    let m = s.matching(&format!("[{spelling}]"), &[WORD]);
                    let got: String = if m["type"] == "result" {
                        m["words"][0]["matches"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|x| letters[x["segments"][0].as_u64().unwrap() as usize])
                            .collect()
                    } else {
                        String::new()
                    };
                    assert_ne!(&got, expected, "[{spelling}] must not reach {id}: {m}");
                }
            }
        }
    }
    assert_eq!(nulls, 1, "exactly the unnamed class whose id is `no]way`");
    assert_eq!(
        class(&r, "no]way")["token"],
        Value::Null,
        "{}",
        class(&r, "no]way")
    );
    // Every class listed has an entry, so a class added to the fixture is covered.
    assert_eq!(selects.len() - 1, r["classes"].as_array().unwrap().len());
}

#[test]
fn spaces_non_ascii_and_brackets_in_names_follow_the_token_rules() {
    let mut s = Server::start();
    let r = s.inventory();
    assert_eq!(class(&r, "ncNas")["token"], "Nasal Stop");
    assert_eq!(class(&r, "ncEng")["token"], "Ŋ-class");
    assert_eq!(
        class(&r, "ncBr")["token"],
        "ncBr",
        "a name holding ] falls back to the id"
    );
    assert_eq!(class(&r, "ncOpen")["token"], "[Open");
    // Case is exact.
    let m = s.matching("[voiced]", &["a"]);
    assert_eq!(m["code"], "unknown_natural_class", "{m}");
}

#[test]
fn match_takes_the_name_then_the_xml_id_and_the_first_of_equal_names_wins() {
    let mut s = Server::start();
    let r = s.inventory();
    // Name and id are both accepted: `Voiced` is a name, `ncVoiced` is an id.
    let by_name = s.matching("[Voiced]", &["bad"]);
    let by_id = s.matching("[ncVoiced]", &["bad"]);
    assert_eq!(by_name["words"], by_id["words"]);
    assert_eq!(by_name["words"][0]["matches"].as_array().unwrap().len(), 3);
    // Two classes named V: the first (the segment class) owns the name; the second is reached by its id.
    assert_eq!(class(&r, "ncV")["token"], "V");
    assert_eq!(class(&r, "ncV2")["token"], "ncV2");
    let first = s.matching("[V]", &["pa"]);
    assert_eq!(first["words"][0]["matches"][0]["segments"], json!([1, 2]));
    let second = s.matching("[ncV2]", &["pa"]);
    assert_eq!(second["words"][0]["matches"][0]["segments"], json!([0, 1]));
    // A class whose id equals another class's name loses to the name: `[W]` is class ncW, and class `W` is spelled by its own name.
    assert_eq!(class(&r, "ncW")["token"], "W");
    assert_eq!(class(&r, "W")["token"], "Other");
    let w = s.matching("[W]", &["pa"]);
    assert_eq!(
        w["words"][0]["matches"][0]["segments"],
        json!([0, 1]),
        "{w}"
    );
    // An empty name and a name no bracket can hold fall back to the id.
    assert_eq!(class(&r, "ncAnon")["token"], "ncAnon");
    assert_eq!(class(&r, "ncPlus")["token"], "ncPlus");
    assert_eq!(class(&r, "ncPlus")["name"], "+hi");
}

#[test]
fn a_work_limit_returns_a_typed_partial_list_never_a_silent_one() {
    let mut s = Server::start();
    let r = s.ask(json!({"id": "i", "op": "inventory", "maxWork": 3}));
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(r["complete"], false, "{r}");
    assert_eq!(r["stopReason"], "work_limit");
    assert_eq!(r["classes"].as_array().unwrap().len(), 3);
    assert!(r["features"].as_array().unwrap().is_empty());
}

#[test]
fn request_keys_are_checked() {
    let mut s = Server::start();
    for bad in [
        json!({"id": "i", "op": "inventory", "maxWork": 0}),
        json!({"id": "i", "op": "inventory", "wordTimeoutMs": "x"}),
        json!({"id": "i", "op": "inventory", "scope": "all"}),
    ] {
        let r = s.ask(bad.clone());
        assert_eq!(r["code"], "bad_request", "{bad}: {r}");
    }
    let r = s.ask(json!({"id": "i", "op": "inventory", "maxWork": 100000, "wordTimeoutMs": 5000}));
    assert_eq!(r["complete"], true, "{r}");
}

#[test]
fn the_inventory_is_deterministic() {
    let mut s = Server::start();
    let first = s.inventory();
    assert_eq!(first, s.inventory());
    assert_eq!(first, Server::start().inventory());
}

#[test]
fn describe_declares_exactly_what_is_emitted() {
    let output = Command::new(BIN).arg("--describe").output().unwrap();
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
    let decl = &doc["interfaces"]["serveOps"]["inventory"];
    assert_eq!(decl["version"], 1);
    assert_eq!(decl["readOnly"], true);
    let strings = |v: &Value| -> BTreeSet<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect()
    };
    let keys = |v: &Value| -> BTreeSet<String> { v.as_object().unwrap().keys().cloned().collect() };

    let mut s = Server::start();
    let r = s.inventory();
    let kinds: BTreeSet<String> = r["classes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["kind"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(strings(&decl["classKinds"]), kinds);
    for c in r["classes"].as_array().unwrap() {
        assert_eq!(keys(c), strings(&decl["classFields"]), "{c}");
    }
    for f in r["features"].as_array().unwrap() {
        assert_eq!(keys(f), strings(&decl["featureFields"]), "{f}");
    }

    // `work_limit` is elicited by a limit; `deadline` is wall-clock, so a test cannot force it on a listing this small.
    let w = s.ask(json!({"id": "i", "op": "inventory", "maxWork": 1}));
    assert_eq!(w["stopReason"], "work_limit");
    assert_eq!(
        strings(&decl["stopReasons"]),
        BTreeSet::from(["work_limit".to_string(), "deadline".to_string()])
    );

    let mut errors = BTreeSet::new();
    let e = s.ask(json!({"id": "i", "op": "inventory", "bogus": 1}));
    errors.insert(e["code"].as_str().unwrap().to_string());
    // `internal` guards a panic a request cannot reach.
    errors.insert("internal".into());
    assert_eq!(strings(&decl["errors"]), errors);
    assert_eq!(
        strings(&decl["request"]),
        BTreeSet::from(["maxWork".to_string(), "wordTimeoutMs".to_string()])
    );
}

#[test]
fn under_the_default_segment_test_a_token_selects_the_features_union_of_its_class() {
    let mut s = Server::start();
    let r = s.ask(json!({"id": "m", "op": "match", "pattern": "[V]", "words": [WORD]}));
    assert_eq!(r["type"], "result", "{r}");
    let letters: Vec<char> = WORD.chars().collect();
    let got: String = r["words"][0]["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| letters[x["segments"][0].as_u64().unwrap() as usize])
        .collect();
    // `V` lists a, i and u; the default engine test also takes the segments their feature union admits.
    assert_eq!(got, "aiunŋ", "{r}");
    let exact = s.matching("[V]", &[WORD]);
    assert_ne!(exact, r);
}

const KAT_FWDATA: &str =
    include_str!("../../../../docs/divergences/evidence/071/kat-fixture-with-k.fwdata");
const KAT_NAME_BLOCK: &str = "<Name>\n<AUni ws=\"en\">Vowels</AUni>\n</Name>\n<Segments>";

/// Serves `project` (a path to an XML or fwdata file) and returns its `inventory` answer.
fn inventory_of(project: &std::path::Path) -> Value {
    let mut child = Command::new(BIN)
        .args(["serve", "--project"])
        .arg(project)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("pangloss serve must start");
    let mut server = Server {
        stdin: child.stdin.take(),
        stdout: BufReader::new(child.stdout.take().unwrap()),
        child,
    };
    assert_eq!(server.read_line()["type"], "ready");
    server.inventory()
}

fn fwdata_inventory(project: &str) -> Value {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kat.fwdata");
    std::fs::write(&path, project).unwrap();
    inventory_of(&path)
}

fn v_class(r: &Value) -> &Value {
    r["classes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "00000000-0000-0000-0000-000000000016")
        .unwrap_or_else(|| panic!("no class V: {r}"))
}

#[test]
fn a_fieldworks_class_lists_its_name_beside_the_abbreviation_label() {
    let r = fwdata_inventory(KAT_FWDATA);
    let c = v_class(&r);
    assert_eq!(
        c["name"], "V",
        "the existing label stays the Abbreviation: {c}"
    );
    assert_eq!(c["fullName"], "Vowels", "{c}");
    assert_eq!(c["token"], "V", "{c}");
}

#[test]
fn a_fieldworks_class_without_a_name_has_a_null_full_name() {
    assert!(
        KAT_FWDATA.contains(KAT_NAME_BLOCK),
        "the fixture no longer has the Name block"
    );
    let r = fwdata_inventory(&KAT_FWDATA.replace(KAT_NAME_BLOCK, "<Segments>"));
    let c = v_class(&r);
    assert_eq!(c["name"], "V", "{c}");
    assert!(c["fullName"].is_null(), "{c}");
    assert!(
        c.as_object().unwrap().contains_key("fullName"),
        "the key is always present: {c}"
    );
}

#[test]
fn an_xml_loaded_grammar_has_only_its_one_label_so_full_name_is_null() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/match-fixture.xml");
    let r = inventory_of(&project);
    let classes = r["classes"].as_array().unwrap();
    assert!(!classes.is_empty());
    for c in classes {
        assert!(
            c["fullName"].is_null() && c.as_object().unwrap().contains_key("fullName"),
            "{c}"
        );
    }
}
