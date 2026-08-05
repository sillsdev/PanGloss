//! The typed mechanism graph: six language-name-free mechanism kinds, the dependency/order edges
//! between them, and the strategy-attributed bindings that say what executing one actually costs.
//!
//! # Why this vocabulary, and not a node-plus-hand-written-contract shape
//! A node modelled with a hand-written `InterfaceContract` on every edge has three defects that
//! this vocabulary is built to make impossible, not just avoid.
//!
//! **1. Duplicate wire provenance.** Writing the same model id down on both halves of an edge (or
//! repeating a node's own source list in the edge body) invites the copies to drift, and a
//! validator that then *asserts* the copies are equal is not validation -- it is a consistency
//! check on a redundancy that should not exist. Provenance is written exactly once: **typed source
//! references live in [`MechanismNode::sources`]**, the active table lives in
//! [`MechanismNode::symbol_space`], and the stratum lives in [`MechanismNode::stratum`]. Bodies
//! carry only what those cannot express.
//!
//! **2. Unproved blanket contracts.** A type like `IdentityGuarantee` or `MultiplicityGuarantee` is
//! a *declaration*: whoever writes the edge writes `Preserved`/`ExactMultiset`, and a validator
//! that only confirms `Preserved` satisfies `Preserved` proves nothing. Measured case: a candidate
//! 2.2x cheaper than the winner was `identity-mismatch`ed at runtime -- a declared "identity is
//! preserved" on that edge would have been simply false, and the graph would have validated
//! anyway. Analysis identity and multiplicity are the **parity relation**, established by measuring
//! a candidate against an oracle, never by an annotation on an edge, so this vocabulary does not
//! represent them at all. Nothing in this module ranks or selects, so nothing needs them.
//!
//! **3. Guarantees that did not name whose guarantee they were.**
//! [`crate::capability::Disposition::ConfirmOnly`] is defined as *"recall-preserving only if the
//! proposer proposes the superset"* -- a claim about a **proposer**, not about a grammar. Without a
//! place to name which compiler makes that claim, one compiler's ability can be inherited by every
//! compiler that touches the same construct: `Compounding` rested at `ConfirmOnly` grammar-wide
//! while [`crate::uflexc`], the only lexicon emitter
//! [`crate::enumerate::EmissionStrategy::PlanComposed`] has, could not propose a single compound.
//!
//! So this vocabulary makes that inexpressible. A [`MechanismNode`] owns **requirements**, never
//! recall guarantees: [`MechanismNode::construct_requirements`] is a set of
//! [`CharacteristicKind`]s -- "faithfully executing this mechanism requires the compiler to
//! represent these constructs." The ONLY type in this module that expresses what a compiler can
//! actually deliver is [`MechanismBinding`], whose [`MechanismBinding::strategy`] is mandatory,
//! whose fields are private, and whose only constructor is [`MechanismBinding::derive`]. There is
//! no way to write down an [`ExecutionDisposition`] without naming the [`EmissionStrategy`] it
//! belongs to, and no way to write one down by hand at all.
//!
//! # How requirements connect to [`crate::strategy_coverage`] without restating it
//! `strategy_coverage` already owns the 3 x 22 `(EmissionStrategy, CharacteristicKind)` account,
//! exhaustively matched so a new compiler or construct breaks the build. This module does not hold
//! a second copy, a subset, or a summary of it. A node's requirements are expressed **in
//! `CharacteristicKind`**, exactly the key that table is indexed by, and
//! [`MechanismBinding::derive`] resolves them by calling
//! [`crate::strategy_coverage::representation_of`] once per requirement and taking the meet. Adding
//! a construct or a compiler still breaks `strategy_coverage`'s build first, and every mechanism's
//! answer moves with it automatically.
//!
//! Two node-owned facts are deliberately NOT expressed as `CharacteristicKind` because they are
//! genuinely a different question: [`SymbolSpace`] and [`BoundaryState`]. `strategy_coverage`
//! answers "can compiler S represent construct K", which takes no position on whether the boundary
//! symbols are still present at the point one mechanism reads another's output, or on which
//! character-definition table the two are speaking. Those are properties of a mechanism's
//! **position in the composition**, with no compiler input at all, and they are what
//! [`MechanismGraph::validate`] checks along an edge. They are structural facts, not recall claims,
//! and they are named accordingly -- `state`/`space`, never `guarantee`.
//!
//! # No node names a plan shape
//! Wave 3 measured plan-shape permutation to vary nothing: on Sena two different families with two
//! different transforms produced bit-identical networks (2044 states / 21114 arcs), and on
//! Indonesian all five plan-composed permutations scored bit-identically. Accordingly there is no
//! node, body field, or edge attribute here that names a family, a topology, or a permutation. The
//! order mechanisms compose in is a single canonical spine ([`MechanismKind::COMPOSITION_ORDER`]),
//! not an axis.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{
    AllomorphId, FamilyId, LexEntryId, MRuleId, MorphemeId, MprId, NatClassId, PRuleId, StemNameId,
    StratumId, TableId, TemplateId, VarId,
};
use serde::{Deserialize, Serialize};

