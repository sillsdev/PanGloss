use std::collections::BTreeMap;

use serde_json::Value;

use super::{event, parse, rewrite_grammar, template_grammar, GRAMMAR};

const FULL: &[&str] = &["--trace-max-paths", "64", "--trace-max-events", "512"];

fn paths(records: &[Value]) -> BTreeMap<u64, &Value> {
    records
        .iter()
        .filter(|r| r["type"] == "path")
        .map(|r| (r["pathId"].as_u64().unwrap(), r))
        .collect()
}

#[test]
fn depth_limit_keeps_terminal_and_marks_the_boundary() {
    let grammar = rewrite_grammar();
    let deep = parse(&grammar, "kib", FULL);
    let shallow = parse(
        &grammar,
        "kib",
        &[
            "--trace-max-paths",
            "64",
            "--trace-max-events",
            "512",
            "--trace-max-depth",
            "1",
        ],
    );
    assert_eq!(deep[0]["captureLimits"]["maxDepth"], 32);
    assert_eq!(shallow[0]["captureLimits"]["maxDepth"], 1);
    let row_end = |records: &[Value]| records[records.len() - 2].clone();
    assert_eq!(row_end(&deep)["search"], row_end(&shallow)["search"]);
    assert!(row_end(&shallow)["capture"]["truncatedBy"]
        .as_array()
        .unwrap()
        .contains(&Value::from("depth")));
    let rejected = event(&shallow, "surface_mismatch", "synthesis");
    let paths = paths(&shallow);
    let leaf = paths[&rejected["pathId"].as_u64().unwrap()];
    assert_eq!(leaf["firstRejectEventId"], rejected["eventId"]);
    assert!(paths.values().any(|p| p["parentStatus"] == "omitted"));
    for leaf in paths.values() {
        if let Some(parent) = leaf["parentPathId"].as_u64() {
            assert_eq!(paths[&parent]["parentPathId"], Value::Null);
        }
    }
}

#[test]
fn a_confirmed_sibling_has_its_own_terminal_witness() {
    let grammar = GRAMMAR.replace(
        r#"<Allomorph id="aKib"><PhoneticShape>kib</PhoneticShape>"#,
        r#"<Allomorph id="aKib"><PhoneticShape>kib</PhoneticShape>
            <RequiredEnvironments><Environment id="syntheticLeft"><LeftEnvironment><PhoneticTemplate>
              <PhoneticSequence><Segment segment="ca" /></PhoneticSequence>
            </PhoneticTemplate></LeftEnvironment></Environment></RequiredEnvironments>"#,
    ).replace(
        r#"<LexicalEntry id="eKib" partOfSpeech="posV">"#,
        r#"<LexicalEntry id="eOtherKib" partOfSpeech="posV"><MorphemeId>other</MorphemeId><Allomorphs>
             <Allomorph id="aOtherKib"><PhoneticShape>kib</PhoneticShape></Allomorph>
           </Allomorphs></LexicalEntry><LexicalEntry id="eKib" partOfSpeech="posV">"#,
    );
    let records = parse(&grammar, "kib", FULL);
    let rejected = event(&records, "environment", "synthesis");
    let paths = paths(&records);
    let accepted = paths.values().find(|p| p["outcome"] == "accepted").unwrap();
    assert_eq!(accepted["firstRejectEventId"], Value::Null);
    assert_eq!(accepted["outputStatus"], "retained");
    let mut cursor = accepted["parentPathId"].as_u64();
    while let Some(id) = cursor {
        assert_ne!(paths[&id]["pathId"], rejected["pathId"]);
        cursor = paths[&id]["parentPathId"].as_u64();
    }
}

