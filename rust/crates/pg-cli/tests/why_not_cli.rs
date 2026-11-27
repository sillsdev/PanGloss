//! `pangloss why-not` input validation, one answer per kind, and the suite runner (docs/formats/why-not.md).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");

fn grammar() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../conformance-staging/edge-cases/loader-isactive/grammar.xml")
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .arg("why-not")
        .arg(grammar())
        .arg("ata")
        .args(args)
        .output()
        .expect("run pangloss why-not")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn json(args: &[&str]) -> Value {
    let mut all = args.to_vec();
    all.push("--json");
    let o = run(&all);
    assert!(o.status.success(), "{}", stderr(&o));
    serde_json::from_slice(&o.stdout).unwrap()
}

#[test]
fn a_single_candidate_still_returns_one_result() {
    let v = json(&["--expect", "le1,le2"]);
    assert_eq!(v["schema"], "pangloss.whynot.v1");
    assert_eq!(v["word"], "ata");
    assert_eq!(v["results"].as_array().unwrap().len(), 1);
    let r = &v["results"][0];
    assert!(["accepted", "rejected", "unknown"].contains(&r["result"].as_str().unwrap()));
    assert_eq!(r["expect"][0][0]["morpheme"], "le1");
    assert_eq!(r["expect"][1][0]["morpheme"], "le2");
}

#[test]
fn repeated_expect_gives_one_result_per_candidate_in_input_order() {
    let v = json(&[
        "--expect",
        "le1",
        "--expect",
        "le2,le1",
        "--expect",
        "le1,le1,le2",
    ]);
    let results = v["results"].as_array().unwrap();
    let lens: Vec<usize> = results
        .iter()
        .map(|r| r["expect"].as_array().unwrap().len())
        .collect();
    assert_eq!(lens, [1, 2, 3]);
}

#[test]
fn choices_at_a_position_are_echoed() {
    let v = json(&["--expect", "le1|le2,le1"]);
    let e = &v["results"][0]["expect"];
    assert_eq!(e[0].as_array().unwrap().len(), 2);
    assert_eq!(e[0][0]["morpheme"], "le1");
    assert_eq!(e[0][1]["morpheme"], "le2");
    assert!(e[0][0].get("allomorph").is_none());
}

