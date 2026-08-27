//! The compilation `Plan`: a content-addressed AND-OR DAG over a closed node-kind enum.
//!
//! Compilation topology is data here rather than control flow, so a compiler can be COMPOSED from
//! parts instead of hand-written. `crate::enumerate` emits a `Plan` for a `Grammar`;
//! `crate::build` interprets one into real `foma::types::Fsm`s.
//!
//! `emit`/`preexpand` keep their own hardcoded seams and do not consult a `Plan`.
//!
//! # The five node kinds
//! - `PlanNodeKind::Leaf` — an atomic FST-to-be-compiled-from-source: a `FragmentSpec`
//!   describing *what* it will encode, plus a `Provenance` recording *which grammar construct*
//!   it encodes (the source of the capability-evidence-provenance field). No live `Fsm`
//!   here — that comes later, from an interpreter over this data.
//! - `PlanNodeKind::Compose` — n-ary composition (Allauzen & Mohri's 3-way composition result:
//!   n-ary is cost-relevant, not sugar for a binary fold), tagged with a `ComposeStrategy` kept
//!   deliberately separate from topology so a cost model can vary it per edge.
//! - `PlanNodeKind::Union` — merges independently-compiled branches.
//! - `PlanNodeKind::Gate` — `gate.rs`'s subrule-gated partition-and-union, promoted to a named
//!   node kind; see `GatePartitionSpec`.
//! - `PlanNodeKind::Replace` — `replace.rs`'s rewrite-cascade construction, promoted to a named
//!   node kind; see `ReplaceCascadeSpec`.
//!
//! The enum is closed **on purpose**: adding a node kind is a closed-set change, so
//! every exhaustive match over node kinds must fail to compile until a new kind is handled. Every
//! `match` in this file over `PlanNodeKind` is written with no catch-all arm, demonstrating that
//! discipline; see `PlanNodeKind::children` and `PlanNodeKind::kind_name`.
//!
//! # Content addressing
//! `NodeId = hash(kind, child NodeIds, config)`. This is what makes (a) the plan a usable cache
//! key, (b) cross-plan subtree sharing possible (two plans differing only in how the phonological
//! cascade is grouped share their identical lexicon leaves — measured once, stored once), and (c)
//! a future memoized AND-OR search actually work. A tree would force duplicating shared subtrees;
//! see `Plan`'s doc for the arena/interner that makes this concrete.

use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};

use pg_grammar::model::{LexEntryId, MRuleId, PRuleId, TemplateId};

// Content-addressed node identity

/// FNV-1a, unseeded: deterministic across processes, unlike `DefaultHasher`'s per-run `RandomState`; not collision-resistant, an accepted tradeoff for same-process subtree dedup.
#[derive(Clone)]
struct StableHasher(u64);

impl StableHasher {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        StableHasher(Self::OFFSET_BASIS)
    }
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }
}

/// Content-addressed identity of a plan node. Two nodes with equal `PlanNodeKind` content
/// (same kind, same children, same config) always produce the same `NodeId`; two nodes differing
/// in ANY of those — including a config-only difference like `ComposeStrategy` — produce
/// different ids. This is what lets `Plan::add_node` dedup identical subtrees for free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u64);

impl NodeId {
    /// The raw 64-bit content address, for callers that want to persist, compare, or log it (e.g.
    /// as the plan-cache key, or for tagging fixtures by the node addresses they
    /// exercise).
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// `kind`'s content address: hashes the whole value, which already covers the discriminant, children, and config.
fn content_address(kind: &PlanNodeKind) -> NodeId {
    let mut hasher = StableHasher::new();
    kind.hash(&mut hasher);
    NodeId(hasher.finish())
}

// Compose strategy: physical strategy kept separate from topology

/// The *physical* strategy for a `PlanNodeKind::Compose` node, kept as a separate enum from
/// topology so a future cost model can vary the strategy per edge without the enumerator
/// having to emit a combinatorially separate `Compose` topology for every strategy choice.
///
/// Single-variant today: `crate::build::build_controllable` and `crate::enumerate`'s
/// `refine_gate_partition` both construct and interpret only `Static`. The enum stays (rather
/// than collapsing `Compose { strategy, .. }` to drop the field) because a config-only difference
/// is still a content-address difference: a future strategy belongs
/// in this enum, not in a new field elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeStrategy {
    /// Materialize-then-trim: builds the full composed network eagerly, the only strategy in use.
    Static,
}

