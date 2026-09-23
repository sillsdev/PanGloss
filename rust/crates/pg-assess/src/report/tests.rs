use super::*;
use crate::outcome::BudgetDimension;

fn identity(morphemes: &[Option<&str>], category: Option<&str>) -> AnalysisIdentity {
    AnalysisIdentity {
        morphemes: morphemes
            .iter()
            .map(|m| m.map(str::to_string))
            .collect::<Vec<_>>(),
        root_index: 0,
        category: category.map(str::to_string),
    }
}

fn suite_ref() -> SuiteRef {
    SuiteRef {
        suite_id: "suite-1".into(),
        suite_revision: "r1".into(),
        semantic_digest: "sha256:suite".into(),
        analysis_identity_profile: IDENTITY_PROFILE.into(),
    }
}

fn draft(cases: Vec<CaseRecord>) -> ReportDraft {
    ReportDraft {
        generated_at: "2026-07-29T00:00:00Z".into(),
        suite: suite_ref(),
        execution: Execution {
            pipeline: "foma-confirm".into(),
            ..Execution::default()
        },
        provenance: Provenance {
            source_sha256: "sha256:source".into(),
            source_kind: "hc-xml".into(),
            model_fingerprint: "sha256:model".into(),
            importer_version: "1".into(),
            compiler_version: "1".into(),
        },
        diagnostics: Vec::new(),
        cases,
        failure: None,
        extensions: None,
    }
}

fn complete(case_id: &str, input: &str, analyses: &[AnalysisIdentity]) -> CaseRecord {
    CaseRecord {
        case_id: case_id.into(),
        input: input.into(),
        outcome: CaseOutcome::Complete(AnalysisSet::from_observed(analyses.to_vec())),
        supersedes: Vec::new(),
    }
}

fn sample() -> ReportDraft {
    draft(vec![
        complete(
            "c1",
            "walked",
            &[
                identity(&[Some("guid-walk"), Some("guid-ed")], Some("guid-verb")),
                identity(&[Some("guid-walk")], Some("guid-noun")),
            ],
        ),
        complete("c2", "walked", &[]),
    ])
}

#[test]
fn a_timestamp_moves_only_the_report_id() {
    let a = sample().finish().unwrap();
    let mut later = sample();
    later.generated_at = "2027-01-01T00:00:00Z".into();
    let b = later.finish().unwrap();

    assert_ne!(a.report_id(), b.report_id());
    assert_eq!(a.semantic_digest(), b.semantic_digest());
    assert_eq!(a.outcome_digest(), b.outcome_digest());
}

#[test]
fn a_compiler_upgrade_moves_the_semantic_digest_but_not_the_outcome_digest() {
    // The query a diff tool asks constantly: "did the grammar's behaviour change?" independent of any PanGloss version bump.
    let a = sample().finish().unwrap();
    let mut upgraded = sample();
    upgraded.provenance.compiler_version = "2".into();
    let b = upgraded.finish().unwrap();

    assert_ne!(a.semantic_digest(), b.semantic_digest());
    assert_eq!(a.outcome_digest(), b.outcome_digest());
}

#[test]
fn a_source_hash_change_moves_neither_semantic_nor_outcome_digest() {
    // `core.autocrlf` gives the same grammar different bytes on Windows and Linux, so the source hash must stay out of the semantic projection.
    let a = sample().finish().unwrap();
    let mut relf = sample();
    relf.provenance.source_sha256 = "sha256:crlf-flavoured".into();
    let b = relf.finish().unwrap();

    assert_ne!(a.report_id(), b.report_id(), "the bytes really did differ");
    assert_eq!(a.semantic_digest(), b.semantic_digest());
    assert_eq!(a.outcome_digest(), b.outcome_digest());
}

#[test]
fn duplicate_counts_move_the_semantic_digest_only() {
    let once = sample().finish().unwrap();
    let mut twice = sample();
    twice.cases[0].outcome = CaseOutcome::Complete(AnalysisSet::from_observed([
        identity(&[Some("guid-walk"), Some("guid-ed")], Some("guid-verb")),
        identity(&[Some("guid-walk"), Some("guid-ed")], Some("guid-verb")),
        identity(&[Some("guid-walk")], Some("guid-noun")),
    ]));
    let b = twice.finish().unwrap();

    assert_ne!(once.semantic_digest(), b.semantic_digest());
    assert_eq!(once.outcome_digest(), b.outcome_digest());
}

