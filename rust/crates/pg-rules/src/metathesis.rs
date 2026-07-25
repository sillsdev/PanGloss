//! Part 2 (phase-2 W4) — metathesis rule application: synthesis (physical node reorder) / analysis
//! (feature union). Ports `SIL.Machine.Morphology.HermitCrab/PhonologicalRules/{MetathesisRule,
//! AnalysisMetathesisRule(Spec),SynthesisMetathesisRule(Spec)}.cs`
//! (`rust/docs/phase2-completed/metathesis-w4.md`'s class map). Driven from the same `MutShape` working-
//! shape machinery `pg_rules::rewrite` uses (reused, not duplicated — both ports independently need
//! the "resolve to concrete node data before mutating" discipline the C# "RUSTIFY Stage 2" comment
//! calls for, `SynthesisMetathesisRuleSpec.cs:78-80`).
//!
//! ## Model shape (deliberate divergence from the sub-plan's sketch)
//! [`pg_grammar::model::MetathesisRuleDef`] carries ONE compiled pattern (no separate LHS/RHS split,
//! no environments — C#'s `IPhonologicalPatternSubruleSpec.LeftEnvironmentMatcher`/
//! `RightEnvironmentMatcher` are hardcoded `null` for both Analysis/SynthesisMetathesisRuleSpec) plus
//! two switch positions (`left_switch`/`right_switch`, indices into `pattern.nodes`). The sub-plan
//! sketched adding an authored `PatternNode::Group` kind (+ a `CompileNode::Group` case in
//! `pg_rules::bridge::PatternBridge`) to represent a switch. This port does that lowering **post-hoc**
//! instead ([`compile_switch_pattern`]): compile the plain pattern via `PatternBridge` as usual, then
//! wrap the two switch positions' already-compiled nodes in a named `pg_fst::CompileNode::Group` and
//! recover their matched spans via `Fst::get_offsets` after a match — exactly the technique
//! `pg_rules::rewrite::compile_env_impl` already uses to recover alpha-variable positions (Tier-2
//! #12), and the same primitive `pg_rules::morph::compile_parts` uses for affix-part captures. This
//! is strictly less new surface (no model/bridge change at all) and is justified by a fact only
//! discovered while building this milestone's fixtures: a real grammar's switch group is **always
//! exactly one shape node wide** (`<Segments>`/`<OptionalSegmentSequence>` switch-tagging is DTD-legal
//! but fails to compile against the real C# engine — see
//! `rust/conformance/metathesis/complex_rule/README.md`'s finding), so there is nothing for a
//! model-level Group to usefully wrap that post-hoc wrapping doesn't already achieve. The general
//! (multi-node-range) machinery below is still written to handle a wider range correctly — cheap
//! insurance, not exercised by any grammar the real engine can load.
//!
//! ## Synthesis (forward, physical reorder)
//! Pattern compiled in *document* order (no swap — mirrors `SynthesisMetathesisRuleSpec`'s ctor,
//! which clones every node as-is, only pinning `Modified=Clean` on the two switch nodes), matched
//! with `rule.dir` over `Segment+Boundary+Anchor` (C# `SynthesisMetathesisRule`'s
//! `MatcherSettings.Filter`), deterministic (C# never sets `Nondeterministic` here). On a match,
//! [`synthesis_reorder`] physically swaps the two switch ranges (see its doc for the exact
//! node-identity algorithm — a faithful, not shortcut, port of `MoveNodesAfter`); a node strictly
//! between them keeps its slot untouched.
//!
//! ## Analysis (reverse, feature union)
//! Pattern REBUILT ([`build_analysis_pattern`]), physical-position-driven (see that function's own
//! doc for two fixes vs. an earlier revision, both discovered building `pg_foma`'s FST-metathesis
//! containment suite, `pg_foma::tests::phase_c_metathesis`):
//! 1. **Switch order**: C#'s own `AnalysisMetathesisRuleSpec` ctor (`AnalysisMetathesisRuleSpec.
//!    cs:19-45`) always re-adds `leftGroupName`'s node first, `rightGroupName`'s second — a
//!    TAG-NAME-driven order. For every attested grammar (`left_switch` tagging whichever node is
//!    physically LAST), this coincides with `synthesis_reorder`'s own PHYSICAL-position-driven
//!    behavior (physically-last always ends up first — see that function's doc), so the two
//!    conventions were indistinguishable until `pg_grammar_gen::build::metathesis::build`'s own
//!    recipe exposed the "reversed" case (`left_switch` tagging the physically-FIRST node): C#'s
//!    tag-driven rebuild there would search for the surface's ORIGINAL, un-swapped arrangement (a
//!    vacuous no-op), disagreeing with what synthesis actually produces. This port instead orders
//!    by PHYSICAL position unconditionally (whichever of `left_switch`/`right_switch` is physically
//!    LAST goes first, physically FIRST goes second) — identical output to C#'s tag-driven order for
//!    every attested shape, and additionally a true inverse of `synthesis_reorder` for the reversed
//!    one.
//! 2. **Middle node preserved**: a node strictly between the two switches (by original physical
//!    position) is no longer dropped — it is kept in its own slot, between the two (now reordered)
//!    switch nodes, UNLESS it resolves to a `CharDefKind::Boundary` (a literal `<BoundaryMarker>`):
//!    a boundary is excluded from the analysis match sequence entirely regardless of pattern shape
//!    (`MutShape::segs(false)`'s own `NodeKind::Boundary` exclusion, "Segment+Anchor only, no
//!    boundaries" below), so keeping one in the rebuilt pattern would require a match position that
//!    can never be satisfied — dropping it is not just harmless but necessary. This is why C#'s own
//!    unconditional drop (`AnalysisMetathesisRuleSpec.cs:27-45`'s `TakeWhile(!Group)` on both ends)
//!    never surfaced as a problem in the one real shape its own conformance suite exercises there
//!    (`mrComplexMeta`'s `<BoundaryMarker>`, `metathesis-phase-isolation`'s `mu+?i` word, ported as
//!    `csharp_port_metathesis.rs::complex_rule`) — a boundary is dropped either way. A genuine middle
//!    SEGMENT context node (never attested in any real HermitCrab fixture C#'s own conformance suite
//!    carries, but DTD-legal and reachable via `pg_grammar_gen`) is instead preserved, matching
//!    `synthesis_reorder`'s real behavior (`synthesis_reorder`'s own doc: "a node strictly between
//!    them keeps its slot untouched") and letting analysis actually confirm it.
//!
//! Matched with `reverse(rule.dir)` over `Segment+Anchor` only (no boundaries), nondeterministic (C#:
//! shape nodes can be underspecified during analysis). On a match, [`ana_union`] bitwise-ORs the two
//! matched nodes' lanes onto each other (see its doc for why this equals C#'s `FeatureStruct.Union`
//! under this port's dense-lane representation) and resets both nodes' `char_def` identity to
//! `NO_CHAR_DEF` (mirroring `pg_rules::rewrite::syn_feature`'s identical, already-documented choice
//! for the same "a feature-changed node must stop being treated as a concrete, single-char-def-
//! identity node" reason).
//!
//! ## MPR/POS immunity
//! No subrule-level gating exists at all (see [`pg_grammar::model::MetathesisRuleDef`]'s doc) — every
//! `synthesize`/`analyze` call here always considers the rule applicable. Deliberately **not** pinned
//! by a dedicated test: the DTD's `<MetathesisRule>` has no `requiredMPRFeatures`/`excludedMPRFeatures`/
//! `requiredPartsOfSpeech` attribute at all, so there is no grammar any test could author that would
//! even attempt to gate one — this doc comment (and `MetathesisRuleDef`'s matching one) is the pin
//! instead, so a future "fix" adding an `MprSet`/POS parameter to `synthesize`/`analyze` here has to
//! first explain what XML would ever set it.

