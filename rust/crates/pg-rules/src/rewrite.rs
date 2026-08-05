//! Part 2 — rewrite (phonological) rule application: synthesis (forward) and analysis (reverse).
//!
//! Ports `SIL.Machine.Morphology.HermitCrab/PhonologicalRules/` at the **rule level** (the full
//! Morpher is a later milestone): given a `pg_grammar::model::RewriteRuleDef` and a
//! feature-bearing input `Shape`, `synthesize` applies the rule forward and `analyze`
//! un-applies it. The three rewrite shapes the reference grammars use — feature-change,
//! deletion/narrowing, and epenthesis — are dispatched by the C# LHS-vs-RHS child-count rule
//! (`AnalysisRewriteRule` ctor / `SynthesisRewriteRuleSpec` ctor).
//!
//! ## Model impedance (see the module report for the flagged gaps)
//! HermitCrab threads three engine-only symbolic features through matching that the frozen
//! `pg_shape`/`pg_fst` contracts do **not** encode as lanes:
//! - **`Type`** (Segment/Boundary/Anchor) → `NodeKind` + which nodes are fed to the matcher
//!   (synthesis: Segment+Boundary; analysis: Segment only) + the anchor endpoints;
//! - **`Modified`** (Dirty/Clean) → an aux `dirty` bit on `MutNode`; the iterative loop's primary
//!   termination is the cursor advance (C# `start = end.Next`), with `dirty` as the re-match guard
//!   that the iterative-synthesis `Modified=Clean` LHS constraint provides;
//! - **`Deletion`** (Deleted/NotDeleted) → an aux `deleted` bit (synthesis) / physical delete.
//!
//! The FST matcher (`pg_fst`) also cannot bind alpha variables or apply `UseDefaults` feature
//! defaults; those are reported as frozen-contract gaps. The hand-built gate rules avoid them.

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
// Mutable working shape (C# `Shape` with Optional / Deleted / Dirty flags, which the frozen
// `pg_shape::Shape` does not carry).
// =================================================================================================

// `pub(crate)` (not `pub`, still crate-private): `pg_rules::metathesis` (a sibling module, not a
// submodule of `rewrite`) reuses this exact mutable-shape machinery rather than duplicating it —
// the "resolve to concrete node data before mutating" discipline this type already encodes is
// precisely what the metathesis synthesis reorder (`pg_rules::metathesis::synthesis_reorder`) needs,
// matching how the C# original structures this same operation.
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

    /// Build the FST segment sequence + node-index mapping under the matcher filter. Analysis
    /// filters to Segment|Anchor (boundaries excluded); synthesis adds Boundary as optional
    /// segments. Deleted nodes are always skipped (`!ann.IsDeleted()`).
    ///
    /// A `Segment`-kind node's own `optional` flag must also produce `Segment::optional`, not just
    /// boundaries — see the identical fix/rationale on `pg_rules::morph::segs_of`. Within this
    /// module it matters for a *later* phonological rule's own matching (e.g. Indonesian prule1
    /// re-scanning a shape after prule5/prule4 already marked a re-inserted candidate segment
    /// Optional) as well as for downstream morphological-rule analysis, which reuses these Optional
    /// segments via the frozen shape this rule's `analyze` returns.
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

/// The `(feature, symbol-bits)` pairs a `Context`/`CharDef` pattern node **pins**. A feature is
/// pinned iff the node constrains it to a proper subset of its symbols (an unconstrained lane is
/// `full_mask`); alpha-variable features are treated as unpinned (unconstrained — the flagged
/// variable-binding gap).
///
/// `pub` (F7, HYBRID_FST_RUST_PLAN.md §7.1): exposed so `hc_hybrid::env_nfa`/`hc_hybrid::compiler`
/// can build identity-arc/probe-representative lane rows for a pattern node without duplicating
/// this natural-class/char-def lane resolution — a small, additive, reviewed contract change (no
/// existing caller's behavior changes; the function body itself is unmodified).
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

/// Full `W`-lane vector for a pattern node, unconstrained lanes = `full_mask` (the driver's
/// feature-math representation, distinct from the FST-facing `UNCONSTRAINED = u64::MAX`).
///
/// `pub` (F7, HYBRID_FST_RUST_PLAN.md §7.1) — see `node_pins`'s doc for why.
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

/// Compile a lane sequence (pattern nodes in DOCUMENT order) to a target FST for traversal in
/// `dir`, C#-faithfully: `PatternNode.GenerateNfa` builds the NFA from `Children.GetNodes(
/// fsa.Direction)` (`SIL.Machine/Matching/PatternNode.cs:55`), i.e. the children are enumerated
/// REVERSED for a `RightToLeft` matcher — so a C# RtL matcher consumes the pattern's LAST child at
/// the physically-rightmost annotation and matches the SAME physical substring as LtR (direction
/// changes scan order/preference, not the matched string). `pg_fst`'s own frozen convention is the
/// opposite (`compile_with_direction` never reorders; its guard test
/// `rtl_asymmetric_language_walks_right_to_left` asserts an RtL-compiled `[a b c]` accepts physical
/// `c b a`), so the document→traversal reorder must happen HERE, at the pattern-compile boundary.
/// Every current caller is an analysis-side target compiled with `reverse(dir_of(rule))`; without
/// this reversal a multi-node analysis target (e.g. `boundary_rules`' 2-node epenthesis RHS `ta`)
/// silently matched the physically-reversed sequence — invisible on the reference grammars'
/// single-node targets, wrong on anything wider.
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

/// `compile_lane_fst`, but each row is wrapped in its own named `CompileNode::Group` ("g0".."g
/// {N-1}", DOCUMENT order, i.e. `lanes_seq`'s own index — stable regardless of the `RightToLeft`
/// physical reorder below) so a caller can recover, per accepted match, which single physical
/// segment each row *actually* consumed via `Fst::get_offsets(name, ..).0` (the group's START
/// tag) — needed by `ana_feature`. See that function's module-doc citation of
/// `FeatureAnalysisRewriteRuleSpec.cs:48,68-71`'s `new Group("target"+i)`, the real C# mechanism
/// this mirrors; `EnvFst::group_names`/`crate::morph::compile_parts` already use the identical
/// `pg_fst` primitive for the same "recover a sub-match's real position" need.
///
/// Empirically probed (P6 investigation): a group's START offset is always the row's true
/// single-segment position, even when `pg_fst::traverse::Transduce::advance`'s "skip the next
/// Optional annotation" branch (see `width_matches`'s doc) widens that SAME group's END tag to
/// swallow a transparently-skipped Optional segment immediately following it — only the END is
/// contaminated by the skip, never the START. Do not read a `get_offsets` END from this FST's
/// groups for anything semantic; only `.0` (the start) is trustworthy.
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

