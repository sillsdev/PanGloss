//! The affix pieces of the rules by region and side, and the rules the tiling cannot model.

use super::classify::{classify, Item, Side, Tile, Why, Window, Windowed};
use super::TileCounts;
use crate::admission::plan::regions;
use crate::replace::SegAlphabet;
use pg_grammar::model::{Grammar, MorphRuleDef};
use std::collections::BTreeMap;

/// The edge rules' pieces by region and side, with the rules that are not edge rules.
#[derive(Default)]
pub(super) struct Pieces {
    pub(super) inner_left: Vec<Side>,
    pub(super) inner_right: Vec<Side>,
    pub(super) outer_left: Vec<Side>,
    pub(super) outer_right: Vec<Side>,
    pub(super) not_tileable: Vec<usize>,
    /// Some rule of the region copies stems whole more than once or joins stems.
    pub(super) repeat_inner: bool,
    pub(super) repeat_outer: bool,
    /// Edit subrules of inner rules and the nesting depth their applications can reach.
    pub(super) windowed: Vec<Windowed>,
    pub(super) edit_apps: usize,
    /// Most segments the edits can chop off the front and the back of a root.
    pub(super) chop_left: usize,
    pub(super) chop_right: usize,
}

/// Fewest segments of material an edit puts on its side.
fn items_min(items: &[Item]) -> usize {
    items
        .iter()
        .map(|item| match item {
            Item::Segments(lo, _) => *lo,
            Item::Lit(_, lo, _) => *lo,
        })
        .sum()
}

/// Most segments `applications` edits can cut from one end of a root: each cuts its window from the material in front of the root first.
fn root_chop(
    windowed: &[Windowed],
    applications: usize,
    side: impl Fn(&Windowed) -> (Window, &Vec<Item>),
) -> usize {
    let edits: Vec<(usize, usize)> = windowed
        .iter()
        .filter_map(|w| {
            let (window, items) = side(w);
            window.1.map(|hi| (hi, items_min(items)))
        })
        .collect();
    let mut best: BTreeMap<usize, usize> = BTreeMap::new();
    best.insert(0, 0);
    for _ in 0..applications {
        let mut next = best.clone();
        for (zone, cut) in &best {
            for (hi, put) in &edits {
                let from_root = hi.saturating_sub(*zone);
                let zone_after = zone.saturating_sub(*hi) + put;
                let total = cut + from_root;
                let entry = next.entry(zone_after).or_insert(0);
                *entry = (*entry).max(total);
            }
        }
        best = next;
    }
    best.values().copied().max().unwrap_or(0)
}

/// Most times the rules in `set` can apply in one derivation: each slot once, each stratum rule up to its maximum.
fn applications(g: &Grammar, set: &[bool], at: usize) -> usize {
    let mut total = 0;
    let mut listed = vec![false; set.len()];
    for (si, stratum) in g.strata.iter().enumerate() {
        for id in &stratum.mrules {
            listed[id.0 as usize] = true;
            if si <= at && set[id.0 as usize] {
                total += g.mrules[id.0 as usize].max_apps().max(1) as usize;
            }
        }
        for t in &stratum.templates {
            for slot in &g.templates[t.0 as usize].slots {
                let mut widest = 0;
                for id in &slot.rules {
                    listed[id.0 as usize] = true;
                    if set[id.0 as usize] {
                        widest = widest.max(g.mrules[id.0 as usize].max_apps().max(1) as usize);
                    }
                }
                if si <= at {
                    total += widest;
                }
            }
        }
    }
    for (index, rule) in g.mrules.iter().enumerate() {
        if set[index] && !listed[index] {
            total += rule.max_apps().max(1) as usize;
        }
    }
    total
}

/// The stratum that holds the phonological rules, or the count of strata when none does.
pub(super) fn phonological_stratum(g: &Grammar) -> usize {
    g.strata
        .iter()
        .position(|s| !s.prules.is_empty())
        .unwrap_or(g.strata.len())
}

fn has_any_side(sides: &[(Side, Side)]) -> bool {
    sides
        .iter()
        .any(|(l, r)| *l == Side::Any || *r == Side::Any)
}

fn count_literal(sides: &[Side]) -> usize {
    let literal = |side: &&Side| {
        matches!(side, Side::Lit(texts) if texts.iter().any(|x| !x.is_empty()))
            || **side == Side::Any
    };
    sides.iter().filter(literal).count()
}

pub(super) fn gather(g: &Grammar, alphabet: &SegAlphabet, counts: &mut TileCounts) -> Pieces {
    let region = regions(g);
    let mut pieces = Pieces::default();
    let mut edit_rules = vec![false; g.mrules.len()];
    let mut reasons: Vec<(String, usize)> = Vec::new();
    counts.rules = g.mrules.len();
    for (index, rule) in g.mrules.iter().enumerate() {
        let mut tile = classify(alphabet, rule, &g.char_tables[0]);
        if matches!(&tile, Tile::Edge { windowed, .. } if !windowed.is_empty() && region.outer[index])
        {
            tile = Tile::Not(Why::NotEdge);
        }
        let (sides, repeats, windowed) = match tile {
            Tile::Not(why) => {
                pieces.not_tileable.push(index);
                match reasons.iter_mut().find(|(label, _)| label == why.label()) {
                    Some((_, n)) => *n += 1,
                    None => reasons.push((why.label().to_owned(), 1)),
                }
                continue;
            }
            Tile::Edge {
                sides,
                repeats,
                windowed,
            } => (sides, repeats, windowed),
        };
        counts.tileable += 1;
        if !windowed.is_empty() {
            counts.windowed += 1;
            edit_rules[index] = true;
            pieces.windowed.extend(windowed);
        }
        if repeats {
            counts.repeating += 1;
            pieces.repeat_inner |= region.inner[index];
            pieces.repeat_outer |= region.outer[index];
        }
        counts.any_side_rules += usize::from(has_any_side(&sides));
        for (left, right) in sides {
            if region.inner[index] {
                pieces.inner_left.push(left.clone());
                pieces.inner_right.push(right.clone());
            }
            if region.outer[index] {
                pieces.outer_left.push(left);
                pieces.outer_right.push(right);
            }
        }
    }
    counts.not_tileable = pieces.not_tileable.len();
    counts.by_reason = reasons;
    pieces.windowed.sort();
    pieces.windowed.dedup();
    pieces.edit_apps = applications(g, &edit_rules, phonological_stratum(g));
    pieces.chop_left = root_chop(&pieces.windowed, pieces.edit_apps, |w| (w.left, &w.pre));
    pieces.chop_right = root_chop(&pieces.windowed, pieces.edit_apps, |w| (w.right, &w.post));
    counts.edit_apps = pieces.edit_apps;
    counts.windowed_distinct = pieces.windowed.len();
    counts.chop_left = pieces.chop_left;
    counts.chop_right = pieces.chop_right;
    counts.inner_left_pieces = count_literal(&pieces.inner_left);
    counts.inner_right_pieces = count_literal(&pieces.inner_right);
    counts.outer_left_pieces = count_literal(&pieces.outer_left);
    counts.outer_right_pieces = count_literal(&pieces.outer_right);
    pieces
}

pub(super) fn rule_label(g: &Grammar, rule: usize) -> String {
    match &g.mrules[rule] {
        MorphRuleDef::Compounding(def) => format!(
            "compounding {}",
            def.name.clone().unwrap_or_else(|| def.xml_id.clone())
        ),
        other => other
            .morpheme()
            .map(|m| g.morphemes[m.0 as usize].xml_key.clone())
            .unwrap_or_default(),
    }
}
