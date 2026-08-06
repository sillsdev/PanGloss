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

// =================================================================================================
// Mutable working shape — C# `Shape` plus the Optional/Deleted/Dirty flags `pg_shape` omits.
// =================================================================================================

// Crate-visible because `crate::metathesis` reuses this machinery rather than duplicating it: the
// "resolve to concrete node data before mutating" discipline it encodes is exactly what that
// module's synthesis reorder needs, matching how C# structures the same operation.
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

// =================================================================================================
// Feature-constraint helpers (the "which features does this node pin, and to what" resolution).
// =================================================================================================

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

// =================================================================================================
// Matching (target + environments) on top of the frozen FST.
// =================================================================================================

/// Compile a lane sequence, given in DOCUMENT order, to a target FST traversed in `dir`.
///
/// The reversal is load-bearing and must happen HERE, at the pattern-compile boundary. C# enumerates
/// a pattern's children reversed for a right-to-left matcher, so direction changes scan preference
/// but not the physical substring matched; `pg_fst` takes the opposite convention and never
/// reorders. Without this, a multi-node target silently matches the physically-reversed sequence —
/// invisible on a single-node target, wrong on anything wider.
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

/// `compile_lane_fst`, but each row is wrapped in its own named group ("g0".."g{N-1}", in DOCUMENT
/// order, so the names stay stable across the right-to-left physical reorder). A caller can then
/// recover which single physical segment each row actually consumed, which is what `ana_feature`
/// needs. C# mirrors this with its own per-target-position groups.
///
/// **Only the group's START offset is trustworthy.** The traversal's "skip the next Optional
/// annotation" branch (see `width_matches`) can widen a group's END tag to swallow a transparently
/// skipped segment; the START is never contaminated. Do not read an END from this FST's groups.
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

/// `all_spans`, reordered to an Iterative pick-one-then-rescan loop's own scan preference: C#
/// takes the match nearest the direction-side edge first, then resumes scanning further in that
/// same direction. Only a pick-one loop needs this. Every other caller is a Simultaneous
/// collect-then-apply-all consumer with no "which one wins" question, so `all_spans` itself stays
/// a direction-agnostic ascending sort rather than changing its contract for everyone.
///
/// Keying off `target.direction()` rather than the rule's is what keeps both caller families
/// correct with one rule: synthesis compiles its target in the rule's direction, analysis in the
/// reverse, and reading the compiled matcher's own direction cannot get out of sync with either.
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
    /// Per-top-level-pattern-node alpha-variable occurrences, aligned with the authored pattern —
    /// one entry per top-level node, quantifiers included and always empty, since variables nested
    /// inside a quantifier are a separate flagged limitation of `pattern_var_occurrences`.
    node_vars: Vec<Vec<VarOccur>>,
    /// The capture-group name for each var-bearing entry of `node_vars`; `None` where the node has
    /// no occurrences and so was left unwrapped.
    ///
    /// C# needs no such recovery: its arcs carry the live constraint, variables included, and
    /// bindings are checked inline as each arc fires, so the automaton itself distinguishes a
    /// quantifier's looping arc from the var-bearing one. This port cannot, because compilation
    /// erases variable-governed lanes to `UNCONSTRAINED` before the FST is built, leaving a
    /// post-hoc re-check that has to recover which segment each pattern node really consumed. A
    /// positional guess is only correct when every node consumes exactly one segment; these
    /// captures are correct whatever a quantifier elsewhere in the pattern swallowed.
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

    // Wrap each var-bearing top-level node in a named capture group so its matched segment can be
    // recovered independently of any quantifier elsewhere in the pattern; see `EnvFst::group_names`.
    // A node with no alpha-variable occurrence needs no capture and is left unwrapped.
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

/// C# `DeepCloneExceptBoundaries`: drop every node denoting a literal morpheme-boundary character,
/// recursing into quantifiers and dropping one entirely once its filtered children are empty. Only
/// a `CharDef` can denote a boundary in this flattened model — a natural class always injects
/// segment type, and an anchor is a distinct type C# does not strip either.
///
/// Known residual: a pre-segmented `Segments` node could in principle embed a boundary too. Such
/// environments only ever occur on allomorphs, which take the unstripped `compile_env` path, so
/// this is flagged rather than silently wrong on an exercised path.
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

// =================================================================================================
// Alpha-variable agreement.
//
// The FST cannot bind variables, so its arcs over-approximate and the real agreement check runs
// against node lanes after a candidate span is reported: the first occurrence BINDS the node's
// symbol set, or its negation within the feature mask when the occurrence disagrees; a later
// occurrence requires the polarity-adjusted binding to share a symbol with the node, and rejects
// otherwise. Binding order is C#'s — target match first, then left environment, then right.
// =================================================================================================

/// Bindings: `VarId.0` → (bound symbol bits, governed feature index). The feature index recovers the
/// full mask for the disagree/negation cases.
type Bindings = HashMap<u16, (u64, usize)>;

/// One agreement step (C# `SimpleFeatureValue.IsUnifiableImpl` variable arm). `node_bits` is the
/// node's actual symbol set on `occ.feature`. Returns `false` only when a bound variable is violated.
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

