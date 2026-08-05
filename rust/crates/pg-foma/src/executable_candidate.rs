//! The ONE validated
//! [`ExecutableCandidate`], and the portable [`PortablePlan`] document it binds.
//!
//! # What this is for
//! A raw candidate as a `&'static str`
//! label, an in-memory [`Plan`], and an [`EmissionStrategy`] -- all public fields, constructible by
//! anyone -- would have no identity that survives leaving the process and no statement anywhere of what
//! executing it would actually deliver. Every consumer would therefore have to re-derive the missing parts on
//! its own, and the two that matter most -- which compiler realizes it, and what that compiler can
//! represent -- would be re-derived independently in [`crate::recipe_runtime`] and
//! [`crate::strategy_coverage`].
//!
//! [`ExecutableCandidate`] binds all of it in one validated value:
//!
//! | Bound | Field | Derived from |
//! |---|---|---|
//! | Stable semantic digest | [`ExecutableCandidate::semantic_digest()`] | SHA-256 over [`crate::recipe_mechanism::MechanismGraph::canonical_projection`] (a byte-identical fresh-load projection) |
//! | Portable Plan document | [`ExecutableCandidate::plan_document()`] | [`PortablePlan::encode`] of the materialized [`Plan`] |
//! | Plan document digest | [`ExecutableCandidate::plan_digest()`] | SHA-256 over that document's canonical JSON |
//! | Exact lowering adapter | [`ExecutableCandidate::adapter()`] | the [`LoweringAdapter`] the candidate itself carries ([`crate::enumerate::LoweredCandidate::adapter`]), 1:1 with [`EmissionStrategy`] |
//! | Existing runtime requirements | [`ExecutableCandidate::runtime_requirements()`] | the checks [`crate::recipe_runtime`] already performs, made explicit |
//! | Mechanism graph + bindings | [`ExecutableCandidate::mechanism_graph()`] / [`ExecutableCandidate::mechanism_bindings()`] | [`crate::mechanism_provider::derive_mechanism_graph`] + [`crate::recipe_mechanism::MechanismBinding::derive`] |
//! | Certification scope | [`ExecutableCandidate::certification_scope()`] | those bindings, per mechanism, naming the adapter |
//!
//! # The Registry is the sole constructor, and that is enforced by a type
//! [`ExecutableCandidate`]'s fields are private, it has no public constructor, and -- unlike every
//! other data type in this crate that carries provenance -- it deliberately implements neither
//! [`serde::Serialize`] nor [`serde::Deserialize`]. Deserialization would be a second constructor
//! that skips every check below, which is precisely the bypass this design forbids; the PORTABLE part
//! of a candidate is its [`PortablePlan`], which round-trips and re-verifies, not the validated
//! binding around it.
//!
//! Crate-internal callers are blocked too, not merely discouraged: [`seal`] requires a
//! [`crate::recipe_registry::RegistryAuthority`], whose only field is private to
//! `recipe_registry`, so no other module -- in this crate or any other -- can produce one. The
//! authority is taken BY VALUE and is neither `Copy` nor `Clone`, so it cannot be minted once and
//! reused to seal a second, unvalidated candidate.
//!
//! # Never FNV as artifact identity
//! [`crate::plan::NodeId`] is an unseeded 64-bit FNV-1a content address. It is the right tool for
//! what it does -- in-memory interning and cross-plan subtree sharing -- and its own doc says
//! plainly that it is "not collision-resistant, and that is an accepted tradeoff at this
//! data-modeling step". A 64-bit non-cryptographic hash cannot ground a persisted artifact identity
//! or a certification: two distinct plans colliding would make one candidate's measurement read as
//! another's, and nothing downstream could detect it.
//!
//! So identity here is SHA-256, domain-framed by a projection name exactly as
//! `pg_assess::digest::digest_projection` frames its own (same contract, computed locally because
//! `pg-foma` deliberately does not depend on the assessment layer -- `pg_parse::identity` was moved
//! rather than the dependency added). The FNV addresses still
//! APPEAR inside the document, because they are the plan's own structure; what makes them safe
//! there is that [`PortablePlan::decode`] RECOMPUTES every one of them from the decoded content and
//! refuses on the first disagreement. A tampered id is therefore a decode failure, never a silently
//! accepted relabelling -- the ids are checkable data inside the artifact, not the artifact's
//! identity.
//!
//! # Nothing here ranks, selects, or routes
//! Constructing an [`ExecutableCandidate`] changes no
//! outcome and makes nothing selectable that was not selectable before -- this module only builds
//! and verifies portable data, like [`crate::mechanism_provider`]. That is deliberate: measurement
//! found a candidate that was 2.2x cheaper than the winner and returned a DIFFERENT analysis set
//! (`@templated-underlying-tokens` on Amharic); ranking is only ever legitimate downstream of the
//! parity relation measured against an oracle ([`crate::parity`]), never from anything this module
//! can compute. [`CandidateCertificationScope`] accordingly states what a named compiler can
//! REPRESENT and nothing about what it got right.
//!
//! # And no implicit fallback
//! Every way construction can fail is a variant of [`CandidateConstructionError`]. There is no arm
//! that substitutes a different plan, a different adapter, or a different compiler for one that
//! could not be built. A budget verdict relabelled
//! `Unsupported` several call frames away from where it was produced is exactly the same disease as
//! a quiet substitution, and the type system is the cure that does not rot.

use std::collections::BTreeSet;
use std::fmt;