/// Shared width-mismatch guard (plan §6 item 1 / rust-optimizations-phase2.md W1.1): a
/// nondeterministic FST match against a `segs` sequence containing Optional segments (any
/// boundary — always `Segment::optional` in `MutShape::segs` — or an Optional-flagged
/// re-inserted segment) can report an `ENTIRE_MATCH` span `[s, e)` **wider** than the compiled
/// pattern it matched: `pg_fst::traverse::Transduce::advance`'s "skip the next Optional
/// annotation" branch reuses the *same* arc a second time at a position two physical segments
/// ahead (to let a pattern transparently pass over a boundary the way C#'s matcher does), and the
/// registers it writes on that path record the *skipped* segment's own extent, so the recovered
/// span silently absorbs it. Every call site that then indexes a per-pattern-position array
/// positionally by `target_nodes[k]` (`k` in `0..pattern_len`) must reject such an over-wide span
/// first: `ana_feature` used to (independently discovered, `29241a84`) but no longer does (see
/// below); this helper still guards every other `all_spans`-fed site — `ana_epenthesis` (silent
/// wrong-mutation risk) and `syn_feature`/`syn_narrow` (**panic risk**: both index/consume a
/// per-node array sized to the compiled pattern's own node count with no bounds check). A
/// too-narrow span cannot occur (each arc consumes at least one segment), so this is a plain
/// equality test, not `<`.
///
/// **RESIDUAL (P6):** this guard's own assumption — that discarding an over-wide span is safe
/// because a *tight*, exactly-`pattern_len`-wide alternative always survives alongside it — is
/// FALSE exactly when an earlier rule's own analysis-unapply has interposed an Optional segment
/// directly between every candidate pair of this pattern's real target positions (no tight
/// alternative can exist then); see `ana_feature`'s doc for the full derivation. `ana_feature`
/// no longer uses this guard at all (its target rows are recovered via
/// `compile_lane_fst_grouped`'s per-row `Group` capture instead, immune to the issue). The
/// OTHER three callers of this guard (`ana_narrow_general`, `syn_feature`, `syn_narrow`) are
/// still exposed to the identical latent failure mode in principle — none of the three reference
/// grammars (Indonesian/Amharic/Sena) exercises it: `ana_narrow_general`'s only live subrules
/// (Amharic's CV mergers) have single-node RHS targets (no multi-row adjacency to break), and no
/// reference grammar composes a flooding vacuous-deletion rule ahead of a `syn_feature`/
/// `syn_narrow` multi-node target the way this P6 fixture's `ana_feature` case did. Flagged, not
/// fixed — a real grammar exercising that combination would need the same per-row `Group`-capture
/// treatment applied to whichever of those three functions hits it.
///
/// **INVESTIGATED, LEFT AS-IS: a bounded `Quantifier` occupying the WHOLE LHS/RHS** (docs/
/// `phase_c_quantifier.rs`'s own "Why the environment, not the LHS/RHS focus" section; also
/// `phase_c_right_to_left.rs`'s epenthesis note references this same shape). A single
/// `PatternNode::Quantifier` node as the entire LHS or RHS has `Pattern::nodes.len() == 1`
/// regardless of its own `min`/`max`, so every caller here (`rhs_pins.len()`/`rule.lhs.nodes.len()`/
/// `target_len`/`expected_len` — all plain node counts) will reject any REAL match of that
/// Quantifier whose physical width differs from 1 (`max > 1`, or a `min == 0` skip), exactly per
/// this doc's opening paragraph. Probed directly (`pg-rules` synthesis, throwaway, not checked in):
/// `LHS = [Quantifier{min:1, max:2, children:[CharDef(a)]}]`, `RHS = [CharDef(t)]` against "aa"/
/// "aaa" does not crash and does not mis-group — but it also does not honor the quantifier's own
/// multiplicity at all: because every INDIVIDUAL occurrence of the quantifier's own child (a bare
/// single `a`) independently satisfies `min=1` and is a width-1 match, the Iterative scan finds and
/// applies each one separately (rewriting every `a` to `t` one at a time) before the wider,
/// width-2+ span is ever reachable (its start node is already `dirty` by the time the scan gets
/// there) — the quantifier's own grouping is silently invisible to this machinery, not merely its
/// non-unit-width occurrences.
///
/// This is **not treated as a Rust-side gap to close**, because C# has no defined behavior for this
/// shape either — it crashes. `SynthesisRewriteRuleSpec`'s constructor unconditionally does
/// `lhs.Children.Cast<Constraint<Word, ShapeNode>>()` over the RULE's own Lhs
/// (`PhonologicalRules/SynthesisRewriteRuleSpec.cs:33`), and every subrule-spec constructor that
/// consumes a subrule's Rhs does the identical unconditional cast: `FeatureAnalysisRewriteRuleSpec.
/// cs:104`, `NarrowAnalysisRewriteRuleSpec.cs:45`, `EpenthesisAnalysisRewriteRuleSpec.cs:18`
/// (analysis side, one per `Kind`), plus the Simultaneous-mode self-opaquing probe at
/// `AnalysisRewriteRule.cs:53-55` and the metathesis siblings `SynthesisMetathesisRuleSpec.cs:31`/
/// `AnalysisMetathesisRuleSpec.cs:53`. `Constraint<TData,TOffset>` and `Quantifier<TData,TOffset>`
/// are SIBLING subclasses of `PatternNode<TData,TOffset>` with no inheritance relation
/// (`SIL.Machine/Matching/Constraint.cs:12-13`, `Quantifier.cs:13-14`), so LINQ's `Cast<T>` throws
/// `InvalidCastException` the instant it reaches a `Quantifier` child. The DTD genuinely allows one
/// there — `PhoneticInput`/`PhoneticOutput`'s shared `PhoneticSequence` production
/// (`HermitCrabInput.dtd:515`) permits `OptionalSegmentSequence` exactly like `PhoneticTemplate`'s
/// environments do, and `XmlLanguageLoader`'s generic `LoadPatternNodes`/`LoadPhoneticSequence`
/// (`XmlLanguageLoader.cs:1405-1415,1493-1505`) builds a real `Quantifier<Word,ShapeNode>` for it
/// with no LHS/RHS-vs-environment distinction at load time — so this is a genuine DTD-vs-
/// implementation gap IN C# ITSELF (uncaught anywhere between XML loading and `Morpher`
/// construction, `RewriteRule.CompileSynthesisRule`/`CompileAnalysisRule`, both called with no
/// surrounding `try`/`catch` — `SynthesisRewriteRule.cs:31-36`), not an unglamorous corner nobody
/// exercises. `pg_grammar::load` is more permissive than C# here (it happily loads this shape
/// structurally into `PatternNode::Quantifier`, matching the loader's own permissiveness, but
/// nothing downstream crashes) — deliberately so, since crashing to match a C# bug would be a
/// strictly worse outcome than this module's existing behavior for no compensating fidelity gain.
///
/// Per this port's own governing rule (match C# and cite it; if C# is ambiguous or the shape is
/// unrepresentable, do not guess), `width_matches` is left EXACTLY as-is for this shape: there is no
/// C# behavior to converge on. Contrast the ENVIRONMENT case (`phase_c_quantifier.rs`'s own
/// containment fixture): an environment's Quantifier is matched via `EnvFst`/
/// `Transduce::first_match`, a pure existence test with no positional array to mismatch, and C#'s
/// environment matchers are ordinary `Matcher<Word,ShapeNode>` instances built directly from the
/// SAME `Pattern` with no Constraint-only cast anywhere — environments have real, well-defined C#
/// behavior for a Quantifier, and this port already provides it.
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

/// `all_spans`, reordered to match an Iterative pick-one-then-rescan loop's own scan preference
/// (`syn_feature`/`syn_narrow`/`probe_narrow`/`ana_feature` — every direction-BLIND site
/// this fix corrects). C# `IterativePhonologicalPatternRule.Apply`
/// (`PhonologicalRules/IterativePhonologicalPatternRule.cs:17-48`) finds the match nearest the
/// shape's `Matcher.Direction`-side edge first (`Matcher.Match(input)`, no explicit start ⇒ scan
/// from the anchor in `Direction`), applies it (or, if `MatchSubrule` declines it, just steps past
/// its start), then resumes scanning FURTHER in that SAME `Direction`
/// (`targetMatch.Range.GetEnd(Direction).GetNext(Direction)` /
/// `GetStart(Direction).GetNext(Direction)`, cs:29,33) — i.e. for `LeftToRight` it always tries the
/// leftmost not-yet-tried position next; for `RightToLeft` it always tries the rightmost
/// not-yet-tried position next.
///
/// `all_spans` itself stays a plain, direction-agnostic ascending sort — its other callers
/// (`sim_feature`/`sim_narrow`/`probe_sim_narrow`/`ana_narrow_general`, all confirmed Simultaneous-
/// mode / collect-then-apply-every-match consumers, see their own docs and
/// `AnalysisRewriteRule.cs:72-90`'s `mode = RewriteApplicationMode.Simultaneous` for the Narrow
/// cases) apply EVERY accepted candidate regardless of order, so they have no "which one wins"
/// question for this fix to touch. Only a pick-one Iterative loop needs its candidates tried in
/// scan order, so each such loop reorders its own copy via this helper instead of changing
/// `all_spans`'s contract for everyone.
///
/// `target.direction()` is always the right thing to key off, for EITHER caller family: synthesis
/// compiles `target` with `dir_of(rule)` (`lhs_fst`'s own call in `synthesize_with_mpr`), while
/// analysis compiles it with `reverse(dir_of(rule))` (`ana_feature_target_lanes`'s caller) —
/// mirroring `AnalysisRewriteRule`'s own constructor, which builds its `Matcher.Direction` as
/// `rule.Direction == LeftToRight ? RightToLeft : LeftToRight` (`PhonologicalRules/
/// AnalysisRewriteRule.cs:33`), i.e. analysis always scans the OPPOSITE way synthesis would have.
/// Reading `target.direction()` directly (rather than re-deriving `dir_of(rule)`/`reverse(..)` at
/// each call site) means both sides reduce to the same one-line rule — "scan in the direction THIS
/// specific compiled matcher actually traverses" — with no risk of the two getting out of sync.
fn ordered_spans(target: &Fst, segs: &[Segment]) -> Vec<(usize, usize)> {
    let mut spans = all_spans(target, segs);
    if target.direction() == Direction::RightToLeft {
        spans.reverse();
    }
    spans
}