impl Hash for ComposeStrategy {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Manual discriminant hash: derived `Hash` emits no bytes for a single-variant enum, which would change every existing Static node's NodeId.
        match self {
            Self::Static => 0_isize.hash(state),
        }
    }
}

// Leaf payload: FragmentSpec (what to compile) + Provenance (which grammar construct)

/// What a `PlanNodeKind::Leaf` will be compiled from — the *compile-shape* descriptor a builder
/// needs to know HOW to produce this leaf's `Fsm`. Deliberately lightweight: no grammar
/// data is embedded beyond stable ids, and no `Fsm` is built here at all — building/executing a
/// plan into real FSTs happens later.
///
/// Kept **separate** from `Provenance` even though the two overlap for most of today's leaf
/// kinds: `FragmentSpec` answers "what source material, compiled how", while `Provenance` answers
/// "which grammar construct to attribute this to" for capability-evidence/coverage purposes. They
/// can diverge — e.g. a single composite-emission leaf's `FragmentSpec` is one
/// opaque marker, but its `Provenance` may need to name several contributing templates for
/// coverage tagging — so this type keeps them as two fields rather than collapsing them into one,
/// even though most of today's variants line up one-to-one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FragmentSpec {
    /// A lexc-compiled lexicon fragment; `entries = None` means the whole grammar's lexicon, `Some(_)` an explicit subset.
    LexiconFragment { entries: Option<Vec<LexEntryId>> },
    /// A single rewrite rule's transducer, addressed by its `PRuleId` (cascade position).
    RewriteRule { rule: PRuleId },
    /// A gate/guard automaton for one partition group's gating key.
    GuardAutomaton { group_key: Vec<bool> },
    /// Opaque marker resolved against the grammar by an interpreter, not this descriptor: whatever `preexpand`/`emit` already build for this grammar's composite entries.
    CompositeEmissionMarker,
    /// Opaque marker for `emit::build_structural_composites`'s route (circumfixes/subtractive rules, plus any affix rule where `emit::probe_would_refuse` holds); kept distinct from `CompositeEmissionMarker` because the two gate on different seams and a grammar can need either, both, or neither.
    StructuralCompositeMarker,
}

/// Which grammar construct a `PlanNodeKind::Leaf` (or `PlanNodeKind::Replace`) encodes —
/// records which grammar construct it encodes, and is the source of the
/// capability-evidence-provenance field. This type only needs
/// a stable, content-addressable "what grammar-level thing does this node come from" tag so a
/// capability-evidence-provenance consumer can wire it through unchanged rather than re-deriving it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Provenance {
    /// The grammar's lexicon as a whole.
    Lexicon,
    /// A single phonological rewrite rule.
    RewriteRule(PRuleId),
    /// A single morphological rule (an affix chain member).
    MorphRule(MRuleId),
    /// An affix template.
    Template(TemplateId),
    /// A gate/guard automaton (see `GatePartitionSpec`).
    Gate,
    /// A rewrite-cascade construction as a whole (`replace.rs`).
    Replace,
    /// A composite-emission subtree (multi-tag composite entries).
    CompositeEmission,
    /// A structural-composite subtree; kept distinct from `Self::CompositeEmission` per `FragmentSpec::StructuralCompositeMarker`'s doc.
    StructuralComposite,
}

// Gate payload: gate.rs's partition-and-union, promoted to a named node kind

