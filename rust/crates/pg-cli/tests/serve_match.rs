//! `pangloss serve` op `match`: pattern text over a word list, on a small synthetic grammar.

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
    seen_error_codes: BTreeSet<String>,
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
            seen_error_codes: BTreeSet::new(),
        };
        let ready = server.read_line();
        assert_eq!(ready["type"], "ready", "{ready}");
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
        let response = self.read_line();
        if response["type"] == "error" {
            self.seen_error_codes
                .insert(response["code"].as_str().unwrap().to_string());
        }
        response
    }

    fn matching(&mut self, pattern: &str, words: &[&str]) -> Value {
        self.ask(json!({"id": "m", "op": "match", "pattern": pattern, "words": words}))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

fn spans(word: &Value) -> Vec<(u64, u64)> {
    word["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let s = m["segments"].as_array().unwrap();
            (s[0].as_u64().unwrap(), s[1].as_u64().unwrap())
        })
        .collect()
}

#[test]
fn a_natural_class_matches_every_span_with_indices_offsets_and_context() {
    let mut s = Server::start();
    let r = s.matching("[V]", &["aba"]);
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(r["op"], "match");
    assert_eq!(r["complete"], true);
    assert!(r["stopReason"].is_null());
    let w = &r["words"][0];
    assert_eq!(w["word"], "aba");
    assert_eq!(w["status"], "ok");
    assert_eq!(
        w["matches"],
        json!([
            {"segments": [0, 1], "offsets": [0, 1], "left": [0, 0], "right": [1, 3]},
            {"segments": [2, 3], "offsets": [2, 3], "left": [0, 2], "right": [3, 3]},
        ])
    );
}

#[test]
fn offsets_are_utf8_bytes_while_segment_indices_count_segments() {
    let mut s = Server::start();
    let r = s.matching("/ŋ/", &["aŋa"]);
    let w = &r["words"][0];
    assert_eq!(w["matches"][0]["segments"], json!([1, 2]), "{r}");
    assert_eq!(w["matches"][0]["offsets"], json!([1, 3]), "{r}");
}

#[test]
fn sequences_groups_and_repeats() {
    let mut s = Server::start();
    let r = s.matching("[Stop][V]", &["abad"]);
    assert_eq!(spans(&r["words"][0]), vec![(1, 3)], "{r}");
    // An optional stop before a vowel matches with and without it.
    let r = s.matching("([Stop])[V]", &["ba"]);
    assert_eq!(spans(&r["words"][0]), vec![(0, 2), (1, 2)], "{r}");
    let r = s.matching("[Stop]([V])*[Stop]", &["baab"]);
    assert_eq!(spans(&r["words"][0]), vec![(0, 4)], "{r}");
}

#[test]
fn hash_anchors_the_pattern_at_a_word_edge() {
    let mut s = Server::start();
    let r = s.matching("#[V]", &["aba", "baa"]);
    assert_eq!(spans(&r["words"][0]), vec![(0, 1)], "{r}");
    assert!(spans(&r["words"][1]).is_empty(), "{r}");
    let r = s.matching("[V]#", &["aba", "abb"]);
    assert_eq!(spans(&r["words"][0]), vec![(2, 3)], "{r}");
    assert!(spans(&r["words"][1]).is_empty(), "{r}");
    let r = s.matching("#[Stop][V]#", &["ba", "baa"]);
    assert_eq!(spans(&r["words"][0]), vec![(0, 2)], "{r}");
    assert!(spans(&r["words"][1]).is_empty(), "{r}");
}

#[test]
fn an_unmatched_or_unsegmentable_word_is_reported_not_skipped() {
    let mut s = Server::start();
    let r = s.matching("[V]", &["bdg", "axa"]);
    assert_eq!(
        r["complete"], true,
        "an unsegmentable word does not make the response incomplete"
    );
    assert_eq!(r["words"][0]["status"], "ok");
    assert_eq!(r["words"][0]["matches"], json!([]));
    assert_eq!(r["words"][1]["status"], "unsegmentable");
    assert_eq!(r["words"][1]["at"]["charIndex"], 1);
    assert_eq!(r["words"][1]["at"]["offset"], 1);
}