/// A compiled environment (already lifted from a model `Pattern` with its anchors as flags).
///
/// `pub(crate)` (plan §13.1 Tier-1 #5): reused as-is by `crate::validity`'s allomorph-environment
/// gate (`RequiredEnvironments`/`ExcludedEnvironments`, C# `AllomorphEnvironment`), which needs
/// the exact same anchored-suffix/prefix matching this module's phonological-rule environments use
/// — the DTD's `<Environment>`/`<LeftEnvironment>`/`<RightEnvironment>` are one shared XML shape
/// consumed by both `<PhonologicalSubrule>` and `<MorphologicalSubrule>`/lexical `<Allomorph>`.
pub(crate) struct EnvFst {
    fst: Fst,
    anchor_start: bool,
    anchor_end: bool,
    /// The env is a bare word-boundary anchor (`#`) with no segment constraints.
    only_anchor: bool,
    /// Per-top-level-pattern-node alpha-variable occurrences, aligned with the authored pattern
    /// (`pattern_var_occurrences`; one entry per top-level node including quantifiers, which are
    /// always empty here — quantifier-nested variables are a separate, pre-existing, flagged
    /// limitation, see that function's doc).
    node_vars: Vec<Vec<VarOccur>>,
    /// The capture-group name for each var-bearing entry of `node_vars` (`None` where the node has
    /// no occurrences — those nodes were left unwrapped, see `compile_env_impl`).
    ///
    /// Tier-2 #12. **Verified against the actual C# mechanism** (not the Group-capture guess an
    /// earlier draft of this fix assumed): C# does not need any post-match position recovery at
    /// all, because its compiled FSA arcs carry the *live* `FeatureStruct` constraint — variables
    /// included — and `VariableBindings` is bound/checked **inline, arc by arc, during traversal**
    /// (`Input.Matches`/`FeatureValue.IsUnifiableImpl`, `FiniteState/Input.cs:49-61`,
    /// `FeatureModel/SimpleFeatureValue.cs:52-102`, esp. the `IsVariable` arms at 63-94 that this
    /// port's `bind_or_check` already mirrors, `SimpleFeatureValue.cs:62-77` in the earlier, narrower
    /// citation). Because the automaton structurally distinguishes "the quantifier's looping arc"
    /// from "the singular var-bearing arc", the correct annotation is bound the instant that
    /// specific arc fires — a variable-width quantifier elsewhere in the pattern cannot desynchronize
    /// it; there is no analog of this struct or this bug on the C# side.
    ///
    /// The Rust side has this problem only because `pg-fst`'s frozen FSA path cannot carry variables
    /// in arcs at all — `PatternBridge::simple_context_lanes` (`bridge.rs:213-227`) lowers every
    /// variable-governed lane to `UNCONSTRAINED` *before* compiling, making the compiled FST a sound
    /// over-approximation that this module re-checks against real node lanes *after* a candidate
    /// span is found (module doc above). That post-hoc re-check needs some way to recover, for the
    /// specific match found, which segment a given pattern node actually consumed — the pre-fix code
    /// used a positional guess (`s - node_vars.len() + k`) that is only correct when every node
    /// (including quantifiers) consumes exactly one segment. This field is the actual fix: reuse
    /// pg-fst's existing frozen `CompileNode::Group`/`Fst::get_offsets` capture primitive (already
    /// used identically by `pg_rules::morph::compile_parts`/`part_ranges` for affix-part captures) to
    /// read the matched span's real per-node offsets directly off the traversal's own registers,
    /// independent of how many segments any quantifier in the pattern actually consumed. Note C# does
    /// use exactly this class of position-recovery-by-Group elsewhere in this same file family — e.g.
    /// `FeatureAnalysisRewriteRuleSpec` wraps each *target* position in `new Group("target"+i)` and
    /// reads it back via `match.GroupCaptures["target"+i]` for its nonvacuous-unapplication check
    /// (`FeatureAnalysisRewriteRuleSpec.cs:48,68-71`) — so this is a faithful reuse of a real C#
    /// technique, just applied here to the specific gap (environment alpha-variable positions under a
    /// quantifier) that only exists because of this port's pre-compile variable erasure, not because
    /// C#'s *environment* binding uses Group too (it doesn't; see above).
    /// independent of how many segments any quantifier in the pattern actually consumed. No pg-fst
    /// change — this is an additive use of an existing primitive.
    group_names: Vec<Option<String>>,
}

/// Compile an environment for **synthesis** (and `crate::validity`'s allomorph-environment gate,
/// which mirrors `AllomorphEnvironment.cs`'s own `Segment|Boundary|Anchor` filter): boundary-marker
/// constraints are kept verbatim, matching `SynthesisRewriteRule.cs:26`'s matcher filter
/// (`Segment|Boundary|Anchor`) and `SynthesisRewriteSubruleSpec.cs`, which passes
/// `subrule.LeftEnvironment`/`RightEnvironment` straight through with no stripping. Analysis callers
/// must use `compile_env_analysis` instead — see its doc for why.
pub(crate) fn compile_env(g: &Grammar, table_id: TableId, env: Option<&Pattern>) -> Option<EnvFst> {
    compile_env_impl(g, table_id, env, false, false)
}

/// `compile_env` with the P10 `StrRep` identity lane enabled (see `PatternBridge::id_lane`) —
/// for **allomorph** environments only (`crate::validity` / `crate::cache::build_env_cache`),
/// whose match inputs come from `crate::morph::segs_of` and therefore carry the same lane. The
/// phonological-rule environment sites keep plain `compile_env`: their inputs are the rewrite
/// driver's own node lanes (no id lane), and an id-lane constraint against a lane-less input
/// would mis-fire on determinized negated arcs (see the `id_lane` field doc). This split matters
/// beyond precision: allomorph environments feed the W3.2 disjunctive re-check, where an
/// environment that OVER-matches (pre-P10: any `Segments`-class or literal-segment environment on
/// a zero-phonological-feature grammar matched anything) flips into wrongly REJECTING the word —
/// e.g. Sena `ndi-`'s passed-over `i+` allomorph (env `/ _ mb`) spuriously "matching" before
/// `phemb` killed every `ku+ndi+...` parse.
pub(crate) fn compile_env_allomorph(
    g: &Grammar,
    table_id: TableId,
    env: Option<&Pattern>,
) -> Option<EnvFst> {
    compile_env_impl(g, table_id, env, false, true)
}

