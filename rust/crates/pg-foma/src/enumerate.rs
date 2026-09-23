//! `enumerate_default` builds today's compilation topology as a single reified `Plan`
//! (`crate::plan`), verified structurally against the REAL seam functions rather than a
//! re-derivation of their decisions.
//!
//! This module does not implement a plan builder/interpreter (no live `foma::types::Fsm` is
//! built anywhere here), and does not modify the bodies of the three seam functions it mirrors
//! (their *visibility* was widened from private to `pub(crate)` so this module and its tests can
//! call them directly — see `crate::emit::probe_would_refuse`/[`crate::emit::
//! structural_candidate_rules`]'s own doc comments for that rationale; [`crate::preexpand::
//! should_run`] and `crate::gate::partition_entries`/`crate::gate::find_gated_subrules` were
//! already `pub(crate)`/`pub`). Building/executing a `Plan` into real FSTs stays out of scope
//! here (`crate::plan`'s own module doc; `crate::build`'s `build_controllable` is the one
//! interpreter, over the CONTROLLABLE Gate/Replace/Compose subtree only). `crate::emit::
//! plan_topology_decisions` calls `enumerate_default` and reads the built `Plan`'s composite-
//! emission/structural-composite marker presence to decide `emit_with_budget_profiled`'s own
//! topology, replacing that function's independent `preexpand::should_run`/`structural_candidate_
//! rules(...).is_empty()` calls — see that function's own doc. `gate::partition_entries` stays
//! unwired into `emit.rs`'s mainline: that seam belongs to `gate.rs`'s own,
//! separate compile entry point, which `emit.rs`'s lexc-emission path never calls at all.
//!
//! # The three seams and how this module models each
//!
//! | Row | Real seam | Modeled as |
//! |---|---|---|
//! | 1 | `crate::preexpand::should_run` | An `Option<NodeId>` for a `PlanNodeKind::Leaf` tagged `FragmentSpec::CompositeEmissionMarker` — present iff `should_run` is `true`. |
//! | 2 | `crate::emit::probe_would_refuse` / `crate::emit::structural_candidate_rules` | An `Option<NodeId>` for a `PlanNodeKind::Leaf` tagged `FragmentSpec::StructuralCompositeMarker` — present iff `structural_candidate_rules(g)` is non-empty (the REAL gate `emit::emit_with_budget` uses, `!struct_rules.is_empty()`; a strict superset of "`probe_would_refuse` alone" — see that function's own doc). |
//! | 3 | `crate::gate::partition_entries` / `crate::gate::find_gated_subrules` | A `PlanNodeKind::Gate` node whose `partition.groups` has one `GateGroupSpec` per group `partition_entries` yields, one child subplan per group. An ungated grammar collapses to a single-group `Gate` with an empty key (`GatePartitionSpec`'s doc) — the pre-refactor behavior preserved as a specific enumerable plan, not a special case. |
//!
//! # Shape (mirrors `emit::emit_with_budget`'s own compose order: lexicon, composites, structural
//! composites, rules cascade, gate partition)
//! ```text
//! root = Union[ Gate{ partition, children = one Compose per group },
//!               composite-emission Leaf?,       // present iff should_run
//!               structural-composite Leaf? ]    // present iff structural_candidate_rules non-empty
//!
//! each group's Compose = Compose[ group's LexiconFragment Leaf (entries = Some(that group's own
//!                                 sorted entries), mirrors `gate::EntryGroup::entries`),
//!                                 THIS GROUP'S OWN Replace node ]
//!
//! each group's own Replace node = Replace{ cascade = ReplaceCascadeSpec{ rules = prules_in_order's
//!                                    PRuleIds, in order; gated_subrules = the SAME gated-subrule
//!                                    universe for every group; group_key = THIS group's own key },
//!                                    children = one RewriteRule Leaf per rule (content-identical
//!                                    across every group, so these dedup even though the parent
//!                                    Replace node itself does not) }
//! ```
//! The `Union` at the root (rather than nesting the composite-emission/structural-composite markers
//! *inside* each group) is a judgment call, not free of ambiguity — see "Judgment calls" below.
//!
//! # Judgment calls
//!
//! - **Composite/structural markers sit OUTSIDE the `Gate` node, as `Union` siblings, not nested
//!   inside every group.** `crate::gate`'s own module doc says gated compilation's affix chains are
//!   "shared, unfiltered" across every partition group — i.e. NOT gated at all. Composite/structural
//!   entries are exactly this kind of grammar-wide, non-partitioned material in today's code (in
//!   fact `crate::gate`'s prototype compile path doesn't even call `crate::preexpand`/structural
//!   composites at all today — those live only in `emit::emit_with_budget`'s mainline path). One
//!   enumerator that treats all three seams as choices over the SAME plan doesn't match what today's
//!   code literally does (they're two separate compile entry points): this module's shape is
//!   the natural unification, not a literal mirror of one existing function's call graph.
//! - **Every gate group gets its OWN `Replace` node.** An EARLIER version of this module built one
//!   Replace node SHARED by
//!   every group, on the reasoning that `ReplaceCascadeSpec` is rule-level (`Vec<PRuleId>`) and
//!   the per-group subrule-inclusion distinction lives on `GatePartitionSpec::gated_subrules` +
//!   each group's own `key`, so duplicating it into `Replace` would be "redundant, not more
//!   faithful." That turned out to be UNSOUND: `crate::build`'s `build_controllable` needs a
//!   DIFFERENT `subrule_ok` per group, so a single shared `Replace` `NodeId` violates node purity
//!   (a `NodeId`-memoizing interpreter would build the cascade once and silently reuse the WRONG
//!   network for every other group). The fix: `ReplaceCascadeSpec` now carries `gated_subrules` +
//!   `group_key` directly (see that struct's own doc), so THIS group's own `Replace` node's content
//!   fully determines its `subrule_ok` — content-addressing then does the right thing on its
//!   own: two groups with DIFFERENT keys get DIFFERENT `Replace` `NodeId`s (no false sharing, this
//!   module's own tests assert it), while two groups that happen to gate IDENTICALLY still dedup to
//!   the SAME `Replace` `NodeId` (also asserted) — the shared rewrite-rule Leaf CHILDREN still dedup
//!   across every group either way, since those leaves' content never depended on the group at all.
//! - **Per-group `LexiconFragment.entries` is always `Some(sorted group entries)`**, never `None`,
//!   even for the single/ungated group — mirrors `compile_gated_grammar`'s own call
//!   (`emit_underlying_filtered(g, alphabet, Some(&group.entries))`, ALWAYS
//!   `Some`, never `None`, even when there is exactly one group covering every entry). `entries`
//!   is sorted (bucketed through a `HashSet` in `gate::EntryGroup`, so insertion order is not
//!   itself stable) for the same reproducibility reason `partition_entries`' own group order is
//!   sorted below.
//! - **`partition_entries`'s returned `Vec<EntryGroup>` is re-sorted by key** before this module
//!   builds the `Gate` node. `partition_entries` buckets through a `HashMap`, whose iteration order
//!   is not stable across processes; a `Gate` node's `children` order is part of its content
//!   address (`PlanNodeKind::Gate`'s derived `Hash`), so leaving it in `HashMap` order would make
//!   the SAME grammar's enumerated `Plan` hash differently between runs — a direct violation of the
//!   requirement that NodeIds be reproducible across processes. This is enumerate.rs's own
//!   post-processing of the seam's return value, not a change to `partition_entries` itself.
//! - **Recovering each `prules_in_order` entry's `PRuleId`** (needed for `ReplaceCascadeSpec` and
//!   `FragmentSpec::RewriteRule`, which are addressed by the grammar-wide id, NOT by position in
//!   `prules_in_order` — that's `GatedSubruleRef::rule_pos`'s own, different, addressing scheme) is
//!   done by pointer identity against `g.prules` (see `rule_id_of`'s own doc) — every
//!   `prules_in_order` construction site in this crate (`gate.rs`'s/`replace.rs`'s own test
//!   harnesses, `emit.rs`'s production callers) builds it as literal borrows of `g.prules` elements,
//!   never copies, so this is safe, not a hack of convenience.

