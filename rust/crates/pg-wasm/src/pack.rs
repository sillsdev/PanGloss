//! `.pgpack` load-time compatibility gate for this WASM Runtime build.
//!
//! This module replaces what would otherwise be a monolithic engine-compatibility-identifier
//! **equality** check with a load-time containment check: a pack's manifest stamps the
//! **required** runtime-feature set it was built against; this Runtime declares the **provided**
//! set it actually supports ([`provided_runtime_features`]); the pack loads iff
//! `required ⊆ provided` ([`pg_pack::RequiredRuntimeFeatures::satisfied_by`], reused verbatim —
//! this module never reimplements the containment logic itself, only supplies this Runtime's own
//! `provided` side of it and the load-time call site). Because `provided` is append-only, an old
//! pack keeps loading on every newer build of this Runtime unchanged; only a pack that requires a
//! feature this build genuinely lacks is refused, with a typed [`PackLoadError`], never a crash
//! and never a version-equality mismatch.
//!
//! [`load_pack`] also surfaces, at load time, the two other signals the pack manifest
//! carries: the [`pg_pack::CapabilityTrust`] stamp (a pack force-compiled past a
//! characteristics-check refusal is indelibly `Overridden`/unproven, and still loads — see
//! [`LoadedPack::is_unproven`] — the degraded-trust *signal*, not a refusal, is the safety
//! mechanism) and the pack's [`pg_pack::SignatureState`] (reported for the caller's information
//! only; it never gates a load, exactly as [`pg_pack::read_pack`] itself already
//! guarantees). The FST-health admission field is [`pg_foma::health::HealthReport`] reused
//! verbatim through [`pg_pack::PackManifest::fst_health`] — this module does not redefine, re-
//! derive, or duplicate that schema; see [`LoadedPack::fst_health_admission`].
//!
//! # Analysis-only boundary
//! This module depends only on `pg_pack` (plain data types: manifest, compat, trust) and reuses
//! `pg_foma::health` (also plain data). It performs zero FST/lexc compilation, links no compiler
//! constructor, and never calls `pg_foma::analyzer::FomaProposer::new` or any other emit/compile
//! entry point — the one thing it does is validate an already-compiled artifact's envelope and
//! report on it. It does not (yet) construct a working analyzer from the packaged runtime/foma
//! payload bytes; that is a separate, larger "WASM
//! analysis-only loading" scope (deserializing the Rust-HermitCrab runtime payload and
//! reconstructing the foma proposer from its existing binary-memory encoding via
//! `foma::io::fsm_read_binary_mem` — never recompiling it). This module is the load-time gate
//! that scope will sit behind.

