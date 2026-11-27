use std::collections::BTreeMap;
use std::fs;

use serde_json::{json, Value};

use super::{event, run, success, trace_parse, GRAMMAR};

const SNAPSHOT: &str =
    include_str!("../../../../../docs/divergences/evidence/071/kat-snapshot.json");
const SCHEMA: &str = "pangloss.diagnostic.v1";
const ROOT: &str = "kkt";
const SUFFIXED: &str = "kkaas";

fn guid(index: u8) -> String {
    format!("00000000-0000-0000-0000-0000000000{index:02x}")
}

fn snapshot(shared_sense: bool) -> Value {
    let mut snapshot: Value = serde_json::from_str(SNAPSHOT).unwrap();
    let entries = snapshot["lexicon"]["entries"].as_array_mut().unwrap();
    for (entry, spelling) in entries.iter_mut().zip([ROOT, "kkaa"]) {
        entry["citationForm"][0]["form"] = json!(spelling);
        entry["allomorphs"][0]["forms"][0]["form"] = json!(spelling);
    }
    if shared_sense {
        let senses = entries[0]["senses"].as_array_mut().unwrap();
        let mut second = senses[0].clone();
        second["guid"] = json!(guid(0x34));
        senses.push(second);
    }
    snapshot
}

fn authored(entry: u8) -> Value {
    json!({
        "kind": "authored",
        "entryGuid": guid(entry),
        "msaGuid": guid(entry + 2),
        "senseGuid": guid(entry + 3),
        "formGuids": [guid(entry + 1)],
    })
}

fn feature(value: u8) -> Value {
    json!({
        "domain": "syntactic",
        "featurePathGuids": [guid(0x07)],
        "valueGuids": [guid(value)],
        "match": "exact",
    })
}

fn selected(
    source: &str,
    extension: &str,
    word: &str,
    selector: &Value,
    limits: &[&str],
) -> Vec<Value> {
    let data = selected_with_search(source, extension, word, selector, &[], limits);
    assert_eq!(completion(&data)["search"]["status"], "complete");
    data
}

fn selected_with_search(
    source: &str,
    extension: &str,
    word: &str,
    selector: &Value,
    search: &[&str],
    limits: &[&str],
) -> Vec<Value> {
    let temp = tempfile::tempdir().unwrap();
    let grammar = temp.path().join("grammar").with_extension(extension);
    let artifact = temp.path().join("trace.jsonl");
    let request = temp.path().join("request.json");
    fs::write(&grammar, source).unwrap();
    fs::write(
        &request,
        json!({
            "schemaVersion": SCHEMA,
            "targets": [{"row": 0, "requestId": "target-fixture", "selector": selector}]
        })
        .to_string(),
    )
    .unwrap();
    let mut options = vec!["--trace-options", request.to_str().unwrap()];
    options.extend(limits);
    let data = trace_parse(&grammar, &artifact, word, search, &options);
    assert_eq!(completion(&data)["requestId"], "target-fixture");
    data
}

fn completion(data: &[Value]) -> &Value {
    let record = &data[data.len() - 2];
    assert_eq!(record["type"], "word_end");
    record
}

fn output<'a>(data: &'a [Value], id: &Value) -> &'a Value {
    data.iter()
        .find(|row| row["type"] == "output" && &row["outputId"] == id)
        .expect("matchedOutputIds must refer to a retained output")
}

fn mapping<'a>(data: &'a [Value], reference: &Value) -> &'a Value {
    data.iter()
        .find(|row| {
            row["type"] == "mapping"
                && row["object"]["kind"] == reference["kind"]
                && row["object"]["key"] == reference["key"]
        })
        .map(|row| &row["object"])
        .expect("object reference must resolve to a mapping")
}

