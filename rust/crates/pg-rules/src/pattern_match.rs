//! Pattern search over one shape: the seam `pangloss serve` op `match` and later rule-effect queries share.
//!
//! It compiles a model [`Pattern`] with the synthesis environment compiler (`rewrite::compile_env`'s
//! matcher: subsumption matching, `#` anchors, optional boundary nodes) and lists every span the
//! pattern matches, anywhere in the word. Spans are in segment indices: boundary nodes are
//! optional inside the matcher but are not segments, so they are not counted.

use pg_grammar_model::chardef::CharDefTable;
use pg_grammar_model::model::{Grammar, Pattern, TableId};
use pg_shape::{NodeKind, Shape};

use crate::rewrite::{try_compile_env, EnvFst};

/// A pattern compiled once and applied to many shapes.
pub struct CompiledMatch {
    env: Option<EnvFst>,
    table: TableId,
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

/// Compiles `pattern` for shapes segmented with `table`.
pub fn compile_match(
    g: &Grammar,
    table: TableId,
    pattern: &Pattern,
) -> Result<CompiledMatch, MatchCompileError> {
    let env = try_compile_env(g, table, pattern).map_err(MatchCompileError)?;
    Ok(CompiledMatch { env, table })
}

impl CompiledMatch {
    /// Every match in `shape`, sorted ascending and deduplicated.
    pub fn matches(&self, g: &Grammar, shape: &Shape) -> Vec<ShapeMatch> {
        let Some(env) = &self.env else {
            return Vec::new();
        };
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
        let mut out: Vec<ShapeMatch> = env
            .find_spans(&segs)
            .into_iter()
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
    pattern: &Pattern,
    shape: &Shape,
) -> Result<Vec<ShapeMatch>, MatchCompileError> {
    Ok(compile_match(g, table, pattern)?.matches(g, shape))
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
