//! Step 3a of `openspec/changes/reify-compilation-plans` (design.md D3): [`build_controllable`], a
//! [`crate::plan::Plan`] INTERPRETER -- the first piece of Step 3 that turns a reified `Plan` into a
//! real, live [`foma::types::Fsm`] rather than only describing one (Step 1, `crate::plan`; Step 2,
//! `crate::enumerate::enumerate_default`, which is purely data -- "no live `Fsm` is built anywhere
//! there", that module's own doc). This module walks exactly the node kinds
//! [`crate::enumerate::enumerate_default`] emits on the **controllable subtree** -- the [`crate::
//! plan::PlanNodeKind::Gate`] node and its per-group `Compose{LexiconFragment, Replace}` children --
//! and calls the SAME low-level primitives [`crate::gate::compile_gated_grammar_with_budget`] uses
//! ([`crate::uflexc::emit_underlying_filtered_with_budget`], [`crate::replace::
//! compile_and_compose_rules_gated_with_budget`], [`crate::compose_budget`]'s checked compose/union/
//! minimize wrappers). Neither `gate.rs`'s nor `replace.rs`'s bodies are touched -- this module only
//! calls their existing `pub` entry points (the task's own constraint).
//!
//! Proven equivalent to [`crate::gate::compile_gated_grammar_with_budget`]'s own direct-compile
//! output by an APPLY-based test (`equivalence_tests`, below) -- run real query words through BOTH
//! nets' `apply_up` and assert identical results, exactly the predicate a future differential oracle
//! (design.md D4) would use. This is a genuine correctness argument, not a structural-equality
//! shortcut: two networks can differ in shape (state numbering, arc order) and still be the *same
//! relation* modulo determinization/minimization choices, so `apply` is what actually matters here;
//! the module's own test additionally checks minimized state/arc counts as a cheap, meaningful
//! (not merely coincidental, given both paths run the same final `minimize_checked` on networks
//! built from the same primitives) extra signal -- but never in place of the apply comparison.
//!
//! # Scope: controllable subtree only (task's own scope call)
//! The composite-emission / structural-composite branches ([`crate::plan::FragmentSpec::
//! CompositeEmissionMarker`] / [`crate::plan::FragmentSpec::StructuralCompositeMarker`], the
//! black-box lexc `String` [`crate::emit::emit_with_budget`] produces) are OUT OF SCOPE for this
//! step: that path's artifact type is a lexc source string handed to a *separate* lexc-compile step,
//! not this module's own composed `Fsm` -- unifying the two artifact types into one interpreter
//! result is a later step's problem, not this one's. If `enumerate_default`'s plan root is a `Union`
//! carrying those markers alongside a `Gate` node (D2's own shape, `enumerate`'s module doc), this
//! module's [`build_controllable`] locates the single `Gate` child and interprets ONLY that subtree;
//! the marker leaves are checked for by kind (so a genuinely unrecognized Union child is a loud,
//! documented programmer-error panic, never a silent skip of something unexpected) but never built.
//!
//! # The obstacle this step surfaces (Step-3 design signal, per the task's own "if it doesn't fit")
//! `enumerate.rs`'s own module doc already flagged this as a "judgment call" at Step 2, and building
//! against it confirms it is a REAL interpretation obstacle, not a hypothetical one:
//!
//! **Every gate group's `Replace` subplan is the identical, content-addressed-SHARED [`crate::plan::
//! NodeId`]** (`enumerate_default`'s own module doc, and its own test
//! `gated_two_group_fixture_matches_real_partition_and_dedups_shared_replace` asserts this directly)
//! -- but the COMPILED `Fsm` that node must produce differs PER GROUP, because
//! [`crate::replace::compile_and_compose_rules_gated_with_budget`]'s `subrule_ok` callback (which
//! subrules of the shared rewrite cascade a given group's own gating key includes/excludes) is a
//! function of the *group*, not of the `Replace` node's own content (a [`crate::plan::
//! ReplaceCascadeSpec`] is rule-level only -- `Vec<PRuleId>` -- with no field for "which SUBRULE
//! within a rule this specific group's own network includes"; that distinction lives entirely in
//! [`crate::plan::GatePartitionSpec::gated_subrules`] + each group's own `key`, which are the GATE
//! node's data, not the Replace node's). A naive content-addressed interpreter that memoizes a built
//! `Fsm` per `NodeId` (the natural reading of D1's "measured once, stored once" dedup claim) would
//! therefore build the shared `Replace` node's cascade ONCE, using whichever group's `subrule_ok` ran
//! first, and silently reuse that WRONG network for every other group -- an unsound, silent
//! correctness bug, not a missing feature.
//!
//! **Resolution taken here** (matches `enumerate.rs`'s own suggested fix: "resolve it by re-deriving
//! the group's key at build time"): [`build_controllable`] does NOT do generic NodeId-memoized
//! interpretation. It is Gate-aware: for each group, it reads that group's own
//! [`crate::plan::GateGroupSpec::key`] from the `Gate` node's `partition` (not from the `Replace`
//! node at all) and threads it into a freshly-built `subrule_ok` closure for THAT group's own call to
//! `compile_and_compose_rules_gated_with_budget` -- so the shared `Replace` `NodeId` is walked/
//! validated once per group (its `cascade.rules`/child-leaf shape is read and cross-checked against
//! `prules_in_order` every time, cheap) but never has its COMPILED `Fsm` cached/reused across groups.
//! This closes the gap for `build()` v1, but it means [`build_controllable`] is not a fully generic
//! "interpret any DAG shape uniformly" walker -- it specifically special-cases `Gate`'s children,
//! which is exactly the Step-3 design signal the task asked to surface: **a future, more general
//! interpreter (or a richer `GatePartitionSpec`/`ReplaceCascadeSpec` that pushes the per-group subrule
//! detail down onto the `Replace` node itself, e.g. one `Replace` variant per group instead of one
//! shared node) would need to resolve this before Gate could be treated as "just another n-ary
//! Compose over shared children" the way `Union`/plain `Compose` already can be.**
//!
//! # Node kinds handled (exactly what `enumerate_default` emits on the controllable path)
//! - [`crate::plan::PlanNodeKind::Gate`] -- the entry point; see the obstacle note above.
//! - [`crate::plan::PlanNodeKind::Compose`] -- each gate group's child; only
//!   [`crate::plan::ComposeStrategy::Static`] is interpreted (the only strategy `enumerate_default`
//!   ever emits) -- `Lazy`/`LazyLookahead` panic with a precise message rather than silently
//!   compiling eagerly, since no lazy-composition primitive exists anywhere in this crate yet (a
//!   real, separate Plan-model/interpreter gap, not this step's to close).
//! - [`crate::plan::PlanNodeKind::Leaf`] tagged [`crate::plan::FragmentSpec::LexiconFragment`] --
//!   read as `entries` for [`crate::uflexc::emit_underlying_filtered_with_budget`]'s own
//!   `allowed_entries` parameter (always `Some`, matching `enumerate_default`'s own invariant).
//! - [`crate::plan::PlanNodeKind::Replace`] and its [`crate::plan::FragmentSpec::RewriteRule`] Leaf
//!   children -- read and cross-validated against the `prules_in_order` slice the caller supplies
//!   (see [`validate_replace_cascade`]'s own doc for why this check exists and what it catches).
//!
//! # Visibility widened
//! [`crate::enumerate::rule_id_of`] was widened from private to `pub(crate)` so this module can reuse
//! its pointer-identity `PRuleId` recovery rather than re-deriving the same safety-relevant logic a
//! second time (see that function's own doc for why the pointer-identity approach is sound). No other
//! visibility change was needed -- every other primitive this module calls
//! ([`crate::uflexc::emit_underlying_filtered_with_budget`], [`crate::replace::
//! compile_and_compose_rules_gated_with_budget`], [`crate::compose_budget`]'s checked wrappers,
//! [`crate::gate::GatedCompileResult`]) was already `pub`/`pub(crate)`.

