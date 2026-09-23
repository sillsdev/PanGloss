//! `pangloss.grammar-delta/v1` — what changed between two assessment reports, as exact evidence.
//!
//! The operative constraint is stated once and holds everywhere below: **an addition is not an
//! improvement and a removal is not a regression.** This module names what moved and refuses to say
//! whether that is good. The caller adjudicates; PanGloss reports.
//!
//! ## Why identities and not counts
//!
//! FieldWorks' shipped parser report stores a per-word analysis *count* and diffs two reports by
//! subtracting them (`ParserReport.cs:418-442`). A word that produced two analyses before an edit
//! and two entirely different analyses after diffs to zero — a silent "nothing changed" about a
//! word whose every analysis was replaced. That is not a bug in their arithmetic; it is what
//! storing counts instead of sets costs. `compare` joins deduplicated identity sets, so the same
//! case comes back `mixed` with both sides enumerated.
//!
//! ## Why `caseId` and not the surface form
//!
//! The same report keys its per-word map by the vernacular string, so two questions about one form
//! cannot coexist and no case can supersede another. Here the join key is the caller's opaque
//! `caseId`, followed through declared `supersedes` links, and two cases may share an input.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::digest::identity_digest;
use crate::identity::AnalysisIdentity;
use crate::jcs::JcsError;
use crate::outcome::CaseOutcome;
use crate::report::{AssessmentReport, CaseRecord, Diagnostic};
use crate::set::AnalysisSet;

pub const DELTA_SCHEMA: &str = "pangloss.grammar-delta";
pub const DELTA_SCHEMA_VERSION: u32 = 1;

/// What happened to one case between baseline and candidate.
///
/// Membership in the *changed* subset is what a CI consumer counts and what `investigate` is
/// offered for, so it drives real downstream workload and is not cosmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaCategory {
    /// Identical identity sets and identical annotations; duplicate counts and context may still differ, but those are flags, not changes.
    Unchanged,
    AddedOnly,
    RemovedOnly,
    /// Both added and removed: the category FieldWorks' count subtraction cannot express when the two happen to balance.
    Mixed,
    /// Every identity was retained but an annotation moved, in practice `guessed` flipping: the root stopped being found in the lexicon and the parser fabricated one.
    AnnotationChanged,
    /// The outcome kind itself moved, e.g. `complete → incomplete`.
    CompletenessChanged,
    /// Present in baseline only, with no `supersedes` link accounting for it.
    BaselineOnly,
    CandidateOnly,
    /// No comparison is defensible. Always carries a typed reason.
    NotComparable,
}

impl DeltaCategory {
    /// Whether this case counts as changed. `unchanged` and the two one-sided categories do not:
    /// a case that exists on one side only is inventory movement the caller already knows about,
    /// and forcing investigation on it would generate noise on every suite edit.
    pub fn is_changed(self) -> bool {
        matches!(
            self,
            DeltaCategory::AddedOnly
                | DeltaCategory::RemovedOnly
                | DeltaCategory::Mixed
                | DeltaCategory::AnnotationChanged
                | DeltaCategory::CompletenessChanged
        )
    }
}

/// Why a case could not be compared. Typed because `not_comparable` with a prose cause is the least
/// actionable output this system can produce — a consumer cannot branch on prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotComparableReason {
    /// The two sides asked different questions under one case ID: the input text differs.
    CaseDefinitionChanged,
    /// The reports declare different identity profiles, so their encodings mean different things.
    IdentityProfileChanged,
    /// Neither side finished, so neither has an authoritative set to compare.
    BothIncomplete,
    /// Neither side ran.
    BothNotAttempted,
    /// One side finished and the other did not, reported here rather than as `completeness_changed` only when neither side is complete (see `categorize`).
    IncomparableOutcomes,
    /// Two unequal identities share a digest within one report. An integrity error, never a match.
    KeyCollision,
}