use pg_grammar::model::{LexEntryId, MRuleId, PRuleId, TemplateId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::capability::CharacteristicKind;
use crate::enumerate::EmissionStrategy;
use crate::plan::{
    ComposeStrategy, FragmentSpec, GateGroupSpec, GatePartitionSpec, GatedSubruleRef, NodeId, Plan,
    PlanNodeKind, Provenance, ReplaceCascadeSpec,
};
use crate::recipe_mechanism::{
    ExecutionDisposition, MechanismBinding, MechanismGraph, MechanismGraphError, MechanismId,
    WireModelId, WireModelKind,
};
use crate::recipe_registry::{MaterializeError, RecipeInstance, RegistryAuthority};
use crate::strategy_coverage::StrategyRepresentation;

// ---------------------------------------------------------------------------------------------
// Domain-framed SHA-256
// ---------------------------------------------------------------------------------------------

/// Wire version of [`PortablePlan`]. Bumped only for a change that makes an older document decode
/// differently; the projection name below carries the same information into every digest, so a
/// schema change cannot silently produce a colliding identity.
pub const PORTABLE_PLAN_SCHEMA_VERSION: u16 = 1;

/// "These are the same plan, byte for byte, as a portable document."
pub const PLAN_DOCUMENT_PROJECTION: &str = "pangloss.foma.plan-document/v1";

/// "These candidates were derived from the same grammar semantics." The preimage is
/// [`MechanismGraph::canonical_projection`], which is byte-identical across
/// independent loads of the same grammar.
pub const CANDIDATE_SEMANTICS_PROJECTION: &str = "pangloss.foma.candidate-semantics/v1";

/// Hex SHA-256 over `value`, with `projection` bound into the preimage.
///
/// Same contract as `pg_assess::digest::digest_projection`: the projection name is part of what is
/// hashed, so changing what a projection means produces visibly different digests instead of
/// colliding with the old meaning. Both halves are LENGTH-PREFIXED, which is what makes the framing
/// unambiguous -- without it, `("ab", "c")` and `("a", "bc")` would hash alike, and a projection
/// name is exactly the kind of string that gets extended.
pub fn digest_projection(projection: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update((projection.len() as u64).to_le_bytes());
    hasher.update(projection.as_bytes());
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------------------------
// The portable Plan document
// ---------------------------------------------------------------------------------------------

/// A [`crate::plan::FragmentSpec`] in wire form.
///
/// Model ids travel as [`WireModelId`]s -- the same typed carrier
/// [`crate::recipe_mechanism`] already uses -- rather than as bare integers. `pg_grammar`'s id
/// newtypes are not `Serialize`, and more importantly a bare integer lets a `PRuleId` be read back
/// as a `LexEntryId` across a serialization boundary; the kind tag makes that a typed decode error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "fragment", rename_all = "kebab-case")]
pub enum PortableFragment {
    LexiconFragment { entries: Option<Vec<WireModelId>> },
    RewriteRule { rule: WireModelId },
    GuardAutomaton { group_key: Vec<bool> },
    CompositeEmissionMarker,
    StructuralCompositeMarker,
}

/// A [`crate::plan::Provenance`] in wire form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "provenance", rename_all = "kebab-case")]
pub enum PortableProvenance {
    Lexicon,
    RewriteRule { rule: WireModelId },
    MorphRule { rule: WireModelId },
    Template { template: WireModelId },
    Gate,
    Replace,
    CompositeEmission,
    StructuralComposite,
}

/// A [`crate::plan::ComposeStrategy`] in wire form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PortableComposeStrategy {
    Static,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableGatedSubrule {
    pub rule_pos: u64,
    pub sub_idx: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableGateGroup {
    pub key: Vec<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableGatePartition {
    pub gated_subrules: Vec<PortableGatedSubrule>,
    pub groups: Vec<PortableGateGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableReplaceCascade {
    pub rules: Vec<WireModelId>,
    pub gated_subrules: Vec<PortableGatedSubrule>,
    pub group_key: Vec<bool>,
}

/// A [`PlanNodeKind`] in wire form. Children are the 16-hex `Display` form of the child's
/// [`NodeId`], so a document is readable and diffable without a decoder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PortableNodeKind {
    Leaf {
        fragment: PortableFragment,
        provenance: PortableProvenance,
    },
    Compose {
        children: Vec<String>,
        strategy: PortableComposeStrategy,
    },
    Union {
        children: Vec<String>,
    },
    Gate {
        partition: PortableGatePartition,
        children: Vec<String>,
    },
    Replace {
        cascade: PortableReplaceCascade,
        children: Vec<String>,
    },
}

/// One node of a [`PortablePlan`]: its DECLARED content address plus its content.
///
/// The id is declared, not trusted. [`PortablePlan::decode`] recomputes it from the content and
/// refuses on disagreement, which is the whole reason a document can carry FNV addresses at all
/// without them becoming the artifact's identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortablePlanNode {
    pub id: String,
    #[serde(flatten)]
    pub node: PortableNodeKind,
}

/// A [`Plan`] as a portable, round-trippable document.
///
/// Round-trip means exactly this, and nothing weaker: `decode(encode(p))` rebuilds a `Plan` whose
/// re-`encode` is the identical document, and therefore has the identical [`Self::digest`]. Node
/// order is [`Plan::iter`]'s, which is content-address order (`Plan`'s arena is a `BTreeMap`), so
/// two independent encodes of the same plan agree. Child order inside a node is NEVER sorted: it is
/// semantic (a `Replace` cascade is order-sensitive) and is part of the content address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortablePlan {
    pub schema_version: u16,
    pub root: Option<String>,
    pub nodes: Vec<PortablePlanNode>,
}

/// Every way a [`PortablePlan`] can fail to be a faithful plan. Note what is absent: there is no
/// variant that repairs, prunes, or substitutes. A document that does not decode exactly is
/// refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortablePlanError {
    UnsupportedSchema(u16),
    DuplicateNode {
        id: String,
    },
    /// A child (or the root) names an id no node in the document declares.
    UnknownNode {
        referrer: String,
        id: String,
    },
    /// The document's node graph is cyclic. A [`Plan`] is a DAG; a cycle cannot be interned.
    Cycle {
        id: String,
    },
    /// A wire id in the wrong domain (e.g. a `PRuleId` where a `LexEntryId` belongs), or out of
    /// range for its own domain.
    InvalidWireId {
        node: String,
        field: String,
        id: WireModelId,
        expected: WireModelKind,
    },
    /// A `Gate` node whose child count does not match its partition-group count. Checked HERE, and
    /// before interning, because [`Plan::add_node`] enforces the same invariant with a
    /// `debug_assert!` -- which is a panic in a debug build and silence in a release one. Neither is
    /// an acceptable answer to untrusted input.
    GateArityMismatch {
        node: String,
        groups: usize,
        children: usize,
    },
    /// The decoded content does not hash to the id the document declares for it. This is THE
    /// corruption check: it fires for a tampered id, a tampered payload, a tampered child list, or
    /// a tampered descendant anywhere below, because a content address is a pure function of the
    /// whole subtree.
    ContentAddressMismatch {
        declared: String,
        recomputed: String,
    },
    /// The document could not be parsed as JSON at all.
    Malformed(String),
}

impl fmt::Display for PortablePlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PortablePlanError {}

fn wire_of(id: impl Into<WireModelId>) -> WireModelId {
    id.into()
}

fn decode_wire<T>(
    node: &str,
    field: &str,
    id: &WireModelId,
    expected: WireModelKind,
) -> Result<T, PortablePlanError>
where
    T: TryFrom<WireModelId>,
{
    T::try_from(id.clone()).map_err(|_| PortablePlanError::InvalidWireId {
        node: node.to_owned(),
        field: field.to_owned(),
        id: id.clone(),
        expected,
    })
}

fn encode_gated(subrules: &[GatedSubruleRef]) -> Vec<PortableGatedSubrule> {
    subrules
        .iter()
        .map(|s| PortableGatedSubrule {
            rule_pos: s.rule_pos as u64,
            sub_idx: s.sub_idx as u64,
        })
        .collect()
}

fn decode_gated(subrules: &[PortableGatedSubrule]) -> Vec<GatedSubruleRef> {
    subrules
        .iter()
        .map(|s| GatedSubruleRef {
            rule_pos: s.rule_pos as usize,
            sub_idx: s.sub_idx as usize,
        })
        .collect()
}

impl PortablePlan {
    /// Project `plan` into its portable document.
    pub fn encode(plan: &Plan) -> Self {
        let nodes = plan
            .iter()
            .map(|(id, kind)| PortablePlanNode {
                id: id.to_string(),
                node: encode_node(kind),
            })
            .collect();
        Self {
            schema_version: PORTABLE_PLAN_SCHEMA_VERSION,
            root: plan.root().map(|root| root.to_string()),
            nodes,
        }
    }

    /// The document's canonical JSON. Deterministic without a JCS pass: every collection reaching
    /// here is already in a deterministic order (nodes in content-address order, everything else in
    /// its own semantic order), and `serde_json` emits struct fields in declaration order.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("a portable plan is plain serializable data")
    }

    /// The artifact identity: domain-framed SHA-256 over [`Self::canonical_json`]. NOT the plan's
    /// FNV root -- see this module's doc.
    pub fn digest(&self) -> String {
        digest_projection(PLAN_DOCUMENT_PROJECTION, &self.canonical_json())
    }

    pub fn from_json(json: &str) -> Result<Self, PortablePlanError> {
        serde_json::from_str(json).map_err(|error| PortablePlanError::Malformed(error.to_string()))
    }

    /// Rebuild the [`Plan`] this document describes, recomputing and CHECKING every content
    /// address on the way.
    ///
    /// Nothing is repaired. A mismatch, a dangling child, a cycle, a mis-typed wire id, or a `Gate`
    /// arity error each abort with the corresponding [`PortablePlanError`], leaving the caller to
    /// decide -- which is the point: a decoder that silently rebuilt "the nearest valid plan" would
    /// hand back an artifact whose digest no longer means what it said.
    pub fn decode(&self) -> Result<Plan, PortablePlanError> {
        if self.schema_version != PORTABLE_PLAN_SCHEMA_VERSION {
            return Err(PortablePlanError::UnsupportedSchema(self.schema_version));
        }
        let mut index: std::collections::BTreeMap<&str, &PortableNodeKind> = Default::default();
        for node in &self.nodes {
            if index.insert(node.id.as_str(), &node.node).is_some() {
                return Err(PortablePlanError::DuplicateNode {
                    id: node.id.clone(),
                });
            }
        }

        let mut plan = Plan::new();
        let mut resolved: std::collections::BTreeMap<String, NodeId> = Default::default();
        let mut active: BTreeSet<String> = BTreeSet::new();
        // Every declared node is interned, not only those the root reaches: dropping an
        // unreferenced node would be a silent repair of the document.
        for node in &self.nodes {
            resolve_node(
                &node.id,
                &node.id,
                &index,
                &mut plan,
                &mut resolved,
                &mut active,
            )?;
        }
        if let Some(root) = &self.root {
            let root_id =
                resolved
                    .get(root)
                    .copied()
                    .ok_or_else(|| PortablePlanError::UnknownNode {
                        referrer: "root".to_owned(),
                        id: root.clone(),
                    })?;
            plan.set_root(root_id);
        }
        Ok(plan)
    }
}

