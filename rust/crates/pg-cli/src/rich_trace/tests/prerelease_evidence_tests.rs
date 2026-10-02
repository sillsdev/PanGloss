use super::{
    assert_lookup_result, filter_pass_xml, nodes_of_type, parse_identity_multiset,
    trace_details_for, trace_details_for_grammar, FAMILY_XML,
};
use serde_json::json;

#[test]
fn prerelease_lexical_family_blocker_without_authored_id_has_structural_identity() {
    let xml = FAMILY_XML.replace("<LexicalEntry id=\"eVem\"", "<LexicalEntry");
    let baseline_grammar = pg_grammar::load(FAMILY_XML).unwrap();
    let baseline = trace_details_for_grammar(
        &baseline_grammar,
        "zodut",
        &pg_parse::ParseOptions::default(),
    );
    let mut baseline_blocked = Vec::new();
    super::nodes_of_type(&baseline["trace"], "Blocked", &mut baseline_blocked);
    assert!(!baseline_blocked.is_empty());
    assert!(baseline_blocked.iter().any(|node| {
        node["blockedByEntry"]
            == json!({
                "kind": "lexEntry",
                "id": "eVem",
                "quality": "authored"
            })
    }));

    let grammar = pg_grammar::load(&xml).unwrap();
    let details = trace_details_for_grammar(&grammar, "zodut", &pg_parse::ParseOptions::default());
    assert_eq!(
        details["result"]["signature"],
        baseline["result"]["signature"]
    );
    let mut blocked = Vec::new();
    super::nodes_of_type(&details["trace"], "Blocked", &mut blocked);
    assert!(!blocked.is_empty());
    for node in blocked {
        assert_eq!(node["blockReason"], "LexicalFamilyReplacement");
        let identity = &node["blockedByEntry"];
        assert_eq!(identity["kind"], "lexEntry");
        assert_eq!(identity["quality"], "grammar-local");
        assert!(identity["id"].as_str().is_some_and(|id| !id.is_empty()));
    }

    let control = trace_details_for_grammar(&grammar, "kib", &pg_parse::ParseOptions::default());
    assert_ne!(control["result"]["signature"], "-");
    assert_eq!(
        parse_identity_multiset(&pg_parse::Morpher::new(&grammar, usize::MAX).parse_word("kib"))
            .len(),
        1
    );

    let baseline_parse = pg_parse::Morpher::new(&baseline_grammar, usize::MAX).parse_word("vem");
    let structural_parse = pg_parse::Morpher::new(&grammar, usize::MAX).parse_word("vem");
    assert_eq!(baseline_parse.analyses, structural_parse.analyses);
    assert_eq!(baseline_parse.signature(), structural_parse.signature());
}

#[test]
fn prerelease_whitespace_lexical_id_is_not_published_as_authored() {
    let xml = FAMILY_XML.replace("<LexicalEntry id=\"eVem\"", "<LexicalEntry id=\"   \"");
    let details = trace_details_for(&xml, "zodut");
    let mut blocked = Vec::new();
    nodes_of_type(&details["trace"], "Blocked", &mut blocked);
    assert!(!blocked.is_empty());
    for node in blocked {
        assert_eq!(node["blockedByEntry"]["quality"], "grammar-local");
        assert!(node["blockedByEntry"]["id"]
            .as_str()
            .is_some_and(|id| !id.trim().is_empty()));
    }
}

