//! `pangloss.assessment-report/v1` — the immutable evidence one `assess` run produces.
//!
//! The report is the artifact everything downstream joins on, so two properties matter more than
//! convenience.
//!
//! **Identities are values, interned only for size.** Every stable source key appears once in a
//! top-level `keyTable` and cases reference it by index (design D6). On a 50k-case suite that is
//! roughly a 5-10x reduction, and it makes the key table itself useful — two reports' tables diff
//! directly to show what was added or deleted at the inventory level. But interning is a
//! *serialization* concern: every digest is computed over the **expanded** form, so a different
//! table ordering can never move a digest. Hashing the indices instead would silently break that.
//!
//! **Three digests over three drop-lists.** `reportId` drops nothing, `semanticDigest` drops
//! timestamps, paths, timings and `sourceSha256`, `outcomeDigest` additionally drops tool versions,
//! budgets, pipeline, diagnostics and duplicate counts (design D3/D3a). Reading which one moved
//! localizes a change without diffing anything.
//!
//! Diagnostics reach the semantic projection as `(code, count)` pairs rather than prose, so
//! rewording an importer warning is never reported as a context difference.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::digest::{digest_projection, identity_digest, OUTCOME_PROJECTION, SEMANTIC_PROJECTION};
use crate::identity::{AnalysisIdentity, IDENTITY_PROFILE};
use crate::jcs::{self, JcsError};
use crate::outcome::{AssessmentStatus, CaseOutcome, IncompleteReason, NotAttemptedReason};
use crate::set::AnalysisSet;

pub const REPORT_SCHEMA: &str = "pangloss.assessment-report";
pub const REPORT_SCHEMA_VERSION: u32 = 1;

/// The suite this report answers, recorded so `golden-diff` can refuse to evaluate an old run
/// against revised policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SuiteRef {
    pub suite_id: String,
    pub suite_revision: String,
    pub semantic_digest: String,
    pub analysis_identity_profile: String,
}

/// How the run was executed and what contained it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Execution {
    /// `foma-confirm` or `hermitcrab`. Recorded rather than inferred: the two pipelines must agree
    /// on complete cases, and a disagreement is exactly what a reader needs to see.
    pub pipeline: String,
    /// Effective logical budgets by dimension. Empty means unbounded, which is the default until a
    /// resource envelope is named.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub budgets: BTreeMap<String, u64>,
    /// The outer wall-clock safety net, if one was armed. Recorded in the report but never in a
    /// digest: it is machine-dependent, and if it fires the report is marked unreproducible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_clock_limit_us: Option<u64>,
}

/// What was analyzed, and by which build of PanGloss.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Provenance {
    /// Exact source bytes. Recorded and visible in `contextDifferences`, but deliberately outside
    /// the semantic projection (design D3a): with `core.autocrlf` in play the same grammar has
    /// different bytes on Windows and Linux, and that difference is git's, not the grammar's.
    pub source_sha256: String,
    pub source_kind: String,
    /// What was actually analyzed. `semanticDigest` rests entirely on this.
    pub model_fingerprint: String,
    pub importer_version: String,
    pub compiler_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Warning,
    Error,
}

/// One importer or compiler diagnostic.
///
/// The `code` is what `compare` diffs; `message` is for humans and never reaches a digest, so
/// rewording it is not a change in the grammar's context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
}

/// Why a whole assessment failed, when one did (§17.7's "nullable typed `failure`").
///
/// Distinct from a per-case outcome. A case that was `not_attempted` says nothing about *why* the
/// run as a whole could not proceed, and a consumer scanning a `failed` report should not have to
/// infer that from a diagnostic's prose or from the process exit code — which it may not even have,
/// if it is reading an artifact someone else produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// Suite validation passed, then import, compile, or setup failed safely, with all available build evidence retained.
    AssessmentSetupFailed,
    /// The requested pipeline cannot run this grammar. Never a silent fallback to the other one.
    UnsupportedCapability,
    /// Resource containment prevented a trustworthy artifact.
    ContainmentPrevented,
    /// An internal fault. Named rather than dressed up as a grammar problem.
    InternalError,
}

