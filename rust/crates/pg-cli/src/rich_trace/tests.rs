use std::time::Duration;

use super::{render_envelope_v2, validate_details, TraceMetadata};
use pg_grammar::model::StratumId;
use pg_parse::ParseOutcome;
use pg_rules::stats::{Counters, Direction, ObjectKind, OverlayPhase, StatsRow};
use serde_json::json;

#[test]
fn details_requires_trace_and_json() {
    assert!(validate_details(true, "json", true, false).is_ok());
    assert!(validate_details(false, "json", true, false).is_err());
    assert!(validate_details(true, "text", true, false).is_err());
    assert!(validate_details(true, "json", true, true).is_err());
}

#[test]
fn envelope_keeps_tree_result_and_unmeasured_timing_explicit() {
    let outcome = ParseOutcome {
        analyses: vec![("root+past".into(), "sagd".into())],
        structured: Vec::new(),
        capped: false,
        invalid_shape: false,
        steps: 17,
        timed_out: false,
        guessed: false,
        candidates_generated: 1,
    };
    let rows = vec![
        StatsRow {
            kind: ObjectKind::MorphRule,
            object_index: 0,
            stratum: StratumId(0),
            allomorph: 0,
            direction: Direction::Synthesis,
            counters: Counters {
                attempts: 2,
                not_applied: 1,
                self_time_ns: 11,
                ..Counters::default()
            },
        },
        StatsRow {
            kind: ObjectKind::MorphRule,
            object_index: 0,
            stratum: StratumId(0),
            allomorph: 1,
            direction: Direction::Synthesis,
            counters: Counters {
                not_applied: 2,
                self_time_ns: 7,
                ..Counters::default()
            },
        },
        StatsRow {
            kind: ObjectKind::Overlay,
            object_index: OverlayPhase::Search.index(),
            stratum: StratumId(0),
            allomorph: 0,
            direction: Direction::Analysis,
            counters: Counters {
                attempts: 1,
                work: 4,
                self_time_ns: 90,
                ..Counters::default()
            },
        },
        StatsRow {
            kind: ObjectKind::Overlay,
            object_index: OverlayPhase::Materialize.index(),
            stratum: StratumId(0),
            allomorph: 0,
            direction: Direction::Analysis,
            counters: Counters {
                attempts: 1,
                work: 3,
                self_time_ns: 9,
                ..Counters::default()
            },
        },
        StatsRow {
            kind: ObjectKind::PhonRule,
            object_index: 0,
            stratum: StratumId(0),
            allomorph: 0,
            direction: Direction::Analysis,
            counters: Counters {
                attempts: 3,
                self_time_ns: 77,
                ..Counters::default()
            },
        },
        StatsRow {
            kind: ObjectKind::RootIndex,
            object_index: 0,
            stratum: StratumId(0),
            allomorph: 0,
            direction: Direction::Analysis,
            counters: Counters {
                attempts: 4,
                self_time_ns: 23,
                ..Counters::default()
            },
        },
    ];
    let tree =
        json!({ "type": "WordAnalysis", "children": [{"type": "Successful", "children": []}] });
    let value: serde_json::Value = serde_json::from_str(
        &render_envelope_v2(
            tree.clone(),
            "sagd",
            &outcome,
            &rows,
            Duration::from_nanos(300),
            &TraceMetadata::default(),
        )
        .expect("rich envelope serializes"),
    )
    .expect("rich envelope is JSON");
    assert_eq!(value["schemaVersion"], "pangloss.trace-details.v3");
    assert_eq!(value["trace"], tree);
    assert_eq!(value["search"]["completed"], true);
    assert_eq!(value["result"]["signature"], "root+past|sagd");
    assert_eq!(value["categories"]["morphRule"]["selfElapsedNs"], 18);
    assert_eq!(value["categories"]["morphRule"]["analysisSelfElapsedNs"], 0);
    assert_eq!(
        value["categories"]["morphRule"]["synthesisSelfElapsedNs"],
        18
    );
    assert_eq!(value["categories"]["morphRule"]["notApplied"], 1);
    assert_eq!(value["categories"]["morphRule"]["attempts"], 2);
    assert_eq!(value["categories"]["phonRule"]["attempts"], 3);
    assert_eq!(value["categories"]["phonRule"]["selfElapsedNs"], 77);
    assert_eq!(value["categories"]["phonRule"]["timingAvailable"], true);
    assert_eq!(value["categories"]["rootIndex"]["selfElapsedNs"], 23);
    assert_eq!(value["categories"]["rootIndex"]["timingAvailable"], true);
    assert_eq!(value["categories"]["overlay"]["selfElapsedNs"], 99);
    assert_eq!(value["categories"]["overlay"]["timingAvailable"], true);
    let phases = &value["categories"]["overlay"]["phases"];
    assert_eq!(
        phases["search"],
        json!({"attempts": 1, "work": 4, "selfElapsedNs": 90})
    );
    assert_eq!(
        phases["gate"],
        json!({"attempts": 0, "work": 0, "selfElapsedNs": 0})
    );
    assert_eq!(
        phases["materialize"],
        json!({"attempts": 1, "work": 3, "selfElapsedNs": 9})
    );
    assert_eq!(
        value["categories"]["overlay"]["synthesisSelfElapsedNs"],
        serde_json::Value::Null
    );
    assert_eq!(value["search"]["timedNs"], 217);
    assert_eq!(value["search"]["unattributedNs"], 83);
}

