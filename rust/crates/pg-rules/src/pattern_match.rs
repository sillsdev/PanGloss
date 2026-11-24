//! Pattern search over one shape: the seam `pangloss serve` op `match` and later rule-effect queries share.
//!
//! It compiles a model [`Pattern`] with the synthesis environment compiler (`rewrite::compile_env`'s
//! matcher: subsumption matching, `#` anchors, optional boundary nodes) and lists every span the
//! pattern matches, anywhere in the word. Spans are in segment indices: boundary nodes are
//! optional inside the matcher but are not segments, so they are not counted.
//!
//! A pattern of the form `target / left _ right` compiles each of its three parts the same way and
//! keeps the target spans whose sides hold, checked by the code a rewrite rule checks its own
//! environment with, so the result agrees with where a rule applies.
//!
//! Alpha variables are bound by that same walk, in a rule's order: the target first, then the left
//! side, then the right, so one variable agrees across every position of one match. A feature bundle
//! such as `[+voice]` is a natural class the pattern text defines itself (see
//! [`crate::pattern_text::ParsedPattern::synthetic_classes`]).

use pg_grammar_model::chardef::{CharDefKind, CharDefTable};
use pg_grammar_model::model::{AnchorSide, Grammar, Pattern, PatternNode, TableId};

use crate::pattern_text::ParsedPattern;
use pg_shape::{NodeKind, Shape};

use crate::rewrite::{bind_exact_span, environment_holds, try_compile_env, Bindings, EnvFst};

/// A pattern compiled once and applied to many shapes.
pub struct CompiledMatch {
    env: Option<EnvFst>,
    /// The sides of `target / left _ right`; `Some` makes this an environment match.
    sides: Option<Sides>,
    table: TableId,
    /// Every non-anchor node is an optional group, so the empty string matches.
    nullable: bool,
    anchor_start: bool,
    anchor_end: bool,
}

/// The compiled sides of an environment; a side with no items is `None` and always holds.
struct Sides {
    left: Option<EnvFst>,
    right: Option<EnvFst>,
}

/// Why a pattern could not be compiled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchCompileError(pub String);

/// One match: half-open segment indices `[start, end)` over the shape's segments (boundaries excluded).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ShapeMatch {
    pub start: usize,
    pub end: usize,
}

/// Compiles `parsed` for shapes segmented with `table`.
pub fn compile_match(
    g: &Grammar,
    table: TableId,
    parsed: &ParsedPattern,
) -> Result<CompiledMatch, MatchCompileError> {
    let pattern = &parsed.target;
    let extra = &parsed.synthetic_classes;
    let env = try_compile_env(g, table, pattern, extra).map_err(MatchCompileError)?;
    let anchor = |side| {
        pattern
            .nodes
            .iter()
            .any(|n| matches!(n, PatternNode::Anchor(s) if *s == side))
    };
    let nullable = pattern.nodes.iter().all(|n| {
        matches!(n, PatternNode::Anchor(_)) || matches!(n, PatternNode::Quantifier { min: 0, .. })
    });
    let sides = match &parsed.environment {
        None => None,
        Some(sides) => {
            let side = |p: &Option<Pattern>| match p {
                None => Ok(None),
                Some(p) => try_compile_env(g, table, p, extra).map_err(MatchCompileError),
            };
            Some(Sides {
                left: side(&sides.left)?,
                right: side(&sides.right)?,
            })
        }
    };
    Ok(CompiledMatch {
        env,
        sides,
        table,
        nullable,
        anchor_start: anchor(AnchorSide::Left),
        anchor_end: anchor(AnchorSide::Right),
    })
}