#[test]
fn prerelease_blank_environment_id_is_absent_but_keeps_authored_text() {
    let xml = filter_pass_xml("allomorph-compatibility");
    let xml = xml.replace("<Environment>", "<Environment id=\"\">/ _ [Vowel]");
    let details = trace_details_for(&xml, "kapita");
    let mut failed = Vec::new();
    super::nodes_of_type(&details["trace"], "Failed", &mut failed);
    let environments: Vec<_> = failed
        .into_iter()
        .filter(|node| node["failureReason"] == "Environments")
        .collect();
    assert!(!environments.is_empty());
    for node in environments {
        for alternative in node["failureContext"]["evidence"]["alternatives"]
            .as_array()
            .unwrap()
        {
            assert_eq!(alternative["accepted"], false);
            assert_eq!(alternative["sourceStatus"], "captured");
            assert!(alternative["sourceIdentity"].is_null());
            assert_eq!(
                alternative["authoredText"].as_str().unwrap().trim(),
                "/ _ [Vowel]"
            );
        }
    }

    let mut lookups = Vec::new();
    nodes_of_type(&details["trace"], "LexicalLookup", &mut lookups);
    let candidate_count: usize = lookups
        .iter()
        .map(|node| assert_lookup_result(node, "lexicon"))
        .sum();
    assert_eq!(
        candidate_count, 2,
        "kapita has two materialized root allomorph candidates"
    );
}

#[test]
fn prerelease_blank_environment_id_without_text_has_no_source() {
    for id in ["", "   "] {
        let xml = filter_pass_xml("allomorph-compatibility")
            .replace("<Environment>", &format!("<Environment id=\"{id}\">"));
        let details = trace_details_for(&xml, "kapita");
        let mut failed = Vec::new();
        nodes_of_type(&details["trace"], "Failed", &mut failed);
        let environments: Vec<_> = failed
            .into_iter()
            .filter(|node| node["failureReason"] == "Environments")
            .collect();
        assert!(!environments.is_empty());
        for node in environments {
            for alternative in node["failureContext"]["evidence"]["alternatives"]
                .as_array()
                .unwrap()
            {
                assert!(alternative["sourceIdentity"].is_null());
                assert!(alternative["authoredText"].is_null());
                assert_eq!(alternative["sourceStatus"], "unavailable");
            }
        }
    }
}

#[test]
fn prerelease_suffix_environment_source_reaches_failure_trace_for_snapshot_guid() {
    let mut snapshot: pg_snapshot::Snapshot = serde_json::from_str(include_str!(
        "../../../../../../docs/formats/examples/trace-details-v2-sample.snapshot.json"
    ))
    .unwrap();
    let env_guid = "00000000-0000-0000-0000-000000000501";
    let authored_text = "/ _ k";
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: env_guid.into(),
            name: "before k".into(),
            representation: authored_text.into(),
        });
    let suffix = snapshot
        .lexicon
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.allomorphs)
        .find(|allomorph| allomorph.guid.ends_with("010a"))
        .expect("sample snapshot has the ta suffix allomorph");
    let form_guid = suffix.guid.clone();
    suffix.environments.push(env_guid.into());

    let (grammar, _) = pg_grammar::compile_project(&snapshot).unwrap();
    let details = trace_details_for_grammar(&grammar, "kumata", &pg_parse::ParseOptions::default());
    let mut failed = Vec::new();
    super::nodes_of_type(&details["trace"], "Failed", &mut failed);
    let gates: Vec<_> = failed
        .into_iter()
        .filter(|node| node["failureReason"] == "Environments")
        .collect();
    assert!(!gates.is_empty());
    for node in gates {
        let evidence = &node["failureContext"]["evidence"];
        let source_forms = evidence["failedObject"]["sourceFormIds"]
            .as_array()
            .unwrap();
        assert!(source_forms.contains(&json!(form_guid)));
        let alternatives = evidence["alternatives"].as_array().unwrap();
        assert_eq!(alternatives.len(), 1);
        assert_eq!(alternatives[0]["sourceIdentity"]["id"], env_guid);
        assert_eq!(alternatives[0]["authoredText"], authored_text);
        assert_eq!(alternatives[0]["sourceStatus"], "captured");
        assert_eq!(alternatives[0]["accepted"], false);
    }
}