use std::collections::HashSet;

use foma::options::FomaOptions;
use foma::types::Fsm;

use pg_grammar::model::{Grammar, LexEntryId, PhonRuleDef};

use crate::compose_budget::{
    compose_checked, minimize_checked, union_checked, ComposeBudget, ComposeError,
};
use crate::enumerate::rule_id_of;
use crate::gate::GatedCompileResult;
use crate::plan::{
    ComposeStrategy, FragmentSpec, GatePartitionSpec, NodeId, Plan, PlanNodeKind,
};
use crate::replace::{compile_and_compose_rules_gated_with_budget, SegAlphabet, TupleReport};
use crate::uflexc::{emit_underlying_filtered_with_budget, UEmitReport};

/// Interprets `plan`'s controllable subtree (module doc) into a real, composed `Fsm` -- the plan-walk
/// counterpart of [`crate::gate::compile_gated_grammar_with_budget`]. This function does not call
/// into `gate.rs` at all (it never re-derives the partition itself); it calls the same public/
/// `pub(crate)` low-level primitives that function itself uses. `g`/`alphabet`/`prules_in_order`/
/// `budget` are the SAME inputs
/// [`crate::enumerate::enumerate_default`] (which built `plan`) and
/// [`crate::gate::compile_gated_grammar_with_budget`] both take -- `build_controllable` does not
/// recompute grammar-derived facts `enumerate_default` already baked into `plan` (it never calls
/// `crate::gate::find_gated_subrules`/`partition_entries` itself), it only reads them back out of the
/// plan's own nodes.
///
/// # Panics
/// On any plan shape [`crate::enumerate::enumerate_default`] does not itself produce (a dangling
/// `NodeId`, a `Gate` node missing from the root/root-`Union`, a group's `Compose` node with the wrong
/// child count or a non-`Static` strategy, a `Replace` cascade that doesn't match `prules_in_order`) --
/// these are caller/plan-construction contract violations, not runtime/budget failures, so they panic
/// loudly rather than returning a `ComposeError` variant that doesn't exist for them (mirrors this
/// crate's existing convention, e.g. `crate::gate::compile_gated_grammar_with_budget`'s own
/// `unwrap_or_else(|| panic!(...))` on a lexc-compile failure, and `crate::enumerate::rule_id_of`'s own
/// panic on a caller-supplied slice not borrowed from `g.prules`).
///
/// # Errors
/// Only for the same reasons [`crate::gate::compile_gated_grammar_with_budget`] itself returns
/// `Err` -- a [`ComposeBudget`] cap tripping on the emit/compose/union/minimize primitives this
/// function calls (no NEW budget vector is introduced here; the group-count budget check (V6) that
/// `compile_gated_grammar_with_budget` runs BEFORE any per-group work is not re-run here, since
/// `plan.partition.groups.len()` was already checked at `enumerate_default` build time by that same
/// mechanism if the caller built `plan` through the production path -- `build_controllable` trusts the
/// plan it is handed, per this function's own doc above, rather than re-deriving facts already baked
/// into it).
pub fn build_controllable(
    plan: &Plan,
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    budget: &ComposeBudget,
) -> Result<GatedCompileResult, ComposeError> {
    let gate_id = find_gate_node(plan);
    let PlanNodeKind::Gate { partition, children } = plan
        .get(gate_id)
        .unwrap_or_else(|| panic!("find_gate_node returned a NodeId {gate_id} not interned in plan"))
    else {
        unreachable!("find_gate_node only ever returns the id of a Gate node")
    };
    assert_eq!(
        partition.groups.len(),
        children.len(),
        "Gate node invariant (see Plan::add_node's own debug_assert): one child per partition group"
    );

    let mut final_net: Option<Fsm> = None;
    let mut skipped_rules: Vec<String> = Vec::new();
    let mut skipped_allomorphs: Vec<String> = Vec::new();
    let mut tuple_reports: Vec<(String, Vec<TupleReport>)> = Vec::new();
    let mut group_reports = Vec::new();

    for (group_idx, &compose_id) in children.iter().enumerate() {
        let group_key = &partition.groups[group_idx].key;
        let (lexicon_id, replace_id) = gate_group_children(plan, compose_id);

        let entries = lexicon_fragment_entries(plan, lexicon_id);
        let entries_set: HashSet<LexEntryId> = entries.iter().copied().collect();

        // Walks the shared Replace node's OWN data (cascade + rule-leaf children) and cross-checks
        // it against `prules_in_order` -- see this function's own doc, module doc's "obstacle" note.
        validate_replace_cascade(plan, replace_id, g, prules_in_order);

        let UEmitReport {
            lexc_source,
            skipped: uskipped,
            root_entries,
            prefix_entries,
            suffix_entries,
            ..
        } = emit_underlying_filtered_with_budget(g, alphabet, Some(&entries_set), budget)?;
        skipped_allomorphs.extend(uskipped);
        group_reports.push((group_key.clone(), root_entries, prefix_entries, suffix_entries));

        if root_entries == 0 {
            // Mirrors `compile_gated_grammar_with_budget`'s own doc: an empty group (a gating key
            // combination realized by zero entries) contributes nothing.
            continue;
        }

        let lexc_net = foma::lexcread::fsm_lexc_parse_string(opts, None, &lexc_source)
            .unwrap_or_else(|| panic!("gated group lexc failed to compile:\n{lexc_source}"));

        // The module doc's "obstacle": this group's own gating key, read from the GATE node's
        // partition (never from the shared Replace node), threaded fresh into a per-group
        // subrule_ok closure -- NOT a cached/reused compile of the shared Replace NodeId.
        let subrule_ok = subrule_ok_for_group(partition, group_key);

        let mut group_skipped_rules = Vec::new();
        let rules_net = compile_and_compose_rules_gated_with_budget(
            opts,
            g,
            alphabet,
            prules_in_order,
            &subrule_ok,
            &mut group_skipped_rules,
            &mut tuple_reports,
            budget,
        )?;
        for s in group_skipped_rules {
            if !skipped_rules.contains(&s) {
                skipped_rules.push(s);
            }
        }

        let group_net = match rules_net {
            Some(rules) => compose_checked(
                opts,
                lexc_net,
                rules,
                budget,
                "build_controllable lexc.o.rules",
            )?,
            None => lexc_net,
        };
        final_net = Some(match final_net {
            None => group_net,
            // Safe union: groups are lexically disjoint -- same argument as `crate::gate`'s own
            // module doc ("why the union is safe here"), unchanged by walking a plan instead of
            // recomputing the partition directly.
            Some(prev) => union_checked(
                opts,
                prev,
                group_net,
                budget,
                "build_controllable group union fold",
            )?,
        });
    }

    let final_net = match final_net {
        Some(net) => Some(minimize_checked(
            opts,
            net,
            budget,
            "build_controllable final minimize",
        )?),
        None => None,
    };

    Ok(GatedCompileResult {
        net: final_net,
        groups: partition.groups.len(),
        skipped_rules,
        skipped_allomorphs,
        tuple_reports,
        group_reports,
    })
}