use pg_grammar::model::{Grammar, LexEntryId, PRuleId, PhonRuleDef};

use crate::gate::{find_gated_subrules, partition_entries};
use crate::junctions::PhonologyProbe;
use crate::oracle::permute_gate_groups;
use crate::plan::{
    ComposeStrategy, FragmentSpec, GateGroupSpec, GatePartitionSpec, GatedSubruleRef, NodeId, Plan,
    PlanNodeKind, Provenance, ReplaceCascadeSpec,
};
use crate::{emit, preexpand};

/// `g`'s phonological rules in stratum-cascade (authored) order, as literal borrows of `g.prules` —
/// the exact slice `enumerate_default`, `crate::gate::compile_gated_grammar`,
/// `crate::gate::find_gated_subrules` and `crate::replace`'s cascade builders all take.
///
/// The borrow-from-`g.prules` part is load-bearing, not stylistic: `rule_id_of` recovers a
/// `PRuleId` by POINTER IDENTITY against `g.prules`, so a caller that clones or re-collects the
/// rules panics there. That is the single reason this exists as one shared helper rather than as a
/// three-line idiom copied per call site — every production copy of it was byte-identical, and a
/// divergent one is a panic, not a warning.
///
/// Test modules in this crate keep their own private copies on purpose — test modules don't share
/// private helpers across files, which is why `crate::capability`'s, this module's and
/// `crate::selection`'s test modules each still build the slice themselves. Only PRODUCTION call
/// sites route through here.
pub fn prules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
    g.strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &g.prules[id.0 as usize])
        .collect()
}