#[test]
fn v2_retains_analysis_when_projection_is_unavailable() {
    let outcome = ParseOutcome {
        analyses: vec![("root".into(), "sagd".into())],
        structured: Vec::new(),
        capped: true,
        invalid_shape: false,
        steps: 99,
        timed_out: false,
        guessed: false,
        candidates_generated: 1,
    };
    let value: serde_json::Value = serde_json::from_str(
        &render_envelope_v2(
            json!({"type": "WordAnalysis", "children": []}),
            "sagd",
            &outcome,
            &[],
            Duration::from_nanos(7),
            &TraceMetadata {
                grammar_name: Some("Test".into()),
                grammar_hash: Some("abc".into()),
                grammar_hash_semantics: Some("source-bytes-v1".into()),
                source_kind: "xml".into(),
                ..TraceMetadata::default()
            },
        )
        .expect("v2 envelope serializes"),
    )
    .expect("v2 envelope is JSON");
    assert_eq!(value["schemaVersion"], "pangloss.trace-details.v3");
    assert_eq!(value["provenance"]["grammar"]["name"], "Test");
    assert_eq!(
        value["provenance"]["grammar"]["grammarHashSemantics"],
        "source-bytes-v1"
    );
    assert_eq!(value["search"]["completed"], false);
    assert_eq!(
        value["result"]["analyses"][0]["projection"]["status"],
        "unavailable"
    );
    assert_eq!(value["trace"]["children"].as_array().unwrap().len(), 0);
}
#[test]
fn rich_tree_preserves_nodes_beyond_default_json_parse_depth() {
    use pg_rules::trace::TraceSink;
    let grammar = pg_grammar::load(&filter_pass_xml("exact-span")).unwrap();
    let sink = pg_rules::trace::TreeTraceSink::with_failure_context();
    let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
    let (outcome, rows) =
        morpher.parse_word_traced_with_stats("matinlu", &pg_parse::ParseOptions::default(), &sink);
    let root = sink.root().unwrap();
    let input = sink.node(root).input.unwrap();
    let mut cursor = root;
    for _ in 0..80 {
        cursor = sink.begin_apply_stratum(cursor, StratumId(0), &input);
    }
    let ordinary = crate::trace_render::render_json(&grammar, &sink, root);
    assert!(serde_json::from_str::<serde_json::Value>(&ordinary).is_err());
    let rich = super::render(
        &grammar,
        &sink,
        Some(root),
        "matinlu",
        &outcome,
        &rows,
        Duration::ZERO,
        &TraceMetadata::default(),
    )
    .unwrap();
    assert_eq!(rich.matches("\"type\":").count(), sink.len());
    assert_eq!(ordinary.matches("\"type\":").count(), sink.len());
    assert!(rich.contains("\"schemaVersion\":\"pangloss.trace-details.v3\""));
}