fn encode_node(kind: &PlanNodeKind) -> PortableNodeKind {
    match kind {
        PlanNodeKind::Leaf {
            fragment,
            provenance,
        } => PortableNodeKind::Leaf {
            fragment: match fragment {
                FragmentSpec::LexiconFragment { entries } => PortableFragment::LexiconFragment {
                    entries: entries
                        .as_ref()
                        .map(|ids| ids.iter().copied().map(wire_of).collect()),
                },
                FragmentSpec::RewriteRule { rule } => PortableFragment::RewriteRule {
                    rule: wire_of(*rule),
                },
                FragmentSpec::GuardAutomaton { group_key } => PortableFragment::GuardAutomaton {
                    group_key: group_key.clone(),
                },
                FragmentSpec::CompositeEmissionMarker => PortableFragment::CompositeEmissionMarker,
                FragmentSpec::StructuralCompositeMarker => {
                    PortableFragment::StructuralCompositeMarker
                }
            },
            provenance: match provenance {
                Provenance::Lexicon => PortableProvenance::Lexicon,
                Provenance::RewriteRule(rule) => PortableProvenance::RewriteRule {
                    rule: wire_of(*rule),
                },
                Provenance::MorphRule(rule) => PortableProvenance::MorphRule {
                    rule: wire_of(*rule),
                },
                Provenance::Template(template) => PortableProvenance::Template {
                    template: wire_of(*template),
                },
                Provenance::Gate => PortableProvenance::Gate,
                Provenance::Replace => PortableProvenance::Replace,
                Provenance::CompositeEmission => PortableProvenance::CompositeEmission,
                Provenance::StructuralComposite => PortableProvenance::StructuralComposite,
            },
        },
        PlanNodeKind::Compose { children, strategy } => PortableNodeKind::Compose {
            children: children.iter().map(NodeId::to_string).collect(),
            strategy: match strategy {
                ComposeStrategy::Static => PortableComposeStrategy::Static,
            },
        },
        PlanNodeKind::Union { children } => PortableNodeKind::Union {
            children: children.iter().map(NodeId::to_string).collect(),
        },
        PlanNodeKind::Gate {
            partition,
            children,
        } => PortableNodeKind::Gate {
            partition: PortableGatePartition {
                gated_subrules: encode_gated(&partition.gated_subrules),
                groups: partition
                    .groups
                    .iter()
                    .map(|group| PortableGateGroup {
                        key: group.key.clone(),
                    })
                    .collect(),
            },
            children: children.iter().map(NodeId::to_string).collect(),
        },
        PlanNodeKind::Replace { cascade, children } => PortableNodeKind::Replace {
            cascade: PortableReplaceCascade {
                rules: cascade.rules.iter().copied().map(wire_of).collect(),
                gated_subrules: encode_gated(&cascade.gated_subrules),
                group_key: cascade.group_key.clone(),
            },
            children: children.iter().map(NodeId::to_string).collect(),
        },
    }
}

