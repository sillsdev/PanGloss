//! Classifies each morphological rule as an edge rule, a windowed edit or outside the tiling model.

use crate::admission::plan::{intact_start, nodes_span, tokens, Span};
use crate::replace::SegAlphabet;
use pg_grammar::chardef::CharDefTable;
use pg_grammar::model::{
    AffixAllomorphDef, CompoundingSubruleDef, MorphRuleDef, OutputAction, PartRef,
};

/// Most arbitrary segments an edit may put on its sides before it stays outside the model.
const MAX_FREE: usize = 1;

/// A range of segments, the most unbounded.
pub(super) type Window = (usize, Option<usize>);

/// Material an edge rule puts on one side of the stem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Side {
    /// The literals the side may be, optional nodes taken both ways.
    Lit(Vec<String>),
    /// Some inserted node has no concrete token, so the side may hold any text.
    Any,
}

/// Why a rule is not tileable, for reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Why {
    Compounding,
    EmptyLhs,
    Modify,
    /// A part is copied twice, left out, or material sits between parts.
    NotEdge,
}

impl Why {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Why::Compounding => "compounding",
            Why::EmptyLhs => "empty-lhs",
            Why::Modify => "modify",
            Why::NotEdge => "not-edge (infix, reduplication, truncation)",
        }
    }
}

pub(super) enum Tile {
    /// Pieces per subrule; `repeats` when a stem may be copied whole more than once or joined to other stems.
    Edge {
        sides: Vec<(Side, Side)>,
        repeats: bool,
        windowed: Vec<Windowed>,
    },
    Not(Why),
}

/// Variants cap: an inserted run with more optional nodes than this may hold any text.
const MAX_VARIANTS: usize = 64;

fn side_of(alphabet: &SegAlphabet, actions: &[OutputAction]) -> Option<Side> {
    let mut variants: Vec<String> = vec![String::new()];
    for action in actions {
        match action {
            OutputAction::InsertSegments { shape, .. } => {
                let Some(toks) = tokens(alphabet, &shape.shape) else {
                    return Some(Side::Any);
                };
                for ((_, _, _, flags), tok) in shape.shape.interior().zip(&toks) {
                    if flags.is_iterative() {
                        return Some(Side::Any);
                    }
                    let with: Vec<String> =
                        variants.iter().map(|v| format!("{v}{}", tok.ch)).collect();
                    variants = if flags.is_optional() {
                        variants.into_iter().chain(with).collect()
                    } else {
                        with
                    };
                    if variants.len() > MAX_VARIANTS {
                        return Some(Side::Any);
                    }
                }
            }
            OutputAction::InsertContext(_) => return Some(Side::Any),
            _ => return None,
        }
    }
    variants.sort();
    variants.dedup();
    Some(Side::Lit(variants))
}

/// Runs of inserted material between whole-stem copies, and the copy count; `None` when anything else is there.
fn blocks(
    alphabet: &SegAlphabet,
    actions: &[OutputAction],
    copy: impl Fn(&[OutputAction]) -> Option<usize>,
) -> Option<(Vec<Side>, usize)> {
    let mut runs = Vec::new();
    let mut copies = 0;
    let mut from = 0;
    let mut at = 0;
    while at < actions.len() {
        match copy(&actions[at..]) {
            Some(width) => {
                runs.push(side_of(alphabet, &actions[from..at])?);
                copies += 1;
                at += width;
                from = at;
            }
            None => at += 1,
        }
    }
    runs.push(side_of(alphabet, &actions[from..])?);
    Some((runs, copies))
}

/// A run of inserted material may sit on either side of any stem, so every literal goes to both sides.
fn both_sides(runs: Vec<Side>) -> (Side, Side) {
    let mut all = Vec::new();
    for run in runs {
        match run {
            Side::Any => return (Side::Any, Side::Any),
            Side::Lit(texts) => all.extend(texts),
        }
    }
    all.sort();
    all.dedup();
    (Side::Lit(all.clone()), Side::Lit(all))
}

