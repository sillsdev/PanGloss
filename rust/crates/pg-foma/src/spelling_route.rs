//! Prepared literal spelling paths and their no-phonology coverage evidence.

use std::collections::HashMap;

use pg_grammar::chardef::{CharDefKind, CharDefTable};
use pg_grammar::model::{
    AffixAllomorphDef, AllomorphId, CompoundingRuleDef, Grammar, LexEntryId, MorphRuleDef,
    OutputAction, PartRef, Pattern, PatternNode, PhonRuleDef, TableId,
};
use pg_health::capability::ProposalRecallGap;
use pg_shape::{CdSet, EffectiveCdSet, NodeKind, Shape, NO_CHAR_DEF};

use crate::emit::Role;
use crate::emit::REP_VARIANT_BYTE_BUDGET;

/// Every compound subrule joins two intact stem blocks using a certified boundary spelling.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CompoundRouteProof {
    /// The number of stems covered by the proof, including the initial root.
    pub stem_bound: usize,
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

pub(crate) enum SurfaceSpelling<'g> {
    Singleton { boundary_spellings: Vec<String> },
    Proved(SpellingRoutePlan<'g>),
}

impl<'g> SurfaceSpelling<'g> {
    pub fn plan(&self) -> Option<&SpellingRoutePlan<'g>> {
        match self {
            Self::Singleton { .. } => None,
            Self::Proved(plan) => Some(plan),
        }
    }

    pub fn compound_boundary_spellings(&self) -> &[String] {
        match self {
            Self::Singleton { boundary_spellings } => boundary_spellings,
            Self::Proved(plan) => plan
                .compound
                .as_ref()
                .map_or(&[], |proof| proof.boundary_spellings.as_slice()),
        }
    }
}

/// Soundness horizon for the recursive spelling and identity proof.
const MAX_PROVED_STEMS: usize = 4;

