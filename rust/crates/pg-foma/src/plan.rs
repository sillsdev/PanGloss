//! Reified compilation `Plan` (Step 1 of `openspec/changes/reify-compilation-plans`,
//! `docs`/design.md decision **D1**): a content-addressed AND-OR DAG over a **closed** node-kind
//! enum, replacing the idea of "the compile step's topology" with first-class, enumerable data.
//!
//! This module is **purely additive** and does **not** rewire `replace.rs`/`gate.rs`/`emit.rs`/
//! `preexpand.rs` — it defines the `Plan` data type only. Building a `Plan` into real [`foma`]
//! [`foma::types::Fsm`]s, a strategy enumerator that emits candidate plans for a `Grammar`, and
//! swapping the three hardcoded seams (`preexpand::should_run`, `emit::probe_would_refuse`,
//! `gate::partition_entries`) for enumerator decisions (design D2) are later steps — see
//! `openspec/changes/reify-compilation-plans/tasks.md` §1.2/§1.3. Nothing in this file is wired
//! into `analyzer`/`composite`/any other module's compile path yet.
//!
//! # The five node kinds (D1)
//! - [`PlanNodeKind::Leaf`] — an atomic FST-to-be-compiled-from-source: a [`FragmentSpec`]
//!   describing *what* it will encode, plus a [`Provenance`] recording *which grammar construct*
//!   it encodes (the source of ADR 0001's capability-evidence-provenance field). No live `Fsm`
//!   here — that is Step 2.
//! - [`PlanNodeKind::Compose`] — n-ary composition (Allauzen & Mohri's 3-way composition result:
//!   n-ary is cost-relevant, not sugar for a binary fold), tagged with a [`ComposeStrategy`] kept
//!   deliberately separate from topology so a cost model can vary it per edge.
//! - [`PlanNodeKind::Union`] — merges independently-compiled branches.
//! - [`PlanNodeKind::Gate`] — `gate.rs`'s subrule-gated partition-and-union, promoted to a named
//!   node kind; see [`GatePartitionSpec`].
//! - [`PlanNodeKind::Replace`] — `replace.rs`'s rewrite-cascade construction, promoted to a named
//!   node kind; see [`ReplaceCascadeSpec`].
//!
//! The enum is closed **on purpose** (spec.md: "Adding a node kind is a closed-set change" —
//! every exhaustive match over node kinds must fail to compile until a new kind is handled). Every
//! `match` in this file over [`PlanNodeKind`] is written with no catch-all arm, demonstrating that
//! discipline; see [`PlanNodeKind::children`] and [`PlanNodeKind::kind_name`].
//!
//! # Content addressing (D1)
//! `NodeId = hash(kind, child NodeIds, config)`. This is what makes (a) the plan a usable cache
//! key, (b) cross-plan subtree sharing possible (two plans differing only in how the phonological
//! cascade is grouped share their identical lexicon leaves — measured once, stored once), and (c)
//! a future memoized AND-OR search actually work. A tree would force duplicating shared subtrees;
//! see [`Plan`]'s doc for the arena/interner that makes this concrete.

use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};

use pg_grammar::model::{LexEntryId, MRuleId, PRuleId, TemplateId};

// ---------------------------------------------------------------------------------------------
// Content-addressed node identity
// ---------------------------------------------------------------------------------------------

/// A hand-rolled, unseeded 64-bit FNV-1a (Fowler-Noll-Vo) hasher.
///
/// `NodeId`s must be **reproducible across processes** (D1: the plan cache key, cross-plan subtree
/// sharing, D5's fixture tagging by node address all depend on the same logical node hashing to
/// the same id every time it is built). `std::collections::hash_map::DefaultHasher` is explicitly
/// wrong here: its `RandomState` reseeds per-process, so the same content hashes differently on
/// every run. FNV-1a has no seed/salt at all, so it is deterministic across processes for a fixed
/// plan schema and Rust hashing ABI, and is small enough not to justify a new dependency
/// (design.md D1: "a small hand-rolled stable hash is fine and preferred"). The preimage still
/// comes from standard Hash implementations, whose encoding is not a portable persistence format
/// across platforms or compiler versions; any future cross-toolchain cache needs a separately
/// versioned canonical encoding.
///
/// Not collision-resistant, and that is an accepted tradeoff at this data-modeling step: a 64-bit
/// non-cryptographic hash is adequate for "is this the same subtree" over the grammars this crate
/// compiles today. Revisit if collision risk ever becomes a real concern (a Step 2+ question, not
/// this additive step's).
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

