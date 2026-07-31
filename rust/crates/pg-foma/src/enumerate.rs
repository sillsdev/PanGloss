//! `enumerate_default` (Step 2 of `openspec/changes/reify-compilation-plans`, design.md D2): builds
//! today's compilation topology as a single reified [`Plan`] (Step 1, `crate::plan`), verified
//! structurally against the REAL seam functions rather than a re-derivation of their decisions.
//!
//! This module is purely **additive and behavior-preserving**: it does not implement a plan
//! builder/interpreter (no live [`foma::types::Fsm`] is built anywhere here), and does not modify
//! the bodies of the three seam functions it mirrors (their *visibility* was widened from private
//! to `pub(crate)` so this module and its tests can call them directly — see [`crate::emit::
//! probe_would_refuse`]/[`crate::emit::structural_candidate_rules`]'s own doc comments for that
//! rationale; [`crate::preexpand::should_run`] and [`crate::gate::partition_entries`]/
//! [`crate::gate::find_gated_subrules`] were already `pub(crate)`/`pub`). Building/executing a
//! [`Plan`] into real FSTs stays out of scope here (`crate::plan`'s own module doc; `crate::build`'s
//! `build_controllable` is the one interpreter, over the CONTROLLABLE Gate/Replace/Compose subtree
//! only). Task 1.3 (Step 3) DOES now flip a slice of a production compile path: `crate::emit::
//! plan_topology_decisions` calls [`enumerate_default`] and reads the built `Plan`'s composite-
//! emission/structural-composite marker presence to decide `emit_with_budget_profiled`'s own
//! topology, replacing that function's independent `preexpand::should_run`/`structural_candidate_
//! rules(...).is_empty()` calls — see that function's own doc. `gate::partition_entries` (D2's
//! third seam) stays unwired into `emit.rs`'s mainline: that seam belongs to `gate.rs`'s own,
//! separate compile entry point, which `emit.rs`'s lexc-emission path never calls at all.
//!
//! # The three seams, as D2's table names them, and how this module models each
//!
//! | D2 row | Real seam | Modeled as |
//! |---|---|---|
//! | 1 | [`crate::preexpand::should_run`] | An `Option<NodeId>` for a [`PlanNodeKind::Leaf`] tagged [`FragmentSpec::CompositeEmissionMarker`] — present iff `should_run` is `true`. |
//! | 2 | [`crate::emit::probe_would_refuse`] / [`crate::emit::structural_candidate_rules`] | An `Option<NodeId>` for a [`PlanNodeKind::Leaf`] tagged [`FragmentSpec::StructuralCompositeMarker`] — present iff `structural_candidate_rules(g)` is non-empty (the REAL gate `emit::emit_with_budget` uses, `!struct_rules.is_empty()`; a strict superset of "`probe_would_refuse` alone" — see that function's own doc). |
//! | 3 | [`crate::gate::partition_entries`] / [`crate::gate::find_gated_subrules`] | A [`PlanNodeKind::Gate`] node whose `partition.groups` has one [`GateGroupSpec`] per group `partition_entries` yields, one child subplan per group. An ungated grammar collapses to a single-group `Gate` with an empty key (design.md D2's own words, `GatePartitionSpec`'s doc) — the pre-refactor behavior preserved as a specific enumerable plan, not a special case. |
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
//! # Judgment calls (surfaced for Step 3, per this step's own review instructions)
//!
//! - **Composite/structural markers sit OUTSIDE the `Gate` node, as `Union` siblings, not nested
//!   inside every group.** `crate::gate`'s own module doc says gated compilation's affix chains are
//!   "shared, unfiltered" across every partition group — i.e. NOT gated at all. Composite/structural
//!   entries are exactly this kind of grammar-wide, non-partitioned material in today's code (in
//!   fact `crate::gate`'s prototype compile path doesn't even call `crate::preexpand`/structural
//!   composites at all today — those live only in `emit::emit_with_budget`'s mainline path). D2
//!   asks for ONE enumerator that treats all three seams as choices over the SAME plan, which today's
//!   code doesn't literally do (they're two separate compile entry points): this module's shape is
//!   the natural unification, not a literal mirror of one existing function's call graph.
//! - **Every gate group gets its OWN `Replace` node** (task 1.4, design.md D1 "Soundness
//!   invariant" — resolved; this bullet was a Step-2 judgment call flagging exactly the gap Step 3a
//!   found and task 1.4 closed). An EARLIER version of this module built one Replace node SHARED by
//!   every group, on the reasoning that [`ReplaceCascadeSpec`] is rule-level (`Vec<PRuleId>`) and
//!   the per-group subrule-inclusion distinction lives on [`GatePartitionSpec::gated_subrules`] +
//!   each group's own `key`, so duplicating it into `Replace` would be "redundant, not more
//!   faithful." That turned out to be UNSOUND: `crate::build`'s `build_controllable` needs a
//!   DIFFERENT `subrule_ok` per group, so a single shared `Replace` `NodeId` violates node purity
//!   (a `NodeId`-memoizing interpreter would build the cascade once and silently reuse the WRONG
//!   network for every other group). The fix: `ReplaceCascadeSpec` now carries `gated_subrules` +
//!   `group_key` directly (see that struct's own doc), so THIS group's own `Replace` node's content
//!   fully determines its `subrule_ok` — content-addressing (D1) then does the right thing on its
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
//!   the SAME grammar's enumerated `Plan` hash differently between runs — a direct violation of D1
//!   ("NodeIds must be reproducible across processes"). This is enumerate.rs's own post-processing
//!   of the seam's return value, not a change to `partition_entries` itself.
//! - **Recovering each `prules_in_order` entry's `PRuleId`** (needed for [`ReplaceCascadeSpec`] and
//!   [`FragmentSpec::RewriteRule`], which are addressed by the grammar-wide id, NOT by position in
//!   `prules_in_order` — that's `GatedSubruleRef::rule_pos`'s own, different, addressing scheme) is
//!   done by pointer identity against `g.prules` (see [`rule_id_of`]'s own doc) — every
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
use crate::replace::SegAlphabet;
use crate::{emit, preexpand};