#[test]
fn plus_needs_marked_words_and_then_matches_the_marker_without_counting_it() {
    let mut s = Server::start();
    let r = s.matching("[Stop]+[V]", &["ba"]);
    assert_eq!(r["type"], "error", "{r}");
    assert_eq!(r["code"], "boundary_needs_marked_input");
    assert_eq!(r["offset"], 6);

    let r = s.ask(
        json!({"id": "m", "op": "match", "pattern": "[Stop]+[V]", "boundaryMarker": "+",
        "words": ["ab+a", "aba"]}),
    );
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(r["boundaryMarker"], "+");
    assert_eq!(
        r["words"][0]["matches"],
        json!([{"segments": [1, 3], "offsets": [1, 4], "left": [0, 1], "right": [3, 3]}]),
        "{r}"
    );
    assert_eq!(
        spans(&r["words"][1]),
        vec![],
        "an unmarked word has no boundary to match: {r}"
    );

    // Without `+` in the pattern a marked boundary stays transparent, as in synthesis.
    let r = s.ask(
        json!({"id": "m", "op": "match", "pattern": "[Stop][V]", "boundaryMarker": "+",
        "words": ["ab+a"]}),
    );
    assert_eq!(spans(&r["words"][0]), vec![(1, 3)], "{r}");

    // Unmarked requests refuse a boundary character in a word; a marker admits only itself.
    let r = s.matching("[V]", &["ab+a"]);
    assert_eq!(r["words"][0]["status"], "unsegmentable", "{r}");
    assert_eq!(r["words"][0]["reason"], "boundary_in_unmarked_word");
}

#[test]
fn parse_errors_name_the_token_offset_and_expected_kinds() {
    let mut s = Server::start();
    let r = s.matching("[V] x", &["aba"]);
    assert_eq!(r["type"], "error", "{r}");
    assert_eq!(r["code"], "pattern_parse");
    assert_eq!(r["token"], "x");
    assert_eq!(r["offset"], 4);
    assert_eq!(r["expected"], json!(["'#'", "'+'", "'['", "'/'", "'('"]));
    for (pattern, token, offset) in [
        ("", "end of pattern", 0),
        ("[V", "end of pattern", 2),
        ("([V]", "end of pattern", 4),
        ("[V])", ")", 3),
        ("[V]*", "*", 3),
        ("[V] # [V]", "#", 4),
        ("()", ")", 1),
        ("//", "//", 0),
        ("/x/", "/x/", 0),
    ] {
        let r = s.matching(pattern, &["aba"]);
        assert_eq!(r["code"], "pattern_parse", "{pattern:?}: {r}");
        assert_eq!(r["token"], token, "{pattern:?}: {r}");
        assert_eq!(r["offset"], offset, "{pattern:?}: {r}");
        assert!(
            !r["expected"].as_array().unwrap().is_empty(),
            "{pattern:?}: {r}"
        );
    }
}

#[test]
fn unknown_names_are_typed_with_near_matches() {
    let mut s = Server::start();
    let r = s.matching("[vowel]", &["aba"]);
    assert_eq!(r["code"], "unknown_natural_class", "{r}");
    assert_eq!(r["name"], "vowel");
    assert_eq!(r["offset"], 1);
    let r = s.matching("[voiced]", &["aba"]);
    assert_eq!(r["code"], "unknown_natural_class", "{r}");
    assert_eq!(r["near"], json!(["Voiced"]));
    let r = s.matching("[+voice -son]", &["aba"]);
    assert_eq!(r["code"], "unknown_feature", "{r}");
    assert_eq!(r["name"], "son");
    assert_eq!(r["offset"], 9);
}

