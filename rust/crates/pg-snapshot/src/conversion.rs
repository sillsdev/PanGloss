//! Conversion-provenance schema: what the FieldWorks-to-`Snapshot` pipeline saw, kept, and
//! dropped, tracked independently of the semantic snapshot fields `grammar_hash` covers.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The schema version this build of `pg-snapshot` writes into a fresh [`ConversionProvenance`].
pub const CONVERSION_PROVENANCE_SCHEMA_VERSION: u16 = 1;

/// Which kind of source-graph object (or synthesized stand-in) an [`InventoryKey`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InventoryKind {
    Entry,
    Sense,
    EntryReference,
    Msa,
    Allomorph,
    AffixProcess,
    Environment,
    Phoneme,
    BoundaryMarker,
    NaturalClass,
    FeatureDefinition,
    FeatureValue,
    FeatureStructure,
    PhonologicalContext,
    FeatureConstraint,
    MorphemeCoOccurrence,
    AllomorphCoOccurrence,
    PhonologicalRule,
    CompoundRule,
    PartOfSpeech,
    InflectionClass,
    StemName,
    RuleFeature,
    ParserSetting,
    StrataConfiguration,
    Template,
    TemplateSlot,
}

/// How an [`InventoryKey`] identifies one object: a single owned GUID, an attachment between two
/// GUIDs, an expansion of one GUID into several, or a named setting with no GUID at all. Built
/// only through [`InventoryKey`]'s constructors — never by concatenating strings by hand.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum InventoryIdentity {
    Object {
        guid: String,
    },
    Attachment {
        owner_guid: String,
        target_guid: String,
        role: String,
    },
    Expansion {
        owner_guid: String,
        member_guids: Vec<String>,
        role: String,
    },
    Setting {
        name: String,
    },
}

/// A stable, orderable identity for one item tracked across the conversion inventory stages.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryKey {
    pub kind: InventoryKind,
    pub identity: InventoryIdentity,
}

impl InventoryKey {
    /// A key naming a single owned source object.
    pub fn object(kind: InventoryKind, guid: impl Into<String>) -> Self {
        InventoryKey {
            kind,
            identity: InventoryIdentity::Object { guid: guid.into() },
        }
    }

    /// A key naming an [`InventoryIdentity::Attachment`] between an owner and a target.
    pub fn attachment(
        kind: InventoryKind,
        owner_guid: impl Into<String>,
        target_guid: impl Into<String>,
        role: impl Into<String>,
    ) -> Self {
        InventoryKey {
            kind,
            identity: InventoryIdentity::Attachment {
                owner_guid: owner_guid.into(),
                target_guid: target_guid.into(),
                role: role.into(),
            },
        }
    }

    /// A key naming an owner's expansion into several member guids under a given `role`.
    pub fn expansion(
        kind: InventoryKind,
        owner_guid: impl Into<String>,
        member_guids: Vec<String>,
        role: impl Into<String>,
    ) -> Self {
        InventoryKey {
            kind,
            identity: InventoryIdentity::Expansion {
                owner_guid: owner_guid.into(),
                member_guids,
                role: role.into(),
            },
        }
    }

    /// A key naming a source-graph-wide setting that has no owning GUID (e.g. a parser option).
    pub fn setting(kind: InventoryKind, name: impl Into<String>) -> Self {
        InventoryKey {
            kind,
            identity: InventoryIdentity::Setting { name: name.into() },
        }
    }
}

/// Which conversion-pipeline stage an [`InventoryKey`] reached, tracked as one set per stage so
/// the same key can be compared across stages (e.g. `authored` minus `represented` names what
/// was silently lost); `represented` is a pre-compaction claim, recorded before any later reachability/natural-class compaction pass runs against the compiled grammar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionInventory {
    pub authored: BTreeSet<InventoryKey>,
    pub considered: BTreeSet<InventoryKey>,
    pub selected: BTreeSet<InventoryKey>,
    pub represented: BTreeSet<InventoryKey>,
    pub rejected: BTreeSet<InventoryKey>,
    pub synthesized: BTreeSet<InventoryKey>,
}