use crate::capability::{CharacteristicKind, ModelLocation};
use crate::enumerate::EmissionStrategy;
use crate::strategy_coverage::{representation_of, StrategyCoverageRow, StrategyRepresentation};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MechanismId(pub String);

// ---------------------------------------------------------------------------------------------
// Typed source references (wire domain)
// ---------------------------------------------------------------------------------------------

/// The model-id domain a [`WireModelId`] belongs to. Kept from the initial commit unchanged: it is
/// the type-tag half of a typed source reference, and it is what stops a `PRuleId` being read back
/// as an `MRuleId` across a serialization boundary.
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

/// What kind of authored model object a mechanism was derived from.
///
/// Every variant except [`Self::CharacterTable`] is the image of a
/// [`crate::capability::ModelLocation`] variant under the `From` impl below -- which is the join
/// that lets a mechanism provider attribute a [`crate::capability::CharacteristicObservation`] to a
/// mechanism without ever re-reading the `Grammar`.
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
    /// The active `CharacterDefinitionTable`. The one source kind with no [`ModelLocation`]
    /// counterpart: [`MechanismKind::BoundaryCleanup`] is derived from the character table it
    /// cleans, not from an observed construct, and a node with no typed source at all is rejected
    /// by [`MechanismGraph::validate`].
    CharacterTable,
}

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

impl MechanismSource {
    /// The `CharacterDefinitionTable` source for a [`MechanismKind::BoundaryCleanup`] node.
    pub fn character_table(table: TableId) -> Self {
        Self {
            kind: MechanismSourceKind::CharacterTable,
            owner: Some(table.into()),
            child: None,
        }
    }

