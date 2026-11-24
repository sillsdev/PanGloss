//! Whether a rewrite rule's whole pattern (`left LHS right`) can be placed across a morph's edge, given the segments that can stand beyond it.
//!
//! Every segment comparison is the engine's own (`compile_match_as_engine`, a feature test), never identity, and a segment with an absent feature value accepts every node (the engine's permissive reading). The answer over-approximates: `false` for a side is proof that no placement of the pattern, with its target touching the morph, reaches past that edge; `true` proves nothing.

use pg_grammar_model::chardef::CharDefKind;
use pg_grammar_model::model::{Grammar, Pattern, PatternNode, TableId};
use pg_shape::{NodeKind, Shape};

use crate::pattern_match::{
    compile_match_as_engine, has_underspecified_segment, segment_count, CompiledMatch,
};
use crate::shape_feat::segment_with_features_load as segment_with_features;

/// One segment as a shape of its own, with whether it leaves a feature value open.
pub struct Seg {
    shape: Shape,
    underspecified: bool,
}

/// The segment written `text` in `table`, or `None` when `text` is not exactly one segment of it.
pub fn isolate(g: &Grammar, table: TableId, text: &str) -> Option<Seg> {
    let shape = segment_with_features(g, &g.char_tables[table.0 as usize], text).ok()?;
    (segment_count(&shape) == 1 && shape.interior().count() == 1).then(|| Seg {
        underspecified: has_underspecified_segment(g, table, &shape),
        shape,
    })
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

fn flatten_part(
    g: &Grammar,
    table: TableId,
    pattern: &Pattern,
    out: &mut Vec<PatternNode>,
) -> Result<(), &'static str> {
    let chardefs = &g.char_tables[table.0 as usize];
    for node in &pattern.nodes {
        match node {
            PatternNode::Anchor(_) => return Err("boundary_in_pattern"),
            PatternNode::Quantifier { .. } => return Err("optional_or_repeated_group"),
            PatternNode::Context(c) => {
                if !c.vars.is_empty() {
                    return Err("alpha_variable");
                }
                out.push(node.clone());
            }
            PatternNode::CharDef(id) => {
                if chardefs.get(*id).kind() == CharDefKind::Boundary {
                    return Err("boundary_in_pattern");
                }
                out.push(node.clone());
            }
            PatternNode::Segments { shape, .. } => {
                for (_, kind, cd, _) in shape.shape.interior() {
                    if kind != NodeKind::Segment {
                        return Err("boundary_in_pattern");
                    }
                    out.push(PatternNode::CharDef(pg_grammar_model::chardef::CharDefId(
                        cd,
                    )));
                }
            }
        }
    }
    Ok(())
}

/// The pattern `left LHS right` as flat single-segment nodes, or the reason it is not modelled: a boundary, anchor, optional or repeated group, alpha variable, or an empty target.
pub fn flatten(
    g: &Grammar,
    table: TableId,
    left: Option<&Pattern>,
    lhs: &Pattern,
    right: Option<&Pattern>,
) -> Result<FlatPattern, &'static str> {
    let mut nodes = Vec::new();
    if let Some(left) = left {
        flatten_part(g, table, left, &mut nodes)?;
    }
    let left_len = nodes.len();
    flatten_part(g, table, lhs, &mut nodes)?;
    let target = nodes.len() - left_len;
    if target == 0 {
        return Err("empty_lhs_epenthesis");
    }
    if let Some(right) = right {
        flatten_part(g, table, right, &mut nodes)?;
    }
    Ok(FlatPattern {
        nodes,
        left: left_len,
        target,
    })
}

/// One compiled engine test per node of a [`FlatPattern`].
pub struct NodeTests {
    tests: Vec<CompiledMatch>,
}

impl FlatPattern {
    pub fn compile(&self, g: &Grammar, table: TableId) -> Result<NodeTests, &'static str> {
        self.nodes
            .iter()
            .map(|node| {
                compile_match_as_engine(
                    g,
                    table,
                    &Pattern {
                        nodes: vec![node.clone()],
                    },
                )
                .map_err(|_| "pattern_not_compilable")
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|tests| NodeTests { tests })
    }
}

impl NodeTests {
    /// Whether node `node` accepts the segment; an underspecified segment accepts every node.
    pub fn accepts(&self, g: &Grammar, node: usize, seg: &Seg) -> bool {
        seg.underspecified
            || self.tests[node]
                .matches(g, &seg.shape)
                .iter()
                .any(|m| m.start < m.end)
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

/// Tries every placement of `flat` whose target overlaps `form`: a node over the form must accept that segment, the node directly beyond an edge a segment of that side's neighbour set, any node further out some segment of `anywhere`. A pattern node with no segment to stand on (the word ends) fails.
pub fn placements(
    g: &Grammar,
    flat: &FlatPattern,
    tests: &NodeTests,
    form: &[Seg],
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
                tests.accepts(g, node, &form[w as usize])
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