#[test]
fn authored_morph_keeps_lexeme_headword_when_citation_is_absent() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pg-fwdata/tests/data/fixture.fwdata");
    let (mut snapshot, _) = pg_fwdata::import_file(&path).unwrap();
    let entry = snapshot
        .lexicon
        .entries
        .iter_mut()
        .find(|entry| entry.citation_form.iter().any(|form| form.form == "ranna"))
        .unwrap();
    entry.citation_form.clear();
    let lexeme = entry.allomorphs.last().unwrap();
    let form_id = lexeme.guid.clone();
    let expected = lexeme.forms[0].form.clone();
    let msa_id = entry.msas[0].guid().to_string();
    let morph = super::rich_morph(
        Some(&snapshot),
        &pg_parse::ParseMorph {
            form: Some(form_id.clone()),
            msa: Some(msa_id),
            infl_type: None,
            guessed_string: None,
        },
    );
    assert_eq!(morph["headword"]["text"], expected);
    assert_eq!(morph["headword"]["sourceId"], form_id);
    assert_eq!(morph["identity"]["quality"], "authored");
    assert!(!morph["msa"].is_null());
}

fn trace_details_with_options(
    xml: &str,
    word: &str,
    opts: &pg_parse::ParseOptions,
) -> serde_json::Value {
    let grammar = pg_grammar::load(xml).unwrap();
    trace_details_for_grammar(&grammar, word, opts)
}