/// One case's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseDelta {
    pub case_id: String,
    /// The candidate's case ID when a `supersedes` link was followed, so a renumbering is visible
    /// rather than silently absorbed.
    pub candidate_case_id: Option<String>,
    pub input: String,
    pub category: DeltaCategory,
    pub reason: Option<NotComparableReason>,
    pub baseline_outcome: Option<String>,
    pub candidate_outcome: Option<String>,
    pub added: Vec<AnalysisIdentity>,
    pub removed: Vec<AnalysisIdentity>,
    pub retained: Vec<AnalysisIdentity>,
    /// Identities kept on both sides whose `guessed` moved, with the direction.
    pub annotation_changes: Vec<AnnotationChange>,
    /// Identities kept on both sides found a different number of times. Health evidence about the
    /// engine's work, not about the grammar, so it never makes a case changed.
    pub duplicate_count_changes: Vec<DuplicateCountChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationChange {
    pub identity: AnalysisIdentity,
    pub baseline_guessed: bool,
    pub candidate_guessed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateCountChange {
    pub identity: AnalysisIdentity,
    pub baseline_count: u32,
    pub candidate_count: u32,
}

/// A difference in how the two runs were produced. Reported as evidence; never a gate.
///
/// Comparison proceeds regardless. Refusing to compare because the compiler version moved would
/// make the tool useless precisely when it is most needed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextDifference {
    pub field: String,
    pub baseline: Value,
    pub candidate: Value,
}

/// The finished comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarDelta {
    pub baseline_report_id: String,
    pub candidate_report_id: String,
    pub baseline_outcome_digest: String,
    pub candidate_outcome_digest: String,
    /// True iff both outcome digests match — the cheap "did the grammar behave the same" answer,
    /// available without reading a single case.
    pub outcome_digests_agree: bool,
    pub context_differences: Vec<ContextDifference>,
    pub cases: Vec<CaseDelta>,
}

impl GrammarDelta {
    pub fn changed_cases(&self) -> impl Iterator<Item = &CaseDelta> {
        self.cases.iter().filter(|c| c.category.is_changed())
    }