/// The top-level failure record. Present iff the run failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssessmentFailure {
    pub kind: FailureKind,
    /// Human-readable detail. Never the machine-readable part: consumers branch on `kind`.
    pub message: String,
}

/// One case as recorded before interning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseRecord {
    pub case_id: String,
    pub input: String,
    pub outcome: CaseOutcome,
    /// Baseline case IDs this case replaces, carried through from the suite.
    ///
    /// Recorded on the report rather than looked up in the suite at compare time, because a
    /// comparison must work from two artifacts alone — the suite that declared the link may be
    /// several revisions gone by the time anyone compares. Without it a caller who renumbers gets
    /// phantom `baseline_only`/`candidate_only` pairs on every subsequent comparison, permanently.
    pub supersedes: Vec<String>,
}

/// Everything a run produced except its digests, which are derived rather than supplied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportDraft {
    /// Caller- or clock-supplied. Nonsemantic evidence: it moves `reportId` and nothing else.
    pub generated_at: String,
    pub suite: SuiteRef,
    pub execution: Execution,
    pub provenance: Provenance,
    pub diagnostics: Vec<Diagnostic>,
    pub cases: Vec<CaseRecord>,
    /// Why the run failed, when it did (§17.7). `None` for a run that produced results.
    pub failure: Option<AssessmentFailure>,
    /// Namespaced consumer annotations. Outside both semantic projections, inside `reportId`.
    pub extensions: Option<Value>,
}

/// A finished report: a draft plus the three digests and the derived status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssessmentReport {
    draft: ReportDraft,
    status: AssessmentStatus,
    reproducible: bool,
    report_id: String,
    semantic_digest: String,
    outcome_digest: String,
}

impl ReportDraft {
    /// Derive status, reproducibility, and all three digests.
    pub fn finish(self) -> Result<AssessmentReport, JcsError> {
        let outcomes: Vec<CaseOutcome> = self.cases.iter().map(|c| c.outcome.clone()).collect();
        let status = crate::outcome::derive_status(&outcomes);
        let reproducible = outcomes.iter().all(CaseOutcome::is_reproducible);

        let semantic_digest = digest_projection(SEMANTIC_PROJECTION, &self.semantic_value())?;
        let outcome_digest = digest_projection(OUTCOME_PROJECTION, &self.outcome_value())?;

        let mut report = AssessmentReport {
            draft: self,
            status,
            reproducible,
            report_id: String::new(),
            semantic_digest,
            outcome_digest,
        };
        // Computed over the artifact with the `reportId` field itself absent — the only value it cannot contain is its own.
        report.report_id = digest_projection(
            &format!("{REPORT_SCHEMA}/v{REPORT_SCHEMA_VERSION}"),
            &report.to_value(),
        )?;
        Ok(report)
    }

    /// "Was this the same run?" — everything but timestamps, paths, timings, `sourceSha256`, and diagnostic prose.
    fn semantic_value(&self) -> Value {
        json!({
            "suite": serde_json::to_value(&self.suite).expect("suite ref is plain strings"),
            "execution": serde_json::to_value(&self.execution).expect("execution is plain scalars"),
            "provenance": {
                "modelFingerprint": self.provenance.model_fingerprint,
                "sourceKind": self.provenance.source_kind,
                "importerVersion": self.provenance.importer_version,
                "compilerVersion": self.provenance.compiler_version,
            },
            "failure": serde_json::to_value(&self.failure).expect("failure is plain data"),
            "diagnostics": diagnostic_counts(&self.diagnostics),
            "cases": self.cases.iter().map(|c| json!({
                "caseId": c.case_id,
                "input": c.input,
                "supersedes": c.supersedes,
                "outcome": outcome_semantic_value(&c.outcome),
            })).collect::<Vec<_>>(),
        })
    }