/// Locates the single `Gate` node this function will interpret: `plan`'s root itself if it IS a
/// `Gate`, or -- when `enumerate_default` wrapped the root in a `Union` alongside composite/
/// structural marker leaves (D2's own shape) -- the one `Gate` child of that `Union`. Every OTHER
/// `Union` child is checked by kind: a [`FragmentSpec::CompositeEmissionMarker`]/
/// [`FragmentSpec::StructuralCompositeMarker`] leaf is the documented out-of-scope case (module
/// doc) and is silently skipped (never built); anything else is a plan shape this module does not
/// recognize and panics loudly rather than guessing.
fn find_gate_node(plan: &Plan) -> NodeId {
    let root = plan
        .root()
        .expect("build_controllable requires a Plan with a root set");
    match plan
        .get(root)
        .unwrap_or_else(|| panic!("plan root NodeId {root} is not interned in this Plan"))
    {
        PlanNodeKind::Gate { .. } => root,
        PlanNodeKind::Union { children } => {
            let mut gate_ids: Vec<NodeId> = Vec::new();
            for &child in children {
                match plan
                    .get(child)
                    .unwrap_or_else(|| panic!("dangling Union child NodeId {child}"))
                {
                    PlanNodeKind::Gate { .. } => gate_ids.push(child),
                    PlanNodeKind::Leaf { fragment, .. } => match fragment {
                        FragmentSpec::CompositeEmissionMarker
                        | FragmentSpec::StructuralCompositeMarker => {
                            // Out of scope for build_controllable v1 (module doc): these two
                            // markers resolve via a completely separate code path
                            // (`emit::emit_with_budget`) into a lexc `String`, not an `Fsm` this
                            // interpreter builds. Checked-for by kind and skipped, never silently
                            // misinterpreted as something buildable.
                        }
                        other => panic!(
                            "unexpected Union-root Leaf fragment for build_controllable: {other:?} \
                             (enumerate_default only ever places CompositeEmissionMarker/\
                             StructuralCompositeMarker leaves alongside the Gate node at the root)"
                        ),
                    },
                    other => panic!(
                        "unexpected Union-root child kind for build_controllable: {} \
                         (enumerate_default's root Union only ever contains a Gate node plus marker \
                         leaves)",
                        other.kind_name()
                    ),
                }
            }
            match gate_ids.len() {
                1 => gate_ids[0],
                0 => panic!(
                    "plan root Union carries no Gate node -- build_controllable has nothing to \
                     interpret (a composite/structural-marker-only plan is out of scope for build() \
                     v1, see this module's own doc)"
                ),
                _ => panic!(
                    "plan root Union carries more than one Gate node -- not a shape \
                     enumerate_default produces"
                ),
            }
        }
        other => panic!(
            "build_controllable expects a Gate node (optionally wrapped in a root Union alongside \
             composite/structural marker leaves) at the plan root, got {}",
            other.kind_name()
        ),
    }
}