impl<'g> SpellingRoutePlan<'g> {
    /// Prepares the surface route for both the admission envelope and the emitter.
    pub(crate) fn for_surface(g: &'g Grammar) -> Result<SurfaceSpelling<'g>, ProposalRecallGap> {
        use ProposalRecallGap::*;

        let singleton = crate::emit::eager_route_has_single_segment_spellings(g);
        let compounds = g.mrules.iter().filter_map(|rule| match rule {
            MorphRuleDef::Compounding(rule) => Some(rule),
            _ => None,
        });
        let has_compounds = compounds.clone().next().is_some();
        let stem_bound = if has_compounds {
            crate::emit::effective_compound_stems(g)
        } else {
            1
        };
        if singleton && stem_bound <= 2 {
            if has_compounds {
                for stratum in &g.strata {
                    check_compound_segment_starters(&g.char_tables[stratum.table.0 as usize])?;
                }
            }
            if entries_spell_a_boundary(g) {
                return Err(SingletonRootBoundary);
            }
            if compounds_are_staged(g) && phonology_spans_junction(g) {
                return Err(CompoundJunctionPhonology);
            }
            let writes_junction =
                compounds
                    .clone()
                    .flat_map(|rule| &rule.subrules)
                    .any(|subrule| {
                        subrule.rhs.iter().any(|action| {
                            matches!(action, OutputAction::InsertSegments { shape, .. }
                        if shape.shape.interior().next().is_some())
                        })
                    });
            let mut boundary_spellings = Vec::new();
            if writes_junction {
                // The singleton emitter already probes segment surfaces. Use their common table to certify the literal boundary.
                let table_id = shared_surface_table(g).ok_or(CompoundBoundarySpelling)?;
                let mut plan = Self::empty(&g.char_tables[table_id.0 as usize]);
                let proof = plan.prove_compounds(
                    g,
                    table_id,
                    compounds,
                    stem_bound,
                    CompoundBoundarySpelling,
                )?;
                plan.check_compound_tokenization(&proof)?;
                boundary_spellings = proof.boundary_spellings;
            }
            if has_compounds {
                for stratum in &g.strata {
                    let table = &g.char_tables[stratum.table.0 as usize];
                    if !compound_junction_order_is_nfd(
                        segment_spellings(table)
                            .chain(boundary_spellings.iter().map(String::as_str)),
                    ) {
                        return Err(CompoundJunctionTokenization);
                    }
                }
            }
            return Ok(SurfaceSpelling::Singleton { boundary_spellings });
        }
        Self::prepare(g, stem_bound, compounds).map(SurfaceSpelling::Proved)
    }

    fn prepare(
        g: &'g Grammar,
        stem_bound: usize,
        compounds: impl Iterator<Item = &'g CompoundingRuleDef> + Clone,
    ) -> Result<Self, ProposalRecallGap> {
        use ProposalRecallGap::*;

        if stem_bound > MAX_PROVED_STEMS {
            return Err(RecursiveCompoundDepth);
        }
        let recursive = stem_bound > 2;
        let spelling_gap = if recursive {
            RecursiveCompoundSpelling
        } else {
            AmbiguousSegmentSpelling
        };
        if g.strata.iter().any(|stratum| !stratum.prules.is_empty()) {
            return Err(spelling_gap);
        }
        let table_id = shared_surface_table(g).ok_or(spelling_gap)?;
        let restricts_mpr = compounds_restrict_mpr(g);
        let outputs_mpr = (recursive || restricts_mpr) && morphology_outputs_mpr(g);
        if recursive {
            if !subrules_license_both_positions(g) {
                return Err(RecursiveCompoundAsymmetricLicense);
            }
            if restricts_mpr {
                return Err(RecursiveCompoundMprRestriction);
            }
            if outputs_mpr {
                return Err(RecursiveCompoundMprOutput);
            }
            if !g.templates.is_empty() {
                return Err(RecursiveCompoundAffixPlacement);
            }
        } else if restricts_mpr && outputs_mpr {
            return Err(spelling_gap);
        }
        let mut plan = Self::empty(&g.char_tables[table_id.0 as usize]);
        if compounds.clone().next().is_some() {
            check_compound_segment_starters(plan.table)?;
        }
        for entry in &g.entries {
            for allo in &entry.allomorphs {
                if allo.is_pattern
                    || allo.shape.shape.interior().any(|(_, kind, cd, flags)| {
                        kind == NodeKind::Segment && (cd == NO_CHAR_DEF || flags.is_iterative())
                    })
                {
                    return Err(spelling_gap);
                }
                let variants = plan
                    .shape_variants(&allo.shape.shape, true)
                    .ok_or(spelling_gap)?;
                plan.retain(&variants).ok_or(spelling_gap)?;
                plan.roots.insert(allo.id, variants);
            }
        }
        for (i, rule) in g.mrules.iter().enumerate() {
            if matches!(rule, MorphRuleDef::Compounding(_)) {
                continue;
            }
            if recursive
                && !crate::emit::standalone_rule_zones(g, pg_grammar::model::MRuleId(i as u32)).0
            {
                return Err(RecursiveCompoundAffixPlacement);
            }
            for allo in rule.affix_allomorphs().ok_or(spelling_gap)? {
                let role = crate::emit::classify_affix(&allo.rhs);
                if recursive && (role != Role::Prefix || !prefix_copies_whole_host(allo)) {
                    return Err(RecursiveCompoundAffixPlacement);
                }
                if matches!(
                    role,
                    Role::Infix | Role::CircumfixPrefix | Role::CircumfixSuffix | Role::Process
                ) {
                    return Err(spelling_gap);
                }
                plan.check_actions(table_id, &allo.rhs)
                    .ok_or(spelling_gap)?;
                let mut variants = vec![String::new()];
                for action in &allo.rhs {
                    if let OutputAction::InsertSegments { shape, .. } = action {
                        let insert = plan
                            .shape_variants(&shape.shape, true)
                            .ok_or(spelling_gap)?;
                        variants = product(&variants, &insert).ok_or(spelling_gap)?;
                    }
                }
                plan.retain(&variants).ok_or(spelling_gap)?;
                plan.affixes.insert(allo.id, variants);
            }
        }
        if compounds.clone().next().is_some() {
            let compound =
                plan.prove_compounds(g, table_id, compounds, stem_bound, spelling_gap)?;
            plan.check_compound_tokenization(&compound)?;
            plan.compound = Some(compound);
        }
        Ok(plan)
    }

    fn empty(table: &'g CharDefTable) -> Self {
        Self {
            table,
            roots: HashMap::new(),
            affixes: HashMap::new(),
            compound: None,
            bytes: 0,
        }
    }

    fn prove_compounds(
        &mut self,
        g: &Grammar,
        table_id: TableId,
        compounds: impl Iterator<Item = &'g CompoundingRuleDef>,
        stem_bound: usize,
        gap: ProposalRecallGap,
    ) -> Result<CompoundRouteProof, ProposalRecallGap> {
        let mut proof = CompoundRouteProof {
            stem_bound: 1,
            ..Default::default()
        };
        for rule in compounds {
            self.prove_compound(g, table_id, rule, stem_bound > 2, &mut proof)
                .ok_or(gap)?;
        }
        if stem_bound > 2 {
            proof.prove_levels(g, stem_bound)?;
        } else {
            proof.stem_bound = stem_bound;
        }
        proof.boundary_spellings.sort_unstable();
        proof.boundary_spellings.dedup();
        Ok(proof)
    }

    /// Each nonempty piece starts a new foma token and preserves the NFD of the preceding piece.
    fn check_compound_tokenization(
        &self,
        proof: &CompoundRouteProof,
    ) -> Result<(), ProposalRecallGap> {
        let symbols = crate::emit::combining_run_symbols(self.table);
        let mut pieces = self
            .roots
            .values()
            .chain(self.affixes.values())
            .flatten()
            .chain(&proof.boundary_spellings);
        if !compound_junction_order_is_nfd(pieces.clone().map(String::as_str))
            || pieces.any(|text| {
                text.starts_with(crate::emit::char_is_combining)
                    || !pg_grammar::nfd::is_nfd(text)
                    || text.contains(['<', '@'])
                    || crate::emit::combining_runs(text).any(|run| {
                        run.starts_with('0')
                            || !symbols.contains(&crate::emit::escape_lexc_text(run))
                    })
            })
        {
            return Err(ProposalRecallGap::CompoundJunctionTokenization);
        }
        Ok(())
    }

    /// Proves one compounding rule's outputs are the head parts, at most one boundary insertion, then the non-head parts (or the reverse order), or `None`. The emitter writes the boundary spellings only between the two stems, so an insertion anywhere else, or two insertions, must refuse.
    fn prove_compound(
        &mut self,
        g: &Grammar,
        table: TableId,
        rule: &CompoundingRuleDef,
        recursive: bool,
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
                        let mut variants = self.shape_variants(&shape.shape, true)?;
                        variants.retain(|v| !v.is_empty());
                        self.retain(&variants)?;
                        proof.boundary_spellings.extend(variants);
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
                if !recursive {
                    self.reversed_order_is_licensed(g, rule, sr)?;
                }
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

fn compound_junction_order_is_nfd<'a>(pieces: impl Iterator<Item = &'a str>) -> bool {
    use pg_grammar::nfd::canonical_combining_class;

    // Across NFD pieces, reordering can occur only when a trailing class exceeds a nonzero leading class.
    let mut highest_trailing_class = 0;
    let mut lowest_leading_mark_class = u8::MAX;
    for text in pieces {
        if let Some(start) = text.chars().next() {
            let leading_class = canonical_combining_class(start);
            if leading_class != 0 {
                lowest_leading_mark_class = lowest_leading_mark_class.min(leading_class);
            }
            let end = text.chars().next_back().unwrap();
            highest_trailing_class = highest_trailing_class.max(canonical_combining_class(end));
        }
    }
    highest_trailing_class <= lowest_leading_mark_class
}

/// Root spellings on the singleton route drop boundary nodes, so a boundary typed inside a root is never proposed.
fn entries_spell_a_boundary(g: &Grammar) -> bool {
    g.entries.iter().any(|entry| {
        entry.allomorphs.iter().any(|allo| {
            allo.shape
                .shape
                .interior()
                .any(|(_, kind, _, _)| kind == NodeKind::Boundary)
        })
    })
}

/// Only a compounding rule some stratum applies can form a compound.
fn compounds_are_staged(g: &Grammar) -> bool {
    g.strata
        .iter()
        .flat_map(|stratum| &stratum.mrules)
        .any(|id| matches!(g.mrules[id.0 as usize], MorphRuleDef::Compounding(_)))
}

/// A stratum rule whose target or environment can span two stems can rewrite across the compound junction, which the singleton route does not certify.
pub(crate) fn phonology_spans_junction(g: &Grammar) -> bool {
    g.strata
        .iter()
        .flat_map(|stratum| &stratum.prules)
        .any(|id| rule_spans_junction(&g.prules[id.0 as usize]))
}

/// A rewrite stays inside one stem only when it reads and writes one node and matches no neighbour; metathesis always spans two.
pub(crate) fn rule_spans_junction(rule: &PhonRuleDef) -> bool {
    match rule {
        PhonRuleDef::Rewrite(rule) => rule.subrules.iter().any(|sub| {
            let matched = pattern_width(&rule.lhs)
                .saturating_add(sub.left_env.as_ref().map_or(0, pattern_width))
                .saturating_add(sub.right_env.as_ref().map_or(0, pattern_width));
            pattern_width(&rule.lhs) != 1 || pattern_width(&sub.rhs) != 1 || matched != 1
        }),
        PhonRuleDef::Metathesis(_) => true,
    }
}

/// Nodes a pattern consumes; anchors consume none, and anything of unbounded or unknown width counts as too wide.
fn pattern_width(pattern: &Pattern) -> usize {
    pattern
        .nodes
        .iter()
        .map(|node| match node {
            PatternNode::Context(_) | PatternNode::CharDef(_) => 1,
            PatternNode::Anchor(_) => 0,
            PatternNode::Segments { shape, .. } => shape.shape.interior().count(),
            PatternNode::Quantifier { .. } => usize::MAX / 4,
        })
        .fold(0, usize::saturating_add)
}

fn shared_surface_table(g: &Grammar) -> Option<TableId> {
    let table = g.strata.first()?.table;
    g.strata
        .iter()
        .all(|stratum| stratum.table == table)
        .then_some(table)
}

fn segment_spellings(table: &CharDefTable) -> impl Iterator<Item = &str> {
    table
        .iter()
        .filter(|(_, def)| def.kind() == CharDefKind::Segment)
        .flat_map(|(_, def)| def.representations_nfd())
        .map(String::as_str)
}

fn check_compound_segment_starters(table: &CharDefTable) -> Result<(), ProposalRecallGap> {
    if segment_spellings(table).any(|text| text.starts_with(crate::emit::char_is_combining)) {
        return Err(ProposalRecallGap::CompoundJunctionTokenization);
    }
    Ok(())
}

impl CompoundRouteProof {
    /// Right growth keeps the first root in place, while left growth moves it to an extra slot.
    fn prove_levels(&mut self, g: &Grammar, bound: usize) -> Result<(), ProposalRecallGap> {
        let license = crate::emit::compound_license(g).expect("compound proof needs a license");
        let mut first_roots = license.head_eligible.clone();
        for stems in 2..=bound {
            if !first_roots.is_subset(&license.non_head_eligible)
                || !license.non_head_eligible.is_subset(&license.head_eligible)
            {
                return Err(ProposalRecallGap::RecursiveCompoundAsymmetricLicense);
            }
            first_roots.extend(license.non_head_eligible.iter().copied());
            self.stem_bound = stems;
        }
        Ok(())
    }
}

fn subrules_license_both_positions(g: &Grammar) -> bool {
    g.mrules.iter().all(|rule| {
        let MorphRuleDef::Compounding(rule) = rule else {
            return true;
        };
        rule.subrules.iter().all(|sr| {
            g.entries.iter().all(|entry| {
                crate::emit::compound_subrule_licenses_head(g, rule, sr, &entry.mpr)
                    == crate::emit::compound_rule_licenses_non_head(rule, &entry.mpr)
            })
        })
    })
}

/// The affix can move inside a compound only as a prefix on an intact stem.
fn prefix_copies_whole_host(allo: &AffixAllomorphDef) -> bool {
    allo.rhs
        .iter()
        .filter_map(|action| match action {
            OutputAction::Copy(part) => Some(*part),
            _ => None,
        })
        .eq((0..allo.lhs.len() as u16).map(PartRef::Input))
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

fn compounds_restrict_mpr(g: &Grammar) -> bool {
    g.mrules.iter().any(|rule| match rule {
        MorphRuleDef::Compounding(def) => {
            !def.head_prod_restrictions_mpr.is_empty()
                || !def.non_head_prod_restrictions_mpr.is_empty()
                || def
                    .subrules
                    .iter()
                    .any(|sr| !sr.required_mpr.is_empty() || !sr.excluded_mpr.is_empty())
        }
        _ => false,
    })
}

fn morphology_outputs_mpr(g: &Grammar) -> bool {
    g.mrules.iter().any(|rule| match rule {
        MorphRuleDef::Compounding(def) => {
            !def.output_prod_restrictions_mpr.is_empty()
                || def.subrules.iter().any(|sr| !sr.out_mpr.is_empty())
        }
        other => other
            .affix_allomorphs()
            .is_some_and(|allos| allos.iter().any(|a| !a.out_mpr.is_empty())),
    })
}