/// One `(rule position in cascade order, subrule index within that rule)` pair the partition keys
/// on — the same shape as `gate::GatedSubrule`, duplicated here (not re-imported) because this
/// step is data-only: a `Plan` must be constructible without a live `Grammar`/`PhonRuleDef` slice
/// in hand, whereas `gate::GatedSubrule` is computed FROM one by `gate::find_gated_subrules`. A
/// later step is expected to convert between the two, not share a type across the "descriptor" /
/// "computed-from-a-grammar" boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GatedSubruleRef {
    pub rule_pos: usize,
    pub sub_idx: usize,
}

/// One partition group's gating key — the same shape as `gate::EntryGroup::key`, without the
/// `HashSet<LexEntryId>` of member entries (that membership is real grammar data computed by
/// `gate::partition_entries`, out of scope for this data-only descriptor; a `Gate` node's
/// `children` carry the already-compiled-per-group subplans instead).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GateGroupSpec {
    pub key: Vec<bool>,
}

/// The partition descriptor for a `PlanNodeKind::Gate` node: it can reference the gate-key
/// concept without recomputing it here. `gated_subrules` names which
/// subrules the partition keys on; `groups` lists one entry per distinct gating key realized by
/// the grammar, in the same order as the `Gate` node's `children` (one compiled child subplan per
/// group). An ungated grammar collapses to exactly one group with an empty key ("ungated
/// grammar collapses to a single-group `Gate`"), the pre-refactor behavior preserved as a specific
/// enumerable plan rather than a special case.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GatePartitionSpec {
    pub gated_subrules: Vec<GatedSubruleRef>,
    pub groups: Vec<GateGroupSpec>,
}

// Replace payload: replace.rs's rewrite-cascade construction, promoted to a named node kind

/// The cascade descriptor for a `PlanNodeKind::Replace` node: the ordered rewrite rules the
/// cascade applies, addressed by `PRuleId` in cascade order.
///
/// **`gated_subrules` + `group_key`:** a `Replace`
/// node compiled underneath a `PlanNodeKind::Gate` group must exclude/include specific SUBRULES
/// per that group's own gating key (`crate::replace::compile_and_compose_rules_gated`'s
/// `subrule_ok` callback) -- a fact that, before this fix, lived only on the `Gate` node's
/// `GatePartitionSpec`, NOT on the `Replace` node's own content. That made two groups needing
/// DIFFERENT `subrule_ok` behavior reference the SAME `Replace` `NodeId` (`crate::enumerate`'s own
/// `enumerate_default` built exactly one shared `Replace` node for every group), which is unsound
/// for any `NodeId`-keyed cache/memoizing interpreter: the compiled artifact for that shared
/// `NodeId` is NOT a pure function of the id, it also depends on which group is asking.
///
/// The fix: every `Replace` node now carries its OWN group's subrule inclusion directly, in the
/// same shape `GatePartitionSpec` already uses for the analogous group-level facts --
/// `gated_subrules` mirrors `GatePartitionSpec::gated_subrules` (which `(rule_pos, sub_idx)`
/// subrule positions are gated at all) and `group_key` mirrors `GateGroupSpec::key` (THIS
/// cascade's own truth value for each of those positions, same indexing). Two `Replace` nodes are
/// now the SAME `NodeId` iff they share `rules` AND `gated_subrules` AND `group_key` -- i.e. iff
/// they would compile to the identical `subrule_ok` predicate -- so distinct groups (different
/// `group_key`) always get distinct `NodeId`s (no more false sharing), while two groups that
/// happen to gate identically (same `group_key`, e.g. built from two different grammars/`Plan`s
/// that realize the same key) still dedup correctly, because their compiled artifact really would
/// be identical. An ungated cascade (no gated subrules apply at all -- the pre-refactor "1 group,
/// empty key" collapse) carries `gated_subrules: vec![]` / `group_key: vec![]`, matching
/// `GatePartitionSpec`'s own ungated-collapse shape.
///
/// `Replace` is content-pure with this fix in place (`crate::build`'s own module doc): its
/// compiled `Fsm` depends on nothing this struct doesn't already carry, so a future `NodeId`-keyed
/// plan-cache/memoizing interpreter can treat it like any other node, not a `Gate`-aware special
/// case.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReplaceCascadeSpec {
    pub rules: Vec<PRuleId>,
    /// Which `(rule_pos, sub_idx)` subrule positions this cascade's `subrule_ok` gates on at all --
    /// same shape as `GatePartitionSpec::gated_subrules`. Empty for an ungated cascade.
    pub gated_subrules: Vec<GatedSubruleRef>,
    /// THIS cascade's own truth value for each of `gated_subrules`, same indexing -- same shape as
    /// `GateGroupSpec::key`. Empty for an ungated cascade (vacuously matches an empty
    /// `gated_subrules`).
    pub group_key: Vec<bool>,
}