/// One gate group's `Compose` node, resolved to its two children `(lexicon_leaf, replace_node)` --
/// `enumerate_default`'s own shape (module doc: "each group's Compose = Compose[ group's
/// LexiconFragment Leaf ..., the shared Replace node ]"). Panics on any other strategy/child-count
/// shape (module doc's "node kinds handled" list).
fn gate_group_children(plan: &Plan, compose_id: NodeId) -> (NodeId, NodeId) {
    let PlanNodeKind::Compose { children, strategy } = plan
        .get(compose_id)
        .unwrap_or_else(|| panic!("dangling Compose NodeId {compose_id} in plan"))
    else {
        panic!("expected a Compose node as a Gate group's child at {compose_id}");
    };
    assert!(
        matches!(strategy, ComposeStrategy::Static),
        "build_controllable only interprets ComposeStrategy::Static (the only strategy \
         enumerate_default ever emits); got {strategy:?} at node {compose_id} -- no lazy-composition \
         primitive exists anywhere in this crate yet, so this is a real Plan-model/interpreter gap \
         (a genuine Step-3 finding), not something safely ignorable"
    );
    assert_eq!(
        children.len(),
        2,
        "a gate-group Compose node must have exactly 2 children (LexiconFragment leaf, shared \
         Replace node) -- enumerate_default's own shape, got {} at {compose_id}",
        children.len()
    );
    (children[0], children[1])
}

