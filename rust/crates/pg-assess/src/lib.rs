//! The grammar-assessment evidence layer: structured analysis identity, canonical JSON, and the
//! digests every assessment artifact is identified by.
//!
//! This crate is the identity and digest foundation the four operations (`assess`, `compare`,
//! `golden-diff`, `investigate`) are built on. It deliberately knows nothing about suites,
//! reports, or the CLI.
//!
//! The one idea everything here rests on: **an analysis identity is a value, not a reference**
//! (ADR 0006). It carries stable source keys rather than the dense compiler-assigned ordinals
//! `pg_parse::WordAnalysis` uses internally, so a grammar edit that deletes or renames a morpheme
//! produces ordinary `added`/`removed` evidence instead of a comparison failure — and a report
//! stays readable years later, when neither grammar still compiles.

pub mod delta;
pub mod digest;
pub mod golden;
pub mod handoff;
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
pub use golden::{
    golden_diff, GoldenCase, GoldenError, GoldenSetDiff, NotAdjudicatedReason, NotEvaluableReason,
    Verdict, GOLDEN_SCHEMA, GOLDEN_SCHEMA_VERSION,
};
pub use handoff::{
    investigate, ConstructRef, EngineCaveat, Evidence, EvidenceAvailability, HandoffError,
    HandoffRequest, InvestigationHandoff, MissingAnalysis, MissingAnalysisCause, NarrativeStep,
    SourceIdKind, HANDOFF_SCHEMA, HANDOFF_SCHEMA_VERSION,
};
pub use jcs::{canonicalize, JcsError};
pub use model::{model_fingerprint, source_sha256, SourceKind, MODEL_PROJECTION};
pub use outcome::{
    derive_status, AssessmentStatus, BudgetDimension, CaseOutcome, IncompleteReason,
    NotAttemptedReason,
};
/// The identity projection, re-exported from its owning crate.
///
/// Owned by `pg-parse` rather than this crate: it projects `pg_parse::WordAnalysis`, so putting
/// it there lets `pg-foma` express the backend parity relation with the SAME projector instead of
/// either depending on this crate (the engine depending on the assessment layer is backwards) or
/// forking the logic (two definitions of "the same analysis", free to drift). This re-export keeps
/// `pg_assess::identity::*` and `crate::identity::*` working unchanged inside this crate.
pub use pg_parse::identity;
pub use pg_parse::identity::{AnalysisIdentity, IdentityError, MorphemeKey, IDENTITY_PROFILE};
pub use report::{
    parse_report, AssessmentFailure, AssessmentReport, CaseRecord, Diagnostic, Execution,
    FailureKind, Provenance, ReportDraft, ReportError, Severity, SuiteRef, REPORT_SCHEMA,
    REPORT_SCHEMA_VERSION,
};
pub use set::{AnalysisSet, AnalysisSetEntry};
pub use suite::{
    parse_suite, AssessmentCase, AssessmentSuite, Expectation, ExpectationStatus, SuiteError,
    ValidatedSuite, SUITE_SCHEMA, SUITE_SCHEMA_VERSION,
};
