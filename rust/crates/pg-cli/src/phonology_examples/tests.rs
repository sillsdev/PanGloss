//! Pins the request bounds, the hash bindings and each disposition the command can reach.
use super::*;
use std::fs;

fn scratch_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("create scratch directory")
}

fn seg(id: &str, rep: &str, feats: &str) -> String {
    format!(
        r#"<SegmentDefinition id="{id}"><Representations><Representation>{rep}</Representation></Representations>{feats}</SegmentDefinition>"#
    )
}

fn rule(id: &str, from: &str, to: &str) -> String {
    format!(
        r#"<PhonologicalRule id="{id}"><Name>{id}</Name>
<PhoneticInput><PhoneticSequence><Segment segment="{from}"/></PhoneticSequence></PhoneticInput>
<PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="{to}"/></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
</PhonologicalRule>"#
    )
}

/// A one-stratum grammar with p, b, g, a and the given ordered rules (`p>b`, `b>g`).
fn grammar_xml(rules: &[&str], with_entry: bool) -> String {
    let cons = r#"<FeatureValue feature="f_cons" symbolValues="s_cp"/>"#;
    let segs = [
        seg("c_a", "a", r#"<FeatureValue feature="f_cons" symbolValues="s_cm"/>"#),
        seg(
            "c_p",
            "p",
            &format!(r#"{cons}<FeatureValue feature="f_voi" symbolValues="s_vm"/><FeatureValue feature="f_poa" symbolValues="s_lab"/>"#),
        ),
        seg(
            "c_b",
            "b",
            &format!(r#"{cons}<FeatureValue feature="f_voi" symbolValues="s_vp"/><FeatureValue feature="f_poa" symbolValues="s_lab"/>"#),
        ),
        seg(
            "c_g",
            "g",
            &format!(r#"{cons}<FeatureValue feature="f_voi" symbolValues="s_vp"/><FeatureValue feature="f_poa" symbolValues="s_vel"/>"#),
        ),
    ]
    .concat();
    let defs: String = rules
        .iter()
        .map(|r| match *r {
            "p>b" => rule("r_pb", "c_p", "c_b"),
            "b>g" => rule("r_bg", "c_b", "c_g"),
            other => panic!("unknown rule {other}"),
        })
        .collect();
    let ids = rules
        .iter()
        .map(|r| if *r == "p>b" { "r_pb" } else { "r_bg" })
        .collect::<Vec<_>>()
        .join(" ");
    let attr = if ids.is_empty() {
        String::new()
    } else {
        format!(r#" phonologicalRules="{ids}""#)
    };
    let entries = if with_entry {
        r#"<LexicalEntries><LexicalEntry id="eN" partOfSpeech="posN"><MorphemeId>m1</MorphemeId><Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries>"#
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>Phon</Name>
<PartsOfSpeech><PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech></PartsOfSpeech>
<PhonologicalFeatureSystem>
<SymbolicFeature id="f_cons"><Name>cons</Name><Symbols><Symbol id="s_cp">+</Symbol><Symbol id="s_cm">-</Symbol></Symbols></SymbolicFeature>
<SymbolicFeature id="f_voi"><Name>voi</Name><Symbols><Symbol id="s_vp">+</Symbol><Symbol id="s_vm">-</Symbol></Symbols></SymbolicFeature>
<SymbolicFeature id="f_poa"><Name>poa</Name><Symbols><Symbol id="s_lab">lab</Symbol><Symbol id="s_vel">vel</Symbol></Symbols></SymbolicFeature>
</PhonologicalFeatureSystem>
<CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>{segs}</SegmentDefinitions></CharacterDefinitionTable>
<PhonologicalRuleDefinitions>{defs}</PhonologicalRuleDefinitions>
<Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="linear"{attr}><Name>S</Name>{entries}</Stratum></Strata>
</Language></HermitCrabInput>"#
    )
}

fn example(id: &str, entry: &str, underlying: &str, expected: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "context": {"kind": "hcXmlEntry", "entryId": entry},
        "underlying": underlying,
        "expected": expected,
    })
}

fn request(examples: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({
        "version": 1,
        "limits": {"stepCap": 50_000_000u64, "workCap": 1_000_000u64, "timeoutMs": 30_000u64},
        "examples": examples,
    })
}

struct Run {
    dir: tempfile::TempDir,
    grammar: std::path::PathBuf,
    request: std::path::PathBuf,
    out: std::path::PathBuf,
}

fn setup(grammar: &str, request: &serde_json::Value) -> Run {
    let dir = scratch_dir();
    let run = Run {
        grammar: dir.path().join("g.xml"),
        request: dir.path().join("request.json"),
        out: dir.path().join("out.json"),
        dir,
    };
    fs::write(&run.grammar, grammar).unwrap();
    fs::write(&run.request, serde_json::to_vec_pretty(request).unwrap()).unwrap();
    run
}

fn execute(run: &Run) -> Result<serde_json::Value, String> {
    let args = [
        run.grammar.to_string_lossy().into_owned(),
        run.request.to_string_lossy().into_owned(),
        run.out.to_string_lossy().into_owned(),
    ];
    run_phonology_examples(&args)?;
    Ok(serde_json::from_slice(&fs::read(&run.out).unwrap()).unwrap())
}

fn measure(rules: &[&str], examples: Vec<serde_json::Value>) -> serde_json::Value {
    execute(&setup(&grammar_xml(rules, true), &request(examples))).expect("command runs")
}

fn rejects(request: serde_json::Value, needle: &str) {
    let run = setup(&grammar_xml(&["p>b"], true), &request);
    let err = execute(&run).expect_err("request must be refused");
    assert!(err.contains(needle), "`{err}` should mention `{needle}`");
    assert!(!run.out.exists(), "a refused request writes no output");
}

fn good_example() -> serde_json::Value {
    example("one", "eN", "pa", &["ga"])
}

#[test]
fn malformed_and_over_bound_requests_are_refused_without_output() {
    let mut unknown = request(vec![good_example()]);
    unknown["surprise"] = serde_json::json!(1);
    rejects(unknown, "surprise");

    let mut unknown_inner = request(vec![good_example()]);
    unknown_inner["examples"][0]["extra"] = serde_json::json!(true);
    rejects(unknown_inner, "extra");

    let mut unknown_limit = request(vec![good_example()]);
    unknown_limit["limits"]["attemptCap"] = serde_json::json!(3);
    rejects(unknown_limit, "attemptCap");

    rejects(request(vec![]), "examples");
    rejects(
        request(vec![good_example(), example("one", "eN", "pa", &["ga"])]),
        "duplicate example id",
    );
    rejects(request(vec![example("", "eN", "pa", &["ga"])]), "id");
    rejects(request(vec![example("x", "eN", "", &["ga"])]), "underlying");
    rejects(request(vec![example("x", "eN", "pa", &[])]), "expected");
    rejects(request(vec![example("x", "eN", "pa", &[""])]), "expected");
    let many: Vec<String> = (0..17).map(|i| format!("f{i}")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    rejects(request(vec![example("x", "eN", "pa", &many)]), "expected");
    let long = "a".repeat(257);
    rejects(
        request(vec![example("x", "eN", &long, &["ga"])]),
        "underlying",
    );

    let mut too_many = Vec::new();
    for i in 0..501 {
        too_many.push(example(&format!("e{i}"), "eN", "pa", &["ga"]));
    }
    rejects(request(too_many), "examples");

    for (field, value) in [
        ("stepCap", 0u64),
        ("stepCap", 50_000_001),
        ("workCap", 0u64),
        ("workCap", 5_000_000_001),
        ("timeoutMs", 0),
        ("timeoutMs", 600_001),
    ] {
        let mut r = request(vec![good_example()]);
        r["limits"][field] = serde_json::json!(value);
        rejects(r, field);
    }
    let mut wrong_version = request(vec![good_example()]);
    wrong_version["version"] = serde_json::json!(2);
    rejects(wrong_version, "version");
    let mut bad_context = request(vec![good_example()]);
    bad_context["examples"][0]["context"] = serde_json::json!({"kind": "guess"});
    rejects(bad_context, "invalid request");
    let long_context = "e".repeat(MAX_CONTEXT_ID_CHARS + 1);
    rejects(
        request(vec![example("x", &long_context, "pa", &["ga"])]),
        "context.entryId",
    );
}

#[test]
fn unreadable_json_and_wrong_argument_shapes_are_refused() {
    let run = setup(&grammar_xml(&["p>b"], true), &request(vec![good_example()]));
    fs::write(&run.request, b"{ not json").unwrap();
    assert!(execute(&run).is_err());
    assert!(!run.out.exists());
    assert!(run_phonology_examples(&[]).is_err());
    assert!(run_phonology_examples(&["--nope".into(), "a".into()]).is_err());
}

#[test]
fn result_binds_the_grammar_and_request_bytes() {
    let run = setup(
        &grammar_xml(&["p>b", "b>g"], true),
        &request(vec![good_example()]),
    );
    let out = execute(&run).unwrap();
    assert_eq!(out["version"], 1);
    assert_eq!(
        out["grammarSha256"],
        sha256_hex(&fs::read(&run.grammar).unwrap())
    );
    let first_file_hash = out["requestFileSha256"].clone();
    assert_eq!(
        out["requestSha256"],
        executable_request_sha256(&{
            let bytes = fs::read(&run.request).unwrap();
            serde_json::from_slice::<Request>(&bytes).unwrap()
        })
        .unwrap()
    );
    assert_eq!(out["build"], crate::build_info::embedded_build_info());
    assert_eq!(out["measuredEntryCount"], 1);
    assert_eq!(out["limits"]["scope"], "orderedPhonologyFolds");
    assert_eq!(out["limits"]["stepCap"], 50_000_000u64);
    assert_eq!(out["limits"]["workCap"], 1_000_000u64);
    assert_eq!(out["limits"]["timeoutMs"], 30_000u64);
    assert_eq!(out["limits"]["stepAllocation"], "balancedByRequestOrder");
    assert_eq!(out["limits"]["timeAllocation"], "sharedMonotonicDeadline");
    assert_eq!(out["rows"][0]["effectiveLimits"]["stepCap"], 50_000_000u64);
    assert_eq!(out["rows"][0]["effectiveLimits"]["workCap"], 1_000_000u64);
    let row_timeout_ms = out["rows"][0]["effectiveLimits"]["timeoutMs"]
        .as_u64()
        .unwrap();
    assert!((1..=30_000).contains(&row_timeout_ms));

    let before = out["requestSha256"].clone();
    fs::write(
        &run.request,
        serde_json::to_vec(&request(vec![example("one", "eN", "pa", &["ba"])])).unwrap(),
    )
    .unwrap();
    let changed = execute(&run).unwrap();
    assert_ne!(changed["requestSha256"], before);
    assert_ne!(changed["requestFileSha256"], first_file_hash);
}

#[test]
fn citation_metadata_changes_file_provenance_but_not_executable_request_identity() {
    let mut cited = request(vec![good_example()]);
    cited["examples"][0]["citation"] = serde_json::json!({
        "source": "PH-NASAL-N",
        "locator": "P2 section 4"
    });
    let run = setup(&grammar_xml(&["p>b"], true), &cited);
    let first = execute(&run).unwrap();

    cited["examples"][0]["citation"]["locator"] = serde_json::json!("P2 section 5");
    fs::write(&run.request, serde_json::to_vec(&cited).unwrap()).unwrap();
    let second = execute(&run).unwrap();

    assert_eq!(first["requestSha256"], second["requestSha256"]);
    assert_ne!(first["requestFileSha256"], second["requestFileSha256"]);
}

#[test]
fn request_wide_caps_are_split_without_exceeding_the_total() {
    let steps: Vec<u64> = (0..3).map(|index| cap_for_row(10, 3, index)).collect();
    let work: Vec<u64> = (0..3).map(|index| cap_for_row(8, 3, index)).collect();
    assert_eq!(steps, [4, 3, 3]);
    assert_eq!(work, [3, 3, 2]);
    assert_eq!(steps.iter().sum::<u64>(), 10);
    assert_eq!(work.iter().sum::<u64>(), 8);
}

#[test]
fn agree_and_disagree_follow_set_membership_over_a_completed_surface() {
    let out = measure(
        &["p>b", "b>g"],
        vec![
            example("fed", "eN", "pa", &["ba", "ga"]),
            example("wrong", "eN", "pa", &["pa", "ba"]),
            example("control", "eN", "aa", &["aa"]),
        ],
    );
    let rows = &out["rows"];
    assert_eq!(rows[0]["id"], "fed");
    assert_eq!(rows[0]["disposition"], "disagree");
    assert_eq!(rows[0]["completion"], "complete");
    assert_eq!(rows[0]["observed"], "ga");
    assert_eq!(
        rows[0]["expectedObserved"],
        serde_json::json!([false, true])
    );
    assert_eq!(
        rows[0]["requestedContext"],
        example("fed", "eN", "pa", &["ba", "ga"])["context"]
    );
    assert_eq!(rows[0]["resolvedContext"], rows[0]["requestedContext"]);
    assert_eq!(rows[0]["underlying"], "pa");
    assert_eq!(rows[0]["expected"], serde_json::json!(["ba", "ga"]));
    assert!(rows[0]["steps"].as_u64().unwrap() > 0);
    assert!(rows[0]["workSteps"].as_u64().unwrap() > 0);
    assert_eq!(rows[0]["capped"], false);
    assert_eq!(rows[0]["timedOut"], false);
    assert_eq!(rows[0]["reason"], serde_json::Value::Null);
    assert_eq!(
        rows[0]["orderedRuleIds"],
        serde_json::json!(["r_pb", "r_bg"])
    );
    assert_eq!(
        rows[0]["visitedRuleIds"],
        serde_json::json!(["r_pb", "r_bg"])
    );
    assert_eq!(
        rows[0]["appliedRuleIds"],
        serde_json::json!(["r_pb", "r_bg"])
    );
    assert_eq!(rows[1]["disposition"], "disagree");
    assert_eq!(rows[1]["observed"], "ga");
    assert_eq!(rows[2]["disposition"], "agree");
    assert_eq!(out["counts"]["agree"], 1);
    assert_eq!(out["counts"]["disagree"], 2);
    assert_eq!(out["counts"]["total"], 3);
}

#[test]
fn removing_a_rule_turns_agree_into_disagree() {
    let examples = || vec![example("fed", "eN", "pa", &["ga"])];
    assert_eq!(
        measure(&["p>b", "b>g"], examples())["rows"][0]["disposition"],
        "agree"
    );
    let without = measure(&["p>b"], examples());
    assert_eq!(without["rows"][0]["disposition"], "disagree");
    assert_eq!(without["rows"][0]["observed"], "ba");
}

#[test]
fn unavailable_rows_name_a_typed_reason_and_never_agree() {
    let out = measure(
        &["p>b"],
        vec![
            example("nobody", "missing", "pa", &["ba"]),
            example("shape", "eN", "pz", &["ba"]),
        ],
    );
    for (i, reason) in [(0, "unknownEntry"), (1, "invalidShape")] {
        let row = &out["rows"][i];
        assert_eq!(row["disposition"], "unavailable");
        assert_eq!(row["completion"], "notStarted");
        assert_eq!(row["reason"], reason);
        assert_eq!(row["observed"], serde_json::Value::Null);
    }
    assert_eq!(out["counts"]["unavailable"], 2);
    assert_eq!(out["counts"]["agree"], 0);

    let empty = execute(&setup(
        &grammar_xml(&["p>b"], false),
        &request(vec![good_example(), example("two", "eN", "ba", &["ba"])]),
    ))
    .unwrap();
    for row in empty["rows"].as_array().unwrap() {
        assert_eq!(row["disposition"], "unavailable");
        assert_eq!(row["reason"], "emptyLexicon");
    }
}

#[test]
fn a_named_zero_allomorph_entry_is_filtered_before_compiled_measurement() {
    let grammar = grammar_xml(&["p>b"], true).replace(
        "</LexicalEntries>",
        r#"<LexicalEntry id="eZero" partOfSpeech="posN"><MorphemeId>mZero</MorphemeId><Allomorphs></Allomorphs></LexicalEntry></LexicalEntries>"#,
    );
    let out = execute(&setup(
        &grammar,
        &request(vec![example("zero", "eZero", "pa", &["ba"])]),
    ))
    .unwrap();
    assert_eq!(out["measuredEntryCount"], 1);
    let row = &out["rows"][0];
    assert_eq!(row["disposition"], "unavailable");
    assert_eq!(row["completion"], "notStarted");
    assert_eq!(row["reason"], "unknownEntry");
    assert_eq!(row["resolvedContext"], serde_json::Value::Null);
}

#[test]
fn a_forced_stop_is_unavailable_with_incomplete_completion() {
    let mut r = request(vec![example("slow", "eN", "pappppppppa", &["gaggggggga"])]);
    r["limits"]["workCap"] = serde_json::json!(1);
    let out = execute(&setup(&grammar_xml(&["p>b", "b>g"], true), &r)).unwrap();
    let row = &out["rows"][0];
    assert_eq!(row["disposition"], "unavailable");
    assert_eq!(row["completion"], "incomplete");
    assert_eq!(row["reason"], "workCap");
    assert_eq!(row["observed"], serde_json::Value::Null);
    assert_eq!(row["expectedObserved"], serde_json::json!([false]));
    assert_eq!(row["orderedRuleIds"], serde_json::json!(["r_pb", "r_bg"]));
    assert_eq!(row["visitedRuleIds"], serde_json::json!(["r_pb"]));
    assert_eq!(row["appliedRuleIds"], serde_json::json!([]));
    assert_eq!(row["capped"], true);
    assert_eq!(row["timedOut"], false);
    assert_eq!(out["counts"]["unavailable"], 1);
    assert_eq!(out["completionCounts"]["incomplete"], 1);
    assert_eq!(out["counts"]["disagree"], 0);
    assert_eq!(out["completionCounts"]["notStarted"], 0);
}

#[test]
fn disposition_never_treats_an_incomplete_or_unavailable_result_as_agreement() {
    let expected = vec!["ga".to_string()];
    let stopped = PhonologyResult::Incomplete {
        reason: PhonologyStopReason::Timeout,
    };
    assert_eq!(classify(&stopped, &expected).0, Disposition::Unavailable);
    let none = PhonologyResult::Unavailable {
        reason: PhonologyUnavailableReason::UnknownEntry,
    };
    assert_eq!(classify(&none, &expected).0, Disposition::Unavailable);
}

#[test]
fn expectation_membership_uses_producer_nfd_normalization() {
    let result = PhonologyResult::Complete {
        surface: "é".to_string(),
    };
    let expected = vec!["e\u{301}".to_string()];
    assert_eq!(classify(&result, &expected).0, Disposition::Agree);
    assert_eq!(classify(&result, &expected).2, [true]);
}

#[test]
fn the_result_never_replaces_an_input_and_leaves_the_grammar_untouched() {
    let run = setup(&grammar_xml(&["p>b"], true), &request(vec![good_example()]));
    let before = fs::read(&run.grammar).unwrap();
    let args = [
        run.grammar.to_string_lossy().into_owned(),
        run.request.to_string_lossy().into_owned(),
        run.request.to_string_lossy().into_owned(),
    ];
    assert!(run_phonology_examples(&args)
        .unwrap_err()
        .contains("refusing"));
    execute(&run).unwrap();
    assert_eq!(fs::read(&run.grammar).unwrap(), before);
    assert!(run.dir.path().join("out.json").exists());
}

#[test]
fn output_hard_link_aliases_are_refused_without_changing_the_input() {
    let run = setup(&grammar_xml(&["p>b"], true), &request(vec![good_example()]));
    let before = fs::read(&run.request).unwrap();
    let alias = run.dir.path().join("request-alias.json");
    if fs::hard_link(&run.request, &alias).is_ok() {
        let args = [
            run.grammar.to_string_lossy().into_owned(),
            run.request.to_string_lossy().into_owned(),
            alias.to_string_lossy().into_owned(),
        ];
        let error = run_phonology_examples(&args).expect_err("hard link must be refused");
        assert!(error.contains("refusing to overwrite"), "{error}");
        assert_eq!(fs::read(&run.request).unwrap(), before);
        assert_eq!(fs::read(&alias).unwrap(), before);
    }
}

#[test]
fn request_bytes_are_bounded_before_json_deserialization_and_missing_outputs_refuse() {
    let run = setup(&grammar_xml(&["p>b"], true), &request(vec![good_example()]));
    fs::write(
        &run.request,
        vec![b' '; usize::try_from(MAX_REQUEST_BYTES).unwrap() + 1],
    )
    .unwrap();
    let args = [
        run.grammar.to_string_lossy().into_owned(),
        run.request.to_string_lossy().into_owned(),
        run.out.to_string_lossy().into_owned(),
    ];
    let error = run_phonology_examples(&args).unwrap_err();
    assert!(error.contains("exceeds the"), "{error}");
    assert!(!run.out.exists());

    let missing_parent = run.dir.path().join("missing").join("result.json");
    let text = "{}\n";
    assert!(write_atomically(&missing_parent, text).is_err());
    assert!(!missing_parent.exists());

    let altered = run.dir.path().join("altered-result.json");
    fs::write(&altered, b"{\"changed\":true}\n").unwrap();
    assert!(verify_written_output(&altered, text.as_bytes()).is_err());
}

#[test]
fn grammar_hash_and_loaded_grammar_use_the_same_snapshot_bytes() {
    let run = setup(&grammar_xml(&["p>b"], true), &request(vec![good_example()]));
    let grammar_bytes = fs::read(&run.grammar).unwrap();
    let request_bytes = fs::read(&run.request).unwrap();
    fs::write(&run.grammar, grammar_xml(&[], true)).unwrap();
    let request: Request = serde_json::from_slice(&request_bytes).unwrap();

    let response = build_response(
        &run.grammar.to_string_lossy(),
        &grammar_bytes,
        &request_bytes,
        &request,
        None,
    )
    .unwrap();
    assert_eq!(response.grammar_sha256, sha256_hex(&grammar_bytes));
    assert!(response.grammar_dependencies.is_empty());
    assert_eq!(response.rows[0].observed.as_deref(), Some("ba"));
}

#[test]
fn command_help_description_matches_the_registered_dispatch_surface() {
    let spec = crate::surface::find_command("phonology-examples").expect("registered");
    assert_eq!(spec.positionals, &["grammar", "request.json", "out.json?"]);
    assert!(spec.summary.contains("versioned JSON request"));
    assert!(!spec.hidden);
}

#[test]
fn describe_lists_the_command() {
    let spec = crate::surface::find_command("phonology-examples").expect("registered");
    assert_eq!(spec.positionals, &["grammar", "request.json", "out.json?"]);
    assert!(!spec.hidden);
}
