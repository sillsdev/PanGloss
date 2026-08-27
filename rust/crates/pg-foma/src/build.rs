//! `build_controllable`, a
//! `crate::plan::Plan` INTERPRETER -- turns a reified `Plan` into a
//! real, live `foma::types::Fsm` rather than only describing one (`crate::plan`;
//! `crate::enumerate::enumerate_default`, which is purely data -- "no live `Fsm` is built anywhere
//! there", that module's own doc). This module walks exactly the node kinds
//! `crate::enumerate::enumerate_default` emits on the **controllable subtree** -- the [`crate::
//! plan::PlanNodeKind::Gate`] node and its per-group `Compose{LexiconFragment, Replace}` children --
//! and calls the SAME low-level primitives `crate::gate::compile_gated_grammar` uses
//! (`crate::uflexc::emit_underlying_filtered`, [`crate::replace::
//! compile_and_compose_rules_gated`], and direct foma compose/union/minimize calls). The gate and
//! replacement entry points own their unsupported-rule handling; this module only calls their
//! public APIs.
//!
//! Proven equivalent to `crate::gate::compile_gated_grammar`'s own direct-compile
//! output by an APPLY-based test (`equivalence_tests`, below) -- run real query words through BOTH
//! nets' `apply_up` and assert identical results, exactly the predicate a future differential oracle
//! would use. This is a genuine correctness argument, not a structural-equality
//! shortcut: two networks can differ in shape (state numbering, arc order) and still be the *same
//! relation* modulo determinization/minimization choices, so `apply` is what actually matters here;
//! the module's own test additionally checks minimized state/arc counts as a cheap, meaningful
//! (not merely coincidental, given both paths run the same final `fsm_minimize` on networks
//! built from the same primitives) extra signal -- but never in place of the apply comparison.
//!
//! # Scope: controllable subtree only
//! The composite-emission / structural-composite branches ([`crate::plan::FragmentSpec::
//! CompositeEmissionMarker`] / `crate::plan::FragmentSpec::StructuralCompositeMarker`, the
//! black-box lexc `String` `crate::emit::emit_with_budget` produces) are OUT OF SCOPE here:
//! that path's artifact type is a lexc source string handed to a *separate* lexc-compile step,
//! not this module's own composed `Fsm` -- unifying the two artifact types into one interpreter
//! result is a later problem, not this module's. If `enumerate_default`'s plan root is a `Union`
//! carrying those markers alongside a `Gate` node (`enumerate`'s own module doc has the shape), this
//! module's `build_controllable` locates the single `Gate` child and interprets ONLY that subtree;
//! the marker leaves are checked for by kind (so a genuinely unrecognized Union child is a loud,
//! documented programmer-error panic, never a silent skip of something unexpected) but never built.
//!
//! # A soundness obstacle, and how it was closed
//! An earlier version of this module built ONE shared `Replace` subplan per grammar, so every
//! gate group's `Replace` subplan was the identical, content-addressed-SHARED [`crate::plan::
//! NodeId`], yet the COMPILED `Fsm` that node had to produce differed PER GROUP, because
//! `crate::replace::compile_and_compose_rules_gated`'s `subrule_ok` callback is a
//! function of the *group*, not of the `Replace` node's own content. A naive content-addressed
//! interpreter that memoizes a built `Fsm` per `NodeId` would therefore have built the shared
//! `Replace` node's cascade ONCE and silently reused that WRONG network for every other group -- an
//! unsound, silent correctness bug, not a missing feature. That earlier version of
//! `build_controllable` sidestepped this by being Gate-aware (re-deriving each group's
//! `subrule_ok` from the `Gate` node's own `partition`, never caching a compiled `Fsm` against the
//! shared `Replace` `NodeId`), which was correct but kept `Gate` from being "just another n-ary
//! node."
//!
//! **The fix** (`crate::plan::ReplaceCascadeSpec`'s own doc, `crate::enumerate::
//! enumerate_default`'s own module doc): `enumerate_default` now builds ONE `Replace` node PER
//! GROUP, and that node's own `cascade` carries `gated_subrules` + `group_key` directly -- so a
//! group's `subrule_ok` is now fully determined by its OWN `Replace` node's content, not by which
//! `Gate` group happens to reference it. `build_controllable` below reflects this: it derives
//! `subrule_ok` by reading the per-group `Replace` node's own `cascade.gated_subrules`/
//! `cascade.group_key` (see `subrule_ok_for_group`), NOT by re-deriving it from the `Gate` node's
//! partition. The `Gate`-node walk itself is unchanged (this module still locates each group's own
//! `Compose`/`Replace` subtree by walking the `Gate` node's `children`, and still cross-checks
//! `partition.groups[group_idx].key` against the Replace node's own `group_key` as a redundant
//! sanity check -- see the loop in `build_controllable`), but **correctness no longer depends on
//! Gate-awareness of the Replace node**: `Replace`'s compiled artifact is now a pure function of its
//! own `NodeId`, exactly what a soundness invariant requires for content-addressed dedup / a
//! future `NodeId`-keyed plan-cache / the differential oracle (`crate::oracle`) to memoize safely.
//! This module does not build a generic memoizing interpreter -- that remains future work -- it only
//! removes the soundness caveat that would have made one unsound.
//!
//! # Node kinds handled (exactly what `enumerate_default` emits on the controllable path)
//! - `crate::plan::PlanNodeKind::Gate` -- the entry point; see the obstacle note above.
//! - `crate::plan::PlanNodeKind::Compose` -- each gate group's child;
//!   `crate::plan::ComposeStrategy` has only the `Static` variant, so this step has nothing else
//!   to reject and no strategy guard remains.
//! - `crate::plan::PlanNodeKind::Leaf` tagged `crate::plan::FragmentSpec::LexiconFragment` --
//!   read as `entries` for `crate::uflexc::emit_underlying_filtered`'s own
//!   `allowed_entries` parameter (always `Some`, matching `enumerate_default`'s own invariant).
//! - `crate::plan::PlanNodeKind::Replace` and its `crate::plan::FragmentSpec::RewriteRule` Leaf
//!   children -- read and cross-validated against the `prules_in_order` slice the caller supplies
//!   (see `validate_replace_cascade`'s own doc for why this check exists and what it catches).
//!
//! # Visibility widened
//! `crate::enumerate::rule_id_of` was widened from private to `pub(crate)` so this module can reuse
//! its pointer-identity `PRuleId` recovery rather than re-deriving the same safety-relevant logic a
//! second time (see that function's own doc for why the pointer-identity approach is sound). No other
//! visibility change was needed -- every other primitive this module calls
//! (`crate::uflexc::emit_underlying_filtered`, [`crate::replace::
//! compile_and_compose_rules_gated`], and direct foma composition,
//! `crate::gate::GatedCompileResult`) was already `pub`/`pub(crate)`.

