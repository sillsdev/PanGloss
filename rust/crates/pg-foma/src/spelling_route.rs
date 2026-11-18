//! Prepared literal spelling paths and their no-phonology coverage evidence.

use std::collections::HashMap;

use pg_grammar::chardef::CharDefTable;
use pg_grammar::model::{
    AllomorphId, CompoundingRuleDef, Grammar, LexEntryId, MorphRuleDef, OutputAction, PartRef,
    TableId,
};
use pg_shape::{CdSet, EffectiveCdSet, NodeKind, Shape, NO_CHAR_DEF};

use crate::emit::REP_VARIANT_BYTE_BUDGET;

/// What the emitting plan proved about its compound routes: every subrule copies its head parts and its non-head parts once each, in order and as two contiguous blocks, around boundary insertions only, so each compound surface is the two stems' spellings joined. Created only by `SpellingRoutePlan::prepare` and consumed only by the compound chain builders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompoundRouteProof {
    pub rules: usize,
    pub subrules: usize,
    /// Every non-empty spelling the compound outputs' boundary insertions can take in a word, between the two stems; the empty spelling is the plain concatenation.
    pub boundary_spellings: Vec<String>,
    /// Subrules whose output puts the non-head stem first; the physical head-then-non-head path serves them through the swapped licence.
    pub reversed_subrules: usize,
}

pub(crate) struct SpellingRoutePlan<'g> {
    table: &'g CharDefTable,
    pub roots: HashMap<AllomorphId, Vec<String>>,
    pub affixes: HashMap<AllomorphId, Vec<String>>,
    /// `Some` exactly when the grammar has compounding rules and every one of them is proved.
    pub compound: Option<CompoundRouteProof>,
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
            compound: None,
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
        let mut compound = CompoundRouteProof {
            rules: 0,
            subrules: 0,
            boundary_spellings: Vec::new(),
            reversed_subrules: 0,
        };
        for rule in &g.mrules {
            if let MorphRuleDef::Compounding(def) = rule {
                plan.prove_compound(g, table_id, def, &mut compound)?;
                continue;
            }
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
        if compound.rules > 0 {
            if crate::emit::effective_compound_stems(g) > 2 || !morphology_leaves_mpr_alone(g) {
                return None;
            }
            compound.boundary_spellings.sort_unstable();
            compound.boundary_spellings.dedup();
            plan.compound = Some(compound);
        }
        Some(plan)
    }

    /// Proves one compounding rule's outputs are the head parts, at most one boundary insertion, then the non-head parts (or the reverse order), or `None`. The emitter writes the boundary spellings only between the two stems, so an insertion anywhere else, or two insertions, must refuse.
    fn prove_compound(
        &self,
        g: &Grammar,
        table: TableId,
        rule: &CompoundingRuleDef,
        proof: &mut CompoundRouteProof,
    ) -> Option<()> {
        if rule.subrules.is_empty() {
            return None;
        }
        for sr in &rule.subrules {
            let mut order: Vec<PartRef> = Vec::new();
            let mut insert_at: Vec<usize> = Vec::new();
            for action in &sr.rhs {
                match action {
                    OutputAction::Copy(part @ (PartRef::Head(_) | PartRef::NonHead(_))) => {
                        order.push(*part)
                    }
                    OutputAction::InsertSegments {
                        table: origin,
                        shape,
                    } if *origin == table
                        && shape
                            .shape
                            .interior()
                            .all(|(_, kind, _, _)| kind == NodeKind::Boundary) =>
                    {
                        insert_at.push(order.len());
                        let variants = self.shape_variants(&shape.shape, true)?;
                        proof
                            .boundary_spellings
                            .extend(variants.into_iter().filter(|v| !v.is_empty()));
                    }
                    _ => return None,
                }
            }
            let heads = sr.head_lhs.len();
            let non_heads = sr.non_head_lhs.len();
            if heads == 0 || non_heads == 0 || order.len() != heads + non_heads {
                return None;
            }
            let head_first = matches!(order[0], PartRef::Head(_));
            let heads_in_order = (0..heads as u16).map(PartRef::Head);
            let non_heads_in_order = (0..non_heads as u16).map(PartRef::NonHead);
            let expected: Vec<PartRef> = if head_first {
                heads_in_order.chain(non_heads_in_order).collect()
            } else {
                non_heads_in_order.chain(heads_in_order).collect()
            };
            if order != expected {
                return None;
            }
            let first_block = if head_first { heads } else { non_heads };
            if insert_at.iter().any(|at| *at != first_block) || insert_at.len() > 1 {
                return None;
            }
            if !head_first {
                self.reversed_order_is_licensed(g, rule, sr)?;
                proof.reversed_subrules += 1;
            }
            proof.subrules += 1;
        }
        proof.rules += 1;
        Some(())
    }

    /// The physical path is licensed-head stem then licensed-non-head stem. A subrule that writes the non-head stem first is served by that path only if every entry it licenses as non-head is also licensed as a head somewhere, and every entry it licenses as head is licensed as a non-head somewhere.
    fn reversed_order_is_licensed(
        &self,
        g: &Grammar,
        rule: &CompoundingRuleDef,
        sr: &pg_grammar::model::CompoundingSubruleDef,
    ) -> Option<()> {
        // Grammar-wide licence: exact for this rule only while it is the grammar's sole compounding rule.
        let license = crate::emit::compound_license(g)?;
        for (i, entry) in g.entries.iter().enumerate() {
            let id = LexEntryId(i as u32);
            if crate::emit::compound_rule_licenses_non_head(rule, &entry.mpr)
                && !license.head_eligible.contains(&id)
            {
                return None;
            }
            if crate::emit::compound_subrule_licenses_head(g, rule, sr, &entry.mpr)
                && !license.non_head_eligible.contains(&id)
            {
                return None;
            }
        }
        Some(())
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

/// The static licence tests an entry's own MPR set. That equals the stem's set at the compound only when nothing earlier adds MPR features, so a grammar whose compounds restrict by MPR is certified only when no rule outputs MPR features, and a grammar whose compounds do not restrict is unaffected.
fn morphology_leaves_mpr_alone(g: &Grammar) -> bool {
    let restricts = g.mrules.iter().any(|rule| match rule {
        MorphRuleDef::Compounding(def) => {
            !def.head_prod_restrictions_mpr.is_empty()
                || !def.non_head_prod_restrictions_mpr.is_empty()
                || def
                    .subrules
                    .iter()
                    .any(|sr| !sr.required_mpr.is_empty() || !sr.excluded_mpr.is_empty())
        }
        _ => false,
    });
    !restricts
        || g.mrules.iter().all(|rule| match rule {
            MorphRuleDef::Compounding(def) => {
                def.output_prod_restrictions_mpr.is_empty()
                    && def.subrules.iter().all(|sr| sr.out_mpr.is_empty())
            }
            other => other
                .affix_allomorphs()
                .is_none_or(|allos| allos.iter().all(|a| a.out_mpr.is_empty())),
        })
}
