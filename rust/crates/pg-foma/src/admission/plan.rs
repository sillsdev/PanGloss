//! Grammar analysis for rule admission: what each affix subrule leaves in an underlying string, rounding toward admit.

use super::piece::{self, Ctx};
use super::{AdmissionPlan, AdmissionRefusal, Piece, PlanCounts};
use crate::replace::SegAlphabet;
use pg_grammar::chardef::{CharDefKind, CharDefTable};
use pg_grammar::model::{
    AffixAllomorphDef, Grammar, MRuleId, MorphRuleDef, OutputAction, PartRef, Pattern, PatternNode,
};
use pg_shape::NodeKind;

/// A maximum count where `None` is unbounded.
pub(super) type Max = Option<usize>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Span {
    pub min: usize,
    pub max: Max,
}

impl Span {
    const ZERO: Span = Span {
        min: 0,
        max: Some(0),
    };
    const ANY: Span = Span { min: 0, max: None };

    fn fixed(n: usize) -> Span {
        Span {
            min: n,
            max: Some(n),
        }
    }

    fn plus(self, other: Span) -> Span {
        Span {
            min: self.min.saturating_add(other.min),
            max: add(self.max, other.max),
        }
    }
}

pub(super) fn add(a: Max, b: Max) -> Max {
    a.zip(b).map(|(x, y)| x.saturating_add(y))
}

/// One token of an inserted literal: a segment or a boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Tok {
    pub ch: char,
    pub segment: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Mat {
    Lit(Vec<Tok>),
    Wild { min: usize, max: Max },
}

/// Material that can sit between two stem parts, with the depth maxima of the gap.
#[derive(Clone, Debug)]
pub(super) struct Cut {
    pub left: Max,
    pub right: Max,
    pub material: Vec<Mat>,
}

/// A span of stem that a subrule deletes or changes, as the deepest positions it can touch.
#[derive(Clone, Debug)]
pub(super) struct Zone {
    pub left: Max,
    pub right: Max,
    pub node: Option<PatternNode>,
}

/// A seam between captured parts where boundary nodes are dropped.
#[derive(Clone, Copy, Debug)]
pub(super) struct Edge {
    pub left: Max,
    pub right: Max,
}

/// A maximal run of inserted segments and where it sits in the subrule's output.
#[derive(Clone, Debug)]
pub(super) struct Literal {
    pub toks: Option<Vec<Tok>>,
    pub before: usize,
    pub after: usize,
}

#[derive(Default)]
pub(super) struct Model {
    pub cuts: Vec<Cut>,
    pub zones: Vec<Zone>,
    pub edges: Vec<Edge>,
    pub deletion_left: Max,
    pub deletion_right: Max,
    pub root_floor: usize,
}

struct SubruleShape {
    literals: Vec<Literal>,
    /// Deleted segments that can lower left depths, and right depths.
    deleted_left: Max,
    deleted_right: Max,
}

fn node_span(table: &CharDefTable, node: &PatternNode) -> Span {
    match node {
        PatternNode::Context(_) => Span::fixed(1),
        PatternNode::CharDef(cd) => match (cd.0 as usize) < table.len() {
            true => Span::fixed(usize::from(table.get(*cd).kind() == CharDefKind::Segment)),
            false => Span::ANY,
        },
        PatternNode::Segments { shape, .. } => {
            shape
                .shape
                .interior()
                .fold(Span::ZERO, |acc, (_, kind, _, flags)| {
                    if kind != NodeKind::Segment {
                        return acc;
                    }
                    acc.plus(Span {
                        min: usize::from(!flags.is_optional() && !flags.is_iterative()),
                        max: if flags.is_iterative() { None } else { Some(1) },
                    })
                })
        }
        PatternNode::Anchor(_) => Span::ZERO,
        PatternNode::Quantifier { min, max, children } => {
            let child = nodes_span(table, children);
            let max = match (child.max, max) {
                (Some(0), _) => Some(0),
                (Some(c), Some(m)) => Some(c.saturating_mul(*m as usize)),
                _ => None,
            };
            Span {
                min: child.min.saturating_mul(*min as usize),
                max,
            }
        }
    }
}

fn nodes_span(table: &CharDefTable, nodes: &[PatternNode]) -> Span {
    nodes
        .iter()
        .fold(Span::ZERO, |acc, node| acc.plus(node_span(table, node)))
}