use pg_fst::{CompileNode, Direction, Fst, Segment, Transduce, ENTIRE_MATCH};
use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::{
    Grammar, MetathesisRuleDef, PRuleId, Pattern, PatternNode, StratumId, TableId,
};
use pg_shape::{NodeKind, Shape};

use crate::bridge::PatternBridge;
use crate::rewrite::{dir_from_model, reverse, MutNode, MutShape};
use crate::trace::{FailureReason, TraceHandle, TraceSink};
use crate::word::Word;
use rustc_hash::FxHashSet as HashSet;

const LEFT_GROUP: &str = "L";
const RIGHT_GROUP: &str = "R";

// =================================================================================================
// Pattern compilation.
// =================================================================================================

/// A compiled metathesis pattern plus the *traversal*-relative anchor flags [`match_candidates`]
/// must pass to `Transduce::anchored` (see this function's doc for why these are not simply
/// `anchor_start`/`anchor_end` as `PatternBridge` reports them).
struct CompiledSwitchPattern {
    fst: Fst,
    anchor_start: bool,
    anchor_end: bool,
}

/// Compile `pattern` (anchors included; they lift to flags per `PatternBridge::compile_pattern`'s
/// convention), wrapping the compiled nodes at `left_idx`/`right_idx` (indices into the *compiled*,
/// anchor-excluded node sequence — see [`compiled_index`]) in named capture groups so
/// [`match_candidates`] can recover their matched spans after a match, mirroring
/// `pg_rules::rewrite::compile_env_impl`'s identical `CompileNode::Group` post-processing for
/// alpha-variable position recovery.
///
/// **Direction handling** (verified against `pg-fst`'s own test suite,
/// `crates/pg-fst/tests/fst.rs`'s "Class 5: RightToLeft traversal" guards — this module is the
/// first `pg-rules` caller to compile a *multi-node* pattern with `Direction::RightToLeft`, since
/// `pg_rules::rewrite`'s own LHS/RHS patterns never carry more than the rule-level direction their
/// synthesis side already uses, and its analysis-side environments are always compiled
/// `LeftToRight` regardless of the rule's own direction, sidestepping this entirely — see
/// `compile_env_impl`'s fixed `Direction::LeftToRight` compile call): a `Direction::RightToLeft`
/// FST's compiled node list is walked in *traversal* order, where traversal index 0 is the
/// PHYSICALLY LAST segment of whatever span it matches (`rtl_asymmetric_language_walks_right_to_left`).
/// A pattern's nodes must therefore be given to the compiler in **physically-reversed** order for
/// `RightToLeft`, or a multi-node pattern silently matches nothing (confirmed empirically while
/// building this module: the analysis pattern for `simple_rule`/`complex_rule` found zero matches
/// until this reversal was added). Anchors are similarly traversal-relative at the `Transduce`
/// call site (`rtl_start_anchor_binds_physical_end`/`rtl_end_anchor_binds_physical_start`): a
/// `RightToLeft` compile must swap which physical anchor (`PatternBridge`'s `anchor_start`/
/// `anchor_end`, always physical-left/physical-right regardless of direction) feeds `.anchored`'s
/// start/end argument.
fn compile_switch_pattern(
    g: &Grammar,
    table: TableId,
    pattern: &Pattern,
    left_idx: usize,
    right_idx: usize,
    dir: Direction,
    deterministic: bool,
) -> CompiledSwitchPattern {
    let bridge = PatternBridge::new(g)
        .with_table(table)
        .deterministic(deterministic);
    let mut compiled = bridge
        .compile_pattern(pattern)
        .expect("metathesis pattern compiles");
    let (mut left_idx, mut right_idx) = (left_idx, right_idx);
    let (anchor_start, anchor_end) = match dir {
        Direction::LeftToRight => (compiled.anchor_start, compiled.anchor_end),
        Direction::RightToLeft => {
            let nodes = &mut compiled.input.nodes;
            nodes.reverse();
            let last = nodes.len() - 1;
            left_idx = last - left_idx;
            right_idx = last - right_idx;
            (compiled.anchor_end, compiled.anchor_start)
        }
    };
    for (idx, name) in [(left_idx, LEFT_GROUP), (right_idx, RIGHT_GROUP)] {
        let child = std::mem::replace(
            &mut compiled.input.nodes[idx],
            CompileNode::Constraint(Vec::new()),
        );
        compiled.input.nodes[idx] = CompileNode::Group {
            name: name.to_string(),
            children: vec![child],
        };
    }
    let fst = compiled.input.compile_with_direction(dir);
    CompiledSwitchPattern {
        fst,
        anchor_start,
        anchor_end,
    }
}

