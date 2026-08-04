//! The
//! differential-correctness oracle: the refactor this crate's plan reification is built on
//! pays for its own correctness check, made concrete. This is flagged as this crate's genuinely
//! novel research contribution ("building >=2 independently-derived over-approximations of one
//! grammar and using their disagreement as a designed-in correctness oracle"); this module ships
//! ONLY the cheap, always-on tier.
//!
//! # What this module does NOT do (explicitly out of scope)
//! - **No confirm-engine integration.** The cheap tier as shipped elsewhere in this crate's own
//!   product (`crate::composite::FomaAnalyzer`) would run `confirm(propose_P1(w)) ==
//!   confirm(propose_P2(w))` through the trusted HC confirm engine. This module instead compares
//!   the two plans' raw `apply_up` result SETS directly -- [`build::build_controllable`]'s own
//!   `equivalence_tests` module's predicate, generalized to two arbitrary [`Plan`]s + a word list +
//!   shortest-witness reporting. Wiring a real confirm pass in is
//!   future work, not this module's.
//! - **No exact-equivalence stretch tier.** An "expensive, opt-in tier" (decidable FST
//!   equivalence for finite-valued transducers) is explicitly marked a stretch goal, not a v1
//!   requirement -- not attempted here.
//!
//! # The soundness caveat this module must respect
//! The soundness invariant this module depends on is:
//! **a node's compiled artifact must be a pure function of its `NodeId`** for any `NodeId`-keyed
//! memoization to be sound. That was **not true** in general for `Gate`/`Replace` pairing in an
//! earlier construction -- [`build::build_controllable`] sidestepped it by being Gate-aware (re-deriving each group's
//! `subrule_ok` from the `Gate` node's own partition, never caching a compiled `Fsm` against a
//! shared `Replace` `NodeId`) rather than by a generic `NodeId`-memoizing interpreter. This was
//! closed at its root: `crate::enumerate::enumerate_default` now builds one `Replace` node
//! PER GROUP, carrying that group's own `gated_subrules`/`group_key` directly in its
//! `crate::plan::ReplaceCascadeSpec` (that struct's own doc), so distinct groups get distinct
//! `Replace` `NodeId`s and `build_controllable` reads `subrule_ok` from the Replace node's own
//! content, not the `Gate` node's partition (`build`'s own module doc). This module calls
//! [`build::build_controllable`] itself for BOTH plans it diffs, so it inherits that same
//! content-pure behavior -- it never memoizes a compiled artifact by `NodeId` across the two
//! builds (still true, and now provably safe if it did). [`permute_gate_groups`] (below) is careful
//! to keep this sound too: it reorders a `Gate` node's `groups` and `children` IN LOCKSTEP (each
//! group's key travels with its own child, and its own `Replace` node,
//! implicitly, as part of that child subtree), never separately -- so every group's `subrule_ok` is
//! still resolved from the correct key at `build_controllable` time, on both plans.
//!
//! # The oracle's comparison methodology
//! [`differential_oracle`] builds BOTH input plans via [`build::build_controllable`] (never
//! recomputing a partition/cascade itself -- same discipline as `build.rs`'s own module doc), then
//! for every word in the caller-supplied word list computes `apply_up`'s full result-string set on
//! each built net (an empty set, not a panic, for a plan whose build produced no net at all --
//! `GatedCompileResult::net`'s `None` case, e.g. every partition group empty). Words whose two
//! result sets are unequal are disagreements; among those, the SHORTEST disagreeing word (by `char`
//! count, ties broken lexicographically) is reported, together with the symmetric difference of
//! the two result sets (a pattern borrowed from CFG-equivalence tooling). The selection logic itself
//! ([`resolve_verdict`]) is a small pure function over `(word, results_a, results_b)` triples,
//! deliberately factored out of the foma-build-heavy entry point so it can be unit-tested directly
//! against synthetic result sets (this module's own `shortest witness` tests) without needing a
//! grammar whose recognized surface forms happen to span several lengths.
//!
//! # The second topology: [`permute_gate_groups`]
//! A differential oracle needs two genuinely distinct [`Plan`]s that encode the SAME relation to be
//! a non-vacuous same-relation exercise. [`permute_gate_groups`] builds one: a copy of the input
//! plan with every `Gate` node's `partition.groups` (and paired `children`) reordered (reversed).
//! Because [`build::build_controllable`] folds every group's compiled network together with
//! [`crate::compose_budget::union_checked`] (commutative) and always finishes with
//! [`crate::compose_budget::minimize_checked`], a `Gate` node's group ORDER cannot affect the final
//! relation -- only membership does. Reordering therefore changes the `Gate` node's content address
//! ([`crate::plan::NodeId`] is `hash(kind, children, config)`, and both `partition.groups` and
//! `children` are part of that content) without changing what the built network recognizes: a real,
//! non-trivial differential-oracle pair, not two labels for the identical `Plan`.
//!
//! # Judgment call: `Result`, not a bare `OracleResult`
//! [`differential_oracle`] returns `Result<OracleResult, ComposeError>`, not a bare `OracleResult` --
//! [`build::build_controllable`] is itself fallible (a [`crate::compose_budget::ComposeBudget`] cap
//! can trip on either plan), and this module has no sound way to turn that failure into an
//! `OracleResult` variant (neither "the two plans agree" nor "the two plans disagree" is true when
//! one plan didn't build at all). Propagating `ComposeError` mirrors `build_controllable`'s own
//! `Result` convention rather than inventing a third `OracleResult` case for "didn't run".
//!
//! # Second-topology generators, per node kind (the remaining depth this module now closes)
//! [`crate::plan::PlanNodeKind`] is a closed five-variant enum (that module's own doc). For a
//! relation-preserving second topology to be worth shipping here it must clear TWO bars, not one:
//! (a) the restructuring must be sound in the abstract sense [`permute_gate_groups`] already
//! establishes for `Gate` (the built relation provably does not change), AND (b) [`build::
//! build_controllable`] must actually be able to BUILD the restructured plan -- that interpreter is
//! not a generic `Plan` walker, it is hard-shaped to exactly the seven adjacency tuples
//! [`crate::plan_interaction_coverage::legal_adjacency_tuples`] documents as the closed set
//! [`crate::enumerate::enumerate_default`] can ever produce (that module's own doc), and it panics --
//! loudly, by design (`build.rs`'s own module doc) -- on anything else. A restructuring that is sound
//! in the abstract `Plan` model but produces a shape `build_controllable` cannot interpret would make
//! the oracle unable to even ATTEMPT the comparison -- a different, less useful failure than "sound
//! and buildable." Per node kind:
//!
//! - **`Leaf`** -- no children, nothing to restructure. Not a candidate kind at all.
//! - **`Gate`** -- sound and buildable, TWO independent ways: [`permute_gate_groups`] (order,
//!   already shipped, see above) and [`refine_gate_partition`] (cardinality -- splitting one
//!   group's entries into several disjoint sub-groups sharing its own unchanged `Replace` node;
//!   composition distributes over union, so this cannot change what the `Gate` node accepts either
//!   -- see that function's own doc for the full argument). The two are independent axes of the SAME
//!   node kind, not the same restructuring twice: order-permuting never changes
//!   `partition.groups.len()`, refining never changes group ORDER.
//! - **`Union`** -- **sound, and now shipped**: [`permute_union_children`]. `Union`'s own doc
//!   (`plan.rs`) is "merges independently-compiled branches" -- a set union over whatever its
//!   children denote, and set union is commutative, so reordering `children` cannot change what the
//!   node denotes -- exactly [`permute_gate_groups`]'s argument, one level up the tree. It is ALSO
//!   buildable: the only `Union` shape `enumerate_default` ever produces is the root node wrapping
//!   one `Gate` plus optional composite-emission/structural-composite marker leaves (`enumerate.rs`'s
//!   own "Shape" diagram; [`crate::plan_interaction_coverage::legal_adjacency_tuples`] confirms this
//!   is the closed set), and `build_controllable`'s own root-`Union` walk scans children BY KIND, not
//!   position, to find its one `Gate` child -- reordering them changes nothing `build_controllable`
//!   computes. `enumerate.rs`'s own module doc already makes this exact observation to explain why
//!   `enumerate_candidates` does NOT emit a `Union`-reordered candidate ("no genuine topology choice,
//!   just churn") -- but that is a judgment call about what is worth offering the SELECTION step, a
//!   different question from what the DIFFERENTIAL ORACLE needs. The oracle's job is to exercise a
//!   real, structurally-different, same-relation second topology; "the built net turns out to be
//!   bit-identical, not merely apply-equivalent" is a valid (if maximally boring) proof of that, not a
//!   reason to skip it -- see [`permute_union_children`]'s own agreement test.
//! - **`Compose`** -- **no sound generator, and none is shipped.** Two independent reasons, either
//!   one sufficient on its own:
//!   1. *Semantically*: composition is associative but NOT commutative in general -- swapping
//!      `Compose`'s children changes which relation is computed (`A .o. B` and `B .o. A` are, in
//!      general, different relations, sometimes not even tape-compatible). A reordering generator
//!      here would risk exactly what this module's own doc warns against for the oracle as a whole:
//!      "a generator that changes the relation would make the oracle report false disagreements,
//!      which is worse than having no generator" -- confirmed empirically, not just argued, by this
//!      module's own `swapping_compose_children_is_mechanically_rejected_by_build_controllable` test.
//!   2. *Structurally, re-association doesn't even arise as a question*: re-association (regrouping
//!      a CHAIN of 3+ composes) is the one associativity-flavored restructuring that COULD in
//!      principle be sound independent of commutativity, but every `Compose` node `enumerate_default`
//!      ever builds has exactly 2 children in two FIXED, non-interchangeable roles -- `children[0]`
//!      is always the group's `LexiconFragment` leaf, `children[1]` is always that group's own
//!      `Replace` node (`enumerate.rs`'s "Shape" diagram; `build.rs`'s own `gate_group_children`
//!      reads `children[0]`/`children[1]` positionally and panics on anything else). With no chain of
//!      3+ to regroup, "reassociate the compose" is vacuously not a question this crate's actual
//!      plans ever pose. Both reasons are cited so a future `Compose` shape with genuine n-ary chains
//!      (a real enumerator gap, not this module's) knows re-litigating commutativity is still the
//!      live question, not re-association.
//! - **`Replace`** -- **no sound generator, and none is shipped.** Cascade order is rule-APPLICATION
//!   order: `replace.rs`'s cascade composes each rule's output into the next rule's input (a
//!   sequential rewrite, not a set union), so two different cascade orders are, in general, two
//!   different relations -- `enumerate.rs`'s own module doc already states this exactly ("rewrite-
//!   rule order is NOT proven irrelevant... each rule's output feeds the next") when explaining why
//!   `enumerate_candidates` does not emit a reordered-cascade candidate either; this module adopts the
//!   same citation rather than re-deriving it. Mechanically, a reordered cascade is doubly
//!   inadmissible even before that semantic argument matters: `build.rs`'s own
//!   `validate_replace_cascade` asserts `cascade.rules[i]` equals `prules_in_order[i]` POSITIONALLY
//!   for every `i` -- so `build_controllable` PANICS on any `Replace` node whose `cascade.rules` order
//!   does not match `prules_in_order` exactly, well before any question of "does this change the
//!   relation" could even be asked of the built net. Confirmed empirically by this module's own
//!   `reversing_replace_cascade_is_mechanically_rejected_by_build_controllable` test, not just argued.
//!
//! # Seeded random subtree mutation ([`mutate_plan_seeded`])
//! [`mutate_plan_seeded`] draws ONE of the two sound restructurings above (never anything else, by
//! construction: [`eligible_mutation_targets`] only ever names `Gate`/`Union` nodes) at a randomly
//! chosen node, using [`SplitMix64`] -- a tiny, hand-rolled PRNG seeded only by its caller-supplied
//! seed (see that type's own doc for why: no dependency, no wall-clock, no thread-local state, so the
//! SAME seed against the SAME plan always draws the SAME target and the SAME permutation). The draw
//! is packaged as a [`MutationRecipe`] (target kind/id + permutation + the seed itself), so a failure
//! report carries everything needed to replay it exactly.
//!
//! # Failure minimisation ([`minimize_disagreement`])
//! A [`MutationStep`] is a named, replayable, chainable plan transform (typically
//! [`MutationStep::from_seed`], which re-draws [`mutate_plan_seeded`] against whatever plan IT is
//! applied to at its position in the chain -- so dropping an earlier step changes what a later step's
//! own draw sees, exactly the "revert a step, replay the rest" behavior delta-debugging needs).
//! [`minimize_disagreement`] takes a `Vec<MutationStep>` already known to disagree with the base plan
//! (asserted up front) and greedily drops any single step that can be removed while the disagreement
//! still holds, over full passes, until no further step is removable (a standard 1-minimal
//! delta-debugging fixed point) -- the surviving steps plus [`resolve_verdict`]'s own
//! shortest-disagreeing-word are reported as a [`MinimizedRecipe`], whose `Display` impl is the
//! paste-into-a-test format (see that type's own doc for the exact shape).