    /// "Did the grammar behave the same?" — suite digest, per-case outcome kind, and identity sets, deliberately blind to pipeline, cost, and PanGloss build.
    fn outcome_value(&self) -> Value {
        json!({
            "suiteDigest": self.suite.semantic_digest,
            "analysisIdentityProfile": self.suite.analysis_identity_profile,
            // `supersedes` is lineage, not behavior, so it stays out of this projection: relabelling a suite's case IDs must never look like a behavior change.
            "cases": self.cases.iter().map(|c| json!({
                "caseId": c.case_id,
                "outcome": c.outcome.kind(),
                "analyses": c.outcome.analyses().map(AnalysisSet::to_outcome_value),
            })).collect::<Vec<_>>(),
        })
    }
}

impl AssessmentReport {
    pub fn draft(&self) -> &ReportDraft {
        &self.draft
    }
    pub fn status(&self) -> AssessmentStatus {
        self.status
    }
    /// Whether every case outcome was decided deterministically. A wall-clock stop makes this
    /// false, because the same run on another machine could have completed.
    pub fn is_reproducible(&self) -> bool {
        self.reproducible
    }
    pub fn report_id(&self) -> &str {
        &self.report_id
    }
    pub fn semantic_digest(&self) -> &str {
        &self.semantic_digest
    }
    pub fn outcome_digest(&self) -> &str {
        &self.outcome_digest
    }
    pub fn cases(&self) -> &[CaseRecord] {
        &self.draft.cases
    }

    /// Why the run failed, if it did.
    pub fn failure(&self) -> Option<&AssessmentFailure> {
        self.draft.failure.as_ref()
    }

    /// Declared lineage as `(superseded baseline case ID, this report's case ID)` pairs.
    pub fn supersedes(&self) -> Vec<(String, String)> {
        self.draft
            .cases
            .iter()
            .flat_map(|case| {
                case.supersedes
                    .iter()
                    .map(|superseded| (superseded.clone(), case.case_id.clone()))
            })
            .collect()
    }

    /// The serialized artifact, with stable source keys interned into `keyTable`.
    ///
    /// `reportId` is present only once it has been computed; `ReportDraft::finish` calls this
    /// with the field still empty to obtain the preimage.
    pub fn to_value(&self) -> Value {
        let table = KeyTable::build(&self.draft.cases);

        let mut root = Map::new();
        root.insert("schema".into(), json!(REPORT_SCHEMA));
        root.insert("schemaVersion".into(), json!(REPORT_SCHEMA_VERSION));
        if !self.report_id.is_empty() {
            root.insert("reportId".into(), json!(self.report_id));
        }
        root.insert("semanticDigest".into(), json!(self.semantic_digest));
        root.insert("outcomeDigest".into(), json!(self.outcome_digest));
        root.insert("generatedAt".into(), json!(self.draft.generated_at));
        root.insert(
            "status".into(),
            serde_json::to_value(self.status).expect("status is a unit enum"),
        );
        root.insert("reproducible".into(), json!(self.reproducible));
        // Always emitted, never skipped when absent: an explicit null says "did not fail", where a missing key would look identical to an older producer.
        root.insert(
            "failure".into(),
            serde_json::to_value(&self.draft.failure)
                .expect("failure is a unit enum plus a string"),
        );
        root.insert(
            "suite".into(),
            serde_json::to_value(&self.draft.suite).expect("suite ref is plain strings"),
        );
        root.insert(
            "execution".into(),
            serde_json::to_value(&self.draft.execution).expect("execution is plain scalars"),
        );
        root.insert(
            "provenance".into(),
            serde_json::to_value(&self.draft.provenance).expect("provenance is plain strings"),
        );
        root.insert(
            "diagnostics".into(),
            serde_json::to_value(&self.draft.diagnostics).expect("diagnostics are plain strings"),
        );
        root.insert("keyTable".into(), json!(table.keys));
        root.insert(
            "cases".into(),
            Value::Array(
                self.draft
                    .cases
                    .iter()
                    .map(|c| table.case_value(c))
                    .collect(),
            ),
        );
        if let Some(extensions) = &self.draft.extensions {
            root.insert("extensions".into(), extensions.clone());
        }
        Value::Object(root)
    }