    /// The wire domains this source kind's `owner`/`child` must belong to. `None` for a slot that
    /// must be absent.
    fn expected_domains(
        kind: MechanismSourceKind,
    ) -> (Option<WireModelKind>, Option<WireModelKind>) {
        match kind {
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
            MechanismSourceKind::CharacterTable => (Some(WireModelKind::Table), None),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The six mechanism kinds
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MechanismKind {
    StaticPartition,
    Morphotactics,
    StructuralAllomorph,
    CopyProcess,
    OrderedPhonology,
    BoundaryCleanup,
}

impl MechanismKind {
    /// The ONE canonical composition order, from the subrecipe dossiers: a static lexical partition
    /// selects the entry subset, morphotactics assembles complete legal analyses over it, structural
    /// allomorphy applies local structural actions to the assembled form, copy processes copy spans
    /// of it, ordered phonology lowers the result, and boundary cleanup terminally consumes the
    /// boundary symbols every earlier mechanism needed to see.
    ///
    /// This is deliberately a fixed order and NOT an axis. Wave 3 measured plan-shape permutation to
    /// change nothing observable (bit-identical networks on Sena, bit-identical scores on
    /// Indonesian), so a vocabulary that let this order vary would be modelling something that does
    /// not exist. It is also what makes a derived graph's identity canonical: two derivations of the
    /// same grammar cannot differ by node order.
    pub const COMPOSITION_ORDER: &'static [MechanismKind] = &[
        MechanismKind::StaticPartition,
        MechanismKind::Morphotactics,
        MechanismKind::StructuralAllomorph,
        MechanismKind::CopyProcess,
        MechanismKind::OrderedPhonology,
        MechanismKind::BoundaryCleanup,
    ];

    /// Stable identifier for reports, ids and serialized artifacts.
    pub fn label(self) -> &'static str {
        match self {
            Self::StaticPartition => "static-partition",
            Self::Morphotactics => "morphotactics",
            Self::StructuralAllomorph => "structural-allomorph",
            Self::CopyProcess => "copy-process",
            Self::OrderedPhonology => "ordered-phonology",
            Self::BoundaryCleanup => "boundary-cleanup",
        }
    }
}

/// Which mechanism owns a given construct.
///
/// Exhaustively matched with no catch-all, the same discipline
/// [`crate::strategy_coverage::representation_of`] and [`crate::capability::characterize`] hold
/// themselves to: adding a [`CharacteristicKind`] breaks this build until a reviewer assigns it to
/// a mechanism. That compile break is the mechanism -- a new construct cannot silently land in
/// whichever node happens to be nearest.
pub fn mechanism_kind_for(kind: CharacteristicKind) -> MechanismKind {
    use CharacteristicKind::*;
    match kind {
        // Morphotactics owns complete morphological alternatives: which analyses are legal before
        // phonological lowering (its dossier's scope paragraph). Rule kind, rule ordering,
        // co-occurrence units, lexical continuation restrictions and allomorph priority are all
        // statements about which complete analyses exist.
        Affixation
        | RealizationalMorphology
        | Compounding
        | OrderedMorphRuleApplication
        | UnorderedMorphRuleApplication
        | CoOccurrenceConstraint
        | StemName
        | FreeFluctuation => MechanismKind::Morphotactics,

        // A static partition is a lexical/MPR split fixed for the compilation's lifetime.
        // `SubruleGating` is literally what `crate::gate` partitions on; MPR-group append/overwrite
        // is the same shape of fixed state split.
        SubruleGating | MprGroupAppend | MprGroupOverwrite => MechanismKind::StaticPartition,

        // Ordered phonology owns the compiled rewrite cascade: mode, direction, metathesis,
        // epenthesis, quantified patterns, and the natural classes those patterns are written over.
        IterativeRewrite
        | SimultaneousRewrite
        | LeftToRightRewrite
        | RightToLeftRewrite
        | Metathesis
        | Epenthesis
        | QuantifierPattern
        | NaturalClassDefinition => MechanismKind::OrderedPhonology,

        // A local structural action on an assembled form: multi-part LHS with dropped material.
        CircumfixOutputAction => MechanismKind::StructuralAllomorph,

        // Copying a span of the stem.
        Reduplication => MechanismKind::CopyProcess,

        // JUDGMENT CALL, recorded rather than hidden. The work of threading a per-rule owning table
        // lives in `crate::replace` (i.e. in the ordered-phonology cascade), which argues for
        // `OrderedPhonology`. It is assigned to `BoundaryCleanup` instead on a structural ground:
        // cleanup is the only mechanism in this vocabulary that is *table-parameterized* (its
        // symbol space is the identity `validate` checks along every incident edge, and its own
        // dossier's invariant is "table/symbol-space identity is preserved"), whereas the
        // ordered-phonology node is stratum-parameterized. Putting the "there is more than one
        // table to get right" fact on the node whose contract IS table identity keeps the two
        // together. Revisit if a later slice makes phonology nodes table-parameterized.
        MultiTable => MechanismKind::BoundaryCleanup,
    }
}

// ---------------------------------------------------------------------------------------------
// Bodies -- only what sources, symbol space and stratum cannot already express
// ---------------------------------------------------------------------------------------------

/// Morphotactics' non-provenance facts.
///
/// `templates` is NOT duplicate provenance: a template is not the source of an observed construct
/// (no `ModelLocation` names one), so this is the only place a template id appears. `max_depth` is
/// [`crate::capability::GrammarCardinality::max_derivation_chain_depth`], which is honestly `None`
/// today -- carried as an `Option` rather than guessed, exactly as that field's own doc requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MorphotacticsSpec {
    pub templates: Vec<WireModelId>,
    pub max_depth: Option<usize>,
}