/// Content-addressed identity of a plan node (D1). Two nodes with equal `PlanNodeKind` content
/// (same kind, same children, same config) always produce the same `NodeId`; two nodes differing
/// in ANY of those — including a config-only difference like [`ComposeStrategy`] — produce
/// different ids. This is what lets [`Plan::add_node`] dedup identical subtrees for free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u64);

impl NodeId {
    /// The raw 64-bit content address, for callers that want to persist, compare, or log it (e.g.
    /// as the plan-cache key D1 names, or D5's tagging fixtures by the node addresses they
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

/// Computes `kind`'s content address (D1). `PlanNodeKind`'s derived [`Hash`] impl already covers
/// "kind tag" (the enum discriminant) + every field of the matched variant, including nested
/// `Vec<NodeId>` children and config payloads — so hashing the whole value is exactly
/// `hash(kind, child NodeIds, config)`, not an approximation of it.
fn content_address(kind: &PlanNodeKind) -> NodeId {
    let mut hasher = StableHasher::new();
    kind.hash(&mut hasher);
    NodeId(hasher.finish())
}

// ---------------------------------------------------------------------------------------------
// Compose strategy (D1: "physical strategy kept separate from topology")
// ---------------------------------------------------------------------------------------------

/// The *physical* strategy for a [`PlanNodeKind::Compose`] node, kept as a separate enum from
/// topology (D1) so a future cost model can vary the strategy per edge without the enumerator
/// having to emit a combinatorially separate `Compose` topology for every strategy choice.
///
/// Single-variant today: `crate::build::build_controllable` and `crate::enumerate`'s
/// `refine_gate_partition` both construct and interpret only `Static`. The enum stays (rather
/// than collapsing `Compose { strategy, .. }` to drop the field) because it is D1's
/// "config-only difference is still a content-address difference" axis: a future strategy belongs
/// in this enum, not in a new field elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeStrategy {
    /// Materialize-then-trim: build the full composed network eagerly. The only strategy any
    /// builder in this crate constructs or interprets.
    Static,
}

