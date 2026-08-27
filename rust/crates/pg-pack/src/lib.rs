//! `pg-pack`: the `.pgpack` PanGloss Language Pack container format.
//!
//! # What this crate is
//! The self-contained analysis artifact format: fixed PanGloss magic bytes, an integer container
//! version, a length-prefixed canonical JSON **pack manifest** (`manifest::PackManifest`), a
//! length-prefixed Rust-HermitCrab runtime payload, a length-prefixed existing-foma binary payload
//! (opaque, unchanged encoding — see `format`'s module doc), and a trailing SHA-256 digest. See
//! `format` for the exact byte layout, `format::write_pack`/`format::read_pack` for the
//! writer/reader, and `manifest::PackManifest` for every field the manifest carries: the
//! required-runtime-feature set (`compat::RequiredRuntimeFeatures`), the
//! FST-health admission (`pg_foma::health::HealthReport`, reused verbatim, never redefined), an optional license
//! declaration (`license::LicenseDeclaration`), and an optional Ed25519 publisher signature
//! (`signature::SignatureBlock`) whose state (`signature::SignatureState`) is reported but
//! never gates a read.
//!
//! # Placement (why a new crate, not an extension of `pg-snapshot`)
//! `pg-snapshot` is the **pre-compile** interchange format: "the interchange contract between
//! `pg-fwdata` ... and `pg_grammar::compile`" (see that crate's own module doc) — a plain JSON
//! serialization of *project source data* with no notion of a compiled FST, a runtime payload, a
//! health report, or a signature. `.pgpack` is the opposite end of the pipeline: a **post-compile**
//! distributable artifact bundling two opaque compiled-payload blobs plus provenance/compatibility
//! metadata about that compilation. Extending `pg-snapshot` would conflate two different artifacts
//! that serve different consumers (`pg-fwdata`→`pg_grammar::compile` vs. a distributed Language
//! Pack loaded by a Runtime) and would force a dependency edge from `pg-snapshot` onto
//! `pg_foma::health` (needed to reuse `HealthReport`) that `pg-snapshot` — deliberately one of the
//! most upstream, dependency-light crates in this workspace (only `serde`/`serde_json`/`thiserror`
//! today) — has no other reason to carry. A new crate depending on `pg-foma` (for `health`) keeps
//! that dependency where it is actually needed and introduces no cycle: nothing in `pg-foma`'s own
//! dependency graph depends on this crate.
#![forbid(unsafe_code)]

pub mod compat;
pub mod format;
pub mod license;
pub mod manifest;
pub mod signature;

pub use compat::{ProvidedRuntimeFeatures, RequiredRuntimeFeatures};
pub use format::{
    fingerprint_hex, limits_for_version, read_pack, write_pack, PgPackError, ReadPack,
    VersionLimits, CONTAINER_VERSION, MAGIC,
};
pub use license::{LicenseClass, LicenseDeclaration};
pub use manifest::{
    BackendAdviceReference, BackendAssessment, BackendCostEvidence, PackManifest,
    MANIFEST_SCHEMA_VERSION,
};
pub use signature::{sign, verify, SignatureBlock, SignatureState};