/// One partition group, as [`crate::gate::partition_entries`] actually computed it (projected
/// through [`crate::grammar_semantics::GrammarSemantics::entry_partition`], which is the
/// deterministically-ordered owner of that answer).
///
/// The gate key IS the group's identity -- there is no separate `id` string, and no
/// `PartitionPredicate` list. The initial commit's `PartitionPredicate` enum (`Pos`/`Mpr`/
/// `LexicalClass`/`StemFamily`) was never populated by anything and could not be: the real
/// partition mechanism does not expose per-group predicates, only the boolean key vector of which
/// gated subrules apply. A predicate list nobody can derive is a declaration, not a fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionGroupSpec {
    /// One bool per gated subrule, in
    /// [`crate::grammar_semantics::GrammarSemantics::gated_subrules`] order.
    pub key: Vec<bool>,
    /// The group's lexical entries, sorted (the real mechanism collects into a `HashSet`).
    pub members: Vec<WireModelId>,
}

/// The authored cascade order of the phonological rules this mechanism runs.
///
/// Order is load-bearing and is never canonicalized (see [`crate::grammar_semantics`]'s "authored
/// order is preserved" note). The initial commit's `OrderedRuleAtom` enum is gone: its
/// `Rewrite`/`Metathesis` distinction is already carried, per compiler, by the
/// [`CharacteristicKind::Metathesis`] requirement resolving through [`crate::strategy_coverage`],
/// and its `swap_construction_attempted: bool` was an unproved declaration about a compile attempt
/// that had not happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedPhonologySpec {
    pub rule_order: Vec<WireModelId>,
}

/// The boundary symbols this terminal mechanism consumes, from the active table's
/// `CharDefKind::Boundary` definitions. The table itself is the node's
/// [`MechanismNode::symbol_space`], not a field here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryCleanupSpec {
    pub boundary_symbols: Vec<String>,
}

/// The per-kind payload.
///
/// [`Self::StructuralAllomorph`] and [`Self::CopyProcess`] are deliberately payload-free. Their
/// initial-commit specs carried `rule`/`allomorphs` (duplicate provenance -- already in
/// [`MechanismNode::sources`]) plus `bounded_local_shape`, `CopyKind`, `max_span` and
/// `max_chain_depth`, none of which any semantic owner can derive today. An empty body is the
/// honest shape: everything currently knowable about these two mechanisms is their typed sources
/// and their construct requirements. The bounded-vs-unbounded-copy axis needs a real derivation
/// before it can be modelled; inventing the field now would re-create exactly the unproved blanket
/// contract this vocabulary avoids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "spec", rename_all = "kebab-case")]
pub enum MechanismBody {
    StaticPartition(Vec<PartitionGroupSpec>),
    Morphotactics(MorphotacticsSpec),
    StructuralAllomorph,
    CopyProcess,
    OrderedPhonology(OrderedPhonologySpec),
    BoundaryCleanup(BoundaryCleanupSpec),
}

// ---------------------------------------------------------------------------------------------
// Position facts: symbol space and boundary state (NOT recall guarantees)
// ---------------------------------------------------------------------------------------------

/// Which symbol alphabet a mechanism reads and writes, and in which character-definition table.
///
/// A composition fact, not a recall claim: it says nothing about what any compiler can represent,
/// so it is deliberately not expressed in [`CharacteristicKind`] and does not resolve through
/// [`crate::strategy_coverage`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SymbolSpace {
    Surface(WireModelId),
    CharDefTokens(WireModelId),
}

impl SymbolSpace {
    pub fn table(&self) -> &WireModelId {
        match self {
            Self::Surface(table) | Self::CharDefTokens(table) => table,
        }
    }
}