impl Hash for ComposeStrategy {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Preserve the explicit discriminant encoding used when this enum had multiple variants.
        // Rust's derived `Hash` emits no bytes for a single-variant enum, which would otherwise
        // change every existing Static compose node's persistent content address merely because
        // the two unconstructible variants were deleted.
        match self {
            Self::Static => 0_isize.hash(state),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Leaf payload: FragmentSpec (what to compile) + Provenance (which grammar construct)
// ---------------------------------------------------------------------------------------------

/// What a [`PlanNodeKind::Leaf`] will be compiled from — the *compile-shape* descriptor a builder
/// (Step 2) needs to know HOW to produce this leaf's `Fsm`. Deliberately lightweight: no grammar
/// data is embedded beyond stable ids, and no `Fsm` is built here at all (design.md's own scope
/// note: "building/executing a plan into real FSTs is Step 2").
///
/// Kept **separate** from [`Provenance`] even though the two overlap for most of today's leaf
/// kinds: `FragmentSpec` answers "what source material, compiled how", while `Provenance` answers
/// "which grammar construct to attribute this to" for capability-evidence/coverage purposes (ADR
/// 0001, D5). They can diverge — e.g. a single composite-emission leaf's `FragmentSpec` is one
/// opaque marker, but its `Provenance` may need to name several contributing templates for
/// coverage tagging — so this step keeps them as two fields rather than collapsing them into one,
/// even though most of today's variants line up one-to-one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FragmentSpec {
    /// A lexc-compiled lexicon fragment. `entries = None` means the whole grammar's lexicon;
    /// `Some(_)` names an explicit subset (mirrors `gate::EntryGroup::entries`, without
    /// recomputing it here — see [`GatePartitionSpec`]'s doc).
    LexiconFragment { entries: Option<Vec<LexEntryId>> },
    /// A single rewrite rule's transducer, addressed by its `PRuleId` (cascade position).
    RewriteRule { rule: PRuleId },
    /// A gate/guard automaton for one partition group's gating key (mirrors
    /// `gate::EntryGroup::key`).
    GuardAutomaton { group_key: Vec<bool> },
    /// A composite-emission subtree marker: "compile whatever `preexpand`/`emit` already build for
    /// this grammar's composite entries" (see those modules' own docs for what "composite" means).
    /// Opaque at this step — Step 2 resolves it against the grammar, not this descriptor.
    CompositeEmissionMarker,
    /// A structural-composite subtree marker (Step 2, design.md D2 row 2: `emit::probe_would_refuse`
    /// / `emit::structural_candidate_rules`): "compile whatever `emit::build_structural_composites`
    /// already builds for this grammar's structural-candidate rules" — rules `crate::preexpand`'s
    /// ordinary composite mechanism cannot represent at all (circumfixes, subtractive/dropped-LHS-
    /// material rules), plus every ordinary `Prefix`/`Suffix`/`Infix` rule when
    /// `emit::probe_would_refuse` holds (that construct's own doc). Kept as a DISTINCT marker from
    /// [`Self::CompositeEmissionMarker`] even though both are "opaque, Step-2-resolves-it-against-
    /// the-grammar" leaves: they gate on two different seams (`preexpand::should_run` vs.
    /// `emit::probe_would_refuse`/`structural_candidate_rules`) and a grammar can need either, both,
    /// or neither independently — collapsing them into one marker would lose that independence.
    StructuralCompositeMarker,
}

/// Which grammar construct a [`PlanNodeKind::Leaf`] (or [`PlanNodeKind::Replace`]) encodes (D1:
/// "`provenance` records which grammar construct it encodes and is the source of ADR 0001's
/// `capability evidence provenance` field"). ADR 0001 / `add-capability-characteristics-check`
/// (co-designed, not yet landed) owns the full evidence-provenance contract; this step only needs
/// a stable, content-addressable "what grammar-level thing does this node come from" tag so later
/// steps can wire it through unchanged rather than re-deriving it.
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
    /// A gate/guard automaton (see [`GatePartitionSpec`]).
    Gate,
    /// A rewrite-cascade construction as a whole (`replace.rs`).
    Replace,
    /// A composite-emission subtree (multi-tag composite entries).
    CompositeEmission,
    /// A structural-composite subtree (Step 2: the `emit::build_structural_composites` route — see
    /// [`FragmentSpec::StructuralCompositeMarker`]'s doc for why this is kept distinct from
    /// [`Self::CompositeEmission`]).
    StructuralComposite,
}

// ---------------------------------------------------------------------------------------------
// Gate payload (D1/D2: gate.rs's partition-and-union, promoted to a named node kind)
// ---------------------------------------------------------------------------------------------

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

/// The partition descriptor for a [`PlanNodeKind::Gate`] node (D1/D2: "the partition descriptor
/// can reference the gate-key concept without recomputing it here"). `gated_subrules` names which
/// subrules the partition keys on; `groups` lists one entry per distinct gating key realized by
/// the grammar, in the same order as the `Gate` node's `children` (one compiled child subplan per
/// group). An ungated grammar collapses to exactly one group with an empty key (D2: "ungated
/// grammar collapses to a single-group `Gate`", the pre-refactor behavior preserved as a specific
/// enumerable plan rather than a special case).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GatePartitionSpec {
    pub gated_subrules: Vec<GatedSubruleRef>,
    pub groups: Vec<GateGroupSpec>,
}