/// The pieces of one subrule and whether it repeats stems; whole-stem copies in order with insertions around them.
fn edge_subrule(
    alphabet: &SegAlphabet,
    allo: &AffixAllomorphDef,
) -> Result<((Side, Side), bool), Why> {
    if allo.lhs.is_empty() {
        return Err(Why::EmptyLhs);
    }
    if allo
        .rhs
        .iter()
        .any(|a| matches!(a, OutputAction::Modify(..)))
    {
        return Err(Why::Modify);
    }
    let parts = allo.lhs.len();
    let whole = |rest: &[OutputAction]| -> Option<usize> {
        let ok = rest.len() >= parts
            && (0..parts).all(|i| rest[i] == OutputAction::Copy(PartRef::Input(i as u16)));
        ok.then_some(parts)
    };
    let (runs, copies) = blocks(alphabet, &allo.rhs, whole).ok_or(Why::NotEdge)?;
    // Every copy must be a whole in-order run: no stray copy of one part.
    let stray = allo
        .rhs
        .iter()
        .filter(|a| matches!(a, OutputAction::Copy(_)))
        .count();
    if copies == 0 || stray != copies * parts {
        return Err(Why::NotEdge);
    }
    if copies == 1 {
        let start = intact_start(&allo.rhs, parts).ok_or(Why::NotEdge)?;
        let before = side_of(alphabet, &allo.rhs[..start]).ok_or(Why::NotEdge)?;
        let after = side_of(alphabet, &allo.rhs[start + parts..]).ok_or(Why::NotEdge)?;
        return Ok(((before, after), false));
    }
    Ok((both_sides(runs), true))
}

/// A compounding subrule that copies its head and non-head parts whole and in order, with insertions around them.
fn compound_subrule(
    alphabet: &SegAlphabet,
    sub: &CompoundingSubruleDef,
) -> Result<(Side, Side), Why> {
    if sub
        .rhs
        .iter()
        .any(|a| matches!(a, OutputAction::Modify(..)))
    {
        return Err(Why::Modify);
    }
    let (heads, others) = (sub.head_lhs.len(), sub.non_head_lhs.len());
    let whole = |rest: &[OutputAction]| -> Option<usize> {
        let run = |n: usize, part: fn(u16) -> PartRef| {
            n > 0
                && rest.len() >= n
                && (0..n).all(|i| rest[i] == OutputAction::Copy(part(i as u16)))
        };
        if run(heads, PartRef::Head) {
            Some(heads)
        } else if run(others, PartRef::NonHead) {
            Some(others)
        } else {
            None
        }
    };
    let (runs, copies) = blocks(alphabet, &sub.rhs, whole).ok_or(Why::NotEdge)?;
    let stray = sub
        .rhs
        .iter()
        .filter(|a| matches!(a, OutputAction::Copy(_)))
        .count();
    let counted: usize = {
        let mut at = 0;
        let mut width = 0;
        while at < sub.rhs.len() {
            match whole(&sub.rhs[at..]) {
                Some(w) => {
                    width += w;
                    at += w;
                }
                None => at += 1,
            }
        }
        width
    };
    if copies == 0 || stray != counted {
        return Err(Why::NotEdge);
    }
    Ok(both_sides(runs))
}

/// Material of an edit around the stem: a literal, or arbitrary segments of a length range.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Item {
    /// Texts, and the fewest and most segments any of them has.
    Lit(Vec<String>, usize, usize),
    Segments(usize, Option<usize>),
}

/// A subrule with one unbounded part copied once: the bounded parts form a window at each edge of the stem.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Windowed {
    pub(super) left: Window,
    pub(super) right: Window,
    pub(super) pre: Vec<Item>,
    pub(super) post: Vec<Item>,
}

/// The fewest and most segments the inserted material of these actions can have.
fn segments_of(alphabet: &SegAlphabet, actions: &[OutputAction]) -> (usize, usize) {
    let (mut lo, mut hi) = (0, 0);
    for action in actions {
        match action {
            OutputAction::InsertSegments { shape, .. } => {
                let toks = tokens(alphabet, &shape.shape).unwrap_or_default();
                for ((_, _, _, flags), tok) in shape.shape.interior().zip(&toks) {
                    if tok.segment {
                        hi += 1;
                        lo += usize::from(!flags.is_optional());
                    }
                }
            }
            _ => hi += 1,
        }
    }
    (lo, hi)
}