// The closed node-kind enum

/// The closed set of compilation-plan node kinds. Exactly five variants — no catch-all is
/// ever written against this enum in this file: any `match` over node kinds fails to
/// compile until a new kind is handled.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlanNodeKind {
    /// An atomic FST-to-be-compiled-from-source. See `FragmentSpec`/`Provenance`'s docs.
    Leaf {
        fragment: FragmentSpec,
        provenance: Provenance,
    },
    /// N-ary composition (Allauzen & Mohri: cost-relevant, not sugar for a binary fold); `strategy` is the physical strategy, kept separate from topology.
    Compose {
        children: Vec<NodeId>,
        strategy: ComposeStrategy,
    },
    /// Merges independently-compiled branches; legal only where the characteristics check's orthogonality predicate licenses it (checked elsewhere).
    Union { children: Vec<NodeId> },
    /// `gate.rs`'s subrule-gated partition-and-union, promoted to a named node kind.
    Gate {
        partition: GatePartitionSpec,
        children: Vec<NodeId>,
    },
    /// `replace.rs`'s rewrite-cascade construction as a named node kind rather than an opaque `Compose`: its order sensitivity and alpha-tuple resolution are specific to it, and the enumerator needs to recognize a cascade as a cascade.
    Replace {
        cascade: ReplaceCascadeSpec,
        children: Vec<NodeId>,
    },
}

impl PlanNodeKind {
    /// Every child this node references, in order. An exhaustive match with no catch-all arm —
    /// adding a sixth `PlanNodeKind` variant fails this
    /// function to compile until it is handled here too.
    pub fn children(&self) -> &[NodeId] {
        match self {
            PlanNodeKind::Leaf { .. } => &[],
            PlanNodeKind::Compose { children, .. } => children,
            PlanNodeKind::Union { children } => children,
            PlanNodeKind::Gate { children, .. } => children,
            PlanNodeKind::Replace { children, .. } => children,
        }
    }

    /// A short, stable label for this node's kind — used for diagnostics/logging (and, potentially,
    /// fixture-tagging-by-node-kind later). Another exhaustive match with no catch-all arm.
    pub fn kind_name(&self) -> &'static str {
        match self {
            PlanNodeKind::Leaf { .. } => "Leaf",
            PlanNodeKind::Compose { .. } => "Compose",
            PlanNodeKind::Union { .. } => "Union",
            PlanNodeKind::Gate { .. } => "Gate",
            PlanNodeKind::Replace { .. } => "Replace",
        }
    }
}

// The Plan arena/interner

/// The arena of interned nodes for one compilation plan, plus its root: a `Plan`
/// arena/interner so that constructing an identical subtree twice yields the SAME `NodeId` and
/// stores it once — this is the whole point of the DAG-not-tree decision: shared subtrees dedup.
///
/// Nodes are stored keyed by content address in a `BTreeMap` rather than a `HashMap` so iteration
/// order is itself deterministic — a small extra property this data type gets for free that a
/// later differential-oracle pass or coverage report will want when diffing/reporting
/// over two plans' node sets.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    nodes: BTreeMap<NodeId, PlanNodeKind>,
    root: Option<NodeId>,
}

impl Plan {
    /// A new, empty plan with no root set.
    pub fn new() -> Self {
        Plan::default()
    }

