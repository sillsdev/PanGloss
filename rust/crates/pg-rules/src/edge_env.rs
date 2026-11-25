//! Whether a rewrite rule's whole pattern (`left LHS right`) can be placed across a morph's edge, given the segments that can stand beyond it.
//!
//! Every segment comparison is the engine's own, never identity: the node's feature lanes unify with the segment's (a feature the segment leaves open is compatible with any value), or the compiled engine match holds. The answer over-approximates: `false` for a side is proof that no placement of the pattern, with its target touching the morph, reaches past that edge; `true` proves nothing.

use pg_grammar_model::chardef::CharDefKind;
use pg_grammar_model::model::{Grammar, Pattern, PatternNode, TableId};
use pg_shape::NodeKind;
pub use pg_shape::Shape;

use crate::pattern_match::{compile_match_as_engine, segment_count, CompiledMatch};
use crate::shape_feat::segment_with_features_load as segment_with_features;

/// One segment as a shape of its own.
#[derive(Clone)]
pub struct Seg {
    shape: Shape,
}

impl Seg {
    fn lanes(&self) -> &[u64] {
        let (at, ..) = self.shape.interior().next().expect("one segment");
        self.shape.node_lanes(at)
    }

    /// The segment's shape, which identifies it.
    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    fn from_shape(shape: Shape) -> Option<Seg> {
        (segment_count(&shape) == 1 && shape.interior().count() == 1).then_some(Seg { shape })
    }
}

/// What `rule` can leave at position `at` of `window` (one segment per target node), each subrule applied with its environment ignored (an over-approximation of where it fires); `None` when an output does not keep the window's length (a deletion or an expansion changes adjacency) or the engine panics.
pub fn written_at(
    g: &Grammar,
    table: TableId,
    rule: &pg_grammar_model::model::RewriteRuleDef,
    window: &[&Seg],
    at: usize,
) -> Option<Vec<Seg>> {
    // `synthesize` reads the grammar's first character table.
    if table.0 != 0 || at >= window.len() {
        return None;
    }
    let one = |shape: &Shape, i: usize| {
        let mut builder = pg_shape::ShapeBuilder::with_features(shape.feat_width());
        builder.push_segment_with_lanes(shape.char_def(i), shape.node_lanes(i));
        builder
    };
    let mut builder = pg_shape::ShapeBuilder::with_features(window.first()?.shape.feat_width());
    for seg in window {
        let (i, ..) = seg.shape.interior().next()?;
        builder.push_segment_with_lanes(seg.shape.char_def(i), seg.shape.node_lanes(i));
    }
    let input = builder.finish();
    let mut free = rule.clone();
    for sub in &mut free.subrules {
        sub.left_env = None;
        sub.right_env = None;
    }
    let outputs = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::rewrite::synthesize(g, &free, &input)
    }))
    .ok()?;
    outputs
        .into_iter()
        .map(|out| {
            let nodes: Vec<usize> = out.interior().map(|(i, ..)| i).collect();
            if nodes.len() != window.len() {
                return None;
            }
            Seg::from_shape(one(&out, nodes[at]).finish())
        })
        .collect()
}

/// The segment written `text` in `table`, or `None` when `text` is not exactly one segment of it.
pub fn isolate(g: &Grammar, table: TableId, text: &str) -> Option<Seg> {
    let shape = segment_with_features(g, &g.char_tables[table.0 as usize], text).ok()?;
    Seg::from_shape(shape)
}

/// Every segment `table` defines, by its first representation; a def that is not one segment of its own is left out and reported through the `bool`.
pub fn inventory(g: &Grammar, table: TableId) -> (Vec<(String, Seg)>, bool) {
    let mut out = Vec::new();
    let mut complete = true;
    for (_, def) in g.char_tables[table.0 as usize].iter() {
        if def.kind() != CharDefKind::Segment {
            continue;
        }
        let Some(text) = def.representations_nfd().first().filter(|r| !r.is_empty()) else {
            complete = false;
            continue;
        };
        match isolate(g, table, text) {
            Some(seg) => out.push((text.clone(), seg)),
            None => complete = false,
        }
    }
    (out, complete)
}

/// A pattern as one single-segment node per position: `left` nodes, then the target, then `right` nodes.
pub struct FlatPattern {
    nodes: Vec<PatternNode>,
    left: usize,
    target: usize,
}

impl FlatPattern {
    pub fn target_len(&self) -> usize {
        self.target
    }
}

