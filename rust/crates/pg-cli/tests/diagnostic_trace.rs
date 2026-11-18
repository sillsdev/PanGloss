//! Rejection diagnostics through the ordinary parser and batch writer.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{json, Value};

#[path = "diagnostic_trace/lineage.rs"]
mod lineage;
#[path = "diagnostic_trace/owner.rs"]
mod owner;
#[path = "diagnostic_trace/schema.rs"]
mod schema;
#[path = "diagnostic_trace/target.rs"]
mod target;

#[cfg(target_os = "linux")]
#[path = "diagnostic_trace/trace_off.rs"]
mod trace_off;

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");
const GRAMMAR: &str = include_str!("data/trace-family.xml");

fn run(args: &[&str]) -> Output {
    let binary = std::env::var_os("PANGLOSS_DIAGNOSTIC_TEST_BIN").unwrap_or_else(|| BIN.into());
    Command::new(binary).args(args).output().unwrap()
}

fn success(out: &Output) {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn records(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let record = serde_json::from_str(line).unwrap();
            schema::validate_record(&record).unwrap_or_else(|error| {
                panic!("{}:{}: {error}\n{line}", path.display(), index + 1)
            });
            record
        })
        .collect()
}

fn parse(xml: &str, word: &str, extra: &[&str]) -> Vec<Value> {
    parse_with_search(xml, word, &[], extra)
}

fn parse_with_search(xml: &str, word: &str, search: &[&str], trace: &[&str]) -> Vec<Value> {
    let temp = tempfile::tempdir().unwrap();
    let grammar = temp.path().join("grammar.xml");
    let artifact = temp.path().join("trace.jsonl");
    fs::write(&grammar, xml).unwrap();
    trace_parse(&grammar, &artifact, word, search, trace)
}

fn trace_parse(
    grammar: &Path,
    artifact: &Path,
    word: &str,
    search: &[&str],
    trace: &[&str],
) -> Vec<Value> {
    let mut args = vec!["parse", grammar.to_str().unwrap(), word];
    args.extend(search);
    let plain = run(&args);
    args.extend(["--trace-rejections", artifact.to_str().unwrap()]);
    args.extend(trace);
    let observed = run(&args);
    assert_eq!(observed.stdout, plain.stdout);
    assert_eq!(observed.status, plain.status);
    let stderr = String::from_utf8(observed.stderr).unwrap();
    let summary = stderr
        .lines()
        .find(|s| s.starts_with("rejection trace: "))
        .unwrap_or_else(|| panic!("missing rejection trace summary\n{stderr}"));
    assert!(summary.len() < 2048);
    let data = records(artifact);
    assert_eq!(data[0]["schemaVersion"], "pangloss.diagnostic.v1");
    assert_eq!(data.last().unwrap()["processedRows"], 1);
    check_joins(&data);
    data
}

