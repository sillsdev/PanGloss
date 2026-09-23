use super::*;
use crate::identity::IDENTITY_PROFILE;
use crate::outcome::{BudgetDimension, IncompleteReason, NotAttemptedReason};
use crate::report::{Execution, Provenance, ReportDraft, Severity, SuiteRef};

fn id(morphemes: &[&str]) -> AnalysisIdentity {
    AnalysisIdentity {
        morphemes: morphemes.iter().map(|m| Some(m.to_string())).collect(),
        root_index: 0,
        category: None,
    }
}

fn report(cases: Vec<CaseRecord>) -> AssessmentReport {
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
            model_fingerprint: "sha256:model".into(),
            importer_version: "1".into(),
            compiler_version: "1".into(),
        },
        diagnostics: Vec::new(),
        cases,
        failure: None,
        extensions: None,
    }
    .finish()
    .expect("fixture report digests")
}

fn complete(case_id: &str, input: &str, analyses: &[AnalysisIdentity]) -> CaseRecord {
    CaseRecord {
        case_id: case_id.into(),
        input: input.into(),
        outcome: CaseOutcome::Complete(AnalysisSet::from_observed(analyses.to_vec())),
        supersedes: Vec::new(),
    }
}

fn only(delta: &GrammarDelta) -> &CaseDelta {
    assert_eq!(delta.cases.len(), 1, "fixture has one case");
    &delta.cases[0]
}

#[test]
fn two_analyses_replaced_by_two_others_is_mixed_not_no_change() {
    // The FieldWorks defect, pinned: ParserReport.cs subtracts counts, so 2 - 2 = 0 reads as no change even when every analysis was replaced.
    let base = report(vec![complete("c1", "w", &[id(&["a"]), id(&["b"])])]);
    let cand = report(vec![complete("c1", "w", &[id(&["x"]), id(&["y"])])]);

    let delta = compare(&base, &cand).unwrap();
    let case = only(&delta);
    assert_eq!(case.category, DeltaCategory::Mixed);
    assert_eq!(case.added.len(), 2);
    assert_eq!(case.removed.len(), 2);
    assert!(case.retained.is_empty());
    assert!(!delta.outcome_digests_agree);
}

#[test]
fn a_deleted_morpheme_is_removed_evidence_not_a_refusal() {
    // A key present on one side and absent on the other is the most ordinary FieldWorks edit there is.
    let base = report(vec![complete("c1", "w", &[id(&["stem", "affix"])])]);
    let cand = report(vec![complete("c1", "w", &[])]);

    let case = &compare(&base, &cand).unwrap().cases[0];
    assert_eq!(case.category, DeltaCategory::RemovedOnly);
    assert_eq!(case.removed, vec![id(&["stem", "affix"])]);
    assert_eq!(case.reason, None);
}

#[test]
fn discovery_order_does_not_change_any_category() {
    let base = report(vec![complete("c1", "w", &[id(&["a"]), id(&["b"])])]);
    let cand = report(vec![complete("c1", "w", &[id(&["b"]), id(&["a"])])]);
    assert_eq!(
        compare(&base, &cand).unwrap().cases[0].category,
        DeltaCategory::Unchanged
    );
}

#[test]
fn a_guessed_flip_on_a_retained_identity_is_a_changed_case() {
    // The morpheme sequence and category are identical, but the root stopped being found in the lexicon and the parser fabricated one; burying that as unchanged would hide a real regression.
    let base = report(vec![CaseRecord {
        case_id: "c1".into(),
        input: "w".into(),
        outcome: CaseOutcome::Complete(AnalysisSet::from_annotated([(id(&["a"]), false)])),
        supersedes: Vec::new(),
    }]);
    let cand = report(vec![CaseRecord {
        case_id: "c1".into(),
        input: "w".into(),
        outcome: CaseOutcome::Complete(AnalysisSet::from_annotated([(id(&["a"]), true)])),
        supersedes: Vec::new(),
    }]);

    let case = &compare(&base, &cand).unwrap().cases[0];
    assert_eq!(case.category, DeltaCategory::AnnotationChanged);
    assert!(case.category.is_changed());
    assert_eq!(case.retained, vec![id(&["a"])]);
    assert!(case.added.is_empty() && case.removed.is_empty());
    assert!(case.annotation_changes[0].candidate_guessed);
}

#[test]
fn a_duplicate_count_move_is_a_flag_not_a_change() {
    // The opposite of a guessed flip: redundant proposal paths doing more or less work for identical linguistic output, so forcing investigation on it would generate noise on every FST tuning change.
    let base = report(vec![complete("c1", "w", &[id(&["a"])])]);
    let cand = report(vec![complete("c1", "w", &[id(&["a"]), id(&["a"])])]);

    let case = &compare(&base, &cand).unwrap().cases[0];
    assert_eq!(case.category, DeltaCategory::Unchanged);
    assert!(!case.category.is_changed());
    assert_eq!(case.duplicate_count_changes[0].baseline_count, 1);
    assert_eq!(case.duplicate_count_changes[0].candidate_count, 2);
}