/// Full-pattern-node-space index (`MetathesisRuleDef.left_switch`/`right_switch`'s own index space,
/// anchors included) → compiled/top-level-segment-matching index (anchors excluded — the space
/// `PatternBridge::compile_pattern`'s output `CompileNode` sequence uses, since
/// `PatternNode::Anchor` lifts to a flag rather than a node, see `bridge.rs`'s module doc). An anchor
/// can only ever be the very first or very last element of `pattern.nodes` (only
/// `initialBoundaryCondition`/`finalBoundaryCondition` produce one, both applied outside the
/// `<PhoneticSequence>` child loop), so a plain "how many non-anchor nodes precede `full_idx`" count
/// is exact.
fn compiled_index(pattern: &Pattern, full_idx: u32) -> usize {
    non_anchor_count(&pattern.nodes[..full_idx as usize])
}

fn non_anchor_count(nodes: &[PatternNode]) -> usize {
    nodes
        .iter()
        .filter(|n| !matches!(n, PatternNode::Anchor(_)))
        .count()
}

/// Rebuild the search pattern analysis needs to recognize whatever `synthesis_reorder` actually
/// produces (this module's own doc, "Analysis" section, has the full rationale + citations for both
/// fixes below vs. an earlier revision of this function). Returns the rebuilt pattern plus the
/// (now-reordered) switch nodes' own indices in *that* pattern's full-node-space.
///
/// `pre` is every node strictly before the *first* (by original physical position) of the two switch
/// nodes (verbatim, original order); `post` is every node strictly after the *last* (verbatim). The
/// two switch nodes are re-added PHYSICAL-POSITION-first: whichever of `left_switch`/`right_switch`
/// is physically LAST in `pattern.nodes` goes first, physically FIRST goes second — matching
/// `synthesis_reorder`'s own real behavior (physically-last-always-ends-up-first, tag-name-agnostic),
/// not C#'s literal tag-driven `leftGroupName`-always-first order (which happens to coincide with
/// this for every attested grammar, since `left_switch` always tags the physically-last node there,
/// but disagrees for the "reversed" tag convention `pg_grammar_gen`'s own recipe exercises).
///
/// Any node strictly between the two original switch positions is preserved, in its own slot,
/// between the two (now reordered) switch nodes — UNLESS [`is_boundary_node`] reports it resolves to
/// a `CharDefKind::Boundary`, in which case it is dropped (a boundary never appears in the analysis
/// match sequence regardless of pattern shape, so requiring one here could never be satisfied; see
/// this module's doc for why C#'s own unconditional drop never surfaced this as a problem).
fn build_analysis_pattern(
    g: &Grammar,
    table: TableId,
    pattern: &Pattern,
    left_switch: u32,
    right_switch: u32,
) -> (Pattern, u32, u32) {
    let (first, last) = if left_switch < right_switch {
        (left_switch, right_switch)
    } else {
        (right_switch, left_switch)
    };
    let mut nodes: Vec<PatternNode> = pattern.nodes[..first as usize].to_vec();
    nodes.push(pattern.nodes[last as usize].clone());
    let left_full = (nodes.len() - 1) as u32;
    for mid in &pattern.nodes[(first as usize + 1)..(last as usize)] {
        if !is_boundary_node(g, table, mid) {
            nodes.push(mid.clone());
        }
    }
    nodes.push(pattern.nodes[first as usize].clone());
    let right_full = (nodes.len() - 1) as u32;
    nodes.extend(pattern.nodes[(last as usize + 1)..].iter().cloned());
    (Pattern { nodes }, left_full, right_full)
}