#[test]
fn related_links_do_not_count_as_subject_bindings() {
    let mut source = snapshot(false);
    let environment = source["phonology"]["environments"][0]["guid"].clone();
    source["lexicon"]["entries"][0]["allomorphs"][0]["environments"] = json!([environment]);
    let verb = &mut source["morphology"]["partsOfSpeech"][1];
    verb["affixSlots"].as_array_mut().unwrap().push(json!({
        "guid": guid(0x65), "name": "Unused synthetic slot", "optional": true,
    }));
    verb["affixTemplates"][0]["suffixSlots"] = json!([guid(0x65), guid(0x0d)]);

    let temp = tempfile::tempdir().unwrap();
    let grammar = temp.path().join("grammar.json");
    let artifact = temp.path().join("trace.jsonl");
    let related = [
        json!({"kind": "entry", "guid": guid(0x30), "relation": "owning_entry", "ordinal": 0}),
        json!({"kind": "msa", "guid": guid(0x32), "relation": "msa", "ordinal": 0}),
    ];
    let source = source.to_string();
    let unverified = source
        .replace(&guid(0x31), "unverified-form")
        .replace(&guid(0x0d), "unverified-slot");
    for (source, binding, slot_sources, allomorph_sources) in [
        (
            source,
            "exact",
            json!([{"kind": "slot", "guid": guid(0x0d), "relation": "slot", "ordinal": 1}]),
            json!([
                {"kind": "form", "guid": guid(0x31), "relation": "form_part", "ordinal": 0},
                related[0], related[1],
            ]),
        ),
        (unverified, "unavailable", json!([]), json!(related)),
    ] {
        fs::write(&grammar, source).unwrap();
        let data = trace_parse(&grammar, &artifact, ROOT, &[], &[]);
        let rejection = event(&data, "environment", "synthesis");
        let allomorph = mapping(&data, &rejection["objectRefs"][0]);
        assert_eq!(allomorph["bindingStatus"], binding);
        assert_eq!(allomorph["sources"], allomorph_sources);
        let rejection = event(&data, "required_slot_unfilled", "analysis");
        let slot = mapping(&data, &rejection["operands"]["slot"]);
        let template = mapping(&data, &rejection["operands"]["template"]);
        assert_eq!(slot["address"]["compiledSlotIndex"], 0);
        assert_eq!(slot["address"]["templateKey"], template["key"]);
        assert_eq!(slot["bindingStatus"], binding);
        assert_eq!(slot["sources"], slot_sources);
        assert_eq!(template["key"], guid(0x0e));
        assert_eq!(template["bindingStatus"], "exact");
    }
}

#[test]
fn describe_advertises_authored_target_selectors() {
    let description = run(&["--describe"]);
    success(&description);
    let value: Value = serde_json::from_slice(&description.stdout).unwrap();
    let diagnostic = &value["interfaces"]["diagnostic"];
    assert_eq!(
        diagnostic["selectors"],
        json!(["authored_morphs", "structural_zero", "syntactic_features"])
    );
    assert_eq!(diagnostic["maxFeaturePathDepth"], 8);
}

#[test]
fn exact_membership_and_count_survive_omission_of_the_output_witness() {
    let source = snapshot(false).to_string();
    let selector = json!({"rootIndex": 0, "morphs": [authored(0x30)], "features": [feature(0x09)]});
    let complete = selected(&source, "json", ROOT, &selector, &[]);
    let end = completion(&complete);
    let target = &end["target"];
    assert_eq!(target["status"], "found");
    assert_eq!(target["membership"], "found");
    assert_eq!(
        target["matchCount"],
        json!({"value": 1, "accuracy": "exact"})
    );
    assert_eq!(target["matchedOutputIds"], json!([0]));
    let matched = output(&complete, &target["matchedOutputIds"][0]);
    assert_eq!(matched["rootIndex"], 0);
    assert_eq!(matched["morphs"][0]["entry"]["key"], guid(0x30));
    assert_eq!(matched["morphs"][0]["msa"]["key"], guid(0x32));
    assert_eq!(matched["morphs"][0]["forms"][0]["key"], guid(0x31));
    assert_eq!(
        matched["morphs"][0]["senseResolution"],
        "unique_association"
    );
    assert_eq!(
        matched["morphs"][0]["associatedSenseGuids"],
        json!([guid(0x33)])
    );

    let limited = selected(
        &source,
        "json",
        ROOT,
        &selector,
        &["--trace-max-bytes", "1024"],
    );
    let end = completion(&limited);
    assert_eq!(end["search"]["outputCount"], 1);
    assert_eq!(end["target"]["status"], "found");
    assert_eq!(end["target"]["membership"], "found");
    assert_eq!(end["target"]["matchCount"], target["matchCount"]);
    assert_eq!(end["target"]["matchedOutputIds"], json!([]));
    assert_eq!(end["capture"]["omittedOutputs"]["value"], 1);
    assert!(limited.iter().all(|row| row["type"] != "output"));

    let mut opposite = selector;
    opposite["features"] = json!([feature(0x08)]);
    let absent = selected(&source, "json", ROOT, &opposite, &[]);
    let end = completion(&absent);
    assert_eq!(end["search"]["outputCount"], 1);
    assert_eq!(end["target"]["membership"], "absent");
    assert_eq!(
        end["target"]["matchCount"],
        json!({"value": 0, "accuracy": "exact"})
    );
}

