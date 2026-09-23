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
    assert_eq!(value["schemaVersion"], "pangloss.trace-details.v2");
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
    assert_eq!(value["schemaVersion"], "pangloss.trace-details.v2");
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
    let grammar = pg_grammar::load(include_str!(
        "../../../../../conformance-staging/filter-passes/exact-span/grammar.xml"
    ))
    .unwrap();
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
    assert!(rich.contains("\"schemaVersion\":\"pangloss.trace-details.v2\""));
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