fn check_joins(data: &[Value]) {
    let mut mappings = BTreeMap::new();
    let mut paths = BTreeMap::new();
    let mut events = BTreeMap::new();
    let mut outputs = BTreeSet::new();
    for row in data {
        let ordinal = row["row"].as_u64();
        match row["type"].as_str().unwrap() {
            "mapping" => {
                let object = &row["object"];
                let identity = (ordinal, object["kind"].as_str(), object["key"].as_str());
                assert!(mappings.insert(identity, object).is_none());
            }
            "path" => {
                assert!(paths
                    .insert((ordinal, row["pathId"].as_u64()), row)
                    .is_none());
            }
            "event" => {
                assert!(events
                    .insert((ordinal, row["eventId"].as_u64()), row)
                    .is_none());
            }
            "output" => assert!(outputs.insert((ordinal, row["outputId"].as_u64()))),
            _ => {}
        }
    }
    for (&(ordinal, path_id), path) in &paths {
        let parent = path["parentPathId"].as_u64();
        assert_eq!(parent.is_some(), path["parentStatus"] == "retained");
        if parent.is_some() {
            assert_ne!(parent, path_id);
            let parent = paths[&(ordinal, parent)];
            if let Some(depth) = parent["depth"].as_u64() {
                assert_eq!(path["depth"], depth + 1);
            }
        }
        let rejected = path["firstRejectEventId"].as_u64();
        assert_eq!(rejected.is_some(), path["outcome"] == "rejected");
        if rejected.is_some() {
            let event = events[&(ordinal, rejected)];
            assert_eq!(event["pathId"].as_u64(), path_id);
            assert_eq!(event["kind"], "reject");
        }
        let output = path["outputId"].as_u64();
        let output_status = path["outputStatus"].as_str().unwrap();
        assert_eq!(
            output.is_some(),
            matches!(output_status, "retained" | "omitted")
        );
        if output_status == "retained" {
            assert!(outputs.contains(&(ordinal, output)));
        }
        let merged_into = path["mergedIntoPathId"].as_u64();
        assert_eq!(merged_into.is_some(), path["mergeStatus"] == "retained");
        if merged_into.is_some() {
            assert_ne!(merged_into, path_id);
            assert!(paths.contains_key(&(ordinal, merged_into)));
        }
    }
    for (&(ordinal, event_id), event) in &events {
        let objects = event["objectRefs"].as_array().unwrap();
        for object in objects {
            assert!(mappings.contains_key(&(
                ordinal,
                object["kind"].as_str(),
                object["key"].as_str()
            )));
        }
        let path = paths[&(ordinal, event["pathId"].as_u64())];
        assert_eq!(event["parentPathId"], path["parentPathId"]);
        if event["kind"] == "reject" {
            assert_eq!(objects.iter().filter(|r| r["role"] == "subject").count(), 1);
            assert_eq!(path["outcome"], "rejected");
            assert_eq!(path["firstRejectEventId"].as_u64(), event_id);
        }
    }
    for row in data.iter().filter(|row| row["type"] == "word_end") {
        if let Some(matched) = row["target"]["matchedOutputIds"].as_array() {
            assert!(matched
                .iter()
                .all(|id| outputs.contains(&(row["row"].as_u64(), id.as_u64()))));
        }
    }
}

fn event<'a>(data: &'a [Value], reason: &str, direction: &str) -> &'a Value {
    data.iter()
        .find(|row| row["reason"] == reason && row["direction"] == direction)
        .unwrap_or_else(|| panic!("missing {direction} {reason}"))
}

const SWAP: &str = r#"<PhonologicalRuleDefinitions>
  <PhonologicalRule id="prSwap">
    <Name>Synthetic swap</Name>
    <PhoneticInput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticInput>
    <PhonologicalSubrules><PhonologicalSubrule>
      <PhoneticOutput><PhoneticSequence><Segment segment="cd" /></PhoneticSequence></PhoneticOutput>
    </PhonologicalSubrule></PhonologicalSubrules>
  </PhonologicalRule>
</PhonologicalRuleDefinitions><Strata>"#;

const TEMPLATE: &str = r#"<AffixTemplates>
  <AffixTemplate requiredPartsOfSpeech="posV">
    <Name>Synthetic template</Name>
    <Slot morphologicalRules="mrTense"><Name>Required suffix</Name></Slot>
  </AffixTemplate>
</AffixTemplates><LexicalEntries>"#;