fn part_ref(action: &OutputAction) -> Option<usize> {
    match action {
        OutputAction::Copy(PartRef::Input(i)) | OutputAction::Modify(PartRef::Input(i), _) => {
            Some(*i as usize)
        }
        _ => None,
    }
}

fn segment_count(shape: &pg_shape::Shape) -> usize {
    shape
        .interior()
        .filter(|(_, kind, _, _)| *kind == NodeKind::Segment)
        .count()
}

fn action_span(action: &OutputAction, parts: &[Span]) -> Span {
    match action {
        OutputAction::Copy(_) | OutputAction::Modify(..) => match part_ref(action) {
            Some(i) => parts.get(i).copied().unwrap_or(Span::ANY),
            None => Span::ANY,
        },
        OutputAction::InsertSegments { shape, .. } => Span::fixed(segment_count(&shape.shape)),
        OutputAction::InsertContext(_) => Span::fixed(1),
    }
}

fn tokens(alphabet: &SegAlphabet, shape: &pg_shape::Shape) -> Option<Vec<Tok>> {
    if !SegAlphabet::shape_is_tokenizable(shape) {
        return None;
    }
    Some(
        shape
            .interior()
            .map(|(_, kind, cd, _)| Tok {
                ch: alphabet.token(pg_grammar::chardef::CharDefId(cd)),
                segment: kind == NodeKind::Segment,
            })
            .collect(),
    )
}

fn prefix_max(parts: &[Span]) -> Vec<Max> {
    let mut sums = vec![Some(0)];
    for part in parts {
        sums.push(add(*sums.last().expect("seeded"), part.max));
    }
    sums
}

fn suffix_max(parts: &[Span]) -> Vec<Max> {
    let mut sums = vec![Some(0); parts.len() + 1];
    for i in (0..parts.len()).rev() {
        sums[i] = add(sums[i + 1], parts[i].max);
    }
    sums
}

/// The index where the RHS copies every part once, in order, as one run.
fn intact_start(rhs: &[OutputAction], parts: usize) -> Option<usize> {
    if parts == 0 || rhs.len() < parts {
        return None;
    }
    (0..=rhs.len() - parts).find(|&start| {
        (0..parts).all(|i| rhs[start + i] == OutputAction::Copy(PartRef::Input(i as u16)))
    })
}

/// Leftmost in-order embedding of the parts into the actions.
fn embed(rhs: &[OutputAction], parts: usize) -> Vec<Option<usize>> {
    let mut cursor = 0;
    (0..parts)
        .map(|part| {
            let found = (cursor..rhs.len()).find(|&a| part_ref(&rhs[a]) == Some(part))?;
            cursor = found + 1;
            Some(found)
        })
        .collect()
}

fn material_of(alphabet: &SegAlphabet, action: &OutputAction, parts: &[Span]) -> Mat {
    let span = action_span(action, parts);
    match action {
        OutputAction::InsertSegments { shape, .. } => match tokens(alphabet, &shape.shape) {
            Some(toks) => Mat::Lit(toks),
            None => Mat::Wild {
                min: span.min,
                max: span.max,
            },
        },
        _ => Mat::Wild {
            min: span.min,
            max: span.max,
        },
    }
}

fn merge_lits(items: Vec<Mat>) -> Vec<Mat> {
    let mut out: Vec<Mat> = Vec::new();
    for item in items {
        match (out.last_mut(), item) {
            (Some(Mat::Lit(prev)), Mat::Lit(next)) => prev.extend(next),
            (_, item) => out.push(item),
        }
    }
    out
}

fn literals_of(alphabet: &SegAlphabet, rhs: &[OutputAction], mins: &[usize]) -> Vec<Literal> {
    let mut literals = Vec::new();
    let mut start = 0;
    while start < rhs.len() {
        if !matches!(rhs[start], OutputAction::InsertSegments { .. }) {
            start += 1;
            continue;
        }
        let mut end = start;
        let mut toks: Option<Vec<Tok>> = Some(Vec::new());
        let mut segments = 0;
        while let Some(OutputAction::InsertSegments { shape, .. }) = rhs.get(end) {
            segments += segment_count(&shape.shape);
            toks = match (toks, tokens(alphabet, &shape.shape)) {
                (Some(mut all), Some(next)) => {
                    all.extend(next);
                    Some(all)
                }
                _ => None,
            };
            end += 1;
        }
        if segments > 0 {
            literals.push(Literal {
                toks,
                before: mins[..start].iter().sum(),
                after: mins[end..].iter().sum(),
            });
        }
        start = end;
    }
    literals
}