#[test]
fn a_complete_empty_set_and_an_incomplete_outcome_are_not_the_same_case() {
    // If these compared alike, a budget trip would read as "the grammar analyzes this no way at all" -- the exact conflation XAmpleParser.cs makes.
    let base = report(vec![complete("c1", "w", &[id(&["a"])])]);
    let cand = report(vec![CaseRecord {
        case_id: "c1".into(),
        input: "w".into(),
        outcome: CaseOutcome::Incomplete(IncompleteReason::LogicalBudget {
            dimension: BudgetDimension::Candidates,
            value: 5000,
            limit: 4096,
        }),
        supersedes: Vec::new(),
    }]);

    let case = &compare(&base, &cand).unwrap().cases[0];
    assert_eq!(case.category, DeltaCategory::CompletenessChanged);
    assert!(case.category.is_changed());
    assert!(
        case.removed.is_empty(),
        "an incomplete side must not read as everything removed"
    );

    let empty = report(vec![complete("c1", "w", &[])]);
    assert_eq!(
        compare(&base, &empty).unwrap().cases[0].category,
        DeltaCategory::RemovedOnly,
        "a complete empty set IS a positive claim, and differs from incomplete"
    );
}

#[test]
fn both_sides_stopped_is_typed_not_prose() {
    let stop = || CaseRecord {
        case_id: "c1".into(),
        input: "w".into(),
        outcome: CaseOutcome::Incomplete(IncompleteReason::LogicalBudget {
            dimension: BudgetDimension::Candidates,
            value: 5000,
            limit: 4096,
        }),
        supersedes: Vec::new(),
    };
    let case = &compare(&report(vec![stop()]), &report(vec![stop()]))
        .unwrap()
        .cases[0];
    assert_eq!(case.category, DeltaCategory::NotComparable);
    assert_eq!(case.reason, Some(NotComparableReason::BothIncomplete));

    let skipped = || CaseRecord {
        case_id: "c1".into(),
        input: "w".into(),
        outcome: CaseOutcome::NotAttempted(NotAttemptedReason::BatchBudgetExhausted),
        supersedes: Vec::new(),
    };
    assert_eq!(
        compare(&report(vec![skipped()]), &report(vec![skipped()]))
            .unwrap()
            .cases[0]
            .reason,
        Some(NotComparableReason::BothNotAttempted)
    );
}

#[test]
fn one_case_id_asking_two_questions_is_refused() {
    let base = report(vec![complete("c1", "walked", &[id(&["a"])])]);
    let cand = report(vec![complete("c1", "walking", &[id(&["a"])])]);
    let case = &compare(&base, &cand).unwrap().cases[0];
    assert_eq!(case.category, DeltaCategory::NotComparable);
    assert_eq!(
        case.reason,
        Some(NotComparableReason::CaseDefinitionChanged)
    );
}

#[test]
fn two_cases_sharing_a_surface_form_stay_distinct() {
    // What a word-keyed map (`ParserReport.cs:127-129`) structurally cannot do.
    let base = report(vec![
        complete("c1", "bank", &[id(&["bank-money"])]),
        complete("c2", "bank", &[id(&["bank-river"])]),
    ]);
    let cand = report(vec![
        complete("c1", "bank", &[id(&["bank-money"])]),
        complete("c2", "bank", &[]),
    ]);

    let delta = compare(&base, &cand).unwrap();
    assert_eq!(delta.cases.len(), 2);
    assert_eq!(delta.cases[0].category, DeltaCategory::Unchanged);
    assert_eq!(delta.cases[1].category, DeltaCategory::RemovedOnly);
}

#[test]
fn a_renumbered_case_is_followed_through_supersedes() {
    let base = report(vec![complete("old-1", "w", &[id(&["a"])])]);
    let mut draft = report(vec![complete("new-1", "w", &[id(&["a"])])])
        .draft()
        .clone();
    draft.cases[0].supersedes = vec!["old-1".into()];
    let cand = draft.finish().unwrap();

    let delta = compare(&base, &cand).unwrap();
    assert_eq!(
        delta.cases.len(),
        1,
        "no phantom baseline_only/candidate_only"
    );
    assert_eq!(delta.cases[0].category, DeltaCategory::Unchanged);
    assert_eq!(
        delta.cases[0].candidate_case_id.as_deref(),
        Some("new-1"),
        "the renumbering is visible, not silently absorbed"
    );
}

#[test]
fn an_unlinked_renumbering_is_one_sided_on_both_ends() {
    let base = report(vec![complete("old-1", "w", &[id(&["a"])])]);
    let cand = report(vec![complete("new-1", "w", &[id(&["a"])])]);
    let delta = compare(&base, &cand).unwrap();
    assert_eq!(delta.cases[0].category, DeltaCategory::BaselineOnly);
    assert_eq!(delta.cases[1].category, DeltaCategory::CandidateOnly);
    assert!(
        !delta.cases[0].category.is_changed() && !delta.cases[1].category.is_changed(),
        "inventory movement the caller already knows about is not a changed case"
    );
}