// ---------------------------------------------------------------------------------------------
// Replace payload (D1/D2: replace.rs's rewrite-cascade construction, promoted to a named node kind)
// ---------------------------------------------------------------------------------------------

/// The cascade descriptor for a [`PlanNodeKind::Replace`] node: the ordered rewrite rules the
/// cascade applies, addressed by `PRuleId` in cascade order.
///
/// **`gated_subrules` + `group_key` (design.md D1 "Soundness invariant", task 1.4):** a `Replace`
/// node compiled underneath a [`PlanNodeKind::Gate`] group must exclude/include specific SUBRULES
/// per that group's own gating key (`crate::replace::compile_and_compose_rules_gated_with_budget`'s
/// `subrule_ok` callback) -- a fact that, before this fix, lived only on the `Gate` node's
/// [`GatePartitionSpec`], NOT on the `Replace` node's own content. That made two groups needing
/// DIFFERENT `subrule_ok` behavior reference the SAME `Replace` `NodeId` (`crate::enumerate`'s own
/// `enumerate_default` built exactly one shared `Replace` node for every group), which is unsound
/// for any `NodeId`-keyed cache/memoizing interpreter: the compiled artifact for that shared
/// `NodeId` is NOT a pure function of the id, it also depends on which group is asking.
///
/// The fix: every `Replace` node now carries its OWN group's subrule inclusion directly, in the
/// same shape [`GatePartitionSpec`] already uses for the analogous group-level facts --
/// `gated_subrules` mirrors [`GatePartitionSpec::gated_subrules`] (which `(rule_pos, sub_idx)`
/// subrule positions are gated at all) and `group_key` mirrors [`GateGroupSpec::key`] (THIS
/// cascade's own truth value for each of those positions, same indexing). Two `Replace` nodes are
/// now the SAME `NodeId` iff they share `rules` AND `gated_subrules` AND `group_key` -- i.e. iff
/// they would compile to the identical `subrule_ok` predicate -- so distinct groups (different
/// `group_key`) always get distinct `NodeId`s (no more false sharing), while two groups that
/// happen to gate identically (same `group_key`, e.g. built from two different grammars/`Plan`s
/// that realize the same key) still dedup correctly, because their compiled artifact really would
/// be identical. An ungated cascade (no gated subrules apply at all -- the pre-refactor "1 group,
/// empty key" collapse) carries `gated_subrules: vec![]` / `group_key: vec![]`, matching
/// [`GatePartitionSpec`]'s own ungated-collapse shape.
///
/// `Replace` is content-pure with this fix in place (`crate::build`'s own module doc): its
/// compiled `Fsm` depends on nothing this struct doesn't already carry, so a future `NodeId`-keyed
/// plan-cache/memoizing interpreter can treat it like any other node, not a `Gate`-aware special
/// case.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReplaceCascadeSpec {
    pub rules: Vec<PRuleId>,
    /// Which `(rule_pos, sub_idx)` subrule positions this cascade's `subrule_ok` gates on at all --
    /// same shape as [`GatePartitionSpec::gated_subrules`]. Empty for an ungated cascade.
    pub gated_subrules: Vec<GatedSubruleRef>,
    /// THIS cascade's own truth value for each of `gated_subrules`, same indexing -- same shape as
    /// [`GateGroupSpec::key`]. Empty for an ungated cascade (vacuously matches an empty
    /// `gated_subrules`).
    pub group_key: Vec<bool>,
}

// ---------------------------------------------------------------------------------------------
// The closed node-kind enum
// ---------------------------------------------------------------------------------------------