/// A gate group's `LexiconFragment` leaf, resolved to its `entries` list. Panics if the leaf isn't a
/// `LexiconFragment` or if `entries` is `None` -- `enumerate_default`'s own invariant is that a
/// gate-group lexicon leaf is ALWAYS `Some(sorted group entries)`, never `None` (that module's own
/// doc, "Per-group `LexiconFragment.entries` is always `Some(...)`").
fn lexicon_fragment_entries(plan: &Plan, lexicon_id: NodeId) -> Vec<LexEntryId> {
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
            "build_controllable requires Some(entries) on every gate-group LexiconFragment leaf \
             (enumerate_default's own invariant, see that module's doc); got None at {lexicon_id}"
        )
    })
}

/// Reads a gate group's `Replace` node's own `cascade`/rule-leaf children and cross-validates them
/// against `prules_in_order` -- the caller-supplied slice `build_controllable` actually compiles
/// with. This is not redundant bookkeeping: it is the one place this function proves the
/// `prules_in_order` slice the CALLER passed to `build_controllable` is the SAME slice (same
/// `PRuleId`s, same order) `enumerate_default` used to build `plan` in the first place -- a mismatch
/// here means the caller handed `build_controllable` a plan and a rule slice that don't agree, which
/// would otherwise silently miscompile every group's rewrite cascade (the `subrule_ok` closure's
/// `rule_pos` indices are positions into `prules_in_order`, so a reordered/different slice changes
/// which subrules a group's key gates without any other signal). Panics loudly on any mismatch,
/// mirroring `crate::enumerate::rule_id_of`'s own panic for the identical caller-contract shape.
fn validate_replace_cascade(
    plan: &Plan,
    replace_id: NodeId,
    g: &Grammar,
    prules_in_order: &[&PhonRuleDef],
) {
    let PlanNodeKind::Replace { cascade, children } = plan
        .get(replace_id)
        .unwrap_or_else(|| panic!("dangling Replace NodeId {replace_id}"))
    else {
        panic!("expected a Replace node as a gate-group Compose node's second child at {replace_id}");
    };
    assert_eq!(
        cascade.rules.len(),
        children.len(),
        "Replace node invariant: one RewriteRule Leaf child per cascade rule"
    );
    assert_eq!(
        cascade.rules.len(),
        prules_in_order.len(),
        "build_controllable's prules_in_order slice (len {}) does not match the plan's own Replace \
         cascade (len {}) -- the caller passed a slice this plan was not built from",
        prules_in_order.len(),
        cascade.rules.len()
    );
    for (i, &rule_id) in cascade.rules.iter().enumerate() {
        let expected = rule_id_of(g, prules_in_order[i]);
        assert_eq!(
            rule_id, expected,
            "build_controllable's prules_in_order[{i}] does not match the plan's Replace cascade at \
             that position -- the caller passed a slice this plan was not built from"
        );
        let PlanNodeKind::Leaf { fragment, .. } = plan.get(children[i]).unwrap_or_else(|| {
            panic!("dangling RewriteRule Leaf NodeId {} (Replace child {i})", children[i])
        }) else {
            panic!("expected a Leaf node as Replace child {i}");
        };
        let FragmentSpec::RewriteRule { rule } = fragment else {
            panic!("expected FragmentSpec::RewriteRule on Replace child {i}, got {fragment:?}");
        };
        assert_eq!(
            *rule, rule_id,
            "Replace node's RewriteRule Leaf child {i} must carry the same PRuleId as \
             cascade.rules[{i}]"
        );
    }
}

