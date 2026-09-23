use super::*;
use crate::identity::IDENTITY_PROFILE;
use crate::outcome::CaseOutcome;
use crate::report::{CaseRecord, Execution, Provenance, ReportDraft, SuiteRef};
use crate::set::AnalysisSet;

fn id(morpheme: &str) -> AnalysisIdentity {
    AnalysisIdentity {
        morphemes: vec![Some(morpheme.to_string())],
        root_index: 0,
        category: None,
    }
}

fn report(analyses: &[AnalysisIdentity]) -> AssessmentReport {
    ReportDraft {
        generated_at: "2026-07-29T00:00:00Z".into(),
        suite: SuiteRef {
            suite_id: "s".into(),
            suite_revision: "r1".into(),
            semantic_digest: "sha256:suite".into(),
            analysis_identity_profile: IDENTITY_PROFILE.into(),
        },
        execution: Execution {
            pipeline: "foma-confirm".into(),
            ..Execution::default()
        },
        provenance: Provenance {
            source_sha256: "sha256:src".into(),
            source_kind: "hc-xml".into(),
            model_fingerprint: "sha256:model-a".into(),
            importer_version: "1".into(),
            compiler_version: "1".into(),
        },
        diagnostics: Vec::new(),
        cases: vec![CaseRecord {
            case_id: "c1".into(),
            input: "walked".into(),
            outcome: CaseOutcome::Complete(AnalysisSet::from_observed(analyses.to_vec())),
            supersedes: Vec::new(),
        }],
        failure: None,
        extensions: None,
    }
    .finish()
    .expect("fixture report digests")
}

fn request(case_id: &str) -> HandoffRequest {
    HandoffRequest {
        case_id: case_id.into(),
        ..HandoffRequest::default()
    }
}

#[test]
fn the_handoff_binds_report_model_case_and_pipeline() {
    // The one thing FieldWorks' own tracer structurally cannot do.
    let report = report(&[id("a")]);
    let handoff = investigate(&report, &request("c1")).unwrap();
    assert_eq!(handoff.report_id, report.report_id());
    assert_eq!(handoff.model_fingerprint, "sha256:model-a");
    assert_eq!(handoff.case_id, "c1");
    assert_eq!(handoff.input, "walked");
    assert_eq!(handoff.pipeline, "foma-confirm");
}

