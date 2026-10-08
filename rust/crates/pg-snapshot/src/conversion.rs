//! Conversion-provenance schema: what the FieldWorks-to-`Snapshot` pipeline saw, kept, and
//! dropped, tracked independently of the semantic snapshot fields `grammar_hash` covers.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// The schema version this build of `pg-snapshot` writes into a fresh [`ConversionProvenance`].
pub const CONVERSION_PROVENANCE_SCHEMA_VERSION: u16 = 2;

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
    SourceObject,
}

/// How an [`InventoryKey`] identifies one object: a single owned GUID, an attachment between two
/// GUIDs, an expansion of one GUID into several, a compiler-created object with a stable synthetic
/// key, a named setting, or one source-header occurrence. Built through `InventoryKey` constructors.
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
    Synthetic {
        key: String,
    },
    Setting {
        name: String,
    },
    SourceOccurrence {
        ordinal: u64,
        class_name: String,
        raw_guid: String,
        canonical_guid: Option<String>,
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

    /// A key naming a compiler-created object that has no authored GUID.
    pub fn synthetic(kind: InventoryKind, key: impl Into<String>) -> Self {
        InventoryKey {
            kind,
            identity: InventoryIdentity::Synthetic { key: key.into() },
        }
    }

    /// A key naming a source-graph-wide setting that has no owning GUID (e.g. a parser option).
    pub fn setting(kind: InventoryKind, name: impl Into<String>) -> Self {
        InventoryKey {
            kind,
            identity: InventoryIdentity::Setting { name: name.into() },
        }
    }

    /// A key naming one header occurrence in the imported source graph.
    pub fn source_occurrence(
        ordinal: u64,
        class_name: impl Into<String>,
        raw_guid: impl Into<String>,
        canonical_guid: Option<String>,
    ) -> Self {
        InventoryKey {
            kind: InventoryKind::SourceObject,
            identity: InventoryIdentity::SourceOccurrence {
                ordinal,
                class_name: class_name.into(),
                raw_guid: raw_guid.into(),
                canonical_guid,
            },
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
#[derive(Debug, Default, Clone)]
pub struct SelectionRecorder {
    inventory: ConversionInventory,
    issues: Vec<ConversionIssue>,
    load_decisions: Vec<LoadDecision>,
    load_decision_ordinals: BTreeMap<(InventoryKey, LoadPipelineStage, String), u32>,
    load_decision_stage: Option<LoadPipelineStage>,
    /// Owner-selected literal text for substrate inference; independent of the stage sets below.
    text_uses: Vec<(SourceRef, String)>,
}

impl SelectionRecorder {
    /// Creates a recorder that publishes typed decisions for one pipeline stage.
    pub fn for_stage(stage: LoadPipelineStage) -> Self {
        Self {
            load_decision_stage: Some(stage),
            ..Self::default()
        }
    }

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
        if let Some(stage) = self.load_decision_stage {
            self.record_load_decision(LoadDecisionDraft {
                subject: key.clone(),
                pipeline_stage: stage,
                context_key: String::new(),
                disposition: LoadDisposition::Represented,
                loaded: Some(true),
                reason_code: LoadReasonCode::Represented,
                effective_value_json: None,
                issue_code: None,
            });
        }
        self.inventory.represented.insert(key);
    }

    pub fn represented_with_effective_value(
        &mut self,
        key: InventoryKey,
        effective_value_json: String,
    ) {
        if let Some(stage) = self.load_decision_stage {
            self.record_load_decision(LoadDecisionDraft {
                subject: key.clone(),
                pipeline_stage: stage,
                context_key: String::new(),
                disposition: LoadDisposition::Represented,
                loaded: Some(true),
                reason_code: LoadReasonCode::Represented,
                effective_value_json: Some(effective_value_json),
                issue_code: None,
            });
        }
        self.inventory.represented.insert(key);
    }

    pub fn not_considered(
        &mut self,
        subject: InventoryKey,
        context_key: String,
        reason_code: LoadReasonCode,
    ) {
        if let Some(stage) = self.load_decision_stage {
            self.record_load_decision(LoadDecisionDraft {
                subject,
                pipeline_stage: stage,
                context_key,
                disposition: LoadDisposition::NotConsidered,
                loaded: None,
                reason_code,
                effective_value_json: None,
                issue_code: None,
            });
        }
    }

    pub fn synthesized(&mut self, key: InventoryKey) {
        if let Some(stage) = self.load_decision_stage {
            self.record_load_decision(LoadDecisionDraft {
                subject: key.clone(),
                pipeline_stage: stage,
                context_key: String::new(),
                disposition: LoadDisposition::Synthesized,
                loaded: Some(true),
                reason_code: LoadReasonCode::Synthesized,
                effective_value_json: None,
                issue_code: None,
            });
        }
        self.inventory.synthesized.insert(key);
    }

    pub fn rejected(&mut self, key: InventoryKey, issue: ConversionIssue) {
        self.rejected_with_reason(
            key,
            issue.clone(),
            LoadReasonCode::ConversionIssue(issue.code.wire().to_string()),
        );
    }

    pub fn rejected_with_reason(
        &mut self,
        key: InventoryKey,
        issue: ConversionIssue,
        reason_code: LoadReasonCode,
    ) {
        if let Some(stage) = self.load_decision_stage {
            let issue_code = issue.code.wire().to_string();
            self.record_load_decision(LoadDecisionDraft {
                subject: key.clone(),
                pipeline_stage: stage,
                context_key: String::new(),
                disposition: LoadDisposition::Rejected,
                loaded: Some(false),
                reason_code,
                effective_value_json: None,
                issue_code: Some(issue_code),
            });
        }
        self.inventory.rejected.insert(key);
        self.issues.push(issue);
    }

    /// Records a selected rejection owned by a pipeline decision with no conversion issue.
    pub fn rejected_without_issue(&mut self, draft: LoadDecisionDraft) {
        assert_eq!(
            draft.disposition,
            LoadDisposition::Rejected,
            "rejected_without_issue requires a rejected disposition"
        );
        assert_eq!(
            draft.loaded,
            Some(false),
            "rejected_without_issue requires loaded=false"
        );
        self.inventory.rejected.insert(draft.subject.clone());
        self.record_load_decision(draft);
    }

    /// Records an owner's typed disposition without changing the inventory stage sets.
    pub fn record_load_decision(&mut self, draft: LoadDecisionDraft) {
        assert!(
            !draft.reason_code.as_str().is_empty(),
            "load decisions require a reason code"
        );
        assert!(
            !matches!(
                draft.disposition,
                LoadDisposition::Unknown | LoadDisposition::NotConsidered
            ) || draft.loaded.is_none(),
            "unknown and not-considered load decisions cannot claim a loaded value"
        );
        let ordinal_key = (
            draft.subject.clone(),
            draft.pipeline_stage,
            draft.context_key.clone(),
        );
        let ordinal = self.load_decision_ordinals.entry(ordinal_key).or_default();
        let decision_ordinal = *ordinal;
        *ordinal = ordinal
            .checked_add(1)
            .expect("load decision ordinal overflow");
        self.load_decisions.push(LoadDecision {
            subject: draft.subject,
            pipeline_stage: draft.pipeline_stage,
            decision_ordinal,
            context_key: draft.context_key,
            disposition: draft.disposition,
            loaded: draft.loaded,
            reason_code: draft.reason_code,
            effective_value_json: draft.effective_value_json,
            issue_code: draft.issue_code,
        });
    }

    /// Whether an owner has already published a decision for `subject` at `stage`.
    pub fn has_load_decision(&self, subject: &InventoryKey, stage: LoadPipelineStage) -> bool {
        self.load_decisions
            .iter()
            .any(|decision| decision.subject == *subject && decision.pipeline_stage == stage)
    }

    /// Records an issue about retained output without changing its inventory status.
    pub fn noted(&mut self, issue: ConversionIssue) {
        self.issues.push(issue);
    }

    pub fn is_represented(&self, key: &InventoryKey) -> bool {
        self.inventory.represented.contains(key)
    }

    /// Publishes one owner's already-selected literal text (substrate-inference input); see the
    /// `text_uses` field doc for why this never touches `check_invariants`' stage sets.
    pub fn record_text_use(&mut self, source: SourceRef, text: &str) {
        self.text_uses.push((source, text.to_string()));
    }

    /// Every literal text use published so far, in recording order.
    pub fn text_uses(&self) -> &[(SourceRef, String)] {
        &self.text_uses
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
        let issue_code = issue.code.wire().to_string();
        if self.load_decision_stage == Some(LoadPipelineStage::Compile) {
            self.record_load_decision(LoadDecisionDraft {
                subject: key.clone(),
                pipeline_stage: LoadPipelineStage::Compact,
                context_key: String::new(),
                disposition: LoadDisposition::Compacted,
                loaded: Some(false),
                reason_code: LoadReasonCode::ConversionIssue(issue_code.clone()),
                effective_value_json: None,
                issue_code: Some(issue_code),
            });
        }
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
        let (inventory, issues, _) = self.finish_with_load_decisions();
        (inventory, issues)
    }

    /// Finishes recording and returns typed load decisions without cloning them.
    pub fn finish_with_load_decisions(
        self,
    ) -> (ConversionInventory, Vec<ConversionIssue>, Vec<LoadDecision>) {
        let result = self.check_invariants();
        debug_assert!(
            result.is_ok(),
            "selection recorder invariant violated: {result:?}"
        );
        (self.inventory, self.issues, self.load_decisions)
    }
}

/// The conversion-pipeline phase that made a load decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoadPipelineStage {
    Import,
    Snapshot,
    Compile,
    Compact,
}

impl LoadPipelineStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Snapshot => "snapshot",
            Self::Compile => "compile",
            Self::Compact => "compact",
        }
    }
}