fn analyse_subrule(
    table: &CharDefTable,
    alphabet: &SegAlphabet,
    allo: &AffixAllomorphDef,
    model: &mut Model,
) -> SubruleShape {
    let parts: Vec<Span> = allo
        .lhs
        .iter()
        .map(|part: &Pattern| nodes_span(table, &part.nodes))
        .collect();
    let (before_sums, after_sums) = (prefix_max(&parts), suffix_max(&parts));
    for i in 0..=parts.len() {
        model.edges.push(Edge {
            left: before_sums[i],
            right: after_sums[i],
        });
    }
    let mut mins: Vec<usize> = allo
        .rhs
        .iter()
        .map(|action| action_span(action, &parts).min)
        .collect();
    let intact = intact_start(&allo.rhs, parts.len());
    if let Some(start) = intact {
        let stem: usize = parts.iter().map(|p| p.min).sum();
        mins[start] = stem.max(model.root_floor);
        mins[start + 1..start + parts.len()].fill(0);
    }
    let literals = literals_of(alphabet, &allo.rhs, &mins);
    if intact.is_some() {
        return SubruleShape {
            literals,
            deleted_left: Some(0),
            deleted_right: Some(0),
        };
    }
    let pos = embed(&allo.rhs, parts.len());
    let matched: Vec<usize> = pos.iter().flatten().copied().collect();
    let mut deleted = Some(0);
    let (mut all_at_right, mut all_at_left) = (true, true);
    for (i, span) in parts.iter().enumerate() {
        if span.max == Some(0) {
            continue;
        }
        let changed = pos[i].is_some_and(|a| matches!(allo.rhs[a], OutputAction::Modify(..)));
        if pos[i].is_some() && !changed {
            continue;
        }
        if pos[i].is_none() {
            deleted = add(deleted, span.max);
            all_at_right &= after_sums[i + 1] == Some(0);
            all_at_left &= before_sums[i] == Some(0);
        }
        let extent = |outer: Max| add(outer, span.max).map(|v| v.saturating_sub(1));
        model.zones.push(Zone {
            left: extent(before_sums[i]),
            right: extent(after_sums[i + 1]),
            node: (allo.lhs[i].nodes.len() == 1).then(|| allo.lhs[i].nodes[0].clone()),
        });
    }
    for gap in 1..parts.len() {
        let left = (0..gap).rev().find_map(|i| pos[i]);
        let right = (gap..parts.len()).find_map(|i| pos[i]);
        let (Some(left), Some(right)) = (left, right) else {
            continue;
        };
        let items: Vec<Mat> = (left + 1..right)
            .filter(|a| !matched.contains(a))
            .map(|a| material_of(alphabet, &allo.rhs[a], &parts))
            .collect();
        if items.is_empty() {
            continue;
        }
        model.cuts.push(Cut {
            left: before_sums[gap],
            right: after_sums[gap],
            material: merge_lits(items),
        });
    }
    SubruleShape {
        literals,
        deleted_left: if all_at_right { Some(0) } else { deleted },
        deleted_right: if all_at_left { Some(0) } else { deleted },
    }
}

/// A compounding subrule must copy both members whole.
fn compounding_copies_whole(
    rule: &pg_grammar::model::CompoundingRuleDef,
) -> Result<(), AdmissionRefusal> {
    for sub in &rule.subrules {
        let run = |parts: &[Pattern], make: fn(u16) -> PartRef| -> bool {
            parts.is_empty()
                || sub.rhs.windows(parts.len()).any(|w| {
                    w.iter()
                        .enumerate()
                        .all(|(i, a)| *a == OutputAction::Copy(make(i as u16)))
                })
        };
        if !run(&sub.head_lhs, PartRef::Head) || !run(&sub.non_head_lhs, PartRef::NonHead) {
            return Err(AdmissionRefusal::CompoundingEditsStem(
                rule.name.clone().unwrap_or_else(|| rule.xml_id.clone()),
            ));
        }
    }
    Ok(())
}

fn compounding_edges(
    rule: &pg_grammar::model::CompoundingRuleDef,
    table: &CharDefTable,
    model: &mut Model,
) -> Result<(), AdmissionRefusal> {
    compounding_copies_whole(rule)?;
    for sub in &rule.subrules {
        for lhs in [&sub.head_lhs, &sub.non_head_lhs] {
            let parts: Vec<Span> = lhs.iter().map(|p| nodes_span(table, &p.nodes)).collect();
            let (before, after) = (prefix_max(&parts), suffix_max(&parts));
            for i in 0..=parts.len() {
                model.edges.push(Edge {
                    left: before[i],
                    right: after[i],
                });
            }
        }
    }
    Ok(())
}