#[test]
fn ordered_authored_chain_distinguishes_the_same_morphs_in_reverse_order() {
    let source = snapshot(false).to_string();
    let root = authored(0x40);
    let suffix = authored(0x50);
    let matching = json!({"rootIndex": 0, "morphs": [root, suffix]});
    let reversed = json!({"rootIndex": 1, "morphs": [suffix, root]});
    let accepted = selected(&source, "json", SUFFIXED, &matching, &[]);
    let end = completion(&accepted);
    assert_eq!(end["target"]["status"], "found");
    assert_eq!(end["target"]["matchedOutputIds"], json!([0]));
    let output = output(&accepted, &json!(0));
    assert_eq!(output["rootIndex"], 0);
    assert_eq!(output["morphs"][0]["entry"]["key"], guid(0x40));
    assert_eq!(output["morphs"][1]["entry"]["key"], guid(0x50));

    let exact = accepted
        .iter()
        .find(|row| row["type"] == "path" && row["outcome"] == "accepted")
        .unwrap();
    assert_eq!(exact["target"]["relation"], "exact");
    let occurrences = exact["target"]["boundOccurrences"].as_array().unwrap();
    assert_eq!(occurrences.len(), 2);
    for (index, occurrence) in occurrences.iter().enumerate() {
        assert_eq!(occurrence["index"], index);
        assert_eq!(occurrence["identity"], output["morphs"][index]);
    }

    let partial = accepted
        .iter()
        .find(|row| {
            row["type"] == "path"
                && row["outcome"] == "rejected"
                && row["target"]["relation"] == "compatible_partial"
        })
        .unwrap();
    assert_eq!(
        partial["target"]["boundOccurrences"],
        json!([occurrences[0]])
    );

    let data = selected(&source, "json", SUFFIXED, &reversed, &[]);
    let end = completion(&data);
    assert_eq!(end["search"]["outputCount"], 1);
    assert_eq!(end["target"]["membership"], "absent");
    assert_eq!(end["target"]["matchedOutputIds"], json!([]));

    let root_only = json!({"rootIndex": 0, "morphs": [authored(0x40)]});
    let data = selected(&source, "json", SUFFIXED, &root_only, &[]);
    let extra = data
        .iter()
        .find(|row| row["type"] == "path" && row["outcome"] == "accepted")
        .unwrap();
    assert_eq!(extra["target"]["relation"], "different");
    assert_eq!(extra["target"]["incompatibleOccurrences"], json!([]));
}

#[test]
fn shared_msa_keeps_sense_membership_unknown() {
    let source = snapshot(true).to_string();
    let selector = json!({"rootIndex": 0, "morphs": [authored(0x30)]});
    let data = selected(&source, "json", ROOT, &selector, &[]);
    let end = completion(&data);
    assert_eq!(end["search"]["outputCount"], 1);
    assert_eq!(end["target"]["membership"], "unknown");
    assert_eq!(end["target"]["status"], "unknown");
    assert_eq!(end["target"]["coverage"], "incomplete");
    assert_eq!(
        end["target"]["matchCount"],
        json!({"value": 0, "accuracy": "lowerBound"})
    );
    assert!(end["target"]["limitations"]
        .as_array()
        .unwrap()
        .contains(&json!("sense_ambiguous")));
    assert_eq!(end["target"]["matchedOutputIds"], json!([]));
    let output = output(&data, &json!(0));
    assert_eq!(output["morphs"][0]["senseResolution"], "shared_msa");
    assert_eq!(
        output["morphs"][0]["associatedSenseGuids"],
        json!([guid(0x33), guid(0x34)])
    );

    let mut without_sense = selector;
    without_sense["morphs"][0]["senseGuid"] = Value::Null;
    let data = selected(&source, "json", ROOT, &without_sense, &[]);
    assert_eq!(completion(&data)["target"]["membership"], "found");
}

#[test]
fn structural_zero_resolves_to_the_authored_inflection_type() {
    let mut source = snapshot(false);
    let written = "kkak";
    let inflection_type = guid(0x60);
    source["morphology"]["lexEntryInflTypes"] = json!([{
        "guid": inflection_type, "name": "Synthetic irregular", "abbreviation": "irr",
        "glossPrepend": "", "glossAppend": "", "slots": [guid(0x0d)],
    }]);
    source["lexicon"]["entries"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "guid": guid(0x61), "citationForm": [{"ws": "fx", "form": written}],
            "lexemeMorphType": "stem",
            "msas": [], "senses": [],
            "allomorphs": [{"guid": guid(0x62), "morphType": "stem", "isAbstract": false,
                "forms": [{"ws": "fx", "form": written}]}],
            "entryRefs": [{"kind": "variant", "guid": guid(0x63),
                "component_lexemes": [guid(0x40)], "variant_entry_types": [inflection_type]}],
        }));
    let selector = json!({"rootIndex": 0, "morphs": [
        {"kind": "authored", "entryGuid": guid(0x61), "msaGuid": guid(0x42),
            "formGuids": [guid(0x62)], "inflTypeGuid": inflection_type},
        {"kind": "structural_zero", "inflTypeGuid": inflection_type}
    ]});
    let data = selected(&source.to_string(), "json", written, &selector, &[]);
    let target = &completion(&data)["target"];
    assert_eq!(target["membership"], "found");
    assert_eq!(target["matchedOutputIds"], json!([0]));
    let output = output(&data, &target["matchedOutputIds"][0]);
    assert_eq!(output["morphs"][1]["kind"], "structural_zero");
    assert_eq!(
        output["morphs"][1]["inflectionType"]["key"],
        inflection_type
    );
}

