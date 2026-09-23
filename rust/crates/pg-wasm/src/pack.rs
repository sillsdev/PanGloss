//! `.pgpack` load-time compatibility gate for this WASM Runtime build.
//!
//! This module replaces what would otherwise be a monolithic engine-compatibility-identifier
//! **equality** check with a load-time containment check: a pack's manifest stamps the
//! **required** runtime-feature set it was built against; this Runtime declares the **provided**
//! set it actually supports (`provided_runtime_features`); the pack loads iff
//! `required ⊆ provided` (`pg_pack::RequiredRuntimeFeatures::satisfied_by`, reused verbatim —
//! this module never reimplements the containment logic itself, only supplies this Runtime's own
//! `provided` side of it and the load-time call site). A pack that requires a feature this build
//! genuinely lacks is refused, with a typed `PackLoadError`, never a crash.
//!
//! `load_pack` also surfaces, at load time, the pack's `pg_pack::SignatureState` (reported for
//! the caller's information only; it never gates a load, exactly as `pg_pack::read_pack` itself
//! already guarantees). The FST-health admission field is `pg_health::health::HealthReport` reused
//! verbatim through `pg_pack::PackManifest::fst_health` — this module does not redefine, re-
//! derive, or duplicate that schema; see `LoadedPack::fst_health_admission`.
//!
//! # Analysis-only boundary
//! This module depends only on `pg_pack` and `pg_health` (both plain data types: manifest, compat,
//! and health/backend-report shapes). Neither is the FST compiler: this crate's own Cargo
//! dependency graph carries no `pg-foma` and no `foma` at all for the wasm32 target, checked by
//! `pg-wasm/tests/wasm_excludes_compiler.rs`'s `cargo metadata` walk rather than asserted in prose
//! here. This module performs zero FST/lexc compilation, links no compiler constructor, and never
//! calls an emit/compile entry point — the one thing it does is validate an already-compiled
//! artifact's manifest and report on it. It does not (yet) construct a working analyzer from the
//! packaged runtime/foma payload bytes; that is a separate, larger "WASM
//! analysis-only loading" scope (deserializing the Rust-HermitCrab runtime payload and
//! reconstructing the foma proposer from its existing binary-memory encoding via
//! `foma::io::fsm_read_binary_mem` — never recompiling it). This module is the load-time gate
//! that scope will sit behind.

use pg_pack::{
    PackManifest, PgPackError, ProvidedRuntimeFeatures, ReadPack, RequiredRuntimeFeatures,
    SignatureState,
};

/// This build's own required-runtime-feature vocabulary (only constructs needing a
/// runtime operation contribute — e.g. reduplication's query-time peel op). Freeform, stable,
/// delanguaged identifiers; this module does not mint a registry, it only names the ones this
/// Runtime build actually implements today.
pub use pg_health::runtime_features::RUNTIME_FEATURE_REDUPLICATION_PEEL as OP_REDUPLICATION_PEEL;

/// This Runtime build's own declared **provided** runtime-feature set (the other half of
/// the `required ⊆ provided` containment check) — never read from any `.pgpack` file, always
/// derived from this build itself:
///
/// - `payload_format_versions`: every `.pgpack` container framing version this build's
///   `pg_pack::read_pack` understands (currently just `pg_pack::CONTAINER_VERSION`).
/// - `runtime_operations`: stable operation identifiers this build's analysis pipeline actually
///   implements (today: `OP_REDUPLICATION_PEEL`, the compiler-side reduplication-peel identifier
///   `pg_health::runtime_features` declares so both sides read the same string).
/// - `foma_feature_level`/`hc_port_semver`: this build's own foma-feature level and this crate's
///   own semantic version (`CARGO_PKG_VERSION_*`, read at compile time) as the Rust-HermitCrab
///   port version.
/// - `extensions`: empty — no named optional extensions in this build yet.
pub fn provided_runtime_features() -> ProvidedRuntimeFeatures {
    ProvidedRuntimeFeatures {
        payload_format_versions: vec![pg_pack::CONTAINER_VERSION],
        runtime_operations: vec![OP_REDUPLICATION_PEEL.to_string()],
        foma_feature_level: FOMA_FEATURE_LEVEL,
        hc_port_semver: this_crate_semver(),
        extensions: Vec::new(),
    }
}

