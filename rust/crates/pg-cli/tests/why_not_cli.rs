//! `pangloss why-not` input validation and the `unknown` stub (docs/formats/why-not.md).

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
fn a_single_candidate_still_returns_results_and_a_null_best() {
    let v = json(&["--expect", "le1,le2"]);
    assert_eq!(v["schema"], "pangloss.whynot.v1");
    assert_eq!(v["word"], "ata");
    assert_eq!(v["results"].as_array().unwrap().len(), 1);
    let r = &v["results"][0];
    assert_eq!(r["result"], "unknown");
    assert_eq!(r["stop"], "not_implemented");
    assert!(r["analysis"].is_null() && r["failure"].is_null());
    assert_eq!(r["expect"][0][0]["morpheme"], "le1");
    assert_eq!(r["expect"][1][0]["morpheme"], "le2");
    assert!(v["best"].is_null());
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
    assert!(results.iter().all(|r| r["result"] == "unknown"));
    assert!(v["best"].is_null());
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
fn allomorph_selection_is_refused_until_allomorph_xml_ids_exist() {
    for args in [
        &["--expect", "le1@a1"][..],
        &["--expect", "le1", "--allomorphs", "a1"][..],
        &["--expect-json", "-"][..],
    ] {
        let o = if args[0] == "--expect-json" {
            let mut child = Command::new(BIN)
                .arg("why-not")
                .arg(grammar())
                .arg("ata")
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(br#"[[[{"morpheme":"le1","allomorph":"a1"}]]]"#)
                .unwrap();
            child.wait_with_output().unwrap()
        } else {
            run(args)
        };
        assert!(!o.status.success(), "{args:?} should fail");
        assert!(
            stderr(&o).contains("allomorph selection by id arrives with allomorph XML ids"),
            "{args:?}: {}",
            stderr(&o)
        );
        assert!(o.stdout.is_empty());
    }
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
        (&["--expect-guid", "aaa:bbb"], "no morpheme for entryGuid"),
        (&["--expect-guid", "aaa"], "entryGuid:msaGuid"),
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
fn caps_are_accepted_and_the_stub_says_it_ignores_them() {
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
    assert!(stderr(&o).contains("not yet applied"), "{}", stderr(&o));
}

#[test]
fn text_output_names_every_candidate() {
    let o = run(&["--expect", "le1", "--expect", "le2"]);
    assert!(o.status.success());
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(
        text.contains("[0] le1: unknown (not_implemented)"),
        "{text}"
    );
    assert!(text.contains("[1] le2: unknown"), "{text}");
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
    assert_eq!(cmd["positionals"], serde_json::json!(["grammar", "word"]));
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