    /// The canonical bytes of the artifact.
    pub fn to_canonical_json(&self) -> Result<String, JcsError> {
        jcs::canonicalize(&self.to_value())
    }
}

/// Interned stable source keys, sorted for determinism — every digest is taken over expanded identities, never the table's order.
struct KeyTable {
    keys: Vec<String>,
    index: BTreeMap<String, usize>,
}

impl KeyTable {
    fn build(cases: &[CaseRecord]) -> Self {
        let mut keys: Vec<String> = Vec::new();
        let mut seen: BTreeMap<String, ()> = BTreeMap::new();
        for case in cases {
            let Some(set) = case.outcome.analyses() else {
                continue;
            };
            for entry in set.entries() {
                for key in entry.identity.morphemes.iter().flatten() {
                    seen.entry(key.clone()).or_default();
                }
                if let Some(category) = &entry.identity.category {
                    seen.entry(category.clone()).or_default();
                }
            }
        }
        let mut index = BTreeMap::new();
        for (position, key) in seen.into_keys().enumerate() {
            index.insert(key.clone(), position);
            keys.push(key);
        }
        KeyTable { keys, index }
    }

    fn intern(&self, key: &str) -> Value {
        json!(self.index[key])
    }

    fn identity_value(&self, identity: &AnalysisIdentity) -> Value {
        json!({
            "morphemes": identity.morphemes.iter().map(|slot| match slot {
                // A guessed root has no authored source, so it interns to nothing rather than to a key that happens to spell "null".
                None => Value::Null,
                Some(key) => self.intern(key),
            }).collect::<Vec<_>>(),
            "rootIndex": identity.root_index,
            "category": identity.category.as_deref().map(|c| self.intern(c)),
        })
    }

    fn case_value(&self, case: &CaseRecord) -> Value {
        let mut obj = Map::new();
        obj.insert("caseId".into(), json!(case.case_id));
        obj.insert("input".into(), json!(case.input));
        if !case.supersedes.is_empty() {
            obj.insert("supersedes".into(), json!(case.supersedes));
        }
        obj.insert("outcome".into(), json!(case.outcome.kind()));
        match &case.outcome {
            CaseOutcome::Complete(set) => {
                obj.insert(
                    "analyses".into(),
                    Value::Array(
                        set.entries()
                            .iter()
                            .map(|e| {
                                json!({
                                    "identity": self.identity_value(&e.identity),
                                    "identityDigest": e.identity_digest,
                                    "duplicateCount": e.duplicate_count,
                                    "guessed": e.guessed,
                                })
                            })
                            .collect(),
                    ),
                );
            }
            CaseOutcome::Incomplete(reason) => {
                obj.insert(
                    "incomplete".into(),
                    serde_json::to_value(reason).expect("incomplete reasons are plain scalars"),
                );
            }
            CaseOutcome::NotAttempted(reason) => {
                obj.insert(
                    "notAttempted".into(),
                    serde_json::to_value(reason).expect("not-attempted reasons are unit variants"),
                );
            }
        }
        Value::Object(obj)
    }
}

/// Diagnostics reduced to what `compare` may act on: how many of each code, never the prose.
fn diagnostic_counts(diagnostics: &[Diagnostic]) -> Value {
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    for diagnostic in diagnostics {
        *counts.entry(diagnostic.code.as_str()).or_default() += 1;
    }
    Value::Array(
        counts
            .into_iter()
            .map(|(code, count)| json!({ "code": code, "count": count }))
            .collect(),
    )
}

/// A case's contribution to the semantic projection: outcome kind, why it stopped if it did, and the full analysis set with duplicate evidence.
fn outcome_semantic_value(outcome: &CaseOutcome) -> Value {
    let mut obj = Map::new();
    obj.insert("kind".into(), json!(outcome.kind()));
    match outcome {
        CaseOutcome::Complete(set) => {
            obj.insert("analyses".into(), set.to_semantic_value());
        }
        CaseOutcome::Incomplete(reason) => {
            obj.insert(
                "incomplete".into(),
                serde_json::to_value(reason).expect("incomplete reasons are plain scalars"),
            );
        }
        CaseOutcome::NotAttempted(reason) => {
            obj.insert(
                "notAttempted".into(),
                serde_json::to_value(reason).expect("not-attempted reasons are unit variants"),
            );
        }
    }
    Value::Object(obj)
}