use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::rc::Rc;

use foma::apply::apply_init;
use foma::options::FomaOptions;
use foma::types::Fsm;

use pg_grammar::model::{Grammar, LexEntryId, PhonRuleDef};

use crate::build::build_controllable;
use crate::compose_budget::{ComposeBudget, ComposeError};
use crate::plan::{
    ComposeStrategy, FragmentSpec, GateGroupSpec, GatePartitionSpec, NodeId, Plan, PlanNodeKind,
    Provenance,
};
use crate::replace::SegAlphabet;

/// The outcome of one [`differential_oracle`] run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleResult {
    /// Every word compared produced identical `apply_up` result sets on both plans.
    Agree,
    /// At least one word disagreed. `word` is the SHORTEST such word, ties broken
    /// lexicographically; `only_in_a`/`only_in_b` are the symmetric difference of the two plans'
    /// `apply_up` result sets for THAT word (a pattern borrowed from CFG-equivalence tooling); `plan_a_label`/
    /// `plan_b_label` echo [`differential_oracle`]'s own `labels` argument, so a caller printing this
    /// variant does not need to thread the labels through separately.
    Disagree {
        word: String,
        only_in_a: BTreeSet<String>,
        only_in_b: BTreeSet<String>,
        plan_a_label: String,
        plan_b_label: String,
    },
}

/// Every raw string `apply_up` yields for `word` against `net` (module doc: the same predicate
/// [`crate::build`]'s own `equivalence_tests` uses), encoded through `alphabet.encode_query` first.
/// `None` (no net at all -- [`crate::gate::GatedCompileResult::net`]'s `None` case) or a query that
/// fails to encode against this grammar's segment table both yield the EMPTY set, never a panic --
/// "this plan recognizes nothing" and "this word doesn't parse against this plan's net" are both
/// legitimate, comparable outcomes for a differential run, not error conditions.
fn apply_up_results(net: Option<&Fsm>, alphabet: &SegAlphabet, word: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(net) = net else {
        return out;
    };
    let Some(query) = alphabet.encode_query(word) else {
        return out;
    };
    let mut h = apply_init(net);
    for s in h.up(&query) {
        out.insert(s);
    }
    out
}

/// The pure selection core (module doc): given each word's two `apply_up` result sets, returns
/// `Agree` if all agree, else `Disagree` naming the SHORTEST disagreeing word (`char` count, ties
/// broken lexicographically by the word itself -- a total, deterministic order, so ties are broken
/// deterministically). Factored out of
/// [`differential_oracle`] so it can be unit-tested directly against synthetic `(word, results_a,
/// results_b)` triples, independent of building any real `Fsm`.
fn resolve_verdict(
    per_word: Vec<(String, HashSet<String>, HashSet<String>)>,
    plan_a_label: &str,
    plan_b_label: &str,
) -> OracleResult {
    let mut disagreements: Vec<(String, BTreeSet<String>, BTreeSet<String>)> = per_word
        .into_iter()
        .filter(|(_, a, b)| a != b)
        .map(|(word, a, b)| {
            let only_in_a: BTreeSet<String> = a.difference(&b).cloned().collect();
            let only_in_b: BTreeSet<String> = b.difference(&a).cloned().collect();
            (word, only_in_a, only_in_b)
        })
        .collect();

    // Shortest-first, lexicographic tie-break: sort by (char count, word) and take the first --
    // deterministic regardless of `per_word`'s own input order.
    disagreements.sort_by(|(word_x, ..), (word_y, ..)| {
        (word_x.chars().count(), word_x).cmp(&(word_y.chars().count(), word_y))
    });

    match disagreements.into_iter().next() {
        None => OracleResult::Agree,
        Some((word, only_in_a, only_in_b)) => OracleResult::Disagree {
            word,
            only_in_a,
            only_in_b,
            plan_a_label: plan_a_label.to_string(),
            plan_b_label: plan_b_label.to_string(),
        },
    }
}

/// The cheap, always-on differential-correctness tier: builds BOTH `plan_a` and `plan_b` via
/// [`build_controllable`] (never recomputing a partition/cascade itself), then compares their
/// `apply_up` result sets over every word in `words`. Returns `Ok(OracleResult::Agree)` iff every
/// word's two result sets are identical; otherwise `Ok(OracleResult::Disagree { .. })` naming the
/// shortest disagreeing word (module doc, [`resolve_verdict`]). `labels` are `(plan_a`'s label,
/// `plan_b`'s label`)`, echoed back on a `Disagree` for a caller's own reporting -- purely
/// diagnostic, no bearing on the comparison itself.
///
/// `opts`/`g`/`alphabet`/`prules_in_order`/`budget` are threaded straight through to both
/// `build_controllable` calls -- the SAME grammar-derived inputs both plans must have been
/// enumerated against (mismatched inputs would make "the two plans encode the same relation" an
/// incoherent claim to begin with; this function does not attempt to detect that caller error, the
/// same trust convention `build_controllable` itself documents for its own `prules_in_order`
/// parameter).
///
/// # Errors
/// Propagates a [`ComposeError`] from either `build_controllable` call unchanged (module doc's
/// judgment-call note: no `OracleResult` variant means "one plan didn't build").
#[allow(clippy::too_many_arguments)] // mirrors build_controllable's own args, taken for BOTH plans
                                     // plus labels/words -- same convention as this crate's other
                                     // many-parameter entry points (replace.rs/preexpand.rs/emit.rs).
pub fn differential_oracle(
    plan_a: &Plan,
    plan_b: &Plan,
    labels: (&str, &str),
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    budget: &ComposeBudget,
    words: &[&str],
) -> Result<OracleResult, ComposeError> {
    let (label_a, label_b) = labels;

    let built_a = build_controllable(plan_a, opts, g, alphabet, prules_in_order, budget)?;
    let built_b = build_controllable(plan_b, opts, g, alphabet, prules_in_order, budget)?;

    let per_word: Vec<(String, HashSet<String>, HashSet<String>)> = words
        .iter()
        .map(|&word| {
            let a = apply_up_results(built_a.net.as_ref(), alphabet, word);
            let b = apply_up_results(built_b.net.as_ref(), alphabet, word);
            (word.to_string(), a, b)
        })
        .collect();

    Ok(resolve_verdict(per_word, label_a, label_b))
}

/// The shape of the callback [`copy_plan_transforming`]/[`copy_node`] thread through their recursive
/// copy for a `Gate` node: given that node's own `NodeId` plus its own `(partition, children)`,
/// returns the `(partition, children)` to install in the copy. Takes the node's `NodeId` (not just
/// its content) so a caller can single out ONE specific node by identity and leave every other `Gate`
/// node in the plan untouched -- [`apply_permutation_at`] needs exactly that; the
/// whole-plan generator ([`permute_gate_groups`]) simply ignores the id and transforms every `Gate`
/// node uniformly. Named purely to satisfy `clippy::type_complexity` -- not a new abstraction beyond
/// what [`copy_plan_transforming`]'s own doc already describes.
type GateTransform<'a> =
    dyn FnMut(NodeId, &GatePartitionSpec, &[NodeId]) -> (GatePartitionSpec, Vec<NodeId>) + 'a;

/// The `Union`-node counterpart of [`GateTransform`], used by [`permute_union_children`]:
/// given a `Union` node's own `NodeId` and its own `children`, returns the `children` to install in
/// the copy.
type UnionTransform<'a> = dyn FnMut(NodeId, &[NodeId]) -> Vec<NodeId> + 'a;

/// Recursively rebuilds `plan` into a fresh [`Plan`] arena, applying `transform_gate` to every `Gate`
/// node's own `(id, partition, children)` and `transform_union` to every `Union` node's own `(id,
/// children)` as each is encountered -- BEFORE recursing into whichever children the callback decides
/// to keep, so a transform that drops a child entirely never even copies that subtree into the new
/// plan. A single unified walk (rather than two near-duplicate copies of the same five-arm match) so
/// [`permute_gate_groups`], [`permute_union_children`], and [`apply_permutation_at`]
/// all exercise the identical recursion, and this module's own test-only "drop a gate group"
/// deliberately-wrong-plan constructor keeps sharing it too -- one shared walk rather than
/// several near-duplicate copies.
///
/// Every non-`Gate`/non-`Union` node is copied verbatim (same fragment/provenance/strategy/cascade,
/// children recursively copied) -- [`Plan::add_node`]'s own content-addressed dedup means an
/// unaffected subtree copied this way still interns to a stable `NodeId`, just possibly a different
/// one than in the source plan if any of ITS descendants changed (which, for every transform this
/// module ships, only ever happens below the ONE targeted `Gate`/`Union` node, if any).
fn copy_plan_transforming(
    plan: &Plan,
    transform_gate: &mut GateTransform<'_>,
    transform_union: &mut UnionTransform<'_>,
) -> Plan {
    let root = plan
        .root()
        .expect("copy_plan_transforming requires a Plan with a root set");
    let mut new_plan = Plan::new();
    let new_root = copy_node(plan, root, &mut new_plan, transform_gate, transform_union);
    new_plan.set_root(new_root);
    new_plan
}

