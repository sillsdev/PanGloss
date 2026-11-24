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
        Server::start_with("match-fixture.xml")
    }

    fn start_with(fixture: &str) -> Server {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(fixture);
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
    let r = s.ask(json!({"id": "m", "op": "match", "pattern": "/ŋ/", "words": ["aŋa"], "segmentTest": "identity"}));
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
    let r = s.matching("[+voice -hard]", &["aba"]);
    assert_eq!(r["code"], "unknown_feature", "{r}");
    assert_eq!(r["name"], "hard");
    assert_eq!(r["offset"], 9);
}

#[test]
fn a_feature_value_a_sign_cannot_name_is_typed_unsupported() {
    let mut s = Server::start();
    let r = s.matching("[+tone]", &["aba"]);
    assert_eq!(r["code"], "unsupported_construct", "{r}");
    assert_eq!(r["construct"], "featureValue", "{r}");
    assert_eq!(r["offset"], 1);
    let r = s.matching("[+voice -tone]", &["aba"]);
    assert_eq!(r["construct"], "featureValue", "{r}");
    assert_eq!(r["offset"], 8);
}

#[test]
fn one_greek_letter_on_two_features_is_typed_unsupported() {
    let mut s = Server::start();
    for (pattern, offset) in [
        ("[αvoice] [αson]", 11),
        ("[αvoice αson]", 9),
        ("/b/ / [αvoice] _ [αson]", 19),
    ] {
        let r = s.matching(pattern, &["bab"]);
        assert_eq!(r["code"], "unsupported_construct", "{pattern}: {r}");
        assert_eq!(r["construct"], "alphaAcrossFeatures", "{pattern}: {r}");
        assert_eq!(r["offset"], offset, "{pattern}: {r}");
    }
    // The same letter on the same feature, and different letters on different features, still match.
    assert_eq!(s.matching("[αvoice][αvoice]", &["bab"])["type"], "result");
    assert_eq!(s.matching("[αvoice βson]", &["bab"])["type"], "result");
}

fn first_spans(r: &Value) -> Vec<Vec<(u64, u64)>> {
    r["words"].as_array().unwrap().iter().map(spans).collect()
}

#[test]
fn an_environment_returns_the_target_span_where_both_sides_hold() {
    let mut s = Server::start();
    let r = s.matching("[Stop] / [V] _ [V]", &["abada", "bada", "abba"]);
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(
        first_spans(&r),
        vec![vec![(1, 2), (3, 4)], vec![(2, 3)], vec![]],
        "{r}"
    );
    assert_eq!(
        r["words"][0]["matches"][0],
        json!({"segments": [1, 2], "offsets": [1, 2], "left": [0, 1], "right": [2, 5]})
    );
    // One side alone, and a pattern with both sides empty, which is the plain target.
    let r = s.matching("[Stop] / [V] _", &["abada"]);
    assert_eq!(first_spans(&r), vec![vec![(1, 2), (3, 4)]], "{r}");
    let r = s.matching("[Stop] / _ [V]", &["abab"]);
    assert_eq!(first_spans(&r), vec![vec![(1, 2)]], "{r}");
    let plain = s.matching("[V]", &["abada"]);
    let empty = s.matching("[V] / _", &["abada"]);
    assert_eq!(plain["words"], empty["words"]);
    // A literal target next to the separator still parses as a literal.
    let r = s.matching("/b/ / [V] _ [V]", &["abab"]);
    assert_eq!(first_spans(&r), vec![vec![(1, 2)]], "{r}");
}

#[test]
fn environment_anchors_hold_at_both_edges() {
    let mut s = Server::start();
    let r = s.matching("[V] / # _", &["aba", "baa"]);
    assert_eq!(first_spans(&r), vec![vec![(0, 1)], vec![]], "{r}");
    let r = s.matching("[V] / _ #", &["aba", "baa", "bab"]);
    assert_eq!(
        first_spans(&r),
        vec![vec![(2, 3)], vec![(2, 3)], vec![]],
        "{r}"
    );
    let r = s.matching("[V] / # _ #", &["a", "aa"]);
    assert_eq!(first_spans(&r), vec![vec![(0, 1)], vec![]], "{r}");
    // The side may be the anchor alone, with the other side open.
    let r = s.matching("[Stop] / # _ [V]", &["ba", "aba"]);
    assert_eq!(first_spans(&r), vec![vec![(0, 1)], vec![]], "{r}");
}

