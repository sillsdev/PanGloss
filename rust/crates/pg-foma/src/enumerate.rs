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
//!   even for the single/ungated group — mirrors `compile_gated_grammar_with_budget`'s own call
//!   (`emit_underlying_filtered_with_budget(g, alphabet, Some(&group.entries), budget)`, ALWAYS
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
use crate::lowering_adapter::LoweringAdapter;
use crate::oracle::permute_gate_groups;
use crate::plan::{
    ComposeStrategy, FragmentSpec, GateGroupSpec, GatePartitionSpec, GatedSubruleRef, NodeId, Plan,
    PlanNodeKind, Provenance, ReplaceCascadeSpec,
};
use crate::replace::SegAlphabet;
use crate::{emit, preexpand};

/// `g`'s phonological rules in stratum-cascade (authored) order, as literal borrows of `g.prules` —
/// the exact slice `enumerate_default`, `crate::gate::compile_gated_grammar_with_budget`,
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
/// Takes exactly the inputs the real compile seams take: `alphabet`/`prules_in_order` (the shape
/// `crate::gate::compile_gated_grammar_with_budget` and `crate::replace`'s cascade builders take)
/// plus `phon` (what `crate::preexpand::should_run` and `crate::emit::emit_with_budget` take).
///
/// `alphabet` is accepted, not read: this is data-only (module doc — no live `Fsm` is built
/// here), and none of today's three topology seams this mirrors ( `should_run`,
/// `probe_would_refuse`/`structural_candidate_rules`, `partition_entries`) consult the segment
/// alphabet to decide topology — only a real FST builder will need it. Kept as a parameter
/// anyway so this function's signature already matches what such a builder will need to thread
/// through, rather than growing a new parameter later.
pub fn enumerate_default(
    g: &Grammar,
    _alphabet: &SegAlphabet<'_>,
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
    /// This grammar's default compilation; a baseline needing marker subtrees `build_controllable` can't build is realized by the whole-grammar tuned adapter instead (see `crate::backend_runtime`).
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
/// # Why the compiler axis is a typed adapter, and the baseline fact lives here
/// 1. **The compiler axis is a typed `LoweringAdapter`, not an `EmissionStrategy`.** Lowering
///    dispatches on the adapter the candidate itself carries instead of on a second enum that had
///    to be kept in correspondence with it by hand. `EmissionStrategy` survives, deliberately:
///    it is the REPORTED selection axis (`RuntimeEvaluation::realized_strategy`,
///    `BackendOptimizationReport::winner_strategy`, `strategy_coverage`), measured to be the
///    decisive one — two whole-grammar compilers win two different languages. The
///    two are 1:1 in both directions (`lowering_adapter`'s own
///    `every_strategy_has_exactly_one_adapter_and_back`), so `Self::strategy` is a projection,
///    not a second source of truth.
/// 2. **The baseline fact lives here**, as `CandidateRole` — see that type for the two measured
///    failures of putting it anywhere else.
#[derive(Debug)]
pub struct LoweredCandidate {
    pub label: &'static str,
    pub plan: Plan,
    /// WHICH compiler lowers this candidate into a network. A different axis from `plan`, which
    /// describes assembly SHAPE within the one adapter that reads a plan at all.
    pub adapter: LoweringAdapter,
    /// Whether this candidate is the grammar's default compilation.
    pub role: CandidateRole,
}

impl LoweredCandidate {
    /// The `EmissionStrategy` this candidate's adapter realizes — the axis reports and
    /// `strategy_coverage` speak in. A projection of `Self::adapter`, never independent of it.
    pub fn strategy(&self) -> EmissionStrategy {
        self.adapter.strategy()
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmissionStrategy {
    /// Interpret `plan` with `build::build_controllable`: the only strategy honouring a plan's assembly shape and expressing a permutation, but it builds the controllable subtree only, omitting whatever marker leaves contribute.
    #[default]
    PlanComposed,
    /// `emit::emit` (surface-probed) + rules: whole-grammar, every construct covered, ignoring `plan` entirely since this compiler derives its own topology.
    TunedSurfaceProbed,
    /// `emit::emit_underlying_templated` + a compiled rewrite cascade: whole-grammar, composite-free, also ignoring `plan` for the same reason.
    TemplatedUnderlyingTokens,
}

impl EmissionStrategy {
    /// Whether this strategy realizes the whole grammar rather than the controllable subtree only.
    /// A caller comparing candidates across strategies needs this: a `PlanComposed` candidate on a
    /// marker-carrying grammar is not measuring the same object as either whole-grammar strategy.
    pub fn is_whole_grammar(self) -> bool {
        !matches!(self, Self::PlanComposed)
    }

    /// Stable identifier for reports and backend ids.
    pub fn label(self) -> &'static str {
        match self {
            Self::PlanComposed => "plan-composed",
            Self::TunedSurfaceProbed => "tuned-surface-probed",
            Self::TemplatedUnderlyingTokens => "templated-underlying-tokens",
        }
    }
}

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
/// `crate::compose_budget::union_checked` (commutative) and always finishes with
/// `crate::compose_budget::minimize_checked`, so a `Gate` node's group ORDER cannot affect the
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
    alphabet: &SegAlphabet<'_>,
    prules_in_order: &[&PhonRuleDef],
    phon: Option<&PhonologyProbe<'_>>,
) -> Vec<LoweredCandidate> {
    let default_plan = enumerate_default(g, alphabet, prules_in_order, phon);
    let mut candidates = vec![LoweredCandidate {
        label: "default",
        plan: default_plan,
        adapter: LoweringAdapter::ControllablePlanCompose,
        // Stated here rather than inferred from position, which is the whole point of `CandidateRole`.
        role: CandidateRole::Baseline,
    }];

    let permuted = permute_gate_groups(&candidates[0].plan);
    if permuted.root() != candidates[0].plan.root() {
        candidates.push(LoweredCandidate {
            label: "gate-group-permuted",
            plan: permuted,
            adapter: LoweringAdapter::ControllablePlanCompose,
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
mod tests {
    use super::*;
    use crate::plan::PlanNodeKind;

    fn load(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    fn prules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
        g.strata
            .iter()
            .flat_map(|s| &s.prules)
            .map(|&id| &g.prules[id.0 as usize])
            .collect()
    }

    /// A bare, rule-free grammar (no phonological or morphological rules): `should_run` is `false`, and it is the ungated case, one partition group with an empty key.
    fn ungated_no_composite_fixture() -> String {
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>EnumerateUngatedFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
        .to_string()
    }

    /// A grammar with one real ungated phonological rewrite rule: `should_run` is `true`, the structural route is absent (no circumfix/dropped-material rule), and it is still ungated (1 group).
    fn should_run_ordinary_phonology_fixture() -> String {
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>EnumerateShouldRunFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="c1" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="pr1">
        <Name>PR</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="pr1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
        .to_string()
    }

    /// A grammar with one gated MPR-restricted subrule and two entries realizing both truth values of that gate key, so `partition_entries` must yield exactly 2 groups; the structural route stays absent, isolating the Gate seam from the other two.
    fn gated_two_group_fixture() -> String {
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>EnumerateGatedTwoGroupFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mpr1">f1</MorphologicalPhonologicalRuleFeature>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule1">
        <Name>gate1</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule requiredMPRFeatures="mpr1">
            <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prule1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e0" partOfSpeech="posV">
            <Allomorphs><Allomorph id="allo0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e0</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e1" partOfSpeech="posV" ruleFeatures="mpr1">
            <Allomorphs><Allomorph id="allo1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
        .to_string()
    }

    /// Every `Leaf` node in `plan` whose fragment matches `pred`.
    fn leaves_matching(plan: &Plan, pred: impl Fn(&FragmentSpec) -> bool) -> Vec<NodeId> {
        plan.iter()
            .filter_map(|(id, kind)| match kind {
                PlanNodeKind::Leaf { fragment, .. } if pred(fragment) => Some(id),
                _ => None,
            })
            .collect()
    }

    fn gate_of(plan: &Plan) -> (NodeId, GatePartitionSpec) {
        plan.iter()
            .find_map(|(id, kind)| match kind {
                PlanNodeKind::Gate { partition, .. } => Some((id, partition.clone())),
                _ => None,
            })
            .expect("plan must contain exactly one Gate node")
    }

    /// Row 3: the enumerated Plan's Gate `partition.groups.len()` equals the real `partition_entries` count; both must be 1 for the ungated fixture.
    #[test]
    fn ungated_fixture_collapses_to_single_group_gate_matching_real_seam() {
        let g = load(&ungated_no_composite_fixture());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        // Cross-check against the REAL seam functions, not a re-derivation.
        assert!(
            !preexpand::should_run(&g, phon.as_ref()),
            "fixture must NOT exercise should_run"
        );
        let gated = find_gated_subrules(&g, &ro);
        assert!(gated.is_empty(), "fixture must declare zero gated subrules");
        let real_groups = partition_entries(&g, &gated, &ro);
        assert_eq!(real_groups.len(), 1, "ungated grammar collapses to 1 group");

        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let (_, partition) = gate_of(&plan);
        assert_eq!(
            partition.groups.len(),
            real_groups.len(),
            "enumerated Gate's group count must match the real seam's partition_entries count"
        );
        assert_eq!(partition.groups[0].key, Vec::<bool>::new());
        assert!(
            partition.gated_subrules.is_empty(),
            "no gated subrules declared"
        );

        // Row 1/2: neither composite marker should be present.
        assert!(leaves_matching(&plan, |f| matches!(
            f,
            FragmentSpec::CompositeEmissionMarker
        ))
        .is_empty());
        assert!(leaves_matching(&plan, |f| matches!(
            f,
            FragmentSpec::StructuralCompositeMarker
        ))
        .is_empty());
    }

    /// Row 1: the composite-emission subtree is present in the enumerated Plan iff `preexpand::should_run` says so.
    #[test]
    fn composite_subtree_present_iff_should_run() {
        let g = load(&should_run_ordinary_phonology_fixture());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        let real_should_run = preexpand::should_run(&g, phon.as_ref());
        assert!(real_should_run, "fixture must exercise should_run");
        let real_refuses = emit::probe_would_refuse(&g);
        assert!(
            !real_refuses,
            "fixture's rule has a real LHS, not epenthesis/metathesis"
        );

        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let composite_leaves = leaves_matching(&plan, |f| {
            matches!(f, FragmentSpec::CompositeEmissionMarker)
        });
        assert_eq!(
            !composite_leaves.is_empty(),
            real_should_run,
            "composite-emission subtree presence must match preexpand::should_run exactly"
        );

        // Row 2: the structural route must be absent (probe_would_refuse is false, no circumfix/dropped-material rule).
        let structural_leaves = leaves_matching(&plan, |f| {
            matches!(f, FragmentSpec::StructuralCompositeMarker)
        });
        assert_eq!(
            !structural_leaves.is_empty(),
            real_refuses,
            "structural-composite subtree presence must match probe_would_refuse for this fixture \
             (no non-refusing structural-rule construct is declared here)"
        );
    }

    /// Row 3 on a real gated multi-group grammar: the two groups realize different gate keys, so each must get a distinct `Replace` `NodeId` (the soundness invariant); the companion test below proves the other half, that identically-gated groups still dedup to the same `NodeId`.
    #[test]
    fn gated_two_group_fixture_matches_real_partition_and_gives_distinct_per_group_replace_nodes() {
        let g = load(&gated_two_group_fixture());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        let gated = find_gated_subrules(&g, &ro);
        assert_eq!(gated.len(), 1, "fixture declares exactly 1 gated subrule");
        let real_groups = partition_entries(&g, &gated, &ro);
        assert_eq!(
            real_groups.len(),
            2,
            "fixture's 2 entries realize both gate-key values"
        );

        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let (gate_id, partition) = gate_of(&plan);
        assert_eq!(
            partition.groups.len(),
            real_groups.len(),
            "enumerated Gate's group count must match the real seam's partition_entries count"
        );
        assert_eq!(
            partition.gated_subrules.len(),
            gated.len(),
            "one GatedSubruleRef per find_gated_subrules entry"
        );
        assert_eq!(partition.gated_subrules[0].rule_pos, gated[0].rule_pos);
        assert_eq!(partition.gated_subrules[0].sub_idx, gated[0].sub_idx);

        // Both possible keys (true and false) must be realized, since the fixture's 2 entries split exactly that way.
        let mut keys: Vec<Vec<bool>> = partition.groups.iter().map(|gr| gr.key.clone()).collect();
        keys.sort();
        assert_eq!(keys, vec![vec![false], vec![true]]);

        // The soundness invariant: these two groups gate differently ([true] vs [false]), so sharing one Replace node between them would be unsound.
        let PlanNodeKind::Gate { children, .. } = plan.get(gate_id).unwrap() else {
            unreachable!("gate_of only ever returns a Gate node")
        };
        assert_eq!(children.len(), 2, "one Compose child per partition group");
        let replace_ids: Vec<NodeId> = children
            .iter()
            .map(|&compose_id| {
                let PlanNodeKind::Compose { children, .. } = plan.get(compose_id).unwrap() else {
                    panic!("each Gate child must be a Compose node")
                };
                children[1]
            })
            .collect();
        assert_ne!(
            replace_ids[0], replace_ids[1],
            "two DIFFERENTLY-gated groups must get DISTINCT Replace NodeIds (task 1.4: a node's \
             compiled artifact must be a pure function of its own NodeId, so no two groups needing \
             different subrule_ok may share one Replace node)"
        );
        let replace_node_count = plan
            .iter()
            .filter(|(_, kind)| kind.kind_name() == "Replace")
            .count();
        assert_eq!(
            replace_node_count, 2,
            "two differently-gated groups must be stored as two DISTINCT Replace nodes, not one \
             shared/deduped node"
        );

        // The two groups' own LexiconFragment leaves must differ (different entries subsets), the real per-group filtering.
        let lexicon_leaves =
            leaves_matching(&plan, |f| matches!(f, FragmentSpec::LexiconFragment { .. }));
        assert_eq!(
            lexicon_leaves.len(),
            2,
            "the 2 groups' lexicon fragments carry different entry subsets, so must NOT dedup \
             against each other"
        );
    }

    /// The other half of the soundness invariant: groups that gate identically still dedup to the same `Replace` `NodeId` across two independent `enumerate_default` calls, since content addressing dedups by content, never by which `Plan` built a node.
    #[test]
    fn identically_gated_groups_across_independent_plans_share_the_same_replace_node_id() {
        let g = load(&gated_two_group_fixture());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        let plan_1 = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let plan_2 = enumerate_default(&g, &alphabet, &ro, phon.as_ref());

        fn replace_ids_by_key(plan: &Plan) -> std::collections::BTreeMap<Vec<bool>, NodeId> {
            let (gate_id, partition) = gate_of(plan);
            let PlanNodeKind::Gate { children, .. } = plan.get(gate_id).unwrap() else {
                unreachable!("gate_of only ever returns a Gate node")
            };
            partition
                .groups
                .iter()
                .zip(children.iter())
                .map(|(group, &compose_id)| {
                    let PlanNodeKind::Compose { children, .. } = plan.get(compose_id).unwrap()
                    else {
                        panic!("each Gate child must be a Compose node")
                    };
                    (group.key.clone(), children[1])
                })
                .collect()
        }

        let ids_1 = replace_ids_by_key(&plan_1);
        let ids_2 = replace_ids_by_key(&plan_2);
        assert_eq!(
            ids_1.keys().collect::<Vec<_>>(),
            ids_2.keys().collect::<Vec<_>>(),
            "both independently-built plans must realize the same set of gate keys"
        );
        for (key, id_1) in &ids_1 {
            let id_2 = ids_2[key];
            assert_eq!(
                *id_1, id_2,
                "the SAME gating key ({key:?}), built in two INDEPENDENT Plans, must yield the \
                 SAME Replace NodeId -- content addressing dedups by content, never by which Plan \
                 built a node"
            );
        }
    }

    /// A regression pin for `rule_id_of`: every rewrite-rule Leaf's `PRuleId` must equal the one `prules_in_order` was itself built from, not merely "some id or other".
    #[test]
    fn rewrite_rule_leaves_carry_the_correct_prule_id() {
        let g = load(&should_run_ordinary_phonology_fixture());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        assert_eq!(ro.len(), 1, "fixture declares exactly 1 phonological rule");
        let phon = PhonologyProbe::new(&g);

        let expected_id = g
            .prules
            .iter()
            .position(|candidate| std::ptr::eq(candidate, ro[0]))
            .map(|i| PRuleId(i as u32))
            .unwrap();

        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let rule_leaves = leaves_matching(&plan, |f| matches!(f, FragmentSpec::RewriteRule { .. }));
        assert_eq!(rule_leaves.len(), 1);
        let PlanNodeKind::Leaf {
            fragment,
            provenance,
        } = plan.get(rule_leaves[0]).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(*fragment, FragmentSpec::RewriteRule { rule: expected_id });
        assert_eq!(*provenance, Provenance::RewriteRule(expected_id));
    }

    /// `Plan::root` resolves to a `Union` when both a Gate node and a composite marker are present.
    #[test]
    fn root_is_union_when_composite_marker_and_gate_both_present() {
        let g = load(&should_run_ordinary_phonology_fixture());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());

        let root = plan.root().expect("root must be set");
        match plan.get(root).unwrap() {
            PlanNodeKind::Union { children } => {
                assert_eq!(children.len(), 2, "Gate node + composite-emission marker");
            }
            other => panic!("expected Union at root, got {}", other.kind_name()),
        }
    }

    /// Companion to the above: when neither composite marker is present, the root collapses directly to the Gate node, no pointless one-child `Union`.
    #[test]
    fn root_is_gate_directly_when_no_composite_markers_present() {
        let g = load(&ungated_no_composite_fixture());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());

        let root = plan.root().expect("root must be set");
        assert_eq!(
            plan.get(root).unwrap().kind_name(),
            "Gate",
            "no composite markers present -- root must collapse directly to the Gate node"
        );
    }

    /// Determinism: building the same fixture's Plan twice yields the same root NodeId and node count.
    #[test]
    fn enumerate_default_is_deterministic_across_independent_calls() {
        let g = load(&gated_two_group_fixture());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        let plan_a = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let plan_b = enumerate_default(&g, &alphabet, &ro, phon.as_ref());

        assert_eq!(plan_a.root(), plan_b.root());
        assert_eq!(plan_a.len(), plan_b.len());
    }

    // enumerate_candidates

    /// A grammar with ≥2 gate groups must yield 2 candidates, `"default"` and `"gate-group-permuted"`, with genuinely different root NodeIds.
    #[test]
    fn enumerate_candidates_yields_two_distinct_candidates_for_a_multi_group_gated_fixture() {
        let g = load(&gated_two_group_fixture());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        let candidates = enumerate_candidates(&g, &alphabet, &ro, phon.as_ref());
        assert_eq!(
            candidates.len(),
            2,
            "a ≥2-group gated grammar must yield exactly 2 candidates"
        );
        assert_eq!(candidates[0].label, "default");
        assert_eq!(candidates[1].label, "gate-group-permuted");
        assert_ne!(
            candidates[0].plan.root(),
            candidates[1].plan.root(),
            "the two candidates must be genuinely distinct topologies (different root NodeIds)"
        );
    }

    /// An ungated (single-group) grammar must yield exactly 1 candidate: permuting a single-element group list is a no-op, so `enumerate_candidates` must not append a relabeled copy.
    #[test]
    fn enumerate_candidates_yields_one_candidate_for_an_ungated_fixture() {
        let g = load(&ungated_no_composite_fixture());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        let candidates = enumerate_candidates(&g, &alphabet, &ro, phon.as_ref());
        assert_eq!(
            candidates.len(),
            1,
            "a single-group (ungated) grammar must yield exactly 1 candidate -- permuting 1 group \
             is a no-op, not a genuine second topology"
        );
        assert_eq!(candidates[0].label, "default");
    }

    /// Determinism across independent calls: building the same fixture's candidates twice yields the same root NodeIds in the same order.
    #[test]
    fn enumerate_candidates_is_deterministic_across_independent_calls() {
        let g = load(&gated_two_group_fixture());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        let candidates_a = enumerate_candidates(&g, &alphabet, &ro, phon.as_ref());
        let candidates_b = enumerate_candidates(&g, &alphabet, &ro, phon.as_ref());

        let roots_a: Vec<_> = candidates_a
            .iter()
            .map(|c| (c.label, c.plan.root()))
            .collect();
        let roots_b: Vec<_> = candidates_b
            .iter()
            .map(|c| (c.label, c.plan.root()))
            .collect();
        assert_eq!(roots_a, roots_b);
    }
}
