//! `pg-pack`: the `.pgpack` PanGloss Language Pack container format —
//! `openspec/changes/IMPLEMENTATION-READINESS.md` **R2A**, `docs/adr/0004-runtime-feature-
//! compatibility.md`, `docs/adr/0005-capability-override-unproven-grammars.md`, and
//! `openspec/changes/make-wasm-analysis-only/` (design.md/spec.md own the manifest's compat field
//! names this crate aligns with).
//!
//! # What this crate is
//! The self-contained analysis artifact format: fixed PanGloss magic bytes, an integer container
//! version, a length-prefixed canonical JSON **pack manifest** ([`manifest::PackManifest`]), a
//! length-prefixed Rust-HermitCrab runtime payload, a length-prefixed existing-foma binary payload
//! (opaque, unchanged encoding — see [`format`]'s module doc), and a trailing SHA-256 digest. See
//! [`format`] for the exact byte layout, [`format::write_pack`]/[`format::read_pack`] for the
//! writer/reader, and [`manifest::PackManifest`] for every field the manifest carries: the ADR
//! 0004 required-runtime-feature set ([`compat::RequiredRuntimeFeatures`]), the ADR 0005
//! capability-trust stamp ([`trust::CapabilityTrust`]), the FST-health admission
//! (`pg_foma::health::HealthReport`, reused verbatim, never redefined), an optional license
//! declaration ([`license::LicenseDeclaration`]), and an optional Ed25519 publisher signature
//! ([`signature::SignatureBlock`]) whose state ([`signature::SignatureState`]) is reported but
//! never gates a read.
//!
//! # What this crate is not (yet)
//! **Purely additive.** Nothing in `pg-cli`, `pg-wasm`, or any production compile/analysis path
//! constructs, writes, or reads a `.pgpack` file yet — this crate defines the format, a writer, a
//! reader, and its own tests only, the same "define the data type, wire it up later" shape
//! `pg_foma::health`/`pg_foma::plan`/`pg_foma::capability` used for their own Step 1s. Wiring a
//! real compiler pass to produce the runtime/foma payload bytes, and wiring `pg-cli`/`pg-wasm` to
//! consume them, is later work.
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