/// Whether boundary/marker symbols are still present in a mechanism's symbol stream.
///
/// Two-point and derived, never declared: exactly one mechanism kind
/// ([`MechanismKind::BoundaryCleanup`]) transitions `Present` to `Removed`, and every mechanism
/// requires `Present` on input because every one of them may need to see a boundary. That single
/// rule is what makes "all boundary-consuming consumers run before cleanup" (the cleanup dossier's
/// first invariant) a structural property of the graph rather than a review checklist item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BoundaryState {
    Present,
    Removed,
}

/// Which position fact an edge failed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InterfaceField {
    Boundaries,
    Stratum,
}

// ---------------------------------------------------------------------------------------------
// The node
// ---------------------------------------------------------------------------------------------

/// One mechanism.
///
/// Owns its typed source references, its position in the symbol pipeline, and the typed
/// **requirements** that decide whether a given compiler can represent it faithfully. It owns NO
/// recall guarantee: see [`MechanismBinding`], which is the only type here that can express one,
/// and cannot express one anonymously.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MechanismNode {
    pub id: MechanismId,
    /// Where in the authored model this mechanism came from. Never empty (enforced by
    /// [`MechanismGraph::validate`]): a mechanism with no source is a mechanism nobody can justify.
    pub sources: Vec<MechanismSource>,
    /// The symbol alphabet and active table this mechanism reads and writes.
    pub symbol_space: SymbolSpace,
    /// The stratum this mechanism is scoped to, or `None` for a grammar-wide mechanism. Written
    /// exactly once: no body field and no edge repeats it.
    pub stratum: Option<WireModelId>,
    /// Every construct the compiler must be able to represent for this mechanism to execute
    /// faithfully. Expressed in [`CharacteristicKind`] precisely so it resolves through
    /// [`crate::strategy_coverage`]'s existing 3 x 22 table rather than restating any of it.
    pub construct_requirements: BTreeSet<CharacteristicKind>,
    pub body: MechanismBody,
}

impl MechanismNode {
    pub fn kind(&self) -> MechanismKind {
        match &self.body {
            MechanismBody::StaticPartition(_) => MechanismKind::StaticPartition,
            MechanismBody::Morphotactics(_) => MechanismKind::Morphotactics,
            MechanismBody::StructuralAllomorph => MechanismKind::StructuralAllomorph,
            MechanismBody::CopyProcess => MechanismKind::CopyProcess,
            MechanismBody::OrderedPhonology(_) => MechanismKind::OrderedPhonology,
            MechanismBody::BoundaryCleanup(_) => MechanismKind::BoundaryCleanup,
        }
    }

    /// The boundary state this mechanism needs on its input. `Present` for every kind: any
    /// mechanism may need to see a boundary, and cleanup itself needs the symbols it removes.
    pub fn boundary_input(&self) -> BoundaryState {
        BoundaryState::Present
    }