/// Builds today's compilation topology for `g` as a single reified `Plan`.
///
/// Takes the grammar, authored-order phonological rules, and optional phonology probe used by
/// today's topology seams.
pub fn enumerate_default(
    g: &Grammar,
    prules_in_order: &[&PhonRuleDef],
    phon: Option<&PhonologyProbe<'_>>,
) -> Plan {
    let mut plan = Plan::new();

    // Row 1: preexpand::should_run -> composite-emission subtree presence.
    let composite_leaf = preexpand::should_run(g, phon).then(|| {
        plan.add_node(PlanNodeKind::Leaf {
            fragment: FragmentSpec::CompositeEmissionMarker,
            provenance: Provenance::CompositeEmission,
        })
    });

    // Row 2: structural_candidate_rules -> structural-composite subtree presence, mirroring emit::emit_with_budget's own gate.
    let structural_leaf = (!emit::structural_candidate_rules(g).is_empty()).then(|| {
        plan.add_node(PlanNodeKind::Leaf {
            fragment: FragmentSpec::StructuralCompositeMarker,
            provenance: Provenance::StructuralComposite,
        })
    });

    // Row 3: gate::find_gated_subrules / gate::partition_entries -> the Gate node's partition.
    let gated = find_gated_subrules(g, prules_in_order);
    let mut groups = partition_entries(g, &gated, prules_in_order);
    // Reproducibility: `partition_entries` buckets through a `HashMap`, so re-sort by key before this order becomes part of the Gate node's content address.
    groups.sort_by(|a, b| a.key.cmp(&b.key));

    // The gated-subrule universe is the same for every group, computed once and cloned into each group's own Replace node below.
    let gated_subrule_refs: Vec<GatedSubruleRef> = gated
        .iter()
        .map(|gs| GatedSubruleRef {
            rule_pos: gs.rule_pos,
            sub_idx: gs.sub_idx,
        })
        .collect();
    let cascade_rules: Vec<PRuleId> = prules_in_order.iter().map(|pr| rule_id_of(g, pr)).collect();

    // The rewrite-rule Leaf children are content-identical regardless of which group compiles them, so they dedup across every group's Replace node even though the Replace parent itself no longer does.
    let rule_children: Vec<NodeId> = prules_in_order
        .iter()
        .map(|pr| {
            let rule = rule_id_of(g, pr);
            plan.add_node(PlanNodeKind::Leaf {
                fragment: FragmentSpec::RewriteRule { rule },
                provenance: Provenance::RewriteRule(rule),
            })
        })
        .collect();

    // One Compose (group lexicon fragment .o. this group's own Replace node) per partition group; each Replace carries its own group_key, so distinct groups get distinct NodeIds.
    let group_children: Vec<NodeId> = groups
        .iter()
        .map(|group| {
            let mut entries: Vec<LexEntryId> = group.entries.iter().copied().collect();
            entries.sort();
            let lexicon_leaf = plan.add_node(PlanNodeKind::Leaf {
                fragment: FragmentSpec::LexiconFragment {
                    entries: Some(entries),
                },
                provenance: Provenance::Lexicon,
            });
            let replace_node = plan.add_node(PlanNodeKind::Replace {
                cascade: ReplaceCascadeSpec {
                    rules: cascade_rules.clone(),
                    gated_subrules: gated_subrule_refs.clone(),
                    group_key: group.key.clone(),
                },
                children: rule_children.clone(),
            });
            plan.add_node(PlanNodeKind::Compose {
                children: vec![lexicon_leaf, replace_node],
                strategy: ComposeStrategy::Static,
            })
        })
        .collect();

    let gate_node = plan.add_node(PlanNodeKind::Gate {
        partition: GatePartitionSpec {
            gated_subrules: gated_subrule_refs,
            groups: groups
                .iter()
                .map(|group| GateGroupSpec {
                    key: group.key.clone(),
                })
                .collect(),
        },
        children: group_children,
    });

    // Root: the gate-partitioned, rule-composed lexicon, unioned with whichever composite-emission markers this grammar's should_run/structural facts license.
    let mut root_children = vec![gate_node];
    root_children.extend(composite_leaf);
    root_children.extend(structural_leaf);
    let root = if root_children.len() == 1 {
        root_children[0]
    } else {
        plan.add_node(PlanNodeKind::Union {
            children: root_children,
        })
    };
    plan.set_root(root);

    plan
}