    pub fn to_value(&self) -> Value {
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        for case in &self.cases {
            let key = serde_json::to_value(case.category)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            *counts.entry(key).or_default() += 1;
        }

        json!({
            "schema": DELTA_SCHEMA,
            "schemaVersion": DELTA_SCHEMA_VERSION,
            "baseline": { "reportId": self.baseline_report_id, "outcomeDigest": self.baseline_outcome_digest },
            "candidate": { "reportId": self.candidate_report_id, "outcomeDigest": self.candidate_outcome_digest },
            "outcomeDigestsAgree": self.outcome_digests_agree,
            "contextDifferences": self.context_differences,
            // Denominators, never a rate, so a consumer can compute whatever it wants without the artifact implying a verdict.
            "summary": { "totalCases": self.cases.len(), "byCategory": counts,
                         "changedCases": self.changed_cases().count() },
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

fn case_value(case: &CaseDelta) -> Value {
    let mut obj = Map::new();
    obj.insert("caseId".into(), json!(case.case_id));
    if let Some(candidate_case_id) = &case.candidate_case_id {
        obj.insert("candidateCaseId".into(), json!(candidate_case_id));
    }
    obj.insert("input".into(), json!(case.input));
    obj.insert(
        "category".into(),
        serde_json::to_value(case.category).expect("category is a unit enum"),
    );
    if let Some(reason) = case.reason {
        obj.insert(
            "notComparableReason".into(),
            serde_json::to_value(reason).expect("reason is a unit enum"),
        );
    }
    obj.insert("baselineOutcome".into(), json!(case.baseline_outcome));
    obj.insert("candidateOutcome".into(), json!(case.candidate_outcome));
    obj.insert("added".into(), identities_value(&case.added));
    obj.insert("removed".into(), identities_value(&case.removed));
    obj.insert("retained".into(), identities_value(&case.retained));
    obj.insert(
        "annotationChanges".into(),
        Value::Array(
            case.annotation_changes
                .iter()
                .map(|c| {
                    json!({
                        "identity": c.identity.to_canonical_value(),
                        "baselineGuessed": c.baseline_guessed,
                        "candidateGuessed": c.candidate_guessed,
                    })
                })
                .collect(),
        ),
    );
    obj.insert(
        "duplicateCountChanges".into(),
        Value::Array(
            case.duplicate_count_changes
                .iter()
                .map(|c| {
                    json!({
                        "identity": c.identity.to_canonical_value(),
                        "baselineCount": c.baseline_count,
                        "candidateCount": c.candidate_count,
                    })
                })
                .collect(),
        ),
    );
    Value::Object(obj)
}

/// Compare two assessment reports.
///
/// Never fails on a grammar difference: a deleted morpheme, a renamed key, a case that only one
/// side has, and a compiler upgrade are all ordinary evidence. The only refusals are structural —
/// incompatible identity profiles, or a case whose two sides asked different questions.
pub fn compare(
    baseline: &AssessmentReport,
    candidate: &AssessmentReport,
) -> Result<GrammarDelta, JcsError> {
    let context_differences = context_differences(baseline, candidate);

    let profiles_agree = baseline.draft().suite.analysis_identity_profile
        == candidate.draft().suite.analysis_identity_profile;

    // Candidate cases by ID, and the supersession map: a candidate declaring `supersedes: [x]` answers for baseline case `x`, or a renumbering caller gets phantom baseline_only/candidate_only pairs permanently.
    let candidate_by_id: BTreeMap<&str, &CaseRecord> = candidate
        .cases()
        .iter()
        .map(|c| (c.case_id.as_str(), c))
        .collect();
    let declared_lineage = candidate.supersedes();
    let supersedes: BTreeMap<&str, &CaseRecord> = declared_lineage
        .iter()
        .filter_map(|(superseded, case_id)| {
            candidate_by_id
                .get(case_id.as_str())
                .map(|case| (superseded.as_str(), *case))
        })
        .collect();

    let mut cases = Vec::new();
    let mut matched_candidates: BTreeSet<&str> = BTreeSet::new();

    // Baseline order first: a reader scanning the delta sees their suite's own order.
    for base in baseline.cases() {
        let matched = candidate_by_id
            .get(base.case_id.as_str())
            .copied()
            .or_else(|| supersedes.get(base.case_id.as_str()).copied());

        match matched {
            Some(cand) => {
                matched_candidates.insert(cand.case_id.as_str());
                cases.push(categorize(base, cand, profiles_agree));
            }
            None => cases.push(one_sided(base, DeltaCategory::BaselineOnly)),
        }
    }
    // Then candidate-only cases in candidate order.
    for cand in candidate.cases() {
        if !matched_candidates.contains(cand.case_id.as_str()) {
            cases.push(one_sided(cand, DeltaCategory::CandidateOnly));
        }
    }

    Ok(GrammarDelta {
        baseline_report_id: baseline.report_id().to_string(),
        candidate_report_id: candidate.report_id().to_string(),
        baseline_outcome_digest: baseline.outcome_digest().to_string(),
        candidate_outcome_digest: candidate.outcome_digest().to_string(),
        outcome_digests_agree: baseline.outcome_digest() == candidate.outcome_digest(),
        context_differences,
        cases,
    })
}

fn one_sided(case: &CaseRecord, category: DeltaCategory) -> CaseDelta {
    let (baseline_outcome, candidate_outcome) = match category {
        DeltaCategory::BaselineOnly => (Some(case.outcome.kind().to_string()), None),
        _ => (None, Some(case.outcome.kind().to_string())),
    };
    CaseDelta {
        case_id: case.case_id.clone(),
        candidate_case_id: None,
        input: case.input.clone(),
        category,
        reason: None,
        baseline_outcome,
        candidate_outcome,
        added: Vec::new(),
        removed: Vec::new(),
        retained: Vec::new(),
        annotation_changes: Vec::new(),
        duplicate_count_changes: Vec::new(),
    }
}

fn not_comparable(base: &CaseRecord, cand: &CaseRecord, reason: NotComparableReason) -> CaseDelta {
    CaseDelta {
        case_id: base.case_id.clone(),
        candidate_case_id: (base.case_id != cand.case_id).then(|| cand.case_id.clone()),
        input: base.input.clone(),
        category: DeltaCategory::NotComparable,
        reason: Some(reason),
        baseline_outcome: Some(base.outcome.kind().to_string()),
        candidate_outcome: Some(cand.outcome.kind().to_string()),
        added: Vec::new(),
        removed: Vec::new(),
        retained: Vec::new(),
        annotation_changes: Vec::new(),
        duplicate_count_changes: Vec::new(),
    }
}

fn categorize(base: &CaseRecord, cand: &CaseRecord, profiles_agree: bool) -> CaseDelta {
    if !profiles_agree {
        return not_comparable(base, cand, NotComparableReason::IdentityProfileChanged);
    }
    if base.input != cand.input {
        // One case ID, two questions. Comparing the answers would be comparing different things.
        return not_comparable(base, cand, NotComparableReason::CaseDefinitionChanged);
    }

    let (base_set, cand_set) = (base.outcome.analyses(), cand.outcome.analyses());
    let (base_set, cand_set) = match (base_set, cand_set) {
        (Some(b), Some(c)) => (b, c),
        // Exactly one side finished; reporting an empty set as "everything removed" would be the collapse the atomic-outcome contract exists to prevent.
        (Some(_), None) | (None, Some(_)) => {
            let mut delta = not_comparable(base, cand, NotComparableReason::IncomparableOutcomes);
            delta.category = DeltaCategory::CompletenessChanged;
            delta.reason = None;
            return delta;
        }
        (None, None) => {
            let reason = match (&base.outcome, &cand.outcome) {
                (CaseOutcome::Incomplete(_), CaseOutcome::Incomplete(_)) => {
                    NotComparableReason::BothIncomplete
                }
                (CaseOutcome::NotAttempted(_), CaseOutcome::NotAttempted(_)) => {
                    NotComparableReason::BothNotAttempted
                }
                _ => NotComparableReason::IncomparableOutcomes,
            };
            return not_comparable(base, cand, reason);
        }
    };

    let mut delta = diff_sets(base_set, cand_set);
    delta.case_id = base.case_id.clone();
    delta.candidate_case_id = (base.case_id != cand.case_id).then(|| cand.case_id.clone());
    delta.input = base.input.clone();
    delta.baseline_outcome = Some(base.outcome.kind().to_string());
    delta.candidate_outcome = Some(cand.outcome.kind().to_string());
    delta
}

/// Join two deduplicated sets on identity digest; a key present on one side and absent on the other is `added`/`removed`, never `not_comparable`, since a deleted affix is the most ordinary edit there is.
fn diff_sets(base: &AnalysisSet, cand: &AnalysisSet) -> CaseDelta {
    let base_by_digest: BTreeMap<&str, _> = base
        .entries()
        .iter()
        .map(|e| (e.identity_digest.as_str(), e))
        .collect();
    let cand_by_digest: BTreeMap<&str, _> = cand
        .entries()
        .iter()
        .map(|e| (e.identity_digest.as_str(), e))
        .collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut retained = Vec::new();
    let mut annotation_changes = Vec::new();
    let mut duplicate_count_changes = Vec::new();
    let mut collision = false;

    for (digest, base_entry) in &base_by_digest {
        match cand_by_digest.get(digest) {
            None => removed.push(base_entry.identity.clone()),
            Some(cand_entry) => {
                if cand_entry.identity != base_entry.identity {
                    // Two unequal identities sharing a digest. An integrity error, not a match.
                    collision = true;
                    continue;
                }
                retained.push(base_entry.identity.clone());
                if base_entry.guessed != cand_entry.guessed {
                    annotation_changes.push(AnnotationChange {
                        identity: base_entry.identity.clone(),
                        baseline_guessed: base_entry.guessed,
                        candidate_guessed: cand_entry.guessed,
                    });
                }
                if base_entry.duplicate_count != cand_entry.duplicate_count {
                    duplicate_count_changes.push(DuplicateCountChange {
                        identity: base_entry.identity.clone(),
                        baseline_count: base_entry.duplicate_count,
                        candidate_count: cand_entry.duplicate_count,
                    });
                }
            }
        }
    }
    for (digest, cand_entry) in &cand_by_digest {
        if !base_by_digest.contains_key(digest) {
            added.push(cand_entry.identity.clone());
        }
    }

    let category = if collision {
        DeltaCategory::NotComparable
    } else {
        match (added.is_empty(), removed.is_empty()) {
            (false, false) => DeltaCategory::Mixed,
            (false, true) => DeltaCategory::AddedOnly,
            (true, false) => DeltaCategory::RemovedOnly,
            // Same identities. An annotation move is still a change; a duplicate-count move is not.
            (true, true) if !annotation_changes.is_empty() => DeltaCategory::AnnotationChanged,
            (true, true) => DeltaCategory::Unchanged,
        }
    };

    CaseDelta {
        case_id: String::new(),
        candidate_case_id: None,
        input: String::new(),
        category,
        reason: collision.then_some(NotComparableReason::KeyCollision),
        baseline_outcome: None,
        candidate_outcome: None,
        added,
        removed,
        retained,
        annotation_changes,
        duplicate_count_changes,
    }
}

fn context_differences(
    baseline: &AssessmentReport,
    candidate: &AssessmentReport,
) -> Vec<ContextDifference> {
    let (b, c) = (baseline.draft(), candidate.draft());
    let mut out = Vec::new();
    let mut note = |field: &str, left: Value, right: Value| {
        if left != right {
            out.push(ContextDifference {
                field: field.to_string(),
                baseline: left,
                candidate: right,
            });
        }
    };

    note(
        "provenance.sourceSha256",
        json!(b.provenance.source_sha256),
        json!(c.provenance.source_sha256),
    );
    note(
        "provenance.modelFingerprint",
        json!(b.provenance.model_fingerprint),
        json!(c.provenance.model_fingerprint),
    );
    note(
        "provenance.importerVersion",
        json!(b.provenance.importer_version),
        json!(c.provenance.importer_version),
    );
    note(
        "provenance.compilerVersion",
        json!(b.provenance.compiler_version),
        json!(c.provenance.compiler_version),
    );
    note(
        "execution.pipeline",
        json!(b.execution.pipeline),
        json!(c.execution.pipeline),
    );
    note(
        "execution.budgets",
        json!(b.execution.budgets),
        json!(c.execution.budgets),
    );
    note(
        "suite.suiteRevision",
        json!(b.suite.suite_revision),
        json!(c.suite.suite_revision),
    );
    note(
        "suite.analysisIdentityProfile",
        json!(b.suite.analysis_identity_profile),
        json!(c.suite.analysis_identity_profile),
    );
    note(
        "reproducible",
        json!(baseline.is_reproducible()),
        json!(candidate.is_reproducible()),
    );
    // By code and count, never by prose: rewording an importer warning is not a change in the grammar's context.
    note(
        "diagnostics",
        diagnostic_code_counts(&b.diagnostics),
        diagnostic_code_counts(&c.diagnostics),
    );

    out
}

fn diagnostic_code_counts(diagnostics: &[Diagnostic]) -> Value {
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    for diagnostic in diagnostics {
        *counts.entry(diagnostic.code.as_str()).or_default() += 1;
    }
    json!(counts)
}

#[cfg(test)]
mod tests;