/// The closed set of compilation-plan node kinds (D1). Exactly five variants — no catch-all is
/// ever written against this enum in this file (spec.md: "any `match` over node kinds fails to
/// compile until [a new kind] is handled").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlanNodeKind {
    /// An atomic FST-to-be-compiled-from-source. See [`FragmentSpec`]/[`Provenance`]'s docs.
    Leaf {
        fragment: FragmentSpec,
        provenance: Provenance,
    },
    /// N-ary composition (D1: "not binary — Allauzen & Mohri's 3-way composition result proves
    /// n-ary composition is strictly cost-relevant... when out-degrees are skewed"). `strategy` is
    /// the physical strategy, kept separate from topology (see [`ComposeStrategy`]'s doc).
    Compose {
        children: Vec<NodeId>,
        strategy: ComposeStrategy,
    },
    /// Merges independently-compiled branches. Legal only where the characteristics check's
    /// orthogonality predicate licenses it (design.md D1) — that check is a later step; this kind
    /// only needs to exist as data here.
    Union { children: Vec<NodeId> },
    /// `gate.rs`'s subrule-gated partition-and-union, promoted to a named node kind. See
    /// [`GatePartitionSpec`]'s doc for the invariant between `partition.groups` and `children`.
    Gate {
        partition: GatePartitionSpec,
        children: Vec<NodeId>,
    },
    /// `replace.rs`'s rewrite-cascade construction, promoted to a named node kind so a future
    /// enumerator can reorder/rewire around it (D1) rather than treating it as an opaque `Compose`.
    /// `children` are the cascade's ordered per-rule (or per-alpha-tuple) constituent subplans —
    /// the same shape as `Compose`'s children, but named separately because a rewrite cascade's
    /// order sensitivity and alpha-tuple resolution semantics (`replace.rs`'s
    /// `resolve_alpha_tuples`) are specific to this construction, not "any n-ary compose" — the
    /// enumerator needs to recognize a cascade AS a cascade (D2's `emit::probe_would_refuse` seam:
    /// whether the plan routes affix rules through the structural-composite subtree vs. the
    /// ordinary concatenative one is exactly a choice made *about* a `Replace` node, not a generic
    /// `Compose`).
    Replace {
        cascade: ReplaceCascadeSpec,
        children: Vec<NodeId>,
    },
}

impl PlanNodeKind {
    /// Every child this node references, in order. An exhaustive match with no catch-all arm
    /// (spec.md's closed-set requirement) — adding a sixth `PlanNodeKind` variant fails this
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

    /// A short, stable label for this node's kind — used for diagnostics/logging (and, later, D5's
    /// fixture-tagging-by-node-kind). Another exhaustive match with no catch-all arm.
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

// ---------------------------------------------------------------------------------------------
// The Plan arena/interner
// ---------------------------------------------------------------------------------------------

/// The arena of interned nodes for one compilation plan, plus its root (D1: "a `Plan`
/// arena/interner so that constructing an identical subtree twice yields the SAME `NodeId` and
/// stores it once — this is the whole point of the DAG-not-tree decision: shared subtrees dedup").
///
/// Nodes are stored keyed by content address in a `BTreeMap` rather than a `HashMap` so iteration
/// order is itself deterministic — a small extra property this data type gets for free that a
/// later differential-oracle pass (D4) or coverage report (D5) will want when diffing/reporting
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

    /// Interns `kind`, returning its content-addressed [`NodeId`]. If a node with the same content
    /// address is already present, the existing entry is kept and `kind` is dropped without adding
    /// new storage — this dedup IS the DAG-sharing behavior D1 requires, not an optimization
    /// layered on top of it: constructing the identical subtree twice through this method always
    /// yields one stored node and one `NodeId`.
    ///
    /// Debug-only invariant check (never a behavior change in a release build, and not a
    /// substitute for a real validated builder — this step ships a data type, not a validator): a
    /// [`PlanNodeKind::Gate`] node's `children` length must equal its `partition.groups` length
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
            // Dedup is correct only if equal content addresses imply equal content. A 64-bit
            // non-cryptographic hash makes that overwhelmingly likely but not certain; a collision
            // between two DISTINCT nodes would otherwise silently alias them (the existing node is
            // kept, the new one dropped), which would corrupt the differential-correctness oracle
            // (D4) by making two genuinely different subplans look identical. Turn that silent
            // hazard into a loud debug-mode failure, in the same spirit as the Gate invariant above
            // (never a release-mode behavior change). Revisit the hash width if this ever fires.
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
    /// cost, per D1's "measured once, stored once" claim.
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

