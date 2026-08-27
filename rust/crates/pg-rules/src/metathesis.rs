//! Metathesis rule application: synthesis (physical node reorder) / analysis
//! (feature union). Ports `SIL.Machine.Morphology.HermitCrab/PhonologicalRules/{MetathesisRule,
//! AnalysisMetathesisRule(Spec),SynthesisMetathesisRule(Spec)}.cs`. Driven from the same
//! `MutShape` working-shape machinery `pg_rules::rewrite` uses (reused, not duplicated — both
//! ports independently need the "resolve to concrete node data before mutating" discipline the C#
//! implementation also calls for).
//!
//! ## Model shape (deliberate divergence from an authored-`Group`-kind design)
//! `pg_grammar::model::MetathesisRuleDef` carries ONE compiled pattern (no separate LHS/RHS split,
//! no environments — C#'s `IPhonologicalPatternSubruleSpec.LeftEnvironmentMatcher`/
//! `RightEnvironmentMatcher` are hardcoded `null` for both Analysis/SynthesisMetathesisRuleSpec) plus
//! two switch positions (`left_switch`/`right_switch`, indices into `pattern.nodes`). An authored
//! `PatternNode::Group` kind (+ a `CompileNode::Group` case in `pg_rules::bridge::PatternBridge`)
//! could represent a switch, but this port does that lowering **post-hoc**
//! instead (`compile_switch_pattern`): compile the plain pattern via `PatternBridge` as usual, then
//! wrap the two switch positions' already-compiled nodes in a named `pg_fst::CompileNode::Group` and
//! recover their matched spans via `Fst::get_offsets` after a match — exactly the technique
//! `pg_rules::rewrite::compile_env_impl` already uses to recover alpha-variable positions, and the
//! same primitive `pg_rules::morph::compile_parts` uses for affix-part captures. This
//! is strictly less new surface (no model/bridge change at all) and is justified by a fact only
//! discovered while building fixtures for this rule: a real grammar's switch group is **always
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
//! `synthesis_reorder` physically swaps the two switch ranges (see its doc for the exact
//! node-identity algorithm — a faithful, not shortcut, port of `MoveNodesAfter`); a node strictly
//! between them keeps its slot untouched.
//!
//! ## Analysis (reverse, feature union)
//! Pattern REBUILT (`build_analysis_pattern`), physical-position-driven (see that function's own
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
//! shape nodes can be underspecified during analysis). On a match, `ana_union` bitwise-ORs the two
//! matched nodes' lanes onto each other (see its doc for why this equals C#'s `FeatureStruct.Union`
//! under this port's dense-lane representation) and resets both nodes' `char_def` identity to
//! `NO_CHAR_DEF` (mirroring `pg_rules::rewrite::syn_feature`'s identical, already-documented choice
//! for the same "a feature-changed node must stop being treated as a concrete, single-char-def-
//! identity node" reason).
//!
//! ## MPR/POS immunity
//! No subrule-level gating exists at all (see `pg_grammar::model::MetathesisRuleDef`'s doc) — every
//! `synthesize`/`analyze` call here always considers the rule applicable. Deliberately **not** pinned
//! by a dedicated test: the DTD's `<MetathesisRule>` has no `requiredMPRFeatures`/`excludedMPRFeatures`/
//! `requiredPartsOfSpeech` attribute at all, so there is no grammar any test could author that would
//! even attempt to gate one — this doc comment (and `MetathesisRuleDef`'s matching one) is the pin
//! instead, so a future "fix" adding an `MprSet`/POS parameter to `synthesize`/`analyze` here has to
//! first explain what XML would ever set it.

use pg_featstruct::flat_unifiable;
use pg_fst::{CompileNode, Direction, Fst, Segment, Transduce, ENTIRE_MATCH};
use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
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

// Pattern compilation.

/// A compiled metathesis pattern plus the traversal-relative anchor flags `match_candidates` must pass to `Transduce::anchored` (not simply the physical `anchor_start`/`anchor_end` `PatternBridge` reports).
struct CompiledSwitchPattern {
    fst: Fst,
    anchor_start: bool,
    anchor_end: bool,
}

/// Compile `pattern`, wrapping the compiled nodes at `left_idx`/`right_idx` in named capture groups so `match_candidates` can recover their matched spans; a `RightToLeft` pattern's nodes and anchors must be given to the compiler in physically-reversed order, since traversal index 0 is the physically last segment.
/// See docs/research/pg-rules-metathesis-design-notes.md for the verified direction-handling rationale.
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

/// Full-pattern-node-space index (`MetathesisRuleDef.left_switch`/`right_switch`'s own space, anchors included) → compiled index (anchors excluded); an anchor can only be the first or last element of `pattern.nodes`, so counting non-anchor nodes before `full_idx` is exact.
fn compiled_index(pattern: &Pattern, full_idx: u32) -> usize {
    non_anchor_count(&pattern.nodes[..full_idx as usize])
}

