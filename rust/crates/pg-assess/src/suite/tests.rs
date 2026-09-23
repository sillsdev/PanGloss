use super::*;
use serde_json::json;

fn identity(morpheme: &str) -> Value {
    json!({ "morphemes": [morpheme], "rootIndex": 0, "category": null })
}

fn suite_json(cases: Value) -> String {
    json!({
        "schema": SUITE_SCHEMA,
        "schemaVersion": SUITE_SCHEMA_VERSION,
        "suiteId": "s",
        "suiteRevision": "r1",
        "analysisIdentityProfile": IDENTITY_PROFILE,
        "cases": cases,
    })
    .to_string()
}

#[test]
fn a_minimal_suite_parses_and_keeps_case_order() {
    let s = parse_suite(&suite_json(json!([
        { "caseId": "c2", "input": "beta" },
        { "caseId": "c1", "input": "alpha" },
    ])))
    .unwrap();
    // Declared order is authoritative and must not be sorted into something tidier.
    let ids: Vec<&str> = s.cases().iter().map(|c| c.case_id.as_str()).collect();
    assert_eq!(ids, ["c2", "c1"]);
}

#[test]
fn duplicate_surface_forms_stay_distinct_cases() {
    // Two occurrences of one word are two questions, not one.
    let s = parse_suite(&suite_json(json!([
        { "caseId": "c1", "input": "same" },
        { "caseId": "c2", "input": "same" },
    ])))
    .unwrap();
    assert_eq!(s.cases().len(), 2);
}

#[test]
fn duplicate_case_ids_are_refused() {
    let err = parse_suite(&suite_json(json!([
        { "caseId": "dup", "input": "a" },
        { "caseId": "dup", "input": "b" },
    ])))
    .unwrap_err();
    assert_eq!(
        err,
        SuiteError::DuplicateCaseId {
            case_id: "dup".to_string()
        }
    );
}

#[test]
fn overlapping_expectation_sets_are_refused() {
    let err = parse_suite(&suite_json(json!([{
        "caseId": "c1",
        "input": "a",
        "expectation": {
            "status": "adjudicated",
            "required": [identity("m")],
            "forbidden": [identity("m")],
        }
    }])))
    .unwrap_err();
    match err {
        SuiteError::OverlappingExpectation { case_id, .. } => assert_eq!(case_id, "c1"),
        other => panic!("expected an overlap error, got {other:?}"),
    }
}

#[test]
fn the_same_identity_repeated_within_one_set_is_not_an_overlap() {
    parse_suite(&suite_json(json!([{
        "caseId": "c1",
        "input": "a",
        "expectation": {
            "status": "adjudicated",
            "required": [identity("m"), identity("m")],
        }
    }])))
    .expect("a repeated identity inside one set is redundant, not contradictory");
}