/// What the owning pipeline stage decided about one source object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoadDisposition {
    Represented,
    Rejected,
    Defaulted,
    Synthesized,
    Compacted,
    MetadataOnly,
    NotConsidered,
    Unknown,
}

impl LoadDisposition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Represented => "represented",
            Self::Rejected => "rejected",
            Self::Defaulted => "defaulted",
            Self::Synthesized => "synthesized",
            Self::Compacted => "compacted",
            Self::MetadataOnly => "metadata_only",
            Self::NotConsidered => "not_considered",
            Self::Unknown => "unknown",
        }
    }
}

/// Stable reason emitted by the owner of a load decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "code")]
pub enum LoadReasonCode {
    Represented,
    Disabled,
    Synthesized,
    MetadataOnly,
    Abstract,
    EmptyForm,
    MorphTypeNotInBucket,
    MissingGuid,
    DuplicateHeader,
    UnknownClass,
    Unreferenced,
    NoEligibleAllomorph,
    OwnerNotLoaded,
    AncestorDefaultInflectionClass,
    AdditionalPhonemeSetNotSelected,
    MissingReference,
    WrongKindReference,
    DecisionUnrecorded,
    SourceInventoryUnknown,
    ConversionIssue(String),
}

impl LoadReasonCode {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Represented => "represented",
            Self::Disabled => "disabled",
            Self::Synthesized => "synthesized",
            Self::MetadataOnly => "metadataOnly",
            Self::Abstract => "abstract",
            Self::EmptyForm => "emptyForm",
            Self::MorphTypeNotInBucket => "morphTypeNotInBucket",
            Self::MissingGuid => "missing_guid",
            Self::DuplicateHeader => "duplicate_header",
            Self::UnknownClass => "unknown_class",
            Self::Unreferenced => "unreferenced",
            Self::NoEligibleAllomorph => "noEligibleAllomorph",
            Self::OwnerNotLoaded => "ownerNotLoaded",
            Self::AncestorDefaultInflectionClass => "ancestorDefaultInflectionClass",
            Self::AdditionalPhonemeSetNotSelected => "additional_phoneme_set_not_selected",
            Self::MissingReference => "missing_reference",
            Self::WrongKindReference => "wrong_kind_reference",
            Self::DecisionUnrecorded => "decision_unrecorded",
            Self::SourceInventoryUnknown => "source_inventory_unknown",
            Self::ConversionIssue(code) => code,
        }
    }
}