/// Builds today's compilation topology for `g` as a single reified [`Plan`] (Step 2, design.md D2).
///
/// Takes exactly the inputs the real compile seams take: `alphabet`/`prules_in_order` (the shape
/// `crate::gate::compile_gated_grammar_with_budget` and `crate::replace`'s cascade builders take)
/// plus `phon` (what `crate::preexpand::should_run` and `crate::emit::emit_with_budget` take).
///
/// `alphabet` is accepted, not read: this step is data-only (module doc — no live `Fsm` is built
/// here), and none of today's three topology seams this step mirrors ( `should_run`,
/// `probe_would_refuse`/`structural_candidate_rules`, `partition_entries`) consult the segment
/// alphabet to decide topology — only Step 3's actual FST builder will need it. Kept as a parameter
/// anyway so this function's signature already matches what a Step-3 builder will need to thread
/// through, rather than growing a new parameter later.
pub fn enumerate_default(
    g: &Grammar,
    _alphabet: &SegAlphabet<'_>,
    prules_in_order: &[&PhonRuleDef],
    phon: Option<&PhonologyProbe<'_>>,
) -> Plan {
    let mut plan = Plan::new();

    // D2 row 1: preexpand::should_run -> composite-emission subtree presence.
    let composite_leaf = preexpand::should_run(g, phon).then(|| {
        plan.add_node(PlanNodeKind::Leaf {
            fragment: FragmentSpec::CompositeEmissionMarker,
            provenance: Provenance::CompositeEmission,
        })
    });

    // D2 row 2: emit::probe_would_refuse / structural_candidate_rules -> structural-composite
    // subtree presence. Mirrors `emit::emit_with_budget`'s own `!struct_rules.is_empty()` gate
    // exactly (module doc: a strict superset of "probe_would_refuse alone").
    let structural_leaf = (!emit::structural_candidate_rules(g).is_empty()).then(|| {
        plan.add_node(PlanNodeKind::Leaf {
            fragment: FragmentSpec::StructuralCompositeMarker,
            provenance: Provenance::StructuralComposite,
        })
    });

    // D2 row 3: gate::find_gated_subrules / gate::partition_entries -> the Gate node's partition.
    let gated = find_gated_subrules(g, prules_in_order);
    let mut groups = partition_entries(g, &gated, prules_in_order);
    // Reproducibility (module doc): `partition_entries` buckets through a `HashMap`, so re-sort by
    // key before this order becomes part of the Gate node's content address.
    groups.sort_by(|a, b| a.key.cmp(&b.key));

    // The gated-subrule universe (task 1.4/D1): the SAME for every group in this grammar, so
    // computed once and cloned into each group's own Replace node below (only `group_key` differs
    // per group).
    let gated_subrule_refs: Vec<GatedSubruleRef> = gated
        .iter()
        .map(|gs| GatedSubruleRef {
            rule_pos: gs.rule_pos,
            sub_idx: gs.sub_idx,
        })
        .collect();
    let cascade_rules: Vec<PRuleId> = prules_in_order.iter().map(|pr| rule_id_of(g, pr)).collect();

    // The rewrite-rule Leaf children (`replace.rs`'s per-rule transducers, promoted to `Replace`
    // per D1/D2): content-identical regardless of which group compiles them, so these dedup across
    // every group's own Replace node below even though the Replace PARENT node itself no longer
    // does (task 1.4's fix -- see module doc's "Judgment calls").
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

    // One Compose (group lexicon fragment .o. THIS GROUP'S OWN Replace node) per partition group --
    // mirrors `compile_gated_grammar_with_budget`'s own per-group `lexc_net .o. rules_net` step.
    // Task 1.4: each group's Replace node carries its OWN `group_key`, so distinct groups get
    // distinct Replace NodeIds (module doc) rather than sharing one node under different intended
    // meanings.
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

    // Root: the gate-partitioned, rule-composed lexicon, UNIONed with whichever composite-emission
    // markers this grammar's should_run/structural facts license (module doc's judgment call on
    // why Union, and why these sit OUTSIDE the Gate node rather than nested per-group).
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

/// One candidate topology [`enumerate_candidates`] emits, labeled for provenance/diagnostics
/// (task 2.1/2.2, `openspec/changes/reify-compilation-plans`). The label is a static string naming
/// WHICH axis produced this candidate (`"default"`, `"gate-group-permuted"`, ...), not a
/// user-facing description — see [`crate::selection::select_plan`]'s own doc for how this feeds a
/// caller's provenance report.
#[derive(Debug)]
pub struct CandidatePlan {
    pub label: &'static str,
    pub plan: Plan,
    /// WHICH compiler turns this candidate into a network. See [`EmissionStrategy`] — this is a
    /// different axis from `plan`, which describes assembly SHAPE within one compiler.
    pub strategy: EmissionStrategy,
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
/// * [`Self::TunedSurfaceProbed`] bakes phonology into the lexc via `emit`'s surface probe, then
///   patches the resulting expressive gaps with synthesized composite entries
///   (`preexpand::build_composites`, `emit::build_structural_composites`) — the material the `Plan`
///   can only NAME, via its `CompositeEmissionMarker`/`StructuralCompositeMarker` leaves.
/// * [`Self::TemplatedUnderlyingTokens`] emits plain char-def tokens and lets a real compiled
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
    /// Interpret `plan` with `build::build_controllable`. The only strategy that honours a plan's
    /// assembly shape, and the only one that can express a permutation — but it builds the
    /// controllable subtree ONLY, so on a plan carrying marker leaves its network omits whatever
    /// those subtrees contribute.
    #[default]
    PlanComposed,
    /// `emit::emit` (surface-probed) + rules, via `analyzer::FomaProposer::new`. Whole-grammar, every
    /// construct covered. Ignores `plan` entirely: this compiler derives its own topology, so it can
    /// express a grammar's DEFAULT compilation and nothing else.
    TunedSurfaceProbed,
    /// `emit::emit_underlying_templated` + a compiled rewrite cascade, via
    /// `templated_compile::compile_templated_morphotactics`. Whole-grammar and composite-free.
    /// Also ignores `plan`, for the same reason.
    TemplatedUnderlyingTokens,
}

impl EmissionStrategy {
    /// Whether this strategy realizes the whole grammar rather than the controllable subtree only.
    /// A caller comparing candidates across strategies needs this: a `PlanComposed` candidate on a
    /// marker-carrying grammar is not measuring the same object as either whole-grammar strategy.
    pub fn is_whole_grammar(self) -> bool {
        !matches!(self, Self::PlanComposed)
    }

    /// Stable identifier for reports and recipe ids.
    pub fn label(self) -> &'static str {
        match self {
            Self::PlanComposed => "plan-composed",
            Self::TunedSurfaceProbed => "tuned-surface-probed",
            Self::TemplatedUnderlyingTokens => "templated-underlying-tokens",
        }
    }
}