    /// D1's core dedup claim: constructing the identical subtree twice yields the SAME `NodeId`
    /// and stores it once, not twice.
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

    /// Task 1.4's core content-addressing claim, at the `ReplaceCascadeSpec` level directly (no
    /// `Grammar`/`Gate` node involved -- `crate::enumerate`/`crate::build`'s own tests exercise the
    /// same property end-to-end on real seam-derived data; this is the minimal, data-only proof):
    /// two `Replace` nodes with the SAME `rules`/`gated_subrules` but DIFFERENT `group_key` must get
    /// DIFFERENT `NodeId`s (the soundness fix itself -- distinct groups never false-share), while
    /// two `Replace` nodes with IDENTICAL `rules`/`gated_subrules`/`group_key` must get the SAME
    /// `NodeId` and dedup to one stored node (identically-gated cascades still share correctly).
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

    /// A small hand-built DAG where two parents share one child leaf: the leaf is stored once, not
    /// once per parent (spec.md's "Two plans share a lexicon leaf" scenario, scaled down to two
    /// parents within one plan rather than two separate plans — the storage argument is the same).
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
        // 1 shared leaf + 2 distinct rule leaves + 2 distinct parents = 5 stored nodes, NOT 6 (a
        // tree would store the shared leaf twice, once per parent).
        assert_eq!(plan.len(), 5);
        assert!(plan.contains(shared_leaf));
        assert_eq!(plan.get(parent_a).unwrap().children()[0], shared_leaf);
        assert_eq!(plan.get(parent_b).unwrap().children()[0], shared_leaf);
    }

    /// The removed Lazy variants historically made derived Hash encode Static's discriminant as
    /// eight zero bytes on this target. Keep that legacy tag reserved: changing it moves every
    /// Compose node and its ancestors, which can alter candidate ordering and tie-breaks.
    #[test]
    fn static_compose_strategy_preserves_legacy_hash_tag() {
        let mut hasher = StableHasher::new();
        ComposeStrategy::Static.hash(&mut hasher);
        assert_eq!(hasher.finish(), 0xa8c7_f832_281a_39c5);
    }

    /// Content addresses are stable/deterministic: building the same plan (same nodes, same
    /// order) in two completely independent `Plan` instances yields equal `NodeId`s for
    /// corresponding nodes — the property that lets two different process runs (or two plans in
    /// the same enumerator pass) agree on a subtree's identity without communicating.
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

    /// The debug-only invariant in [`Plan::add_node`] catches a `Gate` node whose `children` count
    /// doesn't match its partition's group count.
    ///
    /// `#[cfg(debug_assertions)]`-gated because the invariant it exercises is a `debug_assert!`,
    /// which the compiler strips under `--release` — so a `#[should_panic]` test of it cannot pass
    /// there. Without this gate `cargo test --workspace --release` fails on this one test even though
    /// nothing is wrong (found by the delanguaging Part C sweep, which ran the release suite). Gating
    /// keeps the assertion genuinely tested in the debug profile — where `debug_assert!` actually
    /// fires and where the whole test suite normally runs — instead of weakening it to an
    /// always-checked `assert!` purely to satisfy a profile that strips it.
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

    /// Exercises [`PlanNodeKind::children`]/[`PlanNodeKind::kind_name`]'s exhaustive matches over
    /// every variant. If a sixth kind is ever added without updating those matches, THIS FILE
    /// fails to compile -- not this test (spec.md's "Adding a node kind is a closed-set change"
    /// scenario).
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

    /// [`Plan::set_root`]/[`Plan::root`] round-trip.
    #[test]
    fn root_round_trips() {
        let mut plan = Plan::new();
        assert_eq!(plan.root(), None);
        let leaf = plan.add_node(lexicon_leaf());
        plan.set_root(leaf);
        assert_eq!(plan.root(), Some(leaf));
    }
}