/// Reading a report back is not the inverse of writing one: the artifact is the authority, and a
/// consumer must be able to load a two-year-old report whose grammar no longer compiles. So this
/// works entirely from the artifact's own key table and never touches a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportError {
    Malformed(String),
    WrongSchema(String),
    UnsupportedVersion(u64),
    BadField(String),
    KeyIndexOutOfRange(usize, usize),
    ForeignIdentityProfile(String),
}

impl std::fmt::Display for ReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReportError::Malformed(e) => {
                write!(f, "report is not valid JSON for this schema: {e}")
            }
            ReportError::WrongSchema(found) => {
                write!(f, "expected schema {REPORT_SCHEMA}, found {found}")
            }
            ReportError::UnsupportedVersion(found) => write!(
                f,
                "report schemaVersion {found} is not supported (this build reads \
                 {REPORT_SCHEMA_VERSION})"
            ),
            ReportError::BadField(name) => {
                write!(f, "field {name} is missing or has the wrong type")
            }
            ReportError::KeyIndexOutOfRange(index, len) => write!(
                f,
                "key index {index} is outside a key table of {len} entries"
            ),
            ReportError::ForeignIdentityProfile(found) => write!(
                f,
                "report declares identity profile {found}; this build implements {IDENTITY_PROFILE}"
            ),
        }
    }
}

impl std::error::Error for ReportError {}

/// Parse an assessment report, expanding interned keys back to values.
pub fn parse_report(document: &str) -> Result<AssessmentReport, ReportError> {
    let root: Value =
        serde_json::from_str(document).map_err(|e| ReportError::Malformed(e.to_string()))?;
    let object = root
        .as_object()
        .ok_or_else(|| ReportError::BadField("<root>".into()))?;

    match object.get("schema").and_then(Value::as_str) {
        Some(REPORT_SCHEMA) => {}
        other => return Err(ReportError::WrongSchema(other.unwrap_or("").to_string())),
    }
    match object.get("schemaVersion").and_then(Value::as_u64) {
        Some(v) if v == u64::from(REPORT_SCHEMA_VERSION) => {}
        Some(v) => return Err(ReportError::UnsupportedVersion(v)),
        None => return Err(ReportError::BadField("schemaVersion".into())),
    }

    let suite: SuiteRef = field(object, "suite")?;
    if suite.analysis_identity_profile != IDENTITY_PROFILE {
        // Refused rather than best-effort: expectations written under another profile's encoding would silently miss.
        return Err(ReportError::ForeignIdentityProfile(
            suite.analysis_identity_profile,
        ));
    }

    let key_table: Vec<String> = field(object, "keyTable")?;
    let cases_value = object
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| ReportError::BadField("cases".into()))?;
    let mut cases = Vec::with_capacity(cases_value.len());
    for case in cases_value {
        cases.push(read_case(case, &key_table)?);
    }

    let draft = ReportDraft {
        generated_at: object
            .get("generatedAt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        suite,
        execution: field(object, "execution")?,
        provenance: field(object, "provenance")?,
        diagnostics: field(object, "diagnostics")?,
        failure: match object.get("failure") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                serde_json::from_value(value.clone())
                    .map_err(|_| ReportError::BadField("failure".into()))?,
            ),
        },
        cases,
        extensions: object.get("extensions").cloned(),
    };

    // Recomputed from the expanded content rather than trusted from the file, so a hand-edited report is caught immediately.
    draft
        .finish()
        .map_err(|e| ReportError::Malformed(e.to_string()))
}

fn field<T: for<'de> Deserialize<'de>>(
    object: &Map<String, Value>,
    name: &str,
) -> Result<T, ReportError> {
    let value = object
        .get(name)
        .ok_or_else(|| ReportError::BadField(name.into()))?;
    serde_json::from_value(value.clone()).map_err(|_| ReportError::BadField(name.into()))
}

