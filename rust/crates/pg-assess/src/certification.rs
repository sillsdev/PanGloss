//! Exact semantic certification evidence.
//!
//! This module is intentionally narrower than an assessment report.  A report may contain
//! historical, partial, budget-limited, or broad-recall evidence; a certification ledger may
//! certify only a declared, ordered set of cases for which both sides produced complete exact
//! analysis sets.  Keeping that distinction in the type makes it impossible for a time limit or
//! a rule-only result to accidentally become a passing denominator.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{json, Value};

use crate::{canonicalize, AnalysisSet, JcsError};

pub const CERTIFICATION_LEDGER_SCHEMA: &str = "pangloss.fst-certification-ledger";
pub const CERTIFICATION_LEDGER_SCHEMA_VERSION: u32 = 1;
pub const THREE_LANGUAGE_REPORT_SCHEMA: &str = "pangloss.three-language-certification";
pub const THREE_LANGUAGE_REPORT_SCHEMA_VERSION: u32 = 1;
pub const CANONICAL_LANGUAGES: [&str; 3] = ["indonesian", "amharic", "aweti"];

/// Why a case is or is not authoritative.
///
/// All variants other than `Complete` are deliberately non-certifying.  In particular, a
/// timeout, a candidate budget latch, or an identity projection is evidence for a report but not
/// evidence that the candidate's complete semantic set was equal to the oracle's set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CaseStatus {
    Complete,
    LogicalBudget {
        dimension: String,
        value: u64,
        limit: u64,
    },
    WallClockTimeout {
        elapsed_us: u64,
        limit_us: u64,
    },
    InvalidShape { side: String },
    NotAttempted { reason: String },
    CandidateBudget {
        dimension: String,
        value: u64,
        limit: u64,
    },
    IdentityProjection { side: String, reason: String },
    SetupFailure { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseEvidence {
    pub case_id: String,
    pub source_line: usize,
    pub input: String,
    pub status: CaseStatus,
    pub oracle: Option<AnalysisSet>,
    pub candidate: Option<AnalysisSet>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceError {
    CompleteStatus,
}

impl std::fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CompleteStatus => write!(
                f,
                "noncomplete evidence cannot be constructed with the Complete status"
            ),
        }
    }
}

impl std::error::Error for EvidenceError {}

impl CaseEvidence {
    pub fn complete(
        case_id: impl Into<String>,
        source_line: usize,
        input: impl Into<String>,
        oracle: AnalysisSet,
        candidate: AnalysisSet,
    ) -> Self {
        Self {
            case_id: case_id.into(),
            source_line,
            input: input.into(),
            status: CaseStatus::Complete,
            oracle: Some(oracle),
            candidate: Some(candidate),
        }
    }

    pub fn noncomplete(
        case_id: impl Into<String>,
        source_line: usize,
        input: impl Into<String>,
        status: CaseStatus,
    ) -> Result<Self, EvidenceError> {
        if matches!(status, CaseStatus::Complete) {
            return Err(EvidenceError::CompleteStatus);
        }
        Ok(Self {
            case_id: case_id.into(),
            source_line,
            input: input.into(),
            status,
            oracle: None,
            candidate: None,
        })
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.status, CaseStatus::Complete)
            && self.oracle.is_some()
            && self.candidate.is_some()
    }

    /// Compare the canonical semantic set, not discovery order or duplicate path counts.
    pub fn exact_match(&self) -> bool {
        match (&self.status, &self.oracle, &self.candidate) {
            (CaseStatus::Complete, Some(oracle), Some(candidate)) => {
                oracle.to_outcome_value() == candidate.to_outcome_value()
            }
            _ => false,
        }
    }

    fn canonical_value(&self) -> Value {
        let mut value = json!({
            "caseId": self.case_id,
            "sourceLine": self.source_line,
            "input": self.input,
            "status": &self.status,
        });
        if self.is_complete() {
            let object = value
                .as_object_mut()
                .expect("case evidence JSON is an object");
            object.insert(
                "oracle".into(),
                self.oracle
                    .as_ref()
                    .expect("complete evidence has an oracle")
                    .to_outcome_value(),
            );
            object.insert(
                "candidate".into(),
                self.candidate
                    .as_ref()
                    .expect("complete evidence has a candidate")
                    .to_outcome_value(),
            );
            object.insert("exact".into(), Value::Bool(self.exact_match()));
        }
        value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenominatorError {
    EmptyLanguage,
    EmptyCaseSetId,
    EmptyLedger,
    EmptyCaseId,
    DuplicateCaseId { case_id: String },
    ZeroSourceLine { case_id: String },
    DuplicateSourceLine { source_line: usize },
    UnstableCaseOrder,
    DuplicateLanguage { language: String },
    MissingLanguage { language: String },
    UnexpectedLanguage { language: String },
    DuplicateExpectedLanguage { language: String },
}

impl std::fmt::Display for DenominatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyLanguage => write!(f, "language must not be empty"),
            Self::EmptyCaseSetId => write!(f, "case-set ID must not be empty"),
            Self::EmptyLedger => write!(f, "certification ledger must contain at least one case"),
            Self::EmptyCaseId => write!(f, "case ID must not be empty"),
            Self::DuplicateCaseId { case_id } => write!(f, "duplicate case ID {case_id}"),
            Self::ZeroSourceLine { case_id } => {
                write!(f, "case {case_id} has a zero source line")
            }
            Self::DuplicateSourceLine { source_line } => {
                write!(f, "duplicate source line {source_line}")
            }
            Self::UnstableCaseOrder => write!(f, "case source lines are not strictly increasing"),
            Self::DuplicateLanguage { language } => write!(f, "duplicate language {language}"),
            Self::MissingLanguage { language } => write!(f, "missing language {language}"),
            Self::UnexpectedLanguage { language } => write!(f, "unexpected language {language}"),
            Self::DuplicateExpectedLanguage { language } => {
                write!(f, "duplicate expected language {language}")
            }
        }
    }
}