/// Most variants an optional group may expand a pattern into.
const MAX_VARIANTS: usize = 16;

/// The single-segment node sequences `nodes` can stand for: and an optional group (`min` 0, `max` 1) is expanded with and without its children.
fn expand(
    g: &Grammar,
    table: TableId,
    nodes: &[PatternNode],
    env: bool,
) -> Result<Vec<Vec<PatternNode>>, &'static str> {
    let chardefs = &g.char_tables[table.0 as usize];
    let mut variants: Vec<Vec<PatternNode>> = vec![Vec::new()];
    for node in nodes {
        match node {
            PatternNode::Anchor(_) => return Err("boundary_in_pattern"),
            PatternNode::Quantifier { min, max, children } if env => {
                let inner = expand(g, table, children, env)?;
                match (*min, *max) {
                    (1, Some(1)) => {
                        variants = product(&variants, &inner);
                    }
                    (0, Some(1)) => {
                        let with = product(&variants, &inner);
                        variants.extend(with);
                    }
                    _ => return Err("optional_or_repeated_group"),
                }
            }
            PatternNode::Quantifier { .. } => return Err("optional_or_repeated_group"),
            PatternNode::Context(c) => {
                if !c.vars.is_empty() {
                    return Err("alpha_variable");
                }
                for v in &mut variants {
                    v.push(node.clone());
                }
            }
            PatternNode::CharDef(id) => {
                if chardefs.get(*id).kind() == CharDefKind::Boundary {
                    return Err("boundary_in_pattern");
                }
                for v in &mut variants {
                    v.push(node.clone());
                }
            }
            PatternNode::Segments { shape, .. } => {
                for (_, kind, cd, _) in shape.shape.interior() {
                    if kind != NodeKind::Segment {
                        return Err("boundary_in_pattern");
                    }
                    let seg = PatternNode::CharDef(pg_grammar_model::chardef::CharDefId(cd));
                    for v in &mut variants {
                        v.push(seg.clone());
                    }
                }
            }
        }
        if variants.len() > MAX_VARIANTS {
            return Err("optional_or_repeated_group");
        }
    }
    Ok(variants)
}

fn product(left: &[Vec<PatternNode>], right: &[Vec<PatternNode>]) -> Vec<Vec<PatternNode>> {
    left.iter()
        .flat_map(|l| {
            right.iter().map(move |r| {
                let mut both = l.clone();
                both.extend(r.iter().cloned());
                both
            })
        })
        .collect()
}

/// The patterns `left LHS right` can stand for, as flat single-segment nodes (one per way of taking its optional groups), or the reason it is not modelled: a boundary or anchor in the target, a repeated group, an alpha variable, or an empty target.
pub fn flatten_all(
    g: &Grammar,
    table: TableId,
    left: Option<&Pattern>,
    lhs: &Pattern,
    right: Option<&Pattern>,
) -> Result<Vec<FlatPattern>, &'static str> {
    let lefts = match left {
        Some(p) => expand(g, table, &p.nodes, true)?,
        None => vec![Vec::new()],
    };
    let rights = match right {
        Some(p) => expand(g, table, &p.nodes, true)?,
        None => vec![Vec::new()],
    };
    let mut targets = expand(g, table, &lhs.nodes, false)?;
    let target = targets.pop().filter(|t| !t.is_empty());
    let Some(target) = target else {
        return Err("empty_lhs_epenthesis");
    };
    let mut out = Vec::new();
    for l in &lefts {
        for r in &rights {
            let mut nodes = l.clone();
            let left_len = nodes.len();
            nodes.extend(target.iter().cloned());
            nodes.extend(r.iter().cloned());
            out.push(FlatPattern {
                nodes,
                left: left_len,
                target: target.len(),
            });
        }
    }
    Ok(out)
}

/// [`flatten_all`] for a pattern with no optional group.
pub fn flatten(
    g: &Grammar,
    table: TableId,
    left: Option<&Pattern>,
    lhs: &Pattern,
    right: Option<&Pattern>,
) -> Result<FlatPattern, &'static str> {
    let mut all = flatten_all(g, table, left, lhs, right)?;
    if all.len() == 1 {
        Ok(all.remove(0))
    } else {
        Err("optional_or_repeated_group")
    }
}

