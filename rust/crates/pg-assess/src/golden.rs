//! `pangloss.golden-set-diff/v1` — evaluating a suite's expectations against one assessment.
//!
//! This is where PanGloss comes closest to saying a grammar is wrong, so the boundary is drawn
//! sharply: it reports whether observed analyses **agree with what the caller declared**, never
//! whether the caller was right. An expectation the caller has not adjudicated produces
//! `not_adjudicated`, not a pass and not a failure. PanGloss never writes back to a suite, never
//! moves an expectation between statuses, and never proposes one.
//!
//! ## Three refusals that keep the evidence honest
//!
//! **An incomplete case satisfies nothing.** The temptation is to treat an incomplete case's empty
//! set as "no analyses observed" and let a closed-world expectation pass. That would turn a budget
//! trip into a confident claim of ungrammaticality — precisely the conflation FieldWorks makes when
//! `XAmpleParser` records a `ReachedMaxAnalyses` truncation as an ordinary analysis count
//! (`XAmpleParser.cs:183-228`). Only a `complete` outcome is evaluable.
//!
//! **An old run is never re-judged against revised policy.** Evaluation requires the exact suite
//! ID, revision, semantic digest, and identity profile the assessment recorded. Otherwise a caller
//! edits an expectation, re-runs `golden-diff` against last week's report, and gets a verdict about
//! a run that never faced those expectations.
//!
//! **Every aggregate carries its denominator.** "12 disagreements" is unreadable without knowing
//! whether 12 of 15 or 12 of 40,000 cases were even evaluable. There are no rates and no scores
//! here — only counts and the populations they came from.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::digest::identity_digest;
use crate::identity::AnalysisIdentity;
use crate::outcome::CaseOutcome;
use crate::report::AssessmentReport;
use crate::set::AnalysisSet;
use crate::suite::{Expectation, ValidatedSuite};

pub const GOLDEN_SCHEMA: &str = "pangloss.golden-set-diff";
pub const GOLDEN_SCHEMA_VERSION: u32 = 1;

/// Whether one case's expectation could be evaluated, and if so how it came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Every `required` identity was observed, no `forbidden` identity was, and under a closed
    /// world nothing outside `required ∪ allowed` appeared.
    Agrees,
    /// At least one of those failed. Which ones are enumerated on the case; this word alone is not
    /// the evidence.
    Disagrees,
    /// The case did not complete, so no authoritative set exists to judge.
    NotEvaluable,
    /// No adjudicated expectation exists for this case.
    NotAdjudicated,
}

/// Why an expectation could not be evaluated. Typed, for the same reason `not_comparable` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotEvaluableReason {
    Incomplete,
    NotAttempted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotAdjudicatedReason {
    /// The suite declares no expectation for this case at all.
    NoExpectation,
    Unresolved,
    OutOfScope,
    Invalid,
}

/// One case's evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenCase {
    pub case_id: String,
    pub input: String,
    pub verdict: Verdict,
    pub not_evaluable_reason: Option<NotEvaluableReason>,
    pub not_adjudicated_reason: Option<NotAdjudicatedReason>,
    /// Required identities that were observed.
    pub matching_required: Vec<AnalysisIdentity>,
    /// Required identities that were not. The grammar stopped producing something the caller
    /// declared it should — stated as that, not as a regression.
    pub missing_required: Vec<AnalysisIdentity>,
    /// Allowed identities that happened to be observed. Neither demanded nor banned.
    pub matching_allowed: Vec<AnalysisIdentity>,
    /// Forbidden identities that were observed.
    pub observed_forbidden: Vec<AnalysisIdentity>,
    /// Observed identities in no declared set. Only meaningful under a closed world; under an open
    /// world they are recorded and do not affect the verdict.
    pub unexpected: Vec<AnalysisIdentity>,
    pub closed_world: bool,
}

/// The finished evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenSetDiff {
    pub report_id: String,
    pub suite_id: String,
    pub suite_revision: String,
    pub cases: Vec<GoldenCase>,
}

/// Why an evaluation was refused outright. A mismatch here is structural: nothing partial is
/// emitted, because a partial answer about the wrong pairing is worse than none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoldenError {
    SuiteMismatch {
        field: &'static str,
        report: String,
        suite: String,
    },
}

impl std::fmt::Display for GoldenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GoldenError::SuiteMismatch {
                field,
                report,
                suite,
            } => write!(
                f,
                "the assessment was produced against a different suite: {field} is {report} in the \
                 report and {suite} in the suite. Re-run `assess` rather than judging an old run \
                 against revised expectations"
            ),
        }
    }
}

impl std::error::Error for GoldenError {}