    /// Interns `kind`, returning its content-addressed `NodeId`. If a node with the same content
    /// address is already present, the existing entry is kept and `kind` is dropped without adding
    /// new storage — this dedup IS the DAG-sharing behavior this type requires, not an optimization
    /// layered on top of it: constructing the identical subtree twice through this method always
    /// yields one stored node and one `NodeId`.
    ///
    /// Debug-only invariant check (never a behavior change in a release build, and not a
    /// substitute for a real validated builder — this module ships a data type, not a validator): a
    /// `PlanNodeKind::Gate` node's `children` length must equal its `partition.groups` length
    /// (one compiled child subplan per partition group).
    pub fn add_node(&mut self, kind: PlanNodeKind) -> NodeId {
        if let PlanNodeKind::Gate {
            partition,
            children,
        } = &kind
        {
            debug_assert_eq!(
                partition.groups.len(),
                children.len(),
                "Gate node must have exactly one child per partition group"
            );
        }
        let id = content_address(&kind);
        if let Some(existing) = self.nodes.get(&id) {
            // Debug-only collision check: a 64-bit hash makes two distinct nodes aliasing to the same NodeId unlikely but not impossible, and a silent alias would corrupt any oracle comparing subplans.
            debug_assert_eq!(
                *existing, kind,
                "content-address collision: two distinct plan nodes hashed to the same NodeId {id}"
            );
        } else {
            self.nodes.insert(id, kind);
        }
        id
    }

    /// Looks up a node by its content address.
    pub fn get(&self, id: NodeId) -> Option<&PlanNodeKind> {
        self.nodes.get(&id)
    }