#[test]
fn a_different_model_is_refused_rather_than_traced() {
    let report = report(&[id("a")]);
    let mut req = request("c1");
    req.current_model_fingerprint = Some("sha256:model-b".into());
    match investigate(&report, &req) {
        Err(HandoffError::ModelFingerprintMismatch { .. }) => {}
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_pipeline_the_report_did_not_use_is_refused() {
    let report = report(&[id("a")]);
    let mut req = request("c1");
    req.requested_pipeline = Some("hermitcrab".into());
    assert!(matches!(
        investigate(&report, &req),
        Err(HandoffError::PipelineMismatch { .. })
    ));
}

#[test]
fn an_unknown_case_is_refused() {
    assert_eq!(
        investigate(&report(&[id("a")]), &request("nope")),
        Err(HandoffError::UnknownCase("nope".into()))
    );
}

#[test]
fn a_proposer_recall_gap_is_attributed_to_us_not_to_the_grammar() {
    // The attribution that stops a reviewer editing a perfectly good grammar.
    let report = report(&[id("a")]);
    let mut req = request("c1");
    req.asked_about = vec![id("b")];
    req.causes = vec![(id("b"), MissingAnalysisCause::ProposerRecallGap)];

    let handoff = investigate(&report, &req).unwrap();
    assert_eq!(handoff.missing.len(), 1);
    assert_eq!(
        handoff.missing[0].cause,
        MissingAnalysisCause::ProposerRecallGap
    );
}

#[test]
fn an_unattributed_missing_analysis_stays_undetermined() {
    // Attribution needs both pipelines. Guessing would be the artifact overstating what we know.
    let report = report(&[id("a")]);
    let mut req = request("c1");
    req.asked_about = vec![id("b")];
    let handoff = investigate(&report, &req).unwrap();
    assert_eq!(handoff.missing[0].cause, MissingAnalysisCause::Undetermined);
}

#[test]
fn an_analysis_that_is_present_is_not_reported_missing() {
    let report = report(&[id("a")]);
    let mut req = request("c1");
    req.asked_about = vec![id("a"), id("b")];
    let handoff = investigate(&report, &req).unwrap();
    assert_eq!(handoff.missing.len(), 1);
    assert_eq!(handoff.missing[0].identity, id("b"));
}

#[test]
fn absent_evidence_is_labelled_unavailable_not_omitted() {
    let handoff = investigate(&report(&[id("a")]), &request("c1")).unwrap();
    assert_eq!(
        handoff.evidence.availability,
        EvidenceAvailability::Unavailable
    );
    assert!(handoff.evidence.note.is_some());
}

#[test]
fn regenerated_evidence_is_never_presented_as_captured() {
    let report = report(&[id("a")]);
    let mut req = request("c1");
    req.evidence = Some(Evidence {
        availability: EvidenceAvailability::Regenerated,
        engine: "hermitcrab".into(),
        note: Some(
            "re-run on the HermitCrab pipeline; the report was produced with foma-confirm".into(),
        ),
    });
    let value = investigate(&report, &req).unwrap().to_value();
    assert_eq!(value["evidence"]["availability"], json!("regenerated"));
    assert_eq!(value["evidence"]["engine"], json!("hermitcrab"));
}

#[test]
fn a_rule_reference_is_marked_compiler_assigned_not_dressed_as_a_source_id() {
    // Presenting a dense ordinal as a FieldWorks identity would send an investigator looking for something that is not there.
    let report = report(&[id("a")]);
    let mut req = request("c1");
    req.constructs = vec![
        ConstructRef::source("lexicalEntry", "b2c4-guid", Some("walk".into())),
        ConstructRef::compiler_assigned("morphologicalRule", 7, Some("PastTense".into())),
    ];
    let value = investigate(&report, &req).unwrap().to_value();
    assert_eq!(value["constructs"][0]["idKind"], json!("sourceId"));
    assert_eq!(value["constructs"][1]["idKind"], json!("compilerAssigned"));
}

#[test]
fn the_narrative_carries_typed_failure_reasons_verbatim() {
    // Named exactly as `pg_rules::trace::FailureReason`, so a Rust narrative and a C# trace name the same thing.
    let report = report(&[id("a")]);
    let mut req = request("c1");
    req.narrative = vec![NarrativeStep {
        candidate: "walk + ed".into(),
        at: ConstructRef::compiler_assigned("phonologicalRule", 2, None),
        failure_reason: "Environments".into(),
        detail: "the rule's left environment did not match".into(),
    }];
    let value = investigate(&report, &req).unwrap().to_value();
    assert_eq!(
        value["narrative"][0]["failureReason"],
        json!("Environments")
    );
}

#[test]
fn every_handoff_states_the_different_engine_caveat() {
    let value = investigate(&report(&[id("a")]), &request("c1"))
        .unwrap()
        .to_value();
    let caveat = value["caveat"]["text"].as_str().unwrap();
    assert!(caveat.contains("different implementation"));
    assert!(caveat.contains("not necessarily a grammar defect"));
}

#[test]
fn no_artifact_field_prescribes_a_grammar_edit() {
    // PanGloss supplies material, never a diagnosis — guarded structurally rather than trusted.
    let report = report(&[id("a")]);
    let mut req = request("c1");
    req.asked_about = vec![id("b")];
    req.narrative = vec![NarrativeStep {
        candidate: "walk + ed".into(),
        at: ConstructRef::compiler_assigned("morphologicalRule", 1, None),
        failure_reason: "SurfaceFormMismatch".into(),
        detail: "the synthesized surface form did not match the input".into(),
    }];
    let text = serde_json::to_string(&investigate(&report, &req).unwrap().to_value()).unwrap();

    // "caused by", "should", "fix", "add a", "remove the" — the vocabulary of a diagnosis.
    for prescriptive in [
        "caused by",
        "you should",
        "the fix",
        "must be changed",
        "root cause",
    ] {
        assert!(
            !text.to_lowercase().contains(prescriptive),
            "the handoff must not diagnose: found {prescriptive:?}"
        );
    }
}