#[test]
fn optional_elements_in_an_environment_side_may_be_present_or_absent() {
    let mut s = Server::start();
    let r = s.matching("[V] / # ([Stop]) _", &["ba", "bba", "aba"]);
    assert_eq!(
        first_spans(&r),
        vec![vec![(1, 2)], vec![], vec![(0, 1)]],
        "{r}"
    );
    let r = s.matching("[V] / _ ([Stop]) #", &["a", "ab", "abb"]);
    assert_eq!(
        first_spans(&r),
        vec![vec![(0, 1)], vec![(0, 1)], vec![]],
        "{r}"
    );
    let r = s.matching("[Stop] / [V] ([V])* _", &["bab", "baab", "bb"]);
    assert_eq!(
        first_spans(&r),
        vec![vec![(2, 3)], vec![(3, 4)], vec![]],
        "{r}"
    );
}

#[test]
fn an_empty_target_is_an_insertion_point_between_the_sides() {
    let mut s = Server::start();
    let r = s.matching("/ [V] _ [V]", &["aa", "aba", "baab"]);
    assert_eq!(
        first_spans(&r),
        vec![vec![(1, 1)], vec![], vec![(2, 2)]],
        "{r}"
    );
    let r = s.matching("/ # _ [Stop]", &["ba", "ab"]);
    assert_eq!(first_spans(&r), vec![vec![(0, 0)], vec![]], "{r}");
    assert_eq!(
        r["words"][0]["matches"][0],
        json!({"segments": [0, 0], "offsets": [0, 0], "left": [0, 0], "right": [0, 2]})
    );
}

#[test]
fn a_boundary_is_transparent_to_a_side_unless_the_side_names_it() {
    let mut s = Server::start();
    let ask = |s: &mut Server, pattern: &str| {
        s.ask(
            json!({"id": "m", "op": "match", "pattern": pattern, "boundaryMarker": "+",
            "words": ["a+ba", "aba"]}),
        )
    };
    let r = ask(&mut s, "[V] / _ [Stop]");
    assert_eq!(first_spans(&r), vec![vec![(0, 1)], vec![(0, 1)]], "{r}");
    let r = ask(&mut s, "[V] / _ + [Stop]");
    assert_eq!(first_spans(&r), vec![vec![(0, 1)], vec![]], "{r}");
}

#[test]
fn environment_text_errors_are_typed() {
    let mut s = Server::start();
    for (pattern, token, offset) in [
        ("[V] / [Stop]", "end of pattern", 12),
        ("[V] _ [Stop]", "_", 4),
        ("# [V] / _ [Stop]", "#", 0),
        ("[V] / [Stop] # _", "#", 13),
        ("[V] / _ # [Stop]", "#", 8),
        ("[V] / _ [Stop] / _", "/", 15),
        ("[V] / ) _", ")", 6),
    ] {
        let r = s.matching(pattern, &["aba"]);
        assert_eq!(r["code"], "pattern_parse", "{pattern:?}: {r}");
        assert_eq!(r["token"], token, "{pattern:?}: {r}");
        assert_eq!(r["offset"], offset, "{pattern:?}: {r}");
    }
}

#[test]
fn an_environment_runs_under_the_work_budget() {
    let mut s = Server::start();
    let long = long_word();
    let r = s.ask(json!({"id": "m", "op": "match",
        "pattern": "[V] / ([V])*([V])*([V])*([V])*([V])*[Stop] _ [Stop]",
        "words": [long, "abab"], "maxWork": 100000}));
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(r["complete"], false, "{r}");
    assert_eq!(r["stopReason"], "work_limit");
    assert_eq!(r["words"][0]["stop"], "work_limit");
    assert_eq!(r["words"][0]["matches"], json!([]));
    assert_eq!(r["words"][1]["status"], "ok", "{r}");
    let r = s.ask(json!({"id": "m", "op": "match",
        "pattern": "[V] / ([V])*([V])*([V])*([V])*([V])*[Stop] _ [Stop]",
        "words": [very_long_word()], "maxWork": 4_000_000_000u64, "wordTimeoutMs": 200}));
    assert_eq!(r["stopReason"], "deadline", "{r}");
}