impl GoldenSetDiff {
    /// Counts with the populations they came from. No rate, no score, no verdict on the grammar.
    pub fn to_value(&self) -> Value {
        let mut total = 0u64;
        let (mut agrees, mut disagrees, mut not_evaluable, mut not_adjudicated) = (0, 0, 0, 0);
        let mut by_not_evaluable: BTreeMap<&str, u64> = BTreeMap::new();
        let mut by_not_adjudicated: BTreeMap<&str, u64> = BTreeMap::new();

        for case in &self.cases {
            total += 1;
            match case.verdict {
                Verdict::Agrees => agrees += 1,
                Verdict::Disagrees => disagrees += 1,
                Verdict::NotEvaluable => {
                    not_evaluable += 1;
                    let key = match case.not_evaluable_reason {
                        Some(NotEvaluableReason::Incomplete) => "incomplete",
                        _ => "not_attempted",
                    };
                    *by_not_evaluable.entry(key).or_default() += 1;
                }
                Verdict::NotAdjudicated => {
                    not_adjudicated += 1;
                    let key = match case.not_adjudicated_reason {
                        Some(NotAdjudicatedReason::Unresolved) => "unresolved",
                        Some(NotAdjudicatedReason::OutOfScope) => "out_of_scope",
                        Some(NotAdjudicatedReason::Invalid) => "invalid",
                        _ => "no_expectation",
                    };
                    *by_not_adjudicated.entry(key).or_default() += 1;
                }
            }
        }

        json!({
            "schema": GOLDEN_SCHEMA,
            "schemaVersion": GOLDEN_SCHEMA_VERSION,
            "reportId": self.report_id,
            "suite": { "suiteId": self.suite_id, "suiteRevision": self.suite_revision },
            "summary": {
                "totalCases": total,
                "adjudicatedAndEvaluable": agrees + disagrees,
                "agrees": agrees,
                "disagrees": disagrees,
                "notEvaluable": not_evaluable,
                "notEvaluableByReason": by_not_evaluable,
                "notAdjudicated": not_adjudicated,
                "notAdjudicatedByReason": by_not_adjudicated,
            },
            "cases": self.cases.iter().map(case_value).collect::<Vec<_>>(),
        })
    }
}

fn identities_value(identities: &[AnalysisIdentity]) -> Value {
    Value::Array(
        identities
            .iter()
            .map(|i| {
                json!({
                    "identity": i.to_canonical_value(),
                    "identityDigest": identity_digest(i),
                })
            })
            .collect(),
    )
}

fn case_value(case: &GoldenCase) -> Value {
    json!({
        "caseId": case.case_id,
        "input": case.input,
        "verdict": serde_json::to_value(case.verdict).expect("verdict is a unit enum"),
        "notEvaluableReason": case.not_evaluable_reason,
        "notAdjudicatedReason": case.not_adjudicated_reason,
        "closedWorld": case.closed_world,
        // Structured identities, not counts, so a caller can see WHICH analysis is missing.
        "matchingRequired": identities_value(&case.matching_required),
        "missingRequired": identities_value(&case.missing_required),
        "matchingAllowed": identities_value(&case.matching_allowed),
        "observedForbidden": identities_value(&case.observed_forbidden),
        "unexpected": identities_value(&case.unexpected),
    })
}

/// Evaluate a suite's expectations against an assessment of that exact suite.
///
/// The suite is read-only here in the strongest sense: this function takes `&ValidatedSuite` and
/// returns a new artifact. There is no code path that writes one.
pub fn golden_diff(
    report: &AssessmentReport,
    suite: &ValidatedSuite,
) -> Result<GoldenSetDiff, GoldenError> {
    let recorded = &report.draft().suite;
    let declared = suite.suite();
    let check = |field: &'static str, report: &str, suite: &str| {
        (report != suite).then(|| GoldenError::SuiteMismatch {
            field,
            report: report.to_string(),
            suite: suite.to_string(),
        })
    };
    if let Some(mismatch) = check("suiteId", &recorded.suite_id, &declared.suite_id)
        .or_else(|| {
            check(
                "suiteRevision",
                &recorded.suite_revision,
                &declared.suite_revision,
            )
        })
        .or_else(|| {
            check(
                "suiteSemanticDigest",
                &recorded.semantic_digest,
                suite.semantic_digest(),
            )
        })
        .or_else(|| {
            check(
                "analysisIdentityProfile",
                &recorded.analysis_identity_profile,
                &declared.analysis_identity_profile,
            )
        })
    {
        return Err(mismatch);
    }

    let expectations: BTreeMap<&str, &Expectation> = suite
        .cases()
        .iter()
        .filter_map(|case| {
            case.expectation
                .as_ref()
                .map(|e| (case.case_id.as_str(), e))
        })
        .collect();

    let cases = report
        .cases()
        .iter()
        .map(|case| {
            let expectation = expectations.get(case.case_id.as_str()).copied();
            evaluate(&case.case_id, &case.input, &case.outcome, expectation)
        })
        .collect();

    Ok(GoldenSetDiff {
        report_id: report.report_id().to_string(),
        suite_id: declared.suite_id.clone(),
        suite_revision: declared.suite_revision.clone(),
        cases,
    })
}