impl std::error::Error for DenominatorError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LedgerSummary {
    pub declared: usize,
    pub complete: usize,
    pub exact: usize,
    pub mismatches: usize,
    pub logical_budgets: usize,
    pub timeouts: usize,
    pub invalid_shapes: usize,
    pub not_attempted: usize,
    pub candidate_budgets: usize,
    pub identity_projections: usize,
    pub setup_failures: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificationLedger {
    pub language: String,
    pub case_set_id: String,
    pub cases: Vec<CaseEvidence>,
}

impl CertificationLedger {
    pub fn new(
        language: impl Into<String>,
        case_set_id: impl Into<String>,
        cases: Vec<CaseEvidence>,
    ) -> Result<Self, DenominatorError> {
        let language = language.into();
        let case_set_id = case_set_id.into();
        if language.trim().is_empty() {
            return Err(DenominatorError::EmptyLanguage);
        }
        if case_set_id.trim().is_empty() {
            return Err(DenominatorError::EmptyCaseSetId);
        }
        if cases.is_empty() {
            return Err(DenominatorError::EmptyLedger);
        }
        let mut ids = BTreeSet::new();
        let mut lines = BTreeSet::new();
        let mut previous = None;
        for case in &cases {
            if case.case_id.trim().is_empty() {
                return Err(DenominatorError::EmptyCaseId);
            }
            if case.source_line == 0 {
                return Err(DenominatorError::ZeroSourceLine {
                    case_id: case.case_id.clone(),
                });
            }
            if !ids.insert(case.case_id.clone()) {
                return Err(DenominatorError::DuplicateCaseId {
                    case_id: case.case_id.clone(),
                });
            }
            if !lines.insert(case.source_line) {
                return Err(DenominatorError::DuplicateSourceLine {
                    source_line: case.source_line,
                });
            }
            if previous.is_some_and(|line| case.source_line <= line) {
                return Err(DenominatorError::UnstableCaseOrder);
            }
            previous = Some(case.source_line);
        }
        Ok(Self {
            language,
            case_set_id,
            cases,
        })
    }

    pub fn reconcile(&self) -> LedgerSummary {
        let mut summary = LedgerSummary {
            declared: self.cases.len(),
            complete: 0,
            exact: 0,
            mismatches: 0,
            logical_budgets: 0,
            timeouts: 0,
            invalid_shapes: 0,
            not_attempted: 0,
            candidate_budgets: 0,
            identity_projections: 0,
            setup_failures: 0,
        };
        for case in &self.cases {
            match &case.status {
                CaseStatus::Complete => {
                    summary.complete += 1;
                    if case.exact_match() {
                        summary.exact += 1;
                    } else {
                        summary.mismatches += 1;
                    }
                }
                CaseStatus::LogicalBudget { .. } => summary.logical_budgets += 1,
                CaseStatus::WallClockTimeout { .. } => summary.timeouts += 1,
                CaseStatus::InvalidShape { .. } => summary.invalid_shapes += 1,
                CaseStatus::NotAttempted { .. } => summary.not_attempted += 1,
                CaseStatus::CandidateBudget { .. } => summary.candidate_budgets += 1,
                CaseStatus::IdentityProjection { .. } => summary.identity_projections += 1,
                CaseStatus::SetupFailure { .. } => summary.setup_failures += 1,
            }
        }
        summary
    }

    pub fn can_certify(&self) -> bool {
        let summary = self.reconcile();
        summary.declared > 0 && summary.exact == summary.declared
    }

    pub fn canonical_value(&self) -> Value {
        json!({
            "schema": CERTIFICATION_LEDGER_SCHEMA,
            "schemaVersion": CERTIFICATION_LEDGER_SCHEMA_VERSION,
            "language": self.language,
            "caseSetId": self.case_set_id,
            "reconciliation": self.reconcile(),
            "canCertify": self.can_certify(),
            "cases": self.cases.iter().map(CaseEvidence::canonical_value).collect::<Vec<_>>(),
        })
    }

