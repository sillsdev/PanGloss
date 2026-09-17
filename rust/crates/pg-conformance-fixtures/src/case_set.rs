//! Versioned, source-bound declarations of semantic corpus cases.
//!
//! A case set is deliberately separate from a word-list slice.  It names the exact source line
//! and input text that a later semantic gate must execute, so changing a private corpus cannot
//! silently change a denominator.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CASE_SET_SCHEMA: &str = "pangloss.conformance-case-set";
pub const CASE_SET_SCHEMA_VERSION: u32 = 1;

/// Hash the exact source bytes named by a case set.  This is kept local to the fixture schema so
/// the shared fixture crate does not acquire a reverse dependency on the assessment layer just
/// to borrow its digest helper.
pub fn source_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::from("sha256:");
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaseSetDocument {
    pub schema: String,
    pub schema_version: u32,
    pub case_set_id: String,
    pub source: String,
    pub source_sha256: String,
    pub declared_count: usize,
    pub cases: Vec<CaseSetCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaseSetCase {
    pub case_id: String,
    /// One-based line number in the exact source file.
    pub source_line: usize,
    pub input: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseSetError {
    Malformed(String),
    WrongSchema {
        found: String,
    },
    UnsupportedVersion {
        found: u32,
    },
    EmptyField {
        field: &'static str,
    },
    InvalidSourcePath,
    InvalidDigest,
    DeclaredCountMismatch {
        declared: usize,
        actual: usize,
    },
    DuplicateCaseId {
        case_id: String,
    },
    DuplicateSourceLine {
        source_line: usize,
    },
    UnstableCaseOrder {
        previous: usize,
        current: usize,
    },
    ZeroSourceLine {
        case_id: String,
    },
    SourceNotUtf8,
    SourceHashMismatch {
        expected: String,
        actual: String,
    },
    SourceLineMissing {
        case_id: String,
        source_line: usize,
    },
    SourceLineTextMismatch {
        case_id: String,
        source_line: usize,
        expected: String,
        actual: String,
    },
}

impl std::fmt::Display for CaseSetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(error) => write!(f, "case set is malformed: {error}"),
            Self::WrongSchema { found } => write!(f, "expected {CASE_SET_SCHEMA}, found {found}"),
            Self::UnsupportedVersion { found } => write!(
                f,
                "case-set schema version {found} is unsupported (implemented {CASE_SET_SCHEMA_VERSION})"
            ),
            Self::EmptyField { field } => write!(f, "case-set field {field} must not be empty"),
            Self::InvalidSourcePath => write!(f, "case-set source must be a relative path inside the corpus root"),
            Self::InvalidDigest => write!(f, "sourceSha256 must be a sha256: digest"),
            Self::DeclaredCountMismatch { declared, actual } => {
                write!(f, "declared case count {declared} does not match {actual} cases")
            }
            Self::DuplicateCaseId { case_id } => write!(f, "duplicate case ID {case_id}"),
            Self::DuplicateSourceLine { source_line } => {
                write!(f, "duplicate source line {source_line}")
            }
            Self::UnstableCaseOrder { previous, current } => {
                write!(f, "case source lines are not strictly increasing: {previous}, {current}")
            }
            Self::ZeroSourceLine { case_id } => {
                write!(f, "case {case_id} has a zero source line")
            }
            Self::SourceNotUtf8 => write!(f, "case-set source is not UTF-8"),
            Self::SourceHashMismatch { expected, actual } => {
                write!(f, "source hash mismatch: expected {expected}, got {actual}")
            }
            Self::SourceLineMissing { case_id, source_line } => {
                write!(f, "case {case_id} names missing source line {source_line}")
            }
            Self::SourceLineTextMismatch {
                case_id,
                source_line,
                expected,
                actual,
            } => write!(
                f,
                "case {case_id} line {source_line} differs: expected {expected:?}, got {actual:?}"
            ),
        }
    }
}

impl std::error::Error for CaseSetError {}

impl CaseSetDocument {
    pub fn verify_source(&self, source_bytes: &[u8]) -> Result<(), CaseSetError> {
        let actual_hash = source_sha256(source_bytes);
        if self.source_sha256 != actual_hash {
            return Err(CaseSetError::SourceHashMismatch {
                expected: self.source_sha256.clone(),
                actual: actual_hash,
            });
        }
        let source = std::str::from_utf8(source_bytes).map_err(|_| CaseSetError::SourceNotUtf8)?;
        let lines: Vec<&str> = source.lines().collect();
        for case in &self.cases {
            let Some(actual) = lines.get(case.source_line.saturating_sub(1)) else {
                return Err(CaseSetError::SourceLineMissing {
                    case_id: case.case_id.clone(),
                    source_line: case.source_line,
                });
            };
            if *actual != case.input {
                return Err(CaseSetError::SourceLineTextMismatch {
                    case_id: case.case_id.clone(),
                    source_line: case.source_line,
                    expected: case.input.clone(),
                    actual: (*actual).to_string(),
                });
            }
        }
        Ok(())
    }
}

pub fn parse_case_set(document: &str) -> Result<CaseSetDocument, CaseSetError> {
    let parsed: CaseSetDocument = serde_json::from_str(document)
        .map_err(|error| CaseSetError::Malformed(error.to_string()))?;
    validate_case_set(parsed)
}

fn validate_case_set(document: CaseSetDocument) -> Result<CaseSetDocument, CaseSetError> {
    if document.schema != CASE_SET_SCHEMA {
        return Err(CaseSetError::WrongSchema {
            found: document.schema,
        });
    }
    if document.schema_version != CASE_SET_SCHEMA_VERSION {
        return Err(CaseSetError::UnsupportedVersion {
            found: document.schema_version,
        });
    }
    if document.case_set_id.trim().is_empty() {
        return Err(CaseSetError::EmptyField { field: "caseSetId" });
    }
    if document.source.trim().is_empty() {
        return Err(CaseSetError::EmptyField { field: "source" });
    }
    let source_path = Path::new(&document.source);
    if source_path.is_absolute() || document.source.split(['/', '\\']).any(|part| part == "..") {
        return Err(CaseSetError::InvalidSourcePath);
    }
    if !document.source_sha256.starts_with("sha256:")
        || document.source_sha256.len() != "sha256:".len() + 64
        || !document.source_sha256["sha256:".len()..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(CaseSetError::InvalidDigest);
    }
    if document.declared_count != document.cases.len() {
        return Err(CaseSetError::DeclaredCountMismatch {
            declared: document.declared_count,
            actual: document.cases.len(),
        });
    }

    let mut ids = BTreeSet::new();
    let mut lines = BTreeSet::new();
    let mut previous = None;
    for case in &document.cases {
        if case.case_id.trim().is_empty() {
            return Err(CaseSetError::EmptyField { field: "caseId" });
        }
        if case.source_line == 0 {
            return Err(CaseSetError::ZeroSourceLine {
                case_id: case.case_id.clone(),
            });
        }
        if !ids.insert(case.case_id.clone()) {
            return Err(CaseSetError::DuplicateCaseId {
                case_id: case.case_id.clone(),
            });
        }
        if !lines.insert(case.source_line) {
            return Err(CaseSetError::DuplicateSourceLine {
                source_line: case.source_line,
            });
        }
        if let Some(previous) = previous {
            if case.source_line <= previous {
                return Err(CaseSetError::UnstableCaseOrder {
                    previous,
                    current: case.source_line,
                });
            }
        }
        previous = Some(case.source_line);
    }
    Ok(document)
}