/// Compile an environment for phonological **analysis** — C#
/// `AnalysisRewriteSubruleSpec.CreateEnvironmentPattern` (AnalysisRewriteSubruleSpec.cs:26-32), which
/// runs every left/right environment pattern through `HermitCrabExtensions.DeepCloneExceptBoundaries`
/// (HermitCrabExtensions.cs:143-198) before compiling it.
///
/// Why: the analysis matcher's own `Filter` is `Segment|Anchor` only (`AnalysisRewriteRule.cs:34`) —
/// `Type=Boundary` nodes are never presented to the FST traversal at all
/// (`TraversalMethodBase.cs:41-46` skips any annotation failing the filter when building its node
/// list), so a literal `BoundaryMarker` constraint left in an environment pattern could never match
/// anything and the environment would always spuriously fail. C# instead drops the boundary
/// constraint from the pattern entirely, which — combined with the *matcher's* filter transparently
/// skipping over physical boundary nodes when it lands on one mid-traversal (`Matcher.cs:335-337`,
/// `TraversalMethodBase`) — makes a morpheme boundary invisible/transparent during analysis-side
/// environment matching: e.g. Amharic `prule5` ("a deletion before a")'s `RightEnvironment =
/// [BoundaryMarker, Segment(a)]` degenerates on analysis to "the next real segment is *a*", silently
/// skipping over any boundary in between.
///
/// This port's `ana_*` functions already mirror the Segment|Anchor filter by building their match
/// universe via `ms.segs(false)` (boundaries excluded from the sequence). Without this stripping
/// step, a `compile_env`-compiled environment that still requires a literal Boundary segment could
/// never find one in that boundary-free sequence — `left_env_ok`/`right_env_ok` would always return
/// `false`, silently killing every analysis subrule whose environment references a morpheme
/// boundary. Confirmed real, current, grammar-agnostic impact: Indonesian's *meN-* prefix
/// nasal-assimilation analysis environments reference morpheme boundaries and were silently always
/// failing before this fix.
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

    // Tier-2 #12: wrap each var-bearing top-level node in a named capture group so its matched
    // segment can be recovered positionally-independent of any quantifier elsewhere in the pattern
    // (see `EnvFst::group_names`'s doc). Mirrors `pg_rules::morph::compile_parts`'s identical
    // `CompileNode::Group` wrapping for affix-part captures — same frozen pg-fst primitive, no
    // pg-fst change. Nodes with no alpha-variable occurrence are left unwrapped (no capture needed).
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

/// C# `HermitCrabExtensions.DeepCloneExceptBoundaries` (HermitCrabExtensions.cs:143-198): drop every
/// node that denotes a literal morpheme-boundary character (`Type() == HCFeatureSystem.Boundary`),
/// recursing into `Quantifier` groups and dropping a quantifier entirely once its filtered children
/// are empty (mirroring C#'s `if (newQuantifier.Children.Count > 0) yield return ...`). Only
/// `PatternNode::CharDef` can denote a boundary in this port's flattened model (a `Context` natural
/// class always injects `Type=Segment`, C# `NaturalClass.cs`; `Anchor` is the `#` word-edge marker,
/// a distinct `Type` from `Boundary`, and is never stripped by C# either).
///
/// KNOWN RESIDUAL: `PatternNode::Segments` (a pre-segmented `<Segments>` shape) could in principle
/// embed a boundary character too, but no `<PhonologicalRule>` environment in any of the three
/// reference grammars uses `<Segments>` (confirmed by grep; Sena's `<Segments>`-bearing environments
/// are all `<MorphologicalSubrule>`/lexical-allomorph environments — a different C# code path,
/// `AllomorphEnvironment.cs`, whose own filter is `Segment|Boundary|Anchor` and is *not* stripped, so
/// it correctly keeps calling plain `compile_env`, not this function). Left unhandled here per this
/// module's existing scope-note convention — flagged, not silently wrong on any exercised path.
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

/// Left environment holds iff some suffix of the left context (`segs[0..left_end]`), ending
/// adjacent to the target, matches the env (word-start-anchored if the env began with `#`). A bare
/// `#` left env holds iff the target is at the word start (`left_end == 0`).
///
/// Returns the match itself (`Some(Option<FstResult>)`) rather than a bare bool so a caller that
/// also needs alpha-variable bindings (`resolve_bindings`) can reuse the *same* traversal's
/// registers for group-capture lookups instead of re-running the FST: outer `None` = the
/// environment failed to match (reject the candidate); `Some(None)` = no environment was authored
/// (vacuous pass, nothing to bind); `Some(Some(result))` = matched, `result.registers` holds the
/// capture groups `resolve_bindings` reads via `EnvFst`'s `group_names`.
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

/// Right environment holds iff a prefix of the right context (`segs[right_start..]`), starting
/// adjacent to the target, matches the env (word-end-anchored if the env ended with `#`). A bare
/// `#` right env holds iff the target is at the word end (`right_start == segs.len()`). See
/// `left_env_match`'s doc for the `Option<Option<FstResult>>` shape.
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
// Alpha-variable agreement (C# `SimpleFeatureValue.cs` variable arms + `VariableBindings`).
// =================================================================================================
//
// The frozen FST cannot bind variables, so its arc constraints over-approximate (variable lanes
// lowered to UNCONSTRAINED). After it reports a candidate span, we run the *actual* agreement check
// against node lanes, exactly mirroring `SimpleFeatureValue.IsUnifiableImpl`'s `IsVariable &&
// !otherSfv.IsVariable` arm (SimpleFeatureValue.cs:62-77):
//   - first occurrence BINDS `varBindings[name] = otherSfv.GetVariableValue(Agree)` — the node's
//     symbol set if agree, its negation (within the feature mask) if disagree
//     (SimpleFeatureValue.cs:391-393);
//   - a subsequent occurrence checks `binding.Overlaps(!Agree, nodeValue)` — the (polarity-adjusted)
//     binding must share a symbol with the node (SimpleFeatureValue.cs:66-69). No overlap ⇒ reject.
// Binding order matches C# `RewriteRuleSpec.MatchSubrule`: the target match binds first, then the
// left environment, then the right environment (RewriteRuleSpec.cs:82-101).

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

/// C# `SynthesisRewriteSubruleSpec.IsApplicable` (SynthesisRewriteSubruleSpec.cs:31-70): required
/// syntactic FS (POS) + required/excluded MPR feature gating, checked dynamically against the
/// *current word being synthesized* — not a static, compile-time property of the subrule. Both
/// halves are now threaded: `syn_fs` (the word's current `SyntacticFeatureStruct`/`Word.syn_fs`) for
/// the POS half (see `required_pos_ok`), and `mpr` (the word's current `MprFeatures`/`Word.mpr`)
/// for the MPR half, gated exactly as C#: `MprFeatureSet.IsMatchRequired`/`IsMatchExcluded`, i.e.
/// `pg_grammar::model::Grammar::mpr_group_ok` (W3.1: MPR-group-aware; this was a flat overlap
/// check before that fix, correct only because every reference grammar's groups are singletons).
///
/// Before the POS half was ported, every
/// subrule declaring a nonempty `requiredPartsOfSpeech` was unconditionally treated as inapplicable
/// during synthesis, regardless of the actual word's POS — Amharic authors this 3× (`amharic-hc.xml
/// :12151,12169,12188`), so those subrules silently never fired. Real Amharic corpus impact measured
/// after the fix (see `pg-parse/tests/csharp_port_rewrite.rs::boundary_rules_required_pos_on_subrule
/// _finding`'s doc for the synthetic-fixture confirmation).
///
/// Before the MPR half was ported (an earlier fix, unrelated to this one): every subrule declaring a
/// nonempty `required_mpr`/`excluded_mpr` was unconditionally treated as inapplicable during
/// synthesis, regardless of the actual word — e.g. Indonesian `prule5` ("Voiceless obstruent
/// deletion", `excludedMPRFeatures="mpr1"`) never fired for *any* word, so a re-synthesized
/// `meN-`-prefixed word (analysis round-trip via `SynthesisStratumRule`'s trailing prule application)
/// never deleted the assimilated-nasal's following obstruent, and the resynthesized surface could
/// never equal the input surface — `is_match` always failed, producing a complete non-parse
/// regardless of how correct the rest of the analysis was.
///
/// C#'s analysis-side counterpart, `AnalysisRewriteSubruleSpec`, does **not** override
/// `RewriteSubruleSpec.IsApplicable` (whose base implementation is `return true`
/// unconditionally, RewriteSubruleSpec.cs:46-49) — so unapplication is never MPR/POS-gated. This
/// port's `analyze()` correctly never calls `subrule_applicable` at all; this gate is
/// synthesis-only, matching that asymmetry.
///
/// `pub` (not `pub(crate)`): `pg_foma`'s P6 MPR/POS flag-diacritics prototype
/// (`pg-foma/src/gate.rs`) calls this DIRECTLY, at grammar-compile time, once per (lexical entry,
/// gated subrule) pair, to partition entries into groups that agree on every gated subrule's
/// applicability — the compiled foma network is then built per-group with each group's
/// inapplicable subrules dropped, rather than re-deriving this predicate's semantics (MPR groups'
/// All/Any match-type, the POS "unset = vacuous pass" rule) a second time in a different crate.
/// Reusing the engine's own function is what makes the two paths provably agree.
pub fn subrule_applicable(
    g: &Grammar,
    sr: &RewriteSubruleDef,
    syn_fs: &FeatureStruct,
    mpr: MprSet,
) -> bool {
    required_pos_ok(g, &sr.required_pos, syn_fs)
        && g.mpr_group_ok(sr.required_mpr, sr.excluded_mpr, mpr)
}