/// Resolve one declared id to an interned [`NodeId`], recursing into its children first.
///
/// Recursion depth is plan DEPTH (`Union` -> `Gate` -> `Compose` -> `Replace` -> `Leaf` in every
/// plan this crate enumerates), not plan size; a `Gate` node's hundreds of children are siblings,
/// each resolved at the same depth.
fn resolve_node(
    id: &str,
    referrer: &str,
    index: &std::collections::BTreeMap<&str, &PortableNodeKind>,
    plan: &mut Plan,
    resolved: &mut std::collections::BTreeMap<String, NodeId>,
    active: &mut BTreeSet<String>,
) -> Result<NodeId, PortablePlanError> {
    if let Some(existing) = resolved.get(id) {
        return Ok(*existing);
    }
    if !active.insert(id.to_owned()) {
        return Err(PortablePlanError::Cycle { id: id.to_owned() });
    }
    let wire = index
        .get(id)
        .copied()
        .ok_or_else(|| PortablePlanError::UnknownNode {
            referrer: referrer.to_owned(),
            id: id.to_owned(),
        })?;

    let resolve_children = |children: &[String],
                            plan: &mut Plan,
                            resolved: &mut std::collections::BTreeMap<String, NodeId>,
                            active: &mut BTreeSet<String>|
     -> Result<Vec<NodeId>, PortablePlanError> {
        children
            .iter()
            .map(|child| resolve_node(child, id, index, plan, resolved, active))
            .collect()
    };

    let kind = match wire {
        PortableNodeKind::Leaf {
            fragment,
            provenance,
        } => PlanNodeKind::Leaf {
            fragment: match fragment {
                PortableFragment::LexiconFragment { entries } => FragmentSpec::LexiconFragment {
                    entries: entries
                        .as_ref()
                        .map(|ids| {
                            ids.iter()
                                .map(|wid| {
                                    decode_wire::<LexEntryId>(
                                        id,
                                        "lexicon-fragment.entries",
                                        wid,
                                        WireModelKind::LexEntry,
                                    )
                                })
                                .collect::<Result<Vec<_>, _>>()
                        })
                        .transpose()?,
                },
                PortableFragment::RewriteRule { rule } => FragmentSpec::RewriteRule {
                    rule: decode_wire::<PRuleId>(
                        id,
                        "rewrite-rule.rule",
                        rule,
                        WireModelKind::PRule,
                    )?,
                },
                PortableFragment::GuardAutomaton { group_key } => FragmentSpec::GuardAutomaton {
                    group_key: group_key.clone(),
                },
                PortableFragment::CompositeEmissionMarker => FragmentSpec::CompositeEmissionMarker,
                PortableFragment::StructuralCompositeMarker => {
                    FragmentSpec::StructuralCompositeMarker
                }
            },
            provenance: match provenance {
                PortableProvenance::Lexicon => Provenance::Lexicon,
                PortableProvenance::RewriteRule { rule } => {
                    Provenance::RewriteRule(decode_wire::<PRuleId>(
                        id,
                        "provenance.rewrite-rule",
                        rule,
                        WireModelKind::PRule,
                    )?)
                }
                PortableProvenance::MorphRule { rule } => {
                    Provenance::MorphRule(decode_wire::<MRuleId>(
                        id,
                        "provenance.morph-rule",
                        rule,
                        WireModelKind::MRule,
                    )?)
                }
                PortableProvenance::Template { template } => {
                    Provenance::Template(decode_wire::<TemplateId>(
                        id,
                        "provenance.template",
                        template,
                        WireModelKind::Template,
                    )?)
                }
                PortableProvenance::Gate => Provenance::Gate,
                PortableProvenance::Replace => Provenance::Replace,
                PortableProvenance::CompositeEmission => Provenance::CompositeEmission,
                PortableProvenance::StructuralComposite => Provenance::StructuralComposite,
            },
        },
        PortableNodeKind::Compose { children, strategy } => PlanNodeKind::Compose {
            children: resolve_children(children, plan, resolved, active)?,
            strategy: match strategy {
                PortableComposeStrategy::Static => ComposeStrategy::Static,
            },
        },
        PortableNodeKind::Union { children } => PlanNodeKind::Union {
            children: resolve_children(children, plan, resolved, active)?,
        },
        PortableNodeKind::Gate {
            partition,
            children,
        } => {
            if partition.groups.len() != children.len() {
                return Err(PortablePlanError::GateArityMismatch {
                    node: id.to_owned(),
                    groups: partition.groups.len(),
                    children: children.len(),
                });
            }
            PlanNodeKind::Gate {
                partition: GatePartitionSpec {
                    gated_subrules: decode_gated(&partition.gated_subrules),
                    groups: partition
                        .groups
                        .iter()
                        .map(|group| GateGroupSpec {
                            key: group.key.clone(),
                        })
                        .collect(),
                },
                children: resolve_children(children, plan, resolved, active)?,
            }
        }
        PortableNodeKind::Replace { cascade, children } => PlanNodeKind::Replace {
            cascade: ReplaceCascadeSpec {
                rules: cascade
                    .rules
                    .iter()
                    .map(|wid| {
                        decode_wire::<PRuleId>(id, "cascade.rules", wid, WireModelKind::PRule)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                gated_subrules: decode_gated(&cascade.gated_subrules),
                group_key: cascade.group_key.clone(),
            },
            children: resolve_children(children, plan, resolved, active)?,
        },
    };

    let interned = plan.add_node(kind);
    let recomputed = interned.to_string();
    if recomputed != id {
        return Err(PortablePlanError::ContentAddressMismatch {
            declared: id.to_owned(),
            recomputed,
        });
    }
    active.remove(id);
    resolved.insert(id.to_owned(), interned);
    Ok(interned)
}

// ---------------------------------------------------------------------------------------------
// The lowering adapter
// ---------------------------------------------------------------------------------------------

/// WHICH of this crate's compilers lowers a candidate into a network -- named as an adapter rather
/// than left implicit in a `match` on [`EmissionStrategy`] scattered across
/// [`crate::recipe_runtime`].
///
/// One variant per [`EmissionStrategy`], both directions exhaustively matched, so the adapter axis
/// provably expresses everything the enum selects between. Measurement showed that axis to be the
/// DECISIVE one (two whole-grammar compilers win two different languages), so nothing here
/// hard-codes a winner and nothing here ranks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoweringAdapter {
    /// [`crate::build::build_controllable`] interprets the candidate's own [`Plan`]. The ONLY
    /// adapter that reads a plan at all.
    ControllablePlanCompose,
    /// `crate::emit`'s surface probe via [`crate::analyzer::FomaProposer::new`]. Whole-grammar;
    /// derives its own topology and ignores the plan.
    TunedSurfaceEmit,
    /// `crate::emit::emit_underlying_templated` plus a compiled rewrite cascade via
    /// [`crate::templated_compile`]. Whole-grammar; likewise ignores the plan.
    TemplatedUnderlyingEmit,
}

impl LoweringAdapter {
    pub fn for_strategy(strategy: EmissionStrategy) -> Self {
        match strategy {
            EmissionStrategy::PlanComposed => Self::ControllablePlanCompose,
            EmissionStrategy::TunedSurfaceProbed => Self::TunedSurfaceEmit,
            EmissionStrategy::TemplatedUnderlyingTokens => Self::TemplatedUnderlyingEmit,
        }
    }

    /// The strategy this adapter realizes. Exhaustive in both directions so the correspondence is
    /// compiler-checked, not documented.
    pub fn strategy(self) -> EmissionStrategy {
        match self {
            Self::ControllablePlanCompose => EmissionStrategy::PlanComposed,
            Self::TunedSurfaceEmit => EmissionStrategy::TunedSurfaceProbed,
            Self::TemplatedUnderlyingEmit => EmissionStrategy::TemplatedUnderlyingTokens,
        }
    }

    /// Whether this adapter INTERPRETS the candidate's plan. `false` for both whole-grammar
    /// adapters, which is exactly why [`crate::recipe_runtime::build_candidate`] refuses to be
    /// handed one: honouring it there would measure a different compiler than the candidate names.
    pub fn interprets_plan(self) -> bool {
        matches!(self, Self::ControllablePlanCompose)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ControllablePlanCompose => "controllable-plan-compose",
            Self::TunedSurfaceEmit => "tuned-surface-emit",
            Self::TemplatedUnderlyingEmit => "templated-underlying-emit",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Runtime requirements
// ---------------------------------------------------------------------------------------------

/// A requirement the RUNTIME already enforces today, stated on the candidate instead of rediscovered
/// per evaluation.
///
/// Every variant is DERIVED from the adapter and the plan; none is declared by whoever builds the
/// candidate. That is the same discipline imposed on
/// [`crate::recipe_mechanism::MechanismEdge`], after an earlier vocabulary let an edge assert a
/// property its endpoints lacked.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeRequirement {
    /// The plan must have a root. [`crate::recipe_registry::Registry::materialize`] already checks
    /// this and returns [`MaterializeError::RootlessPlan`]; the requirement records that a rootless
    /// plan is unbuildable for every adapter, not just the interpreting one.
    RootedPlan,
    /// The adapter interprets this candidate's plan, so the plan's shape is load-bearing and a
    /// permutation of it is a genuinely different candidate.
    PlanInterpretedByAdapter,
    /// The adapter is whole-grammar: it recompiles the grammar its own way and IGNORES the plan.
    /// Recorded so a report can never attribute a plan permutation to a compiler that never read
    /// one -- `recipe_runtime` calls that "a fabricated comparison".
    WholeGrammarRecompilation,
    /// [`crate::build::unbuildable_markers`]: the marker leaves this plan names that
    /// `build_controllable` cannot build. Empty is the common, healthy case and is still recorded,
    /// because "the interpreter must be able to build every leaf this plan names" is a requirement
    /// whether or not it currently bites.
    ControllableSubtreeBuildable { unbuildable_markers: Vec<String> },
}

// ---------------------------------------------------------------------------------------------
// Certification scope
// ---------------------------------------------------------------------------------------------

/// One coverage citation behind a non-clean disposition: which mechanism, which construct, which
/// verdict, and where in this crate the verdict is established.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageCitation {
    pub mechanism: MechanismId,
    pub kind: CharacteristicKind,
    pub representation: String,
    pub evidence: String,
}

/// What a certification of this candidate could HONESTLY cover -- per mechanism, under one named
/// adapter.
///
/// Deliberately narrow, and deliberately not called `CertificationScope`: the assessment layer owns the
/// versioned corpus-side `CorpusSnapshot`/`CertificationScope` (profile, authority, source/model
/// revision, options, occurrence order, normalization, exclusions, oracle completeness), and this
/// is the candidate-side half only. The two must not collapse into one type; a corpus scope says
/// what was measured, this says what the compiler can represent.
///
/// It states no identity or multiplicity guarantee, and it never will. Those ARE the parity
/// relation, established by measuring against an oracle ([`crate::parity`]); Amharic's 2.2x-cheaper
/// `identity-mismatch` candidate is the measured reason a declared "identity preserved" is a false
/// comfort. Every field below names the adapter it belongs to, because a guarantee that cannot name
/// whose guarantee it is, is the `uflexc`/`Compounding` bug.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateCertificationScope {
    adapter: LoweringAdapter,
    exact_fst: Vec<MechanismId>,
    confirm_only: Vec<MechanismId>,
    peeled: Vec<MechanismId>,
    citations: Vec<CoverageCitation>,
}