    /// `true` iff `id` is interned in this plan.
    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// The number of distinct interned nodes (post-dedup) — the DAG's real storage/measurement
    /// cost, per the "measured once, stored once" claim above.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// `true` iff no nodes have been interned yet.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Every interned `(NodeId, &PlanNodeKind)` pair, in content-address order (deterministic —
    /// see this struct's own doc).
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &PlanNodeKind)> {
        self.nodes.iter().map(|(id, kind)| (*id, kind))
    }

    /// Marks `root` as this plan's root node. Does not require `root` to already be interned in
    /// this `Plan` (callers are expected to `add_node` it first in practice, but this data type
    /// does not itself enforce that at this step).
    pub fn set_root(&mut self, root: NodeId) {
        self.root = Some(root);
    }

    /// This plan's root node, if one has been set.
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lexicon_leaf() -> PlanNodeKind {
        PlanNodeKind::Leaf {
            fragment: FragmentSpec::LexiconFragment { entries: None },
            provenance: Provenance::Lexicon,
        }
    }

    fn rule_leaf(id: u32) -> PlanNodeKind {
        PlanNodeKind::Leaf {
            fragment: FragmentSpec::RewriteRule { rule: PRuleId(id) },
            provenance: Provenance::RewriteRule(PRuleId(id)),
        }
    }

    /// The core dedup claim: constructing the identical subtree twice yields the same `NodeId` and stores it once.
    #[test]
    fn identical_subtree_interned_once() {
        let mut plan = Plan::new();
        let a = plan.add_node(lexicon_leaf());
        let b = plan.add_node(lexicon_leaf());
        assert_eq!(
            a, b,
            "constructing the same leaf twice must yield the same NodeId"
        );
        assert_eq!(plan.len(), 1, "must be stored exactly once, not duplicated");
    }

    /// The core content-addressing claim at the `ReplaceCascadeSpec` level: two `Replace` nodes with the same `rules`/`gated_subrules` but different `group_key` get different `NodeId`s, while identical ones dedup to one.
    #[test]
    fn replace_nodes_differing_only_in_group_key_yield_different_ids_but_identical_keys_dedup() {
        let mut plan = Plan::new();
        let leaf = plan.add_node(rule_leaf(1));
        let gated_subrules = vec![GatedSubruleRef {
            rule_pos: 0,
            sub_idx: 0,
        }];

        let replace_true = plan.add_node(PlanNodeKind::Replace {
            cascade: ReplaceCascadeSpec {
                rules: vec![PRuleId(1)],
                gated_subrules: gated_subrules.clone(),
                group_key: vec![true],
            },
            children: vec![leaf],
        });
        let replace_false = plan.add_node(PlanNodeKind::Replace {
            cascade: ReplaceCascadeSpec {
                rules: vec![PRuleId(1)],
                gated_subrules: gated_subrules.clone(),
                group_key: vec![false],
            },
            children: vec![leaf],
        });
        assert_ne!(
            replace_true, replace_false,
            "two Replace nodes differing only in group_key are different candidate compiled \
             cascades (different subrule_ok), and must get different NodeIds -- this is the task \
             1.4 fix's whole point"
        );

        let replace_true_again = plan.add_node(PlanNodeKind::Replace {
            cascade: ReplaceCascadeSpec {
                rules: vec![PRuleId(1)],
                gated_subrules,
                group_key: vec![true],
            },
            children: vec![leaf],
        });
        assert_eq!(
            replace_true, replace_true_again,
            "two Replace nodes with an IDENTICAL group_key (same rules, same gated_subrules) must \
             still dedup to the SAME NodeId -- the fix must not break sound sharing"
        );
        assert_eq!(
            plan.len(),
            3,
            "leaf + 2 distinct Replace nodes (true/false), the redundant true-again NOT duplicated"
        );
    }

    /// Two parents sharing one child leaf: the leaf is stored once, not once per parent.
    #[test]
    fn shared_child_leaf_stored_once_across_two_parents() {
        let mut plan = Plan::new();
        let shared_leaf = plan.add_node(lexicon_leaf());
        let rule_a = plan.add_node(rule_leaf(1));
        let rule_b = plan.add_node(rule_leaf(2));

        let parent_a = plan.add_node(PlanNodeKind::Compose {
            children: vec![shared_leaf, rule_a],
            strategy: ComposeStrategy::Static,
        });
        let parent_b = plan.add_node(PlanNodeKind::Compose {
            children: vec![shared_leaf, rule_b],
            strategy: ComposeStrategy::Static,
        });

        assert_ne!(
            parent_a, parent_b,
            "the two parents differ in one child, so must differ"
        );
        // 1 shared leaf + 2 rule leaves + 2 parents = 5 stored nodes, not 6 (a tree would store the shared leaf twice).
        assert_eq!(plan.len(), 5);
        assert!(plan.contains(shared_leaf));
        assert_eq!(plan.get(parent_a).unwrap().children()[0], shared_leaf);
        assert_eq!(plan.get(parent_b).unwrap().children()[0], shared_leaf);
    }

    /// Pins Static's legacy hash tag (from when Lazy variants existed); changing it moves every Compose node's `NodeId` and can alter candidate ordering.
    #[test]
    fn static_compose_strategy_preserves_legacy_hash_tag() {
        let mut hasher = StableHasher::new();
        ComposeStrategy::Static.hash(&mut hasher);
        assert_eq!(hasher.finish(), 0xa8c7_f832_281a_39c5);
    }

    /// Two independently built `Plan`s with identical content produce equal `NodeId`s for corresponding nodes.
    #[test]
    fn content_addresses_are_stable_across_independently_built_plans() {
        let mut plan_1 = Plan::new();
        let leaf_1 = plan_1.add_node(lexicon_leaf());
        let root_1 = plan_1.add_node(PlanNodeKind::Compose {
            children: vec![leaf_1],
            strategy: ComposeStrategy::Static,
        });

        let mut plan_2 = Plan::new();
        let leaf_2 = plan_2.add_node(lexicon_leaf());
        let root_2 = plan_2.add_node(PlanNodeKind::Compose {
            children: vec![leaf_2],
            strategy: ComposeStrategy::Static,
        });

        assert_eq!(
            leaf_1, leaf_2,
            "identical leaf content built in two independent plans must hash identically"
        );
        assert_eq!(
            root_1, root_2,
            "identical Compose content (same child id, same strategy) built independently must \
             hash identically"
        );
    }

    /// A well-formed `Gate` node (children length matches partition group count) interns cleanly.
    #[test]
    fn gate_node_with_matching_children_and_groups_interns() {
        let mut plan = Plan::new();
        let group_a = plan.add_node(rule_leaf(1));
        let group_b = plan.add_node(rule_leaf(2));
        let partition = GatePartitionSpec {
            gated_subrules: vec![GatedSubruleRef {
                rule_pos: 0,
                sub_idx: 0,
            }],
            groups: vec![
                GateGroupSpec { key: vec![true] },
                GateGroupSpec { key: vec![false] },
            ],
        };
        let gate = plan.add_node(PlanNodeKind::Gate {
            partition,
            children: vec![group_a, group_b],
        });
        assert!(plan.get(gate).is_some());
        assert_eq!(plan.get(gate).unwrap().kind_name(), "Gate");
    }

    /// `Plan::add_node`'s debug-only invariant on a `Gate` node whose `children` count doesn't match its partition's group count; gated on `debug_assertions` because `debug_assert!` is stripped in release, where a `#[should_panic]` test of it would fail.
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "one child per partition group")]
    fn gate_node_invariant_panics_in_debug_on_mismatch() {
        let mut plan = Plan::new();
        let group_a = plan.add_node(rule_leaf(1));
        let partition = GatePartitionSpec {
            gated_subrules: vec![],
            groups: vec![
                GateGroupSpec { key: vec![true] },
                GateGroupSpec { key: vec![false] },
            ],
        };
        plan.add_node(PlanNodeKind::Gate {
            partition,
            // Only 1 child for 2 groups -- must trip the invariant.
            children: vec![group_a],
        });
    }

    /// Exercises `PlanNodeKind::children`/`kind_name`'s exhaustive matches over every variant; a new variant fails the FILE to compile, not this test.
    #[test]
    fn kind_name_and_children_cover_every_node_kind() {
        let mut plan = Plan::new();

        let leaf = plan.add_node(lexicon_leaf());
        assert_eq!(plan.get(leaf).unwrap().kind_name(), "Leaf");
        assert!(plan.get(leaf).unwrap().children().is_empty());

        let compose = plan.add_node(PlanNodeKind::Compose {
            children: vec![leaf],
            strategy: ComposeStrategy::Static,
        });
        assert_eq!(plan.get(compose).unwrap().kind_name(), "Compose");
        assert_eq!(plan.get(compose).unwrap().children(), &[leaf]);

        let union = plan.add_node(PlanNodeKind::Union {
            children: vec![leaf, compose],
        });
        assert_eq!(plan.get(union).unwrap().kind_name(), "Union");
        assert_eq!(plan.get(union).unwrap().children(), &[leaf, compose]);

        let replace = plan.add_node(PlanNodeKind::Replace {
            cascade: ReplaceCascadeSpec {
                rules: vec![PRuleId(0), PRuleId(1)],
                gated_subrules: vec![],
                group_key: vec![],
            },
            children: vec![leaf],
        });
        assert_eq!(plan.get(replace).unwrap().kind_name(), "Replace");
        assert_eq!(plan.get(replace).unwrap().children(), &[leaf]);

        let gate = plan.add_node(PlanNodeKind::Gate {
            partition: GatePartitionSpec {
                gated_subrules: vec![],
                groups: vec![GateGroupSpec { key: vec![] }],
            },
            children: vec![leaf],
        });
        assert_eq!(plan.get(gate).unwrap().kind_name(), "Gate");
        assert_eq!(plan.get(gate).unwrap().children(), &[leaf]);

        assert_eq!(plan.len(), 5);
        assert_eq!(plan.iter().count(), 5);
    }

    /// `Plan::set_root`/`Plan::root` round-trip.
    #[test]
    fn root_round_trips() {
        let mut plan = Plan::new();
        assert_eq!(plan.root(), None);
        let leaf = plan.add_node(lexicon_leaf());
        plan.set_root(leaf);
        assert_eq!(plan.root(), Some(leaf));
    }
}