/// One typed load decision emitted by the stage that made it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadDecision {
    pub subject: InventoryKey,
    pub pipeline_stage: LoadPipelineStage,
    pub decision_ordinal: u32,
    pub context_key: String,
    pub disposition: LoadDisposition,
    pub loaded: Option<bool>,
    pub reason_code: LoadReasonCode,
    pub effective_value_json: Option<String>,
    pub issue_code: Option<String>,
}

/// The owner-supplied fields for a load decision; the recorder assigns its ordinal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadDecisionDraft {
    pub subject: InventoryKey,
    pub pipeline_stage: LoadPipelineStage,
    pub context_key: String,
    pub disposition: LoadDisposition,
    pub loaded: Option<bool>,
    pub reason_code: LoadReasonCode,
    pub effective_value_json: Option<String>,
    pub issue_code: Option<String>,
}

/// The conversion-loss categories a [`ConversionInventory`] implies but does not itself name --
/// derived, never recorded independently, so they can never drift from the stage sets they read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryDelta {
    pub inventory: ConversionInventory,
    pub issues: Vec<ConversionIssue>,
    /// `selected` minus (`represented` union `rejected`): chosen for the grammar but neither built nor refused -- the category this recording exists to surface.
    pub silently_omitted: BTreeSet<InventoryKey>,
    /// `represented` minus (`authored` union `synthesized`): present in the grammar with no recorded origin.
    pub unclassified: BTreeSet<InventoryKey>,
    /// `synthesized` minus `authored`: created by the compiler rather than by the source.
    pub synthesized_only: BTreeSet<InventoryKey>,
}