/// Accumulates one conversion stage's selection inventory (one set per pipeline stage, plus rejection issues); every mutation names its stage and nothing here re-decides what the caller already decided.
#[derive(Debug, Default)]
pub struct SelectionRecorder {
    inventory: ConversionInventory,
    issues: Vec<ConversionIssue>,
}

impl SelectionRecorder {
    pub fn authored(&mut self, key: InventoryKey) {
        self.inventory.authored.insert(key);
    }

    pub fn considered(&mut self, key: InventoryKey) {
        self.inventory.considered.insert(key);
    }

    pub fn selected(&mut self, key: InventoryKey) {
        self.inventory.selected.insert(key);
    }

    pub fn represented(&mut self, key: InventoryKey) {
        self.inventory.represented.insert(key);
    }

    pub fn synthesized(&mut self, key: InventoryKey) {
        self.inventory.synthesized.insert(key);
    }

    pub fn rejected(&mut self, key: InventoryKey, issue: ConversionIssue) {
        self.inventory.rejected.insert(key);
        self.issues.push(issue);
    }

    pub fn is_represented(&self, key: &InventoryKey) -> bool {
        self.inventory.represented.contains(key)
    }

    /// Moves an already-`represented` key to `rejected`, for a key a post-hoc reachability/reference
    /// compaction pass dropped after the compiler represented it. Panics if `key` was not represented:
    /// that would mean the caller is re-deriving reachability instead of relaying an owner's own decision.
    pub fn revoke_represented(&mut self, key: InventoryKey, issue: ConversionIssue) {
        let was_represented = self.inventory.represented.remove(&key);
        assert!(
            was_represented,
            "revoke_represented: key was not represented: {key:?}"
        );
        self.inventory.rejected.insert(key);
        self.issues.push(issue);
    }

    /// `represented ⊆ selected ⊆ considered ⊆ authored ∪ synthesized`; `rejected ⊆ selected`; `represented ∩ rejected = ∅`.
    pub fn check_invariants(&self) -> Result<(), String> {
        let authored_or_synthesized: BTreeSet<_> = self
            .inventory
            .authored
            .union(&self.inventory.synthesized)
            .cloned()
            .collect();
        for key in &self.inventory.considered {
            if !authored_or_synthesized.contains(key) {
                return Err(format!(
                    "considered but neither authored nor synthesized: {key:?}"
                ));
            }
        }
        for key in &self.inventory.selected {
            if !self.inventory.considered.contains(key) {
                return Err(format!("selected but not considered: {key:?}"));
            }
        }
        for key in &self.inventory.represented {
            if !self.inventory.selected.contains(key) {
                return Err(format!("represented but not selected: {key:?}"));
            }
        }
        for key in &self.inventory.rejected {
            if !self.inventory.selected.contains(key) {
                return Err(format!("rejected but not selected: {key:?}"));
            }
            if self.inventory.represented.contains(key) {
                return Err(format!("both represented and rejected: {key:?}"));
            }
        }
        Ok(())
    }

    pub fn finish(self) -> (ConversionInventory, Vec<ConversionIssue>) {
        let result = self.check_invariants();
        debug_assert!(result.is_ok(), "selection recorder invariant violated: {result:?}");
        (self.inventory, self.issues)
    }
}

/// What kind of problem a [`ConversionIssue`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueClass {
    MalformedSource,
    InvalidSource,
    AmbiguousSource,
    UnrepresentableForHc,
    SubstrateUnresolvable,
    MigrationDifference,
    /// The compiler represented this key and a later reachability/reference compaction pass then
    /// removed it from the compiled `Grammar` — exactly as an exporter walking the finished
    /// grammar would never visit it. Never fatal.
    UnreachableInGrammar,
}