#[test]
fn terminal_environment_witness_explains_an_exact_target_rejection() {
    let mut source = snapshot(false);
    let environment = source["phonology"]["environments"][0]["guid"].clone();
    source["phonology"]["rules"] = json!([]);
    source["lexicon"]["entries"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    source["lexicon"]["entries"][0]["allomorphs"][0]["environments"] = json!([environment]);
    source["morphology"]["partsOfSpeech"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    source["morphology"]["parserParameters"]["strata"] = json!("Morphology");
    let source = source.to_string();
    let selector = json!({"rootIndex": 0, "morphs": [authored(0x30)]});

    let full = selected(&source, "json", ROOT, &selector, &[]);
    let end = completion(&full);
    let target = &end["target"];
    assert_eq!(end["search"]["outputCount"], 0);
    assert_eq!(target["membership"], "absent");
    assert_eq!(target["status"], "rejected");
    assert_eq!(target["coverage"], "exhausted");
    let rejected = event(&full, "environment", "synthesis");
    let path = full
        .iter()
        .find(|row| row["type"] == "path" && row["pathId"] == rejected["pathId"])
        .unwrap();
    assert_eq!(path["firstRejectEventId"], rejected["eventId"]);
    assert_eq!(path["target"]["relation"], "exact");
    let bound = &path["target"]["boundOccurrences"][0];
    assert_eq!(bound["index"], 0);
    assert_eq!(bound["identity"]["entry"]["key"], guid(0x30));

    let one_event = selected(
        &source,
        "json",
        ROOT,
        &selector,
        &["--trace-max-events", "1"],
    );
    let one_event_end = completion(&one_event);
    assert_eq!(one_event_end["target"]["status"], "rejected");
    assert_eq!(one_event_end["capture"]["truncatedBy"], json!(["events"]));
    assert_eq!(
        one_event
            .iter()
            .filter(|row| row["type"] == "event")
            .count(),
        1
    );
    event(&one_event, "environment", "synthesis");

    let limited = selected(
        &source,
        "json",
        ROOT,
        &selector,
        &["--trace-max-bytes", "1024"],
    );
    let limited_end = completion(&limited);
    let limited_target = &limited_end["target"];
    assert_eq!(limited_end["search"], end["search"]);
    assert_eq!(limited_target["membership"], "absent");
    assert_eq!(limited_target["status"], "unknown");
    assert_eq!(limited_target["matchCount"], target["matchCount"]);
    assert!(limited_target["limitations"]
        .as_array()
        .unwrap()
        .contains(&json!("witness_truncated")));
    assert_eq!(limited_end["capture"]["truncatedBy"], json!(["bytes"]));
    assert!(limited.iter().all(|row| row["reason"] != "environment"));
}

#[test]
fn unsupported_targets_leave_the_ordinary_parse_intact() {
    let source = snapshot(false).to_string();
    let mut invalid_owner = authored(0x30);
    invalid_owner["msaGuid"] = json!(guid(0x42));
    let mut unsupported_domain = feature(0x09);
    unsupported_domain["domain"] = json!("realizational");
    let mut unsupported_part = authored(0x30);
    unsupported_part["parts"] = json!([{"formGuid": guid(0x31), "placement": "root"}]);
    for (source, extension, word, selector) in [
        (
            GRAMMAR,
            "xml",
            "kib",
            json!({"rootIndex": 0, "morphs": [authored(0x30)]}),
        ),
        (
            &source,
            "json",
            ROOT,
            json!({"rootIndex": 0, "morphs": [invalid_owner]}),
        ),
        (
            &source,
            "json",
            ROOT,
            json!({"rootIndex": 0, "morphs": [authored(0x30)], "features": [unsupported_domain]}),
        ),
        (
            &source,
            "json",
            ROOT,
            json!({"rootIndex": 0, "morphs": [unsupported_part]}),
        ),
    ] {
        let data = selected(source, extension, word, &selector, &[]);
        let end = completion(&data);
        assert_eq!(end["search"]["outputCount"], 1);
        assert_eq!(end["target"]["error"]["code"], "unsupported_selector");
        assert_eq!(end["target"]["coverage"], "not_assessed");
        assert_eq!(end["target"]["membership"], "unknown");
    }
}

#[test]
fn an_infix_does_not_duplicate_its_split_root() {
    let mut source = snapshot(false);
    let affix = &mut source["lexicon"]["entries"][2];
    affix["lexemeMorphType"] = json!("infix");
    affix["allomorphs"][0]["morphType"] = json!("infix");
    affix["allomorphs"][0]["positions"] = affix["allomorphs"][0]
        .as_object_mut()
        .unwrap()
        .remove("environments")
        .unwrap();
    source["phonology"]["environments"][0]["representation"] = json!("/k_a");
    source["phonology"]["rules"] = json!([]);
    let source = source.to_string();
    let root = authored(0x40);
    let infix = authored(0x50);
    let selector = json!({"rootIndex": 1, "morphs": [infix, root]});
    let data = selected(&source, "json", "kksaa", &selector, &[]);
    let end = completion(&data);
    assert_eq!(end["search"]["outputCount"], 1);
    let reading = output(&data, &json!(0));
    assert_eq!(reading["morphs"].as_array().unwrap().len(), 2);
    assert_eq!(reading["rootIndex"], 1);
    assert_eq!(reading["morphs"][0]["entry"]["key"], guid(0x50));
    assert_eq!(reading["morphs"][1]["entry"]["key"], guid(0x40));
    assert_eq!(end["target"]["membership"], "found");
    let reversed = json!({"rootIndex": 0, "morphs": [root, infix]});
    let data = selected(&source, "json", "kksaa", &reversed, &[]);
    assert_eq!(completion(&data)["target"]["membership"], "absent");
}

#[test]
fn merged_stem_msas_keep_both_authored_sense_associations() {
    let mut source = snapshot(false);
    let entry = &mut source["lexicon"]["entries"][0];
    let mut msa = entry["msas"][0].clone();
    msa["guid"] = json!(guid(0x35));
    let mut sense = entry["senses"][0].clone();
    sense["guid"] = json!(guid(0x34));
    sense["msa"] = msa["guid"].clone();
    entry["msas"].as_array_mut().unwrap().push(msa);
    entry["senses"].as_array_mut().unwrap().push(sense);
    source["lexicon"]["msaEquality"] = json!([
        {"msa": guid(0x32), "equalTo": [guid(0x35)]},
        {"msa": guid(0x35), "equalTo": [guid(0x32)]},
    ]);

    for merged in [false, true] {
        source["morphology"]["parserParameters"]["mergeMsas"] = json!(merged);
        for (msa, sense) in [(0x32, 0x33), (0x35, 0x34)] {
            let mut morph = authored(0x30);
            morph["msaGuid"] = json!(guid(if merged { 0x32 } else { msa }));
            morph["senseGuid"] = json!(guid(sense));
            let selector = json!({"rootIndex": 0, "morphs": [morph]});
            let data = selected(&source.to_string(), "json", ROOT, &selector, &[]);
            let end = completion(&data);
            assert_eq!(end["search"]["outputCount"], if merged { 1 } else { 2 });
            if merged {
                assert_eq!(end["target"]["membership"], "unknown");
                assert!(end["target"]["limitations"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("sense_ambiguous")));
                let reading = output(&data, &json!(0));
                assert_eq!(reading["morphs"][0]["senseResolution"], "shared_msa");
                assert_eq!(
                    reading["morphs"][0]["associatedSenseGuids"],
                    json!([guid(0x33), guid(0x34)])
                );
            } else {
                assert_eq!(end["target"]["membership"], "found");
            }
        }
    }
}

#[test]
fn interrupted_surface_matching_does_not_publish_a_terminal_rejection() {
    let source = snapshot(false).to_string();
    let selector = json!({"rootIndex": 0, "morphs": [authored(0x30)]});
    for cap in ["176", "177", "178", "179"] {
        let data =
            selected_with_search(&source, "json", ROOT, &selector, &["--work-cap", cap], &[]);
        let end = completion(&data);
        assert_eq!(end["search"]["status"], "stopped");
        assert_eq!(end["target"]["membership"], "unknown");
        assert!(end["target"]["limitations"]
            .as_array()
            .unwrap()
            .contains(&json!("search_stopped")));
        assert!(
            data.iter().all(|record| {
                record["reason"] != "surface_mismatch"
                    && !(record["outcome"] == "rejected" && record["target"]["relation"] == "exact")
            }),
            "work cap {cap} published a terminal rejection"
        );
    }
    let control = selected(&source, "json", ROOT, &selector, &[]);
    assert_eq!(completion(&control)["target"]["membership"], "found");
}

#[test]
fn a_valid_no_seed_row_can_use_the_only_reserved_slot() {
    let mut source = snapshot(false);
    source["lexicon"]["entries"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    source["morphology"]["partsOfSpeech"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    source["morphology"]["parserParameters"]["strata"] = json!("Morphology");
    source["phonology"]["rules"] = json!([]);
    let selector = json!({"rootIndex": 0, "morphs": [authored(0x30)]});
    let data = selected(
        &source.to_string(),
        "json",
        "ttk",
        &selector,
        &["--trace-max-paths", "1", "--trace-max-events", "1"],
    );
    let end = completion(&data);
    assert_eq!(end["search"]["status"], "complete");
    assert_eq!(end["capture"]["truncatedBy"], json!([]));
    assert_eq!(end["target"]["status"], "absent");
    event(&data, "lookup_no_seed", "analysis");
}

#[test]
fn invalid_shape_has_a_complete_empty_census() {
    let source = snapshot(false).to_string();
    let selector = json!({"rootIndex": 0, "morphs": [authored(0x30)]});
    let data = selected_with_search(&source, "json", "zzz", &selector, &[], &[]);
    let end = completion(&data);
    assert_eq!(end["search"]["status"], "invalid");
    assert_eq!(end["search"]["stopReasons"], json!([]));
    assert_eq!(end["search"]["outputCensus"], "complete");
    assert_eq!(end["target"]["status"], "absent");
    assert_eq!(end["target"]["coverage"], "exhausted");
    assert_eq!(end["target"]["limitations"], json!([]));
    assert_eq!(
        end["target"]["matchCount"],
        json!({"value": 0, "accuracy": "exact"})
    );
}

#[test]
fn unavailable_output_identity_leaves_membership_unknown() {
    let mut source = snapshot(false);
    let entry = &mut source["lexicon"]["entries"][0];
    entry["msas"][0]["guid"] = json!("synthetic-unverified-msa");
    entry["senses"][0]["msa"] = entry["msas"][0]["guid"].clone();
    let selector = json!({"rootIndex": 0, "morphs": [authored(0x40)]});
    let data = selected(&source.to_string(), "json", ROOT, &selector, &[]);
    let end = completion(&data);
    assert_eq!(end["search"]["status"], "complete");
    assert_eq!(end["search"]["outputCount"], 1);
    assert_eq!(end["target"]["membership"], "unknown");
    assert_eq!(end["target"]["coverage"], "incomplete");
    assert!(end["target"]["limitations"]
        .as_array()
        .unwrap()
        .contains(&json!("identity_unavailable")));
}

fn two_slot_templates() -> Value {
    let mut source = snapshot(false);
    source["morphology"]["parserParameters"]["strata"] = json!("Morphology");
    source["phonology"]["rules"] = json!([]);
    let verb = &mut source["morphology"]["partsOfSpeech"][1];
    verb["affixSlots"].as_array_mut().unwrap().push(json!({
        "guid": guid(0x60), "name": "Synthetic second slot", "optional": false,
    }));
    let mut template = verb["affixTemplates"][0].clone();
    template["guid"] = json!(guid(0x61));
    template["suffixSlots"] = json!([guid(0x0d), guid(0x60)]);
    verb["affixTemplates"]
        .as_array_mut()
        .unwrap()
        .push(template);
    let entries = source["lexicon"]["entries"].as_array_mut().unwrap();
    let mut affix = entries[2].clone();
    affix["guid"] = json!(guid(0x70));
    affix["citationForm"][0]["form"] = json!("t");
    affix["allomorphs"][0]["guid"] = json!(guid(0x71));
    affix["allomorphs"][0]["forms"][0]["form"] = json!("t");
    affix["msas"][0]["guid"] = json!(guid(0x72));
    affix["msas"][0]["slots"] = json!([guid(0x60)]);
    affix["senses"][0]["guid"] = json!(guid(0x73));
    affix["senses"][0]["msa"] = json!(guid(0x72));
    entries.push(affix);
    source
}

#[test]
fn second_slot_failure_binds_the_uniquely_applied_middle_occurrence() {
    let source = two_slot_templates();
    let selector =
        json!({"rootIndex": 0, "morphs": [authored(0x40), authored(0x50), authored(0x70)]});
    let data = selected(
        &source.to_string(),
        "json",
        SUFFIXED,
        &selector,
        &["--trace-max-paths", "64"],
    );
    let rejection = data
        .iter()
        .find(|event| {
            event["reason"] == "required_slot_unfilled"
                && event["direction"] == "synthesis"
                && mapping(&data, &event["operands"]["slot"])["address"]["compiledSlotIndex"] == 1
        })
        .expect("the second required slot was evaluated");
    let path = data
        .iter()
        .find(|path| path["type"] == "path" && path["pathId"] == rejection["pathId"])
        .unwrap();
    assert_eq!(path["target"]["relation"], "compatible_partial");
    assert_eq!(
        path["target"]["boundOccurrences"]
            .as_array()
            .unwrap()
            .iter()
            .map(|bound| &bound["index"])
            .collect::<Vec<_>>(),
        [&json!(0), &json!(1)]
    );
    assert_eq!(path["firstRejectEventId"], rejection["eventId"]);
}

#[test]
fn found_membership_survives_a_partial_output_census() {
    let mut source = snapshot(false);
    add_homophone(&mut source);
    let source = source.to_string();
    let selector = json!({"rootIndex": 0, "morphs": [authored(0x30)]});
    let data = selected_with_search(
        &source,
        "json",
        ROOT,
        &selector,
        &["--work-cap", "193"],
        &[],
    );
    let end = completion(&data);
    assert_eq!(end["search"]["status"], "stopped");
    assert_eq!(end["search"]["outputCensus"], "partial");
    assert_eq!(end["target"]["status"], "found");
    assert_eq!(end["target"]["membership"], "found");
    assert_eq!(end["target"]["coverage"], "incomplete");
    assert_eq!(
        end["target"]["matchCount"],
        json!({"value": 1, "accuracy": "lowerBound"})
    );
    assert!(end["target"]["limitations"]
        .as_array()
        .unwrap()
        .contains(&json!("search_stopped")));
    output(&data, &end["target"]["matchedOutputIds"][0]);
}

#[test]
fn merged_compatible_histories_keep_earliest_death_unknown() {
    let mut source = snapshot(false);
    let verb = &mut source["morphology"]["partsOfSpeech"][1];
    let mut duplicate = verb["affixTemplates"][0].clone();
    duplicate["guid"] = json!(guid(0x61));
    verb["affixTemplates"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);

    let selector = json!({"rootIndex": 0, "morphs": [authored(0x40)]});
    let data = selected(
        &source.to_string(),
        "json",
        SUFFIXED,
        &selector,
        &["--trace-max-paths", "64"],
    );
    let end = completion(&data);
    assert_eq!(end["search"]["status"], "complete");
    assert_eq!(end["target"]["membership"], "absent");
    assert_eq!(end["target"]["status"], "unknown");
    assert_eq!(end["target"]["coverage"], "incomplete");
    assert!(end["target"]["limitations"]
        .as_array()
        .unwrap()
        .contains(&json!("lineage_lost")));
    assert!(end["target"]["limitations"]
        .as_array()
        .unwrap()
        .contains(&json!("owner_unobserved")));
    let merge = event(&data, "equivalent_history", "analysis");
    assert_eq!(merge["operands"]["selectorPreserved"], Value::Null);
    let merged = data
        .iter()
        .find(|row| row["type"] == "path" && row["pathId"] == merge["pathId"])
        .unwrap();
    assert_eq!(merged["firstRejectEventId"], Value::Null);
    assert_eq!(merged["mergeStatus"], "retained");
}

#[test]
fn a_preserved_target_rejection_is_distinct_from_the_accepted_sibling() {
    let mut source = snapshot(false);
    source["lexicon"]["entries"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    source["morphology"]["partsOfSpeech"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    source["morphology"]["parserParameters"]["strata"] = json!("Morphology");
    source["phonology"]["rules"] = json!([]);
    add_homophone(&mut source);
    let environment = source["phonology"]["environments"][0]["guid"].clone();
    source["lexicon"]["entries"][0]["allomorphs"][0]["environments"] = json!([environment]);
    let selector = json!({"rootIndex": 0, "morphs": [authored(0x30)]});
    let data = selected(&source.to_string(), "json", ROOT, &selector, &[]);
    let end = completion(&data);
    assert_eq!(end["search"]["outputCount"], 1);
    assert_eq!(end["target"]["status"], "rejected");
    let rejected = event(&data, "environment", "synthesis");
    let dead = data
        .iter()
        .find(|row| row["type"] == "path" && row["pathId"] == rejected["pathId"])
        .unwrap();
    assert_eq!(dead["target"]["relation"], "exact");
    assert_eq!(dead["firstRejectEventId"], rejected["eventId"]);
    let surviving = data
        .iter()
        .find(|row| row["outcome"] == "accepted")
        .unwrap();
    assert_eq!(surviving["target"]["relation"], "different");
    assert_eq!(surviving["firstRejectEventId"], Value::Null);
}

fn add_homophone(source: &mut Value) {
    let entries = source["lexicon"]["entries"].as_array_mut().unwrap();
    let mut homophone = entries[0].clone();
    homophone["guid"] = json!(guid(0x80));
    homophone["allomorphs"][0]["guid"] = json!(guid(0x81));
    homophone["msas"][0]["guid"] = json!(guid(0x82));
    homophone["senses"][0]["guid"] = json!(guid(0x83));
    homophone["senses"][0]["msa"] = json!(guid(0x82));
    entries.push(homophone);
}

const UNCOVERABLE: &str = "required_slot_uncoverable";

fn plain_signature(source: &Value, word: &str) -> String {
    let temp = tempfile::tempdir().unwrap();
    let grammar = temp.path().join("grammar.json");
    fs::write(&grammar, source.to_string()).unwrap();
    let plain = run(&["parse", grammar.to_str().unwrap(), word]);
    success(&plain);
    String::from_utf8(plain.stdout).unwrap()
}

/// Whether the ordinary parse's signature list holds the target: the root form, then `?` and the form of each affix.
fn ordinary_has(ordinary: &str, target: &[u8]) -> bool {
    let form = |entry: u8| match entry {
        0x40 => "kkaa",
        0x50 => "s",
        0x70 => "t",
        _ => unreachable!(),
    };
    let wanted = format!(
        "{}+{}",
        form(target[0]),
        target[1..]
            .iter()
            .map(|&entry| format!("?{}", form(entry)))
            .collect::<String>()
    );
    ordinary
        .trim()
        .split('|')
        .any(|signature| signature == wanted)
}

fn uncoverable_events(data: &[Value]) -> Vec<&Value> {
    data.iter()
        .filter(|row| row["type"] == "event" && row["reason"] == UNCOVERABLE)
        .collect()
}

fn selected_row(source: &Value, word: &str, morphs: &[u8]) -> Vec<Value> {
    let morphs: Vec<_> = morphs.iter().map(|&entry| authored(entry)).collect();
    let selector = json!({"rootIndex": 0, "morphs": morphs});
    selected(
        &source.to_string(),
        "json",
        word,
        &selector,
        &["--trace-max-paths", "64"],
    )
}

#[test]
fn a_target_missing_the_filler_of_a_required_slot_is_rejected_for_that_slot() {
    let source = two_slot_templates();
    let data = selected_row(&source, "kkaat", &[0x40, 0x70]);
    let end = completion(&data);
    assert_eq!(end["target"]["membership"], "absent");
    assert_eq!(end["target"]["status"], "rejected");
    assert_eq!(end["target"]["coverage"], "exhausted");
    assert!(!end["target"]["limitations"]
        .as_array()
        .unwrap()
        .contains(&json!("owner_unobserved")));
    let events = uncoverable_events(&data);
    assert!(!events.is_empty());
    for event in &events {
        assert_eq!(event["direction"], "analysis");
        assert_eq!(event["kind"], "reject");
        assert_eq!(event["stage"], "template");
        assert_eq!(event["operands"]["kind"], "template_slot");
        let slot = mapping(&data, &event["operands"]["slot"]);
        assert_eq!(slot["address"]["compiledSlotIndex"], 0);
    }
    assert!(
        !ordinary_has(&plain_signature(&source, "kkaat"), &[0x40, 0x70]),
        "the named cause must agree with the ordinary parse"
    );
}

#[test]
fn a_template_that_cannot_cover_the_target_does_not_name_it_rejected_while_another_can() {
    let source = two_slot_templates();
    let data = selected_row(&source, "kkaas", &[0x40, 0x50]);
    let end = completion(&data);
    assert_eq!(end["target"]["status"], "found");
    let events = uncoverable_events(&data);
    assert!(!events.is_empty());
    for event in events {
        let slot = mapping(&data, &event["operands"]["slot"]);
        assert_eq!(slot["address"]["compiledSlotIndex"], 1);
    }
}

#[test]
fn a_target_with_no_affix_morph_keeps_every_template_rejection_unknown() {
    let source = two_slot_templates();
    let data = selected_row(&source, "kkaat", &[0x40]);
    assert!(uncoverable_events(&data).is_empty());
    let end = completion(&data);
    assert_eq!(end["target"]["status"], "unknown");
    assert!(end["target"]["limitations"]
        .as_array()
        .unwrap()
        .contains(&json!("owner_unobserved")));
}

#[test]
fn a_named_cause_never_contradicts_the_ordinary_parse() {
    let source = two_slot_templates();
    let words = ["kkaa", "kkaas", "kkaat", "kkaast", "kkaats"];
    let targets: [&[u8]; 6] = [
        &[0x40],
        &[0x40, 0x50],
        &[0x40, 0x70],
        &[0x40, 0x50, 0x70],
        &[0x40, 0x70, 0x50],
        &[0x40, 0x50, 0x50],
    ];
    let mut census: BTreeMap<String, usize> = BTreeMap::new();
    let mut cells: Vec<String> = Vec::new();
    assert!(ordinary_has(
        &plain_signature(&source, "kkaas"),
        &[0x40, 0x50]
    ));
    for word in words {
        let ordinary = plain_signature(&source, word);
        for target in targets {
            let data = selected_row(&source, word, target);
            let end = completion(&data);
            let status = end["target"]["status"].as_str().unwrap().to_owned();
            let named = !uncoverable_events(&data).is_empty();
            if named && status == "rejected" {
                assert_eq!(end["target"]["matchCount"]["value"], 0);
                assert!(
                    !ordinary_has(&ordinary, target),
                    "{word} {target:?} was named uncoverable but the ordinary parse has it:\n{ordinary}"
                );
            }
            *census
                .entry(format!(
                    "{status}/{}",
                    if named { "named" } else { "plain" }
                ))
                .or_default() += 1;
            cells.push(format!("{word} {target:x?} {status} {named}"));
        }
    }
    assert_eq!(
        format!("{census:?}"),
        r#"{"found/named": 1, "rejected/named": 3, "unknown/named": 11, "unknown/plain": 15}"#
    );
    assert_eq!(
        cells
            .iter()
            .filter(|cell| cell.ends_with("rejected true"))
            .cloned()
            .collect::<Vec<_>>(),
        [
            "kkaat [40, 70] rejected true",
            "kkaast [40, 70] rejected true",
            "kkaats [40, 70] rejected true"
        ]
    );
}

pub(super) fn tuple_fixtures() -> Vec<Vec<Value>> {
    vec![selected_row(&two_slot_templates(), "kkaat", &[0x40, 0x70])]
}