    /// The boundary state this mechanism leaves behind. Only cleanup removes.
    pub fn boundary_output(&self) -> BoundaryState {
        match self.kind() {
            MechanismKind::BoundaryCleanup => BoundaryState::Removed,
            _ => BoundaryState::Present,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The edge: dependency and order, and nothing else
// ---------------------------------------------------------------------------------------------

/// `producer`'s output is `consumer`'s input, and therefore `producer` runs first.
///
/// It carries no contract. Everything an edge used to declare is now either owned by a node (symbol
/// space, boundary state, stratum) or deleted as unprovable (identity, multiplicity, copy span,
/// dynamic state). Compatibility is COMPUTED from the two endpoints by
/// [`MechanismGraph::validate`], so an edge cannot assert a compatibility its endpoints do not
/// have.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MechanismEdge {
    pub producer: MechanismId,
    pub consumer: MechanismId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MechanismEndpoint {
    Producer,
    Consumer,
}

// ---------------------------------------------------------------------------------------------
// The candidate binding: the ONLY place an execution disposition can exist
// ---------------------------------------------------------------------------------------------

/// What executing one mechanism costs under one named compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionDisposition {
    /// Every required construct is represented by this compiler with no cited gap.
    ExactFst,
    /// At least one required construct has a documented partial gap for this compiler
    /// ([`StrategyRepresentation::RepresentsWithKnownGap`]), so the mechanism's output must be
    /// confirm-gated.
    ConfirmOnly,
    /// Executed outside the compiled FST by `crate::peel` -- the division [`crate::capability`]'s
    /// `Reduplication` arm describes, which holds for every strategy alike.
    Peeled,
    /// At least one required construct is [`StrategyRepresentation::CannotRepresent`] for this
    /// compiler: a whole-construct recall hole, so no disposition short of refusal is honest.
    Refused,
}

/// A mechanism's execution disposition **under a named [`EmissionStrategy`]**.
///
/// This is the only type in this module that expresses what a compiler can deliver, and it is
/// structurally impossible to write one down anonymously or by hand: the fields are private, the
/// only constructor is [`Self::derive`], and `derive` requires a strategy.
///
/// That is not stylistic. `Disposition::ConfirmOnly` means "recall-preserving only if the proposer
/// proposes the superset" -- a per-proposer fact. Recording it as a grammar fact is how `uflexc`
/// held a `ConfirmOnly` `Compounding` verdict while being unable to propose a single compound. A
/// guarantee that does not name whose guarantee it is, is that bug.
///
/// Deliberately NOT `Serialize`: a binding is cheap to re-`derive` from a node and a strategy, and
/// serializing one would create a second, staleable copy of an answer whose whole point is that it
/// is recomputed from [`crate::strategy_coverage`] every time that table changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MechanismBinding {
    mechanism: MechanismId,
    strategy: EmissionStrategy,
    disposition: ExecutionDisposition,
    limiting_rows: Vec<StrategyCoverageRow>,
}

impl MechanismBinding {
    /// Resolve `node`'s requirements against `strategy`'s rows in [`crate::strategy_coverage`].
    /// Every verdict comes from [`crate::strategy_coverage::representation_of`]; nothing is
    /// restated or re-decided here.
    pub fn derive(node: &MechanismNode, strategy: EmissionStrategy) -> Self {
        let mut worst = StrategyRepresentation::Represents;
        let mut limiting_rows = Vec::new();
        // `construct_requirements` is a `BTreeSet<CharacteristicKind>`, so iteration order is
        // deterministic and `limiting_rows` is too.
        for &kind in &node.construct_requirements {
            let row = representation_of(strategy, kind);
            if row.representation != StrategyRepresentation::Represents {
                limiting_rows.push(row);
            }
            worst = worse_of(worst, row.representation);
        }

        let disposition = match (worst, node.kind()) {
            (StrategyRepresentation::CannotRepresent, _) => ExecutionDisposition::Refused,
            (StrategyRepresentation::RepresentsWithKnownGap, _) => {
                ExecutionDisposition::ConfirmOnly
            }
            (StrategyRepresentation::Represents, MechanismKind::CopyProcess) => {
                ExecutionDisposition::Peeled
            }
            (StrategyRepresentation::Represents, _) => ExecutionDisposition::ExactFst,
        };

        Self {
            mechanism: node.id.clone(),
            strategy,
            disposition,
            limiting_rows,
        }
    }

    pub fn mechanism(&self) -> &MechanismId {
        &self.mechanism
    }

    /// Whose disposition this is. Never optional.
    pub fn strategy(&self) -> EmissionStrategy {
        self.strategy
    }

    pub fn disposition(&self) -> ExecutionDisposition {
        self.disposition
    }