#[test]
fn an_unsupported_schema_version_is_typed_not_best_effort() {
    let doc = suite_json(json!([])).replace(r#""schemaVersion":1"#, r#""schemaVersion":9"#);
    assert_eq!(
        parse_suite(&doc).unwrap_err(),
        SuiteError::UnsupportedSchemaVersion {
            found: 9,
            supported: 1
        }
    );
}

#[test]
fn another_identity_profile_is_refused_rather_than_reinterpreted() {
    let doc = suite_json(json!([])).replace(IDENTITY_PROFILE, "some.other-profile/v3");
    match parse_suite(&doc).unwrap_err() {
        SuiteError::UnsupportedIdentityProfile { found, .. } => {
            assert_eq!(found, "some.other-profile/v3")
        }
        other => panic!("expected a profile error, got {other:?}"),
    }
}

#[test]
fn closed_world_with_no_identities_is_the_ungrammatical_declaration() {
    let s = parse_suite(&suite_json(json!([{
        "caseId": "c1",
        "input": "nonword",
        "expectation": { "status": "adjudicated", "closedWorld": true }
    }])))
    .unwrap();
    let e = s.cases()[0].expectation.as_ref().unwrap();
    assert!(e.closed_world && e.required.is_empty() && e.allowed.is_empty());
}

#[test]
fn every_expectation_status_round_trips() {
    for (text, expected) in [
        ("adjudicated", ExpectationStatus::Adjudicated),
        ("unresolved", ExpectationStatus::Unresolved),
        ("out_of_scope", ExpectationStatus::OutOfScope),
        ("invalid", ExpectationStatus::Invalid),
    ] {
        let s = parse_suite(&suite_json(json!([{
            "caseId": "c1",
            "input": "a",
            "expectation": { "status": text }
        }])))
        .unwrap();
        assert_eq!(s.cases()[0].expectation.as_ref().unwrap().status, expected);
    }
}

#[test]
fn only_adjudicated_expectations_are_evaluable() {
    assert!(ExpectationStatus::Adjudicated.is_adjudicated());
    for status in [
        ExpectationStatus::Unresolved,
        ExpectationStatus::OutOfScope,
        ExpectationStatus::Invalid,
    ] {
        assert!(!status.is_adjudicated());
    }
}

#[test]
fn unknown_caller_metadata_survives_and_participates_in_the_digest() {
    let with_extra = json!({
        "schema": SUITE_SCHEMA,
        "schemaVersion": SUITE_SCHEMA_VERSION,
        "suiteId": "s",
        "suiteRevision": "r1",
        "analysisIdentityProfile": IDENTITY_PROFILE,
        "cases": [{ "caseId": "c1", "input": "a" }],
        "somethingWeDoNotKnowAbout": { "keep": "me" },
    })
    .to_string();
    let plain = suite_json(json!([{ "caseId": "c1", "input": "a" }]));

    let a = parse_suite(&with_extra).unwrap();
    let b = parse_suite(&plain).unwrap();
    assert_eq!(a.raw()["somethingWeDoNotKnowAbout"]["keep"], json!("me"));
    assert_ne!(
        a.semantic_digest(),
        b.semantic_digest(),
        "unknown metadata is preserved exactly, so it must reach the digest"
    );
}

#[test]
fn the_digest_ignores_key_order_and_whitespace() {
    let a = r#"{ "schema":"pangloss.assessment-suite", "schemaVersion":1, "suiteId":"s",
                     "suiteRevision":"r1", "analysisIdentityProfile":"pangloss.machine-word-analysis/v1",
                     "cases":[{"caseId":"c1","input":"a"}] }"#;
    let b = r#"{"cases":[{"input":"a","caseId":"c1"}],"suiteRevision":"r1","suiteId":"s","schemaVersion":1,"analysisIdentityProfile":"pangloss.machine-word-analysis/v1","schema":"pangloss.assessment-suite"}"#;
    assert_eq!(
        parse_suite(a).unwrap().semantic_digest(),
        parse_suite(b).unwrap().semantic_digest()
    );
}

#[test]
fn case_lineage_is_recorded_never_inferred() {
    let s = parse_suite(&suite_json(json!([{
        "caseId": "new", "input": "a", "supersedes": ["old"]
    }])))
    .unwrap();
    assert_eq!(s.cases()[0].supersedes, ["old"]);
    // A superseded ID need not exist in this suite, since the case it replaces lived in an earlier revision this build does not have.
}

#[test]
fn oversized_source_references_are_refused() {
    let big = "x".repeat(MAX_SOURCE_REFERENCE_BYTES + 1);
    let err = parse_suite(&suite_json(json!([{
        "caseId": "c1", "input": "a", "sourceReferences": [big]
    }])))
    .unwrap_err();
    match err {
        SuiteError::SourceReferenceTooLarge { case_id, .. } => assert_eq!(case_id, "c1"),
        other => panic!("expected a size error, got {other:?}"),
    }
}

#[test]
fn source_references_are_carried_verbatim() {
    let reference = json!({ "kind": "fieldworks-occurrence", "value": { "opaque": [1, 2] } });
    let s = parse_suite(&suite_json(json!([{
        "caseId": "c1", "input": "a", "sourceReferences": [reference.clone()]
    }])))
    .unwrap();
    assert_eq!(s.cases()[0].source_references, vec![reference]);
}

#[test]
fn malformed_json_is_refused_with_its_reason() {
    assert!(matches!(
        parse_suite("{ not json").unwrap_err(),
        SuiteError::Malformed(_)
    ));
}