fn non_anchor_count(nodes: &[PatternNode]) -> usize {
    nodes
        .iter()
        .filter(|n| !matches!(n, PatternNode::Anchor(_)))
        .count()
}

/// Rebuild the search pattern analysis needs to recognize whatever `synthesis_reorder` actually produces, physical-position-first (matching `synthesis_reorder`'s real behavior, not C#'s tag-driven order); a middle node is dropped only if `is_boundary_node` says it can never appear in the analysis match sequence.
/// See docs/research/pg-rules-metathesis-design-notes.md for the full reordering rationale.
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

/// Whether `node` lowers to a `NodeKind::Boundary` shape node at segmentation time (only a literal `<Segment>`/`<BoundaryMarker>` resolving to a `CharDefKind::Boundary` char def); used by `build_analysis_pattern` to decide whether a middle node must be dropped or preserved.
fn is_boundary_node(g: &Grammar, table: TableId, node: &PatternNode) -> bool {
    match node {
        PatternNode::CharDef(id) => {
            g.char_tables[table.0 as usize].get(*id).kind() == CharDefKind::Boundary
        }
        _ => false,
    }
}

// Matching.

/// One accepted candidate: the two switch groups' matched (start,end) ranges in segment-index space (`ms.segs(...)`'s output space, not `ms.nodes` space -- see `seg_range_to_nodes`).
struct Candidate {
    entire: (usize, usize),
    left: (usize, usize),
    right: (usize, usize),
}

/// Every distinct match, direction-first ordered like `pg_rules::rewrite`'s own candidate list.
/// See `docs/research/pg-rules-metathesis-design-notes.md`.
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
    if fst.direction() == Direction::RightToLeft {
        out.reverse();
    }
    out.into_iter()
        .map(|(es, ee, ls, le, rs, re)| Candidate {
            entire: (es, ee),
            left: (ls, le),
            right: (rs, re),
        })
        .collect()
}

/// Translate a segment-index range (from a `Candidate`) to the `ms.nodes` indices it covers.
fn seg_range_to_nodes(node_of: &[usize], range: (usize, usize)) -> Vec<usize> {
    node_of[range.0..range.1].to_vec()
}

// Synthesis: physical reorder.

/// C# `SynthesisMetathesisRuleSpec.ApplyRhs`/`MoveNodesAfter`, ported literally rather than as a "swap the two ranges as blocks" shortcut, since a non-Segment node inside a captured range doesn't move but the loop's cursor still advances past it.
/// See docs/research/pg-rules-metathesis-design-notes.md for the full width->1 argument and the table-resolution rationale.
fn synthesis_reorder(ms: &mut MutShape, left: &[usize], right: &[usize], table: &CharDefTable) {
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

    // First: move the right switch's segments to just after the left switch's own last node.
    let left_end = *left_loc.last().expect("switch range non-empty");
    move_nodes_after(&mut order, &window, Some(left_end), &right_loc);

    // Second: move the left switch's segments to just after whatever originally preceded the right switch's start (`None` = insert at the very front).
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
                // Reset a relocated segment's `char_def` when re-interpreting it against the rule's own owning `table` no longer denotes a meaning-consistent entry (see docs/research/pg-rules-metathesis-design-notes.md for the single- vs multi-table argument this makes safe either way).
                let still_valid = n.char_def != pg_shape::NO_CHAR_DEF
                    && (n.char_def as usize) < table.len()
                    && flat_unifiable(&n.lanes, table.get(CharDefId(n.char_def)).feature_lanes());
                if !still_valid {
                    n.char_def = pg_shape::NO_CHAR_DEF;
                }
            }
            n
        })
        .collect();
    ms.nodes.splice(lo..hi, new_window);
}

/// One `MoveNodesAfter` call: walks `range` one node at a time; a Segment-typed node is removed and reinserted immediately after `cur`'s current position, a non-Segment node is never moved, and `cur` advances to that node's identity either way.
/// See docs/research/pg-rules-metathesis-design-notes.md for why that "advance even when not moving" detail matters, and the degenerate self-adjacent-range fallback.
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

// Analysis: feature union.

/// C# `AnalysisMetathesisRuleSpec.ApplyRhs`: for each Segment-typed `(leftNode, rightNode)` pair, union each node's `FeatureStruct` into the other's (a plain bitwise OR over dense lanes, exact given this port's UNCONSTRAINED=u64::MAX representation) and mark both dirty; also resets both nodes' `char_def`, this port's own addition since C#'s `Union` has no analogous per-node identity to reset.
/// See docs/research/pg-rules-metathesis-design-notes.md for the full union-semantics and char_def-reset argument.
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

// Public API (mirrors `pg_rules::rewrite::synthesize`/`analyze`'s signatures/return convention).