    /// Every requirement row that was not a clean [`StrategyRepresentation::Represents`], with the
    /// citation `strategy_coverage` records for it. Empty iff coverage did not limit the
    /// disposition.
    pub fn limiting_rows(&self) -> &[StrategyCoverageRow] {
        &self.limiting_rows
    }
}

/// The meet of two representations: `CannotRepresent` < `RepresentsWithKnownGap` < `Represents`.
fn worse_of(a: StrategyRepresentation, b: StrategyRepresentation) -> StrategyRepresentation {
    fn rank(r: StrategyRepresentation) -> u8 {
        match r {
            StrategyRepresentation::CannotRepresent => 0,
            StrategyRepresentation::RepresentsWithKnownGap => 1,
            StrategyRepresentation::Represents => 2,
        }
    }
    if rank(a) <= rank(b) {
        a
    } else {
        b
    }
}

// ---------------------------------------------------------------------------------------------
// The graph
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MechanismGraph {
    pub nodes: Vec<MechanismNode>,
    pub edges: Vec<MechanismEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MechanismGraphError {
    InvalidId {
        id: MechanismId,
    },
    DuplicateId {
        id: MechanismId,
    },
    /// A node with no typed source reference at all.
    MissingSource {
        mechanism: MechanismId,
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
    SelfEdge {
        mechanism: MechanismId,
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
}

impl fmt::Display for MechanismGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MechanismGraphError {}

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

fn validate_node(node: &MechanismNode) -> Result<(), MechanismGraphError> {
    if node.sources.is_empty() {
        return Err(MechanismGraphError::MissingSource {
            mechanism: node.id.clone(),
        });
    }
    validate_wire_id(
        &node.id,
        "symbol_space.table",
        node.symbol_space.table(),
        WireModelKind::Table,
    )?;
    if let Some(stratum) = &node.stratum {
        validate_wire_id(&node.id, "stratum", stratum, WireModelKind::Stratum)?;
    }

    for source in &node.sources {
        let (owner_kind, child_kind) = MechanismSource::expected_domains(source.kind);
        match (&source.owner, owner_kind) {
            (Some(id), Some(kind)) => validate_wire_id(&node.id, "sources.owner", id, kind)?,
            (owner, expected) => {
                return Err(MechanismGraphError::InvalidWireId {
                    mechanism: node.id.clone(),
                    field: "sources.owner".to_owned(),
                    id: owner.clone().unwrap_or(WireModelId {
                        kind: expected.unwrap_or(WireModelKind::MRule),
                        value: 0,
                    }),
                    expected: expected.unwrap_or(WireModelKind::MRule),
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
            for id in &spec.templates {
                validate_wire_id(
                    &node.id,
                    "morphotactics.templates",
                    id,
                    WireModelKind::Template,
                )?;
            }
        }
        MechanismBody::StaticPartition(groups) => {
            for group in groups {
                for member in &group.members {
                    validate_wire_id(
                        &node.id,
                        "static_partition.groups.members",
                        member,
                        WireModelKind::LexEntry,
                    )?;
                }
            }
        }
        MechanismBody::OrderedPhonology(spec) => {
            for id in &spec.rule_order {
                validate_wire_id(
                    &node.id,
                    "ordered_phonology.rule_order",
                    id,
                    WireModelKind::PRule,
                )?;
            }
        }
        MechanismBody::StructuralAllomorph
        | MechanismBody::CopyProcess
        | MechanismBody::BoundaryCleanup(_) => {}
    }
    Ok(())
}

impl MechanismGraph {
    /// The node with `id`, if any.
    pub fn node(&self, id: &MechanismId) -> Option<&MechanismNode> {
        self.nodes.iter().find(|node| &node.id == id)
    }

    /// Every node's disposition under one named compiler (see [`MechanismBinding`]). Returned in
    /// node order, which for a derived graph is [`MechanismKind::COMPOSITION_ORDER`].
    pub fn bind(&self, strategy: EmissionStrategy) -> Vec<MechanismBinding> {
        self.nodes
            .iter()
            .map(|node| MechanismBinding::derive(node, strategy))
            .collect()
    }

    /// The bindings `strategy` must refuse. Empty iff `strategy` can represent every construct
    /// every mechanism in this graph requires.
    pub fn refusals(&self, strategy: EmissionStrategy) -> Vec<MechanismBinding> {
        self.bind(strategy)
            .into_iter()
            .filter(|binding| binding.disposition() == ExecutionDisposition::Refused)
            .collect()
    }

    /// A deterministic, byte-stable projection of the whole graph -- the canonical graph identity.
    ///
    /// Every collection reaching this point is already in a deterministic order (node order is
    /// [`MechanismKind::COMPOSITION_ORDER`], requirement sets are `BTreeSet`s, partition members
    /// are sorted, rule order is authored order), so serializing is enough. Nothing is re-sorted
    /// here on purpose: a provider that leaked a hash-ordered collection shows up as a projection
    /// difference instead of being papered over.
    pub fn canonical_projection(&self) -> String {
        serde_json::to_string(self).expect("mechanism graph is plain serializable data")
    }

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
            validate_node(node)?;
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
            if edge.producer == edge.consumer {
                return Err(MechanismGraphError::SelfEdge {
                    mechanism: edge.producer.clone(),
                });
            }
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

        // Position compatibility, COMPUTED from the two endpoints -- an edge cannot declare it.
        for edge in &self.edges {
            let producer = nodes.get(&edge.producer).expect("endpoint checked");
            let consumer = nodes.get(&edge.consumer).expect("endpoint checked");

            if producer.symbol_space != consumer.symbol_space {
                return Err(MechanismGraphError::SymbolSpaceMismatch {
                    producer: producer.id.clone(),
                    consumer: consumer.id.clone(),
                });
            }
            if producer.boundary_output() != consumer.boundary_input() {
                return Err(MechanismGraphError::UnsatisfiedState {
                    producer: producer.id.clone(),
                    consumer: consumer.id.clone(),
                    field: InterfaceField::Boundaries,
                    detail: format!(
                        "producer leaves boundaries {:?} but consumer requires {:?}",
                        producer.boundary_output(),
                        consumer.boundary_input()
                    ),
                });
            }
            if let (Some(produced), Some(required)) = (&producer.stratum, &consumer.stratum) {
                if produced != required {
                    return Err(MechanismGraphError::UnsatisfiedState {
                        producer: producer.id.clone(),
                        consumer: consumer.id.clone(),
                        field: InterfaceField::Stratum,
                        detail: "stratum does not match".to_owned(),
                    });
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every construct is routed to exactly one mechanism, and every mechanism kind is reached. The
    /// routing function is exhaustively matched, so this pins that the partition is also *onto* --
    /// a mechanism nothing routes to would be a node kind with no reason to exist.
    #[test]
    fn every_construct_routes_and_every_mechanism_is_reached() {
        let mut reached = BTreeSet::new();
        for &kind in CharacteristicKind::ALL {
            reached.insert(mechanism_kind_for(kind));
        }
        let all: BTreeSet<MechanismKind> =
            MechanismKind::COMPOSITION_ORDER.iter().copied().collect();
        assert_eq!(
            reached, all,
            "some mechanism kind has no construct routed to it"
        );
    }

    /// The disposition of the SAME mechanism differs by compiler. If it did not, the binding type's
    /// mandatory `strategy` would be decoration.
    #[test]
    fn one_mechanism_gets_different_dispositions_from_different_compilers() {
        let node = MechanismNode {
            id: MechanismId("morphotactics".to_owned()),
            sources: vec![MechanismSource {
                kind: MechanismSourceKind::MorphRule,
                owner: Some(MRuleId(0).into()),
                child: None,
            }],
            symbol_space: SymbolSpace::Surface(TableId(0).into()),
            stratum: None,
            construct_requirements: [CharacteristicKind::RealizationalMorphology]
                .into_iter()
                .collect(),
            body: MechanismBody::Morphotactics(MorphotacticsSpec {
                templates: vec![],
                max_depth: None,
            }),
        };

        let holed = MechanismBinding::derive(&node, EmissionStrategy::PlanComposed);
        assert_eq!(holed.disposition(), ExecutionDisposition::Refused);
        assert_eq!(holed.strategy(), EmissionStrategy::PlanComposed);
        assert_eq!(
            holed.limiting_rows().len(),
            1,
            "the refusal must carry strategy_coverage's own citation"
        );

        let whole = MechanismBinding::derive(&node, EmissionStrategy::TunedSurfaceProbed);
        assert_eq!(whole.disposition(), ExecutionDisposition::ExactFst);
        assert!(whole.limiting_rows().is_empty());
    }
}
