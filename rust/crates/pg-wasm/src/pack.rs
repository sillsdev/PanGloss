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
//! already guarantees). The FST-health admission field is `pg_foma::health::HealthReport` reused
//! verbatim through `pg_pack::PackManifest::fst_health` — this module does not redefine, re-
//! derive, or duplicate that schema; see `LoadedPack::fst_health_admission`.
//!
//! # Analysis-only boundary
//! This module depends only on `pg_pack` (plain data types: manifest and compat) and reuses
//! `pg_foma::health` (also plain data). It performs zero FST/lexc compilation, links no compiler
//! constructor, and never calls `pg_foma::analyzer::FomaProposer::new` or any other emit/compile
//! entry point — the one thing it does is validate an already-compiled artifact's manifest and
//! report on it. It does not (yet) construct a working analyzer from the packaged runtime/foma
//! payload bytes; that is a separate, larger "WASM
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
///
/// **Re-exported from the producing side, deliberately not re-spelled.** This is the identifier
/// `pangloss pack` writes into a manifest's `required_runtime_features.runtime_operations` whenever a
/// grammar needs peeling, so the *provided* set here and the *required* set there MUST be the same
/// string or the `required ⊆ provided` check rejects a pack this Runtime can in fact serve.
/// This crate previously spelled it `"pg.reduplication.peel"` while the producer wrote
/// `"reduplication.peel"` — a latent load-rejection bug that only became reachable once both sides
/// existed. Aliasing the producer's constant makes the mismatch unrepresentable.
pub use pg_foma::peel::RUNTIME_FEATURE_REDUPLICATION_PEEL as OP_REDUPLICATION_PEEL;

