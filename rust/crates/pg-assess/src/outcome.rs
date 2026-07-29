//! Atomic per-case outcomes: complete, incomplete, or not attempted.
//!
//! `CONTEXT.md`'s "Atomic word-analysis result" and "Batch analysis outcome" have specified this
//! three-way contract all along; only the code was missing a `not_attempted`. The distinction that
//! matters most is between a **complete empty** set — the grammar genuinely analyzes this form no
//! way at all, which is what a closed-world empty expectation asserts — and an **incomplete**
//! outcome, where analysis began and a limit stopped it. Collapsing the second into the first would
//! turn every timeout into a confident claim of ungrammaticality.
//!
//! ## Reproducibility
//!
//! Only a deterministic logical budget may decide a case's outcome in a reproducible assessment
//! (design D5). A wall-clock stop is machine-dependent: the same word completes on a fast laptop and
//! stops on a loaded CI runner, so a report containing one cannot promise that re-running it
//! anywhere reproduces its `outcomeDigest`. Rather than hide that, [`IncompleteReason::is_reproducible`]
//! reports it and the report records `reproducible: false`.

use serde::{Deserialize, Serialize};

use crate::set::AnalysisSet;

/// Which magnitude a logical budget counts.
///
/// A closed enum rather than a free string, so the artifact's `dimension` cannot drift from what
/// the engine actually tripped on: the conversion at the engine boundary is a `match` the compiler
/// checks, and a new engine dimension has to be named here before it can reach an artifact. The
/// alternative — each caller spelling its own string — makes a typo silently produce a dimension no
/// consumer can branch on, which is the same "cannot act on prose" failure the typed
/// `not_comparable` reasons exist to avoid.
///
/// This crate stays engine-agnostic, so these are named after the quantity rather than after any
/// one engine's internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BudgetDimension {
    /// Raw paths pulled from the proposer before decoding.
    DecodedPaths,
    /// Distinct candidates offered to confirmation.
    Candidates,
    /// HermitCrab analysis steps.
    HermitcrabSteps,
}

impl BudgetDimension {
    pub fn as_str(self) -> &'static str {
        match self {
            BudgetDimension::DecodedPaths => "decodedPaths",
            BudgetDimension::Candidates => "candidates",
            BudgetDimension::HermitcrabSteps => "hermitcrabSteps",
        }
    }
}

/// Why a case that started could not finish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IncompleteReason {
    /// A deterministic magnitude budget tripped. Same model, same word, same caps gives the same
    /// outcome on every machine and every run.
    #[serde(rename_all = "camelCase")]
    LogicalBudget {
        dimension: BudgetDimension,
        value: u64,
        limit: u64,
    },
    /// An outer safety net stopped the case. Machine-dependent by nature; poisons reproducibility.
    #[serde(rename_all = "camelCase")]
    WallClockTimeout { elapsed_us: u64, limit_us: u64 },
}

impl IncompleteReason {
    /// Whether this stop is decided deterministically.
    pub fn is_reproducible(&self) -> bool {
        matches!(self, IncompleteReason::LogicalBudget { .. })
    }
}

/// Why a case never began.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum NotAttemptedReason {
    /// A cumulative batch budget was exhausted by earlier cases. Those earlier cases stay complete
    /// and valid; only this one never ran.
    BatchBudgetExhausted,
    /// Import, compile, or setup failed, so no case could run.
    AssessmentSetupFailed,
    /// The caller marked the case `invalid` and did not ask for it to be executed anyway.
    CaseStatusInvalid,
}

/// The outcome of one case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseOutcome {
    /// The entire confirmed analysis set. May be empty, which is a positive claim.
    Complete(AnalysisSet),
    /// Analysis began and a limit stopped it. Any partial candidates are diagnostic evidence held
    /// elsewhere and never presented as the authoritative set.
    Incomplete(IncompleteReason),
    /// The case never began.
    NotAttempted(NotAttemptedReason),
}

impl CaseOutcome {
    /// The outcome kind, which is what `outcomeDigest` and delta categorization key on.
    pub fn kind(&self) -> &'static str {
        match self {
            CaseOutcome::Complete(_) => "complete",
            CaseOutcome::Incomplete(_) => "incomplete",
            CaseOutcome::NotAttempted(_) => "not_attempted",
        }
    }

    /// The authoritative analysis set, present only for a complete case.
    pub fn analyses(&self) -> Option<&AnalysisSet> {
        match self {
            CaseOutcome::Complete(set) => Some(set),
            _ => None,
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self, CaseOutcome::Complete(_))
    }

    /// Whether this outcome was decided deterministically.
    pub fn is_reproducible(&self) -> bool {
        match self {
            CaseOutcome::Incomplete(reason) => reason.is_reproducible(),
            _ => true,
        }
    }
}

/// The report-level verdict on execution, not on the grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentStatus {
    /// Every runnable case completed. Cases skipped by policy do not make a run partial.
    Complete,
    /// At least one case completed and another runnable case did not, because execution or batch
    /// limits intervened.
    Partial,
    /// Import, compile, or setup prevented all runnable cases from completing.
    Failed,
}

/// Derive the report status from the outcomes actually recorded.
///
/// A case the caller marked `invalid` and did not ask to run is skipped by policy, not by
/// containment, so it cannot drag an otherwise clean run down to `partial`.
pub fn derive_status(outcomes: &[CaseOutcome]) -> AssessmentStatus {
    let mut any_complete = false;
    let mut any_runnable_unfinished = false;
    let mut any_setup_failure = false;

    for outcome in outcomes {
        match outcome {
            CaseOutcome::Complete(_) => any_complete = true,
            CaseOutcome::Incomplete(_) => any_runnable_unfinished = true,
            CaseOutcome::NotAttempted(NotAttemptedReason::CaseStatusInvalid) => {}
            CaseOutcome::NotAttempted(NotAttemptedReason::AssessmentSetupFailed) => {
                any_setup_failure = true;
            }
            CaseOutcome::NotAttempted(NotAttemptedReason::BatchBudgetExhausted) => {
                any_runnable_unfinished = true;
            }
        }
    }

    if any_complete && !any_runnable_unfinished {
        AssessmentStatus::Complete
    } else if any_complete {
        AssessmentStatus::Partial
    } else if any_setup_failure || any_runnable_unfinished {
        AssessmentStatus::Failed
    } else {
        // Nothing ran and nothing failed: an empty suite, or one entirely skipped by policy.
        AssessmentStatus::Complete
    }
}

#[cfg(test)]
mod tests {
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
}