use pg_pack::{
    CapabilityTrust, PackManifest, PgPackError, ProvidedRuntimeFeatures, ReadPack,
    RequiredRuntimeFeatures, SignatureState,
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
///   [`pg_pack::read_pack`] understands (currently just [`pg_pack::CONTAINER_VERSION`]).
/// - `runtime_operations`: stable operation identifiers this build's analysis pipeline actually
///   implements (today: [`OP_REDUPLICATION_PEEL`], backing `pg_foma::peel::ReduplicationPeeler`).
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

/// This build's own foma-feature level. A plain constant (not derived from any external registry)
/// because this dimension is this Runtime's own compile-time capability declaration
/// — bump it only when this build gains a new foma-level capability a pack's
/// `required_runtime_features.foma_feature_level` could legitimately require.
const FOMA_FEATURE_LEVEL: u32 = 1;

/// This crate's own `Cargo.toml` semantic version, read from the compile-time `CARGO_PKG_VERSION_*`
/// environment variables `cargo` always sets — used as the Rust-HermitCrab port version this
/// Runtime build declares it provides (the `hc_port_semver` dimension). Every workspace
/// crate shares one `version.workspace = true` value, so this is the same number `pg-parse`/
/// `pg-foma` themselves ship at.
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

/// Every typed failure [`load_pack`] can return. Never a panic; a caller (native or, via
/// `PgPack`'s wasm-bindgen wrapper below, JS) always gets one of these back instead of a crash or
/// a silently-accepted incompatible pack.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PackLoadError {
    /// The container itself failed to parse/validate (bad magic, unsupported version, oversize or
    /// truncated section, digest or fingerprint mismatch, ...) — see [`pg_pack::PgPackError`].
    /// Independent of runtime-feature compatibility: a structurally invalid package never reaches
    /// the containment check at all.
    #[error("pack container invalid: {0}")]
    Container(#[from] PgPackError),
    /// The allowed, typed incompatibility: the pack's `required_runtime_features` is not a
    /// subset of this Runtime's `provided` set. Carries both sides so a caller can report exactly
    /// what is missing (e.g. "upgrade PanGloss to run this grammar") rather than a bare boolean.
    /// Boxed (clippy `result_large_err`): both feature-set structs carry several `Vec<String>`
    /// fields, which would otherwise make every [`PackLoadError`] as large as this, the biggest,
    /// variant -- even the common `Container` case.
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
/// ([`pg_pack::read_pack`]) and this Runtime's `required ⊆ provided` containment check.
/// Carries everything [`load_pack`]'s caller needs to surface the trust signal and the
/// FST-health admission alongside the raw parsed manifest and payload bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedPack {
    pub manifest: PackManifest,
    pub runtime_payload: Vec<u8>,
    pub foma_payload: Vec<u8>,
    /// Reported for the caller's information only — signature state never gates a load, so
    /// this is present on every [`LoadedPack`] regardless of its value, exactly as
    /// [`pg_pack::ReadPack::signature_state`] already guarantees at the container level.
    pub signature_state: SignatureState,
}

impl LoadedPack {
    /// The pack-level degraded-trust signal: `true` iff this pack was force-compiled past
    /// a characteristics-check refusal via the capability override
    /// ([`pg_pack::CapabilityTrust::Overridden`]). A consuming application keys its "this is
    /// potentially broken" banner off this at load time. See [`LoadedPack::analysis_trust_flag`]
    /// for the same signal reused as the per-analysis-result flag.
    pub fn is_unproven(&self) -> bool {
        self.manifest.capability_trust.is_unproven()
    }

    /// The per-analysis-result degraded/experimental flag this pack's "two-level" trust signal
    /// names: at load, the pack reports pack-level `unproven`/`overridden` status; on every
    /// analysis, each result carries a degraded/experimental flag. Identical truth value to
    /// [`LoadedPack::is_unproven`] today — a pack's trust stamp is a single pack-wide fact, so
    /// every analysis drawn from the same pack necessarily carries the same flag — kept as its own
    /// named accessor so the eventual per-word analysis result type (not yet wired)
    /// has one obvious call to copy onto itself rather than reaching into `manifest` directly.
    pub fn analysis_trust_flag(&self) -> bool {
        self.is_unproven()
    }

    /// The override record when [`LoadedPack::is_unproven`] is `true` — who authorized
    /// the override, why, and exactly which fail-closed configurations were force-compiled through
    /// (`None` for a cleanly [`pg_pack::CapabilityTrust::Proven`] pack).
    pub fn override_record(&self) -> Option<&pg_pack::CapabilityOverrideRecord> {
        match &self.manifest.capability_trust {
            CapabilityTrust::Proven => None,
            CapabilityTrust::Overridden(record) => Some(record),
        }
    }

    /// The FST-health "admission result" (`pg_foma::health::HealthReport::admission`,
    /// reused verbatim — this module never redefines or re-derives the health schema). The worst non-overridden severity
    /// among this pack's FST-health findings; [`pg_foma::health::Severity::Ideal`] for an empty or
    /// fully-overridden report.
    pub fn fst_health_admission(&self) -> pg_foma::health::Severity {
        self.manifest.fst_health.admission()
    }
}

/// Loads and validates one `.pgpack` container against this Runtime build's own provided
/// runtime-feature set: first the container's own structural validation
/// ([`pg_pack::read_pack`] — magic, version, section limits, truncation, trailing bytes, digest,
/// cross-payload fingerprint), then, only once that passes, the `required ⊆ provided`
/// containment check via [`RequiredRuntimeFeatures::satisfied_by`] against
/// [`provided_runtime_features`]. Fails closed with a typed [`PackLoadError`] at either stage;
/// never partially constructs a [`LoadedPack`].
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
    use pg_pack::{CapabilityOverrideRecord, LicenseDeclaration, OverriddenConfig};

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
        capability_trust: CapabilityTrust,
        runtime_payload: &[u8],
        foma_payload: &[u8],
    ) -> PackManifest {
        PackManifest {
            format: pg_pack::MANIFEST_FORMAT_TAG.to_string(),
            manifest_schema_version: pg_pack::MANIFEST_SCHEMA_VERSION,
            grammar_id: "synthetic-wasm-wiring-grammar".to_string(),
            package_fingerprint: pg_pack::fingerprint_hex(runtime_payload, foma_payload),
            required_runtime_features,
            capability_trust,
            fst_health: HealthReport::new(Vec::new()),
            license: None::<LicenseDeclaration>,
            created_by: "synthetic-test-builder".to_string(),
            created_at: "2026-07-25T00:00:00Z".to_string(),
            signature: None,
        }
    }

    const RUNTIME_PAYLOAD: &[u8] = b"synthetic-rust-hermitcrab-runtime-payload-bytes";
    const FOMA_PAYLOAD: &[u8] = b"synthetic-opaque-foma-binary-memory-payload-bytes";

    // ---------------------------------------------------------------------------------------
    // Deliverable test 1: required ⊆ provided loads.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn pack_whose_required_features_are_a_subset_of_provided_loads() {
        let manifest = synthetic_manifest(
            synthetic_required(vec![OP_REDUPLICATION_PEEL.to_string()]),
            CapabilityTrust::Proven,
            RUNTIME_PAYLOAD,
            FOMA_PAYLOAD,
        );
        let bytes = pg_pack::write_pack(&manifest, RUNTIME_PAYLOAD, FOMA_PAYLOAD).unwrap();

        let loaded = load_pack(&bytes).expect("required ⊆ provided must load");
        assert_eq!(loaded.manifest, manifest);
        assert_eq!(loaded.runtime_payload, RUNTIME_PAYLOAD);
        assert_eq!(loaded.foma_payload, FOMA_PAYLOAD);
        assert!(!loaded.is_unproven());
        assert!(!loaded.analysis_trust_flag());
        assert_eq!(loaded.signature_state, SignatureState::Unsigned);
        assert_eq!(
            loaded.fst_health_admission(),
            pg_foma::health::Severity::Ideal
        );
    }

    #[test]
    fn old_pack_with_no_extra_requirements_keeps_loading_append_only() {
        // Old packs run unchanged forever -- a pack requiring nothing beyond this
        // build's baseline must load exactly like a fully-populated one.
        let manifest = synthetic_manifest(
            synthetic_required(Vec::new()),
            CapabilityTrust::Proven,
            RUNTIME_PAYLOAD,
            FOMA_PAYLOAD,
        );
        let bytes = pg_pack::write_pack(&manifest, RUNTIME_PAYLOAD, FOMA_PAYLOAD).unwrap();
        assert!(load_pack(&bytes).is_ok());
    }

    // ---------------------------------------------------------------------------------------
    // Deliverable test 2: a pack requiring an unprovided feature is rejected with a typed
    // diagnostic, not a crash and not a bare equality mismatch.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn pack_requiring_an_unprovided_runtime_operation_is_rejected_with_typed_diagnostic() {
        let manifest = synthetic_manifest(
            synthetic_required(vec!["pg.brand-new.unimplemented-op".to_string()]),
            CapabilityTrust::Proven,
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
            CapabilityTrust::Proven,
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

    // ---------------------------------------------------------------------------------------
    // Deliverable test 3: an unproven/overridden pack loads WITH the degraded-trust signal.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn unproven_overridden_pack_loads_with_degraded_trust_signal() {
        let override_record = CapabilityOverrideRecord {
            authorized_by: "synthetic-test-operator".to_string(),
            reason: "synthetic field-trial override".to_string(),
            recorded_at: "2026-07-25T00:00:00Z".to_string(),
            overridden_configs: vec![OverriddenConfig {
                predicate: "synthetic.simultaneous.subrule-overlap".to_string(),
                construct: "mrule:synthetic-0001".to_string(),
                witness: "synthetic-witness-form".to_string(),
            }],
        };
        let manifest = synthetic_manifest(
            synthetic_required(Vec::new()),
            CapabilityTrust::Overridden(override_record.clone()),
            RUNTIME_PAYLOAD,
            FOMA_PAYLOAD,
        );
        let bytes = pg_pack::write_pack(&manifest, RUNTIME_PAYLOAD, FOMA_PAYLOAD).unwrap();

        let loaded =
            load_pack(&bytes).expect("an unproven pack still loads -- signal, not refusal");
        assert!(
            loaded.is_unproven(),
            "pack-level degraded-trust signal must fire"
        );
        assert!(
            loaded.analysis_trust_flag(),
            "the same signal must be available as the per-analysis-result flag"
        );
        assert_eq!(loaded.override_record(), Some(&override_record));
    }

    #[test]
    fn proven_pack_carries_no_override_record() {
        let manifest = synthetic_manifest(
            synthetic_required(Vec::new()),
            CapabilityTrust::Proven,
            RUNTIME_PAYLOAD,
            FOMA_PAYLOAD,
        );
        let bytes = pg_pack::write_pack(&manifest, RUNTIME_PAYLOAD, FOMA_PAYLOAD).unwrap();
        let loaded = load_pack(&bytes).unwrap();
        assert!(!loaded.is_unproven());
        assert_eq!(loaded.override_record(), None);
    }

    // ---------------------------------------------------------------------------------------
    // Signature state is reported, never gates -- even paired with an incompatible feature set,
    // signing/signature validity plays no role in the containment decision.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn signature_state_is_independent_of_runtime_feature_compatibility() {
        let manifest = synthetic_manifest(
            synthetic_required(vec![OP_REDUPLICATION_PEEL.to_string()]),
            CapabilityTrust::Proven,
            RUNTIME_PAYLOAD,
            FOMA_PAYLOAD,
        );
        let manifest_no_sig_json = manifest.to_canonical_json();
        let message = pg_pack::sign(&[3u8; 32], &manifest_no_sig_json.into_bytes(), None);
        // Deliberately signed over the WRONG bytes (not the real domain-separated message) so
        // this reports `Invalid` -- proving an invalid signature still loads successfully.
        let mut manifest = manifest;
        manifest.signature = Some(message);
        let bytes = pg_pack::write_pack(&manifest, RUNTIME_PAYLOAD, FOMA_PAYLOAD).unwrap();

        let loaded = load_pack(&bytes).expect("an invalid signature must not block loading");
        assert_eq!(loaded.signature_state, SignatureState::Invalid);
    }

    // ---------------------------------------------------------------------------------------
    // This Runtime's own provided-set shape, sanity-checked.
    // ---------------------------------------------------------------------------------------

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