/// This Runtime build's own declared **provided** runtime-feature set (the other half of
/// the `required ⊆ provided` containment check) — never read from any `.pgpack` file, always
/// derived from this build itself:
///
/// - `payload_format_versions`: every `.pgpack` container framing version this build's
///   `pg_pack::read_pack` understands (currently just `pg_pack::CONTAINER_VERSION`).
/// - `runtime_operations`: stable operation identifiers this build's analysis pipeline actually
///   implements (today: `OP_REDUPLICATION_PEEL`, backing `pg_foma::peel::ReduplicationPeeler`).
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
    /// The FST-health "admission result" (`pg_foma::health::HealthReport::admission`,
    /// reused verbatim — this module never redefines or re-derives the health schema). It is the
    /// worst raw severity among the report's findings.
    pub fn fst_health_admission(&self) -> pg_foma::health::Severity {
        self.manifest.fst_health.admission()
    }

    /// Whether `fst_health_admission` is below the tier that blocks publication
    /// (`Severity::NotProductionReady` or worse). Stable across a `Severity` rename or a new
    /// variant added below that tier, unlike a caller matching `fst_health_admission`'s spelling.
    pub fn fst_health_is_publishable(&self) -> bool {
        self.fst_health_admission() < pg_foma::health::Severity::NotProductionReady
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
mod tests {
    use super::*;
    use pg_foma::health::HealthReport;
    use pg_pack::LicenseDeclaration;

    fn synthetic_required(runtime_operations: Vec<String>) -> RequiredRuntimeFeatures {
        RequiredRuntimeFeatures {
            payload_format_version: pg_pack::CONTAINER_VERSION,
            runtime_operations,
            foma_feature_level: FOMA_FEATURE_LEVEL,
            hc_port_semver: this_crate_semver(),
            extensions: Vec::new(),
        }
    }

    fn synthetic_manifest(
        required_runtime_features: RequiredRuntimeFeatures,
        runtime_payload: &[u8],
        foma_payload: &[u8],
    ) -> PackManifest {
        PackManifest {
            manifest_schema_version: pg_pack::MANIFEST_SCHEMA_VERSION,
            grammar_id: "synthetic-wasm-wiring-grammar".to_string(),
            package_fingerprint: pg_pack::fingerprint_hex(runtime_payload, foma_payload),
            required_runtime_features,
            fst_health: HealthReport::new(Vec::new()),
            backend_assessments: vec![],
            license: None::<LicenseDeclaration>,
            created_by: "synthetic-test-builder".to_string(),
            created_at: "2026-07-25T00:00:00Z".to_string(),
            signature: None,
        }
    }

    const RUNTIME_PAYLOAD: &[u8] = b"synthetic-rust-hermitcrab-runtime-payload-bytes";
    const FOMA_PAYLOAD: &[u8] = b"synthetic-opaque-foma-binary-memory-payload-bytes";

    #[test]
    fn pack_whose_required_features_are_a_subset_of_provided_loads() {
        let manifest = synthetic_manifest(
            synthetic_required(vec![OP_REDUPLICATION_PEEL.to_string()]),
            RUNTIME_PAYLOAD,
            FOMA_PAYLOAD,
        );
        let bytes = pg_pack::write_pack(&manifest, RUNTIME_PAYLOAD, FOMA_PAYLOAD).unwrap();

        let loaded = load_pack(&bytes).expect("required ⊆ provided must load");
        assert_eq!(loaded.manifest, manifest);
        assert_eq!(loaded.runtime_payload, RUNTIME_PAYLOAD);
        assert_eq!(loaded.foma_payload, FOMA_PAYLOAD);
        assert_eq!(loaded.signature_state, SignatureState::Unsigned);
        assert_eq!(
            loaded.fst_health_admission(),
            pg_foma::health::Severity::WithinLimits
        );
    }

    #[test]
    fn pack_requiring_an_unprovided_runtime_operation_is_rejected_with_typed_diagnostic() {
        let manifest = synthetic_manifest(
            synthetic_required(vec!["pg.brand-new.unimplemented-op".to_string()]),
            RUNTIME_PAYLOAD,
            FOMA_PAYLOAD,
        );
        let bytes = pg_pack::write_pack(&manifest, RUNTIME_PAYLOAD, FOMA_PAYLOAD).unwrap();

        let err = load_pack(&bytes).expect_err("an unprovided runtime operation must be refused");
        match err {
            PackLoadError::IncompatibleRuntimeFeatures { required, provided } => {
                assert!(required
                    .runtime_operations
                    .contains(&"pg.brand-new.unimplemented-op".to_string()));
                assert!(!provided
                    .runtime_operations
                    .contains(&"pg.brand-new.unimplemented-op".to_string()));
            }
            other => panic!("expected IncompatibleRuntimeFeatures, got {other:?}"),
        }
    }

    #[test]
    fn pack_requiring_a_newer_hc_port_semver_than_this_build_provides_is_rejected() {
        let mut required = synthetic_required(Vec::new());
        required.hc_port_semver = (
            this_crate_semver().0,
            this_crate_semver().1 + 1,
            this_crate_semver().2,
        );
        let manifest = synthetic_manifest(
            required,
            RUNTIME_PAYLOAD,
            FOMA_PAYLOAD,
        );
        let bytes = pg_pack::write_pack(&manifest, RUNTIME_PAYLOAD, FOMA_PAYLOAD).unwrap();
        assert!(matches!(
            load_pack(&bytes),
            Err(PackLoadError::IncompatibleRuntimeFeatures { .. })
        ));
    }

    #[test]
    fn a_malformed_container_never_reaches_the_containment_check() {
        let mut bytes = b"not a real pgpack container at all, far too short".to_vec();
        bytes.truncate(4);
        assert!(matches!(
            load_pack(&bytes),
            Err(PackLoadError::Container(_))
        ));
    }

    // Signature state is reported, never gates: it plays no role in the containment decision.
    #[test]
    fn signature_state_is_independent_of_runtime_feature_compatibility() {
        let manifest = synthetic_manifest(
            synthetic_required(vec![OP_REDUPLICATION_PEEL.to_string()]),
            RUNTIME_PAYLOAD,
            FOMA_PAYLOAD,
        );
        let manifest_no_sig_json = manifest.to_canonical_json();
        let message = pg_pack::sign(&[3u8; 32], &manifest_no_sig_json.into_bytes(), None);
        // Deliberately signed over the wrong bytes, so this reports `Invalid` and still loads.
        let mut manifest = manifest;
        manifest.signature = Some(message);
        let bytes = pg_pack::write_pack(&manifest, RUNTIME_PAYLOAD, FOMA_PAYLOAD).unwrap();

        let loaded = load_pack(&bytes).expect("an invalid signature must not block loading");
        assert_eq!(loaded.signature_state, SignatureState::Invalid);
    }

    #[test]
    fn provided_runtime_features_declares_this_containers_own_version() {
        let provided = provided_runtime_features();
        assert!(provided
            .payload_format_versions
            .contains(&pg_pack::CONTAINER_VERSION));
        assert!(provided
            .runtime_operations
            .contains(&OP_REDUPLICATION_PEEL.to_string()));
    }
}