#[test]
fn prerelease_circumfix_environment_conjunction_has_no_single_source() {
    let mut snapshot: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../docs/formats/examples/trace-details-v2-sample.snapshot.json"
    ))
    .unwrap();
    let env_prefix = "00000000-0000-0000-0000-000000000511";
    let env_suffix = "00000000-0000-0000-0000-000000000512";
    snapshot["phonology"]["environments"] = serde_json::json!([
        {"guid": env_prefix, "name": "before prefix", "representation": "/t_"},
        {"guid": env_suffix, "name": "after suffix", "representation": "/_k"}
    ]);
    let entry = snapshot["lexicon"]["entries"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["guid"].as_str().unwrap().ends_with("0108"))
        .expect("sample snapshot has the ta affix entry");
    entry["lexemeMorphType"] = serde_json::json!("circumfix");
    let mut suffix = entry["allomorphs"][0].clone();
    suffix["environments"] = serde_json::json!([env_suffix]);
    let mut prefix = suffix.clone();
    prefix["guid"] = serde_json::json!("00000000-0000-0000-0000-000000000513");
    prefix["morphType"] = serde_json::json!("prefix");
    prefix["forms"][0]["form"] = serde_json::json!("ka");
    prefix["environments"] = serde_json::json!([env_prefix]);
    entry["allomorphs"] = serde_json::json!([prefix, suffix]);

    let snapshot: pg_snapshot::Snapshot = serde_json::from_value(snapshot).unwrap();
    let (grammar, _) = pg_grammar::compile_project(&snapshot).unwrap();
    let details =
        trace_details_for_grammar(&grammar, "kakumata", &pg_parse::ParseOptions::default());
    let mut failed = Vec::new();
    nodes_of_type(&details["trace"], "Failed", &mut failed);
    let gates: Vec<_> = failed
        .into_iter()
        .filter(|node| node["failureReason"] == "Environments")
        .collect();
    assert!(!gates.is_empty(), "circumfix contexts must be checked");
    for node in gates {
        let alternatives = node["failureContext"]["evidence"]["alternatives"]
            .as_array()
            .unwrap();
        assert_eq!(alternatives.len(), 1);
        assert_eq!(alternatives[0]["sourceStatus"], "unavailable");
        assert!(alternatives[0]["sourceIdentity"].is_null());
        assert!(alternatives[0]["authoredText"].is_null());
    }
}

#[test]
fn prerelease_blank_snapshot_environment_guid_keeps_text_without_authored_identity() {
    for guid in ["", "   "] {
        for suffix in [false, true] {
            let mut snapshot: pg_snapshot::Snapshot = serde_json::from_str(include_str!(
                "../../../../../../docs/formats/examples/trace-details-v2-sample.snapshot.json"
            ))
            .unwrap();
            let text = "/ _ k";
            snapshot
                .phonology
                .environments
                .push(pg_snapshot::phonology::Environment {
                    guid: guid.into(),
                    name: "before k".into(),
                    representation: text.into(),
                });
            let form = if suffix {
                snapshot
                    .lexicon
                    .entries
                    .iter_mut()
                    .flat_map(|entry| &mut entry.allomorphs)
                    .find(|form| form.guid.ends_with("010a"))
                    .unwrap()
            } else {
                &mut snapshot.lexicon.entries[0].allomorphs[0]
            };
            form.environments.push(guid.into());
            let (grammar, _) = pg_grammar::compile_project(&snapshot).unwrap();
            let details =
                trace_details_for_grammar(&grammar, "kumata", &pg_parse::ParseOptions::default());
            let mut failed = Vec::new();
            nodes_of_type(&details["trace"], "Failed", &mut failed);
            let gates: Vec<_> = failed
                .into_iter()
                .filter(|node| node["failureReason"] == "Environments")
                .collect();
            assert!(!gates.is_empty());
            for node in gates {
                let alternatives = node["failureContext"]["evidence"]["alternatives"]
                    .as_array()
                    .unwrap();
                assert_eq!(alternatives.len(), 1);
                assert!(
                    alternatives[0]["sourceIdentity"].is_null(),
                    "blank GUID {guid:?}, suffix {suffix}"
                );
                assert_eq!(alternatives[0]["authoredText"], text);
                assert_eq!(alternatives[0]["sourceStatus"], "captured");
            }
        }
    }
}