impl CompiledMatch {
    /// Every match in `shape`, sorted ascending and deduplicated. For an environment pattern, the target spans whose sides hold.
    pub fn matches(&self, g: &Grammar, shape: &Shape) -> Vec<ShapeMatch> {
        let (segs, node_of) = crate::morph::segs_of(g, self.table, shape, true);
        // before[p] = segments among the first p matcher positions.
        let mut before = Vec::with_capacity(node_of.len() + 1);
        let mut n = 0usize;
        before.push(0);
        for &node in &node_of {
            if shape.kind(node) == NodeKind::Segment {
                n += 1;
            }
            before.push(n);
        }
        if let Some(sides) = &self.sides {
            return self.environment_matches(g, sides, &segs, &before);
        }
        let Some(env) = &self.env else {
            return Vec::new();
        };
        let mut out: Vec<ShapeMatch> = env
            .find_spans(&segs)
            .into_iter()
            .filter(|&(a, b)| !env.uses_alpha() || bind_exact_span(g, env, &segs[a..b]).is_some())
            .map(|(a, b)| ShapeMatch {
                start: before[a],
                end: before[b],
            })
            .collect();
        // The FST registers no span for the empty string, so a pattern that can match it gets its empty matches here, at every segment position the anchors allow.
        if self.nullable {
            for p in 0..=n {
                if (!self.anchor_start || p == 0) && (!self.anchor_end || p == n) {
                    out.push(ShapeMatch { start: p, end: p });
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The target spans, in matcher positions, whose left and right sides hold; an empty target (an insertion point) is every position.
    fn environment_matches(
        &self,
        g: &Grammar,
        sides: &Sides,
        segs: &[pg_fst::Segment],
        before: &[usize],
    ) -> Vec<ShapeMatch> {
        let mut spans = self
            .env
            .as_ref()
            .map_or_else(Vec::new, |env| env.find_spans(segs));
        if self.nullable {
            spans.extend((0..=segs.len()).map(|p| (p, p)));
        }
        let mut out: Vec<ShapeMatch> = spans
            .into_iter()
            .filter(|&(a, b)| {
                let bound: Option<Bindings> = match &self.env {
                    Some(env) if env.uses_alpha() => bind_exact_span(g, env, &segs[a..b]),
                    _ => Some(Bindings::default()),
                };
                bound.is_some_and(|bound| {
                    environment_holds(g, segs, a, b, &sides.left, &sides.right, bound)
                })
            })
            .map(|(a, b)| ShapeMatch {
                start: before[a],
                end: before[b],
            })
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// One-shot form of [`compile_match`] then [`CompiledMatch::matches`].
pub fn match_pattern(
    g: &Grammar,
    table: TableId,
    parsed: &ParsedPattern,
    shape: &Shape,
) -> Result<Vec<ShapeMatch>, MatchCompileError> {
    Ok(compile_match(g, table, parsed)?.matches(g, shape))
}

/// The byte span of each interior (non-anchor) node of `shape` inside `text`, which must be the NFD text the shape was segmented from. `None` when a node's character definition has no representation at its place, which would mean `shape` is not a segmentation of `text`.
pub fn node_byte_spans(
    table: &CharDefTable,
    shape: &Shape,
    text: &str,
) -> Option<Vec<(usize, usize)>> {
    let mut at = 0usize;
    let mut spans = Vec::new();
    for (_, _, cd, _) in shape.interior() {
        let def = table.get(pg_grammar_model::chardef::CharDefId(cd));
        let rest = &text[at..];
        let len = def
            .representations_nfd()
            .iter()
            .filter(|r| !r.is_empty() && rest.starts_with(r.as_str()))
            .map(String::len)
            .max()?;
        spans.push((at, at + len));
        at += len;
    }
    (at == text.len()).then_some(spans)
}

/// Whether `shape` holds a boundary node other than `allowed` (any boundary at all when `None`).
pub fn has_stray_boundary(
    shape: &Shape,
    allowed: Option<pg_grammar_model::chardef::CharDefId>,
) -> bool {
    shape.interior().any(|(_, kind, cd, _)| {
        kind == NodeKind::Boundary && Some(pg_grammar_model::chardef::CharDefId(cd)) != allowed
    })
}

/// [`node_byte_spans`] restricted to segment nodes, indexed by segment index (boundaries excluded).
pub fn segment_byte_spans(
    table: &CharDefTable,
    shape: &Shape,
    text: &str,
) -> Option<Vec<(usize, usize)>> {
    let spans = node_byte_spans(table, shape, text)?;
    Some(
        shape
            .interior()
            .zip(spans)
            .filter(|((_, kind, _, _), _)| *kind == NodeKind::Segment)
            .map(|(_, s)| s)
            .collect(),
    )
}

/// Whether `pattern` holds an anchor or a boundary node, which can lie on a morpheme edge where the form alone has no node.
pub fn has_edge_marker(g: &Grammar, table: TableId, pattern: &Pattern) -> bool {
    fn walk(chardefs: &CharDefTable, nodes: &[PatternNode]) -> bool {
        nodes.iter().any(|n| match n {
            PatternNode::Anchor(_) => true,
            PatternNode::CharDef(id) => chardefs.get(*id).kind() == CharDefKind::Boundary,
            PatternNode::Segments { shape, .. } => shape
                .shape
                .interior()
                .any(|(_, kind, _, _)| kind != NodeKind::Segment),
            PatternNode::Quantifier { children, .. } => walk(chardefs, children),
            PatternNode::Context(_) => false,
        })
    }
    walk(&g.char_tables[table.0 as usize], &pattern.nodes)
}

/// Whether a segment of `shape` has no value for some feature of a grammar that has features, or none of its features are authored. The matcher here (subsumption) then under-matches what rule application matches, where an absent value is permissive.
pub fn has_underspecified_segment(g: &Grammar, table: TableId, shape: &Shape) -> bool {
    let chardefs = &g.char_tables[table.0 as usize];
    let type_feature = chardefs.type_feature();
    shape.interior().any(|(_, kind, id, _)| {
        let def = chardefs.get(pg_grammar_model::chardef::CharDefId(id));
        // A grammar with no features at all matches by spelling, so only a grammar that has features can leave a segment without a value.
        kind == NodeKind::Segment
            && def.feature_lanes().len() > 1
            && (!def.has_authored_features()
                || (0..def.feature_lanes().len() as u32)
                    .map(pg_grammar_model::featsys::FlatIndex)
                    .any(|f| f != type_feature && def.feature_is_defaulted(f)))
    })
}

/// How many segments `shape` holds (boundaries excluded), the unit of [`ShapeMatch`] spans.
pub fn segment_count(shape: &Shape) -> usize {
    shape
        .interior()
        .filter(|(_, kind, _, _)| *kind == NodeKind::Segment)
        .count()
}

/// Most pattern nodes a straddle probe set is built for; a longer pattern is not modelled.
const STRADDLE_MAX_NODES: usize = 8;

/// One way part of a pattern may lie in the form and the rest in unknown neighbours.
struct Probe {
    matcher: CompiledMatch,
    /// The part in the form begins at the form's left edge.
    at_left_edge: bool,
    /// The part in the form ends at the form's right edge.
    at_right_edge: bool,
}

/// Every way a pattern could match across an edge of a form, the rest of it lying in an unknown neighbour.
///
/// It over-approximates: the neighbour is unconstrained and a quantifier cut by an edge is relaxed to match zero repetitions. A `false` answer is therefore proof that no match overlaps the form and reaches past an edge; a `true` answer proves nothing.
pub struct StraddleProbes {
    probes: Vec<Probe>,
}

/// The pattern as one segment per node; `Err` for an anchor or boundary node (it sits on a morpheme edge, not in a segment), alpha variables, a quantifier over several nodes, a pattern that can match no segment, or one over [`STRADDLE_MAX_NODES`].
fn flatten_for_straddle(
    g: &Grammar,
    table: TableId,
    pattern: &Pattern,
) -> Result<Vec<PatternNode>, MatchCompileError> {
    let refuse = |why: &str| Err(MatchCompileError(format!("straddle not modelled: {why}")));
    let chardefs = &g.char_tables[table.0 as usize];
    let is_boundary =
        |id: pg_grammar_model::chardef::CharDefId| chardefs.get(id).kind() == CharDefKind::Boundary;
    let mut out = Vec::new();
    for node in &pattern.nodes {
        match node {
            PatternNode::Anchor(_) => {}
            PatternNode::Segments { shape, .. } => {
                for (_, kind, cd, _) in shape.shape.interior() {
                    if kind == NodeKind::Segment {
                        out.push(PatternNode::CharDef(pg_grammar_model::chardef::CharDefId(
                            cd,
                        )));
                    }
                }
            }
            PatternNode::CharDef(id) => {
                if !is_boundary(*id) {
                    out.push(node.clone());
                }
            }
            PatternNode::Context(c) => {
                if !c.vars.is_empty() {
                    return refuse("alpha variable");
                }
                out.push(node.clone());
            }
            PatternNode::Quantifier { children, .. } => {
                let single = match children.as_slice() {
                    [PatternNode::Context(c)] => c.vars.is_empty(),
                    [PatternNode::CharDef(id)] => !is_boundary(*id),
                    _ => false,
                };
                if !single {
                    return refuse("quantifier over several nodes");
                }
                out.push(node.clone());
            }
        }
    }
    let nullable = out
        .iter()
        .all(|n| matches!(n, PatternNode::Quantifier { min: 0, .. }));
    if out.is_empty() || nullable {
        return refuse("pattern can match no segment");
    }
    if out.len() > STRADDLE_MAX_NODES {
        return refuse("pattern too long");
    }
    Ok(out)
}

/// A quantifier that may match no repetition; any other node unchanged.
fn relaxed(node: &PatternNode) -> PatternNode {
    match node {
        PatternNode::Quantifier { max, children, .. } => PatternNode::Quantifier {
            min: 0,
            max: *max,
            children: children.clone(),
        },
        other => other.clone(),
    }
}

/// Compiles the probes of `pattern`, a pattern the grammar itself holds (not one parsed from text with feature bundles, whose classes are not in the grammar) for forms segmented with `table`: for each run of nodes `i..=j` other than the whole pattern, that run alone must match a span of the form that reaches the left edge when nodes precede it and the right edge when nodes follow it.
pub fn compile_straddle(
    g: &Grammar,
    table: TableId,
    pattern: &Pattern,
) -> Result<StraddleProbes, MatchCompileError> {
    let flat = flatten_for_straddle(g, table, pattern)?;
    let m = flat.len();
    let is_quantifier = |n: &PatternNode| matches!(n, PatternNode::Quantifier { .. });
    let mut probes = Vec::new();
    for i in 0..m {
        for j in i..m {
            // Each end of the run is either cut by the edge (so more of the pattern lies beyond it) or not; a quantifier at the pattern's own end may also be cut with nothing of the pattern beyond it.
            let left_cuts: &[bool] = if i > 0 {
                &[true]
            } else if is_quantifier(&flat[0]) {
                &[false, true]
            } else {
                &[false]
            };
            let right_cuts: &[bool] = if j < m - 1 {
                &[true]
            } else if is_quantifier(&flat[m - 1]) {
                &[false, true]
            } else {
                &[false]
            };
            for &left_cut in left_cuts {
                for &right_cut in right_cuts {
                    if !left_cut && !right_cut && i == 0 && j == m - 1 {
                        continue;
                    }
                    let mut nodes = flat[i..=j].to_vec();
                    if left_cut && is_quantifier(&nodes[0]) {
                        nodes[0] = relaxed(&nodes[0]);
                    }
                    let last = nodes.len() - 1;
                    if right_cut && is_quantifier(&nodes[last]) {
                        nodes[last] = relaxed(&nodes[last]);
                    }
                    probes.push(Probe {
                        matcher: compile_match(
                            g,
                            table,
                            &ParsedPattern {
                                target: Pattern { nodes },
                                environment: None,
                                synthetic_classes: Vec::new(),
                            },
                        )?,
                        at_left_edge: left_cut,
                        at_right_edge: right_cut,
                    });
                }
            }
        }
    }
    Ok(StraddleProbes { probes })
}

impl StraddleProbes {
    /// Whether the pattern could match with part of it inside `shape` and part beyond an edge of it.
    pub fn may_straddle(&self, g: &Grammar, shape: &Shape) -> bool {
        let n = shape
            .interior()
            .filter(|(_, kind, _, _)| *kind == NodeKind::Segment)
            .count();
        self.probes.iter().any(|p| {
            p.matcher.matches(g, shape).iter().any(|m| {
                m.start < m.end
                    && (!p.at_left_edge || m.start == 0)
                    && (!p.at_right_edge || m.end == n)
            })
        })
    }
}

#[cfg(test)]
mod tests;