use std::collections::HashSet;

use foma::constructions::{fsm_compose, fsm_union};
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::types::Fsm;

use pg_grammar::model::{Grammar, LexEntryId, PhonRuleDef};

use crate::compose_budget::ComposeError;
use crate::enumerate::rule_id_of;
use crate::gate::GatedCompileResult;
use crate::plan::{FragmentSpec, GatedSubruleRef, NodeId, Plan, PlanNodeKind, ReplaceCascadeSpec};
use crate::replace::{compile_and_compose_rules_gated, SegAlphabet, TupleReport};
use crate::uflexc::{emit_underlying_filtered, UEmitReport};

/// The two marker fragments `crate::enumerate::enumerate_default` places alongside the `Gate` node
/// when a grammar's recall depends on the composite-emission / structural-composite subtrees --
/// exactly the leaves `find_gate_node` skips (module doc, "Scope: controllable subtree only").
///
/// A caller that treats `build_controllable`'s net as if it represented the WHOLE grammar must
/// consult this first. On a grammar whose plan carries either marker, the controllable-only net omits
/// the material those subtrees contribute, and the omission is quiet: the net is smaller but
/// perfectly well-formed and `build_controllable` returns `Ok`. Measured on a
/// templated real grammar, the controllable-only net was 135 states / 3309 arcs against the tuned
/// `crate::emit`-based path's 6376 states / 68693 arcs for the same grammar -- a 47x state deficit
/// that proposed nothing for 19 of 20 corpus words while the tuned net proposed correctly.
///
/// Returns the markers present, in plan iteration order, empty when the plan is fully within
/// `build_controllable`'s scope.
pub fn unbuildable_markers(plan: &Plan) -> Vec<FragmentSpec> {
    let mut found = Vec::new();
    for (_, kind) in plan.iter() {
        if let PlanNodeKind::Leaf { fragment, .. } = kind {
            if matches!(
                fragment,
                FragmentSpec::CompositeEmissionMarker | FragmentSpec::StructuralCompositeMarker
            ) && !found.contains(fragment)
            {
                found.push(fragment.clone());
            }
        }
    }
    found
}

