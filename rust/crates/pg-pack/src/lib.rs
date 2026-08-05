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
//! capability-trust stamp (`trust::CapabilityTrust`), the FST-health admission
//! (`pg_foma::health::HealthReport`, reused verbatim, never redefined), an optional license
//! declaration (`license::LicenseDeclaration`), and an optional Ed25519 publisher signature
//! (`signature::SignatureBlock`) whose state (`signature::SignatureState`) is reported but
//! never gates a read.
//!
//! # What this crate is not (yet)
//! `pg-cli`'s `pangloss pack` subcommand (`pg-cli/src/pack.rs`) is a real producer: it constructs a
//! manifest from an actual compiled grammar and writes it via `format::write_pack`. `pg-wasm`
//! (`pg-wasm/src/pack.rs`) is a real load-time consumer of the manifest/compatibility/trust
//! sections. **The two payload sections are not both real yet, though.** The foma payload IS real
//! whenever the grammar's foma compile succeeds — `pg_foma::analyzer::FomaProposer::
//! foma_binary_payload` serializes the actually-compiled network via foma's own existing
//! `fsm_write_binary` (no second network format; see `format`'s module doc). The **runtime**
//! payload — the Rust-HermitCrab port's own analysis-time grammar data — is still an honestly-
//! labeled placeholder: `pg_grammar::model::Grammar` (what `pg-parse`/`pg-rules` actually analyze
//! against) derives `serde::Serialize` on almost none of its dozens of constituent types today, and
//! its `pg_featstruct::Interner<FeatureStruct>` field has no serde impl of its own either — making
//! it round-trip is a large, separate serialization effort this crate does not invent. Nothing
//! (yet) constructs a working *analyzer* from a `.pgpack`'s payload bytes — `pg-wasm`'s own module
//! doc names that as its separate, later "WASM analysis-only loading" scope.
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
//! dependency graph depends on this crate, and this crate is not yet depended on by anything (see
//! this doc's "What this crate is not" above).
#![forbid(unsafe_code)]

pub mod compat;
pub mod format;
pub mod license;
pub mod manifest;
pub mod signature;
pub mod trust;

pub use compat::{ProvidedRuntimeFeatures, RequiredRuntimeFeatures};
pub use format::{
    fingerprint_hex, limits_for_version, read_pack, write_pack, PgPackError, ReadPack,
    VersionLimits, CONTAINER_VERSION, MAGIC,
};
pub use license::{LicenseClass, LicenseDeclaration};
pub use manifest::{PackManifest, MANIFEST_FORMAT_TAG, MANIFEST_SCHEMA_VERSION};
pub use signature::{sign, verify, SignatureBlock, SignatureState};
pub use trust::{CapabilityOverrideRecord, CapabilityTrust, OverriddenConfig};