/// A pointer at the raw source object a [`ConversionIssue`] is about, independent of whether that
/// object ever made it into an [`InventoryKey`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub kind: String,
    pub id: String,
}

/// One problem the importer noticed while building this snapshot from its source graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionIssue {
    pub code: String,
    pub class: IssueClass,
    pub source: Option<SourceRef>,
    pub fatal: bool,
    pub message: String,
}

/// Whether [`ConversionProvenance::source_census`] and `graph_to_snapshot` reflect a real import,
/// and if so, whether that import completed without fatal issues.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceInventoryStatus {
    ImportedComplete,
    ImportedWithFatalIssues,
    Synthetic,
    #[default]
    Unknown,
}

/// A raw tally of the source graph's object classes, taken before any conversion decision is
/// made — the denominator [`ConversionInventory`]'s stages are measured against.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawSourceCensus {
    pub total_occurrences: u64,
    pub class_occurrences: BTreeMap<String, u64>,
    pub unhandled_class_occurrences: BTreeMap<String, u64>,
    pub ordered_header_sha256: String,
}

/// Errors from [`ConversionProvenance::validate`].
#[derive(Debug, Error)]
pub enum ProvenanceError {
    /// A `schema_version` this build has no reading for. Never means clean provenance.
    #[error(
        "unsupported conversion-provenance schema version {found}; this build supports 0 or 1"
    )]
    UnsupportedSchemaVersion { found: u16 },
    /// Schema version 0 predates [`SourceInventoryStatus`] and must carry `Unknown`.
    #[error(
        "schema version 0 conversion-provenance must carry SourceInventoryStatus::Unknown, found {found:?}"
    )]
    VersionZeroRequiresUnknownStatus { found: SourceInventoryStatus },
    /// Schema version 1 always resolves a real status; `Unknown` at that version is impossible.
    #[error("schema version 1 conversion-provenance must not carry SourceInventoryStatus::Unknown")]
    VersionOneForbidsUnknownStatus,
}

/// A versioned record of how this snapshot's `Snapshot::conversion_provenance` was produced:
/// what the source graph contained, what the importer kept or dropped, and what it flagged along
/// the way. Absent from JSON (a pre-provenance document), this deserializes as
/// `schema_version: 0` with `SourceInventoryStatus::Unknown` — [`Default`]'s reading, never
/// treated as "clean".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionProvenance {
    pub schema_version: u16,
    pub source_inventory_status: SourceInventoryStatus,
    pub source_census: RawSourceCensus,
    pub graph_to_snapshot: ConversionInventory,
    pub import_issues: Vec<ConversionIssue>,
}

impl ConversionProvenance {
    /// The provenance a synthetically-built `Snapshot` gets: no source graph exists to census.
    pub fn synthetic() -> Self {
        ConversionProvenance {
            schema_version: CONVERSION_PROVENANCE_SCHEMA_VERSION,
            source_inventory_status: SourceInventoryStatus::Synthetic,
            ..ConversionProvenance::default()
        }
    }