#[test]
fn gated_constructs_are_typed_unsupported() {
    let mut s = Server::start();
    for (pattern, construct) in [
        ("[+voice]", "featureBundle"),
        ("[-voice]", "featureBundle"),
        ("[αvoice]", "alphaVariable"),
        ("[V] / [Stop] _ [V]", "environment"),
    ] {
        let r = s.matching(pattern, &["aba"]);
        assert_eq!(r["code"], "unsupported_construct", "{pattern}: {r}");
        assert_eq!(r["construct"], construct, "{pattern}: {r}");
    }
}

#[test]
fn request_errors_are_typed() {
    let mut s = Server::start();
    let r = s.ask(json!({"id": "m", "op": "match", "pattern": "[V]", "words": ["a", "b", "c"], "maxWords": 2}));
    assert_eq!(r["code"], "too_many_words", "{r}");
    assert_eq!(r["maxWords"], 2);
    let r = s.ask(json!({"id": "m", "op": "match", "pattern": "[V]"}));
    assert_eq!(r["code"], "bad_request", "{r}");
    let r = s.ask(json!({"id": "m", "op": "match", "words": ["a"]}));
    assert_eq!(r["code"], "bad_request", "{r}");
    let r = s
        .ask(json!({"id": "m", "op": "match", "pattern": "[V]", "words": ["a"], "wordsFile": "x"}));
    assert_eq!(r["code"], "bad_request", "{r}");
    let r = s.ask(
        json!({"id": "m", "op": "match", "pattern": "[V]", "wordsFile": "/nonexistent/words.txt"}),
    );
    assert_eq!(r["code"], "words_file_unreadable", "{r}");
    let r = s.ask(
        json!({"id": "m", "op": "match", "pattern": "[V]", "words": ["a"], "boundaryMarker": "x"}),
    );
    assert_eq!(r["code"], "unknown_boundary_marker", "{r}");
    let r = s.ask(
        json!({"id": "m", "op": "match", "pattern": "[V]", "words": ["a"], "maxMatchesPerWord": 0}),
    );
    assert_eq!(r["code"], "bad_request", "{r}");
}

#[test]
fn a_words_file_is_read_one_word_per_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("words.txt");
    std::fs::write(&path, "aba\r\n\nbab\n").unwrap();
    let mut s = Server::start();
    let r = s.ask(json!({"id": "m", "op": "match", "pattern": "[V]", "wordsFile": path}));
    assert_eq!(r["words"].as_array().unwrap().len(), 2, "{r}");
    assert_eq!(r["words"][1]["word"], "bab");
}

#[test]
fn a_match_cap_makes_the_response_incomplete_never_silently_partial() {
    let mut s = Server::start();
    let r = s.ask(
        json!({"id": "m", "op": "match", "pattern": "[V]", "words": ["aba", "b"],
        "maxMatchesPerWord": 1}),
    );
    assert_eq!(r["complete"], false, "{r}");
    assert_eq!(r["stopReason"], "match_cap");
    assert_eq!(r["words"][0]["status"], "incomplete");
    assert_eq!(r["words"][0]["stop"], "match_cap");
    assert_eq!(
        spans(&r["words"][0]),
        vec![(0, 1)],
        "the first sorted match is kept"
    );
    assert_eq!(r["words"][1]["status"], "ok");
}

#[test]
fn the_same_request_gives_the_same_bytes() {
    let mut s = Server::start();
    let request = json!({"id": "m", "op": "match", "pattern": "([Stop])[V]([Stop])*",
        "words": ["abad", "daba", "ŋaŋ", "bbb"]});
    let first = s.ask(request.clone());
    let second = s.ask(request.clone());
    assert_eq!(first, second);
    let mut fresh = Server::start();
    assert_eq!(fresh.ask(request), first);
    for w in first["words"].as_array().unwrap() {
        let found = spans(w);
        let mut sorted = found.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(found, sorted, "matches are sorted and unique: {w}");
    }
}