#[test]
fn a_side_alone_exhausts_the_work_budget_where_the_plain_target_does_not() {
    let mut s = Server::start();
    let long = long_word();
    let cap = 100_000;
    // The same target with no sides stops at the match cap, not the work cap, so the work limit below is the side's.
    let plain = s.ask(json!({"id": "m", "op": "match", "pattern": "[V]",
        "words": [long.clone()], "maxWork": cap}));
    assert_eq!(plain["stopReason"], "match_cap", "{plain}");
    for pattern in [
        "[V] / _ ([V])*([V])*([V])*([V])*([V])*[Stop] #",
        "[V] / # ([V])*([V])*([V])*([V])*([V])*[Stop] _",
    ] {
        let r = s.ask(json!({"id": "m", "op": "match", "pattern": pattern,
            "words": [long.clone(), "ab"], "maxWork": cap}));
        assert_eq!(r["complete"], false, "{pattern}: {r}");
        assert_eq!(r["words"][0]["stop"], "work_limit", "{pattern}: {r}");
        assert_eq!(r["words"][1]["status"], "ok", "{pattern}: {r}");
    }
}

#[test]
fn a_feature_bundle_selects_the_segments_with_those_values() {
    let mut s = Server::start();
    let r = s.matching("[+voice]", &["pabi"]);
    assert_eq!(first_spans(&r), vec![vec![(1, 2), (2, 3), (3, 4)]], "{r}");
    let r = s.matching("[-voice]", &["pabi"]);
    assert_eq!(first_spans(&r), vec![vec![(0, 1)]], "{r}");
    let r = s.matching("[+voice -son]", &["pabi", "adk"]);
    assert_eq!(first_spans(&r), vec![vec![(2, 3)], vec![(1, 2)]], "{r}");
    // A bundle is the same class a grammar class with those values is.
    let bundle = s.matching("[+voice]", &["pabinŋk"]);
    let class = s.matching("[Voiced]", &["pabinŋk"]);
    assert_eq!(bundle["words"], class["words"]);
    // It composes with sequences, groups and environments.
    let r = s.matching("[-voice][+voice -son]", &["pb", "bp", "pa"]);
    assert_eq!(first_spans(&r), vec![vec![(0, 2)], vec![], vec![]], "{r}");
    let r = s.matching("([-son])*[+son]", &["pba"]);
    assert_eq!(first_spans(&r), vec![vec![(0, 3), (1, 3), (2, 3)]], "{r}");
    let r = s.matching("[-son] / [+son] _ [+son]", &["abab", "aba", "bab"]);
    assert_eq!(
        first_spans(&r),
        vec![vec![(1, 2)], vec![(1, 2)], vec![]],
        "{r}"
    );
}

#[test]
fn bundle_text_errors_are_typed() {
    let mut s = Server::start();
    for (pattern, token, offset) in [
        ("[+voice +voice]", "+voice", 8),
        ("[+voice -voice]", "-voice", 8),
        ("[+voice αvoice]", "αvoice", 8),
        ("[+voice son]", "son", 8),
        ("[+]", "+", 1),
    ] {
        let r = s.matching(pattern, &["aba"]);
        assert_eq!(r["code"], "pattern_parse", "{pattern:?}: {r}");
        assert_eq!(r["token"], token, "{pattern:?}: {r}");
        assert_eq!(r["offset"], offset, "{pattern:?}: {r}");
    }
    let r = s.matching("[αnope]", &["aba"]);
    assert_eq!(r["code"], "unknown_feature", "{r}");
    assert_eq!(r["name"], "nope");
    assert_eq!(r["offset"], 3, "after the two-byte letter");
}

#[test]
fn an_alpha_variable_agrees_across_every_position_of_one_match() {
    let mut s = Server::start();
    let r = s.matching("[αvoice][αvoice]", &["pbta", "bda", "ptk"]);
    assert_eq!(
        first_spans(&r),
        vec![vec![], vec![(0, 2), (1, 3)], vec![(0, 2), (1, 3)]],
        "{r}"
    );
    let r = s.matching("[αvoice][-αvoice]", &["pbta"]);
    assert_eq!(first_spans(&r), vec![vec![(0, 2), (1, 3), (2, 4)]], "{r}");
    let r = s.matching("[αvoice][αvoice][αvoice]", &["bdap", "bdab"]);
    assert_eq!(
        first_spans(&r),
        vec![vec![(0, 3)], vec![(0, 3), (1, 4)]],
        "{r}"
    );
    // A variable alone binds and always agrees.
    let alone = s.matching("[αvoice]", &["pbta"]);
    assert_eq!(
        first_spans(&alone),
        vec![vec![(0, 1), (1, 2), (2, 3), (3, 4)]]
    );
}

