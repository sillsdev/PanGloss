use super::*;
use crate::identity::IDENTITY_PROFILE;
use crate::outcome::{BudgetDimension, IncompleteReason, NotAttemptedReason};
use crate::report::{CaseRecord, Execution, Provenance, ReportDraft, SuiteRef};
use crate::suite::parse_suite;

fn id_json(morpheme: &str) -> String {
    format!(r#"{{"morphemes":["{morpheme}"],"rootIndex":0,"category":null}}"#)
}

fn id(morpheme: &str) -> AnalysisIdentity {
    AnalysisIdentity {
        morphemes: vec![Some(morpheme.to_string())],
        root_index: 0,
        category: None,
    }
}

/// A one-case suite whose expectation body is supplied verbatim.
fn suite_with(expectation: &str) -> ValidatedSuite {
    let document = format!(
        r#"{{
              "schema": "pangloss.assessment-suite",
              "schemaVersion": 1,
              "suiteId": "s",
              "suiteRevision": "r1",
              "analysisIdentityProfile": "{IDENTITY_PROFILE}",
              "cases": [
                {{ "caseId": "c1", "input": "w", "expectation": {expectation} }}
              ]
            }}"#
    );
    parse_suite(&document).expect("fixture suite is valid")
}

fn suite_without_expectation() -> ValidatedSuite {
    let document = format!(
        r#"{{
              "schema": "pangloss.assessment-suite",
              "schemaVersion": 1,
              "suiteId": "s",
              "suiteRevision": "r1",
              "analysisIdentityProfile": "{IDENTITY_PROFILE}",
              "cases": [ {{ "caseId": "c1", "input": "w" }} ]
            }}"#
    );
    parse_suite(&document).expect("fixture suite is valid")
}

fn report_for(suite: &ValidatedSuite, outcome: CaseOutcome) -> AssessmentReport {
    ReportDraft {
        generated_at: "2026-07-29T00:00:00Z".into(),
        suite: SuiteRef {
            suite_id: suite.suite().suite_id.clone(),
            suite_revision: suite.suite().suite_revision.clone(),
            semantic_digest: suite.semantic_digest().to_string(),
            analysis_identity_profile: IDENTITY_PROFILE.into(),
        },
        execution: Execution {
            pipeline: "foma-confirm".into(),
            ..Execution::default()
        },
        provenance: Provenance {
            source_sha256: "sha256:src".into(),
            source_kind: "hc-xml".into(),
            model_fingerprint: "sha256:model".into(),
            importer_version: "1".into(),
            compiler_version: "1".into(),
        },
        diagnostics: Vec::new(),
        cases: vec![CaseRecord {
            case_id: "c1".into(),
            input: "w".into(),
            outcome,
            supersedes: Vec::new(),
        }],
        failure: None,
        extensions: None,
    }
    .finish()
    .expect("fixture report digests")
}

fn complete(analyses: &[AnalysisIdentity]) -> CaseOutcome {
    CaseOutcome::Complete(AnalysisSet::from_observed(analyses.to_vec()))
}

fn only(diff: &GoldenSetDiff) -> &GoldenCase {
    assert_eq!(diff.cases.len(), 1);
    &diff.cases[0]
}

#[test]
fn a_required_analysis_present_agrees() {
    let suite = suite_with(&format!(
        r#"{{ "status": "adjudicated", "required": [{}] }}"#,
        id_json("a")
    ));
    let report = report_for(&suite, complete(&[id("a")]));
    let diff = golden_diff(&report, &suite).unwrap();
    let case = only(&diff);
    assert_eq!(case.verdict, Verdict::Agrees);
    assert_eq!(case.matching_required, vec![id("a")]);
    assert!(case.missing_required.is_empty());
}

#[test]
fn a_missing_required_analysis_is_named_not_merely_counted() {
    let suite = suite_with(&format!(
        r#"{{ "status": "adjudicated", "required": [{}, {}] }}"#,
        id_json("a"),
        id_json("b")
    ));
    let report = report_for(&suite, complete(&[id("a")]));
    let case = &golden_diff(&report, &suite).unwrap().cases[0];
    assert_eq!(case.verdict, Verdict::Disagrees);
    assert_eq!(
        case.missing_required,
        vec![id("b")],
        "which analysis went missing must be readable off the artifact"
    );
}