/// Task 2.1/2.2 (`openspec/changes/reify-compilation-plans`, design.md D3): the candidate
/// ENUMERATOR — every legal, **buildable** topology this crate can emit for `g` today, as
/// content-addressed [`Plan`]s a caller (typically [`crate::selection::select_plan`]) can filter by
/// capability and rank by cost. Always emits [`enumerate_default`]'s own plan first (candidate
/// `"default"`); the D3 selection story only becomes meaningful once there is a second, genuinely
/// distinct candidate to choose between.
///
/// # Which axes are emitted, and why
///
/// **Emitted: gate-group order** (candidate `"gate-group-permuted"`, via [`permute_gate_groups`]).
/// [`crate::oracle`]'s own module doc proves this is sound and non-vacuous: [`build::
/// build_controllable`] folds every `Gate` group's compiled network together with
/// [`crate::compose_budget::union_checked`] (commutative) and always finishes with
/// [`crate::compose_budget::minimize_checked`], so a `Gate` node's group ORDER cannot affect the
/// final relation — only membership does. Reordering the groups changes the `Gate` node's content
/// address (D1: `NodeId = hash(kind, children, config)`, and both `partition.groups` and `children`
/// are part of that content) without changing what the built network recognizes: a real, distinct,
/// SAME-relation candidate topology, not a relabeling of the identical `Plan`. **Only added when it
/// is actually a different plan**: a grammar with 0 or 1 partition groups reverses to the identical
/// `Vec`, so `permute_gate_groups` would return a `Plan` with the SAME root `NodeId` — appending it
/// would just be the `"default"` candidate wearing a second label, which is not a genuine
/// alternative for [`crate::selection::select_plan`] to weigh. This function checks the roots differ
/// before appending, so the returned `Vec` has length 1 for an ungated/single-group grammar and
/// length 2 once there are ≥2 groups to reorder.
///
/// # Which axes are deliberately NOT emitted yet, and why
///
/// - **Reordering the root `Union`'s composite-emission/structural-composite marker children.**
///   [`enumerate_default`]'s own module doc already notes `Union`'s commutativity makes child order
///   semantically inert; the reason this is still not a candidate axis is that neither marker leaf
///   is interpreted by [`build::build_controllable`] at all (that module's own scope note: markers
///   are a separate, black-box lexc-`String` artifact, "out of scope for this step"). Permuting
///   `Union` children would therefore change a content address without changing anything
///   [`build::build_controllable`] can measure or build differently — no genuine topology choice,
///   just churn.
/// - **An alternative partition function for the `Gate` node** (grouping entries differently than
///   [`crate::gate::partition_entries`] does). No second partition-computing seam exists anywhere in
///   this crate; inventing one here would mean re-deriving `gate.rs`'s own gating semantics a second,
///   independent way — squarely the kind of change this task's own scope excludes ("do NOT touch
///   replace.rs or lower.rs"; by the same discipline, this step does not reach into `gate.rs` either
///   to manufacture a second partition strategy it was never asked to build).
/// - **Reordering a `Replace` cascade's rule sequence.** Unlike gate-group order, rewrite-rule order
///   is NOT proven irrelevant — `replace.rs`'s cascade is explicitly order-sensitive (each rule's
///   output feeds the next), so two different rule orders are not, in general, the SAME relation at
///   all. Emitting a reordered-cascade candidate here would risk exactly what D3 rules out by
///   construction ("selection can never pick a fast-but-wrong plan"): a candidate that LOOKS like an
///   alternative topology for the same logical request but actually computes a different relation.
///   Absent a proof of order-irrelevance (which no seam in this crate currently supplies), this axis
///   is left unexplored rather than emitted unsoundly.
pub fn enumerate_candidates(
    g: &Grammar,
    alphabet: &SegAlphabet<'_>,
    prules_in_order: &[&PhonRuleDef],
    phon: Option<&PhonologyProbe<'_>>,
) -> Vec<CandidatePlan> {
    let default_plan = enumerate_default(g, alphabet, prules_in_order, phon);
    let mut candidates = vec![CandidatePlan {
        label: "default",
        plan: default_plan,
        strategy: EmissionStrategy::PlanComposed,
    }];

    let permuted = permute_gate_groups(&candidates[0].plan);
    if permuted.root() != candidates[0].plan.root() {
        candidates.push(CandidatePlan {
            label: "gate-group-permuted",
            plan: permuted,
            strategy: EmissionStrategy::PlanComposed,
        });
    }

    candidates
}