fn copy_node(
    old_plan: &Plan,
    old_id: NodeId,
    new_plan: &mut Plan,
    transform_gate: &mut GateTransform<'_>,
    transform_union: &mut UnionTransform<'_>,
) -> NodeId {
    match old_plan
        .get(old_id)
        .unwrap_or_else(|| panic!("dangling NodeId {old_id} while copying a Plan"))
    {
        PlanNodeKind::Leaf {
            fragment,
            provenance,
        } => new_plan.add_node(PlanNodeKind::Leaf {
            fragment: fragment.clone(),
            provenance: provenance.clone(),
        }),
        PlanNodeKind::Compose { children, strategy } => {
            let strategy = *strategy;
            let new_children: Vec<NodeId> = children
                .iter()
                .map(|&c| copy_node(old_plan, c, new_plan, transform_gate, transform_union))
                .collect();
            new_plan.add_node(PlanNodeKind::Compose {
                children: new_children,
                strategy,
            })
        }
        PlanNodeKind::Union { children } => {
            let reordered = transform_union(old_id, children);
            let new_children: Vec<NodeId> = reordered
                .iter()
                .map(|&c| copy_node(old_plan, c, new_plan, transform_gate, transform_union))
                .collect();
            new_plan.add_node(PlanNodeKind::Union {
                children: new_children,
            })
        }
        PlanNodeKind::Replace { cascade, children } => {
            let cascade = cascade.clone();
            let new_children: Vec<NodeId> = children
                .iter()
                .map(|&c| copy_node(old_plan, c, new_plan, transform_gate, transform_union))
                .collect();
            new_plan.add_node(PlanNodeKind::Replace {
                cascade,
                children: new_children,
            })
        }
        PlanNodeKind::Gate {
            partition,
            children,
        } => {
            let (new_partition, kept_old_children) = transform_gate(old_id, partition, children);
            let new_children: Vec<NodeId> = kept_old_children
                .iter()
                .map(|&c| copy_node(old_plan, c, new_plan, transform_gate, transform_union))
                .collect();
            new_plan.add_node(PlanNodeKind::Gate {
                partition: new_partition,
                children: new_children,
            })
        }
    }
}

/// The second same-relation topology (module doc): a copy of `plan` with every
/// `Gate` node's `partition.groups` (and each group's OWN paired `children` entry) reversed. Each
/// group's key travels with its own child in lockstep -- the soundness caveat this module doc
/// discusses: `build_controllable` re-derives a group's `subrule_ok` from THAT group's own key, so
/// as long as key and child stay paired, reordering groups cannot desync which key gates which
/// compiled network. Only the ORDER changes, never membership, so [`differential_oracle`] run over
/// `plan` and `permute_gate_groups(plan)` is expected to `Agree` (module doc: union is commutative,
/// the build always ends in `minimize_checked`).
///
/// # Panics
/// Via [`Plan::add_node`]'s own debug-only invariant, if `plan` contains a malformed `Gate` node
/// (groups/children length mismatch) -- not a new invariant this function introduces.
pub fn permute_gate_groups(plan: &Plan) -> Plan {
    copy_plan_transforming(
        plan,
        &mut |_id, partition, children| {
            assert_eq!(
                partition.groups.len(),
                children.len(),
                "permute_gate_groups: Gate node must have one child per partition group"
            );
            let mut paired: Vec<(GateGroupSpec, NodeId)> = partition
                .groups
                .iter()
                .cloned()
                .zip(children.iter().copied())
                .collect();
            paired.reverse();
            let groups: Vec<GateGroupSpec> = paired.iter().map(|(key, _)| key.clone()).collect();
            let kept_children: Vec<NodeId> = paired.iter().map(|(_, child)| *child).collect();
            (
                GatePartitionSpec {
                    gated_subrules: partition.gated_subrules.clone(),
                    groups,
                },
                kept_children,
            )
        },
        &mut |_id, children| children.to_vec(),
    )
}

/// The `Union` second-topology generator (module doc: "sound, and now shipped"): a copy of `plan` with
/// every `Union` node's `children` reversed. Sound because `Union` denotes a set union over its
/// children (`plan.rs`'s own doc: "merges independently-compiled branches"), and set union is
/// commutative -- reordering `children` cannot change what the node denotes, independent of whether
/// anything downstream actually builds a genuinely different net from it (module doc's own discussion
/// of why this is a real, if maximally boring, second topology for THIS oracle even though
/// `enumerate_candidates` declines to offer it as a compilation choice).
///
/// Does not require `plan` to contain any `Union` node at all -- reversing every `Union` node's
/// children is simply a no-op walk if none exists (this crate's `enumerate_default` plans do not
/// always have a root `Union`; an ungated, marker-free grammar collapses straight to a bare `Gate`
/// root, `enumerate.rs`'s own doc), unlike [`permute_gate_groups`] which needs a well-formed `Gate` to
/// do anything meaningful.
pub fn permute_union_children(plan: &Plan) -> Plan {
    copy_plan_transforming(
        plan,
        &mut |_id, partition, children| {
            (
                GatePartitionSpec {
                    gated_subrules: partition.gated_subrules.clone(),
                    groups: partition.groups.clone(),
                },
                children.to_vec(),
            )
        },
        &mut |_id, children| {
            let mut reversed = children.to_vec();
            reversed.reverse();
            reversed
        },
    )
}

// -------------------------------------------------------------------------------------------------
// Partition refinement: a THIRD sound Gate-node restructuring (cardinality, not order)
// -------------------------------------------------------------------------------------------------

/// How finely [`refine_gate_partition`] subdivides each eligible partition group's own entries.
/// Both variants are sound by the SAME argument (this section's own doc); they differ only in how
/// many pieces a group is cut into, which is exactly what makes them genuinely different candidate
/// `Plan`s (different `Gate.partition.groups.len()`, different content address) rather than the same
/// restructuring wearing two names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionGranularity {
    /// Split an eligible group into at most 2 contiguous sub-groups.
    Bisect,
    /// Split an eligible group into one singleton sub-group per entry (maximal refinement).
    FanOut,
}

impl PartitionGranularity {
    /// The sizes of the sub-groups `total` entries are cut into, in order, summing to `total`
    /// (`0` for `total == 0`, matching [`build::build_controllable`]'s own "an empty group
    /// contributes nothing" convention -- a zero-entry group is simply dropped, never fabricated).
    fn chunk_sizes(self, total: usize) -> Vec<usize> {
        match self {
            Self::Bisect => {
                if total < 2 {
                    // Nothing to split: 0 or 1 entries has no non-trivial bisection. Returning
                    // `vec![total]` (not `vec![]`) keeps the single existing entry (if any) in
                    // its own unsplit group, so this is a true no-op on such a group -- not a
                    // silent drop.
                    vec![total]
                } else {
                    let half = total / 2;
                    vec![total - half, half]
                }
            }
            Self::FanOut => vec![1; total],
        }
    }
}

/// One gate group's `Compose` shape, as [`crate::enumerate::enumerate_default`] always builds it
/// (that module's own "Shape" doc): exactly 2 children, `[0]` the group's `LexiconFragment` leaf,
/// `[1]` its own `Replace` node, [`ComposeStrategy::Static`]. Duplicated from `crate::build`'s
/// private `gate_group_children` rather than shared across the module boundary (this module is a
/// `Plan`-to-`Plan` rewrite, not a builder -- the same "don't reach into `build.rs`" discipline this
/// module's own doc already follows for [`permute_gate_groups`]/[`permute_union_children`]). No
/// strategy guard: [`ComposeStrategy`] has only `Static`, so every `Compose` node is `Static` by
/// construction.
fn gate_group_parts(plan: &Plan, compose_id: NodeId) -> (NodeId, NodeId) {
    let PlanNodeKind::Compose { children, .. } = plan
        .get(compose_id)
        .unwrap_or_else(|| panic!("dangling Compose NodeId {compose_id} in plan"))
    else {
        panic!("expected a Compose node as a Gate group's child at {compose_id}");
    };
    assert_eq!(
        children.len(),
        2,
        "a gate-group Compose node must have exactly 2 children (LexiconFragment leaf, Replace \
         node), got {} at {compose_id}",
        children.len()
    );
    (children[0], children[1])
}

/// A gate group's `LexiconFragment` leaf, resolved to its `entries` list (mirrors `crate::build`'s
/// private `lexicon_fragment_entries` -- same duplication rationale as [`gate_group_parts`]).
fn lexicon_entries(plan: &Plan, lexicon_id: NodeId) -> Vec<LexEntryId> {
    let PlanNodeKind::Leaf { fragment, .. } = plan
        .get(lexicon_id)
        .unwrap_or_else(|| panic!("dangling LexiconFragment NodeId {lexicon_id}"))
    else {
        panic!("expected a Leaf node as a gate-group Compose node's first child at {lexicon_id}");
    };
    let FragmentSpec::LexiconFragment { entries } = fragment else {
        panic!(
            "expected FragmentSpec::LexiconFragment on the gate-group lexicon leaf at \
             {lexicon_id}, got {fragment:?}"
        );
    };
    entries.clone().unwrap_or_else(|| {
        panic!(
            "refine_gate_partition requires Some(entries) on every gate-group LexiconFragment \
             leaf (enumerate_default's own invariant); got None at {lexicon_id}"
        )
    })
}

