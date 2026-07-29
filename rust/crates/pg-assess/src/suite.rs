//! `pangloss.assessment-suite/v1` — the caller's questions, and the only join key between runs.
//!
//! A suite is caller-owned in a strong sense. PanGloss validates it, executes it, and records what
//! it says; it never authors a case, never edits an expectation, and never moves an expectation
//! between statuses. Everything opaque here — `suiteId`, `suiteRevision`, `caseId`,
//! `sourceReferences`, `metadata`, `extensions` — stays exactly as supplied.
//!
//! ## Why `caseId` matters more than it looks
//!
//! `compare` matches cases by exact `caseId` and nothing else. Two cases may carry the same surface
//! form and remain distinct, because a case is a question the caller is asking, not a word: "does
//! *this occurrence* still analyze the way we adjudicated?" Without stable case identity there is no
//! join between two runs at all, which is why the suite is a prerequisite for comparison rather than
//! a convenience over a word list.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::digest::digest_projection;
use crate::identity::{AnalysisIdentity, IDENTITY_PROFILE};

pub const SUITE_SCHEMA: &str = "pangloss.assessment-suite";
pub const SUITE_SCHEMA_VERSION: u32 = 1;

/// The projection a suite's semantic digest is taken under.
pub const SUITE_PROJECTION: &str = "pangloss.assessment-suite/v1";

/// Caps on caller-supplied opaque data. Generous enough that no legitimate suite meets them, and
/// present so an accidental multi-megabyte blob is refused with a typed error rather than silently
/// carried into every report that references the suite.
pub const MAX_SOURCE_REFERENCE_BYTES: usize = 64 * 1024;
pub const MAX_SOURCE_REFERENCES_PER_CASE: usize = 64;

/// What the caller has decided about a case, recorded and never transitioned by PanGloss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectationStatus {
    /// The executable policy is authoritative for this suite revision.
    Adjudicated,
    /// Disagreement exists; do not evaluate pass/fail policy.
    Unresolved,
    /// Retained for reporting, intentionally excluded from evaluation.
    OutOfScope,
    /// The caller knows the case is unusable; do not execute unless explicitly requested.
    Invalid,
}

impl ExpectationStatus {
    /// Only an adjudicated expectation is evaluated for agreement.
    pub fn is_adjudicated(self) -> bool {
        matches!(self, ExpectationStatus::Adjudicated)
    }
}

/// A caller's executable policy for one case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Expectation {
    pub status: ExpectationStatus,
    /// With `closedWorld` true and both `required` and `allowed` empty, the expected result is a
    /// complete *empty* analysis set — this is how a form is declared ungrammatical. With it false,
    /// analyses the policy does not mention are neither accepted nor rejected.
    #[serde(default)]
    pub closed_world: bool,
    #[serde(default)]
    pub required: Vec<AnalysisIdentity>,
    #[serde(default)]
    pub forbidden: Vec<AnalysisIdentity>,
    #[serde(default)]
    pub allowed: Vec<AnalysisIdentity>,
}

/// One question in a suite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentCase {
    pub case_id: String,
    pub input: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Case IDs this case replaces. PanGloss follows a declared link when matching two runs; it
    /// never infers one, and never rewrites the caller's history.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<String>,
    /// Opaque to PanGloss beyond shape and size. Carried or omitted exactly as supplied, never
    /// interpreted and never reanchored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_references: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expectation: Option<Expectation>,
    /// Namespaced consumer annotations. Excluded from both assessment projections, so a second
    /// tool's notes can never change a digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// A versioned collection of cases with authoritative order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentSuite {
    pub schema: String,
    pub schema_version: u32,
    pub suite_id: String,
    pub suite_revision: String,
    #[serde(default = "default_profile")]
    pub analysis_identity_profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    pub cases: Vec<AssessmentCase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

fn default_profile() -> String {
    IDENTITY_PROFILE.to_string()
}

/// A validated suite, paired with the exact document it was parsed from.
///
/// The raw document is retained because the semantic digest is taken over it: unknown caller
/// metadata is preserved exactly and therefore participates, which re-serializing the typed struct
/// would quietly discard.
#[derive(Debug, Clone)]
pub struct ValidatedSuite {
    suite: AssessmentSuite,
    raw: Value,
    semantic_digest: String,
}

impl ValidatedSuite {
    pub fn suite(&self) -> &AssessmentSuite {
        &self.suite
    }

    pub fn raw(&self) -> &Value {
        &self.raw
    }

    /// SHA-256 over the JCS form of the whole submitted document.
    pub fn semantic_digest(&self) -> &str {
        &self.semantic_digest
    }

    pub fn cases(&self) -> &[AssessmentCase] {
        &self.suite.cases
    }
}

/// Why a suite was refused. Every variant is a typed failure, never best-effort execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuiteError {
    Malformed(String),
    WrongSchema {
        found: String,
    },
    UnsupportedSchemaVersion {
        found: u32,
        supported: u32,
    },
    /// Two cases share a case ID, so `compare`'s join key would be ambiguous.
    DuplicateCaseId {
        case_id: String,
    },
    /// `required`, `forbidden`, and `allowed` must be pairwise disjoint: an identity that is both
    /// demanded and banned has no satisfiable reading.
    OverlappingExpectation {
        case_id: String,
        first: &'static str,
        second: &'static str,
        identity_digest: String,
    },
    /// A declared identity profile this build cannot evaluate. Not silently reinterpreted — an
    /// expectation written under another profile's encoding means something else.
    UnsupportedIdentityProfile {
        found: String,
        supported: String,
    },
    SourceReferencesTooMany {
        case_id: String,
        found: usize,
        limit: usize,
    },
    SourceReferenceTooLarge {
        case_id: String,
        bytes: usize,
        limit: usize,
    },
}

