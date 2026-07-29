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

pub mod digest;
pub mod identity;
pub mod jcs;
pub mod model;
pub mod set;

pub use digest::{
    digest_projection, identity_digest, sha256_bytes, OUTCOME_PROJECTION, SEMANTIC_PROJECTION,
};
pub use identity::{AnalysisIdentity, IdentityError, MorphemeKey, IDENTITY_PROFILE};
pub use jcs::{canonicalize, JcsError};
pub use model::{model_fingerprint, source_sha256, SourceKind, MODEL_PROJECTION};
pub use set::{AnalysisSet, AnalysisSetEntry};