/// A THIRD sound Gate-node restructuring, alongside [`permute_gate_groups`] (order) and
/// [`permute_union_children`] (a different node kind entirely): this one changes a `Gate` node's
/// partition CARDINALITY, not its order. [`permute_gate_groups`]'s own doc establishes that
/// [`build::build_controllable`] folds every group's compiled net together with
/// [`crate::compose_budget::union_checked`] (commutative) and always finishes with
/// [`crate::compose_budget::minimize_checked`] -- that argument shows group ORDER is inert. This
/// function needs one more step, but it is a standard one: **composition distributes over union**
/// (`(A ∪ B) .o. R == (A .o. R) ∪ (B .o. R)` for any relation `R`) -- a basic fact about relational
/// composition, true independent of anything `foma`-specific. Read one eligible group's own entries
/// as `A ∪ B` (an arbitrary partition of that SAME entry set into disjoint pieces) and its own
/// `Replace` cascade as `R`: splitting the group's `LexiconFragment` into several smaller
/// `LexiconFragment`s, each still composed with the group's OWN, UNCHANGED `Replace` node (so
/// `subrule_ok` -- read from that node's own `cascade.gated_subrules`/`group_key`, the same
/// content-pure discipline this module's top doc discusses -- is identical for every sub-group), and re-unioning them back together produces
/// EXACTLY the original group's compiled net, byte for byte, before the outer union-of-all-groups
/// fold even runs. Refining a partition therefore cannot change what the whole `Gate` node accepts,
/// for the same underlying reason `permute_gate_groups` cannot: only ENTRY MEMBERSHIP matters to the
/// final accepted relation, and refinement (like reordering) never touches membership -- it only
/// changes how many pieces the union is built from and in what shape.
///
/// A group with fewer than 2 entries (nothing to split) or a fixture where NO group is eligible
/// (module-external callers are expected to gate on `Applicability` before even reaching this
/// function, e.g. `recipe_registry`'s own `HasSplittableGateGroup`) is a genuine no-op: every
/// `LexiconFragment`/`Compose` this function reconstructs for such a group has IDENTICAL content to
/// the original (same entries, same `Replace` id, same strategy), so `Plan::add_node`'s content
/// addressing dedups it straight back to the SAME `NodeId` -- never a forced, purposeless rebuild.
///
/// Reuses each ORIGINAL group's own `Replace` `NodeId` for every one of its sub-groups (recomputed
/// via a plain recursive copy, never re-derived) -- content addressing dedups it back to ONE
/// stored node automatically, since none of `cascade.rules`/`gated_subrules`/`group_key` changed;
/// only each sub-group's OWN `LexiconFragment` leaf (a smaller `entries` subset) and its wrapping
/// `Compose` node are genuinely new.
///
/// # Panics
/// Via [`Plan::add_node`]'s own debug-only invariant, or the assertions in [`gate_group_parts`]/
/// [`lexicon_entries`], if `plan` is not shaped the way [`crate::enumerate::enumerate_default`]
/// always builds it -- not a new invariant this function introduces.
pub fn refine_gate_partition(plan: &Plan, granularity: PartitionGranularity) -> Plan {
    let root = plan
        .root()
        .expect("refine_gate_partition requires a Plan with a root set");
    let mut new_plan = Plan::new();
    let new_root = refine_node(plan, root, &mut new_plan, granularity);
    new_plan.set_root(new_root);
    new_plan
}

fn refine_node(
    old_plan: &Plan,
    old_id: NodeId,
    new_plan: &mut Plan,
    granularity: PartitionGranularity,
) -> NodeId {
    match old_plan
        .get(old_id)
        .unwrap_or_else(|| panic!("dangling NodeId {old_id} while refining a Plan"))
    {
        PlanNodeKind::Leaf {
            fragment,
            provenance,
        } => new_plan.add_node(PlanNodeKind::Leaf {
            fragment: fragment.clone(),
            provenance: provenance.clone(),
        }),
        PlanNodeKind::Compose { children, strategy } => {
            let strategy = *strategy;
            let new_children: Vec<NodeId> = children
                .iter()
                .map(|&c| refine_node(old_plan, c, new_plan, granularity))
                .collect();
            new_plan.add_node(PlanNodeKind::Compose {
                children: new_children,
                strategy,
            })
        }
        PlanNodeKind::Union { children } => {
            let new_children: Vec<NodeId> = children
                .iter()
                .map(|&c| refine_node(old_plan, c, new_plan, granularity))
                .collect();
            new_plan.add_node(PlanNodeKind::Union {
                children: new_children,
            })
        }
        PlanNodeKind::Replace { cascade, children } => {
            let cascade = cascade.clone();
            let new_children: Vec<NodeId> = children
                .iter()
                .map(|&c| refine_node(old_plan, c, new_plan, granularity))
                .collect();
            new_plan.add_node(PlanNodeKind::Replace {
                cascade,
                children: new_children,
            })
        }
        PlanNodeKind::Gate {
            partition,
            children,
        } => {
            assert_eq!(
                partition.groups.len(),
                children.len(),
                "refine_gate_partition: Gate node must have one child per partition group"
            );
            let mut new_groups: Vec<GateGroupSpec> = Vec::new();
            let mut new_children: Vec<NodeId> = Vec::new();
            for (group, &compose_id) in partition.groups.iter().zip(children.iter()) {
                let (lexicon_id, replace_id) = gate_group_parts(old_plan, compose_id);
                let entries = lexicon_entries(old_plan, lexicon_id);
                // The group's own Replace node's CONTENT is untouched by refinement (same rules,
                // same gated_subrules, same group_key) -- copying it recursively re-derives the
                // SAME NodeId via content addressing, so every sub-group split from this group
                // below shares that ONE copy, never a re-derivation per sub-group.
                let new_replace_id = refine_node(old_plan, replace_id, new_plan, granularity);
                let mut offset = 0usize;
                for size in granularity.chunk_sizes(entries.len()) {
                    if size == 0 {
                        continue;
                    }
                    let chunk: Vec<LexEntryId> = entries[offset..offset + size].to_vec();
                    offset += size;
                    let lexicon_leaf = new_plan.add_node(PlanNodeKind::Leaf {
                        fragment: FragmentSpec::LexiconFragment {
                            entries: Some(chunk),
                        },
                        provenance: Provenance::Lexicon,
                    });
                    let compose = new_plan.add_node(PlanNodeKind::Compose {
                        children: vec![lexicon_leaf, new_replace_id],
                        strategy: ComposeStrategy::Static,
                    });
                    new_groups.push(GateGroupSpec {
                        key: group.key.clone(),
                    });
                    new_children.push(compose);
                }
            }
            new_plan.add_node(PlanNodeKind::Gate {
                partition: GatePartitionSpec {
                    gated_subrules: partition.gated_subrules.clone(),
                    groups: new_groups,
                },
                children: new_children,
            })
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Seeded random subtree mutation
// -------------------------------------------------------------------------------------------------

/// A tiny, deterministic, seedable pseudo-random source (SplitMix64, Vigna 2015) -- used ONLY by
/// [`mutate_plan_seeded`] to pick a reproducible mutation target and permutation.
///
/// Hand-rolled rather than a dependency, on purpose: this task's own hard rule is no new crate
/// dependencies, and SplitMix64 is a handful of lines of well-known public-domain arithmetic (the
/// same generator many `rand`-crate seeders use internally to expand a single seed) -- there is no
/// quality reason to pull in an external RNG crate for something this small. It makes no
/// cryptographic-security claim, and needs none: [`mutate_plan_seeded`]'s only requirement is
/// reproducibility, not unpredictability.
///
/// Why not `std`'s `DefaultHasher`/`RandomState`, the system clock, or a thread-local RNG: every one
/// of those is either unseeded, reseeds per process, or reads the clock -- any of which would make
/// the SAME seed produce a DIFFERENT mutation on a second run, and an oracle failure that cannot be
/// replayed from its own reported seed is worthless (this task's own "no wall-clock, no thread-local
/// RNG" requirement -- mirrors [`crate::plan::NodeId`]'s own `StableHasher` doc making the identical
/// argument for content addressing).
struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        SplitMix64(seed)
    }

    /// Advances the generator and returns the next 64-bit output (Vigna's SplitMix64 step: golden-
    /// ratio increment, then a fixed 3-round bit-mixing finalizer).
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A pseudo-uniform index in `0..bound` (`bound` must be `> 0`). Plain modulo, not Lemire's
    /// unbiased-rejection trick: `bound` here is always a small plan-node/child count (never remotely
    /// close to `u64::MAX`), so the modulo bias is negligible for a test/fuzz-harness RNG that makes
    /// no statistical-uniformity claim -- not worth the extra complexity.
    fn next_below(&mut self, bound: usize) -> usize {
        assert!(bound > 0, "next_below requires a positive bound");
        (self.next_u64() % bound as u64) as usize
    }
}

/// A uniformly-random permutation of `0..n`, drawn from `rng` by Fisher-Yates (the standard unbiased
/// in-place shuffle) -- the permutation [`mutate_plan_seeded`] applies to a target's children (and,
/// for a `Gate`, its paired `partition.groups` in lockstep -- [`apply_permutation_at`]'s own doc).
fn random_permutation(rng: &mut SplitMix64, n: usize) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = rng.next_below(i + 1);
        perm.swap(i, j);
    }
    perm
}

/// One node [`mutate_plan_seeded`] is allowed to target: a `Gate` with `>= 2` groups, or a `Union`
/// with `>= 2` children -- module doc's finding that these are the ONLY two node kinds with a proven
/// relation-preserving restructuring. `child_count` is the length any permutation drawn for this node
/// must have (`partition.groups.len()` for a `Gate`, `children.len()` for a `Union`).
struct MutationTarget {
    node_id: NodeId,
    kind: &'static str,
    child_count: usize,
}

/// Every [`MutationTarget`] in `plan`, in [`Plan::iter`]'s own content-address order (that method's
/// own doc: deterministic, not insertion order and not a `HashMap` order) -- so the SAME plan always
/// yields the SAME target list regardless of how it was built, which is what makes indexing into this
/// list by an RNG draw reproducible. A `Gate`/`Union` with fewer than 2 children is excluded: there is
/// no non-trivial permutation of 0 or 1 items, so it is not a real mutation target.
fn eligible_mutation_targets(plan: &Plan) -> Vec<MutationTarget> {
    plan.iter()
        .filter_map(|(id, kind)| match kind {
            PlanNodeKind::Gate { partition, .. } if partition.groups.len() >= 2 => {
                Some(MutationTarget {
                    node_id: id,
                    kind: "Gate",
                    child_count: partition.groups.len(),
                })
            }
            PlanNodeKind::Union { children } if children.len() >= 2 => Some(MutationTarget {
                node_id: id,
                kind: "Union",
                child_count: children.len(),
            }),
            _ => None,
        })
        .collect()
}

/// Copies `plan` unchanged (every `Gate`/`Union` node's children kept in their original order) --
/// [`copy_plan_transforming`] with two pass-through callbacks. Used where a caller needs an owned
/// `Plan` but has nothing to mutate (e.g. [`mutate_plan_seeded`] on a plan with zero eligible
/// targets).
fn identity_copy(plan: &Plan) -> Plan {
    copy_plan_transforming(
        plan,
        &mut |_id, partition, children| {
            (
                GatePartitionSpec {
                    gated_subrules: partition.gated_subrules.clone(),
                    groups: partition.groups.clone(),
                },
                children.to_vec(),
            )
        },
        &mut |_id, children| children.to_vec(),
    )
}

/// Applies exactly ONE permutation at exactly one target node -- a random
/// relation-preserving restructuring at a randomly chosen subtree, built on the SAME
/// [`copy_plan_transforming`] walk [`permute_gate_groups`]/[`permute_union_children`] use, just
/// parameterized by a single `(target, permutation)` pair resolved at runtime instead of a fixed
/// whole-plan rule. Every OTHER `Gate`/`Union` node in `plan` is copied with its children in their
/// ORIGINAL order -- only `target`'s own children (and, if `target` is a `Gate`, its paired
/// `partition.groups`, in lockstep -- the same soundness discipline [`permute_gate_groups`]'s own
/// doc states) are reordered by `permutation`.
///
/// # Panics (debug only)
/// If `permutation`'s length does not match `target`'s own child count.
fn apply_permutation_at(plan: &Plan, target: NodeId, permutation: &[usize]) -> Plan {
    let reorder = |children: &[NodeId]| -> Vec<NodeId> {
        debug_assert_eq!(
            children.len(),
            permutation.len(),
            "apply_permutation_at: permutation length must match the target node's own child count"
        );
        permutation.iter().map(|&i| children[i]).collect()
    };
    copy_plan_transforming(
        plan,
        &mut |id, partition, children| {
            if id == target {
                let groups: Vec<GateGroupSpec> = permutation
                    .iter()
                    .map(|&i| partition.groups[i].clone())
                    .collect();
                (
                    GatePartitionSpec {
                        gated_subrules: partition.gated_subrules.clone(),
                        groups,
                    },
                    reorder(children),
                )
            } else {
                (
                    GatePartitionSpec {
                        gated_subrules: partition.gated_subrules.clone(),
                        groups: partition.groups.clone(),
                    },
                    children.to_vec(),
                )
            }
        },
        &mut |id, children| {
            if id == target {
                reorder(children)
            } else {
                children.to_vec()
            }
        },
    )
}