fn bound_grammar() -> String {
    GRAMMAR.replace(r#"id="aKib""#, r#"id="aKib" isBound="true""#)
}

fn rewrite_grammar() -> String {
    GRAMMAR.replace("<Strata>", SWAP).replace(
        r#"morphologicalRuleOrder="linear""#,
        r#"phonologicalRules="prSwap" morphologicalRuleOrder="linear""#,
    )
}

fn template_grammar() -> String {
    GRAMMAR
        .replace("mrTense rrPast mrPast2 mrPl", "rrPast mrPast2 mrPl")
        .replace("<LexicalEntries>", TEMPLATE)
}

fn nonfinal_grammar(template: &str) -> String {
    template.replace(
        r#"<AffixTemplate requiredPartsOfSpeech="posV">"#,
        r#"<AffixTemplate requiredPartsOfSpeech="posV" final="false">"#,
    )
}

fn lexical_pos_clash_grammar() -> String {
    GRAMMAR
        .replace(
            "</PartsOfSpeech>",
            r#"<PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech></PartsOfSpeech>"#,
        )
        .replace(
            "<LexicalEntries>",
            r#"<LexicalEntries>
              <LexicalEntry id="eKibN" partOfSpeech="posN">
                <Allomorphs><Allomorph id="aKibN"><PhoneticShape>kib</PhoneticShape></Allomorph></Allomorphs>
                <MorphemeId>KIBN</MorphemeId>
              </LexicalEntry>"#,
        )
}

fn nonblocking_grammar() -> String {
    GRAMMAR.replace(
        "</Strata>",
        r#"<Stratum characterDefinitionTable="table1" morphologicalRuleOrder="linear">
            <Name>Outer</Name>
        </Stratum></Strata>"#,
    )
}

fn early_root_grammar() -> String {
    // Distinct trails open the 1024-state gate before the duplicate allomorph is tried last.
    let rules: Vec<_> = (0..1025)
        .map(|index| format!("mrAmbiguous{index}"))
        .collect();
    let definitions: String = rules
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let subrule = |variant| {
                format!(
                    r#"<MorphologicalSubrule id="{id}Sub{variant}">
                        <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                        <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
                    </MorphologicalSubrule>"#,
                )
            };
            let mut subrules = subrule(0);
            if index == 0 {
                subrules.push_str(&subrule(1));
            }
            format!(
                r#"<MorphologicalRule id="{id}" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
                    <Name>{id}</Name><MorphologicalSubrules>{subrules}</MorphologicalSubrules><MorphemeId>{id}</MorphemeId>
                </MorphologicalRule>"#,
            )
        })
        .collect();
    GRAMMAR
        .replace("mrTense rrPast mrPast2 mrPl", &rules.join(" "))
        .replace(
            "<MorphologicalRuleDefinitions>",
            &format!("<MorphologicalRuleDefinitions>{definitions}"),
        )
}

#[test]
fn owner_decisions_are_typed_and_their_controls_survive() {
    let accepted = parse(GRAMMAR, "kib", &[]);
    assert_eq!(
        event(&accepted, "lookup_completed", "analysis")["operands"]["materializedCount"],
        1
    );
    assert_eq!(
        accepted.iter().find(|r| r["type"] == "output").unwrap()["outputId"],
        0
    );
    let absent = parse(GRAMMAR, "zzz", &[]);
    assert_eq!(
        event(&absent, "lookup_no_seed", "analysis")["operands"]["materializedCount"],
        0
    );
    event(&parse(GRAMMAR, "!", &[]), "invalid_shape", "none");
    event(
        &parse(GRAMMAR, "kibes", &[]),
        "remaining_rules_in_stratum",
        "synthesis",
    );
    let bound = parse(&bound_grammar(), "kib", &[]);
    assert_eq!(
        event(&bound, "bound_root", "synthesis")["objectRefs"][0]["kind"],
        "allomorph"
    );
    assert!(accepted.iter().all(|row| row["reason"] != "bound_root"));
}

#[test]
fn surface_mismatch_uses_the_confirmation_operands() {
    let discordant = parse(&rewrite_grammar(), "kib", &[]);
    let mismatch = event(&discordant, "surface_mismatch", "synthesis");
    assert_eq!(mismatch["operands"]["requested"], "kib");
    assert_eq!(mismatch["operands"]["built"]["value"], "kid");
    let without_rewrite = parse(GRAMMAR, "kib", &[]);
    assert!(without_rewrite
        .iter()
        .all(|row| row["reason"] != "surface_mismatch"));
}

#[test]
fn template_lookups_and_stops_have_separate_witnesses() {
    let template = template_grammar();
    let data = parse(&template, "kib", &[]);
    let found = event(&data, "lookup_completed", "analysis");
    let completion = event(&data, "applicable_templates_not_applied", "synthesis");
    assert_ne!(found["pathId"], completion["pathId"]);
    event(&data, "required_slot_unfilled", "analysis");
    let slot = event(&data, "required_slot_unfilled", "synthesis");
    assert_eq!(slot["objectRefs"][0]["kind"], "slot");
    let mapping = data
        .iter()
        .find(|row| row["object"]["key"] == slot["objectRefs"][0]["key"])
        .unwrap();
    assert_eq!(mapping["object"]["address"]["compiledSlotIndex"], 0);
    assert_eq!(mapping["object"]["bindingStatus"], "unavailable");

    let nonfinal = nonfinal_grammar(&template);
    event(
        &parse(&nonfinal, "kibes", &[]),
        "nonfinal_template_last",
        "synthesis",
    );
    let ordinary_final = parse(&template, "kibes", &[]);
    assert!(ordinary_final
        .iter()
        .all(|row| row["reason"] != "nonfinal_template_last"));
}