impl InventoryDelta {
    /// Derives every category from one finished stage's `(inventory, issues)` pair; never recomputes what `SelectionRecorder` already decided.
    pub fn from_stage(inventory: ConversionInventory, issues: Vec<ConversionIssue>) -> Self {
        let represented_or_rejected: BTreeSet<InventoryKey> = inventory
            .represented
            .union(&inventory.rejected)
            .cloned()
            .collect();
        let silently_omitted: BTreeSet<InventoryKey> = inventory
            .selected
            .difference(&represented_or_rejected)
            .cloned()
            .collect();
        let authored_or_synthesized: BTreeSet<InventoryKey> = inventory
            .authored
            .union(&inventory.synthesized)
            .cloned()
            .collect();
        let unclassified: BTreeSet<InventoryKey> = inventory
            .represented
            .difference(&authored_or_synthesized)
            .cloned()
            .collect();
        let synthesized_only: BTreeSet<InventoryKey> = inventory
            .synthesized
            .difference(&inventory.authored)
            .cloned()
            .collect();
        InventoryDelta {
            inventory,
            issues,
            silently_omitted,
            unclassified,
            synthesized_only,
        }
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
    pub kind: crate::FwClass,
    pub id: String,
}

/// One problem the importer noticed while building this snapshot from its source graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionIssue {
    pub code: crate::ImportWarningCode,
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

/// One source header occurrence with the streaming reader's record-retention state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawSourceObject {
    pub ordinal: u64,
    pub class_name: String,
    pub raw_guid: String,
    pub canonical_guid: Option<String>,
    pub handled: bool,
    pub inventory_kind: Option<InventoryKind>,
    pub retained: bool,
    pub duplicate: bool,
}

impl RawSourceObject {
    /// Identifies this exact source header, including malformed and repeated identities.
    pub fn key(&self) -> InventoryKey {
        InventoryKey::source_occurrence(
            self.ordinal,
            self.class_name.clone(),
            self.raw_guid.clone(),
            self.canonical_guid.clone(),
        )
    }
}