/// The POS half of `subrule_applicable`: C# `_subrule.RequiredSyntacticFeatureStruct.IsUnifiable(
/// input.SyntacticFeatureStruct)` (`SynthesisRewriteSubruleSpec.cs:33`). `required_pos` is
/// `pg_grammar::model::RewriteSubruleDef::required_pos` — already loaded from
/// `requiredPartsOfSpeech` as a POS symbol bitset (`pg-grammar/src/load.rs::parse_pos_bits`), the
/// port's flattened analog of C#'s `FeatureStruct{POS: symbolset}`.
///
/// `None` (no `requiredPartsOfSpeech` attribute — C#'s default empty `RequiredSyntacticFeatureStruct`)
/// is vacuously satisfied: an empty `FeatureStruct` has no entries, and `is_unifiable`
/// (`pg-featstruct/src/ops.rs:106`) treats a feature present on only one side as never blocking. When
/// `required_pos` IS present, that reduces to a single symbolic-feature comparison: `syn_fs` missing
/// a POS entry entirely (also unconstrained — same "feature present on only one side" rule) always
/// satisfies it; a present POS entry must share at least one symbol with `required_pos`
/// (`SymbolBits::overlaps`'s un-negated arm — plain non-empty-intersection, matching
/// `SimpleFeatureValue.IsUnifiableImpl`'s non-variable arm exactly). The mask parameter is unused on
/// that arm (`pg-featstruct/src/ops.rs`'s own `NO_MASK` constant documents this same shortcut for the
/// tree-level `is_unifiable`), so `0` is passed rather than computing `g.syn_features.mask(..)`.
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

/// Apply `rule` forward to `input` (C# `SynthesisRewriteRule.Apply`). Returns the rewritten shape
/// in a one-element vec if the rule applied, else an empty vec (mirroring `Apply`'s
/// `input.ToEnumerable()` / `Empty`).
///
/// Thin wrapper over `synthesize_with_mpr` with an empty MPR set AND an empty syntactic FS —
/// preserves this function's existing signature (and every existing caller/test) for grammars/rules
/// with no MPR/POS gating (an empty `FeatureStruct` has no POS entry, so `required_pos_ok`'s
/// "feature present on only one side" rule vacuously satisfies any `requiredPartsOfSpeech`, matching
/// this wrapper's pre-existing MPR behavior exactly); real pipeline callers that need `Word.mpr`/
/// `Word.syn_fs` gating (`pg_rules::stratum::synthesize_stratum`'s trailing prule application) call
/// `synthesize_with_mpr` directly.
pub fn synthesize(g: &Grammar, rule: &RewriteRuleDef, input: &Shape) -> Vec<Shape> {
    synthesize_with_mpr(g, rule, input, &FeatureStruct::EMPTY, MprSet::EMPTY)
}

/// Identical to `synthesize`, but gates each subrule's `requiredPartsOfSpeech`/`required_mpr`/
/// `excluded_mpr` (see `subrule_applicable`) against the synthesizing word's actual syntactic FS
/// and MPR feature set instead of assuming empty.
///
/// Recompiles every subrule's target/environment matchers on every call — kept as-is (not folded
/// into the cache) because this function (like `synthesize`/`analyze` below) is also called directly
/// on standalone, non-grammar-resident `RewriteRuleDef` fixtures throughout the test suite, which
/// have no stable index into a `crate::cache::RuleCache`. The real per-word pipeline
/// (`crate::stratum`) calls `synthesize_with_mpr_cached` instead, which skips all of this
/// recompilation. See `crate::cache`'s module doc for the full compile-once rationale.
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
        // P13: `rule.mode` selects the function pair for Feature/Narrow; Epenthesis reuses
        // `syn_epenthesis` for both modes -- its existing one-snapshot-collect-then-apply
        // shape already matches `SimultaneousPhonologicalPatternRule`'s semantics, and is also the
        // (pre-existing, unrelated-to-P13) best-available stand-in for Iterative epenthesis today
        // -- see `syn_epenthesis`'s own doc for the documented residual.
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