/// Whether a candidate IS this grammar's default compilation, stated by whoever built it.
///
/// # Why this is a field and not a position
/// It used to be a parallel `is_baseline: &[bool]` slice passed alongside the candidate slice, and
/// before that it was position zero. Both were wrong, in ways that were measured rather than
/// theorised:
///
/// * **Position.** The production optimizer evaluates candidates ONE AT A TIME —
///   `pg_cli`'s `CandidateEvaluator::evaluate` calls in with `std::slice::from_ref(candidate)` — so
///   every candidate is "element zero". A positional test answered `true` for all of them and every
///   permutation of a marker-requiring plan took the baseline's whole-grammar route and was reported
///   as confirmed with the baseline's own network counts.
/// * **A parallel slice.** The fix for that was a caller-supplied `&[bool]`, kept honest only by a
///   length `assert_eq!` whose own message admitted the hazard ("a mismatch here is how a
///   permutation would silently be treated as the baseline"). A slice of the right LENGTH but the
///   wrong ORDER is exactly as wrong as position was, and nothing could detect it.
///
/// Carried on the candidate, the fact travels with the thing it is a fact about; reordering,
/// filtering, or evaluating a single candidate cannot separate them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateRole {
    /// This grammar's default compilation; a baseline needing marker subtrees that
    /// `build_controllable` cannot build is rejected rather than rerouted to another adapter.
    Baseline,
    /// Any other candidate; never realized by a whole-grammar adapter it did not ask for, since that adapter derives its own topology and would measure the baseline network instead.
    Alternative,
}

impl CandidateRole {
    pub fn is_baseline(self) -> bool {
        matches!(self, Self::Baseline)
    }
}

/// One candidate topology `enumerate_candidates` emits, labeled for provenance/diagnostics.
/// The label is a static string naming
/// WHICH axis produced this candidate (`"default"`, `"gate-group-permuted"`, ...), not a
/// user-facing description — see `crate::selection::select_plan`'s own doc for how this feeds a
/// caller's provenance report.
///
/// # Why the compiler axis is `EmissionStrategy`, and the baseline fact lives here
/// 1. **The compiler axis is `EmissionStrategy` itself**, not a second enum kept in
///    correspondence with it by hand (`crate::backend::backend_for` is the one place that turns a
///    strategy into behaviour, via the `crate::backend::Backend` trait). It is also the REPORTED
///    selection axis (`RuntimeEvaluation::realized_strategy`,
///    `BackendOptimizationReport::winner_strategy`, `strategy_coverage`), measured to be the
///    decisive one — two whole-grammar compilers win two different languages.
/// 2. **The baseline fact lives here**, as `CandidateRole` — see that type for the two measured
///    failures of putting it anywhere else.
#[derive(Debug)]
pub struct LoweredCandidate {
    pub label: &'static str,
    pub plan: Plan,
    /// WHICH compiler realizes this candidate. A different axis from `plan`, which describes
    /// assembly SHAPE within the one strategy that reads a plan at all
    /// (`crate::backend::Backend::interprets_plan`).
    pub adapter: EmissionStrategy,
    /// Whether this candidate is the grammar's default compilation.
    pub role: CandidateRole,
}