#[test]
fn separate_applications_of_one_allomorph_remain_distinct() {
    let grammar = GRAMMAR.replace(r#"id="mrTense""#, r#"id="mrTense" multipleApplication="2""#);
    let data = parse(&grammar, "kibeses", FULL);
    let output = data
        .iter()
        .find(|record| record["type"] == "output")
        .unwrap();
    let morphs = output["morphs"].as_array().unwrap();
    assert_eq!(morphs.len(), 3);
    assert_eq!(output["rootIndex"], 0);
    assert_eq!(morphs[1], morphs[2]);
    assert_ne!(morphs[0], morphs[1]);
}

#[test]
fn a_duplicate_slot_rule_reports_the_synthesis_merge() {
    let grammar = template_grammar().replace(
        r#"<Slot morphologicalRules="mrTense">"#,
        r#"<Slot morphologicalRules="mrTense mrTense">"#,
    );
    let data = parse(&grammar, "kibes", FULL);
    let merge = event(&data, "equivalent_history", "synthesis");
    let paths = paths(&data);
    let path = paths[&merge["pathId"].as_u64().unwrap()];
    assert_eq!(path["mergeStatus"], "retained");
    assert!(paths.contains_key(&path["mergedIntoPathId"].as_u64().unwrap()));
    assert_eq!(data[data.len() - 2]["search"]["outputCount"], 1);
}

fn duplicate_templates() -> Vec<Value> {
    let template = template_grammar();
    let (_, templates) = template.split_once("<AffixTemplates>").unwrap();
    let (body, _) = templates.split_once("</AffixTemplates>").unwrap();
    let optional = body.replace(
        r#"morphologicalRules="mrTense""#,
        r#"morphologicalRules="mrTense" optional="true""#,
    );
    let grammar = template.replace("</AffixTemplates>", &format!("{optional}</AffixTemplates>"));
    parse(&grammar, "kibes", FULL)
}

#[test]
fn discarded_duplicates_join_to_the_retained_paths() {
    let records = duplicate_templates();
    let paths = paths(&records);
    assert_eq!(
        records[records.len() - 2]["capture"]["truncatedBy"],
        serde_json::json!([])
    );
    assert!(paths.values().all(|path| path["parentStatus"] != "omitted"));
    for direction in ["analysis", "synthesis"] {
        let merge = event(&records, "equivalent_history", direction);
        let path = paths[&merge["pathId"].as_u64().unwrap()];
        assert_eq!(path["outcome"], "merged");
        assert_eq!(path["mergeStatus"], "retained");
        assert_ne!(path["parentPathId"], path["mergedIntoPathId"]);
        assert_eq!(
            path["mergedIntoPathId"],
            merge["operands"]["retainedPathId"]
        );
        assert_eq!(path["firstRejectEventId"], Value::Null);
    }
    assert_eq!(records[records.len() - 2]["search"]["outputCount"], 1);
}

#[test]
fn preserved_rule_orders_confirm_from_one_root_lookup() {
    let grammar = GRAMMAR
        .replace(r#"morphologicalRuleOrder="linear""#, r#"morphologicalRuleOrder="unordered""#)
        .replace(
            r#"<CopyFromInput index="stemTense" /><InsertSegments><PhoneticShape>es</PhoneticShape></InsertSegments>"#,
            r#"<InsertSegments><PhoneticShape>za</PhoneticShape></InsertSegments><CopyFromInput index="stemTense" />"#,
        );
    let records = parse(&grammar, "zakibon", FULL);
    assert_eq!(
        records
            .iter()
            .filter(|r| r["reason"] == "lookup_completed")
            .count(),
        1
    );
    let paths = paths(&records);
    let accepted: Vec<_> = paths
        .values()
        .filter(|p| p["outcome"] == "accepted")
        .collect();
    assert_eq!(accepted.len(), 2);
    assert_ne!(accepted[0]["parentPathId"], accepted[1]["parentPathId"]);
    assert_ne!(accepted[0]["outputId"], accepted[1]["outputId"]);
    assert!(accepted
        .iter()
        .all(|path| path["firstRejectEventId"].is_null()));
}

pub(super) fn tuple_fixtures() -> Vec<Vec<Value>> {
    vec![duplicate_templates()]
}