    pub fn canonical_json(&self) -> Result<String, JcsError> {
        canonicalize(&self.canonical_value())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ThreeLanguageReconciliation {
    pub language_count: usize,
    pub total_declared: usize,
    pub total_complete: usize,
    pub total_exact: usize,
    pub noncanonical_language_count: usize,
    pub noncertifying_language_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreeLanguageDenominatorGate {
    ledgers: Vec<CertificationLedger>,
    expected_denominators: BTreeMap<String, usize>,
}

impl ThreeLanguageDenominatorGate {
    /// Construct a gate with a fixed, declared denominator for every language.
    ///
    /// The expected list is intentionally explicit.  A gate must not infer its denominator from
    /// the evidence it is supposed to certify.
    pub fn new_with_expected<L, E>(
        ledgers: L,
        expected: E,
    ) -> Result<Self, DenominatorError>
    where
        L: IntoIterator<Item = CertificationLedger>,
        E: IntoIterator<Item = (String, usize)>,
    {
        let mut expected_denominators = BTreeMap::new();
        for (language, count) in expected {
            if language.trim().is_empty() {
                return Err(DenominatorError::EmptyLanguage);
            }
            if expected_denominators
                .insert(language.to_string(), count)
                .is_some()
            {
                return Err(DenominatorError::DuplicateExpectedLanguage {
                    language: language.to_string(),
                });
            }
        }
        for language in expected_denominators.keys() {
            if !CANONICAL_LANGUAGES.contains(&language.as_str()) {
                return Err(DenominatorError::UnexpectedLanguage {
                    language: language.clone(),
                });
            }
        }
        for language in CANONICAL_LANGUAGES {
            if !expected_denominators.contains_key(language) {
                return Err(DenominatorError::MissingLanguage {
                    language: language.to_string(),
                });
            }
        }
        let mut ledgers = ledgers.into_iter().collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        for ledger in &ledgers {
            if !seen.insert(ledger.language.clone()) {
                return Err(DenominatorError::DuplicateLanguage {
                    language: ledger.language.clone(),
                });
            }
            if !expected_denominators.contains_key(&ledger.language) {
                return Err(DenominatorError::UnexpectedLanguage {
                    language: ledger.language.clone(),
                });
            }
        }
        for language in expected_denominators.keys() {
            if !seen.contains(language) {
                return Err(DenominatorError::MissingLanguage {
                    language: language.clone(),
                });
            }
        }
        // The caller's language order is not semantic.  Store the declared Indonesian, Amharic,
        // Aweti order so the same report cannot change bytes merely because a runner discovered
        // languages in a different order.
        ledgers.sort_by_key(|ledger| {
            CANONICAL_LANGUAGES
                .iter()
                .position(|language| *language == ledger.language)
                .expect("validated language is canonical")
        });
        Ok(Self {
            ledgers,
            expected_denominators,
        })
    }

    pub fn reconcile(&self) -> ThreeLanguageReconciliation {
        let mut result = ThreeLanguageReconciliation {
            language_count: self.ledgers.len(),
            total_declared: 0,
            total_complete: 0,
            total_exact: 0,
            noncanonical_language_count: 0,
            noncertifying_language_count: 0,
        };
        for ledger in &self.ledgers {
            let summary = ledger.reconcile();
            result.total_declared += summary.declared;
            result.total_complete += summary.complete;
            result.total_exact += summary.exact;
            if self.expected_denominators.get(&ledger.language) != Some(&summary.declared) {
                result.noncanonical_language_count += 1;
            }
            if !ledger.can_certify() {
                result.noncertifying_language_count += 1;
            }
        }
        result
    }

    pub fn can_certify(&self) -> bool {
        self.ledgers.len() == self.expected_denominators.len()
            && self
                .ledgers
                .iter()
                .all(|ledger| {
                    self.expected_denominators
                        .get(&ledger.language)
                        .is_some_and(|expected| *expected == ledger.reconcile().declared)
                        && ledger.can_certify()
                })
    }

    pub fn canonical_value(&self) -> Value {
        let expected = CANONICAL_LANGUAGES
            .iter()
            .map(|language| {
                json!({
                    "language": language,
                    "declared": self
                        .expected_denominators
                        .get(*language)
                        .copied()
                        .expect("validated denominator is canonical"),
                })
            })
            .collect::<Vec<_>>();
        json!({
            "schema": THREE_LANGUAGE_REPORT_SCHEMA,
            "schemaVersion": THREE_LANGUAGE_REPORT_SCHEMA_VERSION,
            "expectedDenominators": expected,
            "reconciliation": self.reconcile(),
            "canCertify": self.can_certify(),
            "languages": self.ledgers.iter().map(CertificationLedger::canonical_value).collect::<Vec<_>>(),
        })
    }

    pub fn canonical_json(&self) -> Result<String, JcsError> {
        canonicalize(&self.canonical_value())
    }
}
