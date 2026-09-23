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
use crate::tags;
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

/// Refines `unbuildable_markers`'s structural pre-check against the REAL per-marker computation
/// (`crate::emit::composite_emission_marker_material` / `structural_composite_marker_material` --
/// the SAME functions `build_controllable` itself calls via `marker_material` below, so this
/// published refusal and what actually gets built can never drift apart, per this crate's own rule
/// against re-deriving a decision another module already makes).
///
/// A marker `unbuildable_markers` finds is taken back out of the returned (refused) list ONLY when
/// BOTH hold: its real computation is non-empty (there is genuine material to union), AND
/// `crate::emit::marker_admission_is_complete(g)` proves no other derivational/template material in
/// this grammar could ever attach outside the composite/structural stem the union admits as a
/// COMPLETE, STANDALONE word (`compile_marker_material`'s own doc explains why the union cannot
/// also be wrapped by a further affix: that material is already phonology-resolved surface text
/// from a real per-word synthesis pass, and feeding it back through `crate::replace`'s compiled
/// rewrite-rule cascade a second time would be an unverified, possibly unsound double application
/// of a different phonology implementation).
///
/// A marker stays in the returned (refused) list for every OTHER outcome: `Err` (the real
/// computation itself could not finish within its resource boundary), non-empty-but-incomplete
/// (some other affix/template could still wrap the stem -- admitting would under-generate), and,
/// deliberately, EMPTY TOO. Two grammars can be structurally identical under both this function and
/// `marker_admission_is_complete` (zero records, zero standalone rules, zero templates) while one
/// has an entirely unrelated, pre-existing PlanComposed defect neither fact can see -- measured
/// directly: `machine:edge-cases/right-to-left-anchor-environment` (empty material, no other
/// defect, safe to admit) and `machine:edge-cases/loader-default-symbol` /
/// `machine:edge-cases/mpr-gated-exception` (also empty material, but each exposes a real,
/// unrelated rewrite-cascade/MPR-gating gap once no longer blanket-refused) are indistinguishable
/// by any fact this module can compute from the marker alone. Since admitting an EMPTY result never
/// gains anything (there is nothing to union), never admitting on emptiness is the only sound
/// choice available without a broader, out-of-scope capability predicate for those other gaps.
pub fn unbuildable_marker_material(plan: &Plan, g: &Grammar) -> Vec<FragmentSpec> {
    unbuildable_markers(plan)
        .into_iter()
        .filter(|marker| match marker_material(g, marker) {
            Ok(records) if records.is_empty() => true,
            Ok(_non_empty) => !crate::emit::marker_admission_is_complete(g, marker),
            Err(_) => true,
        })
        .collect()
}

/// Dispatches one marker to its real computation, shared by `unbuildable_marker_material` and `build_controllable`.
fn marker_material(
    g: &Grammar,
    marker: &FragmentSpec,
) -> Result<Vec<crate::preexpand::CompositeRec>, String> {
    match marker {
        FragmentSpec::CompositeEmissionMarker => crate::emit::composite_emission_marker_material(g),
        FragmentSpec::StructuralCompositeMarker => {
            crate::emit::structural_composite_marker_material(g)
        }
        other => panic!(
            "enumerate_default only ever places CompositeEmissionMarker/StructuralCompositeMarker \
             leaves at the plan root, got {other:?}"
        ),
    }
}

/// The marker leaves `find_gate_node` skips at a `Union` plan root (module doc "Scope: controllable subtree only"), reused here rather than re-scanning.
fn union_root_markers(plan: &Plan, root: NodeId) -> Vec<FragmentSpec> {
    match plan
        .get(root)
        .unwrap_or_else(|| panic!("plan root NodeId {root} is not interned in this Plan"))
    {
        PlanNodeKind::Gate { .. } => Vec::new(),
        PlanNodeKind::Union { children } => children
            .iter()
            .filter_map(|&child| match plan.get(child) {
                Some(PlanNodeKind::Leaf { fragment, .. })
                    if matches!(
                        fragment,
                        FragmentSpec::CompositeEmissionMarker
                            | FragmentSpec::StructuralCompositeMarker
                    ) =>
                {
                    Some(fragment.clone())
                }
                _ => None,
            })
            .collect(),
        other => panic!(
            "build_controllable's own find_gate_node already validated the plan root is a Gate or \
             Union, got {} here",
            other.kind_name()
        ),
    }
}

/// Compiles one marker's real material into a standalone, bare word `Fsm` in `alphabet`'s own PUA-token space (see `unbuildable_marker_material`'s own doc for why standalone-only); `skipped` records any variant `SegAlphabet::encode_query` cannot re-segment, and `None` comes back when nothing was encodable.
fn compile_marker_material(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    marker: &FragmentSpec,
    records: &[crate::preexpand::CompositeRec],
    skipped: &mut Vec<String>,
) -> Option<Fsm> {
    if records.is_empty() {
        return None;
    }
    let width = tags::tag_width(g.morphemes.len());
    let mut symbols: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut entries: Vec<(String, String)> = Vec::new();
    for rec in records {
        for &(is_root, m) in &rec.chain_morphemes {
            symbols.insert(if is_root {
                tags::root_tag_lexc(m, width)
            } else {
                tags::morph_tag_lexc(m, width)
            });
        }
        for v in &rec.variants {
            match alphabet.encode_query(v) {
                Some(tokens) => entries.push((rec.tag_lexc.clone(), tokens)),
                None => skipped.push(format!(
                    "{marker:?} entry {:?} variant {v:?} unrepresentable in this backend's token \
                     alphabet",
                    rec.tag_lexc
                )),
            }
        }
    }
    if entries.is_empty() {
        return None;
    }
    let mut src = String::from("Multichar_Symbols\n");
    for sym in &symbols {
        src.push_str(sym);
        src.push('\n');
    }
    src.push_str("\nLEXICON Root\nComposites ;\n\nLEXICON Composites\n");
    for (tag_lexc, tokens) in &entries {
        src.push_str(tag_lexc);
        src.push(':');
        src.push_str(tokens);
        src.push_str(" # ;\n");
    }
    Some(
        foma::lexcread::fsm_lexc_parse_string(opts, None, &src).unwrap_or_else(|| {
            panic!("marker material lexc failed to compile for {marker:?}:\n{src}")
        }),
    )
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

    // Marker material is grammar-wide, ungated (enumerate_default's own doc), so it is built once here, never per-group.
    let root = plan
        .root()
        .expect("build_controllable requires a Plan with a root set");
    for marker in union_root_markers(plan, root) {
        let records = marker_material(g, &marker).unwrap_or_else(|reason| {
            panic!(
                "plan carries {marker:?} but its real material could not be built: {reason} -- the \
                 caller should have refused via crate::build::unbuildable_marker_material before \
                 invoking build_controllable"
            )
        });
        if let Some(marker_net) = compile_marker_material(
            opts,
            g,
            alphabet,
            &marker,
            &records,
            &mut skipped_allomorphs,
        ) {
            final_net = Some(match final_net {
                None => marker_net,
                Some(prev) => fsm_union(opts, prev, marker_net),
            });
        }
    }

    let final_net = final_net.map(|net| fsm_minimize(opts, net));

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
                            // Built separately by union_root_markers/compile_marker_material, not by this Gate walk.
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
mod null_shaped_guard_scope_tests;

#[cfg(test)]
mod equivalence_tests;