#[test]
fn output_is_baseline_order_then_candidate_only_in_candidate_order() {
    let base = report(vec![
        complete("b1", "w1", &[]),
        complete("b2", "w2", &[]),
        complete("shared", "w3", &[]),
    ]);
    let cand = report(vec![
        complete("shared", "w3", &[]),
        complete("c1", "w4", &[]),
        complete("c2", "w5", &[]),
    ]);
    let delta = compare(&base, &cand).unwrap();
    let ids: Vec<&str> = delta.cases.iter().map(|c| c.case_id.as_str()).collect();
    assert_eq!(ids, vec!["b1", "b2", "shared", "c1", "c2"]);
}

#[test]
fn incompatible_profiles_produce_a_valid_artifact_of_refusals() {
    // A refusal is still evidence: every case is typed, and the artifact is well formed rather than an error exit with nothing to read.
    let base = report(vec![complete("c1", "w", &[id(&["a"])])]);
    let mut draft = report(vec![complete("c1", "w", &[id(&["a"])])])
        .draft()
        .clone();
    draft.suite.analysis_identity_profile = "pangloss.machine-word-analysis/v2".into();
    let cand = draft.finish().unwrap();

    let delta = compare(&base, &cand).unwrap();
    assert!(delta
        .cases
        .iter()
        .all(|c| c.category == DeltaCategory::NotComparable
            && c.reason == Some(NotComparableReason::IdentityProfileChanged)));
    let value = delta.to_value();
    assert_eq!(value["schema"], json!(DELTA_SCHEMA));
    assert_eq!(value["summary"]["totalCases"], json!(1));
}

#[test]
fn a_compiler_upgrade_is_context_evidence_and_never_gates_comparison() {
    let base = report(vec![complete("c1", "w", &[id(&["a"])])]);
    let mut draft = report(vec![complete("c1", "w", &[id(&["a"])])])
        .draft()
        .clone();
    draft.provenance.compiler_version = "2".into();
    let cand = draft.finish().unwrap();

    let delta = compare(&base, &cand).unwrap();
    assert_eq!(delta.cases[0].category, DeltaCategory::Unchanged);
    assert!(delta
        .context_differences
        .iter()
        .any(|d| d.field == "provenance.compilerVersion"));
    assert!(
        delta.outcome_digests_agree,
        "the grammar behaved identically across the upgrade"
    );
}

#[test]
fn rewording_a_diagnostic_is_not_a_context_difference() {
    let with = |message: &str| {
        let mut draft = report(vec![complete("c1", "w", &[id(&["a"])])])
            .draft()
            .clone();
        draft.diagnostics = vec![Diagnostic {
            code: "dangling-reference".into(),
            severity: Severity::Warning,
            message: message.into(),
        }];
        draft.finish().unwrap()
    };
    let delta = compare(&with("entry 4 lacks an MSA"), &with("reworded")).unwrap();
    assert!(!delta
        .context_differences
        .iter()
        .any(|d| d.field == "diagnostics"));
}

#[test]
fn one_more_warning_of_the_same_code_is_a_context_difference() {
    let with = |count: usize| {
        let mut draft = report(vec![complete("c1", "w", &[id(&["a"])])])
            .draft()
            .clone();
        draft.diagnostics = (0..count)
            .map(|i| Diagnostic {
                code: "skipped-construct".into(),
                severity: Severity::Warning,
                message: format!("{i}"),
            })
            .collect();
        draft.finish().unwrap()
    };
    let delta = compare(&with(1), &with(400)).unwrap();
    assert!(
        delta
            .context_differences
            .iter()
            .any(|d| d.field == "diagnostics"),
        "'the importer skipped 399 more constructs' must be visible"
    );
}

#[test]
fn the_artifact_carries_denominators_and_no_rate() {
    let base = report(vec![
        complete("c1", "w1", &[id(&["a"])]),
        complete("c2", "w2", &[id(&["b"])]),
    ]);
    let cand = report(vec![
        complete("c1", "w1", &[id(&["a"])]),
        complete("c2", "w2", &[]),
    ]);
    let value = compare(&base, &cand).unwrap().to_value();
    assert_eq!(value["summary"]["totalCases"], json!(2));
    assert_eq!(value["summary"]["changedCases"], json!(1));
    assert_eq!(value["summary"]["byCategory"]["removed_only"], json!(1));
    let summary = value["summary"].as_object().unwrap();
    for forbidden in ["percent", "rate", "score", "better", "improvement"] {
        assert!(
            !summary.keys().any(|k| k.to_lowercase().contains(forbidden)),
            "the summary must not imply a verdict"
        );
    }
}