fn blank(case_id: &str, input: &str, verdict: Verdict, closed_world: bool) -> GoldenCase {
    GoldenCase {
        case_id: case_id.to_string(),
        input: input.to_string(),
        verdict,
        not_evaluable_reason: None,
        not_adjudicated_reason: None,
        matching_required: Vec::new(),
        missing_required: Vec::new(),
        matching_allowed: Vec::new(),
        observed_forbidden: Vec::new(),
        unexpected: Vec::new(),
        closed_world,
    }
}

fn evaluate(
    case_id: &str,
    input: &str,
    outcome: &CaseOutcome,
    expectation: Option<&Expectation>,
) -> GoldenCase {
    // Checked before completeness: an un-ruled-on case is `not_adjudicated` regardless of whether it ran.
    let Some(expectation) = expectation else {
        let mut case = blank(case_id, input, Verdict::NotAdjudicated, false);
        case.not_adjudicated_reason = Some(NotAdjudicatedReason::NoExpectation);
        return case;
    };
    if !expectation.status.is_adjudicated() {
        let mut case = blank(
            case_id,
            input,
            Verdict::NotAdjudicated,
            expectation.closed_world,
        );
        case.not_adjudicated_reason = Some(match expectation.status {
            crate::suite::ExpectationStatus::Unresolved => NotAdjudicatedReason::Unresolved,
            crate::suite::ExpectationStatus::OutOfScope => NotAdjudicatedReason::OutOfScope,
            _ => NotAdjudicatedReason::Invalid,
        });
        return case;
    }

    let Some(observed) = outcome.analyses() else {
        // An incomplete case never satisfies an expectation, even an empty closed-world one.
        let mut case = blank(
            case_id,
            input,
            Verdict::NotEvaluable,
            expectation.closed_world,
        );
        case.not_evaluable_reason = Some(match outcome {
            CaseOutcome::Incomplete(_) => NotEvaluableReason::Incomplete,
            _ => NotEvaluableReason::NotAttempted,
        });
        return case;
    };

    judge(case_id, input, observed, expectation)
}

fn judge(
    case_id: &str,
    input: &str,
    observed: &AnalysisSet,
    expectation: &Expectation,
) -> GoldenCase {
    let mut case = blank(case_id, input, Verdict::Agrees, expectation.closed_world);

    let observed_digests: BTreeMap<String, &AnalysisIdentity> = observed
        .entries()
        .iter()
        .map(|e| (e.identity_digest.clone(), &e.identity))
        .collect();
    let seen = |identity: &AnalysisIdentity| {
        let digest = identity_digest(identity);
        // Confirmed against the structured value, never on the digest alone.
        observed_digests
            .get(&digest)
            .is_some_and(|found| *found == identity)
    };

    for required in &expectation.required {
        if seen(required) {
            case.matching_required.push(required.clone());
        } else {
            case.missing_required.push(required.clone());
        }
    }
    for allowed in &expectation.allowed {
        if seen(allowed) {
            case.matching_allowed.push(allowed.clone());
        }
    }
    for forbidden in &expectation.forbidden {
        if seen(forbidden) {
            case.observed_forbidden.push(forbidden.clone());
        }
    }

    let declared: Vec<String> = expectation
        .required
        .iter()
        .chain(&expectation.allowed)
        .map(identity_digest)
        .collect();
    for entry in observed.entries() {
        if !declared.contains(&entry.identity_digest)
            && !expectation
                .forbidden
                .iter()
                .any(|f| identity_digest(f) == entry.identity_digest)
        {
            case.unexpected.push(entry.identity.clone());
        }
    }

    // Open world: an undeclared analysis is recorded and tolerated. Closed world: it disagrees.
    let unexpected_disagrees = expectation.closed_world && !case.unexpected.is_empty();
    if !case.missing_required.is_empty()
        || !case.observed_forbidden.is_empty()
        || unexpected_disagrees
    {
        case.verdict = Verdict::Disagrees;
    }
    case
}

#[cfg(test)]
mod tests;