impl std::fmt::Display for SuiteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SuiteError::Malformed(e) => write!(f, "suite is not valid JSON for this schema: {e}"),
            SuiteError::WrongSchema { found } => {
                write!(f, "expected schema {SUITE_SCHEMA}, found {found}")
            }
            SuiteError::UnsupportedSchemaVersion { found, supported } => write!(
                f,
                "suite schemaVersion {found} is not supported (this build reads {supported})"
            ),
            SuiteError::DuplicateCaseId { case_id } => {
                write!(f, "duplicate caseId {case_id}; case IDs must be unique")
            }
            SuiteError::OverlappingExpectation {
                case_id,
                first,
                second,
                identity_digest,
            } => write!(
                f,
                "case {case_id}: identity {identity_digest} appears in both {first} and {second}"
            ),
            SuiteError::UnsupportedIdentityProfile { found, supported } => write!(
                f,
                "suite declares identity profile {found}; this build implements {supported}"
            ),
            SuiteError::SourceReferencesTooMany {
                case_id,
                found,
                limit,
            } => write!(
                f,
                "case {case_id}: {found} source references exceeds the limit of {limit}"
            ),
            SuiteError::SourceReferenceTooLarge {
                case_id,
                bytes,
                limit,
            } => write!(
                f,
                "case {case_id}: a source reference of {bytes} bytes exceeds the limit of {limit}"
            ),
        }
    }
}

/// Parse and validate a suite document. Validation is complete before any case runs.
pub fn parse_suite(document: &str) -> Result<ValidatedSuite, SuiteError> {
    let raw: Value =
        serde_json::from_str(document).map_err(|e| SuiteError::Malformed(e.to_string()))?;

    // Check schema and version before typed deserialization, so a future-versioned document
    // reports the version rather than an incidental field mismatch.
    match raw.get("schema").and_then(Value::as_str) {
        Some(SUITE_SCHEMA) => {}
        Some(found) => {
            return Err(SuiteError::WrongSchema {
                found: found.to_string(),
            })
        }
        None => {
            return Err(SuiteError::Malformed(
                "missing required field `schema`".to_string(),
            ))
        }
    }
    match raw.get("schemaVersion").and_then(Value::as_u64) {
        Some(v) if v == u64::from(SUITE_SCHEMA_VERSION) => {}
        Some(found) => {
            return Err(SuiteError::UnsupportedSchemaVersion {
                found: found as u32,
                supported: SUITE_SCHEMA_VERSION,
            })
        }
        None => {
            return Err(SuiteError::Malformed(
                "missing required field `schemaVersion`".to_string(),
            ))
        }
    }

    let suite: AssessmentSuite =
        serde_json::from_value(raw.clone()).map_err(|e| SuiteError::Malformed(e.to_string()))?;

    if suite.analysis_identity_profile != IDENTITY_PROFILE {
        return Err(SuiteError::UnsupportedIdentityProfile {
            found: suite.analysis_identity_profile.clone(),
            supported: IDENTITY_PROFILE.to_string(),
        });
    }

    let mut seen: HashSet<&str> = HashSet::new();
    for case in &suite.cases {
        if !seen.insert(case.case_id.as_str()) {
            return Err(SuiteError::DuplicateCaseId {
                case_id: case.case_id.clone(),
            });
        }
        validate_source_references(case)?;
        if let Some(expectation) = &case.expectation {
            validate_disjoint(&case.case_id, expectation)?;
        }
    }

    let semantic_digest = digest_projection(SUITE_PROJECTION, &raw)
        .map_err(|e| SuiteError::Malformed(e.to_string()))?;

    Ok(ValidatedSuite {
        suite,
        raw,
        semantic_digest,
    })
}

fn validate_source_references(case: &AssessmentCase) -> Result<(), SuiteError> {
    if case.source_references.len() > MAX_SOURCE_REFERENCES_PER_CASE {
        return Err(SuiteError::SourceReferencesTooMany {
            case_id: case.case_id.clone(),
            found: case.source_references.len(),
            limit: MAX_SOURCE_REFERENCES_PER_CASE,
        });
    }
    for reference in &case.source_references {
        let bytes = reference.to_string().len();
        if bytes > MAX_SOURCE_REFERENCE_BYTES {
            return Err(SuiteError::SourceReferenceTooLarge {
                case_id: case.case_id.clone(),
                bytes,
                limit: MAX_SOURCE_REFERENCE_BYTES,
            });
        }
    }
    Ok(())
}

fn validate_disjoint(case_id: &str, expectation: &Expectation) -> Result<(), SuiteError> {
    let mut owner: BTreeMap<String, &'static str> = BTreeMap::new();
    for (label, identities) in [
        ("required", &expectation.required),
        ("forbidden", &expectation.forbidden),
        ("allowed", &expectation.allowed),
    ] {
        for identity in identities {
            let digest = crate::digest::identity_digest(identity);
            if let Some(previous) = owner.get(&digest) {
                if *previous != label {
                    return Err(SuiteError::OverlappingExpectation {
                        case_id: case_id.to_string(),
                        first: previous,
                        second: label,
                        identity_digest: digest,
                    });
                }
            } else {
                owner.insert(digest, label);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
        // A superseded ID need not exist in this suite: the case it replaces lived in an earlier
        // revision, which PanGloss does not have and does not require.
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
}