#[test]
fn expect_json_reads_a_file_and_stdin() {
    let dir = std::env::temp_dir().join(format!("why-not-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("candidates.json");
    let text = r#"[[[{"morpheme":"le1"},{"morpheme":"le2"}],[{"morpheme":"le2"}]],[[{"morpheme":"le1"}]]]"#;
    std::fs::write(&file, text).unwrap();
    let v = json(&["--expect-json", file.to_str().unwrap()]);
    assert_eq!(v["results"].as_array().unwrap().len(), 2);
    assert_eq!(v["results"][0]["expect"][0].as_array().unwrap().len(), 2);

    let mut child = Command::new(BIN)
        .args(["why-not"])
        .arg(grammar())
        .args(["ata", "--expect-json", "-", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let from_stdin: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(from_stdin, v);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn input_errors_exit_nonzero_with_a_reason() {
    let cases: &[(&[&str], &str)] = &[
        (&[], "at least one candidate"),
        (&["--expect", "nope"], "unknown morpheme id nope"),
        (&["--expect", "le1,,le2"], "morpheme id is empty"),
        (&["--expect", "le1|"], "morpheme id is empty"),
        (&["--expect-guid", "aaa:bbb"], "no morpheme for entry_guid"),
        (&["--expect-guid", "aaa"], "entry_guid:msa_guid"),
        (
            &["--expect", "le1", "--expect-json", "-"],
            "cannot be combined",
        ),
        (&["--expect", "le1", "--word-timeout-ms", "soon"], "integer"),
        (&["--expect", "le1", "--work-cap", "lots"], "work"),
        (&["--expect", "le1", "--bogus"], "unknown option"),
    ];
    for (args, needle) in cases {
        let o = run(args);
        assert!(!o.status.success(), "{args:?} should fail");
        assert!(stderr(&o).contains(needle), "{args:?}: {}", stderr(&o));
        assert!(o.stdout.is_empty(), "{args:?} wrote stdout");
    }
}

#[test]
fn an_empty_word_and_malformed_json_are_input_errors() {
    let o = Command::new(BIN)
        .arg("why-not")
        .arg(grammar())
        .arg("")
        .args(["--expect", "le1"])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(stderr(&o).contains("word is empty"), "{}", stderr(&o));

    for bad in [
        "[]",
        "[[]]",
        "[[[]]]",
        r#"[[[{"morpheme":"le1","x":1}]]]"#,
        "{}",
    ] {
        let dir = std::env::temp_dir().join(format!("why-not-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("c.json");
        std::fs::write(&f, bad).unwrap();
        let o = run(&["--expect-json", f.to_str().unwrap()]);
        assert!(!o.status.success(), "{bad} should fail");
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn caps_are_applied_and_no_longer_reported_as_ignored() {
    let o = run(&[
        "--expect",
        "le1",
        "--work-cap",
        "1000",
        "--word-timeout-ms",
        "50",
        "--json",
    ]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(!stderr(&o).contains("not yet applied"), "{}", stderr(&o));
}

#[test]
fn text_output_names_every_candidate() {
    let o = run(&["--expect", "le1", "--expect", "le2"]);
    assert!(o.status.success());
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("[0] le1:"), "{text}");
    assert!(text.contains("[1] le2:"), "{text}");
}

#[test]
fn describe_lists_why_not_with_its_flags() {
    let o = Command::new(BIN).arg("--describe").output().unwrap();
    assert!(o.status.success());
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    let cmd = v["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "why-not")
        .expect("why-not is described");
    let flags: Vec<&str> = cmd["flags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    for want in [
        "--expect",
        "--allomorphs",
        "--expect-guid",
        "--expect-json",
        "--json",
        "--work-cap",
        "--word-timeout-ms",
    ] {
        assert!(flags.contains(&want), "missing {want}: {flags:?}");
    }
    assert_eq!(
        cmd["positionals"],
        serde_json::json!(["grammar", "word|queries.jsonl", "output.jsonl?"])
    );
}

#[test]
fn the_docs_json_examples_deserialize_into_the_engine_types() {
    use pg_parse::why_not::{Candidate, FailureKind, FirstFailure, MorphRef};
    let doc = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/formats/why-not.md"),
    )
    .unwrap();
    let blocks: Vec<&str> = doc
        .split("```json\n")
        .skip(1)
        .map(|b| b.split("\n```").next().unwrap())
        .collect();
    let (mut inputs, mut failures, mut kinds) = (0, 0, Vec::new());
    for block in blocks {
        let v: Value = serde_json::from_str(block).unwrap();
        if v.is_array() {
            serde_json::from_value::<Vec<Candidate>>(v).unwrap();
            inputs += 1;
            continue;
        }
        for r in v["results"].as_array().unwrap() {
            serde_json::from_value::<Candidate>(r["expect"].clone()).unwrap();
            if !r["failure"].is_null() {
                let f: FirstFailure = serde_json::from_value(r["failure"].clone()).unwrap();
                kinds.push(f.kind);
                failures += 1;
            }
            for u in r["analysis"]["used"].as_array().into_iter().flatten() {
                serde_json::from_value::<MorphRef>(u.clone()).unwrap();
            }
        }
    }
    assert_eq!(inputs, 1);
    assert!(failures >= FailureKind::ALL.len());
    for kind in FailureKind::ALL {
        assert!(kinds.contains(&kind), "no example for {kind:?}");
    }
}

fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

fn disjunctive_fixture() -> String {
    // The suite fixture sillsdev/fieldworks-parser-conformance edge-cases/disjunctive-folded-env-blocks.
    pg_conformance_fixtures::suite_root()
        .join("edge-cases/disjunctive-folded-env-blocks/grammar.xml")
        .to_str()
        .unwrap()
        .to_string()
}

fn answer_on(grammar: &str, word: &str, args: &[&str]) -> Value {
    let o = Command::new(BIN)
        .arg("why-not")
        .arg(data(grammar))
        .arg(word)
        .args(args)
        .arg("--json")
        .output()
        .expect("run pangloss why-not");
    assert!(o.status.success(), "{}", stderr(&o));
    serde_json::from_slice::<Value>(&o.stdout).unwrap()["results"][0].clone()
}

fn failure_kind(r: &Value) -> &str {
    assert_eq!(r["result"], "rejected", "{r}");
    r["failure"]["kind"].as_str().unwrap()
}

#[test]
fn an_expected_sequence_that_parses_is_accepted_with_its_analysis() {
    let r = answer_on("trace-family.xml", "kibes", &["--expect", "eKib,mrTense"]);
    assert_eq!(r["result"], "accepted", "{r}");
    assert!(r["failure"].is_null() && r["stop"].is_null());
    assert_eq!(r["analysis"]["used"][0]["morpheme"], "eKib");
    assert_eq!(r["analysis"]["used"][1]["morpheme"], "mrTense");
}

#[test]
fn a_surface_no_expected_sequence_produces_is_no_morph_match() {
    let r = answer_on("trace-family.xml", "kibes", &["--expect", "eKib,mrPl"]);
    assert_eq!(failure_kind(&r), "no_morph_match", "{r}");
    assert_eq!(r["failure"]["provenance"], "derived");
}

#[test]
fn a_morpheme_co_occurrence_exclusion_is_an_adhoc_prohibition() {
    let r = answer_on(
        "trace-family.xml",
        "kibuton",
        &["--expect", "eKib,mrPast2,mrPl"],
    );
    assert_eq!(failure_kind(&r), "adhoc_prohibition", "{r}");
    assert_eq!(r["failure"]["hc_reason"], "MorphemeCoOccurrenceRules");
    assert_eq!(
        r["failure"]["position"], 2,
        "the rule's owner, mrPl, is the third morph: {r}"
    );
    let morphs: Vec<&str> = r["failure"]["morphs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["morpheme"].as_str().unwrap())
        .collect();
    assert_eq!(morphs, ["mrPl", "mrPast2"], "{r}");
    let members: Vec<&str> = r["failure"]["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["morpheme"].as_str().unwrap())
        .collect();
    assert_eq!(members, ["mrPl", "mrPast2"], "{r}");
    assert!(r["failure"]["object"]["guid"].is_null(), "{r}");
    for m in r["failure"]["members"].as_array().unwrap() {
        assert!(
            m.get("guid").is_none() && m.get("form_guid").is_none(),
            "{m}"
        );
    }
}

#[test]
fn a_failure_outside_the_named_kinds_is_unknown_with_a_stop_reason() {
    let r = answer_on(
        "trace-family.xml",
        "toy",
        &["--expect", "eTay@aTayRestricted"],
    );
    assert_eq!(r["result"], "unknown", "{r}");
    assert_eq!(r["stop"], "unclassified_failure");
    assert!(r["stop_note"].as_str().is_some_and(|n| !n.is_empty()));
}

#[test]
fn a_candidate_with_no_lexical_entry_is_unknown_no_root() {
    let r = answer_on("trace-family.xml", "es", &["--expect", "mrTense"]);
    assert_eq!(r["result"], "unknown", "{r}");
    assert_eq!(r["stop"], "no_root");
}

#[test]
fn an_excluded_environment_names_the_actual_context() {
    let r = answer_on(
        "why-not-environments.xml",
        "salk",
        &["--expect", "eSal,mrSufK"],
    );
    assert_eq!(failure_kind(&r), "environment_unmet", "{r}");
    assert_eq!(r["failure"]["provenance"], "hc_reported");
    assert_eq!(r["failure"]["environment"]["allomorph_id"], "aSal");
    assert_eq!(r["failure"]["context"]["right"], "k", "{r}");
    assert_eq!(r["failure"]["context"]["left"], "", "{r}");
    assert_eq!(r["failure"]["context"]["provenance"], "derived", "{r}");
    let environment = &r["failure"]["environment"];
    assert!(
        environment.get("guid").is_none() && environment.get("text").is_none(),
        "{r}"
    );
    assert!(r["failure"]["object"]["guid"].is_null(), "{r}");
    assert!(r["failure"]["morphs"][0]["form_guid"].is_null(), "{r}");
    let ok = answer_on(
        "why-not-environments.xml",
        "salt",
        &["--expect", "eSal,mrSufT"],
    );
    assert_eq!(ok["result"], "accepted", "{ok}");
}

#[test]
fn a_required_environment_the_context_does_not_meet_is_environment_unmet() {
    let r = answer_on(
        "why-not-environments.xml",
        "monk",
        &["--expect", "eMon,mrSufK"],
    );
    assert_eq!(failure_kind(&r), "environment_unmet", "{r}");
    assert_eq!(r["failure"]["environment"]["allomorph_id"], "aMon");
    let ok = answer_on(
        "why-not-environments.xml",
        "mont",
        &["--expect", "eMon,mrSufT"],
    );
    assert_eq!(ok["result"], "accepted", "{ok}");
}

#[test]
fn a_suffix_the_word_does_not_end_in_is_no_morph_match() {
    let r = answer_on(
        "why-not-environments.xml",
        "losk",
        &["--expect", "eLos,mrSufT"],
    );
    assert_eq!(failure_kind(&r), "no_morph_match", "{r}");
}

#[test]
fn a_pinned_allomorph_must_be_the_one_that_surfaces() {
    let g = "why-not-allomorphs.xml";
    let short = answer_on(g, "tan", &["--expect", "eTa@aTaShort"]);
    assert_eq!(short["result"], "accepted", "{short}");
    assert_eq!(short["analysis"]["used"][0]["allomorph"], "aTaShort");
    let wrong = answer_on(g, "tan", &["--expect", "eTa@aTaLong"]);
    assert_eq!(failure_kind(&wrong), "no_morph_match", "{wrong}");
    let long = answer_on(g, "tano", &["--expect", "eTa", "--allomorphs", "aTaLong"]);
    assert_eq!(long["result"], "accepted", "{long}");
    let free = answer_on(g, "tano", &["--expect", "eTa"]);
    assert_eq!(free["result"], "accepted", "{free}");
}

#[test]
fn allomorph_selection_errors_name_the_problem() {
    let g = data("why-not-allomorphs.xml");
    for (spec, needle) in [
        ("eTa@nope", "allomorph"),
        ("eTa@aLo", "does not belong to morpheme"),
    ] {
        let o = Command::new(BIN)
            .arg("why-not")
            .arg(&g)
            .arg("tan")
            .args(["--expect", spec])
            .output()
            .unwrap();
        assert!(!o.status.success(), "{spec} should fail");
        assert!(stderr(&o).contains(needle), "{spec}: {}", stderr(&o));
        assert!(o.stdout.is_empty());
    }
}

#[test]
fn the_suite_runner_writes_one_row_per_query_in_the_suite_shape() {
    let dir = std::env::temp_dir().join(format!("why-not-suite-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let queries = dir.join("q.jsonl");
    let out = dir.join("o.jsonl");
    std::fs::write(
        &queries,
        concat!(
            r#"{"word":"kibes","expect":[{"morpheme":"KIB"},{"morpheme":"TENSE"}]}"#,
            "\n",
            r#"{"word":"kibuton","expect":[{"morpheme":"KIB"},{"morpheme":"PAST2"},{"morpheme":"PL"}]}"#,
            "\n",
            r#"{"word":"kibes","expect":[{"morpheme":"NOPE"}]}"#,
            "\n",
        ),
    )
    .unwrap();
    let o = Command::new(BIN)
        .arg("why-not")
        .arg(data("trace-family.xml"))
        .arg(&queries)
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let rows: Vec<Value> = std::fs::read_to_string(&out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["result"], "accepted");
    assert_eq!(rows[1]["result"], "rejected");
    assert_eq!(rows[1]["failure"]["kind"], "adhoc_prohibition");
    assert_eq!(rows[2]["result"], "unknown");
    assert_eq!(rows[2]["stop"]["reason"], "unknown_id");
    assert_eq!(rows[0]["word"], "kibes");
    std::fs::remove_dir_all(&dir).ok();
}

fn is_subset(asserted: &Value, got: &Value) -> bool {
    match (asserted, got) {
        (Value::Object(a), Value::Object(g)) => a
            .iter()
            .all(|(k, v)| g.get(k).is_some_and(|gv| is_subset(v, gv))),
        (Value::Array(a), Value::Array(g)) => {
            a.len() == g.len() && a.iter().zip(g).all(|(x, y)| is_subset(x, y))
        }
        _ => asserted == got,
    }
}

#[test]
fn the_suite_fixture_adhoc_prohibition_row_matches_by_result_and_asserted_failure_fields() {
    let dir = data("adhoc-prohibition-whynot");
    let out = std::env::temp_dir().join(format!("why-not-fixture-{}.jsonl", std::process::id()));
    let o = Command::new(BIN)
        .arg("why-not")
        .arg(dir.join("grammar.xml"))
        .arg(dir.join("queries.jsonl"))
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let rows: Vec<Value> = std::fs::read_to_string(&out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    if let Ok(keep) = std::env::var("WHY_NOT_FIXTURE_RUN_OUT") {
        std::fs::copy(&out, keep).unwrap();
    }
    std::fs::remove_file(&out).ok();
    let asserted: Value = serde_json::json!({
        "word": "tada",
        "result": "rejected",
        "failure": {
            "kind": "adhoc_prohibition",
            "hc_reason": "AllomorphCoOccurrenceRules",
            "morphs": [
                {"morpheme": "TA", "allomorph": "subTa"},
                {"morpheme": "DA", "allomorph": "aDa"}
            ]
        }
    });
    assert!(is_subset(&asserted, &rows[0]), "{}", rows[0]);
    assert_eq!(rows[1]["result"], "accepted", "{}", rows[1]);
}

#[test]
fn json_allomorph_pins_and_a_wrong_guid_on_a_unique_id_is_refused() {
    let dir = std::env::temp_dir().join(format!("why-not-json-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pinned = dir.join("pinned.json");
    std::fs::write(&pinned, r#"[[[{"morpheme":"eTa","allomorph":"aTaLong"}]]]"#).unwrap();
    let r = answer_on(
        "why-not-allomorphs.xml",
        "tan",
        &["--expect-json", pinned.to_str().unwrap()],
    );
    assert_eq!(failure_kind(&r), "no_morph_match", "{r}");
    assert_eq!(r["expect"][0][0]["allomorph"], "aTaLong");

    let wrong = dir.join("wrong.json");
    std::fs::write(
        &wrong,
        r#"[[[{"morpheme":"eTa","guid":{"entry_guid":"e-1","msa_guid":"m-1"}}]]]"#,
    )
    .unwrap();
    let o = Command::new(BIN)
        .arg("why-not")
        .arg(data("why-not-allomorphs.xml"))
        .arg("tan")
        .args(["--expect-json", wrong.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(stderr(&o).contains("guid"), "{}", stderr(&o));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_category_clash_names_the_affix_whose_stem_requirement_the_root_refuses() {
    let r = answer_on(
        "why-not-pos-clash.xml",
        "lost",
        &["--expect", "eLos,mrSufT"],
    );
    assert_eq!(failure_kind(&r), "feature_clash", "{r}");
    let f = &r["failure"];
    assert_eq!(f["object"]["type"], "rule", "{r}");
    assert_eq!(f["object"]["id"], "mrSufT", "{r}");
    assert_eq!(f["hc_reason"], "LexicalLookupFeatures");
    assert!(!f["features"].as_array().unwrap().is_empty(), "{r}");
}

#[test]
fn an_unfilled_required_slot_names_its_template_and_slot() {
    let r = answer_on("why-not-required-slot.xml", "ga", &["--expect", "eGa"]);
    assert_eq!(failure_kind(&r), "required_slot_empty", "{r}");
    let f = &r["failure"];
    assert_eq!(f["object"]["type"], "template");
    assert_eq!(f["object"]["name"], "verbTemplate");
    assert_eq!(f["slot"]["index"], 0);
    assert_eq!(f["slot"]["name"], "prefixSlot");
    assert_eq!(f["hc_reason"], "PartialParse");
}

#[test]
fn a_family_block_names_the_blocking_entry() {
    let r = answer_on("trace-family.xml", "zodut", &["--expect", "eZod,mrPast2"]);
    assert_eq!(failure_kind(&r), "other_gate", "{r}");
    let f = &r["failure"];
    assert_eq!(f["gate"], "family_block");
    assert_eq!(f["object"]["type"], "entry");
    assert_eq!(f["object"]["id"], "eVem");
    assert_eq!(f["provenance"], "derived");
}

#[test]
fn a_category_clash_in_a_derivation_with_several_rules_stays_unknown() {
    // mrSufK then mrSufT: the stem requirement mrSufT meets is the result of mrSufK, so no single affix is proven to own the clash.
    let r = answer_on(
        "why-not-pos-clash.xml",
        "loskt",
        &["--expect", "eLos,mrSufK,mrSufT"],
    );
    assert_ne!(r["result"], "rejected", "a wrong owner was named: {r}");
    assert_eq!(r["result"], "unknown", "{r}");
    assert!(r.get("failure").is_none_or(Value::is_null), "{r}");
    assert!(
        r["stop_note"].as_str().is_some_and(|n| !n.is_empty()),
        "{r}"
    );
}

#[test]
fn an_empty_required_slot_with_two_applicable_templates_stays_unknown() {
    let r = answer_on(
        "why-not-required-slot-two-templates.xml",
        "ga",
        &["--expect", "eGa"],
    );
    assert_eq!(r["result"], "unknown", "a wrong owner was named: {r}");
    assert!(r.get("failure").is_none_or(Value::is_null), "{r}");
    assert!(
        r["stop_note"].as_str().is_some_and(|n| !n.is_empty()),
        "{r}"
    );
}

#[test]
fn a_surface_mismatch_is_unknown_and_names_no_object() {
    // tal+k synthesizes to "tak" (prDelL), not to "talk", and the morphs are otherwise unobjectionable.
    let r = answer_on(
        "why-not-surface-mismatch.xml",
        "talk",
        &["--expect", "eTal,mrSufK"],
    );
    assert_eq!(r["result"], "unknown", "{r}");
    assert_eq!(r["stop"], "unclassified_failure", "{r}");
    assert!(r.get("failure").is_none_or(Value::is_null), "{r}");
    let note = r["stop_note"].as_str().unwrap_or_default();
    assert!(note.contains("synthesize to \"tak\""), "{r}");
}

#[test]
fn a_require_rule_is_a_required_co_occurrence_not_an_adhoc_prohibition() {
    let r = answer_on(
        "why-not-require-rule.xml",
        "lostk",
        &["--expect", "eLos,mrSufT,mrSufK"],
    );
    assert_eq!(failure_kind(&r), "required_co_occurrence", "{r}");
    let f = &r["failure"];
    assert_eq!(f["object"]["type"], "rule");
    assert_eq!(f["morphs"][0]["morpheme"], "mrSufK");
    assert_eq!(f["morphs"][1]["morpheme"], "mrPreO");
    assert_eq!(f["hc_reason"], "MorphemeCoOccurrenceRules");
}

#[test]
fn an_environment_checked_on_a_synthesized_form_says_so() {
    let r = answer_on(
        "why-not-synth-env.xml",
        "salk",
        &["--expect", "eSal,mrSufK"],
    );
    assert_eq!(failure_kind(&r), "environment_unmet", "{r}");
    let plain = r["failure"]["plain"].as_str().unwrap();
    assert!(plain.contains("synthesized form"), "{plain}");
    assert!(plain.contains("\"salk\""), "{plain}");
    assert_eq!(r["failure"]["context"]["left"], "sas", "{r}");
}

#[test]
fn a_deleted_segment_still_counts_for_its_allomorphs_environment() {
    // prDelL deletes the l, but the engine keeps the node, so aSal is placed and its excluded `_ k` fails first.
    let r = answer_on(
        "why-not-phon-delete.xml",
        "salk",
        &["--expect", "eSal,mrSufK"],
    );
    assert_eq!(failure_kind(&r), "environment_unmet", "{r}");
    assert_eq!(r["failure"]["object"]["id"], "aSal", "{r}");
    let plain = r["failure"]["plain"].as_str().unwrap();
    assert!(
        plain.contains("\"sak\"") && plain.contains("\"salk\""),
        "{plain}"
    );
}

#[test]
fn a_wrong_allomorph_pin_does_not_claim_a_wrong_order() {
    let r = answer_on(
        "why-not-allomorphs.xml",
        "tank",
        &["--expect", "eTa@aTaLong,mrSufK"],
    );
    assert_eq!(failure_kind(&r), "no_morph_match", "{r}");
    let plain = r["failure"]["plain"].as_str().unwrap();
    assert!(
        plain.contains("other allomorphs") && !plain.contains("not in this order"),
        "{plain}"
    );
}

#[test]
fn the_derivation_that_got_furthest_is_reported() {
    let g = "why-not-ranking.xml";
    let r = answer_on(g, "monk", &["--expect", "eMon,mrSufK"]);
    assert_eq!(failure_kind(&r), "adhoc_prohibition", "{r}");
    assert_eq!(r["failure"]["position"], 1, "{r}");
    assert_eq!(r["failure"]["alternatives"], 1, "{r}");
    let pinned = answer_on(g, "monk", &["--expect", "eMon@aMon1,mrSufK"]);
    assert_eq!(failure_kind(&pinned), "environment_unmet", "{pinned}");
}

#[test]
fn across_sequences_the_furthest_rejection_wins_in_either_order() {
    let g = "why-not-ranking.xml";
    for spec in ["eMon|eLos,mrSufK", "eLos|eMon,mrSufK"] {
        let r = answer_on(g, "monk", &["--expect", spec]);
        assert_eq!(failure_kind(&r), "adhoc_prohibition", "{spec}: {r}");
    }
}

#[test]
fn an_unknown_sequence_beats_a_rejected_one() {
    let r = answer_on(
        "why-not-ranking.xml",
        "monk",
        &["--expect", "eMon|mrSufT,mrSufK"],
    );
    assert_eq!(r["result"], "unknown", "{r}");
    assert_eq!(r["stop"], "no_root");
}

#[test]
fn a_furthest_derivation_failing_the_disjunctive_check_is_named_not_hidden_by_an_earlier_rejection()
{
    let r = answer_on(
        "why-not-disjunctive-suffix.xml",
        "monkt",
        &["--expect", "eMon,mrSufK,mrSufT"],
    );
    assert_ne!(
        r["failure"]["kind"], "environment_unmet",
        "the root allomorph failing its environment is not the furthest derivation: {r}"
    );
    assert_eq!(failure_kind(&r), "other_gate", "{r}");
    assert_eq!(r["failure"]["hc_reason"], "DisjunctiveAllomorph", "{r}");
    assert_eq!(r["failure"]["gate"], "disjunctive_allomorph", "{r}");
    assert_eq!(r["failure"]["provenance"], "hc_reported", "{r}");
}

#[test]
fn two_candidates_are_ranked_and_the_furthest_is_marked_best() {
    let o = Command::new(BIN)
        .arg("why-not")
        .arg(data("why-not-ranking.xml"))
        .arg("monk")
        .args(["--expect", "eLos,mrSufK", "--expect", "eMon,mrSufK"])
        .output()
        .expect("run pangloss why-not");
    assert!(o.status.success(), "{}", stderr(&o));
    let text = String::from_utf8_lossy(&o.stdout).into_owned();
    let line = |i: usize| {
        text.lines()
            .find(|l| l.trim_start().starts_with(&format!("[{i}]")))
            .unwrap_or_else(|| panic!("no line for candidate {i}: {text}"))
            .to_string()
    };
    assert!(!line(0).ends_with("(best)"), "{text}");
    assert!(line(1).ends_with("(best)"), "{text}");
}

#[test]
fn suite_mode_refuses_json_and_expect_flags() {
    let o = Command::new(BIN)
        .arg("why-not")
        .arg(data("trace-family.xml"))
        .args(["q.jsonl", "o.jsonl", "--json"])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(stderr(&o).contains("suite form"), "{}", stderr(&o));
}

#[test]
fn describe_declares_the_why_not_capability() {
    let o = Command::new(BIN).arg("--describe").output().unwrap();
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    let c = &v["interfaces"]["whyNot"];
    assert_eq!(c["schema"], "pangloss.whynot.v1", "{v}");
    assert_eq!(c["ids"], "grammar-xml");
    assert_eq!(c["suiteRows"], true);
    assert_eq!(c["expectGuidChoices"], true, "{v}");
    let kinds: Vec<&str> = c["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "no_morph_match",
            "environment_unmet",
            "required_slot_empty",
            "feature_clash",
            "adhoc_prohibition",
            "other_gate",
            "required_co_occurrence"
        ]
    );
}

#[test]
fn every_declared_kind_is_really_answered_and_the_rest_are_unknown() {
    let o = Command::new(BIN).arg("--describe").output().unwrap();
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    let declared: Vec<String> = v["interfaces"]["whyNot"]["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().to_string())
        .collect();
    let seen = [
        answer_on("trace-family.xml", "kibes", &["--expect", "eKib,mrPl"]),
        answer_on(
            "why-not-environments.xml",
            "salk",
            &["--expect", "eSal,mrSufK"],
        ),
        answer_on(
            "trace-family.xml",
            "kibuton",
            &["--expect", "eKib,mrPast2,mrPl"],
        ),
        answer_on(&disjunctive_fixture(), "banku", &["--expect", "eBan,mrDat"]),
        answer_on(
            "why-not-pos-clash.xml",
            "lost",
            &["--expect", "eLos,mrSufT"],
        ),
        answer_on("why-not-required-slot.xml", "ga", &["--expect", "eGa"]),
        answer_on(
            "why-not-require-rule.xml",
            "lostk",
            &["--expect", "eLos,mrSufT,mrSufK"],
        ),
    ];
    for r in &seen {
        let k = failure_kind(r).to_string();
        assert!(declared.contains(&k), "{k} answered but not declared");
    }
    assert_eq!(seen.len(), declared.len());
}

fn assert_no_engine_ordinals(r: &Value) {
    let text = r.to_string();
    for key in [
        "morphemeIds",
        "morphOccurrences",
        "morphemeId",
        "allomorphId",
        "posId",
    ] {
        assert!(!text.contains(key), "{key} is an engine ordinal: {text}");
    }
}

#[test]
fn an_environment_failure_names_its_allomorph_and_morpheme_by_grammar_xml_id() {
    let r = answer_on(
        "why-not-environments.xml",
        "salk",
        &["--expect", "eSal,mrSufK"],
    );
    let f = &r["failure"];
    assert_eq!(f["object"]["type"], "allomorph", "{r}");
    assert_eq!(f["object"]["id"], "aSal");
    assert_eq!(f["environment"]["allomorph_id"], "aSal");
    assert_eq!(f["morphs"][0]["morpheme"], "eSal");
    assert_eq!(f["morphs"][0]["allomorph"], "aSal");
    let plain = f["plain"].as_str().unwrap();
    assert!(plain.contains("aSal") && plain.contains("eSal"), "{plain}");
    assert_no_engine_ordinals(&r);
}

#[test]
fn an_allomorph_prohibition_names_both_allomorphs_and_a_typed_absent_rule() {
    let r = answer_on("why-not-ranking.xml", "monk", &["--expect", "eMon,mrSufK"]);
    assert_eq!(failure_kind(&r), "adhoc_prohibition", "{r}");
    let f = &r["failure"];
    assert_eq!(f["hc_reason"], "AllomorphCoOccurrenceRules");
    assert_eq!(f["morphs"][0]["morpheme"], "mrSufK");
    assert!(f["morphs"][0]["allomorph"].is_string(), "{r}");
    assert_eq!(f["morphs"][1]["morpheme"], "eMon");
    assert!(f["morphs"][1]["allomorph"].is_string(), "{r}");
    // grammar.xml gives a co-occurrence rule no id: the object is typed, its id absent.
    assert_eq!(f["object"]["type"], "rule", "{r}");
    assert!(f["object"]["id"].is_null() && f["object"]["guid"].is_null());
    let plain = f["plain"].as_str().unwrap();
    let (a, b) = (
        f["morphs"][0]["allomorph"].as_str().unwrap(),
        f["morphs"][1]["allomorph"].as_str().unwrap(),
    );
    assert!(plain.contains(a) && plain.contains(b), "{plain}");
    assert_no_engine_ordinals(&r);
}

#[test]
fn a_morpheme_prohibition_names_both_morphemes_by_id_in_plain() {
    let r = answer_on(
        "trace-family.xml",
        "kibuton",
        &["--expect", "eKib,mrPast2,mrPl"],
    );
    let plain = r["failure"]["plain"].as_str().unwrap();
    assert!(
        plain.contains("mrPl") && plain.contains("mrPast2"),
        "{plain}"
    );
    assert_eq!(r["failure"]["object"]["type"], "rule", "{r}");
    assert!(r["failure"]["object"]["id"].is_null());
    assert_no_engine_ordinals(&r);
}

#[test]
fn an_accepted_analysis_is_named_by_authored_ids_only() {
    let r = answer_on("trace-family.xml", "kibes", &["--expect", "eKib,mrTense"]);
    let a = &r["analysis"]["analysis"];
    assert_eq!(a["morphs"][0]["morpheme"], "eKib", "{r}");
    assert_eq!(a["morphs"][1]["morpheme"], "mrTense");
    assert!(a["morphs"][0]["allomorph"].is_string());
    assert_eq!(a["root_index"], 0);
    assert!(a["category"].is_string(), "{r}");
    assert_eq!(a["guessed"], false);
    assert_no_engine_ordinals(&r);
}

#[test]
fn a_no_morph_match_names_its_morphs_by_id() {
    let r = answer_on("trace-family.xml", "kibes", &["--expect", "eKib,mrPl"]);
    assert_eq!(r["failure"]["morphs"][0]["morpheme"], "eKib", "{r}");
    assert_eq!(r["failure"]["morphs"][1]["morpheme"], "mrPl");
    assert_no_engine_ordinals(&r);
}

#[test]
fn a_guid_shaped_environment_id_authored_in_grammar_xml_is_not_reported_as_a_guid() {
    let xml = std::fs::read_to_string(data("why-not-environments.xml"))
        .unwrap()
        .replace(
            "<Environment>",
            "<Environment id=\"3f2a1b4c-0000-4000-8000-000000000001\">",
        );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("g.xml");
    std::fs::write(&path, xml).unwrap();
    let o = Command::new(BIN)
        .arg("why-not")
        .arg(&path)
        .arg("salk")
        .args(["--expect", "eSal,mrSufK", "--json"])
        .output()
        .expect("run pangloss why-not");
    assert!(o.status.success(), "{}", stderr(&o));
    let r = serde_json::from_slice::<Value>(&o.stdout).unwrap()["results"][0].clone();
    assert_eq!(failure_kind(&r), "environment_unmet", "{r}");
    let environment = &r["failure"]["environment"];
    assert!(
        environment["guid"].is_null() && environment["text"].is_null(),
        "{r}"
    );
    assert!(!r.to_string().contains("3f2a1b4c"), "{r}");
}

const FW_PROJECT: &str = "../pg-parse/tests/data/why-not-fieldworks-guids/project.fwdata";
const FW: &str = "00000000-0000-0000-0000-";

fn fw_guid(entry: &str, msa: &str) -> String {
    format!(r#"{{"entry_guid":"{FW}{entry}","msa_guid":"{FW}{msa}"}}"#)
}

fn fw_why_not(word: &str, candidates: &str) -> Output {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("c.json");
    std::fs::write(&file, candidates).unwrap();
    Command::new(BIN)
        .arg("why-not")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join(FW_PROJECT))
        .arg(word)
        .args(["--expect-json", file.to_str().unwrap(), "--json"])
        .output()
        .expect("run pangloss why-not")
}

fn fw_results(word: &str, candidates: &str) -> Vec<Value> {
    let o = fw_why_not(word, candidates);
    assert!(o.status.success(), "{}", stderr(&o));
    serde_json::from_slice::<Value>(&o.stdout).unwrap()["results"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn a_json_choice_may_name_a_morpheme_by_guid_alone() {
    let c = format!(
        r#"[[[{{"guid":{}}}]]]"#,
        fw_guid("000000000100", "000000000102")
    );
    let r = fw_results("da", &c);
    assert_eq!(r[0]["failure"]["kind"], "environment_unmet", "{r:?}");
    assert!(r[0]["expect"][0][0]["morpheme"].is_string(), "{r:?}");
    assert_eq!(
        r[0]["failure"]["morphs"][0]["guid"]["msa_guid"],
        format!("{FW}000000000102")
    );
}

#[test]
fn id_and_guid_candidates_mix_in_one_json_call() {
    let guid = fw_guid("000000000100", "000000000102");
    let first = fw_results("da", &format!(r#"[[[{{"guid":{guid}}}]]]"#));
    let key = first[0]["expect"][0][0]["morpheme"].as_str().unwrap();
    let c = format!(r#"[[[{{"morpheme":"{key}"}}]],[[{{"guid":{guid}}}]]]"#);
    let r = fw_results("da", &c);
    assert_eq!(r.len(), 2);
    for one in &r {
        assert_eq!(one["failure"]["kind"], "environment_unmet", "{one}");
        assert_eq!(one["expect"][0][0]["morpheme"], key);
    }
}

#[test]
fn a_guid_only_json_choice_expands_as_expect_guid_does() {
    let (entry, msa) = ("000000000200", "000000000202");
    let spec = format!("{FW}{entry}:{FW}{msa}");
    let by_flag = Command::new(BIN)
        .arg("why-not")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join(FW_PROJECT))
        .arg("tada")
        .args(["--expect-guid", &spec, "--json"])
        .output()
        .unwrap();
    assert!(by_flag.status.success(), "{}", stderr(&by_flag));
    let flag: Value = serde_json::from_slice(&by_flag.stdout).unwrap();
    let c = format!(r#"[[[{{"guid":{}}}]]]"#, fw_guid(entry, msa));
    let json = fw_results("tada", &c);
    assert_eq!(json[0]["expect"], flag["results"][0]["expect"]);
}

#[test]
fn a_json_choice_whose_id_and_guid_disagree_is_an_error() {
    let first = fw_results(
        "da",
        &format!(
            r#"[[[{{"guid":{}}}]]]"#,
            fw_guid("000000000100", "000000000102")
        ),
    );
    let key = first[0]["expect"][0][0]["morpheme"].as_str().unwrap();
    let c = format!(
        r#"[[[{{"morpheme":"{key}","guid":{}}}]]]"#,
        fw_guid("000000000100", "000000000103")
    );
    let o = fw_why_not("da", &c);
    assert!(!o.status.success());
    assert!(
        stderr(&o).contains("does not belong to morpheme"),
        "{}",
        stderr(&o)
    );
}

#[test]
fn a_json_choice_with_neither_id_nor_guid_or_half_a_guid_is_an_error() {
    for (bad, want) in [
        (
            r#"[[[{"allomorph":"a"}]]]"#.to_string(),
            "needs a `morpheme` id or a `guid`",
        ),
        (
            format!(r#"[[[{{"guid":{{"entry_guid":"{FW}000000000100"}}}}]]]"#),
            "needs both guid.entry_guid and guid.msa_guid",
        ),
        (
            r#"[[[{"guid":{"entry_guid":"x","msa_guid":"y"}}]]]"#.to_string(),
            "no morpheme for entry_guid",
        ),
    ] {
        let o = fw_why_not("da", &bad);
        assert!(!o.status.success(), "{bad}");
        assert!(stderr(&o).contains(want), "{bad}: {}", stderr(&o));
    }
}

#[test]
fn a_disjunctive_break_before_the_analysed_allomorph_is_a_named_other_gate() {
    let r = answer_on(&disjunctive_fixture(), "banku", &["--expect", "eBan,mrDat"]);
    assert_eq!(failure_kind(&r), "other_gate", "{r}");
    assert!(
        r["failure"]["hc_reason"].is_null(),
        "C# raises no reason here: {r}"
    );
    assert_eq!(r["failure"]["gate"], "disjunctive_allomorph", "{r}");
    assert_eq!(r["failure"]["provenance"], "derived", "{r}");
    assert_eq!(r["failure"]["object"]["type"], "allomorph", "{r}");
    assert_eq!(r["failure"]["object"]["id"], "subKu", "{r}");
    assert_eq!(r["failure"]["position"], 1, "{r}");
    let morphs = r["failure"]["morphs"].as_array().unwrap();
    assert_eq!(morphs.len(), 2, "{r}");
    assert_eq!(morphs[0]["allomorph"], "subKu", "blocked first: {r}");
    assert_eq!(morphs[1]["allomorph"], "subKa", "blocker second: {r}");
}

#[test]
fn the_parses_beside_a_disjunctive_break_are_still_accepted() {
    for (word, expect) in [
        ("basku", "eBas,mrDat"),
        ("banka", "eBan,mrDat"),
        ("ban", "eBan"),
    ] {
        let r = answer_on(&disjunctive_fixture(), word, &["--expect", expect]);
        assert_eq!(r["result"], "accepted", "{word}: {r}");
    }
}