#[test]
fn lookup_feature_rejection_names_the_mismatching_entry() {
    let xml = lexical_pos_clash_grammar();
    let suffixed = parse(&xml, "kibes", &[]);
    let rejected = event(&suffixed, "lookup_features", "analysis");
    assert_eq!(rejected["kind"], "reject");
    assert_eq!(rejected["stage"], "lookup");
    assert_eq!(rejected["objectRefs"][0]["role"], "subject");
    assert_eq!(rejected["objectRefs"][0]["kind"], "entry");
    assert_eq!(rejected["objectRefs"][1]["role"], "candidate");
    assert_eq!(rejected["objectRefs"][1]["kind"], "allomorph");
    let operands = &rejected["operands"];
    assert_eq!(operands["kind"], "features");
    assert_eq!(operands["status"], "captured");
    let required = &operands["required"][0];
    let actual = &operands["actual"][0];
    assert_eq!(required["polarity"], "required");
    assert_eq!(required["feature"]["domain"], "syntactic");
    assert_eq!(required["feature"]["path"], actual["path"]);
    assert_ne!(required["feature"]["values"], actual["values"]);
    assert_eq!(suffixed[suffixed.len() - 2]["search"]["outputCount"], 1);
    let candidate_key = &rejected["objectRefs"][1]["key"];
    let surviving = suffixed.iter().find(|row| row["type"] == "output").unwrap();
    assert_ne!(&surviving["morphs"][0]["allomorph"]["key"], candidate_key);
    let bare = parse(&xml, "kib", &[]);
    assert_eq!(bare[bare.len() - 2]["search"]["outputCount"], 2);
    assert!(bare.iter().any(
        |row| row["type"] == "output" && &row["morphs"][0]["allomorph"]["key"] == candidate_key
    ));
    assert!(bare.iter().all(|row| row["reason"] != "lookup_features"));
}