impl LoweredCandidate {
    /// This candidate's `EmissionStrategy` — the axis reports and `strategy_coverage` speak in.
    pub fn strategy(&self) -> EmissionStrategy {
        self.adapter
    }

    pub fn is_baseline(&self) -> bool {
        self.role.is_baseline()
    }
}

/// Which of this crate's compilers realizes a candidate.
///
/// # Why this is a separate axis from the `Plan`
/// A `Plan` describes how already-emitted fragments are ASSEMBLED (`Gate`/`Union`/`Compose` shape).
/// Measured on eight marker-free synthetic fixtures, varying only that shape leaves `states`, `arcs`,
/// `proposals`, and `confirmation` bit-identical across candidates — the assembly ends in a
/// minimization step that canonicalizes the difference away. Only `build` time moved, and only
/// upward (partition refinement: 2.1x-5.2x the baseline, non-overlapping over ten runs). So plan
/// shape alone cannot express a better compilation, and a registry that varies only plan shape is
/// searching a space whose interesting dimension is fixed.
///
/// The axis that is NOT erased by minimization is which lexc a grammar is compiled to in the first
/// place, because that changes what gets composed rather than the order of composing it:
///
/// * `Self::TunedSurfaceProbed` bakes phonology into the lexc via `emit`'s surface probe, then
///   patches the resulting expressive gaps with synthesized composite entries
///   (`preexpand::build_composites`, `emit::build_structural_composites`) — the material the `Plan`
///   can only NAME, via its `CompositeEmissionMarker`/`StructuralCompositeMarker` leaves.
/// * `Self::TemplatedUnderlyingTokens` emits plain char-def tokens and lets a real compiled
///   rewrite cascade do the phonological work. Verified: `emit::emit_underlying_templated` contains
///   no composite/pre-expansion machinery at all, so this strategy needs none of that material.
///
/// Those are two complete, semantically-valid compilations of the SAME grammar that reach the same
/// upper tape (both emit `tags::root_tag_lexc`/`morph_tag_lexc`), which is what makes them
/// comparable by `oracle::differential_oracle` and certifiable against the same full-HC corpus —
/// and, before this type existed, they had never been compared, because only one of them was ever
/// offered as a candidate.
///
/// Plain data the pack format and the Runtime also read (a compiled pack names the backend it
/// came from), so the type itself lives in `pg-health`; re-exported here at its historical path.
pub use pg_health::strategy::EmissionStrategy;

