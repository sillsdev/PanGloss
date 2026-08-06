//! Rewrite (phonological) rule application: `synthesize` applies a rule forward, `analyze`
//! un-applies it. The three rewrite shapes — feature-change, deletion/narrowing, and epenthesis —
//! are dispatched by C#'s LHS-vs-RHS child-count rule.
//!
//! HermitCrab threads three engine-only symbolic features through matching that the frozen
//! `pg_shape`/`pg_fst` contracts do not encode as lanes, so each has a local stand-in:
//! - **`Type`** becomes `NodeKind` plus which nodes reach the matcher at all (synthesis feeds
//!   segments and boundaries, analysis only segments) plus the anchor endpoints;
//! - **`Modified`** becomes `MutNode::dirty`. The iterative loop's primary termination is still the
//!   cursor advance; `dirty` only provides the re-match guard C#'s `Modified=Clean` LHS gives;
//! - **`Deletion`** becomes `MutNode::deleted` on synthesis, a physical delete on analysis.
//!
//! `pg_fst` can bind neither alpha variables nor `UseDefaults` feature defaults, so a compiled FST
//! here is an over-approximation: every site that matches one must re-check the candidate span
//! against real node lanes afterwards.

use pg_featstruct::{FeatureStruct, FeatureValue};
use pg_fst::{
    CompileInput, CompileNode, Direction, Fst, FstResult, Segment, Transduce, ENTIRE_MATCH,
};
use pg_grammar::chardef::{CharDefKind, CharDefTable};
use pg_grammar::featsys::FlatIndex;
use pg_grammar::model::{
    Grammar, MprSet, NaturalClassKind, PRuleId, Pattern, PatternNode, RewriteMode, RewriteRuleDef,
    RewriteSubruleDef, StratumId, TableId,
};
use pg_shape::{NodeFlags, NodeKind, Shape, ShapeBuilder};

use crate::bridge::{pattern_var_occurrences, PatternBridge, VarOccur, UNCONSTRAINED};
use crate::trace::{FailureReason, TraceHandle, TraceSink};
use crate::word::Word;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

// Mutable working shape — C# `Shape` plus the Optional/Deleted/Dirty flags `pg_shape` omits.

// Crate-visible: `crate::metathesis` reuses this "resolve to concrete node data before mutating" discipline for its own synthesis reorder.
#[derive(Clone, Debug)]
pub(crate) struct MutNode {
    pub(crate) kind: NodeKind,
    pub(crate) char_def: u32,
    pub(crate) lanes: Vec<u64>,
    pub(crate) optional: bool,
    /// C# `Annotation.FeatureStruct[Deletion] == Deleted` (synthesis narrow marks then filters).
    pub(crate) deleted: bool,
    /// C# `Modified == Dirty` — set after a node is (un)rewritten so the iterative matcher, whose
    /// LHS carries `Modified=Clean`, will not re-match it.
    pub(crate) dirty: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct MutShape {
    pub(crate) w: usize,
    pub(crate) nodes: Vec<MutNode>,
}

impl MutShape {
    pub(crate) fn from_shape(s: &Shape) -> Self {
        let w = s.feat_width() as usize;
        let nodes = (0..s.len())
            .map(|i| MutNode {
                kind: s.kind(i),
                char_def: s.char_def(i),
                lanes: s.node_lanes(i).to_vec(),
                optional: s.flags(i).is_optional(),
                deleted: false,
                dirty: false,
            })
            .collect();
        MutShape { w, nodes }
    }

    /// Freeze back to an immutable `Shape`: drop deleted nodes, carry Optional (segments via the
    /// delete-then-reinsert workaround, since `ShapeBuilder` has no "set flags on existing node" —
    /// a flagged API gap; boundaries get Optional natively from `push_boundary`).
    pub(crate) fn to_shape(&self) -> Shape {
        let w = self.w as u32;
        let interior: Vec<&MutNode> = self
            .nodes
            .iter()
            .filter(|n| {
                !matches!(n.kind, NodeKind::LeftAnchor | NodeKind::RightAnchor) && !n.deleted
            })
            .collect();
        let mut b = ShapeBuilder::with_features_capacity(w, interior.len());
        for n in &interior {
            match n.kind {
                NodeKind::Segment => b.push_segment_with_lanes(n.char_def, &n.lanes),
                NodeKind::Boundary => b.push_boundary_with_lanes(n.char_def, &n.lanes),
                _ => unreachable!(),
            }
        }
        let mut shape = b.finish();
        // Segments that must be Optional: delete + reinsert with the OPTIONAL flag.
        let optional_positions: Vec<usize> = interior
            .iter()
            .enumerate()
            .filter(|(_, n)| n.optional && n.kind == NodeKind::Segment)
            .map(|(i, _)| i + 1) // +1 for the left anchor
            .collect();
        if !optional_positions.is_empty() {
            let mut m = ShapeBuilder::from_shape(&shape);
            for idx in optional_positions {
                let n = interior[idx - 1];
                m.delete(idx);
                m.insert(
                    idx,
                    NodeKind::Segment,
                    n.char_def,
                    NodeFlags(NodeFlags::OPTIONAL),
                    &n.lanes,
                );
            }
            shape = m.freeze();
        }
        shape
    }

