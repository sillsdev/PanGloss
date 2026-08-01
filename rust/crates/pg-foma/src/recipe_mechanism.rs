//! Wire-owned vocabulary and fail-closed validation for executable mechanisms.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{
    AllomorphId, FamilyId, LexEntryId, MRuleId, MorphemeId, MprId, NatClassId, PRuleId, StemNameId,
    StratumId, TableId, TemplateId, VarId,
};
use serde::{Deserialize, Serialize};

use crate::capability::ModelLocation;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MechanismId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionDisposition {
    ExactFst,
    ConfirmOnly,
    Peeled,
    Refused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MechanismKind {
    Morphotactics,
    StaticPartition,
    OrderedPhonology,
    StructuralAllomorph,
    CopyProcess,
    BoundaryCleanup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WireModelKind {
    CharDef,
    Stratum,
    MRule,
    PRule,
    Template,
    LexEntry,
    Morpheme,
    Allomorph,
    NatClass,
    StemName,
    Family,
    Table,
    Var,
    Mpr,
    MprGroup,
    SubruleIndex,
    AllomorphIndex,
    MorphemeIndex,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WireModelId {
    pub kind: WireModelKind,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireModelIdError {
    WrongKind {
        expected: WireModelKind,
        actual: WireModelKind,
    },
    ValueOutOfRange {
        kind: WireModelKind,
        value: u64,
    },
}

impl fmt::Display for WireModelIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongKind { expected, actual } => {
                write!(f, "expected {expected:?}, got {actual:?}")
            }
            Self::ValueOutOfRange { kind, value } => {
                write!(f, "value {value} is out of range for {kind:?}")
            }
        }
    }
}

impl std::error::Error for WireModelIdError {}

macro_rules! wire_id_conversions {
    ($native:ty, $kind:expr, $max:expr) => {
        impl From<$native> for WireModelId {
            fn from(value: $native) -> Self {
                Self {
                    kind: $kind,
                    value: value.0 as u64,
                }
            }
        }

        impl TryFrom<WireModelId> for $native {
            type Error = WireModelIdError;

            fn try_from(value: WireModelId) -> Result<Self, Self::Error> {
                if value.kind != $kind {
                    return Err(WireModelIdError::WrongKind {
                        expected: $kind,
                        actual: value.kind,
                    });
                }
                if value.value > $max as u64 {
                    return Err(WireModelIdError::ValueOutOfRange {
                        kind: value.kind,
                        value: value.value,
                    });
                }
                Ok(Self(value.value as _))
            }
        }
    };
}

wire_id_conversions!(CharDefId, WireModelKind::CharDef, u32::MAX);
wire_id_conversions!(StratumId, WireModelKind::Stratum, u8::MAX);
wire_id_conversions!(MRuleId, WireModelKind::MRule, u32::MAX);
wire_id_conversions!(PRuleId, WireModelKind::PRule, u32::MAX);
wire_id_conversions!(TemplateId, WireModelKind::Template, u32::MAX);
wire_id_conversions!(LexEntryId, WireModelKind::LexEntry, u32::MAX);
wire_id_conversions!(MorphemeId, WireModelKind::Morpheme, u32::MAX);
wire_id_conversions!(AllomorphId, WireModelKind::Allomorph, u32::MAX);
wire_id_conversions!(NatClassId, WireModelKind::NatClass, u32::MAX);
wire_id_conversions!(StemNameId, WireModelKind::StemName, u32::MAX);
wire_id_conversions!(FamilyId, WireModelKind::Family, u32::MAX);
wire_id_conversions!(TableId, WireModelKind::Table, u16::MAX);
wire_id_conversions!(VarId, WireModelKind::Var, u16::MAX);
wire_id_conversions!(MprId, WireModelKind::Mpr, u8::MAX);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MechanismSourceKind {
    MorphRule,
    AffixAllomorph,
    Stratum,
    MprGroup,
    PhonRule,
    RewriteSubrule,
    NaturalClass,
    MorphemeCoOccurrence,
    AllomorphCoOccurrence,
}

pub type SourceKind = MechanismSourceKind;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MechanismSource {
    pub kind: MechanismSourceKind,
    pub owner: Option<WireModelId>,
    pub child: Option<WireModelId>,
}

impl From<ModelLocation> for MechanismSource {
    fn from(location: ModelLocation) -> Self {
        match location {
            ModelLocation::MorphRule(rule) => Self {
                kind: MechanismSourceKind::MorphRule,
                owner: Some(rule.into()),
                child: None,
            },
            ModelLocation::AffixAllomorph {
                rule,
                allomorph_index,
            } => Self {
                kind: MechanismSourceKind::AffixAllomorph,
                owner: Some(rule.into()),
                child: Some(WireModelId {
                    kind: WireModelKind::AllomorphIndex,
                    value: allomorph_index as u64,
                }),
            },
            ModelLocation::Stratum(stratum) => Self {
                kind: MechanismSourceKind::Stratum,
                owner: Some(stratum.into()),
                child: None,
            },
            ModelLocation::MprGroup(index) => Self {
                kind: MechanismSourceKind::MprGroup,
                owner: Some(WireModelId {
                    kind: WireModelKind::MprGroup,
                    value: index as u64,
                }),
                child: None,
            },
            ModelLocation::PhonRule(rule) => Self {
                kind: MechanismSourceKind::PhonRule,
                owner: Some(rule.into()),
                child: None,
            },
            ModelLocation::RewriteSubrule {
                rule,
                subrule_index,
            } => Self {
                kind: MechanismSourceKind::RewriteSubrule,
                owner: Some(rule.into()),
                child: Some(WireModelId {
                    kind: WireModelKind::SubruleIndex,
                    value: subrule_index as u64,
                }),
            },
            ModelLocation::NaturalClass(class) => Self {
                kind: MechanismSourceKind::NaturalClass,
                owner: Some(class.into()),
                child: None,
            },
            ModelLocation::MorphemeCoOccurrence(index) => Self {
                kind: MechanismSourceKind::MorphemeCoOccurrence,
                owner: Some(WireModelId {
                    kind: WireModelKind::MorphemeIndex,
                    value: index as u64,
                }),
                child: None,
            },
            ModelLocation::AllomorphCoOccurrence(allomorph) => Self {
                kind: MechanismSourceKind::AllomorphCoOccurrence,
                owner: Some(allomorph.into()),
                child: None,
            },
        }
    }
}

impl From<&ModelLocation> for MechanismSource {
    fn from(location: &ModelLocation) -> Self {
        (*location).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CopyKind {
    Prefix,
    Suffix,
    FullStem,
    InternalSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OrderedRuleAtom {
    Rewrite {
        rule: WireModelId,
    },
    Metathesis {
        rule: WireModelId,
        swap_construction_attempted: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PartitionPredicate {
    Pos(String),
    Mpr(WireModelId),
    LexicalClass(String),
    StemFamily(WireModelId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionGroupSpec {
    pub id: String,
    pub predicates: Vec<PartitionPredicate>,
    pub members: Vec<WireModelId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MorphotacticsSpec {
    pub strata: Vec<WireModelId>,
    pub templates: Vec<WireModelId>,
    pub rules: Vec<WireModelId>,
    pub cooccurrence_units: Vec<Vec<String>>,
    pub priority_chains: Vec<Vec<WireModelId>>,
    pub max_depth: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticPartitionSpec {
    pub predicates: Vec<PartitionPredicate>,
    pub groups: Vec<PartitionGroupSpec>,
    pub stable_for_lifetime: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedPhonologySpec {
    pub stratum: WireModelId,
    pub rules: Vec<OrderedRuleAtom>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuralAllomorphSpec {
    pub rule: WireModelId,
    pub allomorphs: Vec<WireModelId>,
    pub bounded_local_shape: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyProcessSpec {
    pub rule: WireModelId,
    pub kind: CopyKind,
    pub max_span: Option<usize>,
    pub max_chain_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryCleanupSpec {
    pub table: WireModelId,
    pub boundary_symbols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "spec", rename_all = "kebab-case")]
pub enum MechanismBody {
    Morphotactics(MorphotacticsSpec),
    StaticPartition(StaticPartitionSpec),
    OrderedPhonology(OrderedPhonologySpec),
    StructuralAllomorph(StructuralAllomorphSpec),
    CopyProcess(CopyProcessSpec),
    BoundaryCleanup(BoundaryCleanupSpec),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MechanismSpec {
    pub id: MechanismId,
    pub sources: Vec<MechanismSource>,
    pub stratum: Option<WireModelId>,
    pub body: MechanismBody,
}

impl MechanismSpec {
    pub fn kind(&self) -> MechanismKind {
        match &self.body {
            MechanismBody::Morphotactics(_) => MechanismKind::Morphotactics,
            MechanismBody::StaticPartition(_) => MechanismKind::StaticPartition,
            MechanismBody::OrderedPhonology(_) => MechanismKind::OrderedPhonology,
            MechanismBody::StructuralAllomorph(_) => MechanismKind::StructuralAllomorph,
            MechanismBody::CopyProcess(_) => MechanismKind::CopyProcess,
            MechanismBody::BoundaryCleanup(_) => MechanismKind::BoundaryCleanup,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MechanismEdge {
    pub producer: MechanismId,
    pub consumer: MechanismId,
    pub contract: InterfaceContract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MechanismEndpoint {
    Producer,
    Consumer,
}

pub type Endpoint = MechanismEndpoint;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MechanismGraph {
    pub nodes: Vec<MechanismSpec>,
    pub edges: Vec<MechanismEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterfaceContract {
    pub provided: ProvidedInterface,
    pub required: RequiredInterface,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvidedInterface {
    pub symbol_space: SymbolSpace,
    pub analysis_identity: IdentityGuarantee,
    pub root_identity: IdentityGuarantee,
    pub multiplicity: MultiplicityGuarantee,
    pub boundaries: BoundaryGuarantee,
    pub dynamic_state: DynamicState,
    pub stratum: Option<WireModelId>,
    pub disposition: ExecutionDisposition,
    pub copy_span: CopySpanGuarantee,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredInterface {
    pub symbol_space: SymbolSpace,
    pub analysis_identity: IdentityRequirement,
    pub root_identity: IdentityRequirement,
    pub multiplicity: MultiplicityRequirement,
    pub boundaries: BoundaryRequirement,
    pub dynamic_state: DynamicState,
    pub stratum: Option<WireModelId>,
    pub accepted_dispositions: BTreeSet<ExecutionDisposition>,
    pub copy_span: CopySpanRequirement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicState {
    pub pos: BTreeSet<String>,
    pub mpr: BTreeSet<WireModelId>,
    pub lexical_classes: BTreeSet<String>,
    pub stem_families: BTreeSet<WireModelId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SymbolSpace {
    Surface(WireModelId),
    CharDefTokens(WireModelId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdentityGuarantee {
    Unknown,
    Preserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdentityRequirement {
    Any,
    Preserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MultiplicityGuarantee {
    Unknown,
    SetOnly,
    ExactMultiset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MultiplicityRequirement {
    Any,
    SetOrBetter,
    ExactMultiset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BoundaryGuarantee {
    Unknown,
    Present,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BoundaryRequirement {
    Any,
    Present,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CopySpanGuarantee {
    None,
    Bounded(usize),
    UnboundedPreserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CopySpanRequirement {
    None,
    BoundedAtMost(usize),
    AnyPreserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InterfaceField {
    Identity,
    Multiplicity,
    Boundaries,
    DynamicState,
    Stratum,
    CopySpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MechanismGraphError {
    InvalidId {
        id: MechanismId,
    },
    DuplicateId {
        id: MechanismId,
    },
    InvalidWireId {
        mechanism: MechanismId,
        field: String,
        id: WireModelId,
        expected: WireModelKind,
    },
    MissingEndpoint {
        edge: MechanismEdge,
        endpoint: MechanismEndpoint,
    },
    Cycle {
        members: Vec<MechanismId>,
    },
    CleanupNotTerminal {
        cleanup: MechanismId,
    },
    PathDoesNotTerminateInCleanup {
        mechanism: MechanismId,
    },
    CleanupContractMismatch {
        cleanup: MechanismId,
        table: WireModelId,
        contract_space: SymbolSpace,
    },
    SymbolSpaceMismatch {
        producer: MechanismId,
        consumer: MechanismId,
    },
    UnsatisfiedState {
        producer: MechanismId,
        consumer: MechanismId,
        field: InterfaceField,
        detail: String,
    },
    DispositionMismatch {
        producer: MechanismId,
        consumer: MechanismId,
    },
}

impl fmt::Display for MechanismGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MechanismGraphError {}

impl WireModelId {
    fn has_valid_range(&self) -> bool {
        let max = match self.kind {
            WireModelKind::CharDef
            | WireModelKind::MRule
            | WireModelKind::PRule
            | WireModelKind::Template
            | WireModelKind::LexEntry
            | WireModelKind::Morpheme
            | WireModelKind::Allomorph
            | WireModelKind::NatClass
            | WireModelKind::StemName
            | WireModelKind::Family => u32::MAX as u64,
            WireModelKind::Stratum | WireModelKind::Mpr => u8::MAX as u64,
            WireModelKind::Table | WireModelKind::Var => u16::MAX as u64,
            WireModelKind::MprGroup
            | WireModelKind::SubruleIndex
            | WireModelKind::AllomorphIndex
            | WireModelKind::MorphemeIndex => u64::MAX,
        };
        self.value <= max
    }
}

fn validate_wire_id(
    mechanism: &MechanismId,
    field: impl Into<String>,
    id: &WireModelId,
    expected: WireModelKind,
) -> Result<(), MechanismGraphError> {
    if id.kind != expected || !id.has_valid_range() {
        return Err(MechanismGraphError::InvalidWireId {
            mechanism: mechanism.clone(),
            field: field.into(),
            id: id.clone(),
            expected,
        });
    }
    Ok(())
}

fn validate_dynamic_state_ids(
    mechanism: &MechanismId,
    prefix: &str,
    state: &DynamicState,
) -> Result<(), MechanismGraphError> {
    for id in &state.mpr {
        validate_wire_id(mechanism, format!("{prefix}.mpr"), id, WireModelKind::Mpr)?;
    }
    for id in &state.stem_families {
        validate_wire_id(
            mechanism,
            format!("{prefix}.stem_families"),
            id,
            WireModelKind::Family,
        )?;
    }
    Ok(())
}

fn validate_partition_predicate(
    mechanism: &MechanismId,
    field: &str,
    predicate: &PartitionPredicate,
) -> Result<(), MechanismGraphError> {
    match predicate {
        PartitionPredicate::Pos(_) | PartitionPredicate::LexicalClass(_) => Ok(()),
        PartitionPredicate::Mpr(id) => validate_wire_id(mechanism, field, id, WireModelKind::Mpr),
        PartitionPredicate::StemFamily(id) => {
            validate_wire_id(mechanism, field, id, WireModelKind::Family)
        }
    }
}

fn validate_mechanism_ids(node: &MechanismSpec) -> Result<(), MechanismGraphError> {
    if let Some(stratum) = &node.stratum {
        validate_wire_id(&node.id, "stratum", stratum, WireModelKind::Stratum)?;
    }
    for source in &node.sources {
        let (owner_kind, child_kind) = match source.kind {
            MechanismSourceKind::MorphRule => (Some(WireModelKind::MRule), None),
            MechanismSourceKind::AffixAllomorph => (
                Some(WireModelKind::MRule),
                Some(WireModelKind::AllomorphIndex),
            ),
            MechanismSourceKind::Stratum => (Some(WireModelKind::Stratum), None),
            MechanismSourceKind::MprGroup => (Some(WireModelKind::MprGroup), None),
            MechanismSourceKind::PhonRule => (Some(WireModelKind::PRule), None),
            MechanismSourceKind::RewriteSubrule => (
                Some(WireModelKind::PRule),
                Some(WireModelKind::SubruleIndex),
            ),
            MechanismSourceKind::NaturalClass => (Some(WireModelKind::NatClass), None),
            MechanismSourceKind::MorphemeCoOccurrence => (Some(WireModelKind::MorphemeIndex), None),
            MechanismSourceKind::AllomorphCoOccurrence => (Some(WireModelKind::Allomorph), None),
        };
        match (&source.owner, owner_kind) {
            (Some(id), Some(kind)) => validate_wire_id(&node.id, "sources.owner", id, kind)?,
            _ => {
                return Err(MechanismGraphError::InvalidWireId {
                    mechanism: node.id.clone(),
                    field: "sources.owner".to_owned(),
                    id: source.owner.clone().unwrap_or(WireModelId {
                        kind: owner_kind.unwrap_or(WireModelKind::MRule),
                        value: 0,
                    }),
                    expected: owner_kind.unwrap_or(WireModelKind::MRule),
                });
            }
        }
        match (&source.child, child_kind) {
            (Some(id), Some(kind)) => validate_wire_id(&node.id, "sources.child", id, kind)?,
            (None, None) => {}
            (Some(id), None) => {
                return Err(MechanismGraphError::InvalidWireId {
                    mechanism: node.id.clone(),
                    field: "sources.child".to_owned(),
                    id: id.clone(),
                    expected: id.kind,
                });
            }
            (None, Some(kind)) => {
                return Err(MechanismGraphError::InvalidWireId {
                    mechanism: node.id.clone(),
                    field: "sources.child".to_owned(),
                    id: WireModelId { kind, value: 0 },
                    expected: kind,
                });
            }
        }
    }

    match &node.body {
        MechanismBody::Morphotactics(spec) => {
            for id in &spec.strata {
                validate_wire_id(&node.id, "morphotactics.strata", id, WireModelKind::Stratum)?;
            }
            for id in &spec.templates {
                validate_wire_id(
                    &node.id,
                    "morphotactics.templates",
                    id,
                    WireModelKind::Template,
                )?;
            }
            for id in &spec.rules {
                validate_wire_id(&node.id, "morphotactics.rules", id, WireModelKind::MRule)?;
            }
            for chain in &spec.priority_chains {
                for id in chain {
                    validate_wire_id(
                        &node.id,
                        "morphotactics.priority_chains",
                        id,
                        WireModelKind::Allomorph,
                    )?;
                }
            }
        }
        MechanismBody::StaticPartition(spec) => {
            for predicate in &spec.predicates {
                validate_partition_predicate(&node.id, "static_partition.predicates", predicate)?;
            }
            for group in &spec.groups {
                for predicate in &group.predicates {
                    validate_partition_predicate(
                        &node.id,
                        "static_partition.groups.predicates",
                        predicate,
                    )?;
                }
                for member in &group.members {
                    if !member.has_valid_range() {
                        return Err(MechanismGraphError::InvalidWireId {
                            mechanism: node.id.clone(),
                            field: "static_partition.groups.members".to_owned(),
                            id: member.clone(),
                            expected: member.kind,
                        });
                    }
                }
            }
        }
        MechanismBody::OrderedPhonology(spec) => {
            validate_wire_id(
                &node.id,
                "ordered_phonology.stratum",
                &spec.stratum,
                WireModelKind::Stratum,
            )?;
            for atom in &spec.rules {
                let rule = match atom {
                    OrderedRuleAtom::Rewrite { rule }
                    | OrderedRuleAtom::Metathesis { rule, .. } => rule,
                };
                validate_wire_id(
                    &node.id,
                    "ordered_phonology.rules",
                    rule,
                    WireModelKind::PRule,
                )?;
            }
        }
        MechanismBody::StructuralAllomorph(spec) => {
            validate_wire_id(
                &node.id,
                "structural_allomorph.rule",
                &spec.rule,
                WireModelKind::MRule,
            )?;
            for id in &spec.allomorphs {
                validate_wire_id(
                    &node.id,
                    "structural_allomorph.allomorphs",
                    id,
                    WireModelKind::Allomorph,
                )?;
            }
        }
        MechanismBody::CopyProcess(spec) => validate_wire_id(
            &node.id,
            "copy_process.rule",
            &spec.rule,
            WireModelKind::MRule,
        )?,
        MechanismBody::BoundaryCleanup(spec) => validate_wire_id(
            &node.id,
            "boundary_cleanup.table",
            &spec.table,
            WireModelKind::Table,
        )?,
    }
    Ok(())
}

fn validate_contract_ids(edge: &MechanismEdge) -> Result<(), MechanismGraphError> {
    let provided_table = match &edge.contract.provided.symbol_space {
        SymbolSpace::Surface(id) | SymbolSpace::CharDefTokens(id) => id,
    };
    let required_table = match &edge.contract.required.symbol_space {
        SymbolSpace::Surface(id) | SymbolSpace::CharDefTokens(id) => id,
    };
    validate_wire_id(
        &edge.producer,
        "contract.provided.symbol_space",
        provided_table,
        WireModelKind::Table,
    )?;
    validate_wire_id(
        &edge.consumer,
        "contract.required.symbol_space",
        required_table,
        WireModelKind::Table,
    )?;
    if let Some(stratum) = &edge.contract.provided.stratum {
        validate_wire_id(
            &edge.producer,
            "contract.provided.stratum",
            stratum,
            WireModelKind::Stratum,
        )?;
    }
    if let Some(stratum) = &edge.contract.required.stratum {
        validate_wire_id(
            &edge.consumer,
            "contract.required.stratum",
            stratum,
            WireModelKind::Stratum,
        )?;
    }
    validate_dynamic_state_ids(
        &edge.producer,
        "contract.provided.dynamic_state",
        &edge.contract.provided.dynamic_state,
    )?;
    validate_dynamic_state_ids(
        &edge.consumer,
        "contract.required.dynamic_state",
        &edge.contract.required.dynamic_state,
    )?;
    Ok(())
}

impl MechanismGraph {
    pub fn validate(&self) -> Result<(), MechanismGraphError> {
        let mut nodes = BTreeMap::new();
        for node in &self.nodes {
            if node.id.0.trim().is_empty() {
                return Err(MechanismGraphError::InvalidId {
                    id: node.id.clone(),
                });
            }
            if nodes.insert(node.id.clone(), node).is_some() {
                return Err(MechanismGraphError::DuplicateId {
                    id: node.id.clone(),
                });
            }
            validate_mechanism_ids(node)?;
        }

        for edge in &self.edges {
            if !nodes.contains_key(&edge.producer) {
                return Err(MechanismGraphError::MissingEndpoint {
                    edge: edge.clone(),
                    endpoint: MechanismEndpoint::Producer,
                });
            }
            if !nodes.contains_key(&edge.consumer) {
                return Err(MechanismGraphError::MissingEndpoint {
                    edge: edge.clone(),
                    endpoint: MechanismEndpoint::Consumer,
                });
            }
            validate_contract_ids(edge)?;
        }

        let mut indegree: BTreeMap<MechanismId, usize> =
            nodes.keys().cloned().map(|id| (id, 0)).collect();
        let mut outgoing: BTreeMap<MechanismId, Vec<MechanismId>> = BTreeMap::new();
        for edge in &self.edges {
            *indegree.get_mut(&edge.consumer).expect("endpoint checked") += 1;
            outgoing
                .entry(edge.producer.clone())
                .or_default()
                .push(edge.consumer.clone());
        }
        let mut ready: BTreeSet<MechanismId> = indegree
            .iter()
            .filter_map(|(id, degree)| (*degree == 0).then_some(id.clone()))
            .collect();
        let mut visited = 0;
        while let Some(id) = ready.pop_first() {
            visited += 1;
            for consumer in outgoing.get(&id).into_iter().flatten() {
                let degree = indegree.get_mut(consumer).expect("endpoint checked");
                *degree -= 1;
                if *degree == 0 {
                    ready.insert(consumer.clone());
                }
            }
        }
        if visited != nodes.len() {
            let residual: BTreeSet<_> = indegree
                .iter()
                .filter_map(|(id, degree)| (*degree > 0).then_some(id.clone()))
                .collect();
            return Err(MechanismGraphError::Cycle {
                members: cyclic_members(&residual, &outgoing),
            });
        }

        for node in &self.nodes {
            if node.kind() == MechanismKind::BoundaryCleanup
                && self.edges.iter().any(|edge| edge.producer == node.id)
            {
                return Err(MechanismGraphError::CleanupNotTerminal {
                    cleanup: node.id.clone(),
                });
            }
        }

        let cleanup_ids: BTreeSet<_> = self
            .nodes
            .iter()
            .filter(|node| node.kind() == MechanismKind::BoundaryCleanup)
            .map(|node| node.id.clone())
            .collect();
        if !cleanup_ids.is_empty() {
            let mut reverse: BTreeMap<MechanismId, Vec<MechanismId>> = BTreeMap::new();
            for edge in &self.edges {
                reverse
                    .entry(edge.consumer.clone())
                    .or_default()
                    .push(edge.producer.clone());
            }
            let mut reaches_cleanup = cleanup_ids.clone();
            let mut pending: Vec<_> = cleanup_ids.iter().cloned().collect();
            while let Some(consumer) = pending.pop() {
                for producer in reverse.get(&consumer).into_iter().flatten() {
                    if reaches_cleanup.insert(producer.clone()) {
                        pending.push(producer.clone());
                    }
                }
            }
            if let Some(node) = self
                .nodes
                .iter()
                .find(|node| !reaches_cleanup.contains(&node.id))
            {
                return Err(MechanismGraphError::PathDoesNotTerminateInCleanup {
                    mechanism: node.id.clone(),
                });
            }
        }

        for edge in &self.edges {
            let producer = nodes.get(&edge.producer).expect("endpoint checked");
            let consumer = nodes.get(&edge.consumer).expect("endpoint checked");
            if edge.contract.provided.stratum != producer.stratum
                || edge.contract.required.stratum != consumer.stratum
            {
                return Err(MechanismGraphError::UnsatisfiedState {
                    producer: producer.id.clone(),
                    consumer: consumer.id.clone(),
                    field: InterfaceField::Stratum,
                    detail: "contract stratum is not tied to its mechanism".to_owned(),
                });
            }
            if edge.contract.provided.symbol_space != edge.contract.required.symbol_space {
                return Err(MechanismGraphError::SymbolSpaceMismatch {
                    producer: producer.id.clone(),
                    consumer: consumer.id.clone(),
                });
            }
            if let MechanismBody::BoundaryCleanup(spec) = &consumer.body {
                let contract_space = edge.contract.required.symbol_space.clone();
                let contract_table = match &contract_space {
                    SymbolSpace::Surface(table) | SymbolSpace::CharDefTokens(table) => table,
                };
                if contract_table != &spec.table {
                    return Err(MechanismGraphError::CleanupContractMismatch {
                        cleanup: consumer.id.clone(),
                        table: spec.table.clone(),
                        contract_space,
                    });
                }
            }
            edge.contract.validate(&producer.id, &consumer.id).map_err(
                |failure| match failure {
                    ContractFailure::State { field, detail } => {
                        MechanismGraphError::UnsatisfiedState {
                            producer: producer.id.clone(),
                            consumer: consumer.id.clone(),
                            field,
                            detail,
                        }
                    }
                    ContractFailure::Disposition => MechanismGraphError::DispositionMismatch {
                        producer: producer.id.clone(),
                        consumer: consumer.id.clone(),
                    },
                },
            )?;
        }
        Ok(())
    }
}

fn cyclic_members(
    residual: &BTreeSet<MechanismId>,
    outgoing: &BTreeMap<MechanismId, Vec<MechanismId>>,
) -> Vec<MechanismId> {
    residual
        .iter()
        .filter(|start| {
            let mut seen = BTreeSet::new();
            let mut pending = outgoing.get(*start).cloned().unwrap_or_default();
            while let Some(node) = pending.pop() {
                if &node == *start {
                    return true;
                }
                if residual.contains(&node) && seen.insert(node.clone()) {
                    pending.extend(outgoing.get(&node).into_iter().flatten().cloned());
                }
            }
            false
        })
        .cloned()
        .collect()
}

enum ContractFailure {
    State {
        field: InterfaceField,
        detail: String,
    },
    Disposition,
}

impl InterfaceContract {
    fn validate(
        &self,
        _producer: &MechanismId,
        _consumer: &MechanismId,
    ) -> Result<(), ContractFailure> {
        if !identity_satisfies(
            self.provided.analysis_identity,
            self.required.analysis_identity,
        ) || !identity_satisfies(self.provided.root_identity, self.required.root_identity)
        {
            return Err(ContractFailure::State {
                field: InterfaceField::Identity,
                detail: "analysis/root identity is not preserved".to_owned(),
            });
        }
        if !multiplicity_satisfies(self.provided.multiplicity, self.required.multiplicity) {
            return Err(ContractFailure::State {
                field: InterfaceField::Multiplicity,
                detail: "producer multiplicity is weaker than the consumer requirement".to_owned(),
            });
        }
        if !boundary_satisfies(self.provided.boundaries, self.required.boundaries) {
            return Err(ContractFailure::State {
                field: InterfaceField::Boundaries,
                detail: "boundary state does not match".to_owned(),
            });
        }
        if !dynamic_state_satisfies(&self.provided.dynamic_state, &self.required.dynamic_state) {
            return Err(ContractFailure::State {
                field: InterfaceField::DynamicState,
                detail: "producer dynamic state is not a superset".to_owned(),
            });
        }
        if self.required.stratum.is_some() && self.provided.stratum != self.required.stratum {
            return Err(ContractFailure::State {
                field: InterfaceField::Stratum,
                detail: "stratum does not match".to_owned(),
            });
        }
        if !copy_span_satisfies(self.provided.copy_span, self.required.copy_span) {
            return Err(ContractFailure::State {
                field: InterfaceField::CopySpan,
                detail: "copy span guarantee is insufficient".to_owned(),
            });
        }
        if self.provided.disposition == ExecutionDisposition::Refused
            || !self
                .required
                .accepted_dispositions
                .contains(&self.provided.disposition)
        {
            return Err(ContractFailure::Disposition);
        }
        Ok(())
    }
}

fn identity_satisfies(provided: IdentityGuarantee, required: IdentityRequirement) -> bool {
    matches!(required, IdentityRequirement::Any) || matches!(provided, IdentityGuarantee::Preserved)
}

fn multiplicity_satisfies(
    provided: MultiplicityGuarantee,
    required: MultiplicityRequirement,
) -> bool {
    let provided_strength = match provided {
        MultiplicityGuarantee::Unknown => 0,
        MultiplicityGuarantee::SetOnly => 1,
        MultiplicityGuarantee::ExactMultiset => 2,
    };
    let required_strength = match required {
        MultiplicityRequirement::Any => 0,
        MultiplicityRequirement::SetOrBetter => 1,
        MultiplicityRequirement::ExactMultiset => 2,
    };
    provided_strength >= required_strength
}

fn boundary_satisfies(provided: BoundaryGuarantee, required: BoundaryRequirement) -> bool {
    matches!(required, BoundaryRequirement::Any)
        || matches!(
            (provided, required),
            (BoundaryGuarantee::Present, BoundaryRequirement::Present)
                | (BoundaryGuarantee::Removed, BoundaryRequirement::Removed)
        )
}

fn dynamic_state_satisfies(provided: &DynamicState, required: &DynamicState) -> bool {
    provided.pos.is_superset(&required.pos)
        && provided.mpr.is_superset(&required.mpr)
        && provided
            .lexical_classes
            .is_superset(&required.lexical_classes)
        && provided.stem_families.is_superset(&required.stem_families)
}

fn copy_span_satisfies(provided: CopySpanGuarantee, required: CopySpanRequirement) -> bool {
    match required {
        CopySpanRequirement::None => matches!(provided, CopySpanGuarantee::None),
        CopySpanRequirement::BoundedAtMost(max) => match provided {
            CopySpanGuarantee::None => true,
            CopySpanGuarantee::Bounded(n) => n <= max,
            CopySpanGuarantee::UnboundedPreserved => false,
        },
        CopySpanRequirement::AnyPreserved => {
            matches!(
                provided,
                CopySpanGuarantee::Bounded(_) | CopySpanGuarantee::UnboundedPreserved
            )
        }
    }
}