/// Whether `node` lowers to a `NodeKind::Boundary` shape node at segmentation time (the only
/// `PatternNode` kind that ever does is a literal `<Segment>`/`<BoundaryMarker>` resolving to a
/// `CharDefKind::Boundary` char def — a `Context` node's `SimpleContext` always names a segment
/// natural class, per this crate's own `PatternNode::Context` doc; `pg_grammar::chardef`'s own
/// "AddBoundary always passes `fs: null`" provenance note is why boundaries never carry a natural-
/// class feature constraint). Used by [`build_analysis_pattern`] to decide whether a middle node
/// between the two switches must be dropped (a boundary, transparent to analysis matching either
/// way) or preserved (a real segment, required for a faithful round-trip with `synthesis_reorder`).
fn is_boundary_node(g: &Grammar, table: TableId, node: &PatternNode) -> bool {
    match node {
        PatternNode::CharDef(id) => {
            g.char_tables[table.0 as usize].get(*id).kind() == CharDefKind::Boundary
        }
        _ => false,
    }
}

// =================================================================================================
// Matching.
// =================================================================================================

/// One accepted candidate: the two switch groups' matched (start,end) ranges in *segment-index*
/// space (`ms.segs(...)`'s output space, not `ms.nodes` space — see [`seg_range_to_nodes`]).
struct Candidate {
    entire: (usize, usize),
    left: (usize, usize),
    right: (usize, usize),
}