fn trace_details_for_grammar(
    grammar: &pg_grammar::model::Grammar,
    word: &str,
    opts: &pg_parse::ParseOptions,
) -> serde_json::Value {
    let sink = pg_rules::trace::TreeTraceSink::with_failure_context();
    let morpher = pg_parse::Morpher::new(grammar, usize::MAX);
    let (outcome, rows) = morpher.parse_word_traced_with_stats(word, opts, &sink);
    let plain = morpher.parse_word_opts(word, opts);
    assert_eq!(
        parse_identity_multiset(&outcome),
        parse_identity_multiset(&plain),
        "rich tracing must preserve the complete parse identity multiset"
    );
    assert_eq!(outcome.signature(), plain.signature());
    assert!(!outcome.capped && !outcome.timed_out && !outcome.invalid_shape);
    assert!(!plain.capped && !plain.timed_out && !plain.invalid_shape);
    serde_json::from_str(
        &super::render(
            grammar,
            &sink,
            sink.root(),
            word,
            &outcome,
            &rows,
            Duration::ZERO,
            &TraceMetadata::default(),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn snapshot_environment_guid_text_and_form_identity_reach_json() {
    let mut snapshot: pg_snapshot::Snapshot = serde_json::from_str(include_str!(
        "../../../../../docs/formats/examples/trace-details-v2-sample.snapshot.json"
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
    let root = &mut snapshot.lexicon.entries[0].allomorphs[0];
    let form_guid = root.guid.clone();
    root.environments.push(env_guid.into());
    let (grammar, _) = pg_grammar::compile_project(&snapshot).unwrap();
    let details = trace_details_for_grammar(&grammar, "kumata", &pg_parse::ParseOptions::default());
    let mut failed = Vec::new();
    nodes_of_type(&details["trace"], "Failed", &mut failed);
    let gates: Vec<_> = failed
        .into_iter()
        .filter(|node| node["failureReason"] == "Environments")
        .collect();
    assert!(!gates.is_empty());
    for node in gates {
        let evidence = &node["failureContext"]["evidence"];
        assert_eq!(
            evidence["failedObject"]["sourceFormIds"],
            json!([form_guid])
        );
        let alternatives = evidence["alternatives"].as_array().unwrap();
        assert_eq!(alternatives.len(), 1);
        assert_eq!(alternatives[0]["sourceIdentity"]["id"], env_guid);
        assert_eq!(alternatives[0]["authoredText"], authored_text);
        assert_eq!(alternatives[0]["sourceStatus"], "captured");
        assert_eq!(alternatives[0]["accepted"], false);
    }
}

fn trace_details_for(xml: &str, word: &str) -> serde_json::Value {
    trace_details_with_options(xml, word, &pg_parse::ParseOptions::default())
}

fn parse_identity_multiset(outcome: &pg_parse::ParseOutcome) -> Vec<(String, String, String)> {
    assert_eq!(outcome.analyses.len(), outcome.structured.len());
    let mut identities: Vec<_> = outcome
        .analyses
        .iter()
        .zip(&outcome.structured)
        .map(|((morphemes, surface), structured)| {
            (
                morphemes.clone(),
                surface.clone(),
                serde_json::to_string(structured).expect("structured analysis serializes"),
            )
        })
        .collect();
    identities.sort();
    identities
}

fn nodes_of_type<'a>(
    node: &'a serde_json::Value,
    kind: &str,
    out: &mut Vec<&'a serde_json::Value>,
) {
    if node["type"] == kind {
        out.push(node);
    }
    for child in node["children"].as_array().into_iter().flatten() {
        nodes_of_type(child, kind, out);
    }
}

const FAMILY_XML: &str = include_str!("../../tests/data/trace-family.xml");

#[test]
fn family_blocking_records_reason_and_replacement_entry() {
    let details = trace_details_for(FAMILY_XML, "zodut");
    assert_eq!(details["result"]["signature"], "-");
    let mut blocked = Vec::new();
    nodes_of_type(&details["trace"], "Blocked", &mut blocked);
    assert!(!blocked.is_empty(), "zodut must exercise family blocking");
    for node in blocked {
        assert_eq!(node["sourceIdentity"]["id"], "mrPast2");
        assert_eq!(node["blockReason"], "LexicalFamilyReplacement");
        assert_eq!(
            node["blockedByEntry"],
            json!({ "kind": "lexEntry", "id": "eVem", "quality": "authored" })
        );
        assert_eq!(node["outputShape"], "vem");
    }
    let mut failed = Vec::new();
    nodes_of_type(&details["trace"], "Failed", &mut failed);
    assert!(failed.iter().all(|node| node.get("blockReason").is_none()));
}

fn filter_pass_xml(name: &str) -> String {
    pg_conformance_fixtures::discover_filter_passes()
        .into_iter()
        .find(|fixture| fixture.name == name)
        .unwrap_or_else(|| panic!("required filter-pass fixture {name:?} is missing"))
        .load_grammar_xml()
}

fn edge_case_xml(name: &str) -> String {
    pg_conformance_fixtures::require_fixture("edge-cases", name).load_grammar_xml()
}

fn assert_lookup_result(node: &serde_json::Value, mode: &str) -> usize {
    let result = &node["lookupResult"];
    assert_eq!(result["completed"], true);
    assert_eq!(result["mode"], mode);
    let count = result["matchCount"]
        .as_u64()
        .expect("completed lookup records an integer candidate count") as usize;
    assert_eq!(
        result["status"],
        if count == 0 { "zeroMatches" } else { "matches" }
    );
    count
}

#[test]
fn empty_lexical_lookup_records_completed_zero_matches() {
    let details = trace_details_for(&filter_pass_xml("exact-span"), "matinolu");
    let mut lookups = Vec::new();
    nodes_of_type(&details["trace"], "LexicalLookup", &mut lookups);
    assert!(!lookups.is_empty(), "matinolu must reach lexical lookup");
    for node in lookups {
        assert_eq!(assert_lookup_result(node, "lexicon"), 0);
    }
}

#[test]
fn positive_lexical_lookup_counts_materialized_candidates() {
    let details = trace_details_for(&filter_pass_xml("exact-span"), "matinlu");
    let mut lookups = Vec::new();
    nodes_of_type(&details["trace"], "LexicalLookup", &mut lookups);
    assert!(!lookups.is_empty(), "matinlu must reach lexical lookup");
    let counts: Vec<_> = lookups
        .iter()
        .map(|node| assert_lookup_result(node, "lexicon"))
        .collect();
    assert!(
        counts.iter().any(|&count| count > 0),
        "matinlu must have a completed lexicon lookup with materialized candidates"
    );
}

#[test]
fn guesser_lookup_records_mode_and_positive_match_count() {
    let opts = pg_parse::ParseOptions::default().with_guess_only(true);
    let details = trace_details_with_options(
        include_str!("../../tests/data/trace-guess.xml"),
        "gag",
        &opts,
    );
    let mut lookups = Vec::new();
    nodes_of_type(&details["trace"], "LexicalLookup", &mut lookups);
    assert!(
        !lookups.is_empty(),
        "guess-only input must reach the guesser"
    );
    let counts: Vec<_> = lookups
        .iter()
        .map(|node| assert_lookup_result(node, "guesser"))
        .collect();
    assert!(
        counts.iter().any(|&count| count > 0),
        "gag must have a completed guesser lookup with materialized candidates"
    );
}

#[test]
fn ordinary_trace_omits_rich_lookup_results() {
    use pg_rules::trace::{TraceType, TreeTraceSink};

    let grammar =
        pg_grammar::load(&filter_pass_xml("exact-span")).expect("exact-span grammar loads");
    let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
    let sink = TreeTraceSink::new();
    let outcome = morpher.parse_word_traced("matinlu", &pg_parse::ParseOptions::default(), &sink);
    assert_eq!(
        parse_identity_multiset(&outcome),
        parse_identity_multiset(&morpher.parse_word("matinlu"))
    );
    let root = sink.root().expect("traced parse has a root");
    let mut lookups = Vec::new();
    collect_nodes(&sink, root, &mut lookups);
    let lookups: Vec<_> = lookups
        .into_iter()
        .filter(|node| node.type_ == TraceType::LexicalLookup)
        .collect();
    assert!(!lookups.is_empty());
    assert!(lookups.iter().all(|node| node.lookup_result.is_none()));
    let ordinary_json = crate::trace_render::render_json(&grammar, &sink, root);
    assert!(!ordinary_json.contains("lookupResult"));
}

fn collect_nodes(
    sink: &pg_rules::trace::TreeTraceSink,
    handle: pg_rules::trace::TraceHandle,
    out: &mut Vec<pg_rules::trace::TraceNode>,
) {
    let node = sink.node(handle);
    out.push(node.clone());
    for child in node.children {
        collect_nodes(sink, child, out);
    }
}

#[test]
fn template_slots_and_partial_causes_are_owner_published_in_json() {
    let details = trace_details_for(&edge_case_xml("optional-template-composite"), "sipu");
    let mut outputs = Vec::new();
    nodes_of_type(&details["trace"], "TemplateSynthesisOutput", &mut outputs);
    assert!(outputs.iter().any(|node| {
        node["slots"].as_array().is_some_and(|slots| {
            slots.iter().any(|slot| slot["status"] == "Applied")
                && slots
                    .iter()
                    .filter(|slot| slot["status"] == "OptionalSkipped")
                    .count()
                    == 2
        })
    }));
    for node in outputs {
        for slot in node["slots"].as_array().unwrap() {
            assert_eq!(slot["slotIdentity"]["quality"], "grammar-local");
            assert!(
                slot.get("reason").is_none(),
                "a failed batch has no single rule-level cause"
            );
            if slot["status"] == "Applied" {
                assert_eq!(slot["selectedRule"]["kind"], "morphRule");
            }
        }
    }
    let details = trace_details_for(&filter_pass_xml("exact-span"), "matinlu");
    let mut failed = Vec::new();
    nodes_of_type(&details["trace"], "Failed", &mut failed);
    let partials: Vec<_> = failed
        .into_iter()
        .filter(|node| node["failureReason"] == "PartialParse")
        .collect();
    assert!(!partials.is_empty());
    for node in partials {
        assert!(node["partialParseCause"].is_string());
        assert!(node["stepId"].is_string());
        assert!(node.get("causedByStepId").is_none());
    }
}

#[test]
fn compound_analysis_attempts_keep_owner_reason_and_subrule() {
    let details = trace_details_for(&edge_case_xml("compounding-non-recursive"), "fasu");
    let mut compounds = Vec::new();
    nodes_of_type(&details["trace"], "CompoundingRuleAnalysis", &mut compounds);
    assert!(
        !compounds.is_empty(),
        "bare fasu must attempt the compound rule"
    );
    assert!(compounds
        .iter()
        .any(|node| node["inputShape"] == "fasu" && node["failureReason"] == "Pattern"));
    for node in compounds {
        assert_eq!(node["sourceIdentity"]["id"], "cr1");
        assert!(node["subrule"].is_number());
    }
    let details = trace_details_for(&edge_case_xml("compounding-non-recursive"), "fasubel");
    let mut compounds = Vec::new();
    nodes_of_type(&details["trace"], "CompoundingRuleAnalysis", &mut compounds);
    assert!(compounds.iter().any(|node| node["outputShape"].is_string()));
}

#[test]
fn phonological_non_unapplication_keeps_unknown_reason_explicit() {
    let details = trace_details_for(
        &edge_case_xml("rewrite-analysis-feature-neutralization"),
        "n",
    );
    let mut attempts = Vec::new();
    nodes_of_type(&details["trace"], "PhonologicalRuleAnalysis", &mut attempts);
    let failures: Vec<_> = attempts
        .into_iter()
        .filter(|node| node.get("outputShape").is_none())
        .collect();
    assert!(!failures.is_empty());
    for node in failures {
        assert_eq!(node["nonUnapplicationReason"]["status"], "unavailable");
        assert_eq!(
            node["nonUnapplicationReason"]["unavailableReason"],
            "evaluator-returned-boolean-only"
        );
        assert!(node.get("failureReason").is_none());
        assert!(node["inputShape"].is_string());
        assert!(node["subrule"].is_number());
    }
}

#[test]
fn environments_record_failed_allomorph_span_and_actual_alternative_results() {
    let details = trace_details_for(&filter_pass_xml("allomorph-compatibility"), "kapita");
    let mut failed = Vec::new();
    nodes_of_type(&details["trace"], "Failed", &mut failed);
    let envs: Vec<_> = failed
        .into_iter()
        .filter(|node| node["failureReason"] == "Environments")
        .collect();
    assert!(!envs.is_empty());
    for node in envs {
        let evidence = &node["failureContext"]["evidence"];
        assert_eq!(evidence["kind"], "environments");
        assert_eq!(evidence["failedObject"]["kind"], "allomorph");
        assert_eq!(evidence["span"]["coordinateSystem"], "interior-inclusive");
        let alternatives = evidence["alternatives"].as_array().unwrap();
        assert!(!alternatives.is_empty());
        for env in alternatives {
            assert_eq!(env["accepted"], false);
            assert_eq!(env["sourceStatus"], "unavailable");
            assert!(env["authoredText"].is_null());
        }
    }
}

#[test]
fn authored_environment_provenance_reaches_json_without_changing_legacy_display() {
    let xml = &filter_pass_xml("allomorph-compatibility");
    let baseline = trace_details_for(xml, "kapita");
    let authored = xml.replace(
        "<Environment>",
        "<Environment id=\"env-authored\">/_ [Vowel]",
    );
    let details = trace_details_for(&authored, "kapita");
    let mut baseline_nodes = Vec::new();
    let mut authored_nodes = Vec::new();
    nodes_of_type(&baseline["trace"], "Failed", &mut baseline_nodes);
    nodes_of_type(&details["trace"], "Failed", &mut authored_nodes);
    let env_nodes = |nodes: Vec<&serde_json::Value>| {
        nodes
            .into_iter()
            .filter(|node| node["failureReason"] == "Environments")
            .cloned()
            .collect::<Vec<_>>()
    };
    let baseline_envs = env_nodes(baseline_nodes);
    let authored_envs = env_nodes(authored_nodes);
    assert!(!authored_envs.is_empty());
    assert_eq!(authored_envs.len(), baseline_envs.len());
    for (before, after) in baseline_envs.iter().zip(&authored_envs) {
        assert_eq!(
            after["failureContext"]["environment"],
            before["failureContext"]["environment"]
        );
        for env in after["failureContext"]["evidence"]["alternatives"]
            .as_array()
            .unwrap()
        {
            assert_eq!(env["sourceStatus"], "captured");
            assert_eq!(env["sourceIdentity"]["id"], "env-authored");
            assert_eq!(env["authoredText"].as_str().unwrap().trim(), "/_ [Vowel]");
            assert_eq!(env["accepted"], false);
        }
    }
}

#[test]
fn compound_head_mpr_rejection_records_operands() {
    let details = trace_details_for(&edge_case_xml("compounding-non-recursive"), "numobel");
    let mut failed = Vec::new();
    nodes_of_type(&details["trace"], "CompoundingRuleSynthesis", &mut failed);
    let gates: Vec<_> = failed
        .into_iter()
        .filter(|node| node["failureReason"] == "HeadProdRestrictMprFeatures")
        .collect();
    assert!(!gates.is_empty());
    for node in gates {
        let evidence = &node["failureContext"]["evidence"];
        assert_eq!(evidence["kind"], "mprFeatures");
        assert_eq!(evidence["failedObject"]["id"], "cr1");
        assert!(!evidence["operands"]["required"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(evidence["operands"]["actual"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}

#[test]
fn morpheme_co_occurrence_rejection_preserves_the_ordered_partner_list() {
    let xml = &filter_pass_xml("co-occurrence");
    let xml = xml.replace(
        "otherMorphemes=\"mrFut\"",
        "otherMorphemes=\"mrFut mrEmph\"",
    );
    let details = trace_details_for(&xml, "tarosiluna");
    let mut failed = Vec::new();
    nodes_of_type(&details["trace"], "Failed", &mut failed);
    let gates: Vec<_> = failed
        .into_iter()
        .filter(|node| node["failureReason"] == "MorphemeCoOccurrenceRules")
        .collect();
    assert!(!gates.is_empty());
    for node in gates {
        let evidence = &node["failureContext"]["evidence"];
        assert_eq!(evidence["kind"], "coOccurrence");
        assert_eq!(evidence["failedObject"]["kind"], "morpheme");
        assert_eq!(evidence["failedObject"]["id"], "mrPast");
        assert_eq!(evidence["constraintOwner"], evidence["failedObject"]);
        assert_eq!(evidence["require"], false);
        assert_eq!(evidence["ruleIndex"], 0);
        let others = evidence["others"].as_array().unwrap();
        assert_eq!(others.len(), 2);
        assert_eq!(others[0]["id"], "mrFut");
        assert_eq!(others[1]["id"], "mrEmph");
        assert!(others.iter().all(|other| other["kind"] == "morpheme"));
        assert_eq!(evidence["actual"].as_array().unwrap().len(), 4);
    }
}

#[path = "tests/prerelease_evidence_tests.rs"]
mod prerelease_evidence_tests;