/// The replay-recipe for one [`mutate_plan_seeded`] draw: the seed must appear in any
/// failure report so a disagreement can be replayed exactly. `target_kind`/`target_node_id`/
/// `permutation` are the mutation's own already-resolved output -- enough to redo exactly this
/// mutation without re-running the RNG at all; `seed` is carried too purely for a human-readable
/// failure report, not because replay needs to re-draw from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationRecipe {
    pub seed: u64,
    pub target_kind: &'static str,
    pub target_node_id: NodeId,
    pub permutation: Vec<usize>,
}

impl fmt::Display for MutationRecipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "mutate_plan_seeded(seed={}) -> permute {} {} by {:?}",
            self.seed, self.target_kind, self.target_node_id, self.permutation
        )
    }
}

/// [`mutate_plan_seeded`]'s return: the mutated plan, plus the [`MutationRecipe`] that produced it.
/// `recipe` is `None` iff `plan` had no [`MutationTarget`] at all -- `plan` is returned unchanged in
/// that case (an honest "nothing sound to mutate here", never a panic).
pub struct MutationOutcome {
    pub plan: Plan,
    pub recipe: Option<MutationRecipe>,
}

/// A deterministic, seeded generator that applies ONE random relation-preserving
/// restructuring -- [`permute_gate_groups`]'s per-node form or [`permute_union_children`]'s, the only
/// two node kinds this module has proven sound (module doc) -- at a randomly chosen subtree of
/// `plan`.
///
/// # Determinism
/// The SAME `seed` against the SAME `plan` always draws the SAME target and the SAME permutation:
/// [`SplitMix64`] is a pure function of its seed and draw count, and [`eligible_mutation_targets`]'s
/// order is content-address-deterministic ([`Plan::iter`]'s own doc) -- nothing here reads the clock,
/// a thread-local, or any other non-reproducible source.
///
/// # Non-vacuity
/// Not a no-op generator in disguise: across many seeds this draws every permutation of a target's
/// children with roughly equal likelihood, INCLUDING the identity permutation for some seeds (a real
/// drawn outcome, not a special case avoided) -- see this module's own
/// `different_seeds_produce_different_topologies` test, which requires at least two DIFFERENT root
/// `NodeId`s across a seed sweep, ruling out a generator that always echoes its input.
///
/// # Soundness
/// Only ever applies a transformation this module has itself argued sound (module doc) -- never a
/// `Compose` reorder, never a `Replace` cascade reorder, by construction: [`eligible_mutation_targets`]
/// simply never names those kinds as targets.
pub fn mutate_plan_seeded(plan: &Plan, seed: u64) -> MutationOutcome {
    let targets = eligible_mutation_targets(plan);
    if targets.is_empty() {
        return MutationOutcome {
            plan: identity_copy(plan),
            recipe: None,
        };
    }

    let mut rng = SplitMix64::new(seed);
    let target_idx = rng.next_below(targets.len());
    let target = &targets[target_idx];
    let permutation = random_permutation(&mut rng, target.child_count);

    let mutated = apply_permutation_at(plan, target.node_id, &permutation);
    MutationOutcome {
        plan: mutated,
        recipe: Some(MutationRecipe {
            seed,
            target_kind: target.kind,
            target_node_id: target.node_id,
            permutation,
        }),
    }
}

// -------------------------------------------------------------------------------------------------
// Failure minimisation to a named recipe
// -------------------------------------------------------------------------------------------------

/// One step in a [`minimize_disagreement`] repro sequence: a named, replayable plan transform.
/// `transform` lives behind an `Rc` (not a `Box`) purely so `MutationStep` can be [`Clone`] --
/// [`minimize_disagreement`]'s shrink loop needs to try removing one step and re-running the rest
/// without consuming the sequence it might have to restore, and `Rc::clone` is a cheap pointer bump,
/// never a real clone of the closure or the plan.
#[derive(Clone)]
pub struct MutationStep {
    pub description: String,
    transform: Rc<dyn Fn(&Plan) -> Plan>,
}

impl MutationStep {
    /// A step that redraws [`mutate_plan_seeded`] against WHATEVER plan this step is applied to at
    /// its position in a chain -- described by its `seed` alone. The target and permutation are
    /// re-derived when the step actually runs, not frozen at construction time: chaining this way
    /// means dropping an EARLIER step changes what a LATER step's own draw sees, exactly the
    /// "revert a step, replay the rest" behavior delta-debugging needs.
    pub fn from_seed(seed: u64) -> Self {
        MutationStep {
            description: format!("mutate_plan_seeded(seed={seed})"),
            transform: Rc::new(move |p: &Plan| mutate_plan_seeded(p, seed).plan),
        }
    }
}

/// Applies every step in `steps`, in order, starting from `base` -- step *i* transforms the OUTPUT of
/// step *i-1*, not `base` itself (module doc: this is what makes dropping an earlier step change a
/// later step's own behavior). Requires at least one step (asserted): an empty chain would mean "the
/// mutated plan is `base` itself", which every caller in this module already special-cases rather
/// than routing through here.
fn apply_step_chain(base: &Plan, steps: &[MutationStep]) -> Plan {
    assert!(
        !steps.is_empty(),
        "apply_step_chain requires at least one step"
    );
    let mut current = (steps[0].transform)(base);
    for step in &steps[1..] {
        current = (step.transform)(&current);
    }
    current
}

/// The minimised reproducer [`minimize_disagreement`] returns: the smallest surviving step sequence
/// (in order, by `description`) still known to disagree with the base plan, plus the shortest
/// disagreeing word and symmetric difference [`resolve_verdict`] itself computed for that minimal
/// sequence (NOT the original, possibly-larger one).
///
/// `Display` is the paste-into-a-test recipe format: a numbered step list plus the
/// witness word/symmetric-difference, one field per line -- see this module's own
/// `minimization_converges_to_the_injected_breakage` test for a verbatim example of what this prints
/// on a real (deliberately broken) case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimizedRecipe {
    pub steps: Vec<String>,
    pub word: String,
    pub only_in_a: BTreeSet<String>,
    pub only_in_b: BTreeSet<String>,
}

impl fmt::Display for MinimizedRecipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "oracle disagreement recipe:")?;
        writeln!(f, "  steps:")?;
        for (i, step) in self.steps.iter().enumerate() {
            writeln!(f, "    {}. {}", i + 1, step)?;
        }
        writeln!(f, "  shortest disagreeing word: {:?}", self.word)?;
        writeln!(f, "  only_in_a: {:?}", self.only_in_a)?;
        write!(f, "  only_in_b: {:?}", self.only_in_b)
    }
}

/// Shrinks `steps` (a sequence already known to make `differential_oracle(base_plan,
/// apply_step_chain(base_plan, steps), ...)` return `Disagree`, asserted up front) to a 1-minimal
/// subsequence -- repeatedly tries dropping each remaining step and keeps the drop permanently
/// whenever the shorter sequence STILL disagrees, over full passes, until no single step is removable
/// without losing the disagreement (a standard delta-debugging fixed point). Never shrinks below one
/// step (an empty sequence trivially agrees with itself, so is never a legal reproducer).
///
/// # Panics
/// If the FULL `steps` sequence does not actually disagree with `base_plan` to begin with -- there is
/// nothing to minimise, and silently returning something anyway would misreport a non-bug as one.
///
/// # Errors
/// Propagates a [`ComposeError`] from any [`differential_oracle`] call this makes (same convention as
/// `differential_oracle` itself).
#[allow(clippy::too_many_arguments)] // mirrors differential_oracle's own many-parameter convention
pub fn minimize_disagreement(
    base_plan: &Plan,
    steps: Vec<MutationStep>,
    labels: (&str, &str),
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    budget: &ComposeBudget,
    words: &[&str],
) -> Result<MinimizedRecipe, ComposeError> {
    assert!(
        !steps.is_empty(),
        "minimize_disagreement requires at least one mutation step to shrink"
    );

    let full_mutated = apply_step_chain(base_plan, &steps);
    match differential_oracle(
        base_plan,
        &full_mutated,
        labels,
        opts,
        g,
        alphabet,
        prules_in_order,
        budget,
        words,
    )? {
        OracleResult::Agree => panic!(
            "minimize_disagreement called with a step sequence that does not actually disagree with \
             the base plan -- nothing to minimise"
        ),
        OracleResult::Disagree { .. } => {}
    }

    let mut current = steps;
    loop {
        let mut shrank = false;
        let mut i = 0;
        while i < current.len() {
            if current.len() == 1 {
                break; // never shrink below one step
            }
            let mut candidate = current.clone();
            candidate.remove(i);
            let candidate_mutated = apply_step_chain(base_plan, &candidate);
            let verdict = differential_oracle(
                base_plan,
                &candidate_mutated,
                labels,
                opts,
                g,
                alphabet,
                prules_in_order,
                budget,
                words,
            )?;
            match verdict {
                OracleResult::Disagree { .. } => {
                    current = candidate;
                    shrank = true;
                    // Don't advance i: the list shrank by one, so index i now names the NEXT
                    // element -- re-check it rather than skip over it.
                }
                OracleResult::Agree => {
                    i += 1;
                }
            }
        }
        if !shrank {
            break;
        }
    }

    let final_mutated = apply_step_chain(base_plan, &current);
    let verdict = differential_oracle(
        base_plan,
        &final_mutated,
        labels,
        opts,
        g,
        alphabet,
        prules_in_order,
        budget,
        words,
    )?;
    let (word, only_in_a, only_in_b) = match verdict {
        OracleResult::Disagree {
            word,
            only_in_a,
            only_in_b,
            ..
        } => (word, only_in_a, only_in_b),
        OracleResult::Agree => {
            unreachable!(
                "the shrink loop only ever keeps a `current` sequence proven to still disagree"
            )
        }
    };

    Ok(MinimizedRecipe {
        steps: current.iter().map(|s| s.description.clone()).collect(),
        word,
        only_in_a,
        only_in_b,
    })
}