/// Every distinct match of `pattern` over `segs`, each with its `ENTIRE_MATCH`/`LEFT_GROUP`/
/// `RIGHT_GROUP` capture offsets, sorted ascending (leftmost first) and deduped — mirrors
/// `pg_rules::rewrite::all_spans`'s convention, extended with the two named group offsets
/// `pg_rules::rewrite`'s single-pattern rules never need (metathesis is the first rule kind whose
/// mutation is driven by *which sub-span* matched, not just "the whole pattern matched here"). Passes
/// `pattern`'s (already traversal-relative — see `compile_switch_pattern`'s doc) anchor flags through
/// to `Transduce::anchored`, matching how every anchor-bearing pattern in this crate is enforced (an
/// anchor is a call-site flag, never baked into the compiled `Fst` itself — `pg_rules::bridge`'s
/// module doc).
fn match_candidates(pattern: &CompiledSwitchPattern, segs: &[Segment]) -> Vec<Candidate> {
    if segs.is_empty() {
        return Vec::new();
    }
    let fst = &pattern.fst;
    let results = Transduce::new(fst, segs.to_vec())
        .anchored(pattern.anchor_start, pattern.anchor_end)
        .all_matches();
    let mut out: Vec<(usize, usize, usize, usize, usize, usize)> = results
        .iter()
        .filter_map(|r| {
            let (es, ee) = fst.get_offsets(ENTIRE_MATCH, &r.registers)?;
            let (ls, le) = fst.get_offsets(LEFT_GROUP, &r.registers)?;
            let (rs, re) = fst.get_offsets(RIGHT_GROUP, &r.registers)?;
            Some((
                es as usize,
                ee as usize,
                ls as usize,
                le as usize,
                rs as usize,
                re as usize,
            ))
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out.into_iter()
        .map(|(es, ee, ls, le, rs, re)| Candidate {
            entire: (es, ee),
            left: (ls, le),
            right: (rs, re),
        })
        .collect()
}

/// Translate a segment-index range (from a `Candidate`) to the `ms.nodes` indices it covers, via
/// `node_of` (`ms.segs(...)`'s second return value).
fn seg_range_to_nodes(node_of: &[usize], range: (usize, usize)) -> Vec<usize> {
    node_of[range.0..range.1].to_vec()
}

// =================================================================================================
// Synthesis: physical reorder.
// =================================================================================================

/// C# `SynthesisMetathesisRuleSpec.ApplyRhs`/`MoveNodesAfter` (`SynthesisMetathesisRuleSpec.cs:76-140`),
/// ported literally rather than as a "swap the two ranges as blocks" shortcut: a non-Segment node
/// (e.g. a boundary) *inside* one switch's own captured range does not itself move (C#:
/// `if (node.Type() == HCFeatureSystem.Segment) { Remove; AddAfter; }`), but the loop's cursor still
/// advances past it (`cur = node`, unconditional) — so a segment captured *later* in the same
/// multi-node range anchors off that boundary's ORIGINAL position, not off wherever an earlier
/// segment of the same range ended up. This only matters for a switch range wider than one node,
/// which no grammar the real C# engine can load actually produces (see this module's doc) — kept
/// general anyway since the C# source itself is written generally and the algorithm is no harder to
/// get right in general form than to special-case down to width 1.
///
/// `left`/`right` are the two switch ranges as `ms.nodes` index lists (already resolved from the
/// match BEFORE any mutation — the "RUSTIFY Stage 2" fix `SynthesisMetathesisRuleSpec.cs:78-80` asks
/// for, mirrored here by resolving everything to concrete `MutNode` data up front and performing one
/// final `Vec::splice`, rather than mutating `ms.nodes` in place through a sequence of index-shifting
/// operations).
fn synthesis_reorder(ms: &mut MutShape, left: &[usize], right: &[usize]) {
    let lo = *left
        .iter()
        .chain(right)
        .min()
        .expect("both ranges non-empty");
    let hi = *left
        .iter()
        .chain(right)
        .max()
        .expect("both ranges non-empty")
        + 1;
    let window: Vec<MutNode> = ms.nodes[lo..hi].to_vec();
    let loc = |abs: usize| abs - lo;
    let left_loc: Vec<usize> = left.iter().map(|&i| loc(i)).collect();
    let right_loc: Vec<usize> = right.iter().map(|&i| loc(i)).collect();

    let mut order: Vec<usize> = (0..window.len()).collect();

    // Step 1 (C#: `MoveNodesAfter(shape, leftEnd, rightRange)`): move the right switch's segments to
    // just after the left switch's own last node.
    let left_end = *left_loc.last().expect("switch range non-empty");
    move_nodes_after(&mut order, &window, Some(left_end), &right_loc);

    // Step 2 (C#: `MoveNodesAfter(shape, beforeRightGroup, leftRange)`): move the left switch's
    // segments to just after whatever ORIGINALLY preceded the right switch's start (`None` = the
    // right switch started the window, i.e. "insert at the very front").
    let right_start = *right_loc.first().expect("switch range non-empty");
    let before_right = right_start.checked_sub(1);
    move_nodes_after(&mut order, &window, before_right, &left_loc);

    let moved: HashSet<usize> = left_loc
        .iter()
        .chain(right_loc.iter())
        .copied()
        .filter(|&i| window[i].kind == NodeKind::Segment)
        .collect();
    let new_window: Vec<MutNode> = order
        .iter()
        .map(|&i| {
            let mut n = window[i].clone();
            if moved.contains(&i) {
                n.dirty = true;
            }
            n
        })
        .collect();
    ms.nodes.splice(lo..hi, new_window);
}

/// One `MoveNodesAfter` call: walk `range` (original-local indices into `window`, in original
/// left-to-right order) one node at a time. A Segment-typed node is removed from wherever it
/// currently sits in `order` and reinserted immediately after `cur`'s *current* position (`None` =
/// the position before the very start of `order`); a non-Segment node is never moved. Either way
/// `cur` advances to that node's identity before the next iteration — see this function's caller doc
/// for why that "advance even when not moving" detail matters.
///
/// Degenerate case (not reachable from either of `synthesis_reorder`'s two calls given a *sane*
/// grammar, where the switch named to end up first is not already adjacent-and-first): if `cur`
/// itself is the node currently being moved in `range` (step 2's `cur` can coincide with the left
/// switch's own last node when the two switches are adjacent with the left switch already
/// physically first — a self-defeating rule authoring nobody would write, since it asks the engine
/// to move a span to "right after itself"), `cur`'s position is looked up *after* removing it and is
/// no longer found; this falls back to appending at the current end of `order` rather than panicking.
/// C#'s own `ShapeNode.AddAfter` on a just-removed node is equally not a well-specified operation for
/// this input, so no attempt is made to reproduce a specific (undefined) C# outcome here.
fn move_nodes_after(
    order: &mut Vec<usize>,
    window: &[MutNode],
    mut cur: Option<usize>,
    range: &[usize],
) {
    for &n in range {
        if window[n].kind == NodeKind::Segment {
            let np = order.iter().position(|&i| i == n);
            if let Some(np) = np {
                order.remove(np);
            }
            let insert_at = match cur {
                None => 0,
                Some(c) => order
                    .iter()
                    .position(|&i| i == c)
                    .map(|p| p + 1)
                    .unwrap_or(order.len()),
            };
            order.insert(insert_at.min(order.len()), n);
        }
        cur = Some(n);
    }
}

// =================================================================================================
// Analysis: feature union.
// =================================================================================================

/// C# `AnalysisMetathesisRuleSpec.ApplyRhs` (`AnalysisMetathesisRuleSpec.cs:93-130`): for each
/// `(leftNode, rightNode)` pair (Segment-typed only — zipped over the two switch ranges, so a
/// non-Segment or length-mismatched pairing is simply skipped, matching C#'s
/// `if (tuple.Item1.Type() != Segment || tuple.Item2.Type() != Segment) continue;`), union each
/// node's `FeatureStruct` into the other's and mark both dirty.
///
/// `FeatureStruct.Union` (`FeatureStruct.cs:407-451`) keeps, per feature, the two sides' *symbol-set*
/// union where both sides have that feature, and drops (unconstrains) any feature only one side has.
/// This port's dense per-feature `u64` lanes represent "feature absent" identically to "feature fully
/// unconstrained" (`UNCONSTRAINED = u64::MAX`, `pg_rules::bridge`'s module doc), so a plain bitwise OR
/// across every lane reproduces both halves of that rule at once: two concrete (pinned) lanes OR
/// together into the correct widened symbol set; a lane that is `UNCONSTRAINED` on either side stays
/// `UNCONSTRAINED` after the OR (matching "one side lacks the feature ⇒ result lacks it too") — this
/// is exact, not an approximation, given that representation (verified against
/// `SimpleFeatureValue.UnionImpl`/`UnionWith`'s bit-set semantics in the C# source).
///
/// Also resets both nodes' `char_def` to `NO_CHAR_DEF` (`pg_shape::NO_CHAR_DEF`), mirroring
/// `pg_rules::rewrite::syn_feature`'s identical, already-documented choice: after a lane-widening
/// mutation, a node's stale literal `char_def` identity (the Sena-motivated "restrict to this one
/// segment's own representations" lock, plan §13.1 Tier-1 #3) would otherwise keep root-trie/surface
/// lookups pinned to the *pre-union* segment's own representations, unable to recognize the other
/// (equally valid, now-unioned-in) segment identity. C#'s `Union` has no analogous per-node identity
/// dimension to reset (it is pure `FeatureStruct` algebra), so this is this port's own addition to a
/// representation gap C# does not have — not a divergence in *engine* behavior.
fn ana_union(ms: &mut MutShape, left: &[usize], right: &[usize]) {
    for (&a, &b) in left.iter().zip(right) {
        if ms.nodes[a].kind != NodeKind::Segment || ms.nodes[b].kind != NodeKind::Segment {
            continue;
        }
        let w = ms.nodes[a].lanes.len();
        for f in 0..w {
            let combined = ms.nodes[a].lanes[f] | ms.nodes[b].lanes[f];
            ms.nodes[a].lanes[f] = combined;
            ms.nodes[b].lanes[f] = combined;
        }
        ms.nodes[a].dirty = true;
        ms.nodes[b].dirty = true;
        ms.nodes[a].char_def = pg_shape::NO_CHAR_DEF;
        ms.nodes[b].char_def = pg_shape::NO_CHAR_DEF;
    }
}

// =================================================================================================
// Public API (mirrors `pg_rules::rewrite::synthesize`/`analyze`'s signatures/return convention).
// =================================================================================================

/// Apply `rule` forward to `input` (C# `SynthesisMetathesisRule.Apply`). Returns the rewritten shape
/// in a one-element vec if the rule applied, else empty. No MPR/POS gating exists for a metathesis
/// rule at all (see `MetathesisRuleDef`'s doc) — unlike `rewrite::synthesize_with_mpr`, there is no
/// `_with_mpr` sibling to call instead.
pub fn synthesize(g: &Grammar, rule: &MetathesisRuleDef, input: &Shape) -> Vec<Shape> {
    let table_id = TableId(0);
    let dir = dir_from_model(rule.dir);
    let left_idx = compiled_index(&rule.pattern, rule.left_switch);
    let right_idx = compiled_index(&rule.pattern, rule.right_switch);
    let compiled =
        compile_switch_pattern(g, table_id, &rule.pattern, left_idx, right_idx, dir, true);
    let pattern_len = non_anchor_count(&rule.pattern.nodes);
    synthesize_with_pattern(&compiled, pattern_len, input)
}

pub(crate) fn synthesize_cached(
    rule: &MetathesisRuleDef,
    input: &Shape,
    cache: &MetaCache,
) -> Vec<Shape> {
    let pattern_len = non_anchor_count(&rule.pattern.nodes);
    synthesize_with_pattern(&cache.syn, pattern_len, input)
}

fn synthesize_with_pattern(
    pattern: &CompiledSwitchPattern,
    pattern_len: usize,
    input: &Shape,
) -> Vec<Shape> {
    let mut ms = MutShape::from_shape(input);
    let mut applied = false;
    loop {
        let (segs, node_of) = ms.segs(true);
        let mut acted = false;
        for cand in match_candidates(pattern, &segs) {
            if cand.entire.1 - cand.entire.0 != pattern_len {
                continue; // width guard (plan W1.1): reject an Optional-skip over-wide span.
            }
            let left_nodes = seg_range_to_nodes(&node_of, cand.left);
            let right_nodes = seg_range_to_nodes(&node_of, cand.right);
            if left_nodes
                .iter()
                .chain(&right_nodes)
                .any(|&n| ms.nodes[n].dirty)
            {
                continue; // Modified=Clean: only the switch nodes are gated (see module doc).
            }
            synthesis_reorder(&mut ms, &left_nodes, &right_nodes);
            applied = true;
            acted = true;
            break;
        }
        if !acted {
            break;
        }
    }
    if applied {
        vec![ms.to_shape()]
    } else {
        Vec::new()
    }
}

// =================================================================================================
// P12 chunk 6 — phonological rule tracing (metathesis, synthesis side).
//
// C# `SynthesisMetathesisRule.Apply` (`PhonologicalRules/SynthesisMetathesisRule.cs:35-55`) is the
// simplest of the four phonological-rule trace call sites: ONE compiled pattern, no subrules, no
// MPR/POS gate (see this module's own doc), so there is no per-subrule side channel to build at all
// -- just `PhonologicalRuleApplied(_rule, -1, origInput, input)` on success or
// `PhonologicalRuleNotApplied(_rule, -1, input, FailureReason.Pattern, null)` on failure, subrule
// index ALWAYS -1 either way (cs:47,52). `FailureReason::Pattern` is the ONLY reason a metathesis
// rule can ever report (§1.4 of the design doc: metathesis's sole call site is in the `Pattern`
// row's fan-out list) -- there is no gate to decompose the way rewrite's subrules have.
// =================================================================================================

/// [`synthesize`]'s traced sibling — standalone (recompiles every call, like [`synthesize`] itself).
/// `pid` is a nominal trace-tree identity for fixtures with no grammar-resident prule table (mirrors
/// `pg_rules::rewrite::synthesize_with_mpr_traced`'s convention).
pub fn synthesize_traced(
    g: &Grammar,
    pid: PRuleId,
    rule: &MetathesisRuleDef,
    input: &Shape,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Shape> {
    let result = synthesize(g, rule, input);
    if trace.is_tracing() {
        report_metathesis_synth(trace, parent, pid, input, &result);
    }
    result
}

/// The [`MetaCache`]-aware sibling of [`synthesize_traced`] — the real per-word pipeline's traced
/// entry point (`crate::stratum::synthesize_stratum_traced`'s trailing prule application, the
/// `PhonRuleDef::Metathesis` arm). Takes the real `&Word` so the trace snapshot carries the word's
/// actual full state and `node_parent` can fall back to `input.trace`, exactly like
/// `pg_rules::rewrite::synthesize_with_mpr_cached_traced`.
pub(crate) fn synthesize_cached_traced(
    pid: PRuleId,
    rule: &MetathesisRuleDef,
    input: &Word,
    cache: &MetaCache,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Shape> {
    let result = synthesize_cached(rule, &input.shape, cache);
    if trace.is_tracing() {
        let node_parent = input.trace.unwrap_or(parent);
        let mut out_word = input.clone();
        if let Some(s) = result.first() {
            out_word.shape = s.clone();
        }
        if result.is_empty() {
            trace.phonological_rule_not_applied(
                node_parent,
                pid,
                -1,
                &out_word,
                FailureReason::Pattern,
            );
        } else {
            trace.phonological_rule_applied(node_parent, pid, -1, &out_word);
        }
    }
    result
}

/// Shared readout for the two synthesis-traced functions above: `result` is what the untraced
/// [`synthesize`]/[`synthesize_cached`] already returned -- empty means `NotApplied(Pattern)` (using
/// the original `input`), non-empty means `Applied` (using the rewritten shape).
fn report_metathesis_synth(
    trace: &dyn TraceSink,
    parent: TraceHandle,
    pid: PRuleId,
    input: &Shape,
    result: &[Shape],
) {
    match result.first() {
        Some(out_shape) => {
            let snap = Word::new(out_shape.clone(), StratumId(0));
            trace.phonological_rule_applied(parent, pid, -1, &snap);
        }
        None => {
            let snap = Word::new(input.clone(), StratumId(0));
            trace.phonological_rule_not_applied(parent, pid, -1, &snap, FailureReason::Pattern);
        }
    }
}

/// Un-apply `rule` to `input` (C# `AnalysisMetathesisRule.Apply`). Returns the un-applied shape in a
/// one-element vec if the rule un-applied, else empty.
pub fn analyze(g: &Grammar, rule: &MetathesisRuleDef, input: &Shape) -> Vec<Shape> {
    let table_id = TableId(0);
    let dir = reverse(dir_from_model(rule.dir));
    let (ana_pattern, left_idx_full, right_idx_full) =
        build_analysis_pattern(g, table_id, &rule.pattern, rule.left_switch, rule.right_switch);
    let left_idx = compiled_index(&ana_pattern, left_idx_full);
    let right_idx = compiled_index(&ana_pattern, right_idx_full);
    let compiled =
        compile_switch_pattern(g, table_id, &ana_pattern, left_idx, right_idx, dir, false);
    let pattern_len = non_anchor_count(&ana_pattern.nodes);
    analyze_with_pattern(&compiled, pattern_len, input)
}

pub(crate) fn analyze_cached(
    _rule: &MetathesisRuleDef,
    input: &Shape,
    cache: &MetaCache,
) -> Vec<Shape> {
    analyze_with_pattern(&cache.ana, cache.ana_pattern_len, input)
}

fn analyze_with_pattern(
    pattern: &CompiledSwitchPattern,
    pattern_len: usize,
    input: &Shape,
) -> Vec<Shape> {
    let mut ms = MutShape::from_shape(input);
    let mut applied = false;
    loop {
        let (segs, node_of) = ms.segs(false);
        let mut acted = false;
        for cand in match_candidates(pattern, &segs) {
            if cand.entire.1 - cand.entire.0 != pattern_len {
                continue;
            }
            let left_nodes = seg_range_to_nodes(&node_of, cand.left);
            let right_nodes = seg_range_to_nodes(&node_of, cand.right);
            if left_nodes
                .iter()
                .chain(&right_nodes)
                .any(|&n| ms.nodes[n].dirty)
            {
                continue;
            }
            ana_union(&mut ms, &left_nodes, &right_nodes);
            applied = true;
            acted = true;
            break;
        }
        if !acted {
            break;
        }
    }
    if applied {
        vec![ms.to_shape()]
    } else {
        Vec::new()
    }
}

// =================================================================================================
// P12 chunk 6 — phonological rule tracing (metathesis, analysis side).
//
// C# `AnalysisMetathesisRule.Apply` (`PhonologicalRules/AnalysisMetathesisRule.cs:38-58`): same
// single-pattern, no-subrule shape as the synthesis side, but the analysis event pair carries no
// `FailureReason` at all (`ITraceManager.cs:42-43`) -- `PhonologicalRuleUnapplied(_rule, -1,
// origInput, input)` on success, `PhonologicalRuleNotUnapplied(_rule, -1, input)` on failure
// (cs:49-55). Not yet wired into the live per-word pipeline for the same reason
// `pg_rules::rewrite::analyze_cached_traced` isn't: `crate::stratum::StratumAnalyzer::analyze`
// (the sole caller of `analyze`/`analyze_cached` today) is itself untraced -- a pre-existing,
// separately-documented P12 gap (see that function's doc). Built and unit-tested now so the
// mechanism exists; a future pass that traces `StratumAnalyzer::analyze` calls these instead of
// [`analyze`]/[`analyze_cached`].
// =================================================================================================

/// [`analyze`]'s traced sibling — standalone (recompiles every call). `pid` is a nominal trace-tree
/// identity, matching every other standalone `_traced` function's convention in this crate.
pub fn analyze_traced(
    g: &Grammar,
    pid: PRuleId,
    rule: &MetathesisRuleDef,
    input: &Shape,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Shape> {
    let result = analyze(g, rule, input);
    if trace.is_tracing() {
        report_metathesis_analysis(trace, parent, pid, input, &result);
    }
    result
}

/// The [`MetaCache`]-aware sibling of [`analyze_traced`]. Not yet called from live code (see this
/// section's doc: the analysis-side stratum caller is itself untraced) — `MetaCache`'s own
/// `pub(crate)` visibility rules out the "export it `pub`" dodge `pg_rules::rewrite::
/// analyze_cached_traced` uses for the identical situation, so `dead_code` is silenced explicitly
/// here instead; exercised directly by this module's own unit tests.
#[allow(dead_code)]
pub(crate) fn analyze_cached_traced(
    pid: PRuleId,
    rule: &MetathesisRuleDef,
    input: &Shape,
    cache: &MetaCache,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Shape> {
    let result = analyze_cached(rule, input, cache);
    if trace.is_tracing() {
        report_metathesis_analysis(trace, parent, pid, input, &result);
    }
    result
}

/// Shared readout for the two analysis-traced functions above (see this section's doc for why no
/// `FailureReason` is ever attached, mirroring C# exactly).
fn report_metathesis_analysis(
    trace: &dyn TraceSink,
    parent: TraceHandle,
    pid: PRuleId,
    input: &Shape,
    result: &[Shape],
) {
    match result.first() {
        Some(out_shape) => {
            let snap = Word::new(out_shape.clone(), StratumId(0));
            trace.phonological_rule_unapplied(parent, pid, -1, &snap);
        }
        None => {
            let snap = Word::new(input.clone(), StratumId(0));
            trace.phonological_rule_not_unapplied(parent, pid, -1, &snap);
        }
    }
}

// =================================================================================================
// Compile-once cache (`crate::cache::RuleCache`'s metathesis-rule slice).
// =================================================================================================

/// Per-metathesis-rule precompiled matchers (`crate::cache::RuleCache`'s analog of
/// `rewrite::PruleCache` for this rule kind).
pub(crate) struct MetaCache {
    syn: CompiledSwitchPattern,
    ana: CompiledSwitchPattern,
    /// `non_anchor_count` of the REBUILT analysis pattern's nodes -- cached here (rather than
    /// recomputed by re-running [`build_analysis_pattern`] on every [`analyze_cached`] call, as an
    /// earlier revision did) because that rebuild is now `&Grammar`-aware ([`is_boundary_node`]),
    /// and `analyze_cached` itself has no `&Grammar` in scope (only [`build_meta_cache`] does).
    ana_pattern_len: usize,
}

pub(crate) fn build_meta_cache(
    g: &Grammar,
    table_id: TableId,
    rule: &MetathesisRuleDef,
) -> MetaCache {
    let syn_dir = dir_from_model(rule.dir);
    let syn_left = compiled_index(&rule.pattern, rule.left_switch);
    let syn_right = compiled_index(&rule.pattern, rule.right_switch);
    let syn = compile_switch_pattern(
        g,
        table_id,
        &rule.pattern,
        syn_left,
        syn_right,
        syn_dir,
        true,
    );

    let ana_dir = reverse(syn_dir);
    let (ana_pattern, ana_left_full, ana_right_full) =
        build_analysis_pattern(g, table_id, &rule.pattern, rule.left_switch, rule.right_switch);
    let ana_left = compiled_index(&ana_pattern, ana_left_full);
    let ana_right = compiled_index(&ana_pattern, ana_right_full);
    let ana_pattern_len = non_anchor_count(&ana_pattern.nodes);
    let ana = compile_switch_pattern(
        g,
        table_id,
        &ana_pattern,
        ana_left,
        ana_right,
        ana_dir,
        false,
    );

    MetaCache {
        syn,
        ana,
        ana_pattern_len,
    }
}