/// Recovers `pr`'s [`PRuleId`] (its index into [`Grammar::prules`]) from a `prules_in_order` entry,
/// by pointer identity — see this module's own doc ("Judgment calls") for why this is safe, not a
/// hack: every construction site for a `prules_in_order` slice in this crate borrows its elements
/// directly from `g.prules` (`&g.prules[id.0 as usize]`), never copies them, so the reference's
/// address uniquely identifies its source index.
///
/// # Panics
/// If `pr` is not found in `g.prules` by pointer identity — this would mean a caller passed a
/// `prules_in_order` slice NOT borrowed from this same `g`, which is a caller bug this function
/// cannot silently paper over (silently returning a wrong `PRuleId` would corrupt every downstream
/// consumer of that id, e.g. capability-evidence-provenance tagging, ADR 0001).
///
/// Widened from private to `pub(crate)` for Step 3a (`crate::build`): that module's own
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

    /// A bare, rule-free grammar: no `PhonologicalRuleDefinitions` element at all (so
    /// `PhonologyProbe::new` returns `None`), no `MorphologicalRuleDefinitions` (so no `Role::Infix`
    /// rule exists either) -- `should_run` must be `false`. No gated subrule can exist (there are no
    /// phonological subrules at all), so this is also the ungated case: exactly one partition group
    /// with an empty key (D2: "ungated grammar collapses to a single-group Gate").
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

    /// A grammar with one real (ungated, ordinary) phonological rewrite rule -- `PhonologyProbe::
    /// new` returns `Some`, so `should_run` must be `true` (`preexpand::should_run`'s own doc: `phon.
    /// is_some() || any_infix_rule(g)`). The rule's LHS is a real segment (no empty `<PhoneticInput
    /// />`, no `Metathesis`), so `probe_would_refuse` must be `false` and `structural_candidate_
    /// rules` empty (no circumfix/dropped-material rule declared either) -- the structural route
    /// must be ABSENT. No MPR/POS restriction anywhere, so still ungated (1 group).
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

    /// A grammar with ONE gated MPR-restricted subrule (`requiredMPRFeatures="mpr1"`) and two
    /// entries realizing both truth values of that gate key -- `partition_entries` must yield
    /// exactly 2 groups (mirrors `gate.rs`'s own `sixteen_group_fixture` pattern, scaled down to the
    /// smallest case that still exercises >1 group). Also gives `should_run` a real phonological
    /// rule to be `true` on, and no epenthesis/metathesis/circumfix construct, so the structural
    /// route stays absent -- isolating the Gate seam from the other two.
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

    /// D2 row 3: the enumerated Plan's Gate `partition.groups.len()` equals the REAL
    /// `partition_entries(g, &gated, &prules_in_order).len()` -- for the ungated fixture, both must
    /// be 1 (D2's degenerate-collapse case).
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

        // D2 row 1/2: neither composite marker should be present.
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

    /// D2 row 1: the composite-emission subtree is present in the enumerated Plan IFF the REAL
    /// `preexpand::should_run` says so, for a grammar that actually exercises it.
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

        // D2 row 2: the structural route must be ABSENT here (probe_would_refuse is false and this
        // fixture declares no circumfix/dropped-material rule).
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

    /// D2 row 3 on a REAL gated multi-group grammar: the enumerated Plan's Gate
    /// `partition.groups.len()` equals `partition_entries(...).len()` (here, 2), one
    /// `GatedSubruleRef` per `find_gated_subrules` entry, and (task 1.4, D1's soundness invariant)
    /// each group's `Compose` child now references its OWN, DISTINCT `Replace` `NodeId` — the two
    /// groups here realize different gate keys (`[true]`/`[false]`), so they MUST get different
    /// `Replace` nodes (a single shared node, this module's pre-task-1.4 behavior, would be unsound:
    /// see `ReplaceCascadeSpec`'s own doc). The companion test below,
    /// `identically_gated_groups_across_independent_plans_share_the_same_replace_node_id`, proves
    /// the other half of the invariant: groups that DO gate identically still dedup to the SAME
    /// `Replace` `NodeId`.
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

        // Both possible keys (true and false) must be realized, since the fixture's 2 entries
        // split exactly that way.
        let mut keys: Vec<Vec<bool>> = partition.groups.iter().map(|gr| gr.key.clone()).collect();
        keys.sort();
        assert_eq!(keys, vec![vec![false], vec![true]]);

        // Task 1.4 (D1's soundness invariant): every group's Compose child must reference its OWN
        // Replace NodeId -- these two groups gate DIFFERENTLY ([true] vs. [false]), so sharing one
        // Replace node between them would be the exact unsoundness task 1.4 closes.
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

        // The two groups' own LexiconFragment leaves, by contrast, must differ (different entries
        // subsets) -- the real per-group filtering `compile_gated_grammar_with_budget` performs.
        let lexicon_leaves =
            leaves_matching(&plan, |f| matches!(f, FragmentSpec::LexiconFragment { .. }));
        assert_eq!(
            lexicon_leaves.len(),
            2,
            "the 2 groups' lexicon fragments carry different entry subsets, so must NOT dedup \
             against each other"
        );
    }

    /// The other half of task 1.4's soundness invariant (companion to the test above, which proves
    /// DIFFERENTLY-gated groups get DISTINCT `Replace` NodeIds): groups that gate IDENTICALLY still
    /// dedup to the SAME `Replace` `NodeId`, even across two INDEPENDENT `enumerate_default` calls
    /// (two separate `Plan` arenas) -- content addressing dedups by CONTENT (`rules` +
    /// `gated_subrules` + `group_key`), never by which `Plan`/`Gate` node happened to build a node.
    /// A single `partition_entries` call can never itself realize two groups with the same key
    /// (`partition_entries` is a true partition -- one group per DISTINCT key by construction), so
    /// this property can only be exercised across two independently-enumerated `Plan`s, exactly
    /// what this test does: build the same fixture twice and match up each plan's groups by key.
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

    /// A regression pin for [`rule_id_of`]: every rewrite-rule Leaf's `PRuleId` inside the
    /// enumerated Plan's Replace cascade must equal the `PRuleId` `prules_in_order` was itself built
    /// from (`g.strata`'s own `phonologicalRules` id-list order), not merely "some id or other" --
    /// this is what makes `FragmentSpec::RewriteRule`/`Provenance::RewriteRule` a faithful
    /// capability-evidence-provenance tag (ADR 0001) rather than a coincidentally-plausible one.
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

    /// A small diagnostic-only sanity check that the module-doc example XML actually compiles into
    /// the shape it claims -- exercises [`Plan::root`] resolving to a `Union` when both a Gate node
    /// AND a composite marker are present.
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

    /// Companion to the above: when NEITHER composite marker is present (the ungated,
    /// should_run=false fixture), the root collapses directly to the Gate node -- no pointless
    /// `Union` of one child.
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

    /// Determinism (D1): building the same fixture's Plan twice yields the same root NodeId and the
    /// same node count -- content addresses must be reproducible across independent calls, not just
    /// stable within one.
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

    // ---------------------------------------------------------------------------------------------
    // Task 2.1/2.2: enumerate_candidates
    // ---------------------------------------------------------------------------------------------

    /// A grammar with ≥2 gate groups must yield 2 candidates: `"default"` and
    /// `"gate-group-permuted"`, with genuinely different root NodeIds (the whole point of the
    /// second axis -- see [`enumerate_candidates`]'s own doc).
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

    /// An ungated (single-group) grammar must yield exactly 1 candidate: permuting a single-element
    /// group list is a no-op (same root NodeId as `"default"`), so `enumerate_candidates` must not
    /// append a second, merely-relabeled copy of the same plan.
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

    /// Determinism across independent calls (D1, mirrored for the candidate list): building the
    /// same fixture's candidates twice yields the same root NodeIds in the same order.
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