/// The `crate::cache::RuleCache`-aware sibling of `synthesize_with_mpr`, used by the real
/// per-word pipeline (`crate::stratum::synthesize_stratum`'s trailing prule application): every
/// target/environment matcher is read from `cache.prule_rewrite(pid)` instead of being recompiled. `pid`
/// must identify `rule` (i.e. `rule as *const _ == &g.prules[pid.0 as usize] as *const _`) — every
/// production call site already has both in hand (it indexed `g.prules` by `pid` to get `rule`).
pub(crate) fn synthesize_with_mpr_cached(
    g: &Grammar,
    pid: pg_grammar::model::PRuleId,
    rule: &RewriteRuleDef,
    input: &Shape,
    syn_fs: &FeatureStruct,
    mpr: MprSet,
    cache: &crate::cache::RuleCache,
) -> Vec<Shape> {
    // `pid` (a grammar-resident cache key -- every call site derived it from `g.prules`) resolves
    // this rule's OWN owning stratum's table (`crate::cache::owning_table_for_prule`), never an
    // implicit table-zero default -- see that function's doc for the fallback contract (an
    // orphaned-but-grammar-resident prule, provably unreachable via this cached path in practice).
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
        // Same `(Kind, rule.mode)` dispatch as `synthesize_with_mpr` (§4.2) — `pc.syn_target`/
        // `sc.syn_left`/`sc.syn_right` are identical for either mode of a given `Kind` (§4.2: "no
        // cache schema change expected", confirmed — `sim_feature`/`sim_narrow` read the exact same
        // compiled target/env FSTs `syn_feature`/`syn_narrow` do; only the driving loop differs).
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

/// The shared C#-`SynthesisRewriteRule.Apply` readout tail (cs:65-85): given every subrule's
/// `SubruleOutcome` (in subrule-index order), fire the exact trace events C# would, in the exact
/// order and with the exact early-stop C# uses. `out_word` is the single snapshot passed to EVERY
/// call — matching C#'s own behavior of reusing ONE mutated `Word` reference for every readout call
/// (the readout runs after `_patternRule.Apply` has already fully finished mutating `input` in
/// place, so even a FAILED subrule's reported "Input" reflects the rule's final post-mutation state,
/// not a snapshot from when that subrule was tried — a real, verified-from-source C# quirk, not an
/// approximation). `phonological_rule_applied`/`phonological_rule_not_applied` (`trace.rs`) do not
/// reassign the trace cursor (verified against `TraceManager.cs:174-202`: neither method touches
/// `.CurrentTrace`, unlike `MorphologicalRuleApplied`) — the returned handle is discarded here,
/// matching every call site's existing discipline of only reassigning the cursor where C# does.
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

/// `synthesize_with_mpr`'s traced sibling — standalone (recompiles every call, like
/// `synthesize_with_mpr` itself), for hand-built fixtures with no grammar-resident
/// `crate::cache::RuleCache` index. `pid` is only used as the trace tree's rule identity (a
/// fixture with no real grammar-resident prule table may pass any nominal value); no `&Word` input
/// is required — a snapshot carrying `syn_fs`/`mpr` is built internally via `Word::new`, since
/// this function (unlike its `_cached` sibling below) has no live `Word` in hand to draw a `.trace`
/// cursor from — the caller-supplied `parent` is used as-is.
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
    // See `synthesize_with_mpr_cached`'s doc for the owning-table resolution rationale.
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

/// The `crate::cache::RuleCache`-aware sibling of `synthesize_with_mpr_traced` — the real
/// per-word pipeline's traced entry point (`crate::stratum::synthesize_stratum_traced`'s trailing
/// prule application). Takes the real `&Word` (not bare `Shape`/`syn_fs`/`mpr`) so the trace snapshot
/// carries the word's actual full state, and so `node_parent` can fall back to `input.trace` exactly
/// like every other traced call site in `stratum.rs` (`word.trace.unwrap_or(parent)`).
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
    // See `synthesize_with_mpr_cached`'s doc for the owning-table resolution rationale.
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
        // P13 §4.4: `sr.self_opaquing` (Feature/Epenthesis only -- always `false` for Narrow, which
        // needs no wrapper, §2.2) gates a repeat-until-fixpoint loop around the single-pass
        // `ana_feature`/`ana_epenthesis` call, mirroring C#'s `while (data != null) { applied =
        // true; data = sr.Item2.Apply(data).SingleOrDefault(); }` exactly: repeat calling the SAME
        // unchanged function until a call makes no further change. Both functions already return
        // `bool` ("did anything change"), so the `while` condition falls out directly.
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

/// The `crate::cache::RuleCache`-aware sibling of `analyze`, used by the real per-word pipeline
/// (`crate::stratum::StratumAnalyzer::analyze`'s prule sweep). See `synthesize_with_mpr_cached`'s
/// doc for the `pid`/`rule` correspondence contract.
pub(crate) fn analyze_cached(
    g: &Grammar,
    pid: pg_grammar::model::PRuleId,
    rule: &RewriteRuleDef,
    input: &Shape,
    cache: &crate::cache::RuleCache,
) -> Vec<Shape> {
    // See `synthesize_with_mpr_cached`'s doc for the owning-table resolution rationale.
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
// P12 chunk 6 — phonological rule tracing (analysis side).
//
// C# `AnalysisRewriteRule.Apply` (`PhonologicalRules/AnalysisRewriteRule.cs:128-193`) traces INLINE,
// per subrule, unlike the synthesis side's post-hoc side-channel readout: each loop iteration snapshots
// `origInput` BEFORE that subrule's own attempt, tries it (possibly repeatedly, for `Deletion`/
// `SelfOpaquing` reapply types — out of scope here, this port's `self_opaquing` while-loop already
// covers the same ground per subrule, §4.4), then immediately fires `PhonologicalRuleUnapplied(_rule,
// i, origInput, input)` on success or `PhonologicalRuleNotUnapplied(_rule, i, input)` on failure
// (`AnalysisRewriteRule.cs:178-187`) — no `FailureReason` at all either way (`ITraceManager.cs:42-43`
// takes none), matching this port's existing doc note that `AnalysisRewriteSubruleSpec` never
// overrides `IsApplicable` (no MPR/POS gate on analysis), so there is no reason to decompose. Per
// `TraceSink::phonological_rule_unapplied`/`phonological_rule_not_unapplied`'s own simplified
// signatures (chunk 0 — neither carries a separate `input`-before/`output`-after pair, only one
// `&Word`), a single post-subrule snapshot suffices for both branches, exactly mirroring how the
// synthesis-side functions above needed no separate origInput either.
// =================================================================================================

/// `analyze`'s traced sibling — standalone (recompiles every call). `pid` is a nominal trace-tree
/// identity for fixtures with no grammar-resident prule table (mirrors
/// `synthesize_with_mpr_traced`'s same convention). No `&Word` input is required (analysis has no
/// syn_fs/MPR gate to carry, see this section's doc); the caller-supplied `parent` is used as-is.
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
    // See `synthesize_with_mpr_cached`'s doc for the owning-table resolution rationale.
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

/// The `crate::cache::RuleCache`-aware sibling of `analyze_traced` — not yet wired into the live
/// per-word pipeline (`crate::stratum::StratumAnalyzer::analyze` is itself untraced today, a
/// pre-existing, separately-documented P12 gap — "Analysis-side stratum bookends stay untraced",
/// `rust-optimizations-phase2.md`'s P12 chunk-5 note). Built now so the mechanism exists and is
/// tested; a future pass that traces `StratumAnalyzer::analyze` calls this instead of
/// `analyze_cached`, threading a real `trace`/`parent` the same way
/// `synthesize_stratum_traced` already does for the synthesis side.
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
    // See `synthesize_with_mpr_cached`'s doc for the owning-table resolution rationale.
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
    // Finding N2: the LHS's own full per-position lane rows, needed only by `pattern_defaults_ok`'s
    // UseDefaults confirm step (§ below) — `rhs_pins` already existed for `ApplyRhs`; this is the
    // LHS-side analog (full rows, not sparse pins, since `pattern_defaults_ok` needs to tell
    // "pinned to X" apart from "unpinned" using `full_mask` comparison, same as `node_full_lanes`).
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
        // First span (in `target.direction()`'s own scan order — leftmost-first for LtR, rightmost-
        // first for RtL, see `ordered_spans`'s doc) whose target nodes are all clean
        // (Modified=Clean) and where the environments hold. (Feeding order beyond that is out of
        // scope; the gate rules don't feed.)
        let mut acted = false;
        for (s, e) in ordered_spans(target, &segs) {
            let target_nodes: Vec<usize> = node_of[s..e].to_vec();
            // Width guard (plan §6 item 1): reject an over-wide Optional-skip artifact before the
            // positional `rhs_pins[k]` index below, which would otherwise panic on a multi-node
            // target abutting a boundary — see `width_matches`'s doc.
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
    // THEN apply every accepted candidate (C# second `foreach` loop). Note this still mutates `ms`
    // progressively as it goes, exactly like C#'s shared-`Word` `ApplyRhs` loop -- only the MATCHING
    // phase above is simultaneous/snapshot-based; the applying phase is sequential (this matters
    // only for the unexercised overlapping-target-span case, §4.1's warning).
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

/// The `ana_feature` target FST's per-node lanes (`LHS ⊕ RHS` priority-union, FST-facing),
/// factored out so [`RuleCache`](crate::cache::RuleCache) construction can compile this target
/// exactly once instead of on every `ana_feature` call — see that function's doc for the full
/// rationale (alpha-variable handling, the prule4 archiphoneme example).
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

/// C# `FeatureAnalysisRewriteRuleSpec`: the analysis matcher's target is `LHS ⊕ RHS`
/// (priority-union), and `Unapply` makes each feature the RHS *changed* underspecified (the
/// `rhs.AntiFS − lhs.AntiFS` then `Union` at the lane level = set the changed feature to its full
/// symbol mask). Direction is reversed (RtoL), nondeterministic.
///
/// `target` is compiled by `compile_lane_fst_grouped` (NOT the plain `compile_lane_fst`): each
/// target-pattern row is wrapped in its own named `Group` (`names[k]`), read back per accepted
/// match via that group's START offset — see `compile_lane_fst_grouped`'s doc for the empirically
/// confirmed reason this is needed (root cause of the P6 "deletion composition" finding, `rust/
/// crates/pg-parse/tests/csharp_port_rewrite.rs::multiple_segment_rules_deletion_composition_
/// finding`): a positional `node_of[s..e]` slice (this function's pre-fix approach, still used by
/// `syn_feature`/`syn_narrow`/`ana_narrow_general`) silently assumes the pattern's own N target
/// rows land on N *physically contiguous* real segments. That assumption breaks when an earlier
/// rule's own analysis-unapply (e.g. `ana_narrow_deletion`'s vacuous multi-site OPTIONAL
/// reinsertion) has interposed an Optional non-matching segment directly between two of THIS rule's
/// real target positions: `pg_fst::traverse::Transduce::advance`'s "skip the next Optional
/// annotation" branch (see `width_matches`'s doc) then reports every candidate match as an
/// over-wide `[s, e)` span (it must consume the interposed Optional to reach the second real
/// match), and since NO tight (exactly-`changed.len()`-wide) alternative exists in that case
/// either, `width_matches` discards every candidate — the rule silently unapplies nothing. Group
/// capture sidesteps the whole `[s, e)`-contiguity assumption by reading each row's own real
/// position directly, independent of whatever got transparently skipped between rows.
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

    // Finding N2: the analysis target's own full per-position lane rows (`LHS ⊕ RHS`), for
    // `pattern_defaults_ok`'s UseDefaults confirm step below. `analyze`/`analyze_cached` already
    // compute this once to build the `target: &Fst` this function receives; recomputing it here is
    // the same "recompile on every call" tradeoff this whole function already makes (module doc on
    // `analyze`) rather than threading a new parameter through both call sites and the cache.
    let target_lanes = ana_feature_target_lanes(g, table, rule, sr);

    // The features each RHS node changed relative to the LHS, paired with the bits to OR onto the
    // node's current (matched) value on unapply — Tier-2 #11 (plan §6 item 4 / rust-conversion.md
    // Tier-2 #11): C#'s `FeatureAnalysisRewriteRuleSpec` does NOT reset a changed feature to the
    // full symbol mask; it computes a real `AntiFeatureStruct` negation
    // (`rhsConstraint.FeatureStruct.AntiFeatureStruct()`, then `.Subtract(lhsConstraint.
    // FeatureStruct.AntiFeatureStruct())`, `FeatureAnalysisRewriteRuleSpec.cs:50-51`), unions the
    // result onto the matched node's own current value via `PriorityUnion` then a final
    // struct-level `Union` (`cs:110-112`). Reducing that object-graph chain to bitset algebra (each
    // step here re-derived and cross-checked against `SimpleFeatureValue.SubtractImpl`'s `ExceptWith`
    // and `FeatureStruct.UnionImpl`'s per-leaf `UnionWith`): for a literal (non-alpha) RHS pin with
    // bits `R` against an LHS pin with bits `L`, `AntiFeatureStruct(R).Subtract(AntiFeatureStruct(L))`
    // algebraically simplifies to `L & !R` (both terms already confined within the feature's own
    // mask), and `Union`-ing that onto the matched node's current value `C` (always `C == R` for a
    // literal pin, since the node concretely matched the RHS-pinned target) gives `C | (L & !R) = L
    // | R`. This is a **strict subset** of `full_mask` whenever the feature has a 3rd (or more)
    // symbol neither side mentions — `full_mask` (the pre-fix value) wrongly also accepts that
    // untouched symbol. On a 2-symbol feature (every phonological feature in all 3 reference
    // grammars happens to be 2-valued or the LHS/RHS jointly exhaust >2 values) `L ∪ R` always
    // *equals* `full_mask`, which is exactly why this bug survived those corpora undetected — see
    // this fix's fixture (`rewrite_gate.rs`) for a 3-symbol feature that distinguishes the two.
    //
    // Alpha-governed RHS features (e.g. prule4 "Nasal assimilation"'s output `nc11` + an alpha
    // variable over place) keep the pre-existing full-mask fallback: the bound value isn't known
    // until match time (see `resolve_bindings`), so no per-feature `L & !R` can be precomputed here;
    // `node_pins` deliberately excludes alpha-governed features from its own pin computation (see
    // its doc), which is correct for *matching* but means the literal-only computation would
    // otherwise miss this feature as "changed" entirely — for prule4 the only literal RHS pin
    // already equals the LHS's own value, so without explicitly adding the alpha feature here the
    // nonvacuous check below would be vacuously always false and prule4 analysis would never fire.
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

    // Group-capture recovery (see this function's doc / `compile_lane_fst_grouped`'s doc): each
    // accepted match's per-row real segment position is read from its own named Group's tag,
    // never from a positional `node_of[s..e]` slice -- the slice approach silently assumes the
    // pattern's N target rows land on N *physically contiguous* real segments, which an earlier
    // rule's vacuous deletion-unapply can break by interposing an Optional segment directly
    // between two of THIS rule's own target positions (root cause of the P6 "deletion
    // composition" finding).
    //
    // WHICH tag half (START vs END) is trustworthy is direction-dependent (empirically probed,
    // `pg-rules/src/rewrite.rs`'s `group_probe_diag` module): `pg_fst::traverse::Transduce::
    // advance`'s "skip the next Optional annotation" branch (see `width_matches`'s doc)
    // re-executes a row's own tag commands at a widened offset when a skip is spawned right
    // after that row, corrupting exactly one raw tag half per skip -- but never the OTHER half,
    // and never a row's *entering* tag. For `LeftToRight` (traversal visits rows in document
    // order 0,1,2,..) that means a row's START is always the fresh "just entered this row" value
    // and is never corrupted; only a row's END can be widened by a *following* skip. For
    // `RightToLeft` the compiled node order is document-reversed (`compile_lane_fst_grouped`) so
    // traversal visits rows in REVERSE document order, AND `Fst::get_offsets` swaps
    // (start,end)->(end,start) for this direction (`fst.rs`'s doc) -- both facts together mean
    // the reported END is the one that's always fresh/reliable, never the reported START.
    //
    // NOTE: the `!rtl` (LeftToRight) branch below is DEAD for every reference grammar's own
    // `ana_feature` call -- `analyze`/`analyze_cached` always compile this target with
    // `reverse(dir_of(rule))`, and every `<PhonologicalRule>` in all three reference grammars
    // omits `direction="rtl"`, i.e. `dir_of(rule)` is always `Dir::LeftToRight`, so `reverse(..)`
    // is always `RightToLeft` in practice. The LTR branch is only exercised by
    // `group_probe_diag::group_offsets_survive_interposed_optional_ltr` (a synthetic `pg_fst`-level
    // unit test of `compile_lane_fst_grouped` itself, not an end-to-end `ana_feature` case) --
    // kept correct and tested in case a future grammar (or a currently-unexercised RtL-declared
    // rule) needs it, but do not assume it has full-pipeline coverage.
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
    // Plan §6 item 3 (W1.3): give the RHS the same alpha-variable resolution `syn_feature`'s
    // `rhs_vars` step already has (`PriorityUnion(fs, varBindings)`/`ReplaceVariables` — same C#
    // mechanism, narrowing has no separate code path for it). Before this fix `syn_narrow` never
    // computed bindings at all, so a narrowing RHS natural class carrying an alpha variable bound
    // from a merged LHS segment (e.g. Amharic prule6/7's 20-var CV merger) left that lane at its
    // unresolved default (full-unconstrained) instead of the captured value.
    let lhs_vars = pattern_var_occurrences(&rule.lhs);
    let rhs_vars = pattern_var_occurrences(&sr.rhs);

    let mut applied = false;
    loop {
        let (segs, node_of) = ms.segs(true);
        let mut acted = false;
        // Direction-ordered scan (see `ordered_spans`'s doc): leftmost-first for LtR, rightmost-
        // first for RtL — matching C# `IterativePhonologicalPatternRule.Apply`'s own scan order.
        for (s, e) in ordered_spans(target, &segs) {
            let target_nodes: Vec<usize> = node_of[s..e].to_vec();
            // Width guard (plan §6 item 1): an over-wide Optional-skip span here would delete more
            // physical nodes than the LHS pattern actually matched (a silent wrong mutation, not a
            // panic — this site has no positional per-node array to overrun) — see
            // `width_matches`'s doc.
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

/// C# `NarrowAnalysisRewriteRuleSpec`: dispatches on whether the subrule's RHS is empty
/// (`IsTargetEmpty`), exactly mirroring the constructor's own `if (subrule.Rhs.IsEmpty) ... else
/// ...` branch (`NarrowAnalysisRewriteRuleSpec.cs:24-35`) — see `ana_narrow_deletion` /
/// `ana_narrow_general`'s docs for the two cases. Dispatch lives at each call site (`analyze`/
/// `analyze_cached`) rather than in a shared wrapper, matching this module's existing
/// per-call-site-compiles-its-own-target convention (the general case needs a target `Fst` the
/// deletion case does not).
///
/// C# `NarrowAnalysisRewriteRuleSpec` (Simultaneous, reapply=Deletion): the analysis matcher for a
/// deletion rule matches a single Segment|Anchor node (the site), and `Unapply` re-inserts the LHS
/// segment(s) as **optional** after that node and marks the target nodes optional. We collect all
/// matching sites against the un-mutated shape, then apply (descending) — mirroring
/// AllMatches-then-apply.
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
                                            // C# `RewriteRuleSpec.MatchSubrule`'s `_isTargetEmpty` branch also matches the word-initial
                                            // gap: the substitute `Segment|Anchor` pattern matches the shape's left-anchor node itself
                                            // (anchors always bracket a shape, so `leftNode = rangeStart` / `rightNode = rangeEnd.Next`
                                            // are never null here), and `Unapply` inserts `AddAfter(range.Start)` = right after that
                                            // anchor. `ms.nodes[0]` is always the left anchor per `MutShape::from_shape`/`to_shape`'s own
                                            // invariant. Without this site, a word-initial deletion (e.g. an elided root-initial segment)
                                            // can never be re-inserted by analysis. See `RewriteRuleSpec.cs:55-77` (`isTargetEmpty`
                                            // branch) and `NarrowAnalysisRewriteRuleSpec.cs:24-31`.
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

/// General narrowing (`LHS.count > RHS.count > 0`) / expansion (`0 < LHS.count < RHS.count`) — the
/// `NarrowAnalysisRewriteRuleSpec.cs`'s non-empty-RHS branch (`AnalysisRewriteRule.cs`'s own
/// comment: "`NarrowAnalysisRewriteRuleSpec` works for expansion, too"). Unlike
/// `ana_narrow_deletion`, the analysis matcher's target is the RHS's *own* constraints
/// (`subrule.Rhs.Children`, cloned) — **not** a LHS-vs-RHS priority union like the feature case —
/// matched in the reversed direction, nondeterministically (shape nodes may be underspecified
/// during analysis; the RHS lane formula is textually identical to `ana_epenthesis_target_lanes`'s
/// "the RHS segment sequence, FST-facing", reused by both this function's uncached caller and
/// `build_prule_cache`). On a match, C#'s `Unapply`:
///  1. clones the ORIGINAL (un-narrowed) LHS pattern's constraints, resolves any alpha-variable
///     bindings from the match onto them (`fs.ReplaceVariables(varBindings)`), and splices them in
///     right after the match (`curNode = range.End`, then chained `AddAfter(curNode, fs, true)` —
///     each new node is OPTIONAL);
///  2. marks the RHS-matched nodes themselves optional too (**not** deleted, unlike
///     `ana_narrow_deletion`).
///
/// All matches are found against the pristine (pre-subrule) shape, then applied once, in descending
/// node-index order — mirroring `ana_narrow_deletion`'s existing AllMatches-then-apply technique
/// (C#'s linked-list `ShapeNode` insertions don't invalidate other captured node references, but
/// this port's `Vec<MutNode>`-index representation does, so descending application is the
/// index-safe equivalent). All five Amharic narrowing/expansion subrules have a single-node RHS
/// (`_targetCount == 1`: `prule1`–`prule3` merge concrete segments, `prule6`/`prule7` merge a
/// natural class), so match spans are inherently non-overlapping; a multi-node RHS with
/// overlapping candidate spans (needing real non-overlap interval selection) is a documented
/// residual not exercised by any of the three reference grammars.
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
        // Width guard (plan §6 item 1): an Optional-aware nondeterministic FST match can report a
        // span WIDER than the RHS pattern — see `width_matches`'s doc. `all_spans` separately
        // reports the correctly-sized ("tight") match for the same target, so dropping the
        // over-wide span here only discards a duplicate. Without this guard, on Amharic's
        // Optional-flooded analysis shapes the single-node CV-merger targets (`prule6`/`prule7` →
        // `nc17`; `prule1`/`prule2` → a concrete segment) spuriously match multi-segment windows,
        // marking whole windows Optional and re-reconstructing at every one — one prule sweep
        // compounds a 2-segment word to 40+ Optional nodes and the downstream affix matcher then
        // enumerates a combinatorial number of Optional-skip submatches (a self-inflicted flood C#
        // never has: its RHS target pattern binds each position with a named `Group` capture, so
        // its match range is always exactly the target nodes regardless of interleaved Optional
        // nodes).
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
    // Word-initial gap (synthesis twin of `ana_narrow_deletion`'s site-0 fix): C#
    // `SynthesisRewriteRuleSpec`'s empty-LHS pattern is a single `Segment|Anchor` constraint
    // (`SynthesisRewriteRuleSpec.cs:23-30`), so the shape's LEFT-ANCHOR node is itself an ordinary
    // match site; `RewriteRuleSpec.MatchSubrule`'s `_isTargetEmpty` branch then takes
    // `leftNode = rangeStart` (the anchor) / `rightNode = rangeEnd.Next` (`RewriteRuleSpec.cs:
    // 58-73`) — the left env is matched right-to-left AT the anchor (only `#`/no-env can hold,
    // exactly `left_env_ok(_, _, 0)`'s `only_anchor`/`None` arms) and the right env left-to-right
    // from the first post-anchor node — and `EpenthesisSynthesisRewriteSubruleSpec.ApplyRhs`
    // inserts `AddAfter(range.Start)` = right after the anchor
    // (`EpenthesisSynthesisRewriteSubruleSpec.cs:29-41`). `ms.nodes[0]` is always the left anchor
    // per `MutShape::from_shape`/`to_shape`'s invariant (anchors are never in `node_of`, so the
    // per-segment loop below can't reach this gap). Without this site, a bare-root word-initial
    // epenthesis (e.g. `∅ → ta / # _ C V #`) can never fire during synthesis-confirm.
    if left_env_ok(left, &segs, 0) && right_env_ok(right, &segs, 0) {
        sites.push(0);
    }
    for (site, &node) in node_of.iter().enumerate() {
        // A `Boundary` entry in `node_of` (present here because `segs(true)` feeds boundaries
        // into the matcher stream as transparently-skippable Optional segments — needed so an
        // environment can see *through* an internal morpheme boundary to a real segment beyond
        // it) is never itself a valid epenthesis TARGET/site. C#'s empty-LHS pattern is a single
        // `Symbol(HCFeatureSystem.Segment, HCFeatureSystem.Anchor)` constraint
        // (`SynthesisRewriteRuleSpec.cs:26-29`) — `Segment|Anchor`, never `Boundary` — so the
        // general pattern matcher never produces an LHS match sitting AT a boundary node, and
        // `RewriteRuleSpec.MatchSubrule`'s `rightNode = match.Range.End.Next` is therefore always
        // the shape's structural next node (which MAY be a boundary, matched transparently by the
        // right-environment traversal itself, but is never a *distinct* match position in its own
        // right). Without this guard, treating a boundary's own `node_of` slot as a candidate site
        // double-counts it: the REAL preceding segment's own site already reaches past the
        // boundary via the shared transparent-skip mechanism (`TraversalMethodBase.cs:203-222`,
        // ported by `pg_fst::traverse::Transduce::initialize`'s `start_anchor && optional` arm),
        // so re-checking the identical environment one node later (now anchored directly at the
        // real segment beyond the boundary, needing no skip at all) manufactures a second,
        // C#-nonexistent site — confirmed root cause of `csharp_port_rewrite.rs::epenthesis_rules`
        // sub-cases (2)/(5): root 19's shape "b+ubu" produced 3 "i" epenthesis sites (one per real
        // high vowel, PLUS one spurious extra at the boundary's own slot) instead of the correct 2.
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
// (verified: zero `<MetathesisRule>` elements in any of the three), so
// `probe_apply_rule_cached` refuses (`ProbeOutcome::Refused`) rather than silently mis-track positions
// if either is ever reached -- a conservative stance for a case the gate grammars never exercise,
// not a gap in the covered cases.

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
    // See `synthesize_with_mpr_cached`'s doc for the owning-table resolution rationale.
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