#[test]
fn rewording_a_diagnostic_does_not_move_the_semantic_digest() {
    let mut a = sample();
    a.diagnostics = vec![Diagnostic {
        code: "dangling-reference".into(),
        severity: Severity::Warning,
        message: "entry 4 refers to a missing MSA".into(),
    }];
    let mut b = sample();
    b.diagnostics = vec![Diagnostic {
        code: "dangling-reference".into(),
        severity: Severity::Warning,
        message: "reworded entirely for clarity".into(),
    }];
    let (a, b) = (a.finish().unwrap(), b.finish().unwrap());

    assert_eq!(a.semantic_digest(), b.semantic_digest());
    assert_ne!(a.report_id(), b.report_id(), "the prose is still evidence");
}

#[test]
fn one_more_diagnostic_of_the_same_code_does_move_the_semantic_digest() {
    let mut a = sample();
    a.diagnostics = vec![Diagnostic {
        code: "skipped-construct".into(),
        severity: Severity::Warning,
        message: "x".into(),
    }];
    let mut b = sample();
    b.diagnostics = vec![
        Diagnostic {
            code: "skipped-construct".into(),
            severity: Severity::Warning,
            message: "x".into(),
        },
        Diagnostic {
            code: "skipped-construct".into(),
            severity: Severity::Warning,
            message: "y".into(),
        },
    ];
    assert_ne!(
        a.finish().unwrap().semantic_digest(),
        b.finish().unwrap().semantic_digest()
    );
}

#[test]
fn a_removed_analysis_moves_every_digest() {
    let a = sample().finish().unwrap();
    let mut fewer = sample();
    fewer.cases[0].outcome = CaseOutcome::Complete(AnalysisSet::from_observed([identity(
        &[Some("guid-walk")],
        Some("guid-noun"),
    )]));
    let b = fewer.finish().unwrap();

    assert_ne!(a.report_id(), b.report_id());
    assert_ne!(a.semantic_digest(), b.semantic_digest());
    assert_ne!(a.outcome_digest(), b.outcome_digest());
}

#[test]
fn a_wall_clock_stop_marks_the_report_unreproducible() {
    let mut d = sample();
    d.cases.push(CaseRecord {
        case_id: "c3".into(),
        input: "slow".into(),
        outcome: CaseOutcome::Incomplete(IncompleteReason::WallClockTimeout {
            elapsed_us: 2_000_000,
            limit_us: 1_000_000,
        }),
        supersedes: Vec::new(),
    });
    let report = d.finish().unwrap();
    assert!(!report.is_reproducible());
    assert_eq!(report.status(), AssessmentStatus::Partial);
}

#[test]
fn a_logical_budget_stop_leaves_the_report_reproducible() {
    let mut d = sample();
    d.cases.push(CaseRecord {
        case_id: "c3".into(),
        input: "big".into(),
        outcome: CaseOutcome::Incomplete(IncompleteReason::LogicalBudget {
            dimension: BudgetDimension::Candidates,
            value: 5000,
            limit: 4096,
        }),
        supersedes: Vec::new(),
    });
    let report = d.finish().unwrap();
    assert!(report.is_reproducible());
}

#[test]
fn only_a_complete_case_carries_analyses() {
    let mut d = sample();
    d.cases.push(CaseRecord {
        case_id: "c3".into(),
        input: "big".into(),
        outcome: CaseOutcome::Incomplete(IncompleteReason::LogicalBudget {
            dimension: BudgetDimension::Candidates,
            value: 5000,
            limit: 4096,
        }),
        supersedes: Vec::new(),
    });
    let value = d.finish().unwrap().to_value();
    let cases = value["cases"].as_array().unwrap();
    assert!(cases[2].get("analyses").is_none());
    assert!(cases[2].get("incomplete").is_some());
    assert_eq!(cases[1]["analyses"], json!([]), "complete but empty");
}