/// Every token character standing for a `Boundary`-kind char-def in `table` -- the shared
/// collection `boundary_cleanup_net` (which deletes every one of them, unconditionally),
/// `reroute_null_shaped_affix_chains` (which needs to recognize when a lexc line's ENTIRE
/// underlying text is drawn only from this set, i.e. is about to be deleted down to nothing) and
/// `crate::uflexc::emit_underlying_filtered` (which needs the SAME "will be deleted to
/// nothing" test at EMISSION time, to keep a null-shaped line off a self-looping continuation by
/// construction -- see that module's own "Null-shaped affixes are at most once per juncture"
/// section) must agree on. Kept as one function so the three can never drift on which char-defs
/// "boundary" means here; `pub(crate)` only so uflexc can share it rather than re-deriving it.
pub(crate) fn boundary_tokens(
    table: &pg_grammar::chardef::CharDefTable,
    alphabet: &SegAlphabet,
) -> Vec<char> {
    table
        .iter()
        .filter(|(_, cd)| cd.kind() == pg_grammar::chardef::CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect()
}

/// Deletes every `Boundary` char-def unconditionally -- excluding any subset would leave entries containing it impossible for any surface query to match. `None` when `table` declares none.
fn boundary_cleanup_net(
    opts: &FomaOptions,
    table: &pg_grammar::chardef::CharDefTable,
    alphabet: &SegAlphabet,
) -> Option<Fsm> {
    let tokens = boundary_tokens(table, alphabet);
    if tokens.is_empty() {
        return None;
    }
    let cleanup_regex = tokens
        .iter()
        .map(|c| format!("{c} -> 0"))
        .collect::<Vec<_>>()
        .join(", ");
    foma::regex::fsm_parse_regex(opts, &cleanup_regex, None, None)
}

/// Reroutes null-shaped (fully-boundary) `uflexc` affix lines off the self-looping prefix/suffix chain, closing an epsilon-cycle proposal explosion; name-scoped to `PrefixChain`/`SuffixChain` only.
/// Exact failure mode, rejected alternatives, and mechanics: docs/research/pg-foma-build-design-notes.md.
fn reroute_null_shaped_affix_chains(
    lexc_source: &str,
    table: &pg_grammar::chardef::CharDefTable,
    alphabet: &SegAlphabet,
) -> String {
    let boundary_set: HashSet<char> = boundary_tokens(table, alphabet).into_iter().collect();
    if boundary_set.is_empty() {
        return lexc_source.to_string();
    }

    let mut out = String::with_capacity(lexc_source.len() + 128);
    let mut current_lexicon: Option<&str> = None;
    let mut prefix_no_null_lines: Vec<String> = Vec::new();
    let mut suffix_no_null_lines: Vec<String> = Vec::new();

    for line in lexc_source.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("LEXICON ") {
            current_lexicon = Some(name.trim());
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let side = match current_lexicon {
            Some("PrefixChain") => Some(("PrefixOrRoot", "PrefixOrRootAfterNull")),
            Some("SuffixChain") => Some(("SuffixOrEnd", "SuffixEndOnly")),
            _ => None,
        };
        if let Some((from_continuation, to_continuation)) = side {
            match reroute_line_if_null_shaped(
                line,
                &boundary_set,
                from_continuation,
                to_continuation,
            ) {
                Some(rerouted) => {
                    // Null-shaped: replaced in place, never duplicated, so a second marker occurrence stays unreachable.
                    out.push_str(&rerouted);
                    out.push('\n');
                    continue;
                }
                None => {
                    // Ordinary: also gets a second, continuation-swapped copy so it can combine with an earlier marker.
                    if let Some(dup) = duplicate_ordinary_line_with_continuation(
                        line,
                        from_continuation,
                        to_continuation,
                    ) {
                        match current_lexicon {
                            Some("PrefixChain") => prefix_no_null_lines.push(dup),
                            Some("SuffixChain") => suffix_no_null_lines.push(dup),
                            _ => unreachable!("side is only Some for PrefixChain/SuffixChain"),
                        }
                    }
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }

    if !prefix_no_null_lines.is_empty() {
        out.push_str("\nLEXICON PrefixOrRootAfterNull\nPrefixChainNoNull ;\nRootBare ;\n");
        out.push_str("\nLEXICON PrefixChainNoNull\n");
        for l in &prefix_no_null_lines {
            out.push_str(l);
            out.push('\n');
        }
    }
    if !suffix_no_null_lines.is_empty() {
        out.push_str("\nLEXICON SuffixEndOnly\nSuffixChainNoNull ;\n# ;\n");
        out.push_str("\nLEXICON SuffixChainNoNull\n");
        for l in &suffix_no_null_lines {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

/// Duplicates an ordinary continuation-chain line with its continuation swapped to `to_continuation`; `None` if the line's continuation isn't `from_continuation`.
fn duplicate_ordinary_line_with_continuation(
    line: &str,
    from_continuation: &str,
    to_continuation: &str,
) -> Option<String> {
    let mut sep_byte = None;
    let mut prev = '\0';
    for (i, c) in line.char_indices() {
        if c == ':' && prev != '%' {
            sep_byte = Some(i);
            break;
        }
        prev = c;
    }
    let sep_byte = sep_byte?;
    let tag = &line[..sep_byte];
    let after = &line[sep_byte + 1..];
    let mut fields = after.split_whitespace();
    let underlying = fields.next()?;
    let cont = fields.next()?;
    if cont != from_continuation {
        return None;
    }
    Some(format!("{tag}:{underlying} {to_continuation} ;"))
}

/// Reroutes a continuation-chain line off `from_continuation` onto `to_continuation` if its underlying text is entirely `boundary_tokens` (so cleanup would delete it to nothing); `None` otherwise.
fn reroute_line_if_null_shaped(
    line: &str,
    boundary_tokens: &HashSet<char>,
    from_continuation: &str,
    to_continuation: &str,
) -> Option<String> {
    // A tag's own embedded colon is always escaped as `%:`, so the first unescaped ':' is the real separator.
    let mut sep_byte = None;
    let mut prev = '\0';
    for (i, c) in line.char_indices() {
        if c == ':' && prev != '%' {
            sep_byte = Some(i);
            break;
        }
        prev = c;
    }
    let sep_byte = sep_byte?;
    let tag = &line[..sep_byte];
    let after = &line[sep_byte + 1..];
    let mut fields = after.split_whitespace();
    let underlying = fields.next()?;
    let cont = fields.next()?;
    if cont != from_continuation {
        return None;
    }
    if underlying.is_empty() || !underlying.chars().all(|c| boundary_tokens.contains(&c)) {
        return None;
    }
    Some(format!("{tag}:{underlying} {to_continuation} ;"))
}

/// Finishes a `build_controllable` net into one a `crate::analyzer::FomaProposer` can actually
/// query: composes the boundary-token cleanup net, then re-minimizes.
///
/// **This step is mandatory, not an optimization.** `crate::gate::compile_gated_grammar`'s
/// own doc says so directly -- "Callers that further compose this result (every example/test driver
/// does, with a boundary-cleanup net) still need their OWN final minimize afterward" -- because the
/// composed net still carries the boundary tokens `uflexc` emitted between morphs, which a surface
/// query never contains. Skipping it does not degrade recall gracefully; it silently zeroes it. It
/// was previously open-coded only inside test drivers (`tests/p6_gate_parity.rs`), so
/// `backend_runtime::evaluate_plans` -- the one production caller -- omitted it and measured every
/// candidate against an unqueryable net.
pub fn finish_controllable_net(
    opts: &FomaOptions,
    net: Fsm,
    table: &pg_grammar::chardef::CharDefTable,
    alphabet: &SegAlphabet,
) -> Fsm {
    let net = match boundary_cleanup_net(opts, table, alphabet) {
        Some(cleanup) => fsm_compose(opts, net, cleanup),
        None => net,
    };
    fsm_minimize(opts, net)
}

/// Interprets `plan`'s controllable subtree (module doc) into a real, composed `Fsm` -- the plan-walk
/// counterpart of `crate::gate::compile_gated_grammar`. This function does not call
/// into `gate.rs` at all (it never re-derives the partition itself); it calls the same public/
/// `pub(crate)` low-level primitives that function itself uses. `g`/`alphabet`/`prules_in_order`
/// are the SAME inputs
/// `crate::enumerate::enumerate_default` (which built `plan`) and
/// `crate::gate::compile_gated_grammar` both take -- `build_controllable` does not
/// recompute grammar-derived facts `enumerate_default` already baked into `plan` (it never calls
/// `crate::gate::find_gated_subrules`/`partition_entries` itself), it only reads them back out of the
/// plan's own nodes.
///
/// # Panics
/// On any plan shape `crate::enumerate::enumerate_default` does not itself produce (a dangling
/// `NodeId`, a `Gate` node missing from the root/root-`Union`, a group's `Compose` node with the wrong
/// child count, a `Replace` cascade that doesn't match `prules_in_order`) --
/// these are caller/plan-construction contract violations, not runtime failures, so they panic
/// loudly rather than returning a `ComposeError` variant that doesn't exist for them (mirrors this
/// crate's existing convention, e.g. `crate::gate::compile_gated_grammar`'s own
/// `unwrap_or_else(|| panic!(...))` on a lexc-compile failure, and `crate::enumerate::rule_id_of`'s own
/// panic on a caller-supplied slice not borrowed from `g.prules`).
///
/// # Errors
/// Only for the same reasons `crate::gate::compile_gated_grammar` itself returns `Err` -- the
/// underlying emitter's compound-chain construction can refuse a grammar. No new failure vector is
/// introduced here: `build_controllable` trusts the plan it is handed, per this function's own doc
/// above, rather than re-deriving facts already baked into it.
pub fn build_controllable(
    plan: &Plan,
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
) -> Result<GatedCompileResult, ComposeError> {
    let gate_id = find_gate_node(plan);
    let PlanNodeKind::Gate {
        partition,
        children,
    } = plan.get(gate_id).unwrap_or_else(|| {
        panic!("find_gate_node returned a NodeId {gate_id} not interned in plan")
    })
    else {
        unreachable!("find_gate_node only ever returns the id of a Gate node")
    };
    assert_eq!(
        partition.groups.len(),
        children.len(),
        "Gate node invariant (see Plan::add_node's own debug_assert): one child per partition group"
    );
    let table_for_group = alphabet.table();

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

        // Cascade is read from this group's own Replace node, not re-derived from the Gate node's partition.
        let cascade = validate_replace_cascade(plan, replace_id, g, prules_in_order);
        assert_eq!(
            &cascade.group_key, group_key,
            "this group's own Replace node's group_key must match the Gate node's own partition \
             key for the same group -- a redundant sanity check (task 1.4: subrule_ok is now \
             derived from the Replace node's own cascade, not from this Gate-node value), catching \
             an enumerator bug that desynced the two rather than a normal-path failure"
        );

        let UEmitReport {
            lexc_source,
            skipped: uskipped,
            root_entries,
            prefix_entries,
            suffix_entries,
            ..
        } = emit_underlying_filtered(g, alphabet, Some(&entries_set))?;
        // `uskipped` also carries whole-rule entries the network structurally cannot represent, not only per-allomorph misses; pooled here with no separate channel.
        skipped_allomorphs.extend(uskipped);
        group_reports.push((
            group_key.clone(),
            root_entries,
            prefix_entries,
            suffix_entries,
        ));

        if root_entries == 0 {
            // An empty group (zero entries) contributes nothing.
            continue;
        }

        // Must run on the raw lexc source before compiling, so marker-only lines never reach the compiled `Fsm`.
        let lexc_source = reroute_null_shaped_affix_chains(&lexc_source, table_for_group, alphabet);
        let lexc_net = foma::lexcread::fsm_lexc_parse_string(opts, None, &lexc_source)
            .unwrap_or_else(|| panic!("gated group lexc failed to compile:\n{lexc_source}"));

        // subrule_ok is a pure read of this group's own Replace NodeId content -- no cross-group state to get wrong.
        let subrule_ok = subrule_ok_for_group(&cascade.gated_subrules, &cascade.group_key);

        let mut group_skipped_rules = Vec::new();
        let rules_net = compile_and_compose_rules_gated(
            opts,
            g,
            alphabet,
            prules_in_order,
            &subrule_ok,
            &mut group_skipped_rules,
            &mut tuple_reports,
        );
        for s in group_skipped_rules {
            if !skipped_rules.contains(&s) {
                skipped_rules.push(s);
            }
        }

        let group_net = match rules_net {
            Some(rules) => fsm_compose(opts, lexc_net, rules),
            None => lexc_net,
        };
        final_net = Some(match final_net {
            None => group_net,
            // Safe: groups are lexically disjoint.
            Some(prev) => fsm_union(opts, prev, group_net),
        });
    }

    let final_net = match final_net {
        Some(net) => Some(fsm_minimize(opts, net)),
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

/// Locates the single `Gate` node to interpret: the plan root itself, or its one `Gate` child if the root is a `Union`. Panics on any other plan shape rather than guessing.
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
                            // Resolves via a separate path into a lexc `String`, not an `Fsm` this interpreter builds.
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

/// One gate group's `Compose` node, resolved to its two children `(lexicon_leaf, replace_node)`. Panics on any other child count.
pub(crate) fn gate_group_children(plan: &Plan, compose_id: NodeId) -> (NodeId, NodeId) {
    let PlanNodeKind::Compose { children, .. } = plan
        .get(compose_id)
        .unwrap_or_else(|| panic!("dangling Compose NodeId {compose_id} in plan"))
    else {
        panic!("expected a Compose node as a Gate group's child at {compose_id}");
    };
    assert_eq!(
        children.len(),
        2,
        "a gate-group Compose node must have exactly 2 children (LexiconFragment leaf, shared \
         Replace node) -- enumerate_default's own shape, got {} at {compose_id}",
        children.len()
    );
    (children[0], children[1])
}

/// A gate group's `LexiconFragment` leaf, resolved to its `entries` list. Panics if the leaf isn't a `LexiconFragment` or `entries` is `None`.
pub(crate) fn lexicon_fragment_entries(plan: &Plan, lexicon_id: NodeId) -> Vec<LexEntryId> {
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

/// Cross-validates a gate group's own `Replace` node against `prules_in_order`, proving the caller's slice is the same one `enumerate_default` built `plan` from -- a mismatch would otherwise silently miscompile the group's rewrite cascade with no other signal. Panics on mismatch.
fn validate_replace_cascade<'a>(
    plan: &'a Plan,
    replace_id: NodeId,
    g: &Grammar,
    prules_in_order: &[&PhonRuleDef],
) -> &'a ReplaceCascadeSpec {
    let PlanNodeKind::Replace { cascade, children } = plan
        .get(replace_id)
        .unwrap_or_else(|| panic!("dangling Replace NodeId {replace_id}"))
    else {
        panic!(
            "expected a Replace node as a gate-group Compose node's second child at {replace_id}"
        );
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
            panic!(
                "dangling RewriteRule Leaf NodeId {} (Replace child {i})",
                children[i]
            )
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
    cascade
}

/// Builds one group's `subrule_ok(rule_pos, sub_idx)` predicate, a pure read of the Replace node's own `gated_subrules`/`group_key` content.
fn subrule_ok_for_group<'a>(
    gated_subrules: &'a [GatedSubruleRef],
    group_key: &'a [bool],
) -> impl Fn(usize, usize) -> bool + 'a {
    move |rule_pos: usize, sub_idx: usize| -> bool {
        match gated_subrules
            .iter()
            .position(|gs| gs.rule_pos == rule_pos && gs.sub_idx == sub_idx)
        {
            None => true, // ungated subrule: always included, matches crate::gate's own convention.
            Some(gate_index) => group_key[gate_index],
        }
    }
}

#[cfg(test)]
mod null_shaped_guard_scope_tests {
    //! What `reroute_null_shaped_affix_chains` does and does NOT cover, asserted rather than left
    //! to its doc comment -- because the gap between the two is exactly how the epsilon-loop defect
    //! regressed a second time (that function's own "This function is NAME-SCOPED" section).
    //!
    //! The claim pinned here: on a grammar whose bounded compound loop is genuinely emitted and
    //! genuinely carries a null-shaped prefix allomorph, this rewriter is a **complete no-op on every
    //! compound-loop lexicon** -- every `UCmp*` lexicon body comes out of it byte-identical -- while
    //! the top-level `PrefixChain` body it DOES know by name is genuinely rewritten. Both halves
    //! matter: the first is what makes `crate::uflexc`'s emission-time discipline the load-bearing
    //! mechanism for the compound levels (so nobody "simplifies" it away believing this rewriter has
    //! them covered), and the second is what proves the fixture reaches this rewriter at all rather
    //! than the whole test passing because nothing matched anywhere.

    use super::*;
    use crate::replace::SegAlphabet;
    use crate::uflexc::emit_underlying;

    /// One unrestricted `CompoundingRule` (so the compound levels are really emitted), one ordinary prefix, one all-`Boundary` (`^0+`) prefix.
    const COMPOUND_NULL_PREFIX_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>BuildCompoundNullShapedPrefixFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
        <BoundaryDefinition id="cNull"><Representations><Representation>^0</Representation><Representation>*0</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="cr1 mrRealPfx mrNullPfx">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <CompoundingRule id="cr1">
            <Name>Compound</Name>
            <CompoundingSubrules>
              <CompoundingSubrule>
                <HeadMorphologicalInput>
                  <PhoneticSequence id="h0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </HeadMorphologicalInput>
                <NonHeadMorphologicalInput>
                  <PhoneticSequence id="n0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </NonHeadMorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="h0" />
                  <CopyFromInput index="n0" />
                </MorphologicalOutput>
              </CompoundingSubrule>
            </CompoundingSubrules>
          </CompoundingRule>
          <MorphologicalRule id="mrRealPfx" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>RealPrefix</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="mrRealPfxS">
                <MorphologicalInput>
                  <PhoneticSequence id="stem1">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments>
                  <CopyFromInput index="stem1" />
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <Gloss>RPX</Gloss>
          </MorphologicalRule>
          <MorphologicalRule id="mrNullPfx" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>NullPrefix</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="mrNullPfxS">
                <MorphologicalInput>
                  <PhoneticSequence id="stem2">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <InsertSegments><PhoneticShape>^0+</PhoneticShape></InsertSegments>
                  <CopyFromInput index="stem2" />
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <Gloss>NPX</Gloss>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="root1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="root1a0"><PhoneticShape>s</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>rootS</Gloss>
          </LexicalEntry>
          <LexicalEntry id="root2" partOfSpeech="posV">
            <Allomorphs><Allomorph id="root2a0"><PhoneticShape>t</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>rootT</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    /// `lexc_source` split into `(lexicon name, body lines)` in emission order.
    fn bodies(lexc_source: &str) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        for line in lexc_source.lines() {
            let trimmed = line.trim();
            if let Some(name) = trimmed.strip_prefix("LEXICON ") {
                out.push((name.trim().to_string(), Vec::new()));
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            if let Some(last) = out.last_mut() {
                last.1.push(trimmed.to_string());
            }
        }
        out
    }

    #[test]
    fn reroute_is_a_no_op_on_the_compound_loop_lexicons() {
        let g = pg_grammar::load(COMPOUND_NULL_PREFIX_XML)
            .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let before = emit_underlying(&g, &alphabet)
            .expect("uflexc emission must succeed")
            .lexc_source;
        let after = reroute_null_shaped_affix_chains(&before, alphabet.table(), &alphabet);

        let before_bodies = bodies(&before);
        let after_bodies = bodies(&after);

        // Non-vacuity: the compound loop really is emitted here.
        let compound: Vec<&(String, Vec<String>)> = before_bodies
            .iter()
            .filter(|(name, _)| name.starts_with("UCmp"))
            .collect();
        assert!(
            !compound.is_empty(),
            "fixture emitted no UCmp* lexicon -- the compound loop did not run, so this test would \
             be vacuous:\n{before}"
        );

        // Half 1: every compound-loop lexicon body is byte-identical across the rewrite.
        for (name, body) in &before_bodies {
            if !name.starts_with("UCmp") {
                continue;
            }
            let after_body = after_bodies
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, b)| b.clone())
                .unwrap_or_else(|| {
                    panic!("`reroute_null_shaped_affix_chains` dropped lexicon `{name}`")
                });
            assert_eq!(
                body, &after_body,
                "`reroute_null_shaped_affix_chains` changed compound-loop lexicon `{name}`. Its \
                 `match` only names `PrefixChain`/`SuffixChain`, so if this ever becomes true the \
                 guard's scope was widened -- and `crate::uflexc`'s emission-time discipline plus \
                 this rewrite would then BOTH be acting on the same lines. Reconcile them; do not \
                 just update this assertion."
            );
        }

        // Proves the fixture reaches this rewriter at all, rather than the check above passing vacuously.
        let prefix_chain_before = before_bodies
            .iter()
            .find(|(n, _)| n == "PrefixChain")
            .expect("uflexc always emits a PrefixChain lexicon");
        let prefix_chain_after = after_bodies
            .iter()
            .find(|(n, _)| n == "PrefixChain")
            .expect("the rewrite preserves the PrefixChain header");
        assert_ne!(
            prefix_chain_before.1, prefix_chain_after.1,
            "the top-level `PrefixChain` body was NOT rewritten, so this fixture is not reaching \
             `reroute_null_shaped_affix_chains`'s in-scope branch at all and half 1 above proves \
             nothing:\n{before}"
        );
        assert!(
            after_bodies
                .iter()
                .any(|(n, _)| n == "PrefixOrRootAfterNull"),
            "the rewrite did not append its `PrefixOrRootAfterNull` lexicon, so it did not treat \
             this fixture's `^0+` prefix as null-shaped:\n{after}"
        );
    }
}

#[cfg(test)]
mod equivalence_tests {
    //! The correctness argument for this module's equivalence claim, made semantically meaningful
    //! rather than trivial. For an in-crate gated synthetic fixture, builds BOTH (a)
    //! `compile_gated_grammar` (today's direct-compile path) and (b)
    //! `build_controllable(enumerate_default(...))` (this module's plan-walk), then asserts the two
    //! resulting networks are EQUIVALENT BY APPLY -- `apply_up` on every distinguishing query word
    //! must yield IDENTICAL result sets. This is exactly the predicate a future differential oracle
    //! would use; the module doc explains why it -- not a structural/byte-identity
    //! claim -- is the one that matters. Minimized state/arc counts are ALSO asserted equal, as a
    //! cheap and (here) meaningful extra signal, never a substitute for the apply comparison.

    use std::collections::HashSet;

    use foma::apply::apply_init;
    use foma::options::FomaOptions;
    use foma::types::Fsm;

    use pg_grammar::model::{Grammar, PhonRuleDef};

    use super::*;
    use crate::enumerate::enumerate_default;
    use crate::gate::compile_gated_grammar;
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

    /// One MPR-gated subrule and two entries realizing both gate-key values: `e0` keeps "p" as "p" (gate false), `e1` surfaces it as "q" (gate true) -- so "p"/"q" each analyze under exactly one group.
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

    /// Every raw string `apply_up` yields for `word` against `net` -- the full literal upper-tape output set, not a decoded/collapsed projection.
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

        // (a) today's direct-compile path, unmodified.
        let direct = compile_gated_grammar(&opts, &g, &alphabet, &ro)
            .expect("direct compile must succeed");
        let direct_net = direct
            .net
            .clone()
            .expect("direct compile must produce a non-empty net");

        // (b) the plan-walk this module ships.
        let plan = enumerate_default(&g, &ro, phon.as_ref());
        let built = build_controllable(&plan, &opts, &g, &alphabet, &ro)
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

        // Structural sanity, not a substitute for the apply comparison below: both paths run the same final minimize.
        assert_eq!(
            direct_net.statecount, built_net.statecount,
            "minimized state counts must match between direct compile and plan-walk build"
        );
        assert_eq!(
            direct_net.arccount, built_net.arccount,
            "minimized arc counts must match between direct compile and plan-walk build"
        );

        // apply_up on every distinguishing query word must be identical between the two nets.
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

        // "p" and "q" must resolve to different results, proving the gate actually distinguishes the two groups.
        assert_ne!(
            apply_up_results(&direct_net, &alphabet, "p"),
            apply_up_results(&direct_net, &alphabet, "q"),
            "fixture sanity: \"p\" and \"q\" must resolve to different analyses on the direct-compile \
             net (otherwise the gate isn't actually being exercised)"
        );
    }

    /// Two differently-gated groups must get distinct Replace `NodeId`s, and that distinctness must not change the compiled relation (still apply-equivalent to direct compile).
    #[test]
    fn purity_differently_gated_groups_have_distinct_replace_node_ids_and_build_stays_apply_equivalent(
    ) {
        let g = load(gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let opts = FomaOptions::default();
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        let plan = enumerate_default(&g, &ro, phon.as_ref());

        // (a) node purity: the two gate groups must reference DISTINCT Replace NodeIds now.
        let gate_id = find_gate_node(&plan);
        let PlanNodeKind::Gate { children, .. } = plan.get(gate_id).unwrap() else {
            unreachable!("find_gate_node only ever returns the id of a Gate node")
        };
        assert_eq!(children.len(), 2, "fixture declares exactly 2 gate groups");
        let replace_ids: Vec<NodeId> = children
            .iter()
            .map(|&compose_id| gate_group_children(&plan, compose_id).1)
            .collect();
        assert_ne!(
            replace_ids[0], replace_ids[1],
            "task 1.4: two differently-gated groups must get DISTINCT Replace NodeIds -- a node's \
             compiled artifact is now a pure function of its own NodeId (design.md D1), so no two \
             groups needing different subrule_ok may share one Replace node"
        );

        // (b) that distinctness must not change the compiled relation.
        let direct = compile_gated_grammar(&opts, &g, &alphabet, &ro)
            .expect("direct compile must succeed");
        let direct_net = direct
            .net
            .clone()
            .expect("direct compile must produce a non-empty net");
        let built = build_controllable(&plan, &opts, &g, &alphabet, &ro)
            .expect("plan-walk build must succeed");
        let built_net = built
            .net
            .clone()
            .expect("plan-walk build must produce a non-empty net");

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
                 plan-walk build despite the two groups now having distinct Replace NodeIds"
            );
        }
    }
}