fn root_floor(g: &Grammar) -> usize {
    g.entries
        .iter()
        .flat_map(|entry| &entry.allomorphs)
        .map(|allo| {
            allo.shape
                .shape
                .interior()
                .filter(|(_, kind, _, flags)| *kind == NodeKind::Segment && !flags.is_optional())
                .count()
        })
        .min()
        .unwrap_or(0)
}

/// Which rules sit in the strata up to the phonological one (inner) and after it (outer).
struct Regions {
    inner: Vec<bool>,
    outer: Vec<bool>,
    /// Applications of each rule inside the inner strata.
    inner_uses: Vec<usize>,
}

fn stratum_rules(g: &Grammar, stratum: &pg_grammar::model::StratumDef) -> Vec<MRuleId> {
    let slotted = stratum
        .templates
        .iter()
        .flat_map(|t| &g.templates[t.0 as usize].slots)
        .flat_map(|slot| slot.rules.iter().copied());
    stratum.mrules.iter().copied().chain(slotted).collect()
}

fn regions(g: &Grammar) -> Regions {
    let at = g
        .strata
        .iter()
        .position(|s| !s.prules.is_empty())
        .unwrap_or(g.strata.len());
    let n = g.mrules.len();
    let (mut inner_uses, mut outer) = (vec![0usize; n], vec![false; n]);
    let mut listed = vec![false; n];
    for (i, stratum) in g.strata.iter().enumerate() {
        for id in stratum_rules(g, stratum) {
            listed[id.0 as usize] = true;
            match i <= at {
                true => inner_uses[id.0 as usize] += 1,
                false => outer[id.0 as usize] = true,
            }
        }
    }
    let inner = (0..n).map(|i| inner_uses[i] > 0 || !listed[i]).collect();
    Regions {
        inner,
        outer,
        inner_uses,
    }
}

/// Adds one rule's widest deletion times its applications to a side total; unbounded stays unbounded.
fn accumulate(total: Max, widest: Max, applications: usize) -> Max {
    match widest {
        Some(0) => total,
        Some(w) => add(total, Some(w.saturating_mul(applications))),
        None => None,
    }
}

/// A subrule that copies each input part once, in order, and otherwise only inserts segments.
fn is_intact(allo: &AffixAllomorphDef) -> bool {
    let inserts = |run: &[OutputAction]| {
        run.iter()
            .all(|a| matches!(a, OutputAction::InsertSegments { .. }))
    };
    match intact_start(&allo.rhs, allo.lhs.len()) {
        Some(start) => inserts(&allo.rhs[..start]) && inserts(&allo.rhs[start + allo.lhs.len()..]),
        None => allo.lhs.is_empty() && inserts(&allo.rhs),
    }
}

/// An outer rule's pieces are its literals with no damage to model; `Err` when a subrule edits its input.
fn outer_regexes(
    alphabet: &SegAlphabet,
    allomorphs: &[AffixAllomorphDef],
) -> Result<Option<Vec<String>>, ()> {
    if !allomorphs.iter().all(is_intact) {
        return Err(());
    }
    let mut chosen = Vec::new();
    for allo in allomorphs {
        let mins = vec![0; allo.rhs.len()];
        let best = literals_of(alphabet, &allo.rhs, &mins)
            .iter()
            .filter_map(piece::render_exact)
            .max_by_key(|(_, plain)| *plain);
        match best {
            Some((regex, _)) => chosen.push(regex),
            None => return Ok(None),
        }
    }
    Ok((!chosen.is_empty()).then_some(chosen))
}

/// The best literal of every subrule rendered under the damage model; `None` when any subrule has no usable literal.
fn inner_regexes(
    ctx: &Ctx,
    subrules: &[SubruleShape],
    counts: &mut PlanCounts,
) -> Option<Vec<String>> {
    let mut chosen = Vec::new();
    for subrule in subrules {
        counts.literals += subrule.literals.len();
        let best = subrule
            .literals
            .iter()
            .filter_map(|lit| {
                let rendered = piece::render_literal(ctx, lit);
                if rendered.is_none() {
                    counts.unusable_literals += 1;
                }
                rendered
            })
            .max_by_key(|(_, plain)| *plain);
        chosen.push(best?.0);
    }
    (!chosen.is_empty()).then_some(chosen)
}

