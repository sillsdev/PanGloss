//! Prepared literal spelling paths and their no-phonology coverage evidence.

use std::collections::HashMap;

use pg_grammar::chardef::CharDefTable;
use pg_grammar::model::{AllomorphId, Grammar, OutputAction, TableId};
use pg_shape::{CdSet, EffectiveCdSet, NodeKind, Shape, NO_CHAR_DEF};

use crate::emit::REP_VARIANT_BYTE_BUDGET;

pub(crate) struct SpellingRoutePlan<'g> {
    table: &'g CharDefTable,
    pub roots: HashMap<AllomorphId, Vec<String>>,
    pub affixes: HashMap<AllomorphId, Vec<String>>,
    bytes: usize,
}

impl<'g> SpellingRoutePlan<'g> {
    pub fn prepare(g: &'g Grammar) -> Option<Self> {
        let table_id = g.strata.first()?.table;
        if g.strata
            .iter()
            .any(|s| s.table != table_id || !s.prules.is_empty())
        {
            return None;
        }
        let mut plan = Self {
            table: &g.char_tables[table_id.0 as usize],
            roots: HashMap::new(),
            affixes: HashMap::new(),
            bytes: 0,
        };
        for entry in &g.entries {
            for allo in &entry.allomorphs {
                if allo.is_pattern
                    || allo.shape.shape.interior().any(|(_, kind, cd, flags)| {
                        kind == NodeKind::Segment && (cd == NO_CHAR_DEF || flags.is_iterative())
                    })
                {
                    return None;
                }
                let variants = plan.shape_variants(&allo.shape.shape, true)?;
                plan.retain(&variants)?;
                plan.roots.insert(allo.id, variants);
            }
        }
        for rule in &g.mrules {
            // Compound candidate recovery needs a separate proof of repeated-root identities.
            for allo in rule.affix_allomorphs()? {
                if matches!(
                    crate::emit::classify_affix(&allo.rhs),
                    crate::emit::Role::Infix
                        | crate::emit::Role::CircumfixPrefix
                        | crate::emit::Role::CircumfixSuffix
                        | crate::emit::Role::Process
                ) {
                    return None;
                }
                plan.check_actions(table_id, &allo.rhs)?;
                let mut variants = vec![String::new()];
                for action in &allo.rhs {
                    if let OutputAction::InsertSegments { shape, .. } = action {
                        variants = product(&variants, &plan.shape_variants(&shape.shape, true)?)?;
                    }
                }
                plan.retain(&variants)?;
                plan.affixes.insert(allo.id, variants);
            }
        }
        Some(plan)
    }

    fn check_actions(&self, table: TableId, actions: &[OutputAction]) -> Option<()> {
        actions
            .iter()
            .all(|action| match action {
                OutputAction::Copy(_) => true,
                OutputAction::InsertSegments {
                    table: origin,
                    shape,
                } => {
                    *origin == table
                        && shape.shape.interior().all(|(_, kind, cd, flags)| {
                            kind != NodeKind::Segment
                                || (cd != NO_CHAR_DEF && !flags.is_iterative())
                        })
                }
                _ => false,
            })
            .then_some(())
    }

    fn retain(&mut self, variants: &[String]) -> Option<()> {
        for v in variants {
            self.bytes = self
                .bytes
                .checked_add(v.len() + std::mem::size_of::<String>())?;
            if self.bytes > REP_VARIANT_BYTE_BUDGET {
                return None;
            }
        }
        Some(())
    }

    pub fn shape_variants(&self, shape: &Shape, boundaries_optional: bool) -> Option<Vec<String>> {
        let mut variants = vec![String::new()];
        for (i, kind, cd, flags) in shape.interior() {
            let mut reps = match kind {
                NodeKind::Segment => {
                    if flags.is_iterative() {
                        return None;
                    }
                    let cd_set = match shape.node_cd_set(i) {
                        EffectiveCdSet::Members(bits) => CdSet::Members(bits.clone()),
                        _ => CdSet::Unrestricted,
                    };
                    pg_rules::surface_probe::matching_reps(
                        self.table,
                        cd,
                        &cd_set,
                        shape.node_lanes(i),
                    )
                    .iter()
                    .map(|rep| pg_grammar::nfd::nfd(rep))
                    .collect::<Vec<_>>()
                }
                NodeKind::Boundary if boundaries_optional => {
                    let mut reps = self
                        .table
                        .get(pg_grammar::chardef::CharDefId(cd))
                        .representations_nfd()
                        .to_vec();
                    reps.push(String::new());
                    reps
                }
                _ => continue,
            };
            if flags.is_optional() {
                reps.push(String::new());
            }
            reps.sort_unstable();
            reps.dedup();
            if reps.is_empty() {
                return None;
            }
            variants = product(&variants, &reps)?;
        }
        Some(variants)
    }
}

fn product(left: &[String], right: &[String]) -> Option<Vec<String>> {
    let count = left.len().checked_mul(right.len())?;
    if count.checked_mul(std::mem::size_of::<String>())? > REP_VARIANT_BYTE_BUDGET {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    let mut bytes = 0usize;
    for a in left {
        for b in right {
            bytes = bytes.checked_add(a.len() + b.len() + std::mem::size_of::<String>())?;
            if bytes > REP_VARIANT_BYTE_BUDGET {
                return None;
            }
            out.push(format!("{a}{b}"));
        }
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}