impl CandidateCertificationScope {
    /// Whose scope this is. Never optional.
    pub fn adapter(&self) -> LoweringAdapter {
        self.adapter
    }

    /// Mechanisms this adapter represents with no cited gap.
    pub fn exact_fst(&self) -> &[MechanismId] {
        &self.exact_fst
    }

    /// Mechanisms whose output must be confirm-gated under this adapter, because at least one
    /// required construct has a documented partial gap.
    pub fn confirm_only(&self) -> &[MechanismId] {
        &self.confirm_only
    }

    /// Mechanisms executed outside the compiled FST by [`crate::peel`].
    pub fn peeled(&self) -> &[MechanismId] {
        &self.peeled
    }

    /// Every citation behind a non-clean disposition, from `strategy_coverage`'s own table.
    pub fn citations(&self) -> &[CoverageCitation] {
        &self.citations
    }

    fn derive(adapter: LoweringAdapter, bindings: &[MechanismBinding]) -> Self {
        let mut exact_fst = Vec::new();
        let mut confirm_only = Vec::new();
        let mut peeled = Vec::new();
        let mut citations = Vec::new();
        for binding in bindings {
            match binding.disposition() {
                ExecutionDisposition::ExactFst => exact_fst.push(binding.mechanism().clone()),
                ExecutionDisposition::ConfirmOnly => confirm_only.push(binding.mechanism().clone()),
                ExecutionDisposition::Peeled => peeled.push(binding.mechanism().clone()),
                // Unreachable in a sealed candidate: a refusal aborts construction before a scope
                // is derived (see `CandidateConstructionError::MechanismRefused`). Kept as an arm
                // rather than a catch-all so adding a disposition breaks this build.
                ExecutionDisposition::Refused => {}
            }
            for row in binding.limiting_rows() {
                citations.push(CoverageCitation {
                    mechanism: binding.mechanism().clone(),
                    kind: row.kind,
                    representation: row.representation.label().to_owned(),
                    evidence: row.evidence.to_owned(),
                });
            }
        }
        Self {
            adapter,
            exact_fst,
            confirm_only,
            peeled,
            citations,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The validated candidate
// ---------------------------------------------------------------------------------------------

/// Every way constructing an [`ExecutableCandidate`] can fail.
///
/// There is no "substituted X instead" variant and there must never be one. Each of these is a
/// refusal the caller must handle; none of them is recoverable inside this module by quietly
/// building something else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateConstructionError {
    /// The instance did not materialize at all (unknown family, invalid instance, inapplicable,
    /// missing materializer, rootless plan) -- the registry's existing typed error, unwrapped, not
    /// re-classified.
    Materialize(MaterializeError),
    /// The materialized plan has no root, so no adapter can build it.
    RootlessPlan { family: String },
    /// The plan does not survive its own portable round-trip. A candidate whose document cannot be
    /// decoded back to the plan it was encoded from has no artifact identity, so it cannot be
    /// sealed.
    PlanDocument {
        family: String,
        error: PortablePlanError,
    },
    /// The derived mechanism graph is not well-formed.
    MechanismGraph {
        family: String,
        error: MechanismGraphError,
    },
    /// At least one mechanism this grammar requires is
    /// [`StrategyRepresentation::CannotRepresent`] for this adapter: a whole-construct recall hole.
    ///
    /// This is the typed refusal that replaces an implicit fallback. Nothing here reaches for
    /// another compiler that happens to work: substituting one silently is exactly how `uflexc`
    /// held a `ConfirmOnly` `Compounding` verdict it could not deliver, and how a cheaper WRONG
    /// candidate gets preferred. The caller is told which mechanism, under which adapter, with
    /// `strategy_coverage`'s own citation.
    MechanismRefused {
        family: String,
        adapter: LoweringAdapter,
        mechanisms: Vec<MechanismId>,
        citations: Vec<CoverageCitation>,
    },
}

impl fmt::Display for CandidateConstructionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CandidateConstructionError {}

/// One validated, executable candidate.
///
/// Constructible ONLY by [`crate::recipe_registry::Registry::executable_candidate`] (module doc:
/// the [`RegistryAuthority`] parameter on [`seal`] is what enforces that, in this crate as well as
/// outside it). Neither `Serialize` nor `Deserialize`, on purpose -- see the module doc.
#[derive(Debug, Clone)]
pub struct ExecutableCandidate {
    family_id: String,
    instance: RecipeInstance,
    adapter: LoweringAdapter,
    semantic_digest: String,
    plan_document: PortablePlan,
    plan_digest: String,
    runtime_requirements: Vec<RuntimeRequirement>,
    mechanism_graph: MechanismGraph,
    mechanism_bindings: Vec<MechanismBinding>,
    certification_scope: CandidateCertificationScope,
}

impl ExecutableCandidate {
    pub fn family_id(&self) -> &str {
        &self.family_id
    }

    pub fn instance(&self) -> &RecipeInstance {
        &self.instance
    }

    /// The exact lowering adapter. Never inferred by a consumer from a label.
    pub fn adapter(&self) -> LoweringAdapter {
        self.adapter
    }

    /// Domain-framed SHA-256 over the grammar's canonical mechanism projection: "these candidates
    /// describe the same grammar semantics".
    pub fn semantic_digest(&self) -> &str {
        &self.semantic_digest
    }

    pub fn plan_document(&self) -> &PortablePlan {
        &self.plan_document
    }

    /// Domain-framed SHA-256 over the plan document. The artifact identity -- NOT the FNV root.
    pub fn plan_digest(&self) -> &str {
        &self.plan_digest
    }

    pub fn runtime_requirements(&self) -> &[RuntimeRequirement] {
        &self.runtime_requirements
    }

    pub fn mechanism_graph(&self) -> &MechanismGraph {
        &self.mechanism_graph
    }

    /// Every mechanism's disposition under THIS candidate's adapter. Each binding names its own
    /// strategy; none of them is anonymous.
    pub fn mechanism_bindings(&self) -> &[MechanismBinding] {
        &self.mechanism_bindings
    }

    pub fn certification_scope(&self) -> &CandidateCertificationScope {
        &self.certification_scope
    }
}

/// Validate everything and seal the candidate. The [`RegistryAuthority`] is the enforcement; see
/// the module doc.
///
/// Order matters and is chosen so the cheapest refusal comes first: root, then the portable
/// round-trip, then the graph, then per-mechanism representability.
#[allow(clippy::too_many_arguments)]
pub(crate) fn seal(
    _authority: RegistryAuthority,
    family_id: &str,
    instance: &RecipeInstance,
    plan: &Plan,
    adapter: LoweringAdapter,
    graph: MechanismGraph,
) -> Result<ExecutableCandidate, CandidateConstructionError> {
    if plan.root().is_none() {
        return Err(CandidateConstructionError::RootlessPlan {
            family: family_id.to_owned(),
        });
    }
    // The adapter arrives from the candidate itself
    // (`crate::enumerate::LoweredCandidate::adapter`) rather than being re-derived here from an
    // `EmissionStrategy`. `MechanismGraph::bind` still speaks the strategy axis, which is why the
    // projection below exists and why `EmissionStrategy` is NOT deleted: it remains the axis
    // `strategy_coverage`, the mechanism bindings, and every report label are expressed in.
    let strategy = adapter.strategy();

    // The document is not merely produced, it is PROVED faithful: encode, decode (which recomputes
    // every content address), re-encode, and require the two documents to be identical. A candidate
    // whose own document does not round-trip has no artifact identity to seal.
    let plan_document = PortablePlan::encode(plan);
    let decoded =
        plan_document
            .decode()
            .map_err(|error| CandidateConstructionError::PlanDocument {
                family: family_id.to_owned(),
                error,
            })?;
    let reencoded = PortablePlan::encode(&decoded);
    if reencoded != plan_document || decoded.root() != plan.root() {
        return Err(CandidateConstructionError::PlanDocument {
            family: family_id.to_owned(),
            error: PortablePlanError::ContentAddressMismatch {
                declared: plan_document.digest(),
                recomputed: reencoded.digest(),
            },
        });
    }
    let plan_digest = plan_document.digest();

    graph
        .validate()
        .map_err(|error| CandidateConstructionError::MechanismGraph {
            family: family_id.to_owned(),
            error,
        })?;
    let semantic_digest = digest_projection(
        CANDIDATE_SEMANTICS_PROJECTION,
        &graph.canonical_projection(),
    );

    let bindings = graph.bind(strategy);
    let refused: Vec<&MechanismBinding> = bindings
        .iter()
        .filter(|binding| binding.disposition() == ExecutionDisposition::Refused)
        .collect();
    if !refused.is_empty() {
        return Err(CandidateConstructionError::MechanismRefused {
            family: family_id.to_owned(),
            adapter,
            mechanisms: refused
                .iter()
                .map(|binding| binding.mechanism().clone())
                .collect(),
            citations: refused
                .iter()
                .flat_map(|binding| {
                    binding
                        .limiting_rows()
                        .iter()
                        .filter(|row| row.representation == StrategyRepresentation::CannotRepresent)
                        .map(|row| CoverageCitation {
                            mechanism: binding.mechanism().clone(),
                            kind: row.kind,
                            representation: row.representation.label().to_owned(),
                            evidence: row.evidence.to_owned(),
                        })
                })
                .collect(),
        });
    }

    let mut runtime_requirements = vec![RuntimeRequirement::RootedPlan];
    if adapter.interprets_plan() {
        runtime_requirements.push(RuntimeRequirement::PlanInterpretedByAdapter);
        runtime_requirements.push(RuntimeRequirement::ControllableSubtreeBuildable {
            unbuildable_markers: crate::build::unbuildable_markers(plan)
                .iter()
                .map(|marker| format!("{marker:?}"))
                .collect(),
        });
    } else {
        runtime_requirements.push(RuntimeRequirement::WholeGrammarRecompilation);
    }

    let certification_scope = CandidateCertificationScope::derive(adapter, &bindings);

    Ok(ExecutableCandidate {
        family_id: family_id.to_owned(),
        instance: instance.clone(),
        adapter,
        semantic_digest,
        plan_document,
        plan_digest,
        runtime_requirements,
        mechanism_graph: graph,
        mechanism_bindings: bindings,
        certification_scope,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{FragmentSpec, PlanNodeKind, Provenance};

    /// A small plan exercising every node kind, so the round-trip covers the whole closed enum.
    fn sample_plan() -> Plan {
        let mut plan = Plan::new();
        let lexicon = plan.add_node(PlanNodeKind::Leaf {
            fragment: FragmentSpec::LexiconFragment {
                entries: Some(vec![LexEntryId(0), LexEntryId(1)]),
            },
            provenance: Provenance::Lexicon,
        });
        let rule = plan.add_node(PlanNodeKind::Leaf {
            fragment: FragmentSpec::RewriteRule { rule: PRuleId(3) },
            provenance: Provenance::RewriteRule(PRuleId(3)),
        });
        let replace = plan.add_node(PlanNodeKind::Replace {
            cascade: ReplaceCascadeSpec {
                rules: vec![PRuleId(3)],
                gated_subrules: vec![GatedSubruleRef {
                    rule_pos: 0,
                    sub_idx: 0,
                }],
                group_key: vec![true],
            },
            children: vec![rule],
        });
        let compose = plan.add_node(PlanNodeKind::Compose {
            children: vec![lexicon, replace],
            strategy: ComposeStrategy::Static,
        });
        let gate = plan.add_node(PlanNodeKind::Gate {
            partition: GatePartitionSpec {
                gated_subrules: vec![GatedSubruleRef {
                    rule_pos: 0,
                    sub_idx: 0,
                }],
                groups: vec![GateGroupSpec { key: vec![true] }],
            },
            children: vec![compose],
        });
        let marker = plan.add_node(PlanNodeKind::Leaf {
            fragment: FragmentSpec::CompositeEmissionMarker,
            provenance: Provenance::CompositeEmission,
        });
        let root = plan.add_node(PlanNodeKind::Union {
            children: vec![gate, marker],
        });
        plan.set_root(root);
        plan
    }

    /// The required property: a Plan document survives JSON and a decode back to a `Plan` with its
    /// DIGEST unchanged. Both hops matter -- serialization alone would not catch a decoder that
    /// rebuilt a subtly different plan.
    #[test]
    fn a_plan_document_round_trips_and_preserves_its_digest() {
        let plan = sample_plan();
        let document = PortablePlan::encode(&plan);
        let digest = document.digest();
        assert!(digest.starts_with("sha256:"), "{digest}");
        assert_eq!(digest.len(), "sha256:".len() + 64);

        let json = document.canonical_json();
        let parsed = PortablePlan::from_json(&json).expect("document must parse");
        assert_eq!(parsed, document, "JSON round-trip must be lossless");
        assert_eq!(
            parsed.digest(),
            digest,
            "JSON round-trip must not move the digest"
        );

        let rebuilt = parsed.decode().expect("document must decode");
        assert_eq!(
            rebuilt.root(),
            plan.root(),
            "root must survive the round-trip"
        );
        assert_eq!(rebuilt.len(), plan.len(), "every node must survive");
        let reencoded = PortablePlan::encode(&rebuilt);
        assert_eq!(
            reencoded, document,
            "re-encoding a decoded plan must be identical"
        );
        assert_eq!(reencoded.digest(), digest, "and therefore digest-identical");
    }

    /// Encoding is deterministic across two independently built plans with the same content --
    /// otherwise the digest would identify a process, not an artifact.
    #[test]
    fn two_independently_built_identical_plans_digest_alike() {
        assert_eq!(
            PortablePlan::encode(&sample_plan()).digest(),
            PortablePlan::encode(&sample_plan()).digest()
        );
    }

    /// A tampered content address is REFUSED, not repaired. The decoder recomputes every id from
    /// the decoded content, so relabelling one node cannot be absorbed.
    #[test]
    fn a_tampered_node_id_is_refused_rather_than_repaired() {
        let mut document = PortablePlan::encode(&sample_plan());
        // Tamper with a LEAF's declared id, the case a lenient decoder would most easily absorb
        // (nothing about a leaf's payload has to change for its id to be wrong).
        let victim = document
            .nodes
            .iter_mut()
            .find(|node| matches!(node.node, PortableNodeKind::Leaf { .. }))
            .expect("sample plan has leaves");
        let original = victim.id.clone();
        victim.id = "0000000000000000".to_owned();

        match document.decode() {
            Err(PortablePlanError::ContentAddressMismatch {
                declared,
                recomputed,
            }) => {
                assert_eq!(declared, "0000000000000000");
                assert_eq!(
                    recomputed, original,
                    "the refusal must name what the content actually hashes to"
                );
            }
            // A dangling parent reference is an equally valid refusal for this tamper, since the
            // parents still name the ORIGINAL id. Either way it must be a refusal.
            Err(PortablePlanError::UnknownNode { id, .. }) => assert_eq!(id, original),
            other => panic!("a tampered id must be refused, got {other:?}"),
        }
    }

    /// The same for a tampered PAYLOAD, which is the harder half: the ids all still cross-reference
    /// correctly, and only recomputing the content address catches it.
    #[test]
    fn a_tampered_payload_is_refused_rather_than_repaired() {
        let mut document = PortablePlan::encode(&sample_plan());
        for node in &mut document.nodes {
            if let PortableNodeKind::Leaf {
                fragment: PortableFragment::LexiconFragment { entries },
                ..
            } = &mut node.node
            {
                *entries = Some(vec![WireModelId {
                    kind: WireModelKind::LexEntry,
                    value: 99,
                }]);
            }
        }
        assert!(
            matches!(
                document.decode(),
                Err(PortablePlanError::ContentAddressMismatch { .. })
            ),
            "a payload edit under an unchanged id must be refused"
        );
    }

    /// A wire id in the wrong domain is a typed decode error, not a coerced integer. This is what
    /// the kind tag buys: without it, a `PRuleId(3)` written where a `LexEntryId` belongs would
    /// decode happily into a plan that means something else.
    #[test]
    fn a_wire_id_in_the_wrong_domain_is_refused() {
        let mut document = PortablePlan::encode(&sample_plan());
        for node in &mut document.nodes {
            if let PortableNodeKind::Leaf {
                fragment: PortableFragment::LexiconFragment { entries },
                ..
            } = &mut node.node
            {
                *entries = Some(vec![WireModelId {
                    kind: WireModelKind::PRule,
                    value: 0,
                }]);
            }
        }
        assert!(
            matches!(
                document.decode(),
                Err(PortablePlanError::InvalidWireId {
                    expected: WireModelKind::LexEntry,
                    ..
                })
            ),
            "a PRule id where a LexEntry belongs must be a typed refusal"
        );
    }

    /// A `Gate` node whose children and partition groups disagree is refused BEFORE interning.
    /// `Plan::add_node` guards the same invariant with a `debug_assert!`, which panics in a debug
    /// build and is silent in a release one -- neither is an answer to untrusted input.
    #[test]
    fn a_gate_arity_mismatch_is_refused_before_interning() {
        let mut document = PortablePlan::encode(&sample_plan());
        for node in &mut document.nodes {
            if let PortableNodeKind::Gate { partition, .. } = &mut node.node {
                partition
                    .groups
                    .push(PortableGateGroup { key: vec![false] });
            }
        }
        assert!(
            matches!(
                document.decode(),
                Err(PortablePlanError::GateArityMismatch { .. })
            ),
            "a Gate with 2 groups and 1 child must be refused"
        );
    }

    /// A child naming a node the document does not declare is refused rather than dropped.
    #[test]
    fn a_dangling_child_is_refused_rather_than_dropped() {
        let mut document = PortablePlan::encode(&sample_plan());
        for node in &mut document.nodes {
            if let PortableNodeKind::Union { children } = &mut node.node {
                children.push("deadbeefdeadbeef".to_owned());
            }
        }
        assert!(
            matches!(
                document.decode(),
                Err(PortablePlanError::UnknownNode { .. })
            ) || matches!(
                document.decode(),
                Err(PortablePlanError::ContentAddressMismatch { .. })
            ),
            "a dangling child must be refused"
        );
    }

    #[test]
    fn an_unsupported_schema_is_refused() {
        let mut document = PortablePlan::encode(&sample_plan());
        document.schema_version = 99;
        // `Plan` is not `PartialEq`, so the Result cannot be compared directly.
        assert!(matches!(
            document.decode(),
            Err(PortablePlanError::UnsupportedSchema(99))
        ));
    }

    /// The adapter axis is 1:1 with the strategy axis in BOTH directions -- required for the
    /// adapter identity to soundly stand in for the enum.
    #[test]
    fn every_strategy_has_exactly_one_adapter_and_back() {
        for &strategy in crate::strategy_coverage::ALL_STRATEGIES {
            assert_eq!(
                LoweringAdapter::for_strategy(strategy).strategy(),
                strategy,
                "adapter/strategy correspondence must be total and injective"
            );
        }
        let adapters: BTreeSet<LoweringAdapter> = crate::strategy_coverage::ALL_STRATEGIES
            .iter()
            .map(|&s| LoweringAdapter::for_strategy(s))
            .collect();
        assert_eq!(
            adapters.len(),
            crate::strategy_coverage::ALL_STRATEGIES.len()
        );
        assert_eq!(
            adapters
                .iter()
                .filter(|adapter| adapter.interprets_plan())
                .count(),
            1,
            "exactly one adapter reads a plan"
        );
    }
}
