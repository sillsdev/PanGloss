//! Capability-trust stamp: who/when/why/which fail-closed configurations were overridden,
//! written into the pack manifest capability-trust record rather than inventing a parallel one.
//! The trust axis is
//! binary and kept separate from the cost/health axis `pg_foma::health` owns. So this module's
//! `CapabilityTrust` is its own
//! distinct manifest field (see `crate::manifest::PackManifest::capability_trust`), not a
//! per-finding health field; this module's `CapabilityOverrideRecord` is the pack-level
//! correctness-trust override. Distinct fields per axis: reuse the artifact, not the field.
//!
//! This is the **persistent** home for capability-override state: `rust/crates/pg-cli/src/main.rs`'s
//! `GateResult::overridden` is scoped to one CLI invocation, while this module's
//! `CapabilityOverrideRecord` is the indelible serialized record. Once
//! written into a pack manifest and the pack is distributed, the record travels with the pack
//! forever -- the stamp is indelible and cannot be removed by a consumer.
//!
//! `predicate`/`construct`/`witness` on `OverriddenConfig` deliberately mirror the diagnostic
//! shape `pg_foma::capability::CompileDecision::Refuse`'s diagnostics already use (see
//! `pg-cli`'s `capability_gate` — "predicate=... construct=... witness=..." lines) so the same
//! vocabulary describes an override at the CLI-report level and at the persistent pack-manifest
//! level.

use serde::{Deserialize, Serialize};

/// One overridden fail-closed configuration: exactly which fail-closed configuration
/// was overridden. Freeform stable strings, matching `pg-cli`'s existing
/// `predicate=.../construct=.../witness=...` diagnostic vocabulary; this schema step does not
/// mint a registry for any of the three.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverriddenConfig {
    pub predicate: String,
    pub construct: String,
    pub witness: String,
}

/// The permanent, indelible override record: who authorized the force-compile, when, and
/// why, plus exactly which fail-closed configurations were overridden. Written once at pack-build
/// time and never editable by a consumer -- this type has
/// no field a reader could use to erase the fact an override happened; the only way a pack stops
/// carrying one is a clean recompile that doesn't reach this constructor at all).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityOverrideRecord {
    /// Who or what authorized the override (a caller identity, tool name, or operator label).
    pub authorized_by: String,
    /// Why the override was exercised.
    pub reason: String,
    /// Caller-supplied record of when the override was exercised (free-form string, avoiding a
    /// timestamp dependency in this schema-only type).
    pub recorded_at: String,
    /// Every fail-closed configuration the characteristics-check gate refused that this override
    /// force-compiled through.
    pub overridden_configs: Vec<OverriddenConfig>,
}

/// The binary capability-trust axis, stamped into every pack manifest
/// (`crate::manifest::PackManifest::capability_trust`). `Proven` packs passed the
/// characteristics-check gate cleanly; `Overridden` packs were force-compiled past a `Refuse`
/// verdict and are indelibly stamped unproven/recall-unsafe. Tagged so a reader can
/// distinguish the two without probing for `Option`-ness of a shared field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CapabilityTrust {
    /// The characteristics-check gate admitted this grammar cleanly; no override was exercised.
    Proven,
    /// This pack was force-compiled past a `Refuse` verdict via the capability override; permanent and indelible.
    Overridden(CapabilityOverrideRecord),
}

impl CapabilityTrust {
    /// `true` for `CapabilityTrust::Overridden` — the pack-level "unproven" broadcast
    /// required at load and on every analysis result.
    pub fn is_unproven(&self) -> bool {
        matches!(self, CapabilityTrust::Overridden(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proven_is_not_unproven() {
        assert!(!CapabilityTrust::Proven.is_unproven());
    }

    #[test]
    fn proven_round_trips_through_json() {
        let trust = CapabilityTrust::Proven;
        let json = serde_json::to_string(&trust).unwrap();
        assert_eq!(json, r#"{"status":"proven"}"#);
        let parsed: CapabilityTrust = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, trust);
    }

}