#[test]
fn two_different_alphas_agree_independently() {
    let mut s = Server::start();
    // Voice and son must both agree: ba agrees in voice only, pb in neither son-wise nor voice-wise.
    let r = s.matching(
        "[αvoice βson][αvoice βson]",
        &["bd", "ba", "pt", "an", "pb", "ab"],
    );
    assert_eq!(
        first_spans(&r),
        vec![
            vec![(0, 2)],
            vec![],
            vec![(0, 2)],
            vec![(0, 2)],
            vec![],
            vec![]
        ],
        "{r}"
    );
    // The two variables sit at different positions: voice at the ends, son in the middle.
    let r = s.matching("[αvoice βson][βson][αvoice]", &["bdb", "bab", "pdp", "pdb"]);
    assert_eq!(
        first_spans(&r),
        vec![vec![(0, 3)], vec![], vec![(0, 3)], vec![]],
        "{r}"
    );
}

#[test]
fn an_alpha_variable_binds_across_the_target_and_both_sides() {
    let mut s = Server::start();
    let r = s.matching(
        "[+son] / [αvoice] _ [αvoice]",
        &["bab", "pab", "tat", "bai"],
    );
    assert_eq!(
        first_spans(&r),
        vec![vec![(1, 2)], vec![], vec![(1, 2)], vec![(1, 2)]],
        "{r}"
    );
    let r = s.matching("[+son αvoice] / _ [αvoice]", &["ab", "ap", "an"]);
    assert_eq!(
        first_spans(&r),
        vec![vec![(0, 1)], vec![], vec![(0, 1)]],
        "{r}"
    );
    let r = s.matching("[+son αvoice] / _ [-αvoice]", &["ab", "ap"]);
    assert_eq!(first_spans(&r), vec![vec![], vec![(0, 1)]], "{r}");
}