#[test]
fn an_observed_forbidden_analysis_disagrees() {
    let suite = suite_with(&format!(
        r#"{{ "status": "adjudicated", "forbidden": [{}] }}"#,
        id_json("bad")
    ));
    let report = report_for(&suite, complete(&[id("bad")]));
    let case = &golden_diff(&report, &suite).unwrap().cases[0];
    assert_eq!(case.verdict, Verdict::Disagrees);
    assert_eq!(case.observed_forbidden, vec![id("bad")]);
}

#[test]
fn an_allowed_analysis_neither_demands_nor_bans() {
    let suite = suite_with(&format!(
        r#"{{ "status": "adjudicated", "required": [{}], "allowed": [{}] }}"#,
        id_json("a"),
        id_json("maybe")
    ));
    // Present: fine.
    let with = report_for(&suite, complete(&[id("a"), id("maybe")]));
    let case = &golden_diff(&with, &suite).unwrap().cases[0];
    assert_eq!(case.verdict, Verdict::Agrees);
    assert_eq!(case.matching_allowed, vec![id("maybe")]);
    // Absent: also fine.
    let without = report_for(&suite, complete(&[id("a")]));
    assert_eq!(
        golden_diff(&without, &suite).unwrap().cases[0].verdict,
        Verdict::Agrees
    );
}

#[test]
fn an_undeclared_analysis_disagrees_only_under_a_closed_world() {
    let open = suite_with(&format!(
        r#"{{ "status": "adjudicated", "required": [{}] }}"#,
        id_json("a")
    ));
    let report = report_for(&open, complete(&[id("a"), id("surprise")]));
    let case = &golden_diff(&report, &open).unwrap().cases[0];
    assert_eq!(case.verdict, Verdict::Agrees);
    assert_eq!(
        case.unexpected,
        vec![id("surprise")],
        "recorded even when tolerated"
    );

    let closed = suite_with(&format!(
        r#"{{ "status": "adjudicated", "closedWorld": true, "required": [{}] }}"#,
        id_json("a")
    ));
    let report = report_for(&closed, complete(&[id("a"), id("surprise")]));
    assert_eq!(
        golden_diff(&report, &closed).unwrap().cases[0].verdict,
        Verdict::Disagrees
    );
}