/// Resolve alpha-variable bindings for a candidate, in C# `MatchSubrule` order (target, then left
/// env, then right env). Returns `None` if any bound variable is violated (reject the candidate),
/// else the accumulated bindings (for the RHS `ReplaceVariables` step). `node_of` maps segment
/// positions to shape node indices; `e` is the target span's right-boundary segment position
/// (for the right-environment capture math in (3)); `target_nodes[k]` is target-pattern position
/// `k`'s own already-resolved shape-node index (the caller's own `node_of[s..e]`-derived list for
/// every dispatch kind except `ana_feature`, whose own per-row `Fst::get_offsets` recovery — see
/// `compile_lane_fst_grouped`'s doc — can legitimately be NON-contiguous in segment-position space
/// when an earlier rule's vacuous deletion-unapply left Optional segments interposed between this
/// rule's own real target positions).
///
/// `left_match`/`right_match` are the `Option<FstResult>` an environment's match produced (see
/// `left_env_match`/`right_env_match` — `None` when no environment was authored, `Some(result)`
/// when one matched; the caller must already have rejected a failed-to-match environment before
/// calling this).
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

    // (2) left environment: each var-bearing top-level node's matched segment is recovered from its
    // capture group (`EnvFst::group_names`), not a positional offset from the target — a
    // variable-width quantifier elsewhere in the pattern (between the var node and the target, or
    // anywhere else) changes how many *segments* the match consumed without changing which *node*
    // the variable is attached to, so the capture stays correct regardless (see `EnvFst::group_names`'s
    // doc for the verified C# comparison: C# has no analog of this recovery step at all, since its
    // arcs bind variables live during traversal). The env FST is compiled `LeftToRight` and matched
    // against the prefix slice `segs[..s]`, so a captured offset is already an absolute position in
    // `segs`/`node_of`.
    if let (Some(env), Some(result)) = (left, left_match) {
        for (i, occs) in env.node_vars.iter().enumerate() {
            if occs.is_empty() {
                continue;
            }
            let name = env.group_names[i]
                .as_deref()
                .expect("a var-bearing node was wrapped in a capture group at compile time");
            let Some((a, _b)) = env.fst.get_offsets(name, &result.registers) else {
                // Zero-width/unset: the node's capture never fired on this path. Not expected for a
                // mandatory single-segment `<SimpleContext>` (the only node kind that carries
                // `node_vars`), but fail open rather than mis-bind against a stale/wrong node.
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

    // (3) right environment: same capture-based recovery, over the suffix slice `segs[e..]` — a
    // captured offset is relative to that slice, so add `e` to land back in `segs`/`node_of` space.
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

/// Finding N2 (phase2 audit C): C#'s phonological rewrite-rule matcher constructs its `Matcher`
/// with `MatcherSettings.UseDefaults = true` (`Analysis/SynthesisRewriteRule.cs:29-37` — one of
/// the four call sites that set it; `AnalysisMetathesisRule`/`SynthesisMetathesisRule`'s are the
/// other two, unreached here since Metathesis is unported). That flag flows into `Fst.Transduce`
/// (`SIL.Machine/FiniteState/Fst.cs:283-330` -> `TraversalMethodBase._useDefaults` ->
/// `Input.Matches`, `SIL.Machine/FiniteState/Input.cs:49-64`) and from there into
/// `FeatureStruct.IsUnifiable`/`Subsumes` (`FeatureStruct.cs:994-1017,1085-1114`): for a feature
/// the *pattern* side pins that the *data* side leaves unset (no `_definite` entry), C# substitutes
/// the feature's `DefaultValue` for the unset side and re-checks unifiability/subsumption against
/// *that*, instead of treating "unset" as vacuously compatible with anything.
///
/// `pg_fst`'s frozen contract has no analog of "unset vs. explicitly full-mask" (this module's own
/// doc already flags this as a gap `pg_fst` "cannot apply"), so the FST's own match is
/// `useDefaults=false`-equivalent: a `full_mask` (this port's "unspecified") segment lane always
/// overlaps any LHS-pinned constraint, over-approximating exactly like every other confirm-step
/// gap this module already patches post-hoc (alpha variables via `resolve_bindings`, the
/// `Type`/`Modified`/`Deletion` symbolic features via the aux bits on `MutNode`). This function is
/// the analogous confirm step for `UseDefaults`: for each LHS-pinned feature at each matched target
/// position, if the actual node's lane is `full_mask` *and* the feature has a `default_symbol`
/// (`pg_grammar::featsys::PhonFeatureSystem::default_bits`), the candidate is only really valid if
/// the default's bits intersect the LHS's pinned bits — mirroring C#'s `else if (useDefaults &&
/// featVal.Key.DefaultValue != null)` branch.
///
/// **Scope note:** ported for the **Feature-kind subrule target pattern only** (`syn_feature` +
/// `ana_feature`), the dispatch kind C#'s `useDefaults` branch can actually influence a
/// feature-change decision through and the one the conformance fixture exercises. Not yet applied:
/// (a) environments — C# applies `UseDefaults` uniformly across target + both environments (one
/// `MatcherSettings` per rule), so a pattern pinning a defaulted feature only in an environment
/// would still over-match here; (b) the `Narrow`/`Epenthesis` dispatch kinds' target patterns —
/// same one-line confirm shape if ever needed. No known reference grammar or fixture exercises
/// either combination (no grammar in the corpus has `defaultSymbol` at all); real follow-on gaps if
/// one ever does, tracked here rather than blocking this port, consistent with this module's
/// existing environment-vs-target asymmetry documented on `resolve_bindings`.
///
/// `pattern_lanes[k]` is target position `k`'s **full** `W`-wide lane row (every feature, not just
/// the pinned ones — `node_full_lanes`'s shape, one row per target-pattern node position; for
/// synthesis this is the LHS's own lanes, for analysis it's `ana_feature_target_lanes`'s `LHS ⊕
/// RHS` combined target, matching whichever pattern the caller's `target: &Fst` was compiled from).
/// A feature is "pinned" at position `k` iff `pattern_lanes[k][f] != full_mask(g, f)`.
/// `target_nodes[k]` is target-pattern position `k`'s already-resolved shape-node index — see
/// `resolve_bindings`'s doc for why this isn't always a contiguous `node_of[s+k]` slice.
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

// =================================================================================================
// Subrule dispatch (C# LHS-vs-RHS child-count rule).
// =================================================================================================

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

/// The POS half of `subrule_applicable`, a unifiability test flattened to one symbolic comparison.
/// Either side absent is vacuously satisfied, since a feature present on only one side never
/// blocks unification; when both are present they must share at least one symbol. The mask
/// argument is unused on that arm, hence the `0`.
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

// =================================================================================================
// Public API.
// =================================================================================================

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
        // `rule.mode` selects the function pair for Feature and Narrow. Epenthesis reuses
        // `syn_epenthesis` for both: its collect-then-apply shape already matches Simultaneous
        // semantics and stands in for Iterative — see that function's own documented residual.
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
    // `pid` resolves this rule's OWN owning stratum's table, never an implicit table zero; the
    // fallback covers only an orphaned prule, unreachable via this cached path.
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
        // The cached matchers are identical for either mode of a given `Kind` — the Simultaneous
        // functions read the same compiled target and env FSTs; only the driving loop differs.
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

// =================================================================================================
// P12 chunk 6 — phonological rule tracing (synthesis side).
//
// C# `SynthesisRewriteRule.Apply` (`PhonologicalRules/SynthesisRewriteRule.cs:51-89`) does NOT trace
// at the point each subrule is tried. Instead it populates a per-subrule-index side channel,
// `Word.CurrentRuleResults: Dictionary<int, Tuple<FailureReason, object>>`
// (`SynthesisRewriteSubruleSpec.cs:31-83`: `IsApplicable`'s three gates — RequiredSyntacticFeatureStruct,
// then RequiredMprFeatures, then ExcludedMprFeatures, in that order — write the specific reason on
// failure; `MarkSuccessfulApply` overwrites the same slot with the `None` success sentinel once a
// subrule's `ApplyRhs` actually fires), then reads it back out AFTER the whole
// `_patternRule.Apply(input)` call finishes: for i in 0..subrules.len(), an absent dict entry reports
// the `Pattern` fallback, a recorded gate reason reports `NotApplied(reason)`, and the FIRST
// `None`-marked (successful) index reports `Applied` and BREAKS — no further subrule in the same rule
// is ever reported, even one this port's own architecture actually attempted (see this module's doc
// for how Rust's per-subrule-scans-the-whole-word loop differs from C#'s per-position priority race;
// `IsApplicable`'s three gates are themselves position-independent — a pure function of the word's
// `SyntacticFeatureStruct`/`MprFeatures`, never the match position — so they map exactly 1:1 onto this
// port's `subrule_applicable`, checked once per subrule regardless of architecture).
// =================================================================================================

/// One subrule's outcome for the traced synthesis functions below — this port's concrete
/// representation of C#'s `CurrentRuleResults[i]` side channel. Always populated (every subrule is
/// visited exactly once by the existing `for sr in &rule.subrules` loop, unlike C#'s dictionary,
/// which can genuinely have no entry for an index — see `Pattern`'s doc on `FailureReason` for why
/// that C# "absent" case and this port's own `NotApplied(FailureReason::Pattern)` read back
/// identically at the `report_subrule_outcomes` call site: both fire the same trace event).
#[derive(Clone, Copy)]
enum SubruleOutcome {
    Applied,
    NotApplied(FailureReason),
}

/// `SynthesisRewriteSubruleSpec::IsApplicable`'s three gates, decomposed to name WHICH one failed
/// (`SynthesisRewriteSubruleSpec.cs:31-77`) — a read-only re-derivation of exactly what
/// `subrule_applicable` already checks (that function itself is untouched; this just re-runs its
/// two halves separately so a caller can report the specific reason). `None` means the gate passed.
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

/// The shared readout tail: given every subrule's outcome in index order, fire C#'s trace events in
/// C#'s order, with its early stop on the first applied subrule.
///
/// `out_word` is deliberately ONE snapshot passed to every call, because C# reuses one mutated word
/// reference for the whole readout — which runs after the rule has finished mutating in place, so
/// even a failed subrule reports the rule's FINAL state rather than the state when it was tried.
/// That is a verified C# quirk, not an approximation here. Neither trace method reassigns the
/// cursor, so the returned handle is discarded.
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
        // Same `(Kind, rule.mode)` dispatch as `synthesize_with_mpr` (§4.2) -- recompiled per call.
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
        // Same `(Kind, rule.mode)` dispatch as `synthesize_with_mpr_cached` (§4.2).
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
        // `sr.self_opaquing` — Feature and Epenthesis only, always false for Narrow — gates a
        // repeat-until-fixpoint loop around the single-pass call, exactly as C# repeats the same
        // unchanged apply until it makes no further change. Both functions already report whether
        // anything changed, so the loop condition falls out directly.
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
                    // Reuse the epenthesis target-lane formula ("the RHS segment sequence,
                    // FST-facing") — see `ana_narrow_general`'s doc.
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

// =================================================================================================
// Phonological rule tracing, analysis side.
//
// Unlike synthesis's post-hoc readout, C# traces INLINE per subrule: try it, then immediately fire
// unapplied or not-unapplied. Neither event carries a `FailureReason`, because analysis has no
// MPR/POS gate to attribute a failure to, so there is nothing to decompose. A single post-subrule
// snapshot serves both branches.
// =================================================================================================

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

// =================================================================================================
// Feature-change (LHS.count == RHS.count).
// =================================================================================================

/// C# `FeatureSynthesisRewriteSubruleSpec.ApplyRhs` inside `IterativePhonologicalPatternRule`:
/// match the LHS, and for each target node priority-union the corresponding RHS constraint onto its
/// features (`b` wins). Iterative + `Modified=Clean` ⇒ each node is rewritten at most once.
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
    // The LHS's full per-position lane rows, needed only by `pattern_defaults_ok`. Full rows, not
    // sparse pins: that check must tell "pinned to X" apart from "unpinned" by mask comparison.
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
        // First span in the target's own scan order whose nodes are all clean and whose
        // environments hold. See `ordered_spans`.
        let mut acted = false;
        for (s, e) in ordered_spans(target, &segs) {
            let target_nodes: Vec<usize> = node_of[s..e].to_vec();
            // Reject an over-wide Optional-skip artifact before the positional `rhs_pins[k]` index
            // below, which would otherwise panic on a multi-node target abutting a boundary.
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
            // Alpha-variable agreement over target + environments (the frozen FST over-approximated
            // variable lanes; reject candidates that violate a binding).
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
            // Finding N2 (UseDefaults): reject a candidate the FST only accepted because an
            // LHS-pinned feature is unspecified on the actual node AND that feature's default
            // value wouldn't itself have satisfied the pin — see `pattern_defaults_ok`'s doc.
            if !pattern_defaults_ok(g, ms, &target_nodes, &lhs_lanes) {
                continue;
            }
            // ApplyRhs: priority-union each RHS constraint onto the target node, then apply any
            // RHS alpha variables using the resolved bindings (C# `PriorityUnion(fs, varBindings)`
            // / `ReplaceVariables`).
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
                // Rewriting a node's phonological features breaks the correspondence between its
                // ORIGINAL literal `char_def` (e.g. Indonesian's `meN-` archiphoneme, char29) and
                // its now-current feature bundle (post nasal-place-assimilation, indistinguishable
                // from a real "m"/"n"/"ng"). C#'s `CharacterDefinitionTable.GetMatchingStrReps`
                // (CharacterDefinitionTable.cs:96-106) has no notion of a node's "original identity"
                // at all — it is *always* `cd.FeatureStruct.IsUnifiable(node.FeatureStruct)` freshly
                // evaluated against every table entry from the node's current features. This port's
                // `pg_shape::Shape::node_cd_set` (plan §13.1 Tier-1 #3), added to fix a *different*
                // real bug (Sena's zero-phonological-feature grammar: every char-def has empty
                // lanes, so unconstrained unification would match the *entire* table for every
                // node), took the shortcut of treating any node with a concrete `char_def` as an
                // immutable singleton restricted to that one char-def's own representations forever
                // — correct for a node whose char_def was never touched again, but wrong for a node
                // a feature-change phonological rule later rewrites: `matching_str_reps`
                // (`pg-parse/src/surface.rs`) would then only ever consider archiphoneme char29's
                // own (fixed) representations, find that its *rewritten* feature lanes no longer
                // unify with char29's own literal spec (place changed), get zero candidates, and
                // silently render nothing for that position — the archiphoneme visibly vanishing
                // from the surface instead of becoming "m" (confirmed: this dropped exactly the
                // assimilated nasal from Indonesian `memakai`'s resynthesized surface, "me+?akai"
                // instead of "mem+?akai", so `Morpher.IsMatch` could never succeed regardless of how
                // correct the rest of analysis/synthesis was). Clearing `char_def` to `NO_CHAR_DEF`
                // here makes `node_cd_set` fall through to the node's (default `Unrestricted`)
                // stored `CdSet` — i.e. plain global feature unification, matching C# exactly — for
                // precisely the nodes a feature-change rule actually touched; every untouched node
                // (the overwhelming majority, including literal lexical segments) keeps its identity
                // lock, so this does not reopen the Sena bug the lock exists to prevent.
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

/// P13 §4.1: the Simultaneous sibling of `syn_feature` — C#
/// `SimultaneousPhonologicalPatternRule.Apply`: `Matcher.AllMatches(input)` (every candidate span,
/// target+environments env-checked against the SAME pristine snapshot), THEN `ApplyRhs` every
/// accepted match. Shares every helper `syn_feature` uses (`node_pins`/`node_full_lanes`/
/// `resolve_bindings`/`pattern_defaults_ok`); the only difference is the outer loop shape: ONE
/// `ms.segs(true)` snapshot (no rescan-after-each-application loop), collect every accepted
/// candidate first, apply all of them afterward. A node dirtied by an EARLIER subrule of this same
/// rule (this function's own caller loop shares one `MutShape` across subrules, `synthesize_with_mpr`/
/// `synthesize_with_mpr_cached`'s `for sr in &rule.subrules`) is still excluded here, same as
/// `syn_feature` — see §4.1's multi-subrule-disjunction warning / the
/// `simultaneous_two_subrules_first_wins_at_overlapping_position` gate test for why this
/// per-subrule-sequential architecture still reproduces C#'s first-applicable-subrule-wins
/// semantics even though each subrule gets its own collect-then-apply pass rather than one pass
/// shared across the whole rule.
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

    // ONE snapshot, before any mutation -- every candidate's target+environment match is checked
    // against this same pristine snapshot (C#: `Matcher.AllMatches(input)` + `MatchSubrule`, both
    // run before any `ApplyRhs`).
    let (segs, node_of) = ms.segs(true);
    let mut accepted: Vec<(Vec<usize>, Bindings)> = Vec::new();
    for (s, e) in all_spans(target, &segs) {
        let target_nodes: Vec<usize> = node_of[s..e].to_vec();
        if !width_matches(&target_nodes, rhs_pins.len()) {
            continue;
        }
        // `dirty` still gates out a node an EARLIER subrule of this same rule already touched (see
        // this function's doc) -- nothing within this single pass can have gone dirty yet, since
        // collection happens entirely before any of THIS call's own applications.
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
    // THEN apply every accepted candidate. Only the MATCHING phase above is snapshot-based; this
    // one mutates progressively, exactly like C#'s shared-word apply loop. The difference is
    // observable only for overlapping target spans.
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

/// The `ana_feature` target FST's per-node lanes — the FST-facing `LHS ⊕ RHS` priority-union —
/// factored out so cache construction can compile this target once rather than per call.
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

/// C# `FeatureAnalysisRewriteRuleSpec`: the analysis matcher's target is the `LHS ⊕ RHS`
/// priority-union, and unapplying underspecifies each feature the RHS changed. Direction is
/// reversed relative to synthesis, and nondeterministic.
///
/// `target` must come from `compile_lane_fst_grouped`, never the plain `compile_lane_fst`, and its
/// rows must be recovered through their own groups. A positional `node_of[s..e]` slice assumes the
/// pattern's N rows land on N physically contiguous segments; an earlier rule's vacuous unapply can
/// interpose an Optional segment between two of this rule's real target positions, at which point
/// every candidate span is over-wide, no tight alternative exists, `width_matches` discards them
/// all, and the rule silently unapplies nothing.
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
    // RHS alpha-variable occurrences, positionally aligned to `sr.rhs.nodes` (needed by both the
    // target pattern — see `ana_feature_target_lanes` — and the changed-feature set below).
    let rhs_vars = pattern_var_occurrences(&sr.rhs);

    // Recomputed rather than threaded in: the same recompile-per-call tradeoff `analyze` makes.
    let target_lanes = ana_feature_target_lanes(g, table, rule, sr);

    // The features each RHS node changed, paired with the bits to OR onto the node's matched value
    // on unapply. C# does NOT reset a changed feature to the full symbol mask — it negates, then
    // subtracts the LHS negation, then unions onto the current value. In bitset terms, for a
    // literal RHS pin `R` against an LHS pin `L` that reduces to `L | R`, which is a STRICT SUBSET
    // of the full mask whenever the feature has a third symbol neither side mentions. Resetting to
    // the full mask wrongly accepts that untouched symbol; the two coincide only on a 2-symbol
    // feature, which is exactly why the difference hides on most grammars.
    //
    // An alpha-governed RHS feature keeps the full-mask fallback: its bound value is unknown until
    // match time. It must still be listed as changed even though `node_pins` deliberately omits
    // alpha features — otherwise a rule whose only literal RHS pin equals its LHS value looks
    // vacuous and never fires at all.
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

    // The analysis target is `LHS ⊕ RHS`; a variable governing a *changed* feature no longer pins a
    // matchable lane (it was unspecified by the change), so agreement on unapply is keyed off the
    // LHS variables that survive on unchanged features plus the environment variables.
    let lhs_vars = pattern_var_occurrences(&rule.lhs);

    // Which tag half is trustworthy is DIRECTION-DEPENDENT. A spawned Optional skip re-executes a
    // row's tag commands at a widened offset, corrupting exactly one raw half but never the other,
    // and never a row's entering tag. Left-to-right traversal visits rows in document order, so a
    // row's START is always the fresh one. Right-to-left visits them reversed AND `get_offsets`
    // swaps the pair for that direction, so there the reported END is the fresh one instead.
    //
    // The left-to-right branch has no full-pipeline coverage: this target is always compiled in the
    // reverse of the rule's own direction, and a rule declaring right-to-left is unexercised. It is
    // pinned only by `group_probe_diag`'s synthetic unit test.
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

        // `filter_map` drops a match whose group offsets didn't all resolve (not expected for an
        // accepting match against this compiled target, but fail open rather than index a
        // missing position); sort+dedup gives a stable, deduped ordering (lexicographic on
        // `Vec<usize>` sorts by the first row's position first, mirroring `all_spans`' own
        // ascending `(s, e)` sort). Direction fix: this loop is the analysis-side Iterative
        // pick-one-then-rescan loop (`ordered_spans`'s doc), so the candidate a scan actually
        // reaches first must follow `target.direction()`'s own scan order, not always the
        // ascending one -- `rtl` (already computed above for `recover_pos`) reverses the sorted
        // list into descending (rightmost-first) order when this target's compiled direction is
        // `RightToLeft`, exactly like `ordered_spans` does for the `(s, e)`-tuple callers.
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
            // Alpha-variable agreement (C# threads `match.VariableBindings` through the analysis
            // matcher and env matchers exactly as synthesis does). Reject violating candidates.
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
            // Finding N2 (UseDefaults): see `pattern_defaults_ok`'s doc / `syn_feature`'s analogous
            // call — same confirm step, against the analysis target's combined `LHS ⊕ RHS` lanes.
            if !pattern_defaults_ok(g, ms, &target_nodes, &target_lanes) {
                continue;
            }
            // IsUnapplicationNonvacuous (`FeatureAnalysisRewriteRuleSpec.cs:63-99`): nonvacuous iff
            // some changed feature's current value does NOT already **superset** the value being
            // OR'd in (`!nodeSfv.IsSupersetOf(sfv)`) — i.e. applying the OR would actually add bits.
            // Reduces to the pre-fix `!= full_mask` check exactly when `neg == full_mask` (the
            // alpha-governed case, unchanged); for the literal `L & !R` case it correctly detects
            // "would add new bits" instead of "isn't already fully unconstrained".
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
                // Root-cause fix (this file's module doc "major finding"): mirror `syn_feature`'s
                // identical `char_def = NO_CHAR_DEF` clearing (line ~1051) / `metathesis.rs`'s same
                // pattern. Before this fix the node's literal `char_def` (its as-segmented surface
                // identity, e.g. "v") survived the lane-widening above unchanged, so
                // `RootAllomorphIndex::search`'s char_def-equality gate could never find a lexical
                // root whose underlying segment differs from the surface (e.g. root "p" for a
                // widened "v"). C# has no such literal-identity concept at all here: unapply only
                // ever produces a `FeatureStruct`, and root lookup always re-derives candidate
                // string representations from CURRENT lanes (`CharacterDefinitionTable
                // .GetMatchingStrReps`'s per-call `IsUnifiable`). Clearing to `NO_CHAR_DEF` makes
                // `Shape::node_cd_set` fall through to `CdSet::Unrestricted` (this pipeline's
                // `MutNode` carries no separate cd_set column -- `freeze_to_shape` always pushes
                // `NO_CHAR_DEF` segments via the plain `Unrestricted` path, same as `syn_feature`'s
                // proven fix), so lookup falls back to pure lane unification -- exactly matching C#.
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

// =================================================================================================
// Narrow: deletion (RHS empty) / narrowing / expansion.
// =================================================================================================

/// C# `NarrowSynthesisRewriteSubruleSpec.ApplyRhs`: insert the RHS nodes after `range.End`, then
/// delete the target nodes. For a pure deletion rule (RHS empty) this just deletes the targets.
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
    // C# `NarrowSynthesisRewriteSubruleSpec.ApplyRhs`: `AddAfter` inserts each RHS node
    // non-optional, then `SetDirty(true)` is called separately (when iterative) — the RHS is
    // never made skippable. See `NarrowSynthesisRewriteSubruleSpec.cs:31-45`. `rhs_nodes_base` is
    // the per-match template (var-governed lanes not yet resolved — see below); it is cloned and
    // resolved fresh for each accepted match, since the bound values differ per match.
    let rhs_nodes_base: Vec<MutNode> = sr
        .rhs
        .nodes
        .iter()
        .map(|n| new_seg_node_dirty(g, table, n, false, true))
        .collect();
    // The RHS gets the same alpha-variable resolution `syn_feature` does; narrowing has no
    // separate C# code path for it. Skipping it leaves a narrowing RHS natural class that carries
    // an alpha variable bound from a merged LHS segment sitting at full-unconstrained.
    let lhs_vars = pattern_var_occurrences(&rule.lhs);
    let rhs_vars = pattern_var_occurrences(&sr.rhs);

    let mut applied = false;
    loop {
        let (segs, node_of) = ms.segs(true);
        let mut acted = false;
        // Direction-ordered scan; see `ordered_spans`.
        for (s, e) in ordered_spans(target, &segs) {
            let target_nodes: Vec<usize> = node_of[s..e].to_vec();
            // An over-wide Optional-skip span would delete more physical nodes than the LHS
            // matched — a silent wrong mutation here, not a panic, since there is no positional
            // array to overrun. See `width_matches`.
            if !width_matches(&target_nodes, rule.lhs.nodes.len()) {
                continue;
            }
            // Widened to allow a matched Boundary node (Amharic `prule7`'s LHS is
            // `[SimpleContext, BoundaryMarker, SimpleContext]` — a real top-level boundary
            // element, not an environment): a target span may legitimately include one. C#'s
            // synthesis matcher filter is `Segment|Boundary|Anchor` (`SynthesisRewriteRule.cs:26`),
            // so its narrowing target can consume a boundary the same way.
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
            // Alpha-variable agreement over target + environments (mirrors `syn_feature`'s
            // identical step); also the source of bindings the RHS resolution below reads.
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

/// P13 §4.1: the Simultaneous sibling of `syn_narrow` -- same collect-against-one-snapshot,
/// apply-descending shape as `sim_feature`, transposed to narrowing/expansion's splice-then-delete
/// body. Applying in DESCENDING target-span order (by match start, i.e. `.rev()` over the
/// ascending `all_spans` order) matters here in a way it does not for `sim_feature`: a narrowing
/// application changes node COUNT (splice in the RHS, delete the LHS span), so an earlier (lower-
/// index) application's splice/delete would shift a not-yet-applied later match's own captured
/// node indices if applied ascending -- the same reason `ana_narrow_deletion`/`ana_narrow_general`
/// already apply descending. `sim_feature` never needs this (feature rewrites mutate node CONTENTS
/// in place, never indices).
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

/// The empty-RHS branch of C#'s narrowing analysis: match a single site node, then re-insert the
/// LHS segments as **optional** after it. Sites are collected against the un-mutated shape and
/// applied descending, mirroring C#'s all-matches-then-apply.
///
/// Which branch to take is decided at each call site rather than in a shared wrapper, matching this
/// module's convention that a call site compiles its own target — the general case needs one, this
/// case does not.
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

    // Sites: gaps after each segment node where the (deleted) LHS could sit and the environments
    // hold. Insertion goes after the site node; left env ends at the site (inclusive), right env
    // begins after it.
    let (segs, node_of) = ms.segs(false);
    let mut sites: Vec<usize> = Vec::new(); // shape node index after which to insert

    // The word-initial gap is a real site: C#'s substitute pattern matches the left anchor itself
    // and inserts right after it. Node 0 is always that anchor here. Without this, a word-initial
    // deletion can never be re-inserted by analysis at all.
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

/// The non-empty-RHS branch, covering both narrowing and expansion. Unlike `ana_narrow_deletion`,
/// the matcher's target is the RHS's OWN constraints — not a LHS-vs-RHS priority union as in the
/// feature case — matched in the reversed direction, nondeterministically. On a match, the original
/// un-narrowed LHS constraints are cloned, have the match's alpha bindings resolved onto them, and
/// are spliced in right after the match as OPTIONAL nodes; the RHS-matched nodes are marked
/// optional too, not deleted.
///
/// Matches are found against the pristine shape, then applied descending. C#'s linked-list nodes
/// survive insertion; this port's index-based nodes do not, so descending order is the index-safe
/// equivalent. Documented residual: a multi-node RHS with overlapping candidate spans would need
/// real non-overlap interval selection, and gets none.
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
    // The matched pattern here is the RHS, so its own alpha-variable occurrences are what get
    // bound during matching (target position k ↔ RHS node k); the reconstructed LHS nodes are what
    // consume those bindings on insertion.
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
        // Dropping an over-wide span discards only a duplicate here, since `all_spans` reports the
        // tight match for the same target too. Without the guard, on Optional-flooded analysis
        // shapes a single-node target spuriously matches whole multi-segment windows, marks each
        // Optional and reconstructs at every one — a compounding flood C# never has, because its
        // target pattern binds each position with a group capture.
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

    // Apply descending (by match start) so earlier splices don't shift not-yet-applied matches'
    // node indices.
    for (s, e, bindings) in matches.into_iter().rev() {
        let target_nodes: Vec<usize> = node_of[s..e].to_vec();
        // (1) splice the reconstructed original-LHS material in right after the match, with any
        // alpha-variable bindings resolved from the RHS match applied to it.
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

// =================================================================================================
// Epenthesis (LHS empty).
// =================================================================================================

/// C# `EpenthesisSynthesisRewriteSubruleSpec.ApplyRhs`: at each site where the environments hold,
/// insert the RHS nodes. Iterative; the inserted nodes are marked dirty so the site is not
/// re-matched (plus a node-count guard mirroring C#'s `InfiniteLoopException`).
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

    // Collect the sites (gaps after a segment node) where both environments hold, against the
    // current shape, then insert once per site (descending). Epenthesis in the reference grammars
    // is between fixed contexts and does not cascade.
    let (segs, node_of) = ms.segs(true);
    let mut sites: Vec<usize> = Vec::new();
    // The word-initial gap is a real site, the synthesis twin of `ana_narrow_deletion`'s: C#'s
    // empty-LHS pattern matches the left anchor itself and inserts right after it. Anchors never
    // appear in `node_of`, so the per-segment loop below cannot reach this gap, and without this a
    // word-initial epenthesis can never fire during synthesis-confirm.
    if left_env_ok(left, &segs, 0) && right_env_ok(right, &segs, 0) {
        sites.push(0);
    }
    for (site, &node) in node_of.iter().enumerate() {
        // A boundary appears in `node_of` because `segs(true)` feeds boundaries in as
        // transparently-skippable Optional segments, so an environment can see through one. It is
        // never itself a valid epenthesis site: C#'s empty-LHS pattern admits segments and anchors
        // only. Counting a boundary's slot double-counts, because the preceding real segment's own
        // site already reaches past it via that same transparent skip.
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

/// The `ana_epenthesis` target FST's per-node lanes (the RHS segment sequence, FST-facing),
/// factored out so [`RuleCache`](crate::cache::RuleCache) construction can compile this target
/// exactly once. Empty (no RHS at all — a degenerate no-op subrule) means no target is compiled,
/// mirroring `ana_epenthesis`'s own `target_lanes.is_empty()` early return.
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

/// C# `EpenthesisAnalysisRewriteRuleSpec` (reapply Normal/SelfOpaquing): the analysis matcher
/// matches the epenthesized segment(s), and `Unapply` marks them **optional** (so a later lexical
/// lookup may skip them) — it does not delete. The nonvacuous guard skips already-optional nodes.
///
/// **INVESTIGATED: `tests/phase_c_right_to_left.rs`'s reported oracle gap does not reproduce.**
/// That file's own top doc ("Known, out-of-scope oracle gap" area / its epenthesis test's doc)
/// reports a throwaway (not checked in) finding that `pg_parse::Morpher` returns NO analysis at all
/// for ANY word of its `RTL_EPENTHESIS_XML` fixture — including `entryXOnly`'s own unaffected
/// spelling `"x"`, which this rule's environment never even licenses — reproducing byte-identically
/// whether the rule is declared plain `LeftToRight` or `rightToLeftIterative`. Re-investigated here
/// (throwaway probes, not checked in, per this port's "reproduce yourself first" discipline): that
/// EXACT fixture (byte-for-byte, both direction variants) was loaded and run two ways — (1)
/// `analyze` called directly (bypassing `Morpher`/the stratum cascade entirely) and (2) the full
/// `pg_parse::Morpher::parse_word_opts` pipeline. Both ways, both directions, all three surface
/// forms return the oracle-correct result: `"x"` recalls `entryXOnly` unchanged (1 analysis),
/// `"xey"` recalls `entryXY` with its medial segment marked Optional (1 analysis), `"xy"` (the raw,
/// pre-epenthesis spelling of an obligatory rule) recalls nothing (0 analyses, correctly — an
/// obligatory epenthesis rule's un-rewritten input must never itself be valid). `git log` confirms
/// neither this function, `stratum.rs`'s prule cascade (`StratumAnalyzer::analyze`), nor
/// `morpher.rs` has changed since the commit that introduced that fixture, so this is not a case of
/// an intervening fix elsewhere quietly closing the gap — the described symptom simply could not be
/// reproduced against the code as it exists. Left as a documented non-finding, not a guessed fix for
/// an unobservable symptom (this module's own "reproduce first" discipline): the pre-existing
/// `pg-rules/tests/rewrite_gate.rs` epenthesis gates plus this crate's own
/// `epenthesis_natural_class_rhs_round_trips_with_environment` (added alongside this note, using a
/// `PatternNode::Context` RHS + explicit two-sided environment — the same shape as the cited
/// fixture, rather than the pre-existing gates' concrete-`CharDef` RHS) keep pinning correct
/// behavior for this shape in both directions.
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
        // Width guard (plan §6 item 1): reject an over-wide Optional-skip span before it can mark
        // the wrong (extra) node Optional below — see `width_matches`'s doc.
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

// =================================================================================================
// Small shared builders.
// =================================================================================================

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

/// Convert driver full-mask lanes to FST-facing lanes (unconstrained `full_mask` → `u64::MAX`), so
/// the compiled constraint canonicalizes and matches identically.
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

/// Build a fresh segment `MutNode` from an RHS/LHS pattern node (Context/CharDef). `optional`
/// sets the Optional flag (used by the analysis re-insertion of deleted segments).
///
/// KNOWN RESIDUAL (plan §13.1 Tier-1 #3): a `PatternNode::Context` RHS (epenthesis of a natural
/// class) produces a `NO_CHAR_DEF` node here that `freeze_to_shape` (below) pushes via the plain
/// `push_segment_with_lanes`/`push_boundary_with_lanes` path, i.e. `CdSet::Unrestricted` — the same
/// pre-fix over-approximation `pg_rules::morph`'s `InsertSimpleContext` sites had before this
/// milestone's fix. Left unported here because `Kind::Epenthesis` has **zero occurrences** in all
/// three reference grammars' `<RewriteRule>`s (confirmed by direct grep), so this path is
/// unexercised, not silently wrong on any word that matters — flagged per this module's existing
/// scope-note convention rather than plumbed through speculatively.
fn new_seg_node(g: &Grammar, table: &CharDefTable, node: &PatternNode, optional: bool) -> MutNode {
    // C# `AddAfter` never sets `Optional`; callers that need it set explicitly do so themselves
    // (e.g. `ana_narrow`'s re-inserted deleted segment). `dirty` tracks C#'s separate
    // `SetDirty(true)` call and historically was conflated with `optional` here — use
    // `new_seg_node_dirty` when the two need to differ (see `syn_narrow`).
    new_seg_node_dirty(g, table, node, optional, optional)
}

/// Like `new_seg_node` but lets `dirty` be set independently of `optional`, mirroring C#'s
/// `ShapeNode.AddAfter` (never optional by default) + a separate `SetDirty(true)` call
/// (`NarrowSynthesisRewriteSubruleSpec.cs:39-41`).
///
/// `kind` is resolved from the char-def table for a `PatternNode::CharDef` (a `<BoundaryMarker>`
/// node is a real, distinct top-level LHS/RHS element in Amharic's `prule7` — "CV merger at
/// morpheme boundaries" — whose LHS is `[SimpleContext, BoundaryMarker, SimpleContext]`, so
/// reconstructing it on analysis-unapply must re-insert an actual `NodeKind::Boundary`, not a
/// phantom segment). `PatternNode::Context` (a natural-class reference) is always `Segment` — C#
/// `NaturalClass.cs` unconditionally injects `Type=Segment` into every natural class, so a
/// natural-class-driven node can never denote a boundary.
///
/// Getting this wrong is not cosmetic: a boundary char-def has **no phonological features**, so
/// `node_full_lanes` yields all-full-mask lanes for it; mis-kinded as `Segment` it enters the
/// analysis matchers' `ms.segs(false)` universe as a **wildcard segment** that unifies with any
/// affix material — measured on ሄደ, that one wildcard let every clitic/affix rule in the deepest
/// stratum "unapply" everywhere (e.g. በ= consuming the phantom `+`), exploding the combination
/// walk to ~9,000 expanded states / >100k budget ticks where C#'s identical walk visits ~38
/// states. C#'s reconstructed node carries `Type=Boundary` and its analysis matcher filter is
/// `Segment|Anchor` only (`AnalysisRewriteRule.cs:34`), so the boundary is simply invisible —
/// `NodeKind::Boundary` here reproduces that exactly (Boundary nodes are excluded from
/// `segs(false)` and re-emitted by `freeze_to_shape`'s boundary path).
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

// =================================================================================================
// Compile-once cache (plan §13.2 step 5; `crate::cache::RuleCache`'s per-phonological-rule slice).
// =================================================================================================

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
                // The general-narrowing/expansion case (`ana_narrow_general`, `sr.rhs.nodes`
                // non-empty) matches the RHS via a target FST whose lane formula is textually
                // identical to `ana_epenthesis_target_lanes` (both are "the RHS segment sequence,
                // FST-facing") — reused here rather than duplicated. The pure-deletion case
                // (`ana_narrow_deletion`, `sr.rhs.nodes` empty) still compiles no FST at all.
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

// =================================================================================================
// F2 prerequisite (HYBRID_FST_RUST_PLAN.md §7.1 item 2a) -- probing-only synthesis for
// `hc_hybrid::surface`'s `SurfacePhonology` port.
//
// C# never physically removes a node on deletion -- `Annotation.FeatureStruct[Deletion] ==
// Deleted` is an ANNOTATION, so a node's POSITION in the shape's node list is stable for the
// entire life of a `Word`, across as many phonological rules as run over it (`SurfacePhonology.
// RenderNodes` / `SurfaceNodes` rely on exactly this: `outNodes.Skip(1)`/`.Take(underlyingLen)`
// slice by fixed position regardless of what deleted in between). This port's real
// `syn_narrow`/`sim_narrow` (above) physically `Vec::remove`/`splice` instead -- correct and
// deliberately UNCHANGED for the real per-word pipeline, which never needs cross-rule position
// stability (each pipeline call starts from a freshly re-segmented, freshly-frozen `Shape`).
//
// The functions below are text-identical to `syn_narrow`/`sim_narrow` except the matched-and-
// deleted target span is soft-marked (`ms.nodes[n].deleted = true`, left in place) instead of
// `Vec::remove`'d; RHS insertion (`ms.nodes.splice`) is untouched. This reproduces C#'s own node
// -COUNT arithmetic exactly: a pure-deletion subrule (empty RHS) never changes total node count
// (matching C#, where deleted nodes are never removed); a subrule with a non-empty RHS increases
// total node count by the RHS length regardless of how many LHS nodes it "replaces" (matching C#,
// where the new RHS nodes are REAL insertions on top of the still-present-but-deleted LHS span).
// `hc_hybrid::surface`'s own final segment-count check (mirroring `SurfacePhonology.cs:152`'s
// `outNodes.Count != underlyingLen + extra`) is therefore sufficient to reject every insertion
// case exactly where C# would, without this module needing any special-case "bail" logic of its
// own -- see the F2 commit message / advisor discussion for the arithmetic argument in full.
//
// `Kind::Epenthesis` (an empty-LHS rule) has no C# analog here at all: it inserts nodes with no
// originating position whatsoever, which this position-preserving model has nothing to anchor
// them to. None of the three reference grammars (Indonesian/Sena/Amharic) has such a rule
// (verified: every `PhonologicalRule`'s `PhoneticInput` is non-empty) or a `MetathesisRule`
// (verified: zero `<MetathesisRule>` elements in any of the three), so reaching either here
// returns `ProbeOutcome::Refused` rather than silently mis-tracking positions -- a conservative
// stance for a case the gate grammars never exercise, not a gap in the covered cases.

/// Soft-delete sibling of `syn_narrow` (Iterative Narrow, i.e. deletion/narrowing/expansion).
/// See the module note above for exactly what differs (one line: `deleted = true` instead of
/// `nodes.remove`) and why that's sufficient.
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
        // Direction-ordered scan (see `ordered_spans`'s doc) -- same fix as `syn_narrow`, whose
        // soft-delete sibling this function is.
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
            // Soft-delete the target nodes instead of removing them (the one line that differs
            // from `syn_narrow`) -- descending order not required for a mark-in-place, but kept for
            // textual parallelism with the original.
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
    /// An empty-LHS (`Kind::Epenthesis`) rule was reached -- see the module note above for why
    /// this is refused rather than approximated (unreachable on the three reference grammars).
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
/// (`crate::cache`'s own module doc). Recompiling `lhs_fst`/`compile_env` per call here (an earlier,
/// uncached draft of this function) made Amharic's 417-segment-alphabet probing impractically slow
/// (well past the C# ~112s figure the plan flags as a KNOWN, deliberately-deferred cost -- this was a
/// SEPARATE, avoidable inefficiency on top of that, not the same pathology, and had to be fixed to
/// make the F2 gate runnable at all).
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
                // Unreachable on the three reference grammars (verified: zero `<MetathesisRule>`s);
                // refuse rather than silently mis-track positions if one is ever added.
                return ProbeOutcome::Refused;
            }
        }
    }
    ProbeOutcome::NoMatch // caller only inspects `Refused` vs. not; see `probe_synthesize_all_strata`.
}

/// Regression pins for the P6 fix's low-level premise: `compile_lane_fst_grouped`'s per-row
/// `Group` capture correctly recovers each row's REAL matched segment position even when an
/// Optional non-matching segment (e.g. one just inserted by an earlier rule's vacuous
/// `ana_narrow_deletion` unapply) sits directly between two rows -- and that WHICH tag half
/// (start vs end) is reliable flips with compile direction, exactly as `ana_feature`'s
/// `recover_pos` closure assumes. Both tests use the same shape: a real/no-match segment, then
/// alternating real/match and Optional/no-match segments, so a 2-row "adjacent real match"
/// target must transparently skip the Optional to find its second row.
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

    /// `LeftToRight`: each row's START offset is reliable (the entering tag is always freshly
    /// computed); only a row's END can be widened by a *following* skip -- see
    /// `compile_lane_fst_grouped`'s doc. Real matches here are at seg positions 1, 3, 5;
    /// expect the two adjacent real-match pairs (1,3) and (3,5), both correctly identified via
    /// each row's start.
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

    /// `RightToLeft` (the ACTUAL direction `ana_feature`'s target compiles under -- analysis
    /// always uses `reverse(dir_of(rule))`, and every reference-grammar rule's declared `dir`
    /// defaults to `LeftToRight`): the compiled node order is document-reversed AND
    /// `Fst::get_offsets` swaps `(start,end)` back for this direction, so it is each row's END
    /// (not start) that is reliable -- the opposite of the LTR case above. Same real-match
    /// positions (1, 3, 5) and expected pairs (1,3), (3,5), now read via `.1 - 1`.
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