/// One compiled engine test per node of a [`FlatPattern`].
pub struct NodeTests {
    tests: Vec<CompiledMatch>,
    /// Each node's feature constraint; the engine takes a node when a segment's lanes unify with it.
    lanes: Vec<Vec<u64>>,
    /// The grammar has features; without them the engine matches by spelling and lanes say nothing.
    featured: bool,
}

impl FlatPattern {
    pub fn compile(&self, g: &Grammar, table: TableId) -> Result<NodeTests, &'static str> {
        let chardefs = &g.char_tables[table.0 as usize];
        let mut tests = Vec::new();
        let mut lanes = Vec::new();
        for node in &self.nodes {
            tests.push(
                compile_match_as_engine(
                    g,
                    table,
                    &Pattern {
                        nodes: vec![node.clone()],
                    },
                )
                .map_err(|_| "pattern_not_compilable")?,
            );
            lanes.push(crate::rewrite::node_full_lanes(g, chardefs, node));
        }
        Ok(NodeTests {
            featured: chardefs.iter().any(|(_, d)| d.feature_lanes().len() > 1),
            tests,
            lanes,
        })
    }
}

impl NodeTests {
    /// Whether node `node` accepts the segment: its lanes unify with the node's (the engine's test, where a feature the segment leaves open is compatible with any value), or the compiled feature match holds.
    pub fn accepts(&self, g: &Grammar, node: usize, seg: &Seg) -> bool {
        (self.featured && pg_featstruct::flat_unifiable(seg.lanes(), &self.lanes[node]))
            || self.tests[node]
                .matches(g, &seg.shape)
                .iter()
                .any(|m| m.start < m.end)
    }

    /// The number of nodes.
    pub fn len(&self) -> usize {
        self.tests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tests.is_empty()
    }

    /// Whether any node accepts the segment.
    pub fn any_node_accepts(&self, g: &Grammar, seg: &Seg) -> bool {
        (0..self.tests.len()).any(|node| self.accepts(g, node, seg))
    }
}

/// Which placements of a pattern (its target touching the morph) are not ruled out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placements {
    /// Some placement lies wholly inside the morph.
    pub inside: bool,
    /// Some placement reaches left of the morph's first segment.
    pub left: bool,
    /// Some placement reaches right of the morph's last segment.
    pub right: bool,
}

/// The neighbours of a morph: the segments that may stand directly before its first and after its last segment, and every segment that may stand anywhere beyond.
pub struct Neighbours<'a> {
    pub before: &'a [&'a Seg],
    pub after: &'a [&'a Seg],
    pub anywhere: &'a [&'a Seg],
}

/// Tries every placement of `flat` whose target overlaps `form` (each position lists the segments it may hold by then): a node over the form must accept one of them, the node directly beyond an edge a segment of that side's neighbour set, any node further out some segment of `anywhere`. A pattern node with no segment to stand on (the word ends) fails.
pub fn placements(
    g: &Grammar,
    flat: &FlatPattern,
    tests: &NodeTests,
    form: &[Vec<&Seg>],
    near: &Neighbours<'_>,
) -> Placements {
    let m = flat.nodes.len() as isize;
    let n = form.len() as isize;
    let (a, t) = (flat.left as isize, flat.target as isize);
    let any = |node: usize, set: &[&Seg]| set.iter().any(|s| tests.accepts(g, node, s));
    let before_ok: Vec<bool> = (0..m as usize).map(|j| any(j, near.before)).collect();
    let after_ok: Vec<bool> = (0..m as usize).map(|j| any(j, near.after)).collect();
    let far_ok: Vec<bool> = (0..m as usize).map(|j| any(j, near.anywhere)).collect();
    let mut out = Placements {
        inside: false,
        left: false,
        right: false,
    };
    // The target covers `[s + a, s + a + t)`; it touches the morph when that meets `[0, n)`.
    for s in (1 - a - t)..=(n - 1 - a) {
        let feasible = (0..m).all(|j| {
            let w = s + j;
            let node = j as usize;
            if w < 0 {
                if w == -1 {
                    before_ok[node]
                } else {
                    far_ok[node]
                }
            } else if w < n {
                form[w as usize]
                    .iter()
                    .any(|seg| tests.accepts(g, node, seg))
            } else if w == n {
                after_ok[node]
            } else {
                far_ok[node]
            }
        });
        if !feasible {
            continue;
        }
        let (left, right) = (s < 0, s + m > n);
        out.left |= left;
        out.right |= right;
        out.inside |= !left && !right;
    }
    out
}
