//! Load-time compatibility: the pack
//! manifest's **required runtime-feature set** and the Runtime's own **provided** set. "The pack
//! loads iff `required ⊆ provided`" — this module defines both sides of that containment check as
//! plain data plus a pure comparison function, for a `pg-wasm`/`pg-cli` load path to call
//! `RequiredRuntimeFeatures::satisfied_by` against a Runtime-declared `ProvidedRuntimeFeatures`
//! before constructing an analyzer.
//!
//! Five stamped inputs: payload-format version, the runtime *operations* its
//! execution needs, foma-feature level, HC-port semantic version, extensions. Each becomes a
//! field below. `runtime_operations`/`extensions` are open, append-only string sets --
//! the provided set is append-only, so a new foma/HC capability only ever appends; `hc_port_semver` is
//! compared as a simple `(major, minor, patch)` tuple ordering (no pre-release/build-metadata
//! parsing — this schema-only step does not pull in a full semver dependency for one comparison).

use serde::{Deserialize, Serialize};

/// The required-runtime-feature set a pack was compiled against. Carried verbatim in
/// `crate::manifest::PackManifest::required_runtime_features`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RequiredRuntimeFeatures {
    /// This container's own payload-format version (mirrors `crate::format::CONTAINER_VERSION`
    /// at the time the pack was written; kept as a plain field rather than re-derived so a
    /// manifest is meaningful even read out of its container framing).
    pub payload_format_version: u32,
    /// Stable runtime-operation identifiers this pack's execution needs (only
    /// constructs needing a runtime operation contribute — e.g. a reduplication peel op).
    /// Freeform stable strings; this schema step does not mint an operation-identifier registry.
    pub runtime_operations: Vec<String>,
    /// The foma-feature level this pack's proposer payload assumes.
    pub foma_feature_level: u32,
    /// The Rust-HermitCrab port's semantic version this pack's runtime payload assumes, as
    /// `(major, minor, patch)`.
    pub hc_port_semver: (u32, u32, u32),
    /// Additional named extensions beyond the fixed fields above — append-only, freeform.
    pub extensions: Vec<String>,
}

/// The Runtime's own declared **provided** set (the other half of the containment check).
/// Never serialized into a pack — a Runtime constructs this from its own build, not from any
/// `.pgpack` file.
#[derive(Debug, Clone, PartialEq)]
pub struct ProvidedRuntimeFeatures {
    pub payload_format_versions: Vec<u32>,
    pub runtime_operations: Vec<String>,
    pub foma_feature_level: u32,
    pub hc_port_semver: (u32, u32, u32),
    pub extensions: Vec<String>,
}

impl RequiredRuntimeFeatures {
    /// The containment check: `required ⊆ provided`. `true` iff every dimension this pack
    /// requires is present in `provided` — the Runtime's declared payload-format versions include
    /// this pack's, `provided`'s foma-feature level is at least this pack's, `provided`'s HC-port
    /// semver is at least this pack's, and every required runtime operation/extension string is
    /// present in `provided`'s corresponding list. A pure function; callers decide what to do with
    /// `false` (a typed, allowed incompatibility — never a crash).
    pub fn satisfied_by(&self, provided: &ProvidedRuntimeFeatures) -> bool {
        provided
            .payload_format_versions
            .contains(&self.payload_format_version)
            && provided.foma_feature_level >= self.foma_feature_level
            && provided.hc_port_semver >= self.hc_port_semver
            && self
                .runtime_operations
                .iter()
                .all(|op| provided.runtime_operations.iter().any(|p| p == op))
            && self
                .extensions
                .iter()
                .all(|ext| provided.extensions.iter().any(|p| p == ext))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_required() -> RequiredRuntimeFeatures {
        RequiredRuntimeFeatures {
            payload_format_version: 1,
            runtime_operations: vec!["synthetic.reduplication.peel".to_string()],
            foma_feature_level: 2,
            hc_port_semver: (1, 3, 0),
            extensions: vec!["synthetic.extension.alpha".to_string()],
        }
    }

    fn synthetic_provided_superset() -> ProvidedRuntimeFeatures {
        ProvidedRuntimeFeatures {
            payload_format_versions: vec![1, 2],
            runtime_operations: vec![
                "synthetic.reduplication.peel".to_string(),
                "synthetic.other.op".to_string(),
            ],
            foma_feature_level: 3,
            hc_port_semver: (1, 4, 0),
            extensions: vec![
                "synthetic.extension.alpha".to_string(),
                "synthetic.extension.beta".to_string(),
            ],
        }
    }

    #[test]
    fn required_subset_of_provided_is_satisfied() {
        assert!(synthetic_required().satisfied_by(&synthetic_provided_superset()));
    }

    #[test]
    fn missing_runtime_operation_is_not_satisfied() {
        let required = synthetic_required();
        let provided = ProvidedRuntimeFeatures {
            runtime_operations: vec!["synthetic.unrelated.op".to_string()],
            ..synthetic_provided_superset()
        };
        assert!(!required.satisfied_by(&provided));
    }

    #[test]
    fn missing_extension_is_not_satisfied() {
        let required = synthetic_required();
        let provided = ProvidedRuntimeFeatures {
            extensions: vec![],
            ..synthetic_provided_superset()
        };
        assert!(!required.satisfied_by(&provided));
    }

    #[test]
    fn lower_foma_feature_level_is_not_satisfied() {
        let required = synthetic_required();
        let provided = ProvidedRuntimeFeatures {
            foma_feature_level: 1,
            ..synthetic_provided_superset()
        };
        assert!(!required.satisfied_by(&provided));
    }

    #[test]
    fn older_hc_port_semver_is_not_satisfied() {
        let required = synthetic_required();
        let provided = ProvidedRuntimeFeatures {
            hc_port_semver: (1, 2, 9),
            ..synthetic_provided_superset()
        };
        assert!(!required.satisfied_by(&provided));
    }

    #[test]
    fn missing_payload_format_version_is_not_satisfied() {
        let required = synthetic_required();
        let provided = ProvidedRuntimeFeatures {
            payload_format_versions: vec![2, 3],
            ..synthetic_provided_superset()
        };
        assert!(!required.satisfied_by(&provided));
    }
}
