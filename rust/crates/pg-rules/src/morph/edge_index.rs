//! A conservative index of mandatory inserted allomorph edges for analysis rule dispatch.
//!
//! A rule is excluded only when every allomorph fails the existing edge prefilter on the same
//! concrete identity, with unconstrained feature lanes. The real segment can only be more
//! restrictive, so no excluded allomorph could match. Unknown, optional, or non-concrete edges
//! remain candidates; surviving rules still run the original matcher and confirmation. Preparing
//! rows from each intermediate shape preserves this argument throughout the cascade.

use pg_fst::Segment;
use pg_grammar_model::model::{Grammar, MRuleId, MorphRuleDef, OutputAction, TableId};
use pg_shape::{NodeKind, Shape, NO_CHAR_DEF};

use crate::bridge::{id_lane_width, push_id_lane, StrRepMatcher};
use crate::cache::{owning_table_for_morpheme, RuleCache};

/// Maximum retained index size; oversized grammars use the original unindexed path.
const MAX_BYTES: usize = 2 * 1024 * 1024;
/// A rule whose edge cannot be excluded by this index.
const FALLBACK: u32 = u32::MAX;

#[derive(Default)]
/// Character-table edge rows containing packed candidate-rule bits.
pub(crate) struct EdgeIndex {
    offsets: Box<[u32]>,
    tables: Box<[u32]>,
    candidates: Box<[u64]>,
    row_words: usize,
}

impl EdgeIndex {
    pub(crate) fn build(g: &Grammar, cache: &RuleCache) -> Self {
        let row_words = g.mrules.len().div_ceil(64);
        if row_words == 0 {
            return Self::default();
        }
        let mut offsets = Vec::with_capacity(g.char_tables.len() + 1);
        let mut rows = 0usize;
        for table in &g.char_tables {
            let Ok(offset) = u32::try_from(rows) else {
                return Self::default();
            };
            offsets.push(offset);
            let Some(next) = table.len().checked_mul(2).and_then(|n| rows.checked_add(n)) else {
                return Self::default();
            };
            rows = next;
        }
        let Ok(end) = u32::try_from(rows) else {
            return Self::default();
        };
        offsets.push(end);
        let Some(words) = rows.checked_mul(row_words) else {
            return Self::default();
        };
        let Some(bytes) = words
            .checked_mul(8)
            .and_then(|n| n.checked_add((offsets.len() + g.mrules.len()).checked_mul(4)?))
        else {
            return Self::default();
        };
        if bytes > MAX_BYTES - std::mem::size_of::<Self>() {
            return Self::default();
        }
        let mut tables = vec![FALLBACK; g.mrules.len()];
        let mut candidates = vec![u64::MAX; words];
        for (rule_index, rule) in g.mrules.iter().enumerate() {
            let MorphRuleDef::AffixProcess(rule) = rule else {
                continue;
            };
            if rule.allomorphs.is_empty() {
                continue;
            }
            let table = owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
            let Some(defs) = g.char_tables.get(table.0 as usize) else {
                continue;
            };
            let allos: Option<Vec<_>> = rule
                .allomorphs
                .iter()
                .map(|a| {
                    cache
                        .allomorph(a.id)
                        .ana_lhs
                        .as_ref()
                        .map(|(_, lhs)| (a, lhs))
                })
                .collect();
            let Some(allos) = allos else { continue };
            let mut useful = false;
            for cd in 0..defs.len() {
                let width = pg_grammar_model::membership::width(&g.phon_features);
                let mut lanes = vec![u64::MAX; width];
                if let Some(w) = id_lane_width(g, table) {
                    push_id_lane(&mut lanes, w, 1u64 << cd);
                } else {
                    StrRepMatcher::identity(g, defs).input(
                        &mut lanes,
                        cd as u32,
                        &pg_shape::CdSet::Unrestricted,
                    );
                }
                let segment = Segment::new(lanes);
                for first in [true, false] {
                    if allos.iter().all(|(allo, lhs)| {
                        let action = if first { allo.rhs.first() } else { allo.rhs.last() };
                        matches!(action, Some(OutputAction::InsertSegments { shape, .. })
                            if shape.shape.interior().any(|(_, kind, _, _)| kind == NodeKind::Segment))
                            && lhs.inserted_edge_rejects_at(&segment, first)
                    })
                    {
                        let row = offsets[table.0 as usize] as usize + cd * 2 + usize::from(!first);
                        candidates[row * row_words + rule_index / 64] &= !(1 << (rule_index % 64));
                        useful = true;
                    }
                }
            }
            if useful {
                tables[rule_index] = u32::from(table.0);
            }
        }
        if tables.iter().all(|table| *table == FALLBACK) {
            return Self::default();
        }
        Self {
            offsets: offsets.into_boxed_slice(),
            tables: tables.into_boxed_slice(),
            candidates: candidates.into_boxed_slice(),
            row_words,
        }
    }

    pub(crate) fn allocated_bytes(&self) -> usize {
        self.offsets.len() * 4 + self.tables.len() * 4 + self.candidates.len() * 8
    }

    pub(crate) fn candidates(&self, table: TableId, shape: &Shape) -> Option<EdgeCandidates<'_>> {
        let table = table.0 as usize;
        let start = *self.offsets.get(table)? as usize;
        let end = *self.offsets.get(table + 1)? as usize;
        let mut edges = (0..shape.len()).filter(|&i| shape.kind(i) == NodeKind::Segment);
        let first = edges.next()?;
        let last = edges.next_back().unwrap_or(first);
        let mut rows = [None, None];
        for (side, i) in [first, last].into_iter().enumerate() {
            let cd = shape.char_def(i);
            if shape.flags(i).is_optional() || cd == NO_CHAR_DEF {
                continue;
            }
            let row = start + cd as usize * 2 + side;
            if row >= end {
                continue;
            }
            let bits = &self.candidates[row * self.row_words..(row + 1) * self.row_words];
            if bits.iter().any(|&word| word != u64::MAX) {
                rows[side] = Some(bits);
            }
        }
        if rows.iter().all(Option::is_none) {
            return None;
        }
        Some(EdgeCandidates {
            tables: &self.tables,
            table: table as u32,
            rows,
            charged: std::cell::Cell::new(false),
        })
    }
}

/// Borrowed rows for one current shape, sharing a single lookup charge across rule dispatches.
pub(crate) struct EdgeCandidates<'a> {
    tables: &'a [u32],
    table: u32,
    rows: [Option<&'a [u64]>; 2],
    charged: std::cell::Cell<bool>,
}

impl EdgeCandidates<'_> {
    pub(crate) fn charge_needed(&self) -> bool {
        !self.charged.replace(true)
    }

    pub(crate) fn rejects(&self, rule: MRuleId) -> bool {
        let i = rule.0 as usize;
        if self.tables.get(i).copied() != Some(self.table) {
            return false;
        }
        self.rows
            .iter()
            .flatten()
            .any(|bits| bits[i / 64] & (1 << (i % 64)) == 0)
    }
}