#[test]
fn keys_are_interned_and_appear_once() {
    let value = sample().finish().unwrap().to_value();
    let table = value["keyTable"].as_array().unwrap();
    let keys: Vec<&str> = table.iter().map(|k| k.as_str().unwrap()).collect();
    assert_eq!(
        keys,
        vec!["guid-ed", "guid-noun", "guid-verb", "guid-walk"],
        "sorted, deduplicated"
    );
    // `guid-walk` appears in both analyses of case 1 but only once in the table.
    let first = &value["cases"][0]["analyses"][0]["identity"];
    assert_eq!(first["morphemes"], json!([3, 0]));
    assert_eq!(first["category"], json!(2));
}

#[test]
fn a_guessed_root_interns_to_null_not_to_a_key() {
    let d = draft(vec![complete(
        "c1",
        "xyzzy",
        &[identity(&[None], Some("guid-noun"))],
    )]);
    let value = d.finish().unwrap().to_value();
    assert_eq!(
        value["cases"][0]["analyses"][0]["identity"]["morphemes"],
        json!([null])
    );
    assert_eq!(
        value["keyTable"],
        json!(["guid-noun"]),
        "a fabricated root contributes no key"
    );
}

#[test]
fn a_report_round_trips_through_its_own_artifact() {
    let original = sample().finish().unwrap();
    let json = original.to_canonical_json().unwrap();
    let read = parse_report(&json).unwrap();

    assert_eq!(read.report_id(), original.report_id());
    assert_eq!(read.semantic_digest(), original.semantic_digest());
    assert_eq!(read.outcome_digest(), original.outcome_digest());
    assert_eq!(read.cases(), original.cases());
}

#[test]
fn round_tripping_preserves_duplicate_counts_and_guessed() {
    let mut d = sample();
    d.cases[0].outcome = CaseOutcome::Complete(AnalysisSet::from_annotated([
        (identity(&[Some("guid-walk")], None), true),
        (identity(&[Some("guid-walk")], None), true),
    ]));
    let original = d.finish().unwrap();
    let read = parse_report(&original.to_canonical_json().unwrap()).unwrap();

    let entry = &read.cases()[0].outcome.analyses().unwrap().entries()[0];
    assert_eq!(entry.duplicate_count, 2);
    assert!(entry.guessed);
    assert_eq!(read.semantic_digest(), original.semantic_digest());
}

#[test]
fn a_report_from_another_identity_profile_is_refused() {
    let mut d = sample();
    d.suite.analysis_identity_profile = "pangloss.machine-word-analysis/v2".into();
    let json = d.finish().unwrap().to_canonical_json().unwrap();
    assert_eq!(
        parse_report(&json),
        Err(ReportError::ForeignIdentityProfile(
            "pangloss.machine-word-analysis/v2".into()
        ))
    );
}

#[test]
fn a_key_index_past_the_table_is_refused_rather_than_silently_dropped() {
    let mut value = sample().finish().unwrap().to_value();
    value["cases"][0]["analyses"][0]["identity"]["morphemes"] = json!([99]);
    let json = serde_json::to_string(&value).unwrap();
    assert_eq!(
        parse_report(&json),
        Err(ReportError::KeyIndexOutOfRange(99, 4))
    );
}

#[test]
fn an_unsupported_schema_version_is_a_typed_refusal() {
    let mut value = sample().finish().unwrap().to_value();
    value["schemaVersion"] = json!(2);
    let json = serde_json::to_string(&value).unwrap();
    assert_eq!(parse_report(&json), Err(ReportError::UnsupportedVersion(2)));
}

#[test]
fn extensions_survive_but_stay_out_of_both_semantic_projections() {
    let plain = sample().finish().unwrap();
    let mut annotated = sample();
    annotated.extensions = Some(json!({ "com.example.review": { "assignee": "sam" } }));
    let annotated = annotated.finish().unwrap();

    assert_eq!(plain.semantic_digest(), annotated.semantic_digest());
    assert_eq!(plain.outcome_digest(), annotated.outcome_digest());
    assert_ne!(plain.report_id(), annotated.report_id());
    assert_eq!(
        parse_report(&annotated.to_canonical_json().unwrap())
            .unwrap()
            .draft()
            .extensions,
        annotated.draft().extensions
    );
}