fn read_case(value: &Value, table: &[String]) -> Result<CaseRecord, ReportError> {
    let object = value
        .as_object()
        .ok_or_else(|| ReportError::BadField("cases[]".into()))?;
    let case_id = object
        .get("caseId")
        .and_then(Value::as_str)
        .ok_or_else(|| ReportError::BadField("cases[].caseId".into()))?
        .to_string();
    let input = object
        .get("input")
        .and_then(Value::as_str)
        .ok_or_else(|| ReportError::BadField("cases[].input".into()))?
        .to_string();

    let outcome = match object.get("outcome").and_then(Value::as_str) {
        Some("complete") => {
            let analyses = object
                .get("analyses")
                .and_then(Value::as_array)
                .ok_or_else(|| ReportError::BadField("cases[].analyses".into()))?;
            let mut annotated = Vec::with_capacity(analyses.len());
            for analysis in analyses {
                let identity = read_identity(
                    analysis.get("identity").ok_or_else(|| {
                        ReportError::BadField("cases[].analyses[].identity".into())
                    })?,
                    table,
                )?;
                let guessed = analysis
                    .get("guessed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                // Duplicate counts are evidence, so they are restored rather than recounted.
                let count = analysis
                    .get("duplicateCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .max(1);
                for _ in 0..count {
                    annotated.push((identity.clone(), guessed));
                }
            }
            CaseOutcome::Complete(AnalysisSet::from_annotated(annotated))
        }
        Some("incomplete") => {
            CaseOutcome::Incomplete(read_reason::<IncompleteReason>(object, "incomplete")?)
        }
        Some("not_attempted") => {
            CaseOutcome::NotAttempted(read_reason::<NotAttemptedReason>(object, "notAttempted")?)
        }
        _ => return Err(ReportError::BadField("cases[].outcome".into())),
    };

    let supersedes = match object.get("supersedes") {
        None | Some(Value::Null) => Vec::new(),
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| ReportError::BadField("cases[].supersedes".into()))?,
    };

    Ok(CaseRecord {
        case_id,
        input,
        outcome,
        supersedes,
    })
}

fn read_reason<T: for<'de> Deserialize<'de>>(
    object: &Map<String, Value>,
    name: &str,
) -> Result<T, ReportError> {
    field(object, name)
}

fn read_identity(value: &Value, table: &[String]) -> Result<AnalysisIdentity, ReportError> {
    let resolve = |slot: &Value| -> Result<Option<String>, ReportError> {
        match slot {
            Value::Null => Ok(None),
            Value::Number(n) => {
                let index = n
                    .as_u64()
                    .ok_or_else(|| ReportError::BadField("key index".into()))?
                    as usize;
                table
                    .get(index)
                    .cloned()
                    .map(Some)
                    .ok_or(ReportError::KeyIndexOutOfRange(index, table.len()))
            }
            _ => Err(ReportError::BadField("key index".into())),
        }
    };

    let morphemes_value = value
        .get("morphemes")
        .and_then(Value::as_array)
        .ok_or_else(|| ReportError::BadField("identity.morphemes".into()))?;
    let mut morphemes = Vec::with_capacity(morphemes_value.len());
    for slot in morphemes_value {
        morphemes.push(resolve(slot)?);
    }

    Ok(AnalysisIdentity {
        morphemes,
        root_index: value
            .get("rootIndex")
            .and_then(Value::as_i64)
            .ok_or_else(|| ReportError::BadField("identity.rootIndex".into()))?
            as i32,
        category: resolve(value.get("category").unwrap_or(&Value::Null))?,
    })
}

/// The digest of an identity as recorded in a report, for CLI selection. Recomputed from the
/// expanded value rather than read from the artifact, so a stale or forged `identityDigest` cannot
/// make two unequal analyses look like a match.
pub fn recompute_identity_digest(identity: &AnalysisIdentity) -> String {
    identity_digest(identity)
}

#[cfg(test)]
mod tests {
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
}