#[test]
fn an_alpha_pattern_runs_under_the_work_budget() {
    let mut s = Server::start();
    let long = long_word();
    let r = s.ask(json!({"id": "m", "op": "match",
        "pattern": "([αvoice])*([αvoice])*([αvoice])*([αvoice])*[Stop][Stop]",
        "words": [long, "aba"], "maxWork": 100000}));
    assert_eq!(r["complete"], false, "{r}");
    assert_eq!(r["stopReason"], "work_limit", "{r}");
    assert_eq!(r["words"][0]["matches"], json!([]));
    assert_eq!(r["words"][1]["status"], "ok", "{r}");
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

const PATHOLOGICAL: &str = "([V])*([V])*([V])*([V])*([V])*[Stop][Stop]";

fn long_word() -> String {
    format!("{}b", "a".repeat(3000))
}

/// Long enough that a 200 ms deadline ends the word before the work cap would.
fn very_long_word() -> String {
    format!("{}b", "a".repeat(20000))
}

#[test]
fn a_pathological_pattern_gets_a_typed_work_stop_and_the_other_words_still_run() {
    let started = std::time::Instant::now();
    let mut s = Server::start();
    let long = long_word();
    let r = s.ask(json!({"id": "m", "op": "match", "pattern": PATHOLOGICAL,
        "words": [long, "aba"], "maxWork": 100000}));
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(r["complete"], false, "{r}");
    assert_eq!(r["stopReason"], "work_limit");
    assert_eq!(r["words"][0]["status"], "incomplete");
    assert_eq!(r["words"][0]["stop"], "work_limit");
    assert_eq!(
        r["words"][0]["matches"],
        json!([]),
        "a stopped word reports no partial matches"
    );
    assert_eq!(
        r["words"][1]["status"], "ok",
        "the next word still runs: {r}"
    );
    assert!(started.elapsed().as_secs() < 60);
}

#[test]
fn a_pathological_pattern_with_default_limits_returns_a_typed_stop() {
    let started = std::time::Instant::now();
    let mut s = Server::start();
    let r = s.matching(PATHOLOGICAL, &[very_long_word().as_str()]);
    assert_eq!(r["type"], "result", "{r}");
    assert_eq!(r["complete"], false, "{r}");
    assert!(
        ["work_limit", "deadline"].contains(&r["stopReason"].as_str().unwrap()),
        "{r}"
    );
    assert!(
        started.elapsed().as_secs() < 60,
        "the default budget must bound the word"
    );
}

#[test]
fn a_word_timeout_stops_the_word_with_a_deadline() {
    let mut s = Server::start();
    let r = s.ask(json!({"id": "m", "op": "match", "pattern": PATHOLOGICAL,
        "words": [very_long_word()], "maxWork": 4_000_000_000u64, "wordTimeoutMs": 200}));
    assert_eq!(r["stopReason"], "deadline", "{r}");
    assert_eq!(r["words"][0]["stop"], "deadline");
}

#[test]
fn zero_width_matches_are_reported_for_patterns_that_can_match_nothing() {
    let mut s = Server::start();
    let r = s.matching("#([V])*", &["baa"]);
    assert_eq!(spans(&r["words"][0]), vec![(0, 0)], "{r}");
    let r = s.matching("#", &["baa"]);
    assert_eq!(spans(&r["words"][0]), vec![(0, 0)], "{r}");
    let r = s.matching("([V])*", &["bab"]);
    assert_eq!(
        spans(&r["words"][0]),
        vec![(0, 0), (1, 1), (1, 2), (2, 2), (3, 3)],
        "{r}"
    );
    let r = s.matching("([V])*#", &["ba"]);
    assert_eq!(spans(&r["words"][0]), vec![(1, 2), (2, 2)], "{r}");
    assert_eq!(
        r["words"][0]["matches"][1],
        json!({"segments": [2, 2], "offsets": [2, 2], "left": [0, 2], "right": [2, 2]})
    );
}

#[test]
fn a_marked_word_may_hold_only_the_marker_as_a_boundary() {
    let mut s = Server::start();
    let r = s.ask(
        json!({"id": "m", "op": "match", "pattern": "[V]", "boundaryMarker": "+",
        "words": ["a=b"]}),
    );
    assert_eq!(r["words"][0]["status"], "unsegmentable", "{r}");
    assert_eq!(r["words"][0]["reason"], "unexpected_boundary");
}

#[test]
fn unknown_request_fields_are_refused() {
    let mut s = Server::start();
    let r =
        s.ask(json!({"id": "m", "op": "match", "pattern": "[V]", "words": ["a"], "scope": "word"}));
    assert_eq!(r["code"], "bad_request", "{r}");
    assert!(
        r["message"]
            .as_str()
            .unwrap()
            .contains("unknown match field: scope"),
        "{r}"
    );
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
        ("environment", "[V] / [Stop] _ [Stop]"),
        ("featureBundle", "[+voice]"),
        ("alphaVariable", "[αvoice]"),
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
        ("featureValue", "[+tone]"),
        ("alphaAcrossFeatures", "[αvoice] [αson]"),
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
    note(&s.ask(json!({"id": "m", "op": "match", "pattern": PATHOLOGICAL, "words": [long_word()], "maxWork": 100000})));
    note(&s.ask(
        json!({"id": "m", "op": "match", "pattern": PATHOLOGICAL, "words": [very_long_word()],
        "maxWork": 4_000_000_000u64, "wordTimeoutMs": 300}),
    ));
    // `total_match_cap` needs a million matches, which a test cannot afford.
    stops.insert("total_match_cap".into());
    assert_eq!(strings(&decl["statuses"]), statuses);
    assert_eq!(strings(&decl["stopReasons"]), stops);
    let reasons: BTreeSet<String> = [
        "no_character_definition",
        "unexpected_boundary",
        "boundary_in_unmarked_word",
    ]
    .map(String::from)
    .into();
    assert_eq!(strings(&decl["wordReasons"]), reasons);

    // Every declared error code is one the requests in this file elicit, and none is undeclared.
    for (pattern, words) in [
        ("[V] x", "aba"),
        ("[V] / [Stop]", "aba"),
        ("[vowel]", "aba"),
        ("[+voice -hard]", "aba"),
        ("[+voice]", "aba"),
        ("[Stop]+[V]", "aba"),
    ] {
        s.matching(pattern, &[words]);
    }
    s.ask(json!({"id": "m", "op": "match", "pattern": "[V]", "words": ["a", "b"], "maxWords": 1}));
    s.ask(json!({"id": "m", "op": "match"}));
    s.ask(json!({"id": "m", "op": "match", "pattern": "[V]", "words": ["a"], "bogus": 1}));
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

/// The attack grammar of the `effects` review: `e` has the features of `i`, and `{i,u}` (a list of segments) is the union of theirs.
fn attack_matches(
    s: &mut Server,
    pattern: &str,
    word: &str,
    test: Option<&str>,
) -> Vec<(u64, u64)> {
    let mut request = json!({"id": "m", "op": "match", "pattern": pattern, "words": [word]});
    if let Some(test) = test {
        request["segmentTest"] = json!(test);
    }
    let r = s.ask(request);
    assert_eq!(r["type"], "result", "{r}");
    spans(&r["words"][0])
}

#[test]
fn the_engine_segment_test_matches_by_features_unless_identity_is_asked_for() {
    let mut s = Server::start_with("effects-feature-segment-test.xml");
    // Default and `engine`: a literal matches any segment with its feature bundle.
    assert_eq!(attack_matches(&mut s, "/i/", "e", None), [(0, 1)]);
    assert_eq!(attack_matches(&mut s, "/i/", "e", Some("engine")), [(0, 1)]);
    assert_eq!(attack_matches(&mut s, "/i/", "y", None), []);
    // A class built from a list of segments is the union of their features: every vowel is in it.
    for word in ["i", "u", "y", "a", "e"] {
        assert_eq!(
            attack_matches(&mut s, "[ncIU]", word, None),
            [(0, 1)],
            "{word}"
        );
    }
    // `identity` keeps exact segment and class membership.
    assert_eq!(attack_matches(&mut s, "/i/", "e", Some("identity")), []);
    assert_eq!(
        attack_matches(&mut s, "/i/", "i", Some("identity")),
        [(0, 1)]
    );
    for word in ["y", "a", "e"] {
        assert_eq!(
            attack_matches(&mut s, "[ncIU]", word, Some("identity")),
            [],
            "{word}"
        );
    }
    for word in ["i", "u"] {
        assert_eq!(
            attack_matches(&mut s, "[ncIU]", word, Some("identity")),
            [(0, 1)],
            "{word}"
        );
    }
}

#[test]
fn the_segment_test_is_declared_and_validated() {
    let mut s = Server::start();
    let r = s.ask(
        json!({"id": "m", "op": "match", "pattern": "[V]", "words": ["a"], "segmentTest": "loose"}),
    );
    assert_eq!(r["code"], "bad_request", "{r}");
    let r = s
        .ask(json!({"id": "m", "op": "match", "pattern": "[V]", "words": ["a"], "segmentTest": 1}));
    assert_eq!(r["code"], "bad_request", "{r}");
    let out = Command::new(BIN).arg("--describe").output().unwrap();
    let d: Value = serde_json::from_slice(&out.stdout).unwrap();
    let m = &d["interfaces"]["serveOps"]["match"];
    assert_eq!(m["defaultSegmentTest"], "engine", "{m}");
    assert_eq!(m["segmentTests"], json!(["engine", "identity"]), "{m}");
    assert!(
        m["request"]
            .as_array()
            .unwrap()
            .contains(&json!("segmentTest")),
        "{m}"
    );
}

#[test]
fn environment_sides_stay_exact_while_the_target_matches_by_features() {
    let mut s = Server::start_with("effects-feature-segment-test.xml");
    // Rule application tests an environment by identity: `e` shares `i`'s features but is not `i`, and `y` is not a member of `{i,u}`.
    assert_eq!(attack_matches(&mut s, "/a/ / _ /i/", "ae", None), []);
    assert_eq!(attack_matches(&mut s, "/a/ / _ /i/", "ai", None), [(0, 1)]);
    assert_eq!(attack_matches(&mut s, "/a/ / _ [ncIU]", "iay", None), []);
    assert_eq!(
        attack_matches(&mut s, "/a/ / _ [ncIU]", "iau", None),
        [(1, 2)]
    );
    assert_eq!(attack_matches(&mut s, "/a/ / /i/ _", "ia", None), [(1, 2)]);
    assert_eq!(attack_matches(&mut s, "/a/ / /i/ _", "ea", None), []);
    // The target of the same pattern is still tested by features.
    assert_eq!(attack_matches(&mut s, "/i/ / _ /a/", "ea", None), [(0, 1)]);
    assert_eq!(
        attack_matches(&mut s, "/i/ / _ /a/", "ea", Some("identity")),
        []
    );
}