/// Builds one group's `subrule_ok(rule_pos, sub_idx)` predicate from the GATE node's own
/// `partition.gated_subrules` + that group's own `key` -- IDENTICAL shape to `crate::gate::
/// compile_gated_grammar_with_budget`'s own inline closure (that function's body, the `subrule_ok`
/// local), just reading its inputs back out of `plan` data instead of `crate::gate::EntryGroup`/
/// `crate::gate::GatedSubrule`. See the module doc's "obstacle" note for why this MUST be
/// re-derived fresh per group rather than read off (or cached against) the shared `Replace` node.
fn subrule_ok_for_group<'a>(
    partition: &'a GatePartitionSpec,
    group_key: &'a [bool],
) -> impl Fn(usize, usize) -> bool + 'a {
    move |rule_pos: usize, sub_idx: usize| -> bool {
        match partition
            .gated_subrules
            .iter()
            .position(|gs| gs.rule_pos == rule_pos && gs.sub_idx == sub_idx)
        {
            None => true, // ungated subrule: always included, matches crate::gate's own convention.
            Some(gate_index) => group_key[gate_index],
        }
    }
}

#[cfg(test)]
mod equivalence_tests {
    //! The correctness argument for Step 3a (the task's own instruction: "make it semantically
    //! meaningful, not trivial"). For an in-crate gated synthetic fixture, builds BOTH (a)
    //! `compile_gated_grammar_with_budget` (today's direct-compile path) and (b)
    //! `build_controllable(enumerate_default(...))` (this module's plan-walk), then asserts the two
    //! resulting networks are EQUIVALENT BY APPLY -- `apply_up` on every distinguishing query word
    //! must yield IDENTICAL result sets. This is exactly the predicate a future differential oracle
    //! (design.md D4) would use; the module doc explains why it -- not a structural/byte-identity
    //! claim -- is the one that matters. Minimized state/arc counts are ALSO asserted equal, as a
    //! cheap and (here) meaningful extra signal, never a substitute for the apply comparison.

    use std::collections::HashSet;

    use foma::apply::apply_init;
    use foma::options::FomaOptions;
    use foma::types::Fsm;

    use pg_grammar::model::{Grammar, PhonRuleDef};

    use super::*;
    use crate::compose_budget::ComposeBudget;
    use crate::enumerate::enumerate_default;
    use crate::gate::compile_gated_grammar_with_budget;
    use crate::junctions::PhonologyProbe;

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