#[test]
fn closed_world_with_nothing_declared_asserts_ungrammaticality() {
    // The encoding for "this form should not parse at all". An empty complete set satisfies it.
    let suite = suite_with(r#"{ "status": "adjudicated", "closedWorld": true }"#);
    let report = report_for(&suite, complete(&[]));
    assert_eq!(
        golden_diff(&report, &suite).unwrap().cases[0].verdict,
        Verdict::Agrees
    );

    let parsed = report_for(&suite, complete(&[id("a")]));
    let case = &golden_diff(&parsed, &suite).unwrap().cases[0];
    assert_eq!(case.verdict, Verdict::Disagrees);
    assert_eq!(case.unexpected, vec![id("a")]);
}

#[test]
fn an_incomplete_case_never_satisfies_an_empty_closed_world_expectation() {
    // A budget trip must not read as "the grammar analyzes this no way at all".
    let suite = suite_with(r#"{ "status": "adjudicated", "closedWorld": true }"#);
    let report = report_for(
        &suite,
        CaseOutcome::Incomplete(IncompleteReason::LogicalBudget {
            dimension: BudgetDimension::Candidates,
            value: 5000,
            limit: 4096,
        }),
    );
    let case = &golden_diff(&report, &suite).unwrap().cases[0];
    assert_eq!(case.verdict, Verdict::NotEvaluable);
    assert_eq!(
        case.not_evaluable_reason,
        Some(NotEvaluableReason::Incomplete)
    );
    assert_ne!(case.verdict, Verdict::Agrees);
}

#[test]
fn a_not_attempted_case_is_not_evaluable_with_its_own_reason() {
    let suite = suite_with(&format!(
        r#"{{ "status": "adjudicated", "required": [{}] }}"#,
        id_json("a")
    ));
    let report = report_for(
        &suite,
        CaseOutcome::NotAttempted(NotAttemptedReason::BatchBudgetExhausted),
    );
    let case = &golden_diff(&report, &suite).unwrap().cases[0];
    assert_eq!(case.verdict, Verdict::NotEvaluable);
    assert_eq!(
        case.not_evaluable_reason,
        Some(NotEvaluableReason::NotAttempted)
    );
}

#[test]
fn only_adjudicated_expectations_produce_agreement() {
    for (status, reason) in [
        ("unresolved", NotAdjudicatedReason::Unresolved),
        ("out_of_scope", NotAdjudicatedReason::OutOfScope),
        ("invalid", NotAdjudicatedReason::Invalid),
    ] {
        let suite = suite_with(&format!(
            r#"{{ "status": "{status}", "required": [{}] }}"#,
            id_json("a")
        ));
        let report = report_for(&suite, complete(&[id("a")]));
        let case = &golden_diff(&report, &suite).unwrap().cases[0];
        assert_eq!(case.verdict, Verdict::NotAdjudicated, "{status}");
        assert_eq!(case.not_adjudicated_reason, Some(reason), "{status}");
    }
}

#[test]
fn a_case_with_no_expectation_is_not_adjudicated_rather_than_passing() {
    let suite = suite_without_expectation();
    let report = report_for(&suite, complete(&[id("a")]));
    let case = &golden_diff(&report, &suite).unwrap().cases[0];
    assert_eq!(case.verdict, Verdict::NotAdjudicated);
    assert_eq!(
        case.not_adjudicated_reason,
        Some(NotAdjudicatedReason::NoExpectation)
    );
}

#[test]
fn an_unadjudicated_case_that_did_not_run_reports_adjudication_not_completeness() {
    // Finishing the run would not have produced a verdict, so `not_evaluable` is the wrong reason.
    let suite = suite_with(r#"{ "status": "unresolved" }"#);
    let report = report_for(
        &suite,
        CaseOutcome::NotAttempted(NotAttemptedReason::BatchBudgetExhausted),
    );
    assert_eq!(
        golden_diff(&report, &suite).unwrap().cases[0].verdict,
        Verdict::NotAdjudicated
    );
}

#[test]
fn a_revised_suite_is_refused_against_an_old_report() {
    let original = suite_with(&format!(
        r#"{{ "status": "adjudicated", "required": [{}] }}"#,
        id_json("a")
    ));
    let report = report_for(&original, complete(&[id("a")]));

    // Same id and revision, edited expectation — so only the semantic digest catches it.
    let revised = suite_with(&format!(
        r#"{{ "status": "adjudicated", "required": [{}] }}"#,
        id_json("b")
    ));
    match golden_diff(&report, &revised) {
        Err(GoldenError::SuiteMismatch { field, .. }) => {
            assert_eq!(field, "suiteSemanticDigest")
        }
        other => panic!("a revised suite must be refused, got {other:?}"),
    }
}

#[test]
fn every_aggregate_carries_its_denominator_and_no_rate() {
    let suite = suite_with(&format!(
        r#"{{ "status": "adjudicated", "required": [{}] }}"#,
        id_json("a")
    ));
    let report = report_for(&suite, complete(&[id("a")]));
    let value = golden_diff(&report, &suite).unwrap().to_value();
    let summary = value["summary"].as_object().unwrap();

    for denominator in [
        "totalCases",
        "adjudicatedAndEvaluable",
        "agrees",
        "disagrees",
        "notEvaluable",
        "notAdjudicated",
    ] {
        assert!(summary.contains_key(denominator), "{denominator}");
    }
    for forbidden in ["percent", "rate", "score", "better", "pass", "fail"] {
        assert!(
            !summary.keys().any(|k| k.to_lowercase().contains(forbidden)),
            "the summary must not imply a verdict on the grammar: {forbidden}"
        );
    }
}

#[test]
fn evaluation_never_writes_to_the_suite() {
    let suite = suite_with(&format!(
        r#"{{ "status": "adjudicated", "required": [{}] }}"#,
        id_json("a")
    ));
    let before = suite.semantic_digest().to_string();
    let report = report_for(&suite, complete(&[id("wrong")]));
    let _ = golden_diff(&report, &suite).unwrap();
    assert_eq!(
        suite.semantic_digest(),
        before,
        "a disagreement must not amend the expectation that produced it"
    );
}