/// Apply `rule` forward to `input` (C# `SynthesisMetathesisRule.Apply`). Returns the rewritten shape
/// in a one-element vec if the rule applied, else empty. No MPR/POS gating exists for a metathesis
/// rule at all (see `MetathesisRuleDef`'s doc) — unlike `rewrite::synthesize_with_mpr`, there is no
/// `_with_mpr` sibling to call instead.
pub fn synthesize(g: &Grammar, rule: &MetathesisRuleDef, input: &Shape) -> Vec<Shape> {
    // Resolves `rule`'s own owning stratum's table rather than hardcoding `TableId(0)`.
    // See `docs/research/pg-rules-metathesis-design-notes.md`.
    let table_id = crate::cache::owning_table_for_metathesis_rule(g, rule).unwrap_or(TableId(0));
    let dir = dir_from_model(rule.dir);
    let left_idx = compiled_index(&rule.pattern, rule.left_switch);
    let right_idx = compiled_index(&rule.pattern, rule.right_switch);
    let compiled =
        compile_switch_pattern(g, table_id, &rule.pattern, left_idx, right_idx, dir, true);
    let pattern_len = non_anchor_count(&rule.pattern.nodes);
    // `table` is `synthesis_reorder`'s own per-node validity check's target -- see its doc.
    let table = &g.char_tables[table_id.0 as usize];
    synthesize_with_pattern(&compiled, pattern_len, input, table)
}

/// `g` is needed only to resolve `MetaCache::table_id` into the actual `CharDefTable`
/// `synthesis_reorder`'s own per-node validity check reads (see its doc); every OTHER input here is
/// already fully precompiled. Every production call site (`crate::stratum::StratumAnalyzer`) already
/// holds `self.g`, so this is a cheap, already-in-scope reference, not a new lookup.
pub(crate) fn synthesize_cached(
    g: &Grammar,
    rule: &MetathesisRuleDef,
    input: &Shape,
    cache: &MetaCache,
) -> Vec<Shape> {
    let pattern_len = non_anchor_count(&rule.pattern.nodes);
    let table = &g.char_tables[cache.table_id.0 as usize];
    synthesize_with_pattern(&cache.syn, pattern_len, input, table)
}

fn synthesize_with_pattern(
    pattern: &CompiledSwitchPattern,
    pattern_len: usize,
    input: &Shape,
    table: &CharDefTable,
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
            synthesis_reorder(&mut ms, &left_nodes, &right_nodes, table);
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

// Phonological rule tracing (metathesis, synthesis side).
// See `docs/research/pg-rules-metathesis-design-notes.md`.

/// `synthesize`'s traced sibling — standalone (recompiles every call, like `synthesize` itself).
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

/// The `MetaCache`-aware sibling of `synthesize_traced` — the real per-word pipeline's traced
/// entry point (`crate::stratum::synthesize_stratum_traced`'s trailing prule application, the
/// `PhonRuleDef::Metathesis` arm). Takes the real `&Word` so the trace snapshot carries the word's
/// actual full state and `node_parent` can fall back to `input.trace`, exactly like
/// `pg_rules::rewrite::synthesize_with_mpr_cached_traced`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn synthesize_cached_traced(
    g: &Grammar,
    pid: PRuleId,
    rule: &MetathesisRuleDef,
    input: &Word,
    cache: &MetaCache,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Shape> {
    let result = synthesize_cached(g, rule, &input.shape, cache);
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

/// Shared readout for the two synthesis-traced functions above.
/// See `docs/research/pg-rules-metathesis-design-notes.md`.
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
    // Same owning-table resolution and fallback contract as `synthesize`, for the un-apply direction.
    let table_id = crate::cache::owning_table_for_metathesis_rule(g, rule).unwrap_or(TableId(0));
    let dir = reverse(dir_from_model(rule.dir));
    let (ana_pattern, left_idx_full, right_idx_full) = build_analysis_pattern(
        g,
        table_id,
        &rule.pattern,
        rule.left_switch,
        rule.right_switch,
    );
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

// Phonological rule tracing (metathesis, analysis side).
// See `docs/research/pg-rules-metathesis-design-notes.md`.

/// `analyze`'s traced sibling — standalone (recompiles every call). `pid` is a nominal trace-tree
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

/// Shared readout for the two analysis-traced functions above; no `FailureReason` is ever attached, mirroring C# exactly.
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

// Compile-once cache (`crate::cache::RuleCache`'s metathesis-rule slice).

/// Per-metathesis-rule precompiled matchers (`crate::cache::RuleCache`'s analog of
/// `rewrite::PruleCache` for this rule kind).
pub(crate) struct MetaCache {
    syn: CompiledSwitchPattern,
    ana: CompiledSwitchPattern,
    /// `non_anchor_count` of the rebuilt analysis pattern's nodes, cached because the rebuild is `&Grammar`-aware and `analyze_cached` has no `&Grammar` in scope.
    ana_pattern_len: usize,
    /// The rule's own owning table, already resolved once by `crate::cache::owning_table_for_prule`.
    /// See `docs/research/pg-rules-metathesis-design-notes.md`.
    table_id: TableId,
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
    let (ana_pattern, ana_left_full, ana_right_full) = build_analysis_pattern(
        g,
        table_id,
        &rule.pattern,
        rule.left_switch,
        rule.right_switch,
    );
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
        table_id,
    }
}