    /// Build the FST segment sequence and node-index mapping under the matcher filter. Analysis
    /// excludes boundaries; synthesis adds them as optional segments. Deleted nodes are skipped.
    ///
    /// A `Segment`-kind node's own optional flag must also produce an optional segment, not just
    /// boundaries — see `crate::morph::segs_of` for the same requirement. It matters both for a
    /// LATER phonological rule re-scanning a shape this one marked, and for the morphological
    /// analysis that reuses those Optional segments off the frozen shape `analyze` returns.
    pub(crate) fn segs(&self, include_boundaries: bool) -> (Vec<Segment>, Vec<usize>) {
        let mut segs = Vec::new();
        let mut node_of = Vec::new();
        for (i, n) in self.nodes.iter().enumerate() {
            if n.deleted {
                continue;
            }
            match n.kind {
                NodeKind::Segment => {
                    segs.push(if n.optional {
                        Segment::optional(n.lanes.clone())
                    } else {
                        Segment::new(n.lanes.clone())
                    });
                    node_of.push(i);
                }
                NodeKind::Boundary if include_boundaries => {
                    segs.push(Segment::optional(n.lanes.clone()));
                    node_of.push(i);
                }
                _ => {}
            }
        }
        (segs, node_of)
    }
}

// Feature-constraint helpers (the "which features does this node pin, and to what" resolution).

fn full_mask(g: &Grammar, f: usize) -> u64 {
    g.phon_features.mask(FlatIndex(f as u32))
}

/// The `(feature, symbol-bits)` pairs a `Context` or `CharDef` pattern node **pins**. A feature is
/// pinned iff the node constrains it to a proper subset of its symbols; alpha-variable features
/// count as unpinned, since the compiled FST cannot bind them.
pub fn node_pins(g: &Grammar, table: &CharDefTable, node: &PatternNode) -> Vec<(usize, u64)> {
    let w = g.phon_features.len();
    match node {
        PatternNode::Context(sc) => {
            let alpha: HashSet<usize> = sc.vars.iter().map(|v| v.feature.0 as usize).collect();
            match &g.natural_classes[sc.nat_class.0 as usize].kind {
                NaturalClassKind::Feature(pairs) => pairs
                    .iter()
                    .filter(|(f, _)| !alpha.contains(&(f.0 as usize)))
                    .map(|(f, b)| (f.0 as usize, b.0))
                    .collect(),
                NaturalClassKind::Segments(segs) => (0..w)
                    .filter_map(|f| {
                        let bits = segs
                            .iter()
                            .fold(0u64, |acc, cd| acc | table.get(*cd).feature_lanes()[f]);
                        (bits != full_mask(g, f)).then_some((f, bits))
                    })
                    .collect(),
            }
        }
        PatternNode::CharDef(cd) => {
            let lanes = table.get(*cd).feature_lanes();
            (0..w)
                .filter(|&f| lanes[f] != full_mask(g, f))
                .map(|f| (f, lanes[f]))
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Full `W`-lane vector for a pattern node, unconstrained lanes being `full_mask` — the driver's
/// feature-math representation, distinct from the FST-facing `UNCONSTRAINED`.
pub fn node_full_lanes(g: &Grammar, table: &CharDefTable, node: &PatternNode) -> Vec<u64> {
    let w = g.phon_features.len();
    let mut lanes: Vec<u64> = (0..w).map(|f| full_mask(g, f)).collect();
    for (f, bits) in node_pins(g, table, node) {
        lanes[f] = bits;
    }
    lanes
}

// Matching (target + environments) on top of the frozen FST.

/// Compile a lane sequence (DOCUMENT order) to a target FST traversed in `dir`; the `RightToLeft` reversal must happen HERE because `pg_fst` never reorders internally, unlike C#'s matcher.
fn compile_lane_fst(lanes_seq: &[Vec<u64>], dir: Direction, deterministic: bool) -> Fst {
    let mut nodes: Vec<CompileNode> = lanes_seq
        .iter()
        .map(|l| CompileNode::Constraint(l.clone()))
        .collect();
    if dir == Direction::RightToLeft {
        nodes.reverse();
    }
    CompileInput::new(nodes)
        .deterministic(deterministic)
        .compile_with_direction(dir)
}

/// `compile_lane_fst`, but wraps each row in a named group ("g0".."g{N-1}", stable across the right-to-left reorder) so a caller can recover which segment each row consumed; only a group's START offset is trustworthy, since an Optional-skip during traversal can widen a group's END tag instead.
fn compile_lane_fst_grouped(
    lanes_seq: &[Vec<u64>],
    dir: Direction,
    deterministic: bool,
) -> (Fst, Vec<String>) {
    let names: Vec<String> = (0..lanes_seq.len()).map(|i| format!("g{i}")).collect();
    let mut nodes: Vec<CompileNode> = lanes_seq
        .iter()
        .zip(&names)
        .map(|(l, name)| CompileNode::Group {
            name: name.clone(),
            children: vec![CompileNode::Constraint(l.clone())],
        })
        .collect();
    if dir == Direction::RightToLeft {
        nodes.reverse(); // physical traversal order only -- each Group node keeps its own `name`.
    }
    let fst = CompileInput::new(nodes)
        .deterministic(deterministic)
        .compile_with_direction(dir);
    (fst, names)
}

/// Shared width-mismatch guard. A nondeterministic match over a `segs` sequence containing Optional
/// segments can report an `ENTIRE_MATCH` span WIDER than the pattern that matched: the traversal's
/// "skip the next Optional annotation" branch reuses the same arc two segments ahead, and the
/// registers it writes record the skipped segment's extent too. Any call site that then indexes a
/// per-pattern-position array positionally must reject such a span first — for `ana_epenthesis`
/// that means a silent wrong mutation, for `syn_feature`/`syn_narrow` an out-of-bounds panic. A
/// too-narrow span cannot occur, since every arc consumes at least one segment, hence equality.
///
/// **Residual.** The guard assumes a tight, exactly-`pattern_len`-wide alternative always survives
/// alongside the over-wide one. That is false when an earlier rule's unapply has interposed an
/// Optional segment between every candidate pair of this pattern's real target positions — then no
/// tight alternative exists and the match is lost. `ana_feature` sidesteps this entirely by
/// recovering its rows through `compile_lane_fst_grouped`'s per-row captures; the three remaining
/// callers are exposed in principle. Fixing one means giving it the same group-capture treatment.
///
/// **A bounded `Quantifier` spanning the whole LHS or RHS is deliberately unsupported.** Such a
/// pattern has one node whatever its min/max, so every caller's plain node count rejects any real
/// match wider than one segment, and the quantifier's grouping is invisible to this machinery
/// rather than merely mis-measured. There is no C# behavior to converge on: its rule-spec
/// constructors cast every LHS/RHS child to a constraint type that a quantifier does not inherit
/// from, so C# throws on load for the same shape, even though its own DTD and loader permit it.
/// Loading it without crashing is the deliberate choice. Environments are the contrasting case —
/// there a quantifier is a pure existence test with no positional array, C# handles it, and so
/// does this module.
#[inline]
pub(crate) fn width_matches(target_nodes: &[usize], pattern_len: usize) -> bool {
    target_nodes.len() == pattern_len
}

/// All match spans (in segment-position space) of `fst` over `segs`, sorted ascending, deduped.
pub(crate) fn all_spans(fst: &Fst, segs: &[Segment]) -> Vec<(usize, usize)> {
    if segs.is_empty() {
        return Vec::new();
    }
    let results = Transduce::new(fst, segs.to_vec()).all_matches();
    let mut spans: Vec<(usize, usize)> = results
        .iter()
        .filter_map(|r| fst.get_offsets(ENTIRE_MATCH, &r.registers))
        .map(|(a, b)| (a as usize, b as usize))
        .collect();
    spans.sort_unstable();
    spans.dedup();
    spans
}

/// `all_spans`, reordered to the direction-side-first scan order an Iterative pick-one-then-rescan loop needs; keyed off `target.direction()` so synthesis (compiled in the rule's direction) and analysis (compiled reversed) both scan correctly through one function.
fn ordered_spans(target: &Fst, segs: &[Segment]) -> Vec<(usize, usize)> {
    let mut spans = all_spans(target, segs);
    if target.direction() == Direction::RightToLeft {
        spans.reverse();
    }
    spans
}

/// A compiled environment, already lifted from a model `Pattern` with its anchors as flags. Also
/// serves `crate::validity`'s allomorph-environment gate, which needs the same anchored
/// suffix/prefix matching — one shared XML shape feeds both.
pub(crate) struct EnvFst {
    fst: Fst,
    anchor_start: bool,
    anchor_end: bool,
    /// The env is a bare word-boundary anchor (`#`) with no segment constraints.
    only_anchor: bool,
    /// Per-top-level-pattern-node alpha-variable occurrences; a quantifier's own entry is always empty, since nested-in-quantifier variables are a separate limitation of `pattern_var_occurrences`.
    node_vars: Vec<Vec<VarOccur>>,
    /// The capture-group name for each var-bearing `node_vars` entry (`None` if unwrapped); recovers which segment a quantifier elsewhere in the pattern let that node consume, since compilation erases variable-governed lanes before the FST is built and C# needs no such recovery (its arcs bind variables live).
    group_names: Vec<Option<String>>,
}

/// Compile an environment for **synthesis**, and for the allomorph-environment gate, which shares
/// C#'s segment-boundary-anchor filter: boundary-marker constraints are kept verbatim, since C#
/// passes both environments straight through unstripped. Analysis callers must use
/// `compile_env_analysis` instead — see its doc for why.
pub(crate) fn compile_env(g: &Grammar, table_id: TableId, env: Option<&Pattern>) -> Option<EnvFst> {
    compile_env_impl(g, table_id, env, false, false)
}

/// `compile_env` with the `StrRep` identity lane enabled, for **allomorph** environments only,
/// whose match inputs come from `crate::morph::segs_of` and carry the same lane. Phonological-rule
/// environments must keep plain `compile_env`: their inputs are the rewrite driver's own lane-less
/// node lanes, and an id-lane constraint against those mis-fires on determinized negated arcs.
///
/// The split is not merely about precision. Allomorph environments feed the disjunctive re-check,
/// where an environment that OVER-matches flips into wrongly REJECTING the word — a passed-over
/// allomorph "matching" spuriously kills every parse through it.
pub(crate) fn compile_env_allomorph(
    g: &Grammar,
    table_id: TableId,
    env: Option<&Pattern>,
) -> Option<EnvFst> {
    compile_env_impl(g, table_id, env, false, true)
}

/// Compile an environment for phonological **analysis**, stripping boundary constraints as C#'s
/// analysis subrule spec does.
///
/// The analysis matcher's filter admits segments and anchors only, so a boundary node never reaches
/// the traversal at all and a literal boundary constraint left in the pattern could never match. C#
/// drops the constraint entirely, which — with its matcher transparently stepping over physical
/// boundaries mid-traversal — makes a morpheme boundary invisible during analysis-side environment
/// matching: "delete before a boundary then `a`" degenerates to "the next real segment is `a`".
///
/// This port's `ana_*` functions mirror the same filter by building their match universe with
/// boundaries excluded. Without this stripping, any analysis subrule whose environment mentions a
/// morpheme boundary silently fails every time.
pub(crate) fn compile_env_analysis(
    g: &Grammar,
    table_id: TableId,
    env: Option<&Pattern>,
) -> Option<EnvFst> {
    compile_env_impl(g, table_id, env, true, false)
}

fn compile_env_impl(
    g: &Grammar,
    table_id: TableId,
    env: Option<&Pattern>,
    strip_boundaries: bool,
    id_lane: bool,
) -> Option<EnvFst> {
    let env = env?;
    let stripped;
    let nodes: &[PatternNode] = if strip_boundaries {
        let table = &g.char_tables[table_id.0 as usize];
        stripped = strip_boundary_nodes(table, &env.nodes);
        &stripped
    } else {
        &env.nodes
    };
    if nodes.is_empty() {
        return None; // C#: an empty (or fully boundary-stripped) environment pattern installs no matcher.
    }
    let owned_pattern;
    let pat_ref: &Pattern = if strip_boundaries {
        owned_pattern = Pattern {
            nodes: nodes.to_vec(),
        };
        &owned_pattern
    } else {
        env
    };
    let bridge = PatternBridge::new(g).with_table(table_id).id_lane(id_lane);
    let mut compiled = bridge
        .compile_pattern(pat_ref)
        .expect("environment compiles");
    let only_anchor = compiled.top_level_len == 0 && (compiled.anchor_start || compiled.anchor_end);

    // Wrap each var-bearing top-level node in a named capture group so its matched segment is recoverable independent of any quantifier elsewhere in the pattern; see `EnvFst::group_names`.
    let group_names: Vec<Option<String>> = compiled
        .node_vars
        .iter()
        .enumerate()
        .map(|(i, occs)| {
            if occs.is_empty() {
                None
            } else {
                Some(format!("av{i}"))
            }
        })
        .collect();
    for (i, name) in group_names.iter().enumerate() {
        if let Some(name) = name {
            let child = std::mem::replace(
                &mut compiled.input.nodes[i],
                CompileNode::Constraint(Vec::new()),
            );
            compiled.input.nodes[i] = CompileNode::Group {
                name: name.clone(),
                children: vec![child],
            };
        }
    }

    Some(EnvFst {
        fst: compiled
            .input
            .compile_with_direction(Direction::LeftToRight),
        anchor_start: compiled.anchor_start,
        anchor_end: compiled.anchor_end,
        only_anchor,
        node_vars: compiled.node_vars,
        group_names,
    })
}

/// C# `DeepCloneExceptBoundaries`: drops nodes denoting a literal boundary char-def, recursing into quantifiers; a pre-segmented `Segments` node could embed a boundary too, but that shape only occurs on allomorphs, which take the unstripped `compile_env` path instead.
fn strip_boundary_nodes(table: &CharDefTable, nodes: &[PatternNode]) -> Vec<PatternNode> {
    nodes
        .iter()
        .filter_map(|n| match n {
            PatternNode::CharDef(cd) if table.get(*cd).kind() == CharDefKind::Boundary => None,
            PatternNode::Quantifier { min, max, children } => {
                let filtered = strip_boundary_nodes(table, children);
                if filtered.is_empty() {
                    None
                } else {
                    Some(PatternNode::Quantifier {
                        min: *min,
                        max: *max,
                        children: filtered,
                    })
                }
            }
            other => Some(other.clone()),
        })
        .collect()
}

/// Left environment holds iff some suffix of `segs[0..left_end]`, ending adjacent to the target,
/// matches the env; a bare `#` left env holds iff the target is at the word start.
///
/// The nested `Option` is so `resolve_bindings` can reuse this same traversal's registers instead
/// of re-running the FST. Outer `None` means the environment failed — reject the candidate;
/// `Some(None)` means none was authored, a vacuous pass with nothing to bind; `Some(Some(r))` means
/// matched, with `r.registers` holding the capture groups.
pub(crate) fn left_env_match(
    env: &Option<EnvFst>,
    segs: &[Segment],
    left_end: usize,
) -> Option<Option<FstResult>> {
    let Some(env) = env else { return Some(None) };
    if env.only_anchor {
        return if left_end == 0 { Some(None) } else { None };
    }
    if left_end == 0 {
        return None; // no left context for a segment-bearing env to match
    }
    let slice = segs[..left_end].to_vec();
    Transduce::new(&env.fst, slice)
        .anchored(env.anchor_start, true)
        .first_match()
        .map(Some)
}

/// Bool projection of `left_env_match` for callers that don't need alpha-variable bindings
/// (the narrow/epenthesis rule shapes, whose reference-grammar instances use no variables).
pub(crate) fn left_env_ok(env: &Option<EnvFst>, segs: &[Segment], left_end: usize) -> bool {
    left_env_match(env, segs, left_end).is_some()
}

/// Right environment holds iff a prefix of `segs[right_start..]`, starting adjacent to the target,
/// matches the env; a bare `#` right env holds iff the target is at the word end. See
/// `left_env_match` for the nested-`Option` shape.
pub(crate) fn right_env_match(
    env: &Option<EnvFst>,
    segs: &[Segment],
    right_start: usize,
) -> Option<Option<FstResult>> {
    let Some(env) = env else { return Some(None) };
    if env.only_anchor {
        return if right_start == segs.len() {
            Some(None)
        } else {
            None
        };
    }
    if right_start >= segs.len() {
        return None;
    }
    let slice = segs[right_start..].to_vec();
    Transduce::new(&env.fst, slice)
        .anchored(true, env.anchor_end)
        .first_match()
        .map(Some)
}

/// Bool projection of `right_env_match` for callers that don't need alpha-variable bindings.
pub(crate) fn right_env_ok(env: &Option<EnvFst>, segs: &[Segment], right_start: usize) -> bool {
    right_env_match(env, segs, right_start).is_some()
}

// Alpha-variable agreement: the FST cannot bind variables, so the real check runs against node lanes after a candidate span is reported, in C#'s target-then-left-then-right binding order.

/// Bindings: `VarId.0` → (bound symbol bits, governing feature index, kept for the disagree/negation mask).
type Bindings = HashMap<u16, (u64, usize)>;

/// One agreement step (C# `SimpleFeatureValue.IsUnifiableImpl` variable arm): binds on first sight, else checks agreement; returns `false` only when a bound variable is violated.
fn bind_or_check(g: &Grammar, bindings: &mut Bindings, occ: &VarOccur, node_bits: u64) -> bool {
    let mask = full_mask(g, occ.feature);
    match bindings.get(&occ.var) {
        None => {
            // GetVariableValue(Agree): agree → the node's set; disagree → its negation within mask.
            let bound = if occ.plus {
                node_bits
            } else {
                mask & !node_bits
            };
            bindings.insert(occ.var, (bound, occ.feature));
            true
        }
        Some(&(b, _)) => {
            // binding.Overlaps(!Agree, node): negate the binding within the mask when disagree.
            let eff = if occ.plus { b } else { mask & !b };
            eff & node_bits != 0
        }
    }
}

/// Resolve alpha-variable bindings for a candidate in C#'s `MatchSubrule` order (target, then left env, then right env); `None` means a bound variable was violated and the candidate must be rejected.
#[allow(clippy::too_many_arguments)]
fn resolve_bindings(
    g: &Grammar,
    ms: &MutShape,
    node_of: &[usize],
    target_nodes: &[usize],
    e: usize,
    lhs_vars: &[Vec<VarOccur>],
    left: &Option<EnvFst>,
    left_match: &Option<FstResult>,
    right: &Option<EnvFst>,
    right_match: &Option<FstResult>,
) -> Option<Bindings> {
    let mut bindings: Bindings = HashMap::default();

    // (1) target nodes, in match order.
    for (k, occs) in lhs_vars.iter().enumerate() {
        let node = target_nodes[k];
        for occ in occs {
            if !bind_or_check(g, &mut bindings, occ, ms.nodes[node].lanes[occ.feature]) {
                return None;
            }
        }
    }

    // (2) left environment: each var-bearing node's matched segment comes from its capture group (`EnvFst::group_names`), immune to a variable-width quantifier elsewhere shifting segment counts; the env FST is `LeftToRight` over `segs[..s]`, so a captured offset is already absolute.
    if let (Some(env), Some(result)) = (left, left_match) {
        for (i, occs) in env.node_vars.iter().enumerate() {
            if occs.is_empty() {
                continue;
            }
            let name = env.group_names[i]
                .as_deref()
                .expect("a var-bearing node was wrapped in a capture group at compile time");
            let Some((a, _b)) = env.fst.get_offsets(name, &result.registers) else {
                // Zero-width/unset capture: fail open (skip) rather than mis-bind against a stale node; not expected for the only node kind that carries `node_vars`.
                continue;
            };
            let pos = a as usize;
            let node = node_of[pos];
            for occ in occs {
                if !bind_or_check(g, &mut bindings, occ, ms.nodes[node].lanes[occ.feature]) {
                    return None;
                }
            }
        }
    }

    // (3) right environment: same capture-based recovery over `segs[e..]`; add `e` to convert a slice-relative offset back into `segs`/`node_of` space.
    if let (Some(env), Some(result)) = (right, right_match) {
        for (i, occs) in env.node_vars.iter().enumerate() {
            if occs.is_empty() {
                continue;
            }
            let name = env.group_names[i]
                .as_deref()
                .expect("a var-bearing node was wrapped in a capture group at compile time");
            let Some((a, _b)) = env.fst.get_offsets(name, &result.registers) else {
                continue;
            };
            let pos = e + a as usize;
            if pos >= node_of.len() {
                continue;
            }
            let node = node_of[pos];
            for occ in occs {
                if !bind_or_check(g, &mut bindings, occ, ms.nodes[node].lanes[occ.feature]) {
                    return None;
                }
            }
        }
    }

    Some(bindings)
}

/// docs/research/rewrite-usedefaults-confirm.md
/// The Feature-kind confirm step for C#'s `UseDefaults` matcher flag, which `pg_fst` cannot itself apply.
fn pattern_defaults_ok(
    g: &Grammar,
    ms: &MutShape,
    target_nodes: &[usize],
    pattern_lanes: &[Vec<u64>],
) -> bool {
    for (k, row) in pattern_lanes.iter().enumerate() {
        let node = target_nodes[k];
        for (f, &bits) in row.iter().enumerate() {
            let mask = full_mask(g, f);
            if bits == mask {
                continue; // unpinned at this position -- nothing for UseDefaults to confirm
            }
            if ms.nodes[node].lanes[f] == mask {
                if let Some(default_bits) = g.phon_features.default_bits(FlatIndex(f as u32)) {
                    if default_bits & bits == 0 {
                        return false;
                    }
                }
            }
        }
    }
    true
}

// Subrule dispatch (C# LHS-vs-RHS child-count rule).

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Kind {
    Feature,
    Narrow, // deletion (rhs empty) or narrowing/expansion
    Epenthesis,
}

fn classify(rule: &RewriteRuleDef, sr: &RewriteSubruleDef) -> Kind {
    let t = rule.lhs.nodes.len();
    let r = sr.rhs.nodes.len();
    if t == 0 {
        Kind::Epenthesis
    } else if t == r {
        Kind::Feature
    } else {
        Kind::Narrow
    }
}

/// C# `SynthesisRewriteSubruleSpec.IsApplicable`: required-POS plus required/excluded MPR gating,
/// checked dynamically against the *word currently being synthesized* — not a static property of
/// the subrule. Treating either half as static makes every subrule declaring one unconditionally
/// inapplicable, which is silent: the subrule simply never fires, and the resynthesized surface
/// then cannot equal the input, so the whole parse fails however correct the rest of it was.
///
/// **Synthesis only.** C#'s analysis-side subrule spec does not override the base `IsApplicable`,
/// which returns true unconditionally, so unapplication is never MPR- or POS-gated. `analyze` must
/// not call this.
///
/// `pub` because `pg_foma` calls it at grammar-compile time to partition lexical entries into
/// groups agreeing on every gated subrule. Reusing the engine's own predicate is what makes the
/// two paths provably agree rather than re-deriving MPR match-types and the POS vacuous-pass rule.
pub fn subrule_applicable(
    g: &Grammar,
    sr: &RewriteSubruleDef,
    syn_fs: &FeatureStruct,
    mpr: MprSet,
) -> bool {
    required_pos_ok(g, &sr.required_pos, syn_fs)
        && g.mpr_group_ok(sr.required_mpr, sr.excluded_mpr, mpr)
}

/// The POS half of `subrule_applicable`: vacuously satisfied unless both sides are present, in which case they must share at least one symbol; the mask argument is unused on that arm, hence the `0`.
fn required_pos_ok(
    g: &Grammar,
    required_pos: &Option<pg_featstruct::SymbolBits>,
    syn_fs: &FeatureStruct,
) -> bool {
    let Some(req) = required_pos else { return true };
    match syn_fs.get(g.syn_features.pos) {
        None => true,
        Some(FeatureValue::Symbolic(bits)) => bits.overlaps(false, *req, false, 0),
        Some(FeatureValue::Complex(_)) => {
            debug_assert!(
                false,
                "the syntactic POS feature must be symbolic, never complex"
            );
            true
        }
    }
}

// Public API.

/// Apply `rule` forward to `input`: the rewritten shape in a one-element vec if it applied, empty
/// otherwise. A thin wrapper passing empty MPR and syntactic FS, so every subrule gate is vacuously
/// satisfied. A caller that needs real gating must use `synthesize_with_mpr` directly.
pub fn synthesize(g: &Grammar, rule: &RewriteRuleDef, input: &Shape) -> Vec<Shape> {
    synthesize_with_mpr(g, rule, input, &FeatureStruct::EMPTY, MprSet::EMPTY)
}

/// Identical to `synthesize`, but gates each subrule against the synthesizing word's actual
/// syntactic FS and MPR set rather than assuming empty ones. See `subrule_applicable`.
///
/// Recompiles every subrule's matchers per call, deliberately: this entry point is also used on
/// standalone, non-grammar-resident fixtures with no index into a `RuleCache`. The real pipeline
/// calls `synthesize_with_mpr_cached`.
pub fn synthesize_with_mpr(
    g: &Grammar,
    rule: &RewriteRuleDef,
    input: &Shape,
    syn_fs: &FeatureStruct,
    mpr: MprSet,
) -> Vec<Shape> {
    let table_id = TableId(0);
    let table = &g.char_tables[table_id.0 as usize];
    let mut ms = MutShape::from_shape(input);
    let mut applied = false;

    for sr in &rule.subrules {
        if !subrule_applicable(g, sr, syn_fs, mpr) {
            continue;
        }
        // `rule.mode` selects the function pair for Feature/Narrow; Epenthesis reuses `syn_epenthesis` for both modes, since its collect-then-apply shape already matches Simultaneous and stands in for Iterative.
        let did = match (classify(rule, sr), rule.mode) {
            (Kind::Feature, RewriteMode::Iterative) => {
                let target = lhs_fst(g, table_id, &rule.lhs, dir_of(rule), true);
                let left = compile_env(g, table_id, sr.left_env.as_ref());
                let right = compile_env(g, table_id, sr.right_env.as_ref());
                syn_feature(g, table, rule, sr, &mut ms, &target, &left, &right)
            }
            (Kind::Feature, RewriteMode::Simultaneous) => {
                let target = lhs_fst(g, table_id, &rule.lhs, dir_of(rule), true);
                let left = compile_env(g, table_id, sr.left_env.as_ref());
                let right = compile_env(g, table_id, sr.right_env.as_ref());
                sim_feature(g, table, rule, sr, &mut ms, &target, &left, &right)
            }
            (Kind::Narrow, RewriteMode::Iterative) => {
                let target = lhs_fst(g, table_id, &rule.lhs, dir_of(rule), true);
                let left = compile_env(g, table_id, sr.left_env.as_ref());
                let right = compile_env(g, table_id, sr.right_env.as_ref());
                syn_narrow(g, table, rule, sr, &mut ms, &target, &left, &right)
            }
            (Kind::Narrow, RewriteMode::Simultaneous) => {
                let target = lhs_fst(g, table_id, &rule.lhs, dir_of(rule), true);
                let left = compile_env(g, table_id, sr.left_env.as_ref());
                let right = compile_env(g, table_id, sr.right_env.as_ref());
                sim_narrow(g, table, rule, sr, &mut ms, &target, &left, &right)
            }
            (Kind::Epenthesis, _) => {
                let left = compile_env(g, table_id, sr.left_env.as_ref());
                let right = compile_env(g, table_id, sr.right_env.as_ref());
                syn_epenthesis(g, table, sr, &mut ms, &left, &right)
            }
        };
        applied |= did;
    }

    if applied {
        vec![ms.to_shape()]
    } else {
        Vec::new()
    }
}

/// The cache-aware sibling of `synthesize_with_mpr`, used by the real pipeline: every
/// target/environment matcher is read from the cache instead of recompiled. `pid` must identify
/// `rule` — every production call site indexed `g.prules` by `pid` to get it in the first place.
pub(crate) fn synthesize_with_mpr_cached(
    g: &Grammar,
    pid: pg_grammar::model::PRuleId,
    rule: &RewriteRuleDef,
    input: &Shape,
    syn_fs: &FeatureStruct,
    mpr: MprSet,
    cache: &crate::cache::RuleCache,
) -> Vec<Shape> {
    // `pid` resolves this rule's own owning-stratum table, never an implicit table zero; the fallback applies only to an orphaned prule.
    let table_id = crate::cache::owning_table_for_prule(g, pid).unwrap_or(TableId(0));
    let table = &g.char_tables[table_id.0 as usize];
    let mut ms = MutShape::from_shape(input);
    let mut applied = false;
    let pc = cache.prule_rewrite(pid);

    for (i, sr) in rule.subrules.iter().enumerate() {
        if !subrule_applicable(g, sr, syn_fs, mpr) {
            continue;
        }
        let sc = &pc.subrules[i];
        // The cached matchers are identical for either mode of a given `Kind`; only the driving loop differs between Iterative and Simultaneous.
        let did = match (classify(rule, sr), rule.mode) {
            (Kind::Feature, RewriteMode::Iterative) => syn_feature(
                g,
                table,
                rule,
                sr,
                &mut ms,
                pc.syn_target
                    .as_ref()
                    .expect("Feature/Narrow subrule always has a compiled syn target"),
                &sc.syn_left,
                &sc.syn_right,
            ),
            (Kind::Feature, RewriteMode::Simultaneous) => sim_feature(
                g,
                table,
                rule,
                sr,
                &mut ms,
                pc.syn_target
                    .as_ref()
                    .expect("Feature/Narrow subrule always has a compiled syn target"),
                &sc.syn_left,
                &sc.syn_right,
            ),
            (Kind::Narrow, RewriteMode::Iterative) => syn_narrow(
                g,
                table,
                rule,
                sr,
                &mut ms,
                pc.syn_target
                    .as_ref()
                    .expect("Feature/Narrow subrule always has a compiled syn target"),
                &sc.syn_left,
                &sc.syn_right,
            ),
            (Kind::Narrow, RewriteMode::Simultaneous) => sim_narrow(
                g,
                table,
                rule,
                sr,
                &mut ms,
                pc.syn_target
                    .as_ref()
                    .expect("Feature/Narrow subrule always has a compiled syn target"),
                &sc.syn_left,
                &sc.syn_right,
            ),
            (Kind::Epenthesis, _) => {
                syn_epenthesis(g, table, sr, &mut ms, &sc.syn_left, &sc.syn_right)
            }
        };
        applied |= did;
    }

    if applied {
        vec![ms.to_shape()]
    } else {
        Vec::new()
    }
}

// C# `SynthesisRewriteRule.Apply` traces post-hoc from a per-subrule-index side channel (gate-failure reason, or success) rather than inline; readout reports the FIRST successful index and stops, mirroring the same gate order `subrule_applicable` checks.

/// One subrule's outcome — this port's concrete stand-in for C#'s `CurrentRuleResults[i]` side channel, always populated since every subrule is visited exactly once.
#[derive(Clone, Copy)]
enum SubruleOutcome {
    Applied,
    NotApplied(FailureReason),
}

/// Re-derives `subrule_applicable`'s three gates separately so a caller can report WHICH one failed; `None` means every gate passed.
fn subrule_gate_reason(
    g: &Grammar,
    sr: &RewriteSubruleDef,
    syn_fs: &FeatureStruct,
    mpr: MprSet,
) -> Option<FailureReason> {
    if !required_pos_ok(g, &sr.required_pos, syn_fs) {
        return Some(FailureReason::RequiredSyntacticFeatureStruct);
    }
    if !pg_grammar::model::mpr_required_ok(&g.mpr_groups, sr.required_mpr, mpr) {
        return Some(FailureReason::RequiredMprFeatures);
    }
    if !pg_grammar::model::mpr_excluded_ok(&g.mpr_groups, sr.excluded_mpr, mpr) {
        return Some(FailureReason::ExcludedMprFeatures);
    }
    None
}

/// Fires trace events for every subrule in index order, stopping at the first applied one, passing the SAME final-state `out_word` snapshot to every call — a verified C# quirk: even a failed subrule reports the rule's end state, not the state when it was tried.
fn report_subrule_outcomes(
    trace: &dyn TraceSink,
    parent: TraceHandle,
    pid: PRuleId,
    outcomes: &[SubruleOutcome],
    out_word: &Word,
) {
    for (i, outcome) in outcomes.iter().enumerate() {
        match outcome {
            SubruleOutcome::Applied => {
                trace.phonological_rule_applied(parent, pid, i as i32, out_word);
                break;
            }
            SubruleOutcome::NotApplied(reason) => {
                trace.phonological_rule_not_applied(parent, pid, i as i32, out_word, *reason);
            }
        }
    }
}

/// `synthesize_with_mpr`'s traced sibling, recompiling per call for hand-built fixtures with no
/// `RuleCache` index. `pid` serves only as the trace tree's rule identity, so a fixture with no
/// grammar-resident prule table may pass any nominal value. With no live `Word` to draw a cursor
/// from, the caller's `parent` is used as-is and the trace snapshot is built internally.
#[allow(clippy::too_many_arguments)]
pub fn synthesize_with_mpr_traced(
    g: &Grammar,
    pid: PRuleId,
    rule: &RewriteRuleDef,
    input: &Shape,
    syn_fs: &FeatureStruct,
    mpr: MprSet,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Shape> {
    if !trace.is_tracing() {
        return synthesize_with_mpr(g, rule, input, syn_fs, mpr);
    }
    // Owning-table resolution: see `synthesize_with_mpr_cached`.
    let table_id = crate::cache::owning_table_for_prule(g, pid).unwrap_or(TableId(0));
    let table = &g.char_tables[table_id.0 as usize];
    let mut ms = MutShape::from_shape(input);
    let mut applied = false;
    let mut outcomes: Vec<SubruleOutcome> = Vec::with_capacity(rule.subrules.len());

    for sr in &rule.subrules {
        if let Some(reason) = subrule_gate_reason(g, sr, syn_fs, mpr) {
            outcomes.push(SubruleOutcome::NotApplied(reason));
            continue;
        }
        // Same `(Kind, rule.mode)` dispatch as `synthesize_with_mpr`, recompiled per call.
        let did = match (classify(rule, sr), rule.mode) {
            (Kind::Feature, RewriteMode::Iterative) => {
                let target = lhs_fst(g, table_id, &rule.lhs, dir_of(rule), true);
                let left = compile_env(g, table_id, sr.left_env.as_ref());
                let right = compile_env(g, table_id, sr.right_env.as_ref());
                syn_feature(g, table, rule, sr, &mut ms, &target, &left, &right)
            }
            (Kind::Feature, RewriteMode::Simultaneous) => {
                let target = lhs_fst(g, table_id, &rule.lhs, dir_of(rule), true);
                let left = compile_env(g, table_id, sr.left_env.as_ref());
                let right = compile_env(g, table_id, sr.right_env.as_ref());
                sim_feature(g, table, rule, sr, &mut ms, &target, &left, &right)
            }
            (Kind::Narrow, RewriteMode::Iterative) => {
                let target = lhs_fst(g, table_id, &rule.lhs, dir_of(rule), true);
                let left = compile_env(g, table_id, sr.left_env.as_ref());
                let right = compile_env(g, table_id, sr.right_env.as_ref());
                syn_narrow(g, table, rule, sr, &mut ms, &target, &left, &right)
            }
            (Kind::Narrow, RewriteMode::Simultaneous) => {
                let target = lhs_fst(g, table_id, &rule.lhs, dir_of(rule), true);
                let left = compile_env(g, table_id, sr.left_env.as_ref());
                let right = compile_env(g, table_id, sr.right_env.as_ref());
                sim_narrow(g, table, rule, sr, &mut ms, &target, &left, &right)
            }
            (Kind::Epenthesis, _) => {
                let left = compile_env(g, table_id, sr.left_env.as_ref());
                let right = compile_env(g, table_id, sr.right_env.as_ref());
                syn_epenthesis(g, table, sr, &mut ms, &left, &right)
            }
        };
        applied |= did;
        outcomes.push(if did {
            SubruleOutcome::Applied
        } else {
            SubruleOutcome::NotApplied(FailureReason::Pattern)
        });
    }

    let out_shape = if applied {
        ms.to_shape()
    } else {
        input.clone()
    };
    let mut out_word = Word::new(out_shape.clone(), StratumId(0));
    out_word.syn_fs = syn_fs.clone();
    out_word.mpr = mpr;
    report_subrule_outcomes(trace, parent, pid, &outcomes, &out_word);

    if applied {
        vec![out_shape]
    } else {
        Vec::new()
    }
}

/// The cache-aware sibling of `synthesize_with_mpr_traced`, and the real pipeline's traced entry
/// point. It takes a whole `&Word` rather than a shape/FS/MPR triple so the trace snapshot carries
/// the word's real state and the cursor can fall back to `input.trace` like every other call site.
pub fn synthesize_with_mpr_cached_traced(
    g: &Grammar,
    pid: PRuleId,
    rule: &RewriteRuleDef,
    input: &Word,
    cache: &crate::cache::RuleCache,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Shape> {
    if !trace.is_tracing() {
        return synthesize_with_mpr_cached(
            g,
            pid,
            rule,
            &input.shape,
            &input.syn_fs,
            input.mpr,
            cache,
        );
    }
    // Owning-table resolution: see `synthesize_with_mpr_cached`.
    let table_id = crate::cache::owning_table_for_prule(g, pid).unwrap_or(TableId(0));
    let table = &g.char_tables[table_id.0 as usize];
    let mut ms = MutShape::from_shape(&input.shape);
    let mut applied = false;
    let pc = cache.prule_rewrite(pid);
    let mut outcomes: Vec<SubruleOutcome> = Vec::with_capacity(rule.subrules.len());

    for (i, sr) in rule.subrules.iter().enumerate() {
        if let Some(reason) = subrule_gate_reason(g, sr, &input.syn_fs, input.mpr) {
            outcomes.push(SubruleOutcome::NotApplied(reason));
            continue;
        }
        let sc = &pc.subrules[i];
        // Same `(Kind, rule.mode)` dispatch as `synthesize_with_mpr_cached`.
        let did = match (classify(rule, sr), rule.mode) {
            (Kind::Feature, RewriteMode::Iterative) => syn_feature(
                g,
                table,
                rule,
                sr,
                &mut ms,
                pc.syn_target
                    .as_ref()
                    .expect("Feature/Narrow subrule always has a compiled syn target"),
                &sc.syn_left,
                &sc.syn_right,
            ),
            (Kind::Feature, RewriteMode::Simultaneous) => sim_feature(
                g,
                table,
                rule,
                sr,
                &mut ms,
                pc.syn_target
                    .as_ref()
                    .expect("Feature/Narrow subrule always has a compiled syn target"),
                &sc.syn_left,
                &sc.syn_right,
            ),
            (Kind::Narrow, RewriteMode::Iterative) => syn_narrow(
                g,
                table,
                rule,
                sr,
                &mut ms,
                pc.syn_target
                    .as_ref()
                    .expect("Feature/Narrow subrule always has a compiled syn target"),
                &sc.syn_left,
                &sc.syn_right,
            ),
            (Kind::Narrow, RewriteMode::Simultaneous) => sim_narrow(
                g,
                table,
                rule,
                sr,
                &mut ms,
                pc.syn_target
                    .as_ref()
                    .expect("Feature/Narrow subrule always has a compiled syn target"),
                &sc.syn_left,
                &sc.syn_right,
            ),
            (Kind::Epenthesis, _) => {
                syn_epenthesis(g, table, sr, &mut ms, &sc.syn_left, &sc.syn_right)
            }
        };
        applied |= did;
        outcomes.push(if did {
            SubruleOutcome::Applied
        } else {
            SubruleOutcome::NotApplied(FailureReason::Pattern)
        });
    }

    let out_shape = if applied {
        ms.to_shape()
    } else {
        input.shape.clone()
    };
    let mut out_word = input.clone();
    out_word.shape = out_shape.clone();
    let node_parent = input.trace.unwrap_or(parent);
    report_subrule_outcomes(trace, node_parent, pid, &outcomes, &out_word);

    if applied {
        vec![out_shape]
    } else {
        Vec::new()
    }
}

/// Un-apply `rule` to `input` (C# `AnalysisRewriteRule.Apply`). Returns the un-applied shape in a
/// one-element vec if any subrule un-applied, else an empty vec.
///
/// Recompiles on every call — see `synthesize_with_mpr`'s doc for why (standalone test fixtures
/// with no grammar-resident index). The real pipeline calls `analyze_cached`.
pub fn analyze(g: &Grammar, rule: &RewriteRuleDef, input: &Shape) -> Vec<Shape> {
    let table_id = TableId(0);
    let table = &g.char_tables[table_id.0 as usize];
    let mut ms = MutShape::from_shape(input);
    let mut applied = false;

    for sr in &rule.subrules {
        // `sr.self_opaquing` (Feature/Epenthesis only, always false for Narrow) gates a repeat-until-fixpoint loop, matching C#'s repeat-until-no-change.
        let did = match classify(rule, sr) {
            Kind::Feature => {
                let target_lanes = ana_feature_target_lanes(g, table, rule, sr);
                let (target, names) =
                    compile_lane_fst_grouped(&target_lanes, reverse(dir_of(rule)), false);
                let left = compile_env_analysis(g, table_id, sr.left_env.as_ref());
                let right = compile_env_analysis(g, table_id, sr.right_env.as_ref());
                if sr.self_opaquing {
                    let mut any = false;
                    while ana_feature(g, table, rule, sr, &mut ms, &target, &names, &left, &right) {
                        any = true;
                    }
                    any
                } else {
                    ana_feature(g, table, rule, sr, &mut ms, &target, &names, &left, &right)
                }
            }
            Kind::Narrow => {
                let left = compile_env_analysis(g, table_id, sr.left_env.as_ref());
                let right = compile_env_analysis(g, table_id, sr.right_env.as_ref());
                if sr.rhs.nodes.is_empty() {
                    ana_narrow_deletion(g, table, rule, sr, &mut ms, &left, &right)
                } else {
                    // Reuse the epenthesis target-lane formula ("the RHS segment sequence, FST-facing"); see `ana_narrow_general`'s doc.
                    let lanes = ana_epenthesis_target_lanes(g, table, sr);
                    let target = compile_lane_fst(&lanes, reverse(dir_of(rule)), false);
                    ana_narrow_general(g, table, rule, sr, &mut ms, &target, &left, &right)
                }
            }
            Kind::Epenthesis => {
                let target_lanes = ana_epenthesis_target_lanes(g, table, sr);
                let target = (!target_lanes.is_empty())
                    .then(|| compile_lane_fst(&target_lanes, reverse(dir_of(rule)), false));
                let left = compile_env_analysis(g, table_id, sr.left_env.as_ref());
                let right = compile_env_analysis(g, table_id, sr.right_env.as_ref());
                if sr.self_opaquing {
                    let mut any = false;
                    while ana_epenthesis(
                        &mut ms,
                        target.as_ref(),
                        sr.rhs.nodes.len(),
                        &left,
                        &right,
                    ) {
                        any = true;
                    }
                    any
                } else {
                    ana_epenthesis(&mut ms, target.as_ref(), sr.rhs.nodes.len(), &left, &right)
                }
            }
        };
        applied |= did;
    }

    if applied {
        vec![ms.to_shape()]
    } else {
        Vec::new()
    }
}

/// The cache-aware sibling of `analyze`, used by the real pipeline. See `synthesize_with_mpr_cached`
/// for the `pid`/`rule` correspondence contract.
pub(crate) fn analyze_cached(
    g: &Grammar,
    pid: pg_grammar::model::PRuleId,
    rule: &RewriteRuleDef,
    input: &Shape,
    cache: &crate::cache::RuleCache,
) -> Vec<Shape> {
    // Owning-table resolution: see `synthesize_with_mpr_cached`.
    let table_id = crate::cache::owning_table_for_prule(g, pid).unwrap_or(TableId(0));
    let table = &g.char_tables[table_id.0 as usize];
    let mut ms = MutShape::from_shape(input);
    let mut applied = false;
    let pc = cache.prule_rewrite(pid);

    for (i, sr) in rule.subrules.iter().enumerate() {
        let sc = &pc.subrules[i];
        // Same `self_opaquing` repeat-wrapper as `analyze` (§4.4) -- see that function's doc.
        let did = match classify(rule, sr) {
            Kind::Feature => {
                let target = sc
                    .ana_target
                    .as_ref()
                    .expect("Feature subrule always has a compiled ana target");
                let names = sc
                    .ana_target_names
                    .as_ref()
                    .expect("Feature subrule always has compiled ana target group names");
                if sr.self_opaquing {
                    let mut any = false;
                    while ana_feature(
                        g,
                        table,
                        rule,
                        sr,
                        &mut ms,
                        target,
                        names,
                        &sc.ana_left,
                        &sc.ana_right,
                    ) {
                        any = true;
                    }
                    any
                } else {
                    ana_feature(
                        g,
                        table,
                        rule,
                        sr,
                        &mut ms,
                        target,
                        names,
                        &sc.ana_left,
                        &sc.ana_right,
                    )
                }
            }
            Kind::Narrow => {
                if sr.rhs.nodes.is_empty() {
                    ana_narrow_deletion(g, table, rule, sr, &mut ms, &sc.ana_left, &sc.ana_right)
                } else {
                    ana_narrow_general(
                        g,
                        table,
                        rule,
                        sr,
                        &mut ms,
                        sc.ana_target
                            .as_ref()
                            .expect("Narrow-general subrule always has a compiled ana target"),
                        &sc.ana_left,
                        &sc.ana_right,
                    )
                }
            }
            Kind::Epenthesis => {
                if sr.self_opaquing {
                    let mut any = false;
                    while ana_epenthesis(
                        &mut ms,
                        sc.ana_target.as_ref(),
                        sr.rhs.nodes.len(),
                        &sc.ana_left,
                        &sc.ana_right,
                    ) {
                        any = true;
                    }
                    any
                } else {
                    ana_epenthesis(
                        &mut ms,
                        sc.ana_target.as_ref(),
                        sr.rhs.nodes.len(),
                        &sc.ana_left,
                        &sc.ana_right,
                    )
                }
            }
        };
        applied |= did;
    }

    if applied {
        vec![ms.to_shape()]
    } else {
        Vec::new()
    }
}

// Phonological rule tracing, analysis side: unlike synthesis's post-hoc readout, C# traces INLINE per subrule (no `FailureReason`, since analysis has no MPR/POS gate to attribute a failure to).

/// `analyze`'s traced sibling, recompiling per call. `pid` is a nominal trace-tree identity for
/// fixtures with no grammar-resident prule table. No `&Word` is needed, analysis having no gate to
/// carry, so the caller's `parent` is used as-is.
pub fn analyze_traced(
    g: &Grammar,
    pid: PRuleId,
    rule: &RewriteRuleDef,
    input: &Shape,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Shape> {
    if !trace.is_tracing() {
        return analyze(g, rule, input);
    }
    // Owning-table resolution: see `synthesize_with_mpr_cached`.
    let table_id = crate::cache::owning_table_for_prule(g, pid).unwrap_or(TableId(0));
    let table = &g.char_tables[table_id.0 as usize];
    let mut ms = MutShape::from_shape(input);
    let mut applied = false;

    for (i, sr) in rule.subrules.iter().enumerate() {
        let did = match classify(rule, sr) {
            Kind::Feature => {
                let target_lanes = ana_feature_target_lanes(g, table, rule, sr);
                let (target, names) =
                    compile_lane_fst_grouped(&target_lanes, reverse(dir_of(rule)), false);
                let left = compile_env_analysis(g, table_id, sr.left_env.as_ref());
                let right = compile_env_analysis(g, table_id, sr.right_env.as_ref());
                if sr.self_opaquing {
                    let mut any = false;
                    while ana_feature(g, table, rule, sr, &mut ms, &target, &names, &left, &right) {
                        any = true;
                    }
                    any
                } else {
                    ana_feature(g, table, rule, sr, &mut ms, &target, &names, &left, &right)
                }
            }
            Kind::Narrow => {
                let left = compile_env_analysis(g, table_id, sr.left_env.as_ref());
                let right = compile_env_analysis(g, table_id, sr.right_env.as_ref());
                if sr.rhs.nodes.is_empty() {
                    ana_narrow_deletion(g, table, rule, sr, &mut ms, &left, &right)
                } else {
                    let lanes = ana_epenthesis_target_lanes(g, table, sr);
                    let target = compile_lane_fst(&lanes, reverse(dir_of(rule)), false);
                    ana_narrow_general(g, table, rule, sr, &mut ms, &target, &left, &right)
                }
            }
            Kind::Epenthesis => {
                let target_lanes = ana_epenthesis_target_lanes(g, table, sr);
                let target = (!target_lanes.is_empty())
                    .then(|| compile_lane_fst(&target_lanes, reverse(dir_of(rule)), false));
                let left = compile_env_analysis(g, table_id, sr.left_env.as_ref());
                let right = compile_env_analysis(g, table_id, sr.right_env.as_ref());
                if sr.self_opaquing {
                    let mut any = false;
                    while ana_epenthesis(
                        &mut ms,
                        target.as_ref(),
                        sr.rhs.nodes.len(),
                        &left,
                        &right,
                    ) {
                        any = true;
                    }
                    any
                } else {
                    ana_epenthesis(&mut ms, target.as_ref(), sr.rhs.nodes.len(), &left, &right)
                }
            }
        };
        applied |= did;
        let snap = Word::new(ms.to_shape(), StratumId(0));
        if did {
            trace.phonological_rule_unapplied(parent, pid, i as i32, &snap);
        } else {
            trace.phonological_rule_not_unapplied(parent, pid, i as i32, &snap);
        }
    }

    if applied {
        vec![ms.to_shape()]
    } else {
        Vec::new()
    }
}

/// The cache-aware sibling of `analyze_traced`, called by the analysis stratum driver's prule
/// sweep. An untracing sink short-circuits straight back to `analyze_cached`.
pub fn analyze_cached_traced(
    g: &Grammar,
    pid: PRuleId,
    rule: &RewriteRuleDef,
    input: &Shape,
    cache: &crate::cache::RuleCache,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Shape> {
    if !trace.is_tracing() {
        return analyze_cached(g, pid, rule, input, cache);
    }
    // Owning-table resolution: see `synthesize_with_mpr_cached`.
    let table_id = crate::cache::owning_table_for_prule(g, pid).unwrap_or(TableId(0));
    let table = &g.char_tables[table_id.0 as usize];
    let mut ms = MutShape::from_shape(input);
    let mut applied = false;
    let pc = cache.prule_rewrite(pid);

    for (i, sr) in rule.subrules.iter().enumerate() {
        let sc = &pc.subrules[i];
        let did = match classify(rule, sr) {
            Kind::Feature => {
                let target = sc
                    .ana_target
                    .as_ref()
                    .expect("Feature subrule always has a compiled ana target");
                let names = sc
                    .ana_target_names
                    .as_ref()
                    .expect("Feature subrule always has compiled ana target group names");
                if sr.self_opaquing {
                    let mut any = false;
                    while ana_feature(
                        g,
                        table,
                        rule,
                        sr,
                        &mut ms,
                        target,
                        names,
                        &sc.ana_left,
                        &sc.ana_right,
                    ) {
                        any = true;
                    }
                    any
                } else {
                    ana_feature(
                        g,
                        table,
                        rule,
                        sr,
                        &mut ms,
                        target,
                        names,
                        &sc.ana_left,
                        &sc.ana_right,
                    )
                }
            }
            Kind::Narrow => {
                if sr.rhs.nodes.is_empty() {
                    ana_narrow_deletion(g, table, rule, sr, &mut ms, &sc.ana_left, &sc.ana_right)
                } else {
                    ana_narrow_general(
                        g,
                        table,
                        rule,
                        sr,
                        &mut ms,
                        sc.ana_target
                            .as_ref()
                            .expect("Narrow-general subrule always has a compiled ana target"),
                        &sc.ana_left,
                        &sc.ana_right,
                    )
                }
            }
            Kind::Epenthesis => {
                if sr.self_opaquing {
                    let mut any = false;
                    while ana_epenthesis(
                        &mut ms,
                        sc.ana_target.as_ref(),
                        sr.rhs.nodes.len(),
                        &sc.ana_left,
                        &sc.ana_right,
                    ) {
                        any = true;
                    }
                    any
                } else {
                    ana_epenthesis(
                        &mut ms,
                        sc.ana_target.as_ref(),
                        sr.rhs.nodes.len(),
                        &sc.ana_left,
                        &sc.ana_right,
                    )
                }
            }
        };
        applied |= did;
        let snap = Word::new(ms.to_shape(), StratumId(0));
        if did {
            trace.phonological_rule_unapplied(parent, pid, i as i32, &snap);
        } else {
            trace.phonological_rule_not_unapplied(parent, pid, i as i32, &snap);
        }
    }

    if applied {
        vec![ms.to_shape()]
    } else {
        Vec::new()
    }
}

// Feature-change (LHS.count == RHS.count).

/// C# `FeatureSynthesisRewriteSubruleSpec.ApplyRhs`: match the LHS, then priority-union each RHS constraint onto the matched node's features (`b` wins); Iterative + `Modified=Clean` ⇒ each node is rewritten at most once.
#[allow(clippy::too_many_arguments)]
fn syn_feature(
    g: &Grammar,
    table: &CharDefTable,
    rule: &RewriteRuleDef,
    sr: &RewriteSubruleDef,
    ms: &mut MutShape,
    target: &Fst,
    left: &Option<EnvFst>,
    right: &Option<EnvFst>,
) -> bool {
    let rhs_pins: Vec<Vec<(usize, u64)>> = sr
        .rhs
        .nodes
        .iter()
        .map(|n| node_pins(g, table, n))
        .collect();
    // The LHS's full per-position lane rows (not sparse pins), needed by `pattern_defaults_ok` to tell "pinned to X" apart from "unpinned" by mask comparison.
    let lhs_lanes: Vec<Vec<u64>> = rule
        .lhs
        .nodes
        .iter()
        .map(|n| node_full_lanes(g, table, n))
        .collect();
    let lhs_vars = pattern_var_occurrences(&rule.lhs);
    let rhs_vars = pattern_var_occurrences(&sr.rhs);

    let mut applied = false;
    loop {
        let (segs, node_of) = ms.segs(true);
        // First span in the target's own scan order whose nodes are all clean and whose environments hold; see `ordered_spans`.
        let mut acted = false;
        for (s, e) in ordered_spans(target, &segs) {
            let target_nodes: Vec<usize> = node_of[s..e].to_vec();
            // Reject an over-wide Optional-skip artifact before the positional `rhs_pins[k]` index below, which would otherwise panic on a multi-node target abutting a boundary.
            if !width_matches(&target_nodes, rhs_pins.len()) {
                continue;
            }
            if target_nodes
                .iter()
                .any(|&n| ms.nodes[n].dirty || ms.nodes[n].kind != NodeKind::Segment)
            {
                continue;
            }
            let Some(left_match) = left_env_match(left, &segs, s) else {
                continue;
            };
            let Some(right_match) = right_env_match(right, &segs, e) else {
                continue;
            };
            // Alpha-variable agreement over target + environments: the frozen FST over-approximated variable lanes, so reject any candidate that violates a binding.
            let Some(bindings) = resolve_bindings(
                g,
                ms,
                &node_of,
                &target_nodes,
                e,
                &lhs_vars,
                left,
                &left_match,
                right,
                &right_match,
            ) else {
                continue;
            };
            // UseDefaults confirm: reject a candidate the FST only accepted because an LHS-pinned feature is unspecified on the node and its default wouldn't have satisfied the pin; see `pattern_defaults_ok`.
            if !pattern_defaults_ok(g, ms, &target_nodes, &lhs_lanes) {
                continue;
            }
            // ApplyRhs: priority-union each RHS constraint onto the target node, then apply RHS alpha variables from the resolved bindings (C# `PriorityUnion` + `ReplaceVariables`).
            for (k, &node) in target_nodes.iter().enumerate() {
                for &(f, bits) in &rhs_pins[k] {
                    ms.nodes[node].lanes[f] = bits;
                }
                for occ in &rhs_vars[k] {
                    if let Some(&(b, _)) = bindings.get(&occ.var) {
                        let mask = full_mask(g, occ.feature);
                        ms.nodes[node].lanes[occ.feature] = if occ.plus { b } else { mask & !b };
                    }
                }
                ms.nodes[node].dirty = true;
                // Rewriting a node's features breaks the correspondence between its literal `char_def` and its current lanes (C#'s `GetMatchingStrReps` re-derives from current features every time); clear to `u32::MAX` so lookup falls back to lane unification instead of the stale literal's own fixed representations — untouched nodes keep their identity lock, so this can't reopen the empty-lanes match-everything bug that lock exists to prevent.
                ms.nodes[node].char_def = u32::MAX;
            }
            applied = true;
            acted = true;
            break;
        }
        if !acted {
            break;
        }
    }
    applied
}

/// The Simultaneous sibling of `syn_feature`: collects every accepted candidate against ONE pristine snapshot (C# `SimultaneousPhonologicalPatternRule.Apply`'s `AllMatches` then `ApplyRhs`-all), then applies them; a node dirtied by an earlier subrule of this same rule is still excluded.
#[allow(clippy::too_many_arguments)]
fn sim_feature(
    g: &Grammar,
    table: &CharDefTable,
    rule: &RewriteRuleDef,
    sr: &RewriteSubruleDef,
    ms: &mut MutShape,
    target: &Fst,
    left: &Option<EnvFst>,
    right: &Option<EnvFst>,
) -> bool {
    let rhs_pins: Vec<Vec<(usize, u64)>> = sr
        .rhs
        .nodes
        .iter()
        .map(|n| node_pins(g, table, n))
        .collect();
    let lhs_lanes: Vec<Vec<u64>> = rule
        .lhs
        .nodes
        .iter()
        .map(|n| node_full_lanes(g, table, n))
        .collect();
    let lhs_vars = pattern_var_occurrences(&rule.lhs);
    let rhs_vars = pattern_var_occurrences(&sr.rhs);

    // ONE snapshot before any mutation: every candidate's match is checked against it (C#: `Matcher.AllMatches` + `MatchSubrule`, both run before any `ApplyRhs`).
    let (segs, node_of) = ms.segs(true);
    let mut accepted: Vec<(Vec<usize>, Bindings)> = Vec::new();
    for (s, e) in all_spans(target, &segs) {
        let target_nodes: Vec<usize> = node_of[s..e].to_vec();
        if !width_matches(&target_nodes, rhs_pins.len()) {
            continue;
        }
        // `dirty` still gates out a node an earlier subrule of this same rule already touched; nothing within this single pass can have gone dirty yet, since collection precedes all of this call's own applications.
        if target_nodes
            .iter()
            .any(|&n| ms.nodes[n].dirty || ms.nodes[n].kind != NodeKind::Segment)
        {
            continue;
        }
        let Some(left_match) = left_env_match(left, &segs, s) else {
            continue;
        };
        let Some(right_match) = right_env_match(right, &segs, e) else {
            continue;
        };
        let Some(bindings) = resolve_bindings(
            g,
            ms,
            &node_of,
            &target_nodes,
            e,
            &lhs_vars,
            left,
            &left_match,
            right,
            &right_match,
        ) else {
            continue;
        };
        if !pattern_defaults_ok(g, ms, &target_nodes, &lhs_lanes) {
            continue;
        }
        accepted.push((target_nodes, bindings));
    }
    if accepted.is_empty() {
        return false;
    }
    // THEN apply every accepted candidate, mutating progressively like C#'s shared-word apply loop (only the MATCHING phase above is snapshot-based) — observable only for overlapping target spans.
    for (target_nodes, bindings) in accepted {
        for (k, &node) in target_nodes.iter().enumerate() {
            for &(f, bits) in &rhs_pins[k] {
                ms.nodes[node].lanes[f] = bits;
            }
            for occ in &rhs_vars[k] {
                if let Some(&(b, _)) = bindings.get(&occ.var) {
                    let mask = full_mask(g, occ.feature);
                    ms.nodes[node].lanes[occ.feature] = if occ.plus { b } else { mask & !b };
                }
            }
            ms.nodes[node].dirty = true;
            // See `syn_feature`'s identical step for the full char_def-staleness rationale.
            ms.nodes[node].char_def = u32::MAX;
        }
    }
    true
}

/// The `ana_feature` target FST's per-node lanes (FST-facing `LHS ⊕ RHS` priority-union), factored out so cache construction compiles this target once rather than per call.
fn ana_feature_target_lanes(
    g: &Grammar,
    table: &CharDefTable,
    rule: &RewriteRuleDef,
    sr: &RewriteSubruleDef,
) -> Vec<Vec<u64>> {
    let rhs_vars = pattern_var_occurrences(&sr.rhs);
    rule.lhs
        .nodes
        .iter()
        .zip(&sr.rhs.nodes)
        .enumerate()
        .map(|(k, (lhs_n, rhs_n))| {
            let mut lanes = node_full_lanes(g, table, lhs_n);
            for (f, bits) in node_pins(g, table, rhs_n) {
                lanes[f] = bits;
            }
            for occ in &rhs_vars[k] {
                lanes[occ.feature] = full_mask(g, occ.feature);
            }
            to_fst_lanes(g, &lanes)
        })
        .collect()
}

/// C# `FeatureAnalysisRewriteRuleSpec`: the analysis target is the `LHS ⊕ RHS` priority-union (reversed direction, nondeterministic); must use `compile_lane_fst_grouped` and its per-row groups, never a positional `node_of[s..e]` slice — an earlier rule's vacuous unapply can interpose an Optional segment between two real target positions, making every candidate over-wide and the rule silently unapply nothing.
#[allow(clippy::too_many_arguments)]
fn ana_feature(
    g: &Grammar,
    table: &CharDefTable,
    rule: &RewriteRuleDef,
    sr: &RewriteSubruleDef,
    ms: &mut MutShape,
    target: &Fst,
    names: &[String],
    left: &Option<EnvFst>,
    right: &Option<EnvFst>,
) -> bool {
    // RHS alpha-variable occurrences, positionally aligned to `sr.rhs.nodes` (needed by the target pattern, see `ana_feature_target_lanes`, and the changed-feature set below).
    let rhs_vars = pattern_var_occurrences(&sr.rhs);

    // Recomputed rather than threaded in: the same recompile-per-call tradeoff `analyze` makes.
    let target_lanes = ana_feature_target_lanes(g, table, rule, sr);

    // The features each RHS node changed, paired with bits to OR onto the node's value on unapply: C# reduces to `L | R`, NOT a full-mask reset, since resetting would wrongly accept a third symbol neither side mentions on a feature with more than two symbols; an alpha-governed feature keeps the full-mask fallback and is still listed as changed so a vacuous-looking literal pin still fires.
    let changed: Vec<Vec<(usize, u64)>> = rule
        .lhs
        .nodes
        .iter()
        .zip(&sr.rhs.nodes)
        .enumerate()
        .map(|(k, (lhs_n, rhs_n))| {
            let lhs_lanes = node_full_lanes(g, table, lhs_n);
            let mut fs: Vec<(usize, u64)> = node_pins(g, table, rhs_n)
                .into_iter()
                .filter(|&(f, bits)| lhs_lanes[f] != bits)
                .map(|(f, bits)| (f, lhs_lanes[f] & !bits))
                .collect();
            for occ in &rhs_vars[k] {
                if !fs.iter().any(|&(f, _)| f == occ.feature) {
                    fs.push((occ.feature, full_mask(g, occ.feature)));
                }
            }
            fs
        })
        .collect();

    // The analysis target is `LHS ⊕ RHS`; a variable governing a *changed* feature no longer pins a matchable lane, so unapply agreement is keyed off LHS variables surviving on unchanged features plus environment variables.
    let lhs_vars = pattern_var_occurrences(&rule.lhs);

    // Which tag half is trustworthy is DIRECTION-DEPENDENT: LTR keeps a row's START fresh, RTL's `get_offsets` swap makes the END fresh instead — and the LTR branch has no full-pipeline coverage (this target always compiles reversed from the rule's own direction), pinned only by `group_probe_diag`'s unit test.
    let rtl = target.direction() == Direction::RightToLeft;
    let recover_pos = |name: &str, regs: &[pg_fst::Register]| -> Option<usize> {
        target
            .get_offsets(name, regs)
            .map(|(a, b)| if rtl { (b - 1) as usize } else { a as usize })
    };

    let mut applied = false;
    loop {
        let (segs, node_of) = ms.segs(false); // analysis filter: Segment|Anchor (no boundaries)
        let mut acted = false;

        // `filter_map` fails open on an unresolved group offset; sort+dedup gives a stable ascending order, then `rtl` reverses it to descending when this target compiles `RightToLeft` — the same direction-first scan order `ordered_spans` gives its `(s, e)`-tuple callers.
        let mut candidates: Vec<Vec<usize>> = Transduce::new(target, segs.clone())
            .all_matches()
            .iter()
            .filter_map(|r| {
                names
                    .iter()
                    .map(|name| recover_pos(name, &r.registers))
                    .collect::<Option<Vec<usize>>>()
            })
            .collect();
        candidates.sort_unstable();
        candidates.dedup();
        if rtl {
            candidates.reverse();
        }

        for row_starts in candidates {
            let target_nodes: Vec<usize> = row_starts.iter().map(|&pos| node_of[pos]).collect();
            let s = row_starts[0];
            let e = row_starts[row_starts.len() - 1] + 1;
            if target_nodes.iter().any(|&n| ms.nodes[n].dirty) {
                continue;
            }
            let Some(left_match) = left_env_match(left, &segs, s) else {
                continue;
            };
            let Some(right_match) = right_env_match(right, &segs, e) else {
                continue;
            };
            // Alpha-variable agreement (C# threads `match.VariableBindings` through the analysis matcher and env matchers exactly as synthesis does); reject violating candidates.
            if resolve_bindings(
                g,
                ms,
                &node_of,
                &target_nodes,
                e,
                &lhs_vars,
                left,
                &left_match,
                right,
                &right_match,
            )
            .is_none()
            {
                continue;
            }
            // UseDefaults confirm, same as `syn_feature`'s analogous call, but against the analysis target's combined `LHS ⊕ RHS` lanes; see `pattern_defaults_ok`.
            if !pattern_defaults_ok(g, ms, &target_nodes, &target_lanes) {
                continue;
            }
            // IsUnapplicationNonvacuous (C# `FeatureAnalysisRewriteRuleSpec.cs`): nonvacuous iff some changed feature's current value does not already superset the bits being OR'd in — i.e. the OR would actually add bits, not merely "isn't already fully unconstrained".
            let nonvacuous = target_nodes.iter().enumerate().any(|(k, &node)| {
                changed[k]
                    .iter()
                    .any(|&(f, neg)| ms.nodes[node].lanes[f] & neg != neg)
            });
            if !nonvacuous {
                continue;
            }
            for (k, &node) in target_nodes.iter().enumerate() {
                for &(f, neg) in &changed[k] {
                    ms.nodes[node].lanes[f] |= neg;
                }
                ms.nodes[node].dirty = true;
                // Mirrors `syn_feature`'s identical `char_def` clearing (see its doc): an unapplied node's literal identity would otherwise survive the lane-widening above and mismatch its now-current lanes, so `RootAllomorphIndex::search`'s char_def-equality gate could never find a root whose underlying segment differs from the surface. Clearing to `NO_CHAR_DEF` falls through to `CdSet::Unrestricted`, matching C#'s always-re-derived `GetMatchingStrReps`.
                ms.nodes[node].char_def = pg_shape::NO_CHAR_DEF;
            }
            applied = true;
            acted = true;
            break;
        }
        if !acted {
            break;
        }
    }
    applied
}

// Narrow: deletion (RHS empty) / narrowing / expansion.

/// C# `NarrowSynthesisRewriteSubruleSpec.ApplyRhs`: insert the RHS nodes after `range.End`, then delete the target nodes (a pure deletion rule, RHS empty, just deletes them).
#[allow(clippy::too_many_arguments)]
fn syn_narrow(
    g: &Grammar,
    table: &CharDefTable,
    rule: &RewriteRuleDef,
    sr: &RewriteSubruleDef,
    ms: &mut MutShape,
    target: &Fst,
    left: &Option<EnvFst>,
    right: &Option<EnvFst>,
) -> bool {
    // C# `AddAfter` inserts each RHS node non-optional then dirties it separately — never skippable; `rhs_nodes_base` is the per-match template (var-governed lanes unresolved), cloned and resolved fresh per accepted match.
    let rhs_nodes_base: Vec<MutNode> = sr
        .rhs
        .nodes
        .iter()
        .map(|n| new_seg_node_dirty(g, table, n, false, true))
        .collect();
    // The RHS gets the same alpha-variable resolution `syn_feature` does; skipping it would leave a narrowing RHS natural class bound from a merged LHS segment sitting at full-unconstrained.
    let lhs_vars = pattern_var_occurrences(&rule.lhs);
    let rhs_vars = pattern_var_occurrences(&sr.rhs);

    let mut applied = false;
    loop {
        let (segs, node_of) = ms.segs(true);
        let mut acted = false;
        // Direction-ordered scan; see `ordered_spans`.
        for (s, e) in ordered_spans(target, &segs) {
            let target_nodes: Vec<usize> = node_of[s..e].to_vec();
            // An over-wide Optional-skip span would delete more physical nodes than the LHS matched — a silent wrong mutation here (no positional array to overrun into a panic); see `width_matches`.
            if !width_matches(&target_nodes, rule.lhs.nodes.len()) {
                continue;
            }
            // Widened to allow a matched Boundary node (Amharic `prule7`'s LHS has a real top-level `BoundaryMarker`): C#'s synthesis matcher filter is `Segment|Boundary|Anchor`, so its narrowing target can consume one too.
            if target_nodes.iter().any(|&n| {
                ms.nodes[n].dirty
                    || !matches!(ms.nodes[n].kind, NodeKind::Segment | NodeKind::Boundary)
            }) {
                continue;
            }
            let Some(left_match) = left_env_match(left, &segs, s) else {
                continue;
            };
            let Some(right_match) = right_env_match(right, &segs, e) else {
                continue;
            };
            // Alpha-variable agreement over target + environments (mirrors `syn_feature`'s identical step); also the source of bindings the RHS resolution below reads.
            let Some(bindings) = resolve_bindings(
                g,
                ms,
                &node_of,
                &target_nodes,
                e,
                &lhs_vars,
                left,
                &left_match,
                right,
                &right_match,
            ) else {
                continue;
            };
            let rhs_nodes: Vec<MutNode> = rhs_nodes_base
                .iter()
                .cloned()
                .zip(&rhs_vars)
                .map(|(mut n, occs)| {
                    for occ in occs {
                        if let Some(&(b, _)) = bindings.get(&occ.var) {
                            let mask = full_mask(g, occ.feature);
                            n.lanes[occ.feature] = if occ.plus { b } else { mask & !b };
                        }
                    }
                    n
                })
                .collect();
            let last = *target_nodes.last().unwrap();
            // Insert RHS right after the last target node.
            ms.nodes.splice(last + 1..last + 1, rhs_nodes);
            // Delete the target nodes (descending index to keep positions valid).
            for &n in target_nodes.iter().rev() {
                ms.nodes.remove(n);
            }
            applied = true;
            acted = true;
            break;
        }
        if !acted {
            break;
        }
    }
    applied
}

/// The Simultaneous sibling of `syn_narrow`: collect against one snapshot like `sim_feature`, but apply DESCENDING by match start — narrowing changes node COUNT, so an ascending earlier splice/delete would shift a later match's captured indices, unlike `sim_feature`'s in-place content rewrites.
#[allow(clippy::too_many_arguments)]
fn sim_narrow(
    g: &Grammar,
    table: &CharDefTable,
    rule: &RewriteRuleDef,
    sr: &RewriteSubruleDef,
    ms: &mut MutShape,
    target: &Fst,
    left: &Option<EnvFst>,
    right: &Option<EnvFst>,
) -> bool {
    let rhs_nodes_base: Vec<MutNode> = sr
        .rhs
        .nodes
        .iter()
        .map(|n| new_seg_node_dirty(g, table, n, false, true))
        .collect();
    let lhs_vars = pattern_var_occurrences(&rule.lhs);
    let rhs_vars = pattern_var_occurrences(&sr.rhs);

    let (segs, node_of) = ms.segs(true);
    let mut accepted: Vec<(Vec<usize>, Bindings)> = Vec::new();
    for (s, e) in all_spans(target, &segs) {
        let target_nodes: Vec<usize> = node_of[s..e].to_vec();
        if !width_matches(&target_nodes, rule.lhs.nodes.len()) {
            continue;
        }
        if target_nodes.iter().any(|&n| {
            ms.nodes[n].dirty || !matches!(ms.nodes[n].kind, NodeKind::Segment | NodeKind::Boundary)
        }) {
            continue;
        }
        let Some(left_match) = left_env_match(left, &segs, s) else {
            continue;
        };
        let Some(right_match) = right_env_match(right, &segs, e) else {
            continue;
        };
        let Some(bindings) = resolve_bindings(
            g,
            ms,
            &node_of,
            &target_nodes,
            e,
            &lhs_vars,
            left,
            &left_match,
            right,
            &right_match,
        ) else {
            continue;
        };
        accepted.push((target_nodes, bindings));
    }
    if accepted.is_empty() {
        return false;
    }
    for (target_nodes, bindings) in accepted.into_iter().rev() {
        let rhs_nodes: Vec<MutNode> = rhs_nodes_base
            .iter()
            .cloned()
            .zip(&rhs_vars)
            .map(|(mut n, occs)| {
                for occ in occs {
                    if let Some(&(b, _)) = bindings.get(&occ.var) {
                        let mask = full_mask(g, occ.feature);
                        n.lanes[occ.feature] = if occ.plus { b } else { mask & !b };
                    }
                }
                n
            })
            .collect();
        let last = *target_nodes.last().unwrap();
        ms.nodes.splice(last + 1..last + 1, rhs_nodes);
        for &n in target_nodes.iter().rev() {
            ms.nodes.remove(n);
        }
    }
    true
}

/// The empty-RHS branch of C#'s narrowing analysis: match a single site node, then re-insert the LHS segments as **optional** after it; sites are collected against the un-mutated shape and applied descending.
fn ana_narrow_deletion(
    g: &Grammar,
    table: &CharDefTable,
    rule: &RewriteRuleDef,
    sr: &RewriteSubruleDef,
    ms: &mut MutShape,
    left: &Option<EnvFst>,
    right: &Option<EnvFst>,
) -> bool {
    debug_assert!(
        sr.rhs.nodes.is_empty(),
        "ana_narrow_deletion is the IsTargetEmpty branch only"
    );
    let lhs_nodes: Vec<MutNode> = rule
        .lhs
        .nodes
        .iter()
        .map(|n| new_seg_node(g, table, n, true)) // re-inserted deleted segment is OPTIONAL
        .collect();

    // Sites: gaps after each segment node where the (deleted) LHS could sit and both environments hold; insertion goes after the site node.
    let (segs, node_of) = ms.segs(false);
    let mut sites: Vec<usize> = Vec::new(); // shape node index after which to insert

    // The word-initial gap is a real site: C#'s substitute pattern matches the left anchor (node 0, here) and inserts right after it; without this a word-initial deletion can't be re-inserted.
    if left_env_ok(left, &segs, 0) && right_env_ok(right, &segs, 0) {
        sites.push(0);
    }
    for (site, &node) in node_of.iter().enumerate() {
        let left_end = site + 1; // context up to and including the site node
        let right_start = site + 1;
        if left_env_ok(left, &segs, left_end) && right_env_ok(right, &segs, right_start) {
            sites.push(node);
        }
    }
    if sites.is_empty() {
        return false;
    }
    // Apply descending so earlier insertions don't shift later site indices.
    for &site_node in sites.iter().rev() {
        ms.nodes
            .splice(site_node + 1..site_node + 1, lhs_nodes.iter().cloned());
    }
    true
}

/// The non-empty-RHS branch (narrowing/expansion): matches the RHS's own constraints (not an LHS-vs-RHS union), then splices the reconstructed LHS in as OPTIONAL after the match and marks the matched nodes optional too; matches are found against the pristine shape and applied descending, since this port's index-based nodes, unlike C#'s linked list, don't survive insertion.
#[allow(clippy::too_many_arguments)]
fn ana_narrow_general(
    g: &Grammar,
    table: &CharDefTable,
    rule: &RewriteRuleDef,
    sr: &RewriteSubruleDef,
    ms: &mut MutShape,
    target: &Fst,
    left: &Option<EnvFst>,
    right: &Option<EnvFst>,
) -> bool {
    // The matched pattern here is the RHS, so its own alpha-variable occurrences get bound during matching (target position k ↔ RHS node k); the reconstructed LHS nodes consume those bindings on insertion.
    let rhs_vars = pattern_var_occurrences(&sr.rhs);
    let lhs_vars = pattern_var_occurrences(&rule.lhs);
    let lhs_template: Vec<MutNode> = rule
        .lhs
        .nodes
        .iter()
        .map(|n| new_seg_node(g, table, n, true)) // spliced-in reconstruction is OPTIONAL
        .collect();

    // A correctly-aligned RHS match spans exactly one segment node per RHS node.
    let target_len = sr.rhs.nodes.len();

    // Analysis filter: Segment|Anchor (no boundaries), matching `ana_feature`/the deletion case.
    let (segs, node_of) = ms.segs(false);
    let mut matches: Vec<(usize, usize, Bindings)> = Vec::new();
    for (s, e) in all_spans(target, &segs) {
        let target_nodes: Vec<usize> = node_of[s..e].to_vec();
        // Dropping an over-wide span here only discards a duplicate, since `all_spans` also reports the tight match; without the guard, a single-node target on an Optional-flooded shape spuriously matches whole multi-segment windows and reconstructs at every one — a flood C#'s per-position group capture avoids entirely.
        if !width_matches(&target_nodes, target_len) {
            continue;
        }
        let Some(left_match) = left_env_match(left, &segs, s) else {
            continue;
        };
        let Some(right_match) = right_env_match(right, &segs, e) else {
            continue;
        };
        let Some(bindings) = resolve_bindings(
            g,
            ms,
            &node_of,
            &target_nodes,
            e,
            &rhs_vars,
            left,
            &left_match,
            right,
            &right_match,
        ) else {
            continue;
        };
        matches.push((s, e, bindings));
    }
    if matches.is_empty() {
        return false;
    }

    // Apply descending (by match start) so earlier splices don't shift not-yet-applied matches' node indices.
    for (s, e, bindings) in matches.into_iter().rev() {
        let target_nodes: Vec<usize> = node_of[s..e].to_vec();
        // (1) splice the reconstructed original-LHS material in right after the match, with alpha-variable bindings from the RHS match resolved onto it.
        let mut insert_nodes = lhs_template.clone();
        for (k, node) in insert_nodes.iter_mut().enumerate() {
            for occ in &lhs_vars[k] {
                if let Some(&(b, _)) = bindings.get(&occ.var) {
                    let mask = full_mask(g, occ.feature);
                    node.lanes[occ.feature] = if occ.plus { b } else { mask & !b };
                }
            }
        }
        let insert_at = *target_nodes.last().unwrap() + 1;
        ms.nodes.splice(insert_at..insert_at, insert_nodes);
        // (2) mark the originally-matched nodes optional (NOT deleted).
        for &n in &target_nodes {
            ms.nodes[n].optional = true;
        }
    }
    true
}

// Epenthesis (LHS empty).

/// C# `EpenthesisSynthesisRewriteSubruleSpec.ApplyRhs`: insert the RHS nodes at each site where both environments hold, marking them dirty; sites are collected once against the current shape and applied without rescanning, since epenthesis in the reference grammars sits between fixed contexts and never cascades.
fn syn_epenthesis(
    g: &Grammar,
    table: &CharDefTable,
    sr: &RewriteSubruleDef,
    ms: &mut MutShape,
    left: &Option<EnvFst>,
    right: &Option<EnvFst>,
) -> bool {
    let rhs_nodes: Vec<MutNode> = sr
        .rhs
        .nodes
        .iter()
        .map(|n| new_seg_node(g, table, n, false))
        .collect();

    // Sites: gaps after a segment node where both environments hold, collected once against the current shape then inserted descending.
    let (segs, node_of) = ms.segs(true);
    let mut sites: Vec<usize> = Vec::new();
    // The word-initial gap is a real site (synthesis twin of `ana_narrow_deletion`'s): C#'s empty-LHS pattern matches the left anchor and inserts right after it; anchors never appear in `node_of`, so the per-segment loop below can't reach this gap without it.
    if left_env_ok(left, &segs, 0) && right_env_ok(right, &segs, 0) {
        sites.push(0);
    }
    for (site, &node) in node_of.iter().enumerate() {
        // A boundary appears in `node_of` (via `segs(true)`'s transparently-skippable Optional segments) but is never a valid epenthesis site itself — C#'s empty-LHS pattern admits segments and anchors only, and the preceding real segment's own site already reaches past it via the same skip.
        if ms.nodes[node].kind == NodeKind::Boundary {
            continue;
        }
        let left_end = site + 1;
        let right_start = site + 1;
        if left_env_ok(left, &segs, left_end) && right_env_ok(right, &segs, right_start) {
            sites.push(node);
        }
    }
    if sites.is_empty() {
        return false;
    }
    for &site_node in sites.iter().rev() {
        ms.nodes
            .splice(site_node + 1..site_node + 1, rhs_nodes.iter().cloned());
    }
    true
}

/// The `ana_epenthesis` target FST's per-node lanes (RHS segment sequence, FST-facing), factored out for [`RuleCache`](crate::cache::RuleCache) to compile once; empty means a degenerate no-op subrule with no target at all.
fn ana_epenthesis_target_lanes(
    g: &Grammar,
    table: &CharDefTable,
    sr: &RewriteSubruleDef,
) -> Vec<Vec<u64>> {
    sr.rhs
        .nodes
        .iter()
        .map(|n| to_fst_lanes(g, &node_full_lanes(g, table, n)))
        .collect()
}

/// C# `EpenthesisAnalysisRewriteRuleSpec` (reapply Normal/SelfOpaquing): matches the epenthesized segment(s) and marks them **optional** rather than deleting, so a later lexical lookup may skip them; the nonvacuous guard skips already-optional nodes.
fn ana_epenthesis(
    ms: &mut MutShape,
    target: Option<&Fst>,
    expected_len: usize,
    left: &Option<EnvFst>,
    right: &Option<EnvFst>,
) -> bool {
    let Some(target) = target else {
        return false; // no RHS material to have epenthesized (see `ana_epenthesis_target_lanes`).
    };

    let (segs, node_of) = ms.segs(false);
    let mut applied = false;
    for (s, e) in all_spans(target, &segs) {
        let target_nodes: Vec<usize> = node_of[s..e].to_vec();
        // Width guard: reject an over-wide Optional-skip span before it can mark the wrong (extra) node Optional below; see `width_matches`.
        if !width_matches(&target_nodes, expected_len) {
            continue;
        }
        if !left_env_ok(left, &segs, s) || !right_env_ok(right, &segs, e) {
            continue;
        }
        // Nonvacuous: at least one target node is not already optional.
        if target_nodes.iter().all(|&n| ms.nodes[n].optional) {
            continue;
        }
        for &n in &target_nodes {
            ms.nodes[n].optional = true;
            ms.nodes[n].dirty = true;
        }
        applied = true;
    }
    applied
}

// Small shared builders.

fn dir_of(rule: &RewriteRuleDef) -> Direction {
    dir_from_model(rule.dir)
}

/// `pg_grammar::model::Dir` → `pg_fst::Direction`. `pub(crate)` so `pg_rules::metathesis` (whose
/// `MetathesisRuleDef.dir` is the same model `Dir`) can reuse it instead of duplicating the match.
pub(crate) fn dir_from_model(d: pg_grammar::model::Dir) -> Direction {
    match d {
        pg_grammar::model::Dir::LeftToRight => Direction::LeftToRight,
        pg_grammar::model::Dir::RightToLeft => Direction::RightToLeft,
    }
}

pub(crate) fn reverse(d: Direction) -> Direction {
    match d {
        Direction::LeftToRight => Direction::RightToLeft,
        Direction::RightToLeft => Direction::LeftToRight,
    }
}

/// Convert driver full-mask lanes to FST-facing lanes (unconstrained `full_mask` → `u64::MAX`) so the compiled constraint canonicalizes and matches identically.
fn to_fst_lanes(g: &Grammar, lanes: &[u64]) -> Vec<u64> {
    lanes
        .iter()
        .enumerate()
        .map(|(f, &l)| {
            if l == full_mask(g, f) {
                UNCONSTRAINED
            } else {
                l
            }
        })
        .collect()
}

/// Compile an LHS `Pattern` to a target FST via the bridge (deterministic for synthesis).
fn lhs_fst(
    g: &Grammar,
    table_id: TableId,
    lhs: &Pattern,
    dir: Direction,
    deterministic: bool,
) -> Fst {
    let bridge = PatternBridge::new(g)
        .with_table(table_id)
        .deterministic(deterministic);
    let compiled = bridge.compile_pattern(lhs).expect("LHS compiles");
    compiled.input.compile_with_direction(dir)
}

/// Build a fresh segment `MutNode` from an RHS/LHS pattern node; `optional` sets the Optional flag (used by analysis re-insertion of deleted segments). Residual: a `PatternNode::Context` RHS produces a `NO_CHAR_DEF`/`CdSet::Unrestricted` node, unexercised since `Kind::Epenthesis` has zero occurrences in the three reference grammars.
fn new_seg_node(g: &Grammar, table: &CharDefTable, node: &PatternNode, optional: bool) -> MutNode {
    // C# `AddAfter` never sets `Optional`; callers set it explicitly themselves (e.g. `ana_narrow`'s re-inserted deleted segment). Use `new_seg_node_dirty` when `dirty` needs to differ from `optional` (see `syn_narrow`).
    new_seg_node_dirty(g, table, node, optional, optional)
}

/// Like `new_seg_node` but lets `dirty` differ from `optional` (C#'s `AddAfter` + separate `SetDirty`). `kind` must resolve from the char-def table for `PatternNode::CharDef` (a `BoundaryMarker` is a real top-level LHS/RHS element, e.g. Amharic `prule7`) rather than defaulting to `Segment`: a boundary char-def has no phonological features, so mis-kinding it as `Segment` makes it a wildcard that unifies with any affix material, corrupting analysis-side matching.
fn new_seg_node_dirty(
    g: &Grammar,
    table: &CharDefTable,
    node: &PatternNode,
    optional: bool,
    dirty: bool,
) -> MutNode {
    let (char_def, kind) = match node {
        PatternNode::CharDef(cd) => {
            let kind = match table.get(*cd).kind() {
                pg_grammar::chardef::CharDefKind::Segment => NodeKind::Segment,
                pg_grammar::chardef::CharDefKind::Boundary => NodeKind::Boundary,
            };
            (cd.0, kind)
        }
        // feature-only inserted node: no char-def identity (Context RHS); always Segment.
        _ => (u32::MAX, NodeKind::Segment),
    };
    MutNode {
        kind,
        char_def,
        lanes: node_full_lanes(g, table, node),
        optional,
        deleted: false,
        dirty,
    }
}

// Compile-once cache: `crate::cache::RuleCache`'s per-phonological-rule slice.

/// Per-subrule precompiled matchers. Every field mirrors exactly one compile call the uncached
/// `synthesize_with_mpr`/`analyze` would otherwise make per subrule per call: `syn_left`/
/// `syn_right` serve `syn_feature`/`syn_narrow`/`syn_epenthesis` (all three call `compile_env`
/// identically, so one pair covers whichever kind this subrule actually is); `ana_left`/`ana_right`
/// likewise serve `ana_feature`/`ana_narrow_deletion`/`ana_narrow_general`/`ana_epenthesis`;
/// `ana_target` is `Some` for `Kind::Feature`, `Kind::Epenthesis` (when its RHS is non-empty), and
/// `Kind::Narrow` when its RHS is non-empty (`ana_narrow_general` — the deletion case,
/// `ana_narrow_deletion`, walks raw LHS pattern nodes directly and never compiles a target FST).
pub(crate) struct SubruleCache {
    pub(crate) syn_left: Option<EnvFst>,
    pub(crate) syn_right: Option<EnvFst>,
    pub(crate) ana_left: Option<EnvFst>,
    pub(crate) ana_right: Option<EnvFst>,
    pub(crate) ana_target: Option<Fst>,
    /// `Some` (one name per `ana_target` row, "g0".."g{N-1}") iff this subrule is `Kind::Feature` --
    /// see `compile_lane_fst_grouped`'s doc / `ana_feature`'s per-row Group-capture fix. `None`
    /// for `Kind::Epenthesis`/`Kind::Narrow`, whose `ana_target` is still the ungrouped
    /// `compile_lane_fst` (only `ana_feature`'s specific multi-row adjacency search needed this).
    pub(crate) ana_target_names: Option<Vec<String>>,
}

/// Per-phonological-rule precompiled matchers. `syn_target` is the rule-level synthesis LHS target
/// (`lhs_fst(rule.lhs, dir_of(rule), true)`) — identical for every `Kind::Feature`/`Kind::Narrow`
/// subrule of this rule (both call it with the exact same arguments, since the LHS pattern lives on
/// the *rule*, not the subrule), so it is compiled once per rule, not once per subrule. `None` iff
/// `rule.lhs` is empty (every subrule is then `Kind::Epenthesis`, which never reads `syn_target`).
pub(crate) struct PruleCache {
    pub(crate) syn_target: Option<Fst>,
    pub(crate) subrules: Vec<SubruleCache>,
}

/// Build the compile-once cache for one phonological rule (`crate::cache::RuleCache::build` calls
/// this once per `g.prules` entry). Faithfully mirrors the uncached functions' own compile calls —
/// see `SubruleCache`'s doc for exactly which kind reads which field.
pub(crate) fn build_prule_cache(
    g: &Grammar,
    table_id: TableId,
    rule: &RewriteRuleDef,
) -> PruleCache {
    let table = &g.char_tables[table_id.0 as usize];
    let syn_target =
        (!rule.lhs.nodes.is_empty()).then(|| lhs_fst(g, table_id, &rule.lhs, dir_of(rule), true));
    let subrules = rule
        .subrules
        .iter()
        .map(|sr| {
            let (ana_target, ana_target_names) = match classify(rule, sr) {
                Kind::Feature => {
                    let lanes = ana_feature_target_lanes(g, table, rule, sr);
                    let (fst, names) =
                        compile_lane_fst_grouped(&lanes, reverse(dir_of(rule)), false);
                    (Some(fst), Some(names))
                }
                Kind::Epenthesis => {
                    let lanes = ana_epenthesis_target_lanes(g, table, sr);
                    (
                        (!lanes.is_empty())
                            .then(|| compile_lane_fst(&lanes, reverse(dir_of(rule)), false)),
                        None,
                    )
                }
                // The general-narrowing/expansion case (`ana_narrow_general`) reuses `ana_epenthesis_target_lanes`'s formula ("the RHS segment sequence, FST-facing") rather than duplicating it; the pure-deletion case (`ana_narrow_deletion`) still compiles no FST at all.
                Kind::Narrow => {
                    if sr.rhs.nodes.is_empty() {
                        (None, None)
                    } else {
                        let lanes = ana_epenthesis_target_lanes(g, table, sr);
                        (
                            Some(compile_lane_fst(&lanes, reverse(dir_of(rule)), false)),
                            None,
                        )
                    }
                }
            };
            SubruleCache {
                syn_left: compile_env(g, table_id, sr.left_env.as_ref()),
                syn_right: compile_env(g, table_id, sr.right_env.as_ref()),
                ana_left: compile_env_analysis(g, table_id, sr.left_env.as_ref()),
                ana_right: compile_env_analysis(g, table_id, sr.right_env.as_ref()),
                ana_target,
                ana_target_names,
            }
        })
        .collect();
    PruleCache {
        syn_target,
        subrules,
    }
}

// docs/research/rewrite-probe-position-stability.md
// Probing-only synthesis for `hc_hybrid::surface`'s `SurfacePhonology` port: soft-deletes instead of physically removing nodes, since C# never removes a node either.

/// Soft-delete sibling of `syn_narrow`: identical except marking `deleted = true` instead of calling `nodes.remove`; see the module note above for why that's sufficient.
#[allow(clippy::too_many_arguments)]
fn probe_narrow(
    g: &Grammar,
    table: &CharDefTable,
    rule: &RewriteRuleDef,
    sr: &RewriteSubruleDef,
    ms: &mut MutShape,
    target: &Fst,
    left: &Option<EnvFst>,
    right: &Option<EnvFst>,
) -> bool {
    let rhs_nodes_base: Vec<MutNode> = sr
        .rhs
        .nodes
        .iter()
        .map(|n| new_seg_node_dirty(g, table, n, false, true))
        .collect();
    let lhs_vars = pattern_var_occurrences(&rule.lhs);
    let rhs_vars = pattern_var_occurrences(&sr.rhs);

    let mut applied = false;
    loop {
        let (segs, node_of) = ms.segs(true);
        let mut acted = false;
        // Direction-ordered scan (see `ordered_spans`); same as `syn_narrow`, whose soft-delete sibling this function is.
        for (s, e) in ordered_spans(target, &segs) {
            let target_nodes: Vec<usize> = node_of[s..e].to_vec();
            if !width_matches(&target_nodes, rule.lhs.nodes.len()) {
                continue;
            }
            if target_nodes.iter().any(|&n| {
                ms.nodes[n].dirty
                    || !matches!(ms.nodes[n].kind, NodeKind::Segment | NodeKind::Boundary)
            }) {
                continue;
            }
            let Some(left_match) = left_env_match(left, &segs, s) else {
                continue;
            };
            let Some(right_match) = right_env_match(right, &segs, e) else {
                continue;
            };
            let Some(bindings) = resolve_bindings(
                g,
                ms,
                &node_of,
                &target_nodes,
                e,
                &lhs_vars,
                left,
                &left_match,
                right,
                &right_match,
            ) else {
                continue;
            };
            let rhs_nodes: Vec<MutNode> = rhs_nodes_base
                .iter()
                .cloned()
                .zip(&rhs_vars)
                .map(|(mut n, occs)| {
                    for occ in occs {
                        if let Some(&(b, _)) = bindings.get(&occ.var) {
                            let mask = full_mask(g, occ.feature);
                            n.lanes[occ.feature] = if occ.plus { b } else { mask & !b };
                        }
                    }
                    n
                })
                .collect();
            let last = *target_nodes.last().unwrap();
            // Insert RHS right after the last target node (real, physical -- matches `syn_narrow`).
            ms.nodes.splice(last + 1..last + 1, rhs_nodes);
            // Soft-delete the target nodes instead of removing them (the one line that differs from `syn_narrow`); descending order isn't required for a mark-in-place but kept for textual parallelism.
            for &n in target_nodes.iter().rev() {
                ms.nodes[n].deleted = true;
            }
            applied = true;
            acted = true;
            break;
        }
        if !acted {
            break;
        }
    }
    applied
}

/// Soft-delete sibling of `sim_narrow` (Simultaneous Narrow). See the module note above.
#[allow(clippy::too_many_arguments)]
fn probe_sim_narrow(
    g: &Grammar,
    table: &CharDefTable,
    rule: &RewriteRuleDef,
    sr: &RewriteSubruleDef,
    ms: &mut MutShape,
    target: &Fst,
    left: &Option<EnvFst>,
    right: &Option<EnvFst>,
) -> bool {
    let rhs_nodes_base: Vec<MutNode> = sr
        .rhs
        .nodes
        .iter()
        .map(|n| new_seg_node_dirty(g, table, n, false, true))
        .collect();
    let lhs_vars = pattern_var_occurrences(&rule.lhs);
    let rhs_vars = pattern_var_occurrences(&sr.rhs);

    let (segs, node_of) = ms.segs(true);
    let mut accepted: Vec<(Vec<usize>, Bindings)> = Vec::new();
    for (s, e) in all_spans(target, &segs) {
        let target_nodes: Vec<usize> = node_of[s..e].to_vec();
        if !width_matches(&target_nodes, rule.lhs.nodes.len()) {
            continue;
        }
        if target_nodes.iter().any(|&n| {
            ms.nodes[n].dirty || !matches!(ms.nodes[n].kind, NodeKind::Segment | NodeKind::Boundary)
        }) {
            continue;
        }
        let Some(left_match) = left_env_match(left, &segs, s) else {
            continue;
        };
        let Some(right_match) = right_env_match(right, &segs, e) else {
            continue;
        };
        let Some(bindings) = resolve_bindings(
            g,
            ms,
            &node_of,
            &target_nodes,
            e,
            &lhs_vars,
            left,
            &left_match,
            right,
            &right_match,
        ) else {
            continue;
        };
        accepted.push((target_nodes, bindings));
    }
    if accepted.is_empty() {
        return false;
    }
    for (target_nodes, bindings) in accepted.into_iter().rev() {
        let rhs_nodes: Vec<MutNode> = rhs_nodes_base
            .iter()
            .cloned()
            .zip(&rhs_vars)
            .map(|(mut n, occs)| {
                for occ in occs {
                    if let Some(&(b, _)) = bindings.get(&occ.var) {
                        let mask = full_mask(g, occ.feature);
                        n.lanes[occ.feature] = if occ.plus { b } else { mask & !b };
                    }
                }
                n
            })
            .collect();
        let last = *target_nodes.last().unwrap();
        ms.nodes.splice(last + 1..last + 1, rhs_nodes);
        for &n in target_nodes.iter().rev() {
            ms.nodes[n].deleted = true;
        }
    }
    true
}

/// Outcome of applying one `RewriteRuleDef`'s subrules to a probing `MutShape` (`probe_apply_rule_cached`).
pub(crate) enum ProbeOutcome {
    NoMatch,
    Applied,
    /// An empty-LHS (`Kind::Epenthesis`) rule was reached; refused rather than approximated, since no reference grammar exercises this shape -- see the module note above.
    Refused,
}

/// Probing analog of `synthesize_with_mpr_cached`'s subrule loop (`syn_fs`/`mpr` always empty --
/// `SurfacePhonology`'s probe `Word` carries neither, C#'s bare `new Word(surfaceStratum, shape)`
/// two-arg constructor, `SurfacePhonology.cs:316`), routed through `probe_narrow`/
/// `probe_sim_narrow` for `Kind::Narrow` instead of `syn_narrow`/`sim_narrow`; `Kind::Feature`
/// reuses `syn_feature`/`sim_feature` completely unchanged (a feature-change subrule never
/// touches node count or position, so there is nothing for a probing sibling to do differently).
///
/// **Cached, not recompiled per call** (`crate::cache::RuleCache`, read via `cache.prule_rewrite
/// (pid)`): `SurfacePhonology` probes every affix underlying form against every alphabet
/// representative (potentially alphabet² for `DeletionJunctions`), so this is called far more often
/// than any other per-rule entry point in the engine -- exactly the hot-loop `RuleCache` exists for
/// (`crate::cache`'s own module doc). Recompiling `lhs_fst`/`compile_env` per call here made
/// Amharic's 417-segment-alphabet probing impractically slow -- a separate, avoidable inefficiency
/// on top of the inherent per-word cost, and had to be fixed to make probing runnable at all.
pub(crate) fn probe_apply_rule_cached(
    g: &Grammar,
    pid: pg_grammar::model::PRuleId,
    rule: &RewriteRuleDef,
    ms: &mut MutShape,
    cache: &crate::cache::RuleCache,
) -> ProbeOutcome {
    // Owning-table resolution: see `synthesize_with_mpr_cached`.
    let table_id = crate::cache::owning_table_for_prule(g, pid).unwrap_or(TableId(0));
    let table = &g.char_tables[table_id.0 as usize];
    let pc = cache.prule_rewrite(pid);
    let mut applied = false;
    for (i, sr) in rule.subrules.iter().enumerate() {
        if !subrule_applicable(g, sr, &FeatureStruct::EMPTY, MprSet::EMPTY) {
            continue;
        }
        let sc = &pc.subrules[i];
        match classify(rule, sr) {
            Kind::Feature => {
                let target = pc
                    .syn_target
                    .as_ref()
                    .expect("Feature/Narrow subrule always has a compiled syn target");
                let did = match rule.mode {
                    RewriteMode::Iterative => {
                        syn_feature(g, table, rule, sr, ms, target, &sc.syn_left, &sc.syn_right)
                    }
                    RewriteMode::Simultaneous => {
                        sim_feature(g, table, rule, sr, ms, target, &sc.syn_left, &sc.syn_right)
                    }
                };
                applied |= did;
            }
            Kind::Narrow => {
                let target = pc
                    .syn_target
                    .as_ref()
                    .expect("Feature/Narrow subrule always has a compiled syn target");
                let did = match rule.mode {
                    RewriteMode::Iterative => {
                        probe_narrow(g, table, rule, sr, ms, target, &sc.syn_left, &sc.syn_right)
                    }
                    RewriteMode::Simultaneous => probe_sim_narrow(
                        g,
                        table,
                        rule,
                        sr,
                        ms,
                        target,
                        &sc.syn_left,
                        &sc.syn_right,
                    ),
                };
                applied |= did;
            }
            Kind::Epenthesis => return ProbeOutcome::Refused,
        }
    }
    if applied {
        ProbeOutcome::Applied
    } else {
        ProbeOutcome::NoMatch
    }
}

/// Run every phonological rule of one stratum over a probing `MutShape`, in declaration order --
/// the probing analog of a `LinearRuleCascade<Word,int>`'s single-pass-per-rule application. Every
/// forward rule this port's `synthesize`/`synthesize_with_mpr` can apply yields AT MOST ONE result
/// (`vec![ms.to_shape()]` or empty, never a branching set), so `LinearRuleCascade`'s general
/// recursive "first terminal derivation" (`SurfacePhonology.cs`'s `cascade.Apply(word).
/// DefaultIfEmpty(word).First()`) degenerates to a straight sequential fold here -- there is no
/// branching to search, so applying each rule once, in order, to the SAME persistent `ms` already
/// computes exactly that "first" result. `dirty` is reset before each rule (matching every real
/// call site's `MutShape::from_shape`, which always starts `dirty: false`); `deleted` is NEVER
/// reset -- that is the entire point of this probing path (module note above).
pub(crate) fn probe_synthesize_stratum(
    g: &Grammar,
    prules: &[pg_grammar::model::PRuleId],
    ms: &mut MutShape,
    cache: &crate::cache::RuleCache,
) -> ProbeOutcome {
    for &pid in prules {
        for n in ms.nodes.iter_mut() {
            n.dirty = false;
        }
        match &g.prules[pid.0 as usize] {
            pg_grammar::model::PhonRuleDef::Rewrite(r) => {
                if let ProbeOutcome::Refused = probe_apply_rule_cached(g, pid, r, ms, cache) {
                    return ProbeOutcome::Refused;
                }
            }
            pg_grammar::model::PhonRuleDef::Metathesis(_) => {
                // Unreachable on the three reference grammars (verified: zero `<MetathesisRule>`s); refuse rather than silently mis-track positions if one is ever added.
                return ProbeOutcome::Refused;
            }
        }
    }
    ProbeOutcome::NoMatch // caller only inspects `Refused` vs. not; see `probe_synthesize_all_strata`.
}

/// Regression pins for `compile_lane_fst_grouped`'s per-row `Group` capture: it recovers each row's REAL matched position across an interposed Optional non-matching segment, and WHICH tag half (start vs end) is reliable flips with compile direction, matching `ana_feature`'s `recover_pos` assumption.
#[cfg(test)]
mod group_probe_diag {
    use super::*;

    fn probe_segs(match_lane: &[u64], other_lane: &[u64]) -> Vec<Segment> {
        vec![
            Segment::new(other_lane.to_vec()),
            Segment::new(match_lane.to_vec()),
            Segment::optional(other_lane.to_vec()),
            Segment::new(match_lane.to_vec()),
            Segment::optional(other_lane.to_vec()),
            Segment::new(match_lane.to_vec()),
        ]
    }

    /// `LeftToRight`: each row's START offset is reliable (only the END can be widened by a following skip); real matches sit at seg positions 1, 3, 5, expecting adjacent pairs (1,3) and (3,5) via each row's start.
    #[test]
    fn group_offsets_survive_interposed_optional_ltr() {
        let match_lane = vec![0b01u64];
        let other_lane = vec![0b10u64];
        let segs = probe_segs(&match_lane, &other_lane);
        let (fst, names) = compile_lane_fst_grouped(
            &[match_lane.clone(), match_lane.clone()],
            Direction::LeftToRight,
            false,
        );

        let mut pairs: Vec<(i32, i32)> = Transduce::new(&fst, segs.clone())
            .all_matches()
            .iter()
            .map(|r| {
                let g0 = fst.get_offsets(&names[0], &r.registers).expect("g0 set").0;
                let g1 = fst.get_offsets(&names[1], &r.registers).expect("g1 set").0;
                (g0, g1)
            })
            .collect();
        pairs.sort_unstable();
        pairs.dedup();
        assert_eq!(
            pairs,
            vec![(1, 3), (3, 5)],
            "LTR: each row's START must resolve to the real match position"
        );
    }

    /// `RightToLeft` (the direction `ana_feature`'s target actually compiles under): node order is document-reversed and `Fst::get_offsets` swaps `(start,end)` back, so each row's END is reliable instead of START — same real-match positions (1,3,5) and pairs, now read via `.1 - 1`.
    #[test]
    fn group_offsets_survive_interposed_optional_rtl() {
        let match_lane = vec![0b01u64];
        let other_lane = vec![0b10u64];
        let segs = probe_segs(&match_lane, &other_lane);
        let (fst, names) = compile_lane_fst_grouped(
            &[match_lane.clone(), match_lane.clone()],
            Direction::RightToLeft,
            false,
        );

        let mut pairs: Vec<(i32, i32)> = Transduce::new(&fst, segs.clone())
            .all_matches()
            .iter()
            .map(|r| {
                let g0 = fst.get_offsets(&names[0], &r.registers).expect("g0 set").1 - 1;
                let g1 = fst.get_offsets(&names[1], &r.registers).expect("g1 set").1 - 1;
                (g0, g1)
            })
            .collect();
        pairs.sort_unstable();
        pairs.dedup();
        assert_eq!(
            pairs,
            vec![(1, 3), (3, 5)],
            "RTL: each row's END-1 must resolve to the real match position"
        );
    }
}
