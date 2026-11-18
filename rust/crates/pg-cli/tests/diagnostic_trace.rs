//! Rejection diagnostics through the ordinary parser and batch writer.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");
const GRAMMAR: &str = include_str!("data/trace-family.xml");

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
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
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn parse(xml: &str, word: &str, extra: &[&str]) -> Vec<Value> {
    let temp = tempfile::tempdir().unwrap();
    let grammar = temp.path().join("grammar.xml");
    let artifact = temp.path().join("trace.jsonl");
    fs::write(&grammar, xml).unwrap();
    let mut args = vec!["parse", grammar.to_str().unwrap(), word];
    let plain = run(&args);
    args.extend(["--trace-rejections", artifact.to_str().unwrap()]);
    args.extend(extra);
    let observed = run(&args);
    if extra.is_empty() {
        assert_eq!(observed.stdout, plain.stdout);
        assert_eq!(observed.status, plain.status);
    }
    let stderr = String::from_utf8(observed.stderr).unwrap();
    let summary = stderr
        .lines()
        .find(|s| s.starts_with("rejection trace: "))
        .unwrap();
    assert!(summary.len() < 2048);
    let data = records(&artifact);
    assert_eq!(data[0]["schemaVersion"], "pangloss.diagnostic.v1");
    assert_eq!(data.last().unwrap()["processedRows"], 1);
    check_joins(&data);
    data
}

fn check_joins(data: &[Value]) {
    let mut mappings = BTreeMap::new();
    let mut paths = BTreeMap::new();
    let mut events = BTreeMap::new();
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
                assert_eq!(row["parentStatus"], "unavailable");
                assert!(row["parentPathId"].is_null() && row["depth"].is_null());
            }
            "event" => {
                assert!(events
                    .insert((ordinal, row["eventId"].as_u64()), row)
                    .is_none());
            }
            _ => {}
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
        if event["kind"] == "reject" {
            assert_eq!(objects.iter().filter(|r| r["role"] == "subject").count(), 1);
            assert_eq!(path["outcome"], "rejected");
            assert_eq!(path["firstRejectEventId"].as_u64(), event_id);
        } else {
            assert!(path["firstRejectEventId"].is_null());
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
    let rewrite = GRAMMAR.replace("<Strata>", SWAP).replace(
        r#"morphologicalRuleOrder="linear""#,
        r#"phonologicalRules="prSwap" morphologicalRuleOrder="linear""#,
    );
    let discordant = parse(&rewrite, "kib", &[]);
    let mismatch = event(&discordant, "surface_mismatch", "synthesis");
    assert_eq!(mismatch["operands"]["requested"], "kib");
    assert_eq!(mismatch["operands"]["built"]["value"], "kid");
    let without_rewrite = parse(GRAMMAR, "kib", &[]);
    assert!(without_rewrite
        .iter()
        .all(|row| row["reason"] != "surface_mismatch"));
    let registered: Value =
        serde_json::from_str(&String::from_utf8(run(&["--describe"]).stdout).unwrap()).unwrap();
    let registry: Value = serde_json::from_str(include_str!(
        "../../../../docs/formats/rejection-trace-v1/event-tuples-v1.json"
    ))
    .unwrap();
    for tuple in registered["interfaces"]["diagnostic"]["eventTuples"]
        .as_array()
        .unwrap()
    {
        assert!(registry["tuples"].as_array().unwrap().contains(tuple));
    }
}

#[test]
fn template_lookups_and_stops_have_separate_witnesses() {
    let template = GRAMMAR
        .replace("mrTense rrPast mrPast2 mrPl", "rrPast mrPast2 mrPl")
        .replace("<LexicalEntries>", TEMPLATE);
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

    let nonfinal = template.replace(
        r#"<AffixTemplate requiredPartsOfSpeech="posV">"#,
        r#"<AffixTemplate requiredPartsOfSpeech="posV" final="false">"#,
    );
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
        let data = parse(&xml, "kib", &[option, value]);
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
    for (flag, value) in [
        ("--trace-max-depth", "32"),
        ("--engine", "fst"),
        ("--prefilter", "auto"),
    ] {
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

#[test]
fn lookup_reports_failed_mpr_or_stem_name_alternatives_as_observations() {
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
    let check = |xml: &str, reason: &str| {
        let data = parse(xml, "kibes", &[]);
        let observed = event(&data, reason, "analysis");
        assert_eq!(observed["kind"], "observe");
        assert_eq!(observed["stage"], "lookup");
        assert_eq!(observed["objectRefs"][1]["role"], "candidate");
        observed["operands"].clone()
    };

    let required = xml.replace(
        r#"<MorphologicalInput><PhoneticSequence id="stemTense">"#,
        r#"<MorphologicalInput requiredMPRFeatures="mprOther"><PhoneticSequence id="stemTense">"#,
    );
    let operands = check(&required, "required_mpr_features");
    assert_eq!(operands["actualBasis"], "root_upper_bound");
    assert_ne!(
        operands["required"][0]["feature"]["path"],
        operands["actual"][0]["path"]
    );

    let excluded = xml.replace(
        r#"<MorphologicalInput><PhoneticSequence id="stemTense">"#,
        r#"<MorphologicalInput excludedMPRFeatures="mprRoot"><PhoneticSequence id="stemTense">"#,
    );
    let operands = check(&excluded, "excluded_mpr_features");
    assert_eq!(operands["actualBasis"], "root_lower_bound");
    assert_eq!(
        operands["required"][0]["feature"]["path"],
        operands["actual"][0]["path"]
    );

    let stem_name = xml.replace(
        r#"id="mrTense" requiredPartsOfSpeech="#,
        r#"id="mrTense" requiredStemName="snPast" requiredPartsOfSpeech="#,
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