    /// One MPR-gated subrule (`requiredMPRFeatures="mpr1"`, `c1 -> c2`, no environment) and two
    /// entries realizing both truth values of that gate key -- the SAME shape as `enumerate.rs`'s
    /// own `gated_two_group_fixture` (private to that module's own `#[cfg(test)]` block, so
    /// duplicated here rather than exposed across a test-module boundary; both are synthetic,
    /// self-contained, and delanguaged per this repo's own conformance-grammar convention). `e0`
    /// (no `ruleFeatures`) realizes gate key `[false]` (the subrule does not apply, its underlying
    /// "p" stays "p" on the surface); `e1` (`ruleFeatures="mpr1"`) realizes `[true]` (the subrule
    /// fires, "p" surfaces as "q") -- so "p" and "q" are the two words that can only ever be
    /// analyzed by exactly one of the two gate groups, the property this test's apply comparison
    /// needs.
    fn gated_two_group_fixture_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>BuildControllableGatedTwoGroupFixture</Name>
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

    /// Every raw string `apply_up` yields for `word` against `net` (encoded via
    /// `alphabet.encode_query`, module doc's token-space convention) -- the full literal upper-tape
    /// output set, not a decoded/collapsed projection of it, so this comparison is at least as
    /// strict as the decoded-candidate comparisons `tests/p6_gate_parity.rs` itself uses.
    fn apply_up_results(net: &Fsm, alphabet: &SegAlphabet, word: &str) -> HashSet<String> {
        let mut out = HashSet::new();
        let Some(query) = alphabet.encode_query(word) else {
            return out;
        };
        let mut h = apply_init(net);
        for s in h.up(&query) {
            out.insert(s);
        }
        out
    }

    #[test]
    fn plan_walk_matches_direct_compile_by_apply_on_gated_two_group_fixture() {
        let g = load(gated_two_group_fixture_xml());
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let budget = ComposeBudget::unbounded();

        // (a) today's direct-compile path -- unmodified, exactly what `crate::gate`'s own tests
        // call.
        let direct = compile_gated_grammar_with_budget(&opts, &g, &alphabet, &ro, &budget)
            .expect("direct compile must succeed");
        let direct_net = direct
            .net
            .clone()
            .expect("direct compile must produce a non-empty net");

        // (b) the plan-walk this module ships.
        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let built = build_controllable(&plan, &opts, &g, &alphabet, &ro, &budget)
            .expect("plan-walk build must succeed");
        let built_net = built
            .net
            .clone()
            .expect("plan-walk build must produce a non-empty net");

        assert_eq!(
            direct.groups, built.groups,
            "direct-compile and plan-walk must agree on group count"
        );
        assert_eq!(
            direct.groups, 2,
            "fixture sanity: exactly 2 gating groups expected"
        );

        // Structural sanity (module doc: meaningful here, never a substitute for the apply
        // comparison below) -- both paths run the SAME final `minimize_checked` on networks built
        // from the same primitives, so a divergence here would itself be a real finding.
        assert_eq!(
            direct_net.statecount, built_net.statecount,
            "minimized state counts must match between direct compile and plan-walk build"
        );
        assert_eq!(
            direct_net.arccount, built_net.arccount,
            "minimized arc counts must match between direct compile and plan-walk build"
        );

        // The correctness argument itself: apply_up on every distinguishing query word must be
        // IDENTICAL between the two nets.
        for word in ["p", "q"] {
            let want = apply_up_results(&direct_net, &alphabet, word);
            let got = apply_up_results(&built_net, &alphabet, word);
            assert!(
                !want.is_empty(),
                "sanity: {word:?} must actually analyze on the direct-compile net"
            );
            assert_eq!(
                got, want,
                "apply_up results for {word:?} must match EXACTLY between direct compile and \
                 plan-walk build (want from direct compile, got from build_controllable)"
            );
        }

        // A stronger sanity check than "both nonempty": "p" and "q" must resolve to DIFFERENT
        // results on each net (proving the gate actually distinguishes the two groups on THIS
        // fixture, not that both words happen to hit the same over-permissive branch).
        assert_ne!(
            apply_up_results(&direct_net, &alphabet, "p"),
            apply_up_results(&direct_net, &alphabet, "q"),
            "fixture sanity: \"p\" and \"q\" must resolve to different analyses on the direct-compile \
             net (otherwise the gate isn't actually being exercised)"
        );
    }
}
