use super::*;
use crate::identity::AnalysisIdentity;

fn complete_with(n: usize) -> CaseOutcome {
    CaseOutcome::Complete(AnalysisSet::from_observed((0..n).map(|i| {
        AnalysisIdentity {
            morphemes: vec![Some(format!("m{i}"))],
            root_index: 0,
            category: None,
        }
    })))
}

fn budget_stop() -> CaseOutcome {
    CaseOutcome::Incomplete(IncompleteReason::LogicalBudget {
        dimension: BudgetDimension::Candidates,
        value: 5000,
        limit: 4096,
    })
}

fn clock_stop() -> CaseOutcome {
    CaseOutcome::Incomplete(IncompleteReason::WallClockTimeout {
        elapsed_us: 2_000_000,
        limit_us: 1_000_000,
    })
}

#[test]
fn a_complete_empty_set_is_not_an_incomplete_outcome() {
    // The distinction a closed-world empty expectation depends on.
    let empty = complete_with(0);
    assert!(empty.is_complete());
    assert_eq!(empty.analyses().unwrap().len(), 0);
    assert!(!budget_stop().is_complete());
    assert!(budget_stop().analyses().is_none());
}

#[test]
fn an_incomplete_case_never_exposes_an_authoritative_set() {
    assert!(budget_stop().analyses().is_none());
    assert!(clock_stop().analyses().is_none());
    assert!(
        CaseOutcome::NotAttempted(NotAttemptedReason::BatchBudgetExhausted)
            .analyses()
            .is_none()
    );
}

#[test]
fn only_a_logical_budget_keeps_a_case_reproducible() {
    assert!(budget_stop().is_reproducible());
    assert!(!clock_stop().is_reproducible());
    assert!(complete_with(1).is_reproducible());
}

#[test]
fn batch_exhaustion_preserves_earlier_cases_and_reports_partial() {
    let outcomes = [
        complete_with(1),
        complete_with(2),
        CaseOutcome::NotAttempted(NotAttemptedReason::BatchBudgetExhausted),
    ];
    assert_eq!(derive_status(&outcomes), AssessmentStatus::Partial);
    assert!(outcomes[0].is_complete(), "earlier cases stay valid");
}

#[test]
fn a_policy_skipped_invalid_case_does_not_make_a_run_partial() {
    let outcomes = [
        complete_with(1),
        CaseOutcome::NotAttempted(NotAttemptedReason::CaseStatusInvalid),
    ];
    assert_eq!(derive_status(&outcomes), AssessmentStatus::Complete);
}

#[test]
fn a_setup_failure_with_nothing_completed_is_failed() {
    let outcomes = [
        CaseOutcome::NotAttempted(NotAttemptedReason::AssessmentSetupFailed),
        CaseOutcome::NotAttempted(NotAttemptedReason::AssessmentSetupFailed),
    ];
    assert_eq!(derive_status(&outcomes), AssessmentStatus::Failed);
}

#[test]
fn an_incomplete_case_with_nothing_completed_is_failed() {
    assert_eq!(derive_status(&[budget_stop()]), AssessmentStatus::Failed);
}

#[test]
fn an_empty_suite_is_complete_rather_than_failed() {
    assert_eq!(derive_status(&[]), AssessmentStatus::Complete);
}

#[test]
fn outcome_kinds_are_the_stable_names_the_delta_keys_on() {
    assert_eq!(complete_with(0).kind(), "complete");
    assert_eq!(budget_stop().kind(), "incomplete");
    assert_eq!(
        CaseOutcome::NotAttempted(NotAttemptedReason::BatchBudgetExhausted).kind(),
        "not_attempted"
    );
}