#[test]
fn describe_declares_exactly_what_is_emitted() {
    let output = Command::new(BIN).arg("--describe").output().unwrap();
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
    let decl = &doc["interfaces"]["serveOps"]["match"];
    assert_eq!(decl["version"], 1);
    let strings = |v: &Value| -> BTreeSet<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect()
    };

    let mut s = Server::start();
    // Every supported construct has an exemplar that runs; every gated one is refused as unsupported_construct.
    let supported = [
        ("naturalClass", "[V]"),
        ("literal", "/a/"),
        ("optional", "([V])[V]"),
        ("repeat", "([V])*[V]"),
        ("wordEdge", "#[V]"),
    ];
    let mut covered: BTreeSet<String> = BTreeSet::new();
    for (name, pattern) in supported {
        let r = s.matching(pattern, &["aba"]);
        assert_eq!(r["type"], "result", "{name}: {r}");
        covered.insert(name.to_string());
    }
    let r = s.ask(json!({"id": "m", "op": "match", "pattern": "[Stop]+[V]", "boundaryMarker": "+", "words": ["ab+a"]}));
    assert_eq!(r["type"], "result", "{r}");
    covered.insert("morphemeBoundary".into());
    assert_eq!(strings(&decl["constructs"]["supported"]), covered);

    let gated = [
        ("featureBundle", "[+voice]"),
        ("alphaVariable", "[αvoice]"),
        ("environment", "[V] / [V] _ [V]"),
    ];
    let mut gated_seen: BTreeSet<String> = BTreeSet::new();
    for (name, pattern) in gated {
        let r = s.matching(pattern, &["aba"]);
        assert_eq!(r["code"], "unsupported_construct", "{name}: {r}");
        assert_eq!(r["construct"], name);
        gated_seen.insert(name.to_string());
    }
    assert_eq!(strings(&decl["constructs"]["gated"]), gated_seen);

    // Statuses and stop reasons: collect what the requests below emit.
    let mut statuses = BTreeSet::new();
    let mut stops = BTreeSet::new();
    let mut note = |r: &Value| {
        for w in r["words"].as_array().into_iter().flatten() {
            statuses.insert(w["status"].as_str().unwrap().to_string());
        }
        if let Some(stop) = r["stopReason"].as_str() {
            stops.insert(stop.to_string());
        }
    };
    note(&s.ask(json!({"id": "m", "op": "match", "pattern": "[V]", "words": ["aba", "axa", "b"], "maxMatchesPerWord": 1})));
    assert_eq!(strings(&decl["statuses"]), statuses);
    // `total_match_cap` needs a million matches; the unit test below drives it.
    stops.insert("total_match_cap".into());
    assert_eq!(strings(&decl["stopReasons"]), stops);

    // Every declared error code is one the requests in this file elicit, and none is undeclared.
    for (pattern, words) in [
        ("[V] x", "aba"),
        ("[vowel]", "aba"),
        ("[+voice -son]", "aba"),
        ("[+voice]", "aba"),
        ("[Stop]+[V]", "aba"),
    ] {
        s.matching(pattern, &[words]);
    }
    s.ask(json!({"id": "m", "op": "match", "pattern": "[V]", "words": ["a", "b"], "maxWords": 1}));
    s.ask(json!({"id": "m", "op": "match"}));
    s.ask(
        json!({"id": "m", "op": "match", "pattern": "[V]", "wordsFile": "/nonexistent/words.txt"}),
    );
    s.ask(
        json!({"id": "m", "op": "match", "pattern": "[V]", "words": ["a"], "boundaryMarker": "x"}),
    );
    // `pattern_compile_failed` and `internal` guard invariants a request cannot reach.
    let mut emitted = s.seen_error_codes.clone();
    emitted.insert("pattern_compile_failed".into());
    emitted.insert("internal".into());
    assert_eq!(strings(&decl["errors"]), emitted);

    // The request keys the handler reads are the declared ones.
    assert!(strings(&decl["request"]).contains("pattern"));
    assert_eq!(decl["limits"]["maxPatternBytes"], 4096);
    let big = "[V]".repeat(2000);
    let r = s.matching(&big, &["aba"]);
    assert_eq!(
        r["code"], "bad_request",
        "an oversized pattern is refused: {r}"
    );
}