/// Errors from [`ConversionProvenance::validate`].
#[derive(Debug, Error)]
pub enum ProvenanceError {
    /// A `schema_version` this build has no reading for. Never means clean provenance.
    #[error(
        "unsupported conversion-provenance schema version {found}; this build supports 0, 1, or 2"
    )]
    UnsupportedSchemaVersion { found: u16 },
    /// Schema version 0 predates [`SourceInventoryStatus`] and must carry `Unknown`.
    #[error(
        "schema version 0 conversion-provenance must carry SourceInventoryStatus::Unknown, found {found:?}"
    )]
    VersionZeroRequiresUnknownStatus { found: SourceInventoryStatus },
    /// Schema version 1 always resolves a real status; `Unknown` at that version is impossible.
    #[error(
        "schema version 1 conversion-provenance must not carry SourceInventoryStatus::Unknown"
    )]
    VersionOneForbidsUnknownStatus,
    /// Schema version 2 always resolves a real status; `Unknown` at that version is impossible.
    #[error(
        "schema version 2 conversion-provenance must not carry SourceInventoryStatus::Unknown"
    )]
    VersionTwoForbidsUnknownStatus,
    /// Version 2 source objects must account for every census header in source order.
    #[error(
        "schema version 2 conversion-provenance source objects do not match the source census"
    )]
    SourceObjectsDoNotMatchCensus,
    /// Imported version 2 source objects must reproduce the ordered raw-header digest.
    #[error("schema version 2 conversion-provenance header digest does not match source objects")]
    SourceHeaderDigestDoesNotMatch,
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
    #[serde(default)]
    pub source_objects: Vec<RawSourceObject>,
    pub graph_to_snapshot: ConversionInventory,
    #[serde(default)]
    pub import_load_decisions: Vec<LoadDecision>,
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

    /// Checks the supported status/version pairs and the source-header census for this schema.
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
            2 => {
                if self.source_inventory_status == SourceInventoryStatus::Unknown {
                    return Err(ProvenanceError::VersionTwoForbidsUnknownStatus);
                }
                if self.source_objects.len() as u64 != self.source_census.total_occurrences
                    || self
                        .source_objects
                        .iter()
                        .enumerate()
                        .any(|(index, object)| object.ordinal != index as u64 + 1)
                    || self.source_objects.iter().any(|object| {
                        object.canonical_guid != crate::canonical_guid(&object.raw_guid)
                    })
                {
                    return Err(ProvenanceError::SourceObjectsDoNotMatchCensus);
                }
                let mut class_occurrences = BTreeMap::new();
                let mut unhandled_class_occurrences = BTreeMap::new();
                for object in &self.source_objects {
                    *class_occurrences
                        .entry(object.class_name.clone())
                        .or_insert(0_u64) += 1;
                    if !object.handled {
                        *unhandled_class_occurrences
                            .entry(object.class_name.clone())
                            .or_insert(0_u64) += 1;
                    }
                }
                if class_occurrences != self.source_census.class_occurrences
                    || unhandled_class_occurrences != self.source_census.unhandled_class_occurrences
                {
                    return Err(ProvenanceError::SourceObjectsDoNotMatchCensus);
                }
                if matches!(
                    self.source_inventory_status,
                    SourceInventoryStatus::ImportedComplete
                        | SourceInventoryStatus::ImportedWithFatalIssues
                ) {
                    let mut hasher = Sha256::new();
                    for object in &self.source_objects {
                        hasher.update(
                            format!("{}\t{}\n", object.class_name, object.raw_guid).as_bytes(),
                        );
                    }
                    let expected = format!("{:x}", hasher.finalize());
                    if self.source_census.ordered_header_sha256 != expected {
                        return Err(ProvenanceError::SourceHeaderDigestDoesNotMatch);
                    }
                }
            }
            found => return Err(ProvenanceError::UnsupportedSchemaVersion { found }),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