#[test]
fn syntactic_and_mpr_features_resolve_to_distinct_mappings() {
    let xml = lexical_pos_clash_grammar()
        .replace("<StemNames>", r#"<MorphologicalPhonologicalRuleFeatures>
            <MorphologicalPhonologicalRuleFeature id="mprRoot">Synthetic</MorphologicalPhonologicalRuleFeature>
        </MorphologicalPhonologicalRuleFeatures><StemNames>"#)
        .replace(r#"<LexicalEntry id="eKib" partOfSpeech="posV">"#,
                 r#"<LexicalEntry id="eKib" partOfSpeech="posV" ruleFeatures="mprRoot">"#)
        .replace(r#"<MorphologicalInput><PhoneticSequence id="stemTense">"#,
                 r#"<MorphologicalInput excludedMPRFeatures="mprRoot"><PhoneticSequence id="stemTense">"#);
    let data = parse(&xml, "kibes", &["--trace-max-paths", "64"]);
    let syntactic = event(&data, "lookup_features", "analysis");
    let mpr = event(&data, "excluded_mpr_features", "analysis");
    let reference = |event: &Value| event["operands"]["required"][0]["feature"]["path"][0].clone();
    assert_eq!(reference(syntactic)["kind"], "feature");
    assert_eq!(reference(mpr)["kind"], "feature");
    assert_ne!(reference(syntactic)["key"], reference(mpr)["key"]);
}

const AMBIGUOUS_ROOTLESS_WORD: &str = "zzzs";

#[test]
fn early_root_rejection_names_the_stratum() {
    let data = parse(&early_root_grammar(), AMBIGUOUS_ROOTLESS_WORD, &[]);
    let rejected = event(&data, "root_not_viable", "analysis");
    assert_eq!(rejected["kind"], "reject");
    assert_eq!(rejected["stage"], "lookup");
    assert_eq!(rejected["objectRefs"].as_array().unwrap().len(), 1);
    assert_eq!(rejected["objectRefs"][0]["role"], "subject");
    assert_eq!(rejected["objectRefs"][0]["kind"], "stratum");
    assert_eq!(
        rejected["operands"],
        json!({"kind": "lookup", "status": "unavailable", "materializedCount": null, "root": null})
    );
    assert_eq!(data[data.len() - 2]["search"]["status"], "complete");
    assert_eq!(data[data.len() - 2]["search"]["outputCount"], 0);
}

#[test]
fn final_confirmation_counts_the_rules_left_after_an_open_stratum() {
    let data = parse(&nonblocking_grammar(), "kibes", &[]);
    let rejected = event(&data, "remaining_analyzed_rules", "synthesis");
    assert_eq!(rejected["kind"], "reject");
    assert_eq!(rejected["stage"], "completion");
    assert_eq!(rejected["objectRefs"].as_array().unwrap().len(), 1);
    assert_eq!(rejected["objectRefs"][0]["role"], "subject");
    assert_eq!(rejected["objectRefs"][0]["kind"], "stratum");
    assert_eq!(
        rejected["operands"],
        json!({"kind": "count", "status": "captured", "required": 0, "actual": 1})
    );
    assert_eq!(data[data.len() - 2]["search"]["outputCount"], 1);
    let blocking = parse(GRAMMAR, "kibes", &[]);
    assert!(blocking
        .iter()
        .all(|row| row["reason"] != "remaining_analyzed_rules"));
}

#[test]
fn terminal_controls_survive_each_independent_limit() {
    let xml = bound_grammar();
    for (option, value, cut) in [
        ("--trace-max-paths", "1", "paths"),
        ("--trace-max-events", "1", "events"),
        ("--trace-max-bytes", "1024", "bytes"),
    ] {
        let data = parse(&xml, "kib", &[option, value]);
        let end = &data[data.len() - 2];
        assert_eq!(end["search"]["status"], "complete");
        assert_eq!(end["search"]["outputCount"], 0);
        assert_eq!(end["capture"]["truncatedBy"], json!([cut]));
        assert!(end["capture"]["omittedEvents"]["value"].as_u64().unwrap() > 0);
    }
    for (option, value, reason) in [
        ("--work-cap", "1", "work_limit"),
        ("--word-timeout-ms", "0", "time_limit"),
    ] {
        let data = parse_with_search(&xml, "kib", &[option, value], &[]);
        let end = &data[data.len() - 2];
        assert_eq!(end["search"]["status"], "stopped");
        assert_eq!(end["search"]["stopReasons"], json!([reason]));
        assert_eq!(end["capture"]["truncatedBy"], json!([]));
        assert_eq!(data.last().unwrap()["status"], "complete");
    }
}

#[test]
fn artifacts_are_input_ordered_and_cold_work_is_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let grammar = temp.path().join("grammar.xml");
    let words = temp.path().join("words");
    let tsv = temp.path().join("result.tsv");
    let artifact = temp.path().join("trace.jsonl");
    let request = temp.path().join("request.json");
    fs::write(&grammar, GRAMMAR).unwrap();
    fs::write(&words, "fom\nkib\nkib\n!\n").unwrap();
    fs::write(
        &request,
        serde_json::to_vec(&json!({
            "schemaVersion": "pangloss.diagnostic.v1",
            "targets": [{"row": 2, "requestId": "\"".repeat(127), "selector": {
                "rootIndex": 0, "morphs": [{"kind": "unavailable"}]
            }}]
        }))
        .unwrap(),
    )
    .unwrap();
    let batch = |threads, trace| {
        if tsv.exists() {
            fs::remove_file(&tsv).unwrap();
        }
        let mut args = vec![
            "batch",
            grammar.to_str().unwrap(),
            words.to_str().unwrap(),
            tsv.to_str().unwrap(),
            "--start",
            "1",
            "--threads",
            threads,
            "--work-columns",
            "--no-flush",
        ];
        if trace {
            args.extend([
                "--trace-rejections",
                artifact.to_str().unwrap(),
                "--trace-options",
                request.to_str().unwrap(),
            ]);
        }
        success(&run(&args));
        let mut result: Vec<_> = fs::read_to_string(&tsv)
            .unwrap()
            .lines()
            .filter_map(|line| {
                let mut fields: Vec<_> = line.split('\t').collect();
                if fields.get(2) == Some(&"STARTED") {
                    return None;
                }
                fields.remove(2);
                Some(fields.join("\t"))
            })
            .collect();
        result.sort_unstable();
        result
    };
    let plain = batch("1", false);
    assert!(!artifact.exists());
    assert_eq!(batch("1", true), plain);
    let sequential = fs::read(&artifact).unwrap();
    assert_eq!(batch("4", true), plain);
    assert_eq!(fs::read(&artifact).unwrap(), sequential);
    let data = records(&artifact);
    let ends: Vec<_> = data
        .iter()
        .filter(|row| row["type"] == "word_end")
        .collect();
    let ordinals = ends
        .iter()
        .map(|row| row["row"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(ordinals, [1, 2, 3]);
    assert_eq!(ends[1]["requestId"], "\"".repeat(127));
    assert_eq!(ends[1]["target"]["error"]["code"], "unsupported_selector");
    assert_eq!(ends[2]["search"]["status"], "invalid");
    assert_eq!(data[0]["scope"]["requestedRowCount"], 3);
    assert_eq!(data.last().unwrap()["unprocessedRows"], 0);
    let evidence_bytes: usize = sequential
        .split_inclusive(|&byte| byte == b'\n')
        .zip(&data)
        .filter(|(_, entry)| {
            !matches!(
                entry["type"].as_str(),
                Some("word_end" | "header" | "footer")
            )
        })
        .map(|(line, _)| line.len())
        .sum();
    assert_eq!(
        data.last().unwrap()["capture"]["evidenceBytes"],
        evidence_bytes
    );
    assert!(!temp.path().read_dir().unwrap().flatten().any(|entry| entry
        .file_name()
        .to_string_lossy()
        .starts_with(".pangloss-diagnostic-")));
    check_joins(&data);
}

#[test]
fn rejects_explicit_depths_outside_the_wire_range() {
    let temp = tempfile::tempdir().unwrap();
    let grammar = temp.path().join("grammar.xml");
    let artifact = temp.path().join("trace.jsonl");
    fs::write(&grammar, GRAMMAR).unwrap();
    for depth in ["0", "257", "nonnumeric"] {
        let out = run(&[
            "parse",
            grammar.to_str().unwrap(),
            "kib",
            "--trace-rejections",
            artifact.to_str().unwrap(),
            "--trace-max-depth",
            depth,
        ]);
        assert!(!out.status.success());
        let stderr = String::from_utf8(out.stderr).unwrap();
        let error: Value = serde_json::from_str(
            stderr
                .trim()
                .strip_prefix("PANGLOSS_DIAGNOSTIC_ERROR ")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(error["code"], "bad_request");
        assert!(!artifact.exists());
    }
}

#[test]
fn simultaneous_path_event_and_byte_limits_are_all_reported() {
    let data = parse(
        GRAMMAR,
        "kibes",
        &[
            "--trace-max-paths",
            "1",
            "--trace-max-events",
            "1",
            "--trace-max-bytes",
            "2800",
        ],
    );
    let end = &data[data.len() - 2];
    assert_eq!(
        end["capture"]["truncatedBy"],
        json!(["paths", "events", "bytes"])
    );
    assert!(
        data.last().unwrap()["capture"]["evidenceBytes"]
            .as_u64()
            .unwrap()
            < 2800 - 128
    );
}

#[test]
fn batch_marks_traced_cost_and_refuses_stats_before_loading() {
    let temp = tempfile::tempdir().unwrap();
    let grammar = temp.path().join("grammar.xml");
    let words = temp.path().join("words.txt");
    let tsv = temp.path().join("rows.tsv");
    let artifact = temp.path().join("trace.jsonl");
    fs::write(&grammar, GRAMMAR).unwrap();
    fs::write(&words, "kib\nzzz\nkib\n").unwrap();
    let args = [
        "batch",
        grammar.to_str().unwrap(),
        words.to_str().unwrap(),
        tsv.to_str().unwrap(),
        "--threads",
        "1",
        "--cost-column",
        "--no-flush",
    ];
    success(&run(&args));
    let plain = fs::read_to_string(&tsv).unwrap();
    let trace = ["--trace-rejections", artifact.to_str().unwrap()];
    success(&run(&[args.as_slice(), &trace].concat()));
    let traced = fs::read_to_string(&tsv).unwrap();
    assert_eq!(plain.lines().count(), traced.lines().count());
    for (plain, traced) in plain.lines().zip(traced.lines()) {
        let plain: Vec<_> = plain.split('\t').collect();
        let traced: Vec<_> = traced.split('\t').collect();
        if plain.get(2) == Some(&"STARTED") {
            assert_eq!(plain, traced);
            continue;
        }
        assert_eq!(plain.len(), 7);
        assert_eq!(traced.len(), 7);
        assert_eq!(plain[..2], traced[..2]);
        assert_eq!(plain[3..6], traced[3..6]);
        assert!(plain[6].parse::<u64>().is_ok());
        assert_eq!(traced[6], "-");
    }

    let artifact_before = fs::read(&artifact).unwrap();
    fs::remove_file(&grammar).unwrap();
    let refused = run(&[args.as_slice(), &trace, &["--stats"]].concat());
    assert!(!refused.status.success());
    let stderr = String::from_utf8(refused.stderr).unwrap();
    assert!(stderr.starts_with("PANGLOSS_DIAGNOSTIC_ERROR "));
    assert!(stderr.contains(r#""code":"unsupported""#));
    assert_eq!(fs::read(&artifact).unwrap(), artifact_before);
    assert_eq!(fs::read_to_string(&tsv).unwrap(), traced);
}

#[test]
fn unsupported_and_publish_errors_leave_existing_destinations_intact() {
    let temp = tempfile::tempdir().unwrap();
    let grammar = temp.path().join("grammar.xml");
    let artifact = temp.path().join("trace.jsonl");
    fs::write(&grammar, GRAMMAR).unwrap();
    fs::write(&artifact, "previous artifact").unwrap();
    let args = [
        "parse",
        grammar.to_str().unwrap(),
        "kib",
        "--trace-rejections",
        artifact.to_str().unwrap(),
    ];
    for (flag, value) in [("--engine", "fst"), ("--prefilter", "auto")] {
        let out = run(&[args.as_slice(), &[flag, value]].concat());
        assert!(!out.status.success());
        let stderr = String::from_utf8(out.stderr).unwrap();
        let error: Value = serde_json::from_str(
            stderr
                .trim()
                .strip_prefix("PANGLOSS_DIAGNOSTIC_ERROR ")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(error["code"], "unsupported");
        assert_eq!(fs::read_to_string(&artifact).unwrap(), "previous artifact");
    }
    let mut args = args;
    args[4] = temp.path().to_str().unwrap();
    let out = run(&args);
    assert!(!out.status.success());
    assert!(String::from_utf8(out.stderr)
        .unwrap()
        .contains("PANGLOSS_DIAGNOSTIC_ERROR "));
    let entries: BTreeSet<_> = temp
        .path()
        .read_dir()
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(entries, ["grammar.xml".into(), "trace.jsonl".into()].into());
}

fn lookup_alternative_grammars() -> [String; 3] {
    let xml = GRAMMAR
        .replace(
            "<StemNames>",
            r#"<MorphologicalPhonologicalRuleFeatures>
                <MorphologicalPhonologicalRuleFeature id="mprRoot">Root set</MorphologicalPhonologicalRuleFeature>
                <MorphologicalPhonologicalRuleFeature id="mprOther">Other set</MorphologicalPhonologicalRuleFeature>
            </MorphologicalPhonologicalRuleFeatures><StemNames>"#,
        )
        .replace(
            r#"<LexicalEntry id="eKib" partOfSpeech="posV">"#,
            r#"<LexicalEntry id="eKib" partOfSpeech="posV" ruleFeatures="mprRoot">"#,
        );
    let required = xml.replace(
        r#"<MorphologicalInput><PhoneticSequence id="stemTense">"#,
        r#"<MorphologicalInput requiredMPRFeatures="mprOther"><PhoneticSequence id="stemTense">"#,
    );
    let excluded = xml.replace(
        r#"<MorphologicalInput><PhoneticSequence id="stemTense">"#,
        r#"<MorphologicalInput excludedMPRFeatures="mprRoot"><PhoneticSequence id="stemTense">"#,
    );
    let stem_name = xml.replace(
        r#"id="mrTense" requiredPartsOfSpeech="#,
        r#"id="mrTense" requiredStemName="snPast" requiredPartsOfSpeech="#,
    );
    [required, excluded, stem_name]
}

#[test]
fn lookup_reports_failed_mpr_or_stem_name_alternatives_as_observations() {
    let [required, excluded, stem_name] = lookup_alternative_grammars();
    let check = |xml: &str, reason: &str| {
        let data = parse(xml, "kibes", &[]);
        let observed = event(&data, reason, "analysis");
        assert_eq!(observed["kind"], "observe");
        assert_eq!(observed["stage"], "lookup");
        assert_eq!(observed["objectRefs"][1]["role"], "candidate");
        observed["operands"].clone()
    };
    let operands = check(&required, "required_mpr_features");
    assert_eq!(operands["actualBasis"], "root_upper_bound");
    assert_ne!(
        operands["required"][0]["feature"]["path"],
        operands["actual"][0]["path"]
    );
    let operands = check(&excluded, "excluded_mpr_features");
    assert_eq!(operands["actualBasis"], "root_lower_bound");
    assert_eq!(
        operands["required"][0]["feature"]["path"],
        operands["actual"][0]["path"]
    );
    let operands = check(&stem_name, "required_stem_name");
    assert_eq!(operands["required"][0]["kind"], "stem_name");
    assert_eq!(operands["actual"], json!([]));

    let start = required
        .find(r#"<MorphologicalSubrule id="subTense">"#)
        .unwrap();
    let end = required[start..].find("</MorphologicalSubrule>").unwrap()
        + start
        + "</MorphologicalSubrule>".len();
    let sibling = required[start..end]
        .replace("subTense", "subTenseSibling")
        .replace("stemTense", "stemTenseSibling")
        .replace(r#" requiredMPRFeatures="mprOther""#, "");
    let mut accepting_sibling = required;
    accepting_sibling.insert_str(end, &sibling);
    let data = parse(&accepting_sibling, "kibes", &[]);
    assert_eq!(
        event(&data, "required_mpr_features", "analysis")["kind"],
        "observe"
    );
    assert_eq!(data[data.len() - 2]["search"]["outputCount"], 1);
}

#[test]
fn describe_advertises_exactly_the_tuples_emitted_by_the_cli() {
    let description = run(&["--describe"]);
    success(&description);
    let description: Value = serde_json::from_slice(&description.stdout).unwrap();
    let tuples = description["interfaces"]["diagnostic"]["eventTuples"]
        .as_array()
        .unwrap();
    let registry: Value = serde_json::from_str(include_str!(
        "../../../../docs/formats/rejection-trace-v1/event-tuples-v1.json"
    ))
    .unwrap();
    for tuple in tuples {
        assert!(registry["tuples"].as_array().unwrap().contains(tuple));
    }
    let advertised: BTreeSet<[String; 5]> = tuples
        .iter()
        .map(|tuple| serde_json::from_value(tuple.clone()).unwrap())
        .collect();
    assert_eq!(
        advertised.len(),
        tuples.len(),
        "duplicate advertised tuples"
    );

    let template = template_grammar();
    let fixtures = [
        parse(GRAMMAR, "kib", &[]),
        parse(GRAMMAR, "zzz", &[]),
        parse(GRAMMAR, "!", &[]),
        parse(GRAMMAR, "kibes", &[]),
        parse(&bound_grammar(), "kib", &[]),
        parse(&rewrite_grammar(), "kib", &[]),
        parse(&template, "kib", &[]),
        parse(&nonfinal_grammar(&template), "kibes", &[]),
        parse(&lexical_pos_clash_grammar(), "kibes", &[]),
        parse(&early_root_grammar(), AMBIGUOUS_ROOTLESS_WORD, &[]),
        parse(&nonblocking_grammar(), "kibes", &[]),
    ];
    let alternatives = lookup_alternative_grammars().map(|xml| parse(&xml, "kibes", &[]));
    let emitted: BTreeSet<_> = fixtures
        .into_iter()
        .chain(alternatives)
        .chain(owner::tuple_fixtures())
        .chain(lineage::tuple_fixtures())
        .flatten()
        .filter(|record| record["type"] == "event")
        .map(|event| {
            [
                &event["direction"],
                &event["kind"],
                &event["stage"],
                &event["reason"],
                &event["operands"]["kind"],
            ]
            .map(|value| value.as_str().unwrap().to_owned())
        })
        .collect();
    let missing: Vec<_> = advertised.difference(&emitted).collect();
    let unexpected: Vec<_> = emitted.difference(&advertised).collect();
    assert!(
        missing.is_empty() && unexpected.is_empty(),
        "advertised but never emitted: {missing:?}\nemitted but never advertised: {unexpected:?}"
    );
}