/// This build's own foma-feature level; bump only when this build gains a new foma-level capability a pack's manifest could legitimately require.
const FOMA_FEATURE_LEVEL: u32 = 1;

/// This crate's own `Cargo.toml` semantic version, read from the compile-time `CARGO_PKG_VERSION_*` vars, used as the declared `hc_port_semver`.
fn this_crate_semver() -> (u32, u32, u32) {
    const MAJOR: &str = env!("CARGO_PKG_VERSION_MAJOR");
    const MINOR: &str = env!("CARGO_PKG_VERSION_MINOR");
    const PATCH: &str = env!("CARGO_PKG_VERSION_PATCH");
    (
        MAJOR
            .parse()
            .expect("CARGO_PKG_VERSION_MAJOR is always numeric"),
        MINOR
            .parse()
            .expect("CARGO_PKG_VERSION_MINOR is always numeric"),
        PATCH
            .parse()
            .expect("CARGO_PKG_VERSION_PATCH is always numeric"),
    )
}

/// Every typed failure `load_pack` can return. Never a panic; a caller (native or, via
/// `PgPack`'s wasm-bindgen wrapper below, JS) always gets one of these back instead of a crash or
/// a silently-accepted incompatible pack.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PackLoadError {
    /// The container itself failed to parse or validate; see pg_pack::PgPackError. A structurally invalid package is reported here, before any runtime-feature compatibility check.
    #[error("pack container invalid: {0}")]
    Container(#[from] PgPackError),
    /// The pack's required_runtime_features is not a subset of this Runtime's provided set; carries both sides so a caller can report exactly what is missing. Boxed to keep the common Container variant small.
    #[error(
        "pack requires a runtime-feature set this Runtime build does not fully provide: \
         required={required:?} provided={provided:?}"
    )]
    IncompatibleRuntimeFeatures {
        required: Box<RequiredRuntimeFeatures>,
        provided: Box<ProvidedRuntimeFeatures>,
    },
}

/// A `.pgpack` that has passed both the container's own structural validation
/// (`pg_pack::read_pack`) and this Runtime's `required ⊆ provided` containment check.
/// Carries everything `load_pack`'s caller needs to surface the signature state and FST-health
/// admission alongside the raw parsed manifest and payload bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedPack {
    pub manifest: PackManifest,
    pub runtime_payload: Vec<u8>,
    pub foma_payload: Vec<u8>,
    /// Reported for the caller's information only — signature state never gates a load, so
    /// this is present on every `LoadedPack` regardless of its value, exactly as
    /// `pg_pack::ReadPack::signature_state` already guarantees at the container level.
    pub signature_state: SignatureState,
}

impl LoadedPack {
    /// The FST-health "admission result" (`pg_health::health::HealthReport::admission`,
    /// reused verbatim — this module never redefines or re-derives the health schema). It is the
    /// worst raw severity among the report's findings.
    pub fn fst_health_admission(&self) -> pg_health::health::Severity {
        self.manifest.fst_health.admission()
    }
}

/// Loads and validates one `.pgpack` container against this Runtime build's own provided
/// runtime-feature set: first the container's own structural validation
/// (`pg_pack::read_pack` — magic, version, section limits, truncation, trailing bytes, digest,
/// cross-payload fingerprint), then, only once that passes, the `required ⊆ provided`
/// containment check via `RequiredRuntimeFeatures::satisfied_by` against
/// `provided_runtime_features`. Fails closed with a typed `PackLoadError` at either stage;
/// never partially constructs a `LoadedPack`.
pub fn load_pack(bytes: &[u8]) -> Result<LoadedPack, PackLoadError> {
    let ReadPack {
        manifest,
        runtime_payload,
        foma_payload,
        signature_state,
    } = pg_pack::read_pack(bytes)?;

    let provided = provided_runtime_features();
    if !manifest.required_runtime_features.satisfied_by(&provided) {
        return Err(PackLoadError::IncompatibleRuntimeFeatures {
            required: Box::new(manifest.required_runtime_features),
            provided: Box::new(provided),
        });
    }

    Ok(LoadedPack {
        manifest,
        runtime_payload,
        foma_payload,
        signature_state,
    })
}

#[cfg(test)]
mod tests;