#[cfg(test)]
mod tests {
    //! Three outcomes the task requires, in this order: (1) two genuinely distinct SAME-relation
    //! plans (`enumerate_default` vs. [`permute_gate_groups`] of it) -> `Agree`; (2) a deliberately
    //! WRONG second plan (one gate group dropped, module doc's `drop_last_gate_group`) -> a real
    //! `Disagree` naming a concrete word and a non-empty symmetric difference, proving the oracle is
    //! not vacuous; (3) the shortest-witness tie-break, tested directly against
    //! [`resolve_verdict`] with synthetic multi-length word data (this repo's tiny synthetic
    //! fixtures only ever recognize single-segment surface forms, so exercising the length tie-break
    //! through a real grammar+build would need a needlessly elaborate fixture -- testing the pure
    //! selection function directly is the more direct proof of this specific claim).

    use std::collections::HashSet;

    use pg_grammar::model::{Grammar, PhonRuleDef};

    use super::*;
    use crate::enumerate::enumerate_default;
    use crate::junctions::PhonologyProbe;
    use crate::plan::ReplaceCascadeSpec;

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

    /// One MPR-gated subrule and two entries realizing both truth values of that gate key -- the
    /// same shape as `enumerate.rs`'s own private `gated_two_group_fixture` / `build.rs`'s own
    /// duplicate of it (`gated_two_group_fixture_xml`), duplicated here again for the same reason
    /// those two modules each duplicate it rather than share it across a `#[cfg(test)]` boundary.
    /// Synthetic and delanguaged per this repo's own conformance-grammar convention: `e0` (no
    /// `ruleFeatures`) realizes gate key `[false]` (its underlying "p" surfaces unchanged); `e1`
    /// (`ruleFeatures="mpr1"`) realizes `[true]` (its underlying "p" surfaces as "q") -- so "p" and
    /// "q" are the two words that can only ever be produced by exactly one of the two gate groups,
    /// which is exactly the property both this module's Agree and Disagree tests need.
    fn oracle_gated_two_group_fixture_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>OracleGatedTwoGroupFixture</Name>
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
    }

    /// Test-only deliberately-WRONG second topology (module doc, `copy_plan_transforming`): drops the
    /// LAST gate group (post `enumerate_default`'s own sort-by-key, this is the group with the
    /// lexicographically-largest key -- `[true]` on the two-group fixture, i.e. entry `e1`, the ONLY
    /// entry that ever produces surface "q"). A real under-generating topology, not a no-op:
    /// [`differential_oracle`] run against the un-truncated plan must catch this. Also reused, as a
    /// deliberately-UNSOUND [`MutationStep`], to prove [`minimize_disagreement`] converges on the
    /// real culprit when mixed in among genuinely sound mutation steps (`minimisation` tests below).
    fn drop_last_gate_group(plan: &Plan) -> Plan {
        copy_plan_transforming(
            plan,
            &mut |_id, partition, children| {
                assert_eq!(partition.groups.len(), children.len());
                assert!(
                    partition.groups.len() >= 2,
                    "drop_last_gate_group needs >=2 groups to drop one and still have a non-empty Gate"
                );
                let keep = partition.groups.len() - 1;
                let groups = partition.groups[..keep].to_vec();
                let kept_children = children[..keep].to_vec();
                (
                    GatePartitionSpec {
                        gated_subrules: partition.gated_subrules.clone(),
                        groups,
                    },
                    kept_children,
                )
            },
            &mut |_id, children| children.to_vec(),
        )
    }

    /// A test-only [`MutationStep`] wrapping [`drop_last_gate_group`] -- the SAME deliberately-wrong
    /// breakage the oracle's own non-vacuity test uses, repackaged as a step so
    /// `minimize_disagreement` can be handed a sequence that mixes genuinely sound mutations with one
    /// real bug and be proven to find exactly the bug, discarding everything harmless around it.
    fn breakage_step() -> MutationStep {
        MutationStep {
            description: "BREAKAGE(test-only, deliberately unsound): drop_last_gate_group"
                .to_string(),
            transform: Rc::new(drop_last_gate_group),
        }
    }

    /// A grammar with ONE ordinary (should_run=true), ungated phonological rule -- `partition_entries`
    /// collapses to a single group (module doc: not itself a [`MutationTarget`]), but `should_run`
    /// being `true` means `enumerate_default`'s root is a `Union` wrapping that single-group `Gate`
    /// plus a composite-emission marker leaf (`enumerate.rs`'s own "Shape" doc) -- exactly the shape
    /// [`permute_union_children`] needs a real grammar to exercise. Synthetic/delanguaged per this
    /// repo's convention: `c1` ("p") unconditionally surfaces as `c2` ("b").
    fn oracle_union_root_fixture_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>OracleUnionRootFixture</Name>
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
    }

    /// An ungated grammar (no MPR features declared at all, so `partition_entries` collapses to
    /// exactly 1 group -- `enumerate.rs`'s own "ungated grammar collapses to a single-group Gate")
    /// whose one group holds THREE entries, not one -- the smallest fixture that can distinguish
    /// [`PartitionGranularity::Bisect`] (2 sub-groups: 2+1) from [`PartitionGranularity::FanOut`]
    /// (3 singleton sub-groups) from EACH OTHER, not just from baseline. No phonological rule at
    /// all (irrelevant to what this fixture exercises -- partition CARDINALITY, not the rewrite
    /// cascade), so `should_run` is `false` and the plan root collapses directly to the bare `Gate`
    /// node (`enumerate.rs`'s own "root is Gate directly" case). Synthetic/delanguaged: three
    /// distinct one-segment surface forms ("p"/"t"/"k"), one per entry, so each entry's own analysis
    /// is independently checkable via `apply_up`.
    fn oracle_three_entry_ungated_fixture_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>OracleThreeEntryUngatedFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c3"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e0" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e0</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>t</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e1</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e2" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a2"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e2</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
    }

    /// A grammar with TWO ungated, ordinary phonological rules in one cascade (`pr1` then `pr2`) --
    /// used only to prove [`Replace`]'s cascade-order finding empirically: reversing `cascade.rules`
    /// (+ its rule-leaf children) must make `build_controllable` panic, since it cross-validates the
    /// cascade against `prules_in_order` POSITIONALLY (`build.rs`'s own `validate_replace_cascade`).
    /// Synthetic/delanguaged: `c1` ("p") -> `c2` ("b") via `pr1`, `c2` ("b") -> `c3` ("d") via `pr2`
    /// (a real feeding cascade, so the two rules are not accidentally interchangeable even in
    /// principle -- not load-bearing for THIS test, which only needs `build_controllable` to panic
    /// before ever reaching `apply_up`, but avoids the reversed cascade being a coincidental identity
    /// besides).
    fn oracle_two_rule_cascade_fixture_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>OracleTwoRuleCascadeFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c3"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="pr1">
        <Name>PR1</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="pr2">
        <Name>PR2</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><Segment segment="c3" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="pr1 pr2">
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
    }

    /// Test-only deliberately-WRONG restructuring for `Compose` (module doc's `Compose` finding):
    /// swaps a gate group's `Compose` node's two children (`[lexicon fragment, Replace]` ->
    /// `[Replace, lexicon fragment]`). Everything else copied verbatim -- a standalone walker (not
    /// built on `copy_plan_transforming`, which has no `Compose` hook by design: production code
    /// never needs one, module doc's finding that no sound `Compose` generator exists).
    fn swap_compose_children(plan: &Plan) -> Plan {
        fn copy(old_plan: &Plan, old_id: NodeId, new_plan: &mut Plan) -> NodeId {
            match old_plan
                .get(old_id)
                .unwrap_or_else(|| panic!("dangling NodeId {old_id} while copying a Plan"))
            {
                PlanNodeKind::Leaf {
                    fragment,
                    provenance,
                } => new_plan.add_node(PlanNodeKind::Leaf {
                    fragment: fragment.clone(),
                    provenance: provenance.clone(),
                }),
                PlanNodeKind::Compose { children, strategy } => {
                    let strategy = *strategy;
                    assert_eq!(
                        children.len(),
                        2,
                        "swap_compose_children fixture must have a 2-child Compose node"
                    );
                    let mut swapped = children.clone();
                    swapped.swap(0, 1);
                    let new_children: Vec<NodeId> = swapped
                        .iter()
                        .map(|&c| copy(old_plan, c, new_plan))
                        .collect();
                    new_plan.add_node(PlanNodeKind::Compose {
                        children: new_children,
                        strategy,
                    })
                }
                PlanNodeKind::Union { children } => {
                    let new_children: Vec<NodeId> = children
                        .iter()
                        .map(|&c| copy(old_plan, c, new_plan))
                        .collect();
                    new_plan.add_node(PlanNodeKind::Union {
                        children: new_children,
                    })
                }
                PlanNodeKind::Replace { cascade, children } => {
                    let cascade = cascade.clone();
                    let new_children: Vec<NodeId> = children
                        .iter()
                        .map(|&c| copy(old_plan, c, new_plan))
                        .collect();
                    new_plan.add_node(PlanNodeKind::Replace {
                        cascade,
                        children: new_children,
                    })
                }
                PlanNodeKind::Gate {
                    partition,
                    children,
                } => {
                    let partition = partition.clone();
                    let new_children: Vec<NodeId> = children
                        .iter()
                        .map(|&c| copy(old_plan, c, new_plan))
                        .collect();
                    new_plan.add_node(PlanNodeKind::Gate {
                        partition,
                        children: new_children,
                    })
                }
            }
        }
        let root = plan.root().expect("plan must have a root");
        let mut new_plan = Plan::new();
        let new_root = copy(plan, root, &mut new_plan);
        new_plan.set_root(new_root);
        new_plan
    }

    /// Test-only deliberately-WRONG restructuring for `Replace` (module doc's `Replace` finding):
    /// reverses a `Replace` node's `cascade.rules` AND its rule-leaf `children` together (so the
    /// `Replace` node itself stays internally consistent -- `cascade.rules.len() == children.len()`,
    /// and `children[i]`'s own `FragmentSpec::RewriteRule` still names `cascade.rules[i]` -- the
    /// point of this test is that build_controllable rejects the CASCADE ORDER itself, not a
    /// secondary internal-consistency bug). Everything else copied verbatim; a standalone walker for
    /// the same reason `swap_compose_children` is.
    fn reverse_replace_cascade(plan: &Plan) -> Plan {
        fn copy(old_plan: &Plan, old_id: NodeId, new_plan: &mut Plan) -> NodeId {
            match old_plan
                .get(old_id)
                .unwrap_or_else(|| panic!("dangling NodeId {old_id} while copying a Plan"))
            {
                PlanNodeKind::Leaf {
                    fragment,
                    provenance,
                } => new_plan.add_node(PlanNodeKind::Leaf {
                    fragment: fragment.clone(),
                    provenance: provenance.clone(),
                }),
                PlanNodeKind::Compose { children, strategy } => {
                    let strategy = *strategy;
                    let new_children: Vec<NodeId> = children
                        .iter()
                        .map(|&c| copy(old_plan, c, new_plan))
                        .collect();
                    new_plan.add_node(PlanNodeKind::Compose {
                        children: new_children,
                        strategy,
                    })
                }
                PlanNodeKind::Union { children } => {
                    let new_children: Vec<NodeId> = children
                        .iter()
                        .map(|&c| copy(old_plan, c, new_plan))
                        .collect();
                    new_plan.add_node(PlanNodeKind::Union {
                        children: new_children,
                    })
                }
                PlanNodeKind::Replace { cascade, children } => {
                    assert_eq!(
                        cascade.rules.len(),
                        children.len(),
                        "reverse_replace_cascade requires the Replace invariant to hold going in"
                    );
                    assert!(
                        cascade.rules.len() >= 2,
                        "reverse_replace_cascade needs >=2 rules for a reversal to be a genuine \
                         reordering, not a no-op"
                    );
                    let mut rules = cascade.rules.clone();
                    rules.reverse();
                    let mut old_children = children.clone();
                    old_children.reverse();
                    let new_cascade = ReplaceCascadeSpec {
                        rules,
                        gated_subrules: cascade.gated_subrules.clone(),
                        group_key: cascade.group_key.clone(),
                    };
                    let new_children: Vec<NodeId> = old_children
                        .iter()
                        .map(|&c| copy(old_plan, c, new_plan))
                        .collect();
                    new_plan.add_node(PlanNodeKind::Replace {
                        cascade: new_cascade,
                        children: new_children,
                    })
                }
                PlanNodeKind::Gate {
                    partition,
                    children,
                } => {
                    let partition = partition.clone();
                    let new_children: Vec<NodeId> = children
                        .iter()
                        .map(|&c| copy(old_plan, c, new_plan))
                        .collect();
                    new_plan.add_node(PlanNodeKind::Gate {
                        partition,
                        children: new_children,
                    })
                }
            }
        }
        let root = plan.root().expect("plan must have a root");
        let mut new_plan = Plan::new();
        let new_root = copy(plan, root, &mut new_plan);
        new_plan.set_root(new_root);
        new_plan
    }

    fn hs(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // -----------------------------------------------------------------------------------------
    // Outcome 1: two genuinely distinct, SAME-relation plans -> Agree.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn permuted_gate_groups_is_a_genuinely_different_plan() {
        let g = load(oracle_gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        let plan_a = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let plan_b = permute_gate_groups(&plan_a);

        assert_ne!(
            plan_a.root(),
            plan_b.root(),
            "permute_gate_groups must produce a plan with a different root NodeId (module doc: \
             group order is part of the Gate node's content address) -- otherwise this would not \
             be a real second topology for the oracle to diff"
        );
    }

    #[test]
    fn differential_oracle_agrees_on_permuted_gate_groups_of_the_same_grammar() {
        let g = load(oracle_gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();

        let plan_a = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let plan_b = permute_gate_groups(&plan_a);

        let result = differential_oracle(
            &plan_a,
            &plan_b,
            ("enumerate_default", "permute_gate_groups"),
            &opts,
            &g,
            &alphabet,
            &ro,
            &budget,
            &["p", "q"],
        )
        .expect("both plans must build successfully on this fixture");

        match result {
            OracleResult::Agree => {}
            OracleResult::Disagree {
                word,
                only_in_a,
                only_in_b,
                ..
            } => panic!(
                "two same-relation topologies (a grammar's default enumeration and its gate-group- \
                 permuted twin) must Agree, not Disagree -- got a real divergence at {word:?}: \
                 only_in_a={only_in_a:?}, only_in_b={only_in_b:?}. Per this task's own instruction, \
                 this must be reported as a genuine finding, never papered over."
            ),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Outcome 2: a deliberately WRONG second plan -> the oracle actually catches it.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn differential_oracle_catches_a_dropped_gate_group_as_a_real_disagreement() {
        let g = load(oracle_gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();

        let plan_correct = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let plan_wrong = drop_last_gate_group(&plan_correct);
        assert_ne!(
            plan_correct.root(),
            plan_wrong.root(),
            "the truncated plan must be a genuinely different Plan"
        );

        let result = differential_oracle(
            &plan_correct,
            &plan_wrong,
            (
                "enumerate_default",
                "drop_last_gate_group (deliberately wrong)",
            ),
            &opts,
            &g,
            &alphabet,
            &ro,
            &budget,
            &["p", "q"],
        )
        .expect(
            "both plans must build successfully on this fixture (the truncated plan still has \
                  1 non-empty group left)",
        );

        match result {
            OracleResult::Agree => panic!(
                "dropping an entire gate group (entry e1, the only entry that ever produces \
                 surface \"q\") changes the relation -- the oracle returning Agree here would mean \
                 it is VACUOUS, which is exactly what this test exists to rule out"
            ),
            OracleResult::Disagree {
                word,
                only_in_a,
                only_in_b,
                plan_a_label,
                plan_b_label,
            } => {
                assert_eq!(
                    word, "q",
                    "the dropped group is exactly what makes surface \"q\" analyzable -- \"q\" must \
                     be the (only) disagreeing word here"
                );
                assert!(
                    !only_in_a.is_empty() || !only_in_b.is_empty(),
                    "a real disagreement must carry a non-empty symmetric difference, got \
                     only_in_a={only_in_a:?} only_in_b={only_in_b:?}"
                );
                assert!(
                    only_in_b.is_empty(),
                    "the truncated (wrong) plan must UNDER-generate on \"q\" -- nothing should be \
                     unique to it; got only_in_b={only_in_b:?}"
                );
                assert_eq!(plan_a_label, "enumerate_default");
                assert_eq!(plan_b_label, "drop_last_gate_group (deliberately wrong)");
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Outcome 3: shortest-witness tie-break, tested directly against the pure selection core.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn resolve_verdict_reports_the_shortest_disagreeing_word_regardless_of_input_order() {
        let per_word = vec![
            // A LONGER disagreement, listed FIRST -- proves selection is by length, not by
            // first-encountered order.
            ("longerword".to_string(), hs(&["X"]), hs(&["Y"])),
            ("hi".to_string(), hs(&["A"]), hs(&["B"])),
            // An agreeing word, to prove agreements are simply excluded, not treated as a tie.
            ("z".to_string(), hs(&["same"]), hs(&["same"])),
        ];

        match resolve_verdict(per_word, "planA", "planB") {
            OracleResult::Disagree { word, .. } => {
                assert_eq!(word, "hi", "the SHORTEST disagreeing word must be reported")
            }
            OracleResult::Agree => panic!("expected a Disagree (two words genuinely differ)"),
        }
    }

    #[test]
    fn resolve_verdict_breaks_same_length_ties_lexicographically() {
        let per_word = vec![
            ("zz".to_string(), hs(&["X"]), hs(&["Y"])),
            ("aa".to_string(), hs(&["A"]), hs(&["B"])),
        ];

        match resolve_verdict(per_word, "planA", "planB") {
            OracleResult::Disagree { word, .. } => {
                assert_eq!(word, "aa", "same-length ties must break lexicographically")
            }
            OracleResult::Agree => panic!("expected a Disagree (two words genuinely differ)"),
        }
    }

    #[test]
    fn resolve_verdict_agrees_when_every_word_matches() {
        let per_word = vec![
            ("p".to_string(), hs(&["e0"]), hs(&["e0"])),
            ("q".to_string(), hs(&["e1"]), hs(&["e1"])),
        ];
        assert_eq!(
            resolve_verdict(per_word, "planA", "planB"),
            OracleResult::Agree
        );
    }

    // -----------------------------------------------------------------------------------------
    // The `Union` second-topology generator, on a real grammar.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn enumerate_default_on_union_fixture_has_a_union_root_with_two_children() {
        let g = load(oracle_union_root_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());

        let root = plan.root().expect("root must be set");
        match plan.get(root).unwrap() {
            PlanNodeKind::Union { children } => {
                assert_eq!(
                    children.len(),
                    2,
                    "fixture sanity: Gate + composite-emission marker"
                )
            }
            other => panic!(
                "fixture sanity: expected a Union root, got {}",
                other.kind_name()
            ),
        }
    }

    #[test]
    fn permute_union_children_is_a_genuinely_different_plan() {
        let g = load(oracle_union_root_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        let plan_a = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let plan_b = permute_union_children(&plan_a);

        assert_ne!(
            plan_a.root(),
            plan_b.root(),
            "permute_union_children must produce a plan with a different root NodeId -- otherwise \
             this would not be a real second topology for the oracle to diff"
        );
    }

    #[test]
    fn differential_oracle_agrees_on_permuted_union_children_of_the_same_grammar() {
        let g = load(oracle_union_root_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();

        let plan_a = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let plan_b = permute_union_children(&plan_a);

        let result = differential_oracle(
            &plan_a,
            &plan_b,
            ("enumerate_default", "permute_union_children"),
            &opts,
            &g,
            &alphabet,
            &ro,
            &budget,
            &["p"],
        )
        .expect("both plans must build successfully on this fixture");

        match result {
            OracleResult::Agree => {}
            OracleResult::Disagree {
                word,
                only_in_a,
                only_in_b,
                ..
            } => panic!(
                "two same-relation topologies (a grammar's default enumeration and its root-Union- \
                 permuted twin) must Agree, not Disagree -- got a real divergence at {word:?}: \
                 only_in_a={only_in_a:?}, only_in_b={only_in_b:?}."
            ),
        }
    }

    // -----------------------------------------------------------------------------------------
    // `Compose`/`Replace` have NO sound generator -- confirmed empirically (module
    // doc), not just argued: reordering either is mechanically rejected by build_controllable.
    // -----------------------------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "expected a Leaf node as a gate-group Compose node's first child")]
    fn swapping_compose_children_is_mechanically_rejected_by_build_controllable() {
        let g = load(oracle_gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();

        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let swapped = swap_compose_children(&plan);

        // build_controllable enforces children[0] == LexiconFragment / children[1] == Replace
        // POSITIONALLY (build.rs's own `lexicon_fragment_entries`), so swapping them must panic
        // before any apply_up comparison is even attempted -- proof, not just argument, that no
        // sound Compose-child-reordering generator exists for this crate's only Compose shape.
        let _ = build_controllable(&swapped, &opts, &g, &alphabet, &ro, &budget);
    }

    #[test]
    #[should_panic(expected = "does not match the plan's Replace cascade at that position")]
    fn reversing_replace_cascade_is_mechanically_rejected_by_build_controllable() {
        let g = load(oracle_two_rule_cascade_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();

        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let reversed = reverse_replace_cascade(&plan);

        // build_controllable's own validate_replace_cascade cross-checks cascade.rules against
        // prules_in_order POSITIONALLY, so a reversed 2-rule cascade must panic -- proof, not just
        // argument, that no sound Replace-cascade-reordering generator exists.
        let _ = build_controllable(&reversed, &opts, &g, &alphabet, &ro, &budget);
    }

    // -----------------------------------------------------------------------------------------
    // Partition refinement: a THIRD sound Gate-node restructuring (cardinality, not order),
    // proven by the SAME two bars the module's other generators are proven by: apply-based
    // agreement with baseline (semantics-preserving), and a root NodeId that actually differs
    // (content-distinct -- otherwise `Registry::materialize_distinct` would dedup it away).
    // -----------------------------------------------------------------------------------------

    fn gate_group_count(plan: &Plan) -> usize {
        plan.iter()
            .find_map(|(_, kind)| match kind {
                PlanNodeKind::Gate { partition, .. } => Some(partition.groups.len()),
                _ => None,
            })
            .expect("plan must contain exactly one Gate node")
    }

    #[test]
    fn refine_gate_partition_bisect_and_fan_out_are_each_genuinely_different_plans() {
        let g = load(oracle_three_entry_ungated_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        let baseline = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let bisected = refine_gate_partition(&baseline, PartitionGranularity::Bisect);
        let fanned_out = refine_gate_partition(&baseline, PartitionGranularity::FanOut);

        assert_eq!(
            gate_group_count(&baseline),
            1,
            "fixture sanity: ungated grammar collapses to 1 group before refinement"
        );
        assert_eq!(
            gate_group_count(&bisected),
            2,
            "bisecting a 3-entry group must yield 2 sub-groups (sizes 2 and 1)"
        );
        assert_eq!(
            gate_group_count(&fanned_out),
            3,
            "fanning out a 3-entry group must yield 3 singleton sub-groups"
        );

        assert_ne!(
            baseline.root(),
            bisected.root(),
            "bisection must change the plan's root NodeId -- otherwise materialize_distinct would \
             dedup it straight back to baseline and nothing was actually added"
        );
        assert_ne!(
            baseline.root(),
            fanned_out.root(),
            "fan-out must change the plan's root NodeId -- same content-distinctness bar as above"
        );
        assert_ne!(
            bisected.root(),
            fanned_out.root(),
            "bisect and fan-out must be TWO DIFFERENT topologies (different group counts, 2 vs 3), \
             not the same restructuring wearing two names -- exactly the bar this task's own \
             instruction sets for 'genuinely distinct'"
        );
    }

    #[test]
    fn refine_gate_partition_agrees_with_baseline_by_apply_for_both_granularities() {
        let g = load(oracle_three_entry_ungated_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();

        let baseline = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        for (label, granularity) in [
            ("partition-bisect", PartitionGranularity::Bisect),
            ("partition-fan-out", PartitionGranularity::FanOut),
        ] {
            let refined = refine_gate_partition(&baseline, granularity);
            let result = differential_oracle(
                &baseline,
                &refined,
                ("enumerate_default", label),
                &opts,
                &g,
                &alphabet,
                &ro,
                &budget,
                &["p", "t", "k"],
            )
            .unwrap_or_else(|e| {
                panic!("both plans must build under an unbounded budget for {label}: {e:?}")
            });
            match result {
                OracleResult::Agree => {}
                OracleResult::Disagree {
                    word,
                    only_in_a,
                    only_in_b,
                    ..
                } => panic!(
                    "partition refinement ({label}) must not change the accepted relation -- got a \
                     real divergence at {word:?}: only_in_a={only_in_a:?}, only_in_b={only_in_b:?}. \
                     Per this task's own instruction, this must be reported as a genuine finding, \
                     never papered over."
                ),
            }
        }
    }

    #[test]
    fn refine_gate_partition_is_a_no_op_when_no_group_has_two_or_more_entries() {
        let g = load(oracle_union_root_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        let baseline = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let bisected = refine_gate_partition(&baseline, PartitionGranularity::Bisect);
        let fanned_out = refine_gate_partition(&baseline, PartitionGranularity::FanOut);

        assert_eq!(
            baseline.root(),
            bisected.root(),
            "a single-entry group has nothing to bisect -- this must be a true no-op (content- \
             identical, same NodeId), never forced churn on a fixture with nothing eligible"
        );
        assert_eq!(
            baseline.root(),
            fanned_out.root(),
            "a single-entry group is already maximally fanned out -- same true-no-op bar as above"
        );
    }

    // -----------------------------------------------------------------------------------------
    // Seeded random subtree mutation.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn mutate_plan_seeded_is_deterministic_for_the_same_seed() {
        let g = load(oracle_gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());

        let outcome_1 = mutate_plan_seeded(&plan, 42);
        let outcome_2 = mutate_plan_seeded(&plan, 42);

        assert_eq!(
            outcome_1.recipe, outcome_2.recipe,
            "the SAME seed against the SAME plan must draw the SAME recipe"
        );
        assert_eq!(
            outcome_1.plan.root(),
            outcome_2.plan.root(),
            "the SAME seed against the SAME plan must produce the SAME mutated plan"
        );
    }

    #[test]
    fn different_seeds_produce_different_topologies() {
        let g = load(oracle_gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());

        let roots: std::collections::BTreeSet<Option<NodeId>> = (0u64..30)
            .map(|seed| mutate_plan_seeded(&plan, seed).plan.root())
            .collect();

        assert!(
            roots.len() >= 2,
            "a genuinely random generator must produce more than one distinct topology across a \
             seed sweep -- a generator that always echoes its input (or always applies the \
             identical permutation) would fail this, which is exactly the non-vacuity failure mode \
             this test rules out; got {} distinct root(s)",
            roots.len()
        );
    }

    #[test]
    fn mutate_plan_seeded_exercises_the_gate_generator_and_agrees() {
        let g = load(oracle_gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();
        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());

        // This fixture has 2 eligible MutationTargets (its 2-group Gate, AND the root Union
        // wrapping it plus a composite-emission marker -- should_run is true here too, since ANY
        // phonological rule at all makes PhonologyProbe::new Some, and this fixture's one gated
        // subrule IS one). Scan seeds for one that actually draws the Gate, rather than assuming
        // any seed does.
        let mut exercised = false;
        for seed in 0u64..20 {
            let outcome = mutate_plan_seeded(&plan, seed);
            let recipe = outcome
                .recipe
                .clone()
                .expect("this fixture has eligible targets");
            if recipe.target_kind == "Gate" {
                exercised = true;
                let result = differential_oracle(
                    &plan,
                    &outcome.plan,
                    ("enumerate_default", "mutate_plan_seeded(gate permute)"),
                    &opts,
                    &g,
                    &alphabet,
                    &ro,
                    &budget,
                    &["p", "q"],
                )
                .expect("both plans must build successfully on this fixture");
                assert_eq!(
                    result,
                    OracleResult::Agree,
                    "a mutation drawn from the sound Gate-group generator must Agree with the \
                     original plan, got {result:?}"
                );
                break;
            }
        }
        assert!(
            exercised,
            "expected at least one seed in 0..20 to draw the Gate target -- if none did, \
             mutate_plan_seeded's Gate path was never actually exercised by this test"
        );
    }

    #[test]
    fn mutate_plan_seeded_exercises_the_union_generator_and_agrees() {
        let g = load(oracle_union_root_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();
        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());

        // This fixture's Gate node has only 1 group (ungated), so its only eligible
        // MutationTarget is the root Union (Gate + composite-emission marker) -- any seed must
        // draw the Union. Scan a few seeds for one that draws the NON-identity permutation (a
        // real, exercised swap), rather than asserting on a hand-picked seed reasoned about but
        // never run.
        let mut exercised = false;
        for seed in 0u64..20 {
            let outcome = mutate_plan_seeded(&plan, seed);
            let recipe = outcome
                .recipe
                .clone()
                .expect("this fixture has one eligible Union target");
            assert_eq!(recipe.target_kind, "Union");
            if outcome.plan.root() != plan.root() {
                exercised = true;
                let result = differential_oracle(
                    &plan,
                    &outcome.plan,
                    ("enumerate_default", "mutate_plan_seeded(union swap)"),
                    &opts,
                    &g,
                    &alphabet,
                    &ro,
                    &budget,
                    &["p"],
                )
                .expect("both plans must build successfully on this fixture");
                assert_eq!(
                    result,
                    OracleResult::Agree,
                    "a mutation drawn from the sound Union generator must Agree with the original \
                     plan, got {result:?}"
                );
                break;
            }
        }
        assert!(
            exercised,
            "expected at least one seed in 0..20 to draw the non-identity Union permutation -- if \
             none did, mutate_plan_seeded's Union path was never actually exercised by this test"
        );
    }

    // -----------------------------------------------------------------------------------------
    // Failure minimisation to a named recipe.
    // -----------------------------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "does not actually disagree")]
    fn minimize_disagreement_panics_when_the_full_sequence_agrees() {
        let g = load(oracle_gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();
        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());

        // Every step here is sound (mutate_plan_seeded draws), so the full chain must Agree --
        // minimize_disagreement must refuse to "minimise" a non-disagreement rather than silently
        // returning something.
        let steps = vec![MutationStep::from_seed(1), MutationStep::from_seed(2)];
        let _ = minimize_disagreement(
            &plan,
            steps,
            ("base", "mutated"),
            &opts,
            &g,
            &alphabet,
            &ro,
            &budget,
            &["p", "q"],
        );
    }

    #[test]
    fn minimization_converges_to_the_injected_breakage() {
        let g = load(oracle_gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();
        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());

        // Four sound, harmless mutation steps with the ONE real bug (drop_last_gate_group,
        // deliberately unsound -- the same breakage the oracle's own non-vacuity test uses)
        // spliced into the middle -- minimisation must discard every sound step and converge on
        // exactly the one that actually causes the disagreement.
        let steps = vec![
            MutationStep::from_seed(1),
            MutationStep::from_seed(2),
            breakage_step(),
            MutationStep::from_seed(3),
        ];

        let recipe = minimize_disagreement(
            &plan,
            steps,
            ("enumerate_default", "mutated"),
            &opts,
            &g,
            &alphabet,
            &ro,
            &budget,
            &["p", "q"],
        )
        .expect("both plans must build successfully on this fixture");

        assert_eq!(
            recipe.steps.len(),
            1,
            "minimisation must converge to a SINGLE step, the injected breakage -- got {:?}",
            recipe.steps
        );
        assert!(
            recipe.steps[0].contains("BREAKAGE"),
            "the surviving step must be the injected breakage, not one of the sound mutations -- \
             got {:?}",
            recipe.steps
        );
        assert_eq!(
            recipe.word, "q",
            "the dropped gate group is exactly what makes surface \"q\" analyzable, same as the \
             oracle's own non-vacuity test"
        );

        // Confirm the recipe actually reproduces: replaying ONLY the surviving step from the base
        // plan must still disagree, with the SAME witness word.
        let replayed = apply_step_chain(&plan, &[breakage_step()]);
        let replay_result = differential_oracle(
            &plan,
            &replayed,
            ("enumerate_default", "replayed breakage"),
            &opts,
            &g,
            &alphabet,
            &ro,
            &budget,
            &["p", "q"],
        )
        .expect("both plans must build successfully on this fixture");
        match replay_result {
            OracleResult::Disagree { word, .. } => assert_eq!(
                word, "q",
                "the replayed minimal recipe must reproduce the SAME witness word"
            ),
            OracleResult::Agree => {
                panic!("the minimised recipe must actually reproduce the disagreement")
            }
        }

        // Report the recipe's own Display format verbatim: a recipe a human can
        // act on -- this IS that format, printed here so a test failure or a
        // `--nocapture` run shows exactly what a human would paste into a bug report.
        println!("{recipe}");
    }
}