/// Analyses every morphological rule and renders one piece per subrule where a usable literal exists.
pub fn plan_admission(
    g: &Grammar,
    matches: &dyn Fn(&PatternNode, char) -> bool,
) -> Result<AdmissionPlan, AdmissionRefusal> {
    if g.char_tables.len() != 1 {
        return Err(AdmissionRefusal::CharacterTables(g.char_tables.len()));
    }
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let region = regions(g);
    let mut model = Model {
        root_floor: root_floor(g),
        ..Model::default()
    };
    let mut shapes: Vec<(usize, Vec<SubruleShape>)> = Vec::new();
    let mut outer: Vec<Option<Vec<String>>> = vec![None; g.mrules.len()];
    let (mut deletion_left, mut deletion_right) = (Some(0usize), Some(0usize));
    for (index, rule) in g.mrules.iter().enumerate() {
        let Some(allomorphs) = rule.affix_allomorphs() else {
            if let MorphRuleDef::Compounding(compound) = rule {
                match region.inner[index] {
                    true => compounding_edges(compound, table, &mut model)?,
                    false => compounding_copies_whole(compound)?,
                }
            }
            continue;
        };
        if region.outer[index] {
            outer[index] = outer_regexes(&alphabet, allomorphs).map_err(|()| {
                AdmissionRefusal::StratumLayout(format!(
                    "a rule after the phonological stratum edits its input: {}",
                    rule_item(g, rule)
                ))
            })?;
        }
        if !region.inner[index] {
            continue;
        }
        let subrules: Vec<SubruleShape> = allomorphs
            .iter()
            .map(|allo| analyse_subrule(table, &alphabet, allo, &mut model))
            .collect();
        let applications = rule.max_apps() as usize * region.inner_uses[index].max(1);
        let widest = |side: fn(&SubruleShape) -> Max| {
            subrules
                .iter()
                .try_fold(0usize, |acc, s| side(s).map(|b| acc.max(b)))
        };
        deletion_left = accumulate(deletion_left, widest(|s| s.deleted_left), applications);
        deletion_right = accumulate(deletion_right, widest(|s| s.deleted_right), applications);
        shapes.push((index, subrules));
    }
    model.deletion_left = deletion_left;
    model.deletion_right = deletion_right;
    let ctx = Ctx::new(&model, table, matches);
    let mut counts = PlanCounts {
        rules: g.mrules.len(),
        cuts: model.cuts.len(),
        zones: model.zones.len(),
        edges: model.edges.len(),
        deletion_left: model.deletion_left,
        deletion_right: model.deletion_right,
        root_floor: model.root_floor,
        outer_rules: region.outer.iter().filter(|o| **o).count(),
        ..PlanCounts::default()
    };
    let mut inner: Vec<Option<Vec<String>>> = vec![None; g.mrules.len()];
    for (index, subrules) in &shapes {
        if matches!(g.mrules[*index], MorphRuleDef::AffixProcess(_)) {
            inner[*index] = inner_regexes(&ctx, subrules, &mut counts);
        }
    }
    let (mut free, mut pieces, mut outer_pieces) =
        (vec![true; g.mrules.len()], Vec::new(), Vec::new());
    for (index, rule) in g.mrules.iter().enumerate() {
        if matches!(rule, MorphRuleDef::AffixProcess(_)) {
            counts.affix_rules += 1;
        }
        let usable = (!region.inner[index] || inner[index].is_some())
            && (!region.outer[index] || outer[index].is_some())
            && rule.affix_allomorphs().is_some();
        if !usable {
            continue;
        }
        free[index] = false;
        let make = |regexes: Vec<String>| {
            regexes.into_iter().map(move |regex| Piece {
                rule: MRuleId(index as u32),
                regex,
                grammar_item: rule_item(g, rule),
            })
        };
        pieces.extend(inner[index].take().into_iter().flat_map(&make));
        outer_pieces.extend(outer[index].take().into_iter().flat_map(&make));
    }
    counts.free = free.iter().filter(|f| **f).count();
    counts.pieces = pieces.len();
    counts.outer_pieces = outer_pieces.len();
    Ok(AdmissionPlan {
        free,
        pieces,
        outer_pieces,
        counts,
    })
}

fn rule_item(g: &Grammar, rule: &MorphRuleDef) -> String {
    rule.morpheme()
        .map(|m| g.morphemes[m.0 as usize].xml_key.clone())
        .unwrap_or_default()
}