    /// Checks whether `schema_version` and `source_inventory_status` form a pair this build understands.
    pub fn validate(&self) -> Result<(), ProvenanceError> {
        match self.schema_version {
            0 => {
                if self.source_inventory_status != SourceInventoryStatus::Unknown {
                    return Err(ProvenanceError::VersionZeroRequiresUnknownStatus {
                        found: self.source_inventory_status,
                    });
                }
            }
            1 => {
                if self.source_inventory_status == SourceInventoryStatus::Unknown {
                    return Err(ProvenanceError::VersionOneForbidsUnknownStatus);
                }
            }
            found => return Err(ProvenanceError::UnsupportedSchemaVersion { found }),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_identity_carries_the_given_guid() {
        let key = InventoryKey::object(InventoryKind::Entry, "guid-1");
        assert_eq!(
            key,
            InventoryKey {
                kind: InventoryKind::Entry,
                identity: InventoryIdentity::Object {
                    guid: "guid-1".to_string()
                },
            }
        );
    }

    #[test]
    fn attachment_identity_carries_owner_target_and_role() {
        let key = InventoryKey::attachment(InventoryKind::Environment, "owner", "target", "role");
        assert_eq!(
            key,
            InventoryKey {
                kind: InventoryKind::Environment,
                identity: InventoryIdentity::Attachment {
                    owner_guid: "owner".to_string(),
                    target_guid: "target".to_string(),
                    role: "role".to_string(),
                },
            }
        );
    }

    #[test]
    fn expansion_identity_carries_owner_members_and_role() {
        let key = InventoryKey::expansion(
            InventoryKind::Template,
            "owner",
            vec!["m1".to_string(), "m2".to_string()],
            "slots",
        );
        assert_eq!(
            key,
            InventoryKey {
                kind: InventoryKind::Template,
                identity: InventoryIdentity::Expansion {
                    owner_guid: "owner".to_string(),
                    member_guids: vec!["m1".to_string(), "m2".to_string()],
                    role: "slots".to_string(),
                },
            }
        );
    }

    #[test]
    fn setting_identity_carries_only_a_name() {
        let key = InventoryKey::setting(InventoryKind::ParserSetting, "activeParser");
        assert_eq!(
            key,
            InventoryKey {
                kind: InventoryKind::ParserSetting,
                identity: InventoryIdentity::Setting {
                    name: "activeParser".to_string()
                },
            }
        );
    }

    #[test]
    fn inventory_key_btreeset_round_trips_through_json_in_sorted_order() {
        let mut set = BTreeSet::new();
        set.insert(InventoryKey::object(InventoryKind::Sense, "zzz"));
        set.insert(InventoryKey::object(InventoryKind::Entry, "aaa"));
        set.insert(InventoryKey::setting(InventoryKind::ParserSetting, "x"));

        let sorted_before: Vec<_> = set.iter().cloned().collect();
        let json = serde_json::to_string(&set).expect("BTreeSet<InventoryKey> must serialize");
        let round_tripped: BTreeSet<InventoryKey> =
            serde_json::from_str(&json).expect("must deserialize back");
        let sorted_after: Vec<_> = round_tripped.iter().cloned().collect();

        assert_eq!(sorted_before, sorted_after);
        assert_eq!(set, round_tripped);
    }

    #[test]
    fn synthetic_provenance_is_current_schema_version_and_synthetic_status() {
        let provenance = ConversionProvenance::synthetic();
        assert_eq!(
            provenance.schema_version,
            CONVERSION_PROVENANCE_SCHEMA_VERSION
        );
        assert_eq!(
            provenance.source_inventory_status,
            SourceInventoryStatus::Synthetic
        );
    }

    #[test]
    fn validate_accepts_version_zero_with_unknown_status() {
        let provenance = ConversionProvenance {
            schema_version: 0,
            source_inventory_status: SourceInventoryStatus::Unknown,
            ..ConversionProvenance::default()
        };
        assert!(provenance.validate().is_ok());
    }

    #[test]
    fn validate_accepts_version_one_with_a_resolved_status() {
        let provenance = ConversionProvenance {
            schema_version: 1,
            source_inventory_status: SourceInventoryStatus::ImportedComplete,
            ..ConversionProvenance::default()
        };
        assert!(provenance.validate().is_ok());
    }

    #[test]
    fn validate_rejects_version_zero_with_a_resolved_status() {
        let provenance = ConversionProvenance {
            schema_version: 0,
            source_inventory_status: SourceInventoryStatus::ImportedComplete,
            ..ConversionProvenance::default()
        };
        assert!(matches!(
            provenance.validate(),
            Err(ProvenanceError::VersionZeroRequiresUnknownStatus { .. })
        ));
    }

    #[test]
    fn validate_rejects_version_one_with_unknown_status() {
        let provenance = ConversionProvenance {
            schema_version: 1,
            source_inventory_status: SourceInventoryStatus::Unknown,
            ..ConversionProvenance::default()
        };
        assert!(matches!(
            provenance.validate(),
            Err(ProvenanceError::VersionOneForbidsUnknownStatus)
        ));
    }

    #[test]
    fn validate_rejects_an_unrecognized_schema_version_regardless_of_status() {
        let provenance = ConversionProvenance {
            schema_version: 2,
            source_inventory_status: SourceInventoryStatus::ImportedComplete,
            ..ConversionProvenance::default()
        };
        assert!(matches!(
            provenance.validate(),
            Err(ProvenanceError::UnsupportedSchemaVersion { found: 2 })
        ));
    }

    #[test]
    fn invariants_hold_for_a_well_formed_sequence() {
        let mut r = SelectionRecorder::default();
        let key = InventoryKey::object(InventoryKind::Entry, "g1");
        r.authored(key.clone());
        r.considered(key.clone());
        r.selected(key.clone());
        r.represented(key);
        assert!(r.check_invariants().is_ok());
    }

    #[test]
    fn invariants_reject_selected_without_considered() {
        let mut r = SelectionRecorder::default();
        let key = InventoryKey::object(InventoryKind::Entry, "g1");
        r.authored(key.clone());
        r.selected(key);
        assert!(r.check_invariants().is_err());
    }

    #[test]
    fn invariants_reject_considered_without_authored_or_synthesized() {
        let mut r = SelectionRecorder::default();
        let key = InventoryKey::object(InventoryKind::Entry, "g1");
        r.considered(key);
        assert!(r.check_invariants().is_err());
    }

    #[test]
    fn invariants_reject_represented_and_rejected_together() {
        let mut r = SelectionRecorder::default();
        let key = InventoryKey::object(InventoryKind::Entry, "g1");
        r.authored(key.clone());
        r.considered(key.clone());
        r.selected(key.clone());
        r.represented(key.clone());
        r.rejected(
            key,
            ConversionIssue {
                code: "test.code".to_string(),
                class: IssueClass::UnrepresentableForHc,
                source: None,
                fatal: false,
                message: "test".to_string(),
            },
        );
        assert!(r.check_invariants().is_err());
    }

    #[test]
    fn revoke_represented_moves_the_key_to_rejected_and_off_represented() {
        let mut r = SelectionRecorder::default();
        let key = InventoryKey::object(InventoryKind::Msa, "g1");
        r.authored(key.clone());
        r.considered(key.clone());
        r.selected(key.clone());
        r.represented(key.clone());
        r.revoke_represented(
            key.clone(),
            ConversionIssue {
                code: "test.revoked".to_string(),
                class: IssueClass::UnreachableInGrammar,
                source: None,
                fatal: false,
                message: "test".to_string(),
            },
        );
        assert!(!r.inventory.represented.contains(&key));
        assert!(r.inventory.rejected.contains(&key));
        assert!(r.check_invariants().is_ok());
    }

    #[test]
    #[should_panic(expected = "key was not represented")]
    fn revoke_represented_panics_on_a_key_that_was_never_represented() {
        let mut r = SelectionRecorder::default();
        let key = InventoryKey::object(InventoryKind::Msa, "g1");
        r.authored(key.clone());
        r.considered(key.clone());
        r.selected(key.clone());
        r.revoke_represented(
            key,
            ConversionIssue {
                code: "test.revoked".to_string(),
                class: IssueClass::UnreachableInGrammar,
                source: None,
                fatal: false,
                message: "test".to_string(),
            },
        );
    }

    #[test]
    fn synthesized_satisfies_the_authored_or_synthesized_requirement() {
        let mut r = SelectionRecorder::default();
        let key = InventoryKey::object(InventoryKind::Msa, "synth-1");
        r.synthesized(key.clone());
        r.considered(key.clone());
        r.selected(key.clone());
        r.represented(key);
        assert!(r.check_invariants().is_ok());
    }
}
