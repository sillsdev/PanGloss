//! The grammar-assessment evidence layer: structured analysis identity, canonical JSON, and the
//! digests every assessment artifact is identified by.
//!
//! `openspec/changes/add-grammar-assessment`. This crate is merge unit 1 — the identity and digest
//! foundation the four operations (`assess`, `compare`, `golden-diff`, `investigate`) are built on.
//! It deliberately knows nothing about suites, reports, or the CLI.
//!
//! The one idea everything here rests on: **an analysis identity is a value, not a reference**
//! (ADR 0006). It carries stable source keys rather than the dense compiler-assigned ordinals
//! `pg_parse::WordAnalysis` uses internally, so a grammar edit that deletes or renames a morpheme
//! produces ordinary `added`/`removed` evidence instead of a comparison failure — and a report
//! stays readable years later, when neither grammar still compiles.

pub mod delta;
pub mod digest;
pub mod identity;
pub mod jcs;
pub mod model;
pub mod outcome;
pub mod report;
pub mod set;
pub mod suite;

pub use delta::{
    compare, AnnotationChange, CaseDelta, ContextDifference, DeltaCategory, DuplicateCountChange,
    GrammarDelta, NotComparableReason, DELTA_SCHEMA, DELTA_SCHEMA_VERSION,
};
pub use digest::{
    digest_projection, identity_digest, sha256_bytes, OUTCOME_PROJECTION, SEMANTIC_PROJECTION,
};
pub use identity::{AnalysisIdentity, IdentityError, MorphemeKey, IDENTITY_PROFILE};
pub use jcs::{canonicalize, JcsError};
pub use model::{model_fingerprint, source_sha256, SourceKind, MODEL_PROJECTION};
pub use outcome::{
    derive_status, AssessmentStatus, CaseOutcome, IncompleteReason, NotAttemptedReason,
};
pub use report::{
    parse_report, AssessmentReport, CaseRecord, Diagnostic, Execution, Provenance, ReportDraft,
    ReportError, Severity, SuiteRef, REPORT_SCHEMA, REPORT_SCHEMA_VERSION,
};
pub use set::{AnalysisSet, AnalysisSetEntry};
pub use suite::{
    parse_suite, AssessmentCase, AssessmentSuite, Expectation, ExpectationStatus, SuiteError,
    ValidatedSuite, SUITE_SCHEMA, SUITE_SCHEMA_VERSION,
};