fn items_of(
    alphabet: &SegAlphabet,
    actions: &[OutputAction],
    spans: &[Span],
    middle: usize,
) -> Option<Vec<Item>> {
    let mut items = Vec::new();
    let mut run_from = 0;
    let flush = |from: usize, to: usize, items: &mut Vec<Item>| -> Option<()> {
        if to > from {
            match side_of(alphabet, &actions[from..to])? {
                Side::Lit(texts) => {
                    let (lo, hi) = segments_of(alphabet, &actions[from..to]);
                    items.push(Item::Lit(texts, lo, hi))
                }
                Side::Any => items.push(Item::Segments(0, None)),
            }
        }
        Some(())
    };
    for (at, action) in actions.iter().enumerate() {
        let part = match action {
            OutputAction::Copy(PartRef::Input(i)) | OutputAction::Modify(PartRef::Input(i), _) => {
                Some(*i as usize)
            }
            OutputAction::Copy(_) | OutputAction::Modify(..) => return None,
            _ => None,
        };
        let Some(part) = part else { continue };
        flush(run_from, at, &mut items)?;
        run_from = at + 1;
        if part == middle {
            return None;
        }
        let span = spans.get(part)?;
        items.push(Item::Segments(span.min, span.max));
    }
    flush(run_from, actions.len(), &mut items)?;
    Some(items)
}

fn windowed_subrule(
    alphabet: &SegAlphabet,
    table: &CharDefTable,
    allo: &AffixAllomorphDef,
) -> Option<Windowed> {
    let spans: Vec<Span> = allo
        .lhs
        .iter()
        .map(|p| nodes_span(table, &p.nodes))
        .collect();
    let mut unbounded = spans
        .iter()
        .enumerate()
        .filter(|(_, s)| s.max.is_none())
        .map(|(i, _)| i);
    let middle = unbounded.next()?;
    if unbounded.next().is_some() {
        return None;
    }
    let copies: Vec<usize> = allo
        .rhs
        .iter()
        .enumerate()
        .filter(
            |(_, a)| matches!(a, OutputAction::Copy(PartRef::Input(i)) if *i as usize == middle),
        )
        .map(|(at, _)| at)
        .collect();
    let [at] = copies.as_slice() else { return None };
    if allo
        .rhs
        .iter()
        .any(|a| matches!(a, OutputAction::Modify(PartRef::Input(i), _) if *i as usize == middle))
    {
        return None;
    }
    let pre = items_of(alphabet, &allo.rhs[..*at], &spans, middle)?;
    let post = items_of(alphabet, &allo.rhs[at + 1..], &spans, middle)?;
    let free: Option<usize> = pre
        .iter()
        .chain(&post)
        .map(|item| match item {
            Item::Segments(_, hi) => *hi,
            Item::Lit(..) => Some(0),
        })
        .sum();
    if free.is_none_or(|n| n > MAX_FREE) {
        return None;
    }
    let total = |parts: &[Span]| -> (usize, Option<usize>) {
        (
            parts.iter().map(|s| s.min).sum(),
            Some(parts.iter().filter_map(|s| s.max).sum()),
        )
    };
    Some(Windowed {
        left: total(&spans[..middle]),
        right: total(&spans[middle + 1..]),
        pre,
        post,
    })
}

pub(super) fn classify(alphabet: &SegAlphabet, rule: &MorphRuleDef, table: &CharDefTable) -> Tile {
    if let MorphRuleDef::Compounding(def) = rule {
        let mut sides = Vec::new();
        for sub in &def.subrules {
            match compound_subrule(alphabet, sub) {
                Ok(pair) => sides.push(pair),
                Err(why) => return Tile::Not(why),
            }
        }
        return Tile::Edge {
            sides,
            repeats: true,
            windowed: Vec::new(),
        };
    }
    let Some(allomorphs) = rule.affix_allomorphs() else {
        return Tile::Not(Why::Compounding);
    };
    let mut sides = Vec::new();
    let mut windowed = Vec::new();
    let mut repeats = false;
    for allo in allomorphs {
        match edge_subrule(alphabet, allo) {
            Ok((pair, again)) => {
                sides.push(pair);
                repeats |= again;
            }
            Err(why) => match windowed_subrule(alphabet, table, allo) {
                Some(edit) => windowed.push(edit),
                None => return Tile::Not(why),
            },
        }
    }
    Tile::Edge {
        sides,
        repeats,
        windowed,
    }
}