/// The candidate
/// ENUMERATOR — every legal, **buildable** topology this crate can emit for `g` today, as
/// content-addressed `Plan`s a caller (typically `crate::selection::select_plan`) can filter by
/// capability and rank by cost. Always emits `enumerate_default`'s own plan first (candidate
/// `"default"`); the selection story only becomes meaningful once there is a second, genuinely
/// distinct candidate to choose between.
///
/// # Which axes are emitted, and why
///
/// **Emitted: gate-group order** (candidate `"gate-group-permuted"`, via `permute_gate_groups`).
/// `crate::oracle`'s own module doc proves this is sound and non-vacuous:
/// `crate::build::build_controllable` folds every `Gate` group's compiled network together with
/// `foma::constructions::fsm_union` (commutative) and always finishes with
/// `foma::minimize::fsm_minimize`, so a `Gate` node's group ORDER cannot affect the
/// final relation — only membership does. Reordering the groups changes the `Gate` node's content
/// address (`NodeId = hash(kind, children, config)`, and both `partition.groups` and `children`
/// are part of that content) without changing what the built network recognizes: a real, distinct,
/// SAME-relation candidate topology, not a relabeling of the identical `Plan`. **Only added when it
/// is actually a different plan**: a grammar with 0 or 1 partition groups reverses to the identical
/// `Vec`, so `permute_gate_groups` would return a `Plan` with the SAME root `NodeId` — appending it
/// would just be the `"default"` candidate wearing a second label, which is not a genuine
/// alternative for `crate::selection::select_plan` to weigh. This function checks the roots differ
/// before appending, so the returned `Vec` has length 1 for an ungated/single-group grammar and
/// length 2 once there are ≥2 groups to reorder.
///
/// # Which axes are deliberately NOT emitted yet, and why
///
/// - **Reordering the root `Union`'s composite-emission/structural-composite marker children.**
///   `enumerate_default`'s own module doc already notes `Union`'s commutativity makes child order
///   semantically inert; the reason this is still not a candidate axis is that neither marker leaf
///   is interpreted by `crate::build::build_controllable` at all (that module's own scope note: markers
///   are a separate, black-box lexc-`String` artifact, "out of scope for this step"). Permuting
///   `Union` children would therefore change a content address without changing anything
///   `crate::build::build_controllable` can measure or build differently — no genuine topology choice,
///   just churn.
/// - **An alternative partition function for the `Gate` node** (grouping entries differently than
///   `crate::gate::partition_entries` does). No second partition-computing seam exists anywhere in
///   this crate; inventing one here would mean re-deriving `gate.rs`'s own gating semantics a second,
///   independent way — squarely the kind of change this module's own scope excludes: this file does
///   not reach into `gate.rs` to manufacture a second partition strategy it was never asked to build.
/// - **Reordering a `Replace` cascade's rule sequence.** Unlike gate-group order, rewrite-rule order
///   is NOT proven irrelevant — `replace.rs`'s cascade is explicitly order-sensitive (each rule's
///   output feeds the next), so two different rule orders are not, in general, the SAME relation at
///   all. Emitting a reordered-cascade candidate here would risk exactly the failure this design
///   rules out by construction ("selection can never pick a fast-but-wrong plan"): a candidate that LOOKS like an
///   alternative topology for the same logical request but actually computes a different relation.
///   Absent a proof of order-irrelevance (which no seam in this crate currently supplies), this axis
///   is left unexplored rather than emitted unsoundly.
pub fn enumerate_candidates(
    g: &Grammar,
    prules_in_order: &[&PhonRuleDef],
    phon: Option<&PhonologyProbe<'_>>,
) -> Vec<LoweredCandidate> {
    let default_plan = enumerate_default(g, prules_in_order, phon);
    let mut candidates = vec![LoweredCandidate {
        label: "default",
        plan: default_plan,
        adapter: EmissionStrategy::PlanComposed,
        // Stated here rather than inferred from position, which is the whole point of `CandidateRole`.
        role: CandidateRole::Baseline,
    }];

    let permuted = permute_gate_groups(&candidates[0].plan);
    if permuted.root() != candidates[0].plan.root() {
        candidates.push(LoweredCandidate {
            label: "gate-group-permuted",
            plan: permuted,
            adapter: EmissionStrategy::PlanComposed,
            role: CandidateRole::Alternative,
        });
    }

    candidates
}

/// Recovers `pr`'s `PRuleId` (its index into `Grammar::prules`) from a `prules_in_order` entry,
/// by pointer identity — see this module's own doc ("Judgment calls") for why this is safe, not a
/// hack: every construction site for a `prules_in_order` slice in this crate borrows its elements
/// directly from `g.prules` (`&g.prules[id.0 as usize]`), never copies them, so the reference's
/// address uniquely identifies its source index.
///
/// # Panics
/// If `pr` is not found in `g.prules` by pointer identity — this would mean a caller passed a
/// `prules_in_order` slice NOT borrowed from this same `g`, which is a caller bug this function
/// cannot silently paper over (silently returning a wrong `PRuleId` would corrupt every downstream
/// consumer of that id, e.g. capability-evidence-provenance tagging).
///
/// Widened from private to `pub(crate)` for `crate::build`: that module's own
/// `validate_replace_cascade` needs the identical pointer-identity `PRuleId` recovery to
/// cross-check a `Plan`'s `Replace` cascade against a caller-supplied `prules_in_order` slice, and
/// re-deriving the same safety-relevant logic a second time would risk the two copies silently
/// drifting apart.
pub(crate) fn rule_id_of(g: &Grammar, pr: &PhonRuleDef) -> PRuleId {
    let idx = g
        .prules
        .iter()
        .position(|candidate| std::ptr::eq(candidate, pr))
        .unwrap_or_else(|| {
            panic!(
                "prules_in_order entry not found in g.prules by pointer identity -- caller must \
                 pass slices borrowed directly from g.prules"
            )
        });
    PRuleId(idx as u32)
}

#[cfg(test)]
mod tests;
