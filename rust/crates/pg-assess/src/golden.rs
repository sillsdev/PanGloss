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
        // Structured identities, never counts alone: a caller has to be able to see WHICH
        // required analysis went missing, not merely that one did.
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
    // Adjudication is checked before completeness on purpose: a case nobody has ruled on is
    // `not_adjudicated` whether or not it ran, and reporting it as `not_evaluable` would suggest
    // that finishing the run would have produced a verdict.
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
        // An incomplete case never satisfies an expectation — not even an empty closed-world one,
        // which is the case most tempting to wave through.
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

    // Under an open world, an undeclared analysis is recorded and tolerated: the caller said what
    // they cared about and did not claim the list was exhaustive. Under a closed world they did.
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
mod tests {
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
        // The gate for this unit, and the FieldWorks conflation stated as a test: a budget trip must
        // not read as "the grammar analyzes this no way at all".
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
        // Finishing the run would not have produced a verdict, so saying `not_evaluable` would
        // point the caller at the wrong problem.
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
}
