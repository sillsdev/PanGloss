//! Per-cascade feature-bundle closure; private tokens retain material until final spelling.

use std::collections::{HashMap, HashSet};

use super::block::{self, BlockPlan};
use super::Slot;
use crate::replace::{BundleClosureEvidence, PhonologyLoweringError, SegAlphabet};
use foma::{options::FomaOptions, regex::fsm_parse_regex, types::Fsm};
use pg_grammar::{
    chardef::{CharDefId, CharDefKind, CharDefTable},
    featsys::TYPE_SEGMENT_BITS,
    model::{
        Grammar, MorphRuleDef, NaturalClassKind, OutputAction, Pattern, PatternNode, RewriteRuleDef,
        TableId,
    },
};

/// Resource containment, not a representability or production-readiness verdict.
const MAX_BUNDLES: usize = 65536;
const MAX_RETAINED_CHARGE: usize = 100 * 1024 * 1024;

pub(crate) struct IntermediateBundles {
    pub(super) table: TableId,
    pub(super) rows: Vec<(CharDefId, Vec<u64>)>,
    interned: HashMap<Vec<u64>, CharDefId>,
    next_id: u32,
    charge: usize,
    /// How each block rule's relation is built, by rule id.
    pub(crate) blocks: HashMap<String, BlockPlan>,
    pub(crate) evidence: BundleClosureEvidence,
}

/// One RHS node's effect on a row: authored pins, then any-value alpha lanes.
struct RhsUpdate {
    pins: Vec<(usize, u64)>,
    alpha_lanes: Vec<usize>,
    fresh: Vec<u64>,
}

impl RhsUpdate {
    fn apply(&self, base: &[u64], type_lane: usize) -> Vec<u64> {
        let mut row = base.to_vec();
        for &(lane, bits) in &self.pins {
            if lane < row.len() {
                row[lane] = bits;
            }
        }
        row[type_lane] = TYPE_SEGMENT_BITS;
        // A zero lane is unconstrained in `compatible`: one row covers every binding, negation and absent value.
        for &lane in &self.alpha_lanes {
            row[lane] = 0;
        }
        row
    }
}

impl IntermediateBundles {
    fn empty(table: TableId, next_id: u32) -> Self {
        Self {
            table,
            rows: Vec::new(),
            interned: HashMap::new(),
            next_id,
            charge: 0,
            blocks: HashMap::new(),
            evidence: BundleClosureEvidence::default(),
        }
    }

    pub(crate) fn build(
        g: &Grammar,
        table_id: TableId,
        rules: &[&RewriteRuleDef],
    ) -> Result<Self, PhonologyLoweringError> {
        let table = &g.char_tables[table_id.0 as usize];
        let first_rule = rules.first().map_or("", |rule| rule.xml_id.as_str());
        if table.len() > 0x1900 {
            return Err(Self::unavailable(first_rule));
        }
        let mut result = Self::empty(table_id, table.len() as u32);
        for (id, definition) in table.iter() {
            if definition.kind() == CharDefKind::Segment {
                let lanes = definition.feature_lanes().to_vec();
                result.interned.entry(lanes.clone()).or_insert(id);
                result.push(id, lanes, first_rule)?;
            }
        }
        result.evidence.attested_rows = result.rows.len();
        let type_lane = g.phon_features.type_flat().0 as usize;
        result.seed_morphology(g, table_id, first_rule)?;
        for rule in rules {
            if block::is_block_rule(g, rule) || block::is_pair_rule(g, rule) {
                let plan = block::close_rule(&mut result, g, table_id, rule)?;
                let exact = matches!(plan, BlockPlan::Exact { .. });
                result.blocks.insert(rule.xml_id.clone(), plan);
                if exact {
                    continue;
                }
            }
            let updates = Self::rhs_updates(g, table, rule)?;
            // Expansion and epenthesis create a node instead of updating a target.
            for update in &updates {
                result.evidence.transfer_visits += 1;
                result.intern(update.apply(&update.fresh, type_lane), &rule.xml_id)?;
            }
            // Rows this rule appends are visited too, covering repeated site/subrule applications.
            let mut next = 0;
            while next < result.rows.len() {
                let base = result.rows[next].1.clone();
                next += 1;
                for update in &updates {
                    result.evidence.transfer_visits += 1;
                    result.intern(update.apply(&base, type_lane), &rule.xml_id)?;
                }
            }
            result.evidence.widened_alpha_outputs +=
                updates.iter().map(|update| update.alpha_lanes.len()).sum::<usize>();
        }
        result.evidence.private_rows = result.rows.len() - result.evidence.attested_rows;
        Ok(result)
    }

    /// Rows an affix `Modify` or `InsertContext` can create before any phonology runs, closed under repeated modification.
    fn seed_morphology(
        &mut self,
        g: &Grammar,
        table_id: TableId,
        rule: &str,
    ) -> Result<(), PhonologyLoweringError> {
        let type_lane = g.phon_features.type_flat().0 as usize;
        let width = g.phon_features.len();
        let (mut modifies, mut inserts) = (Vec::new(), Vec::new());
        let affix_actions = g
            .mrules
            .iter()
            .filter_map(|rule| rule.affix_allomorphs())
            .flatten()
            .flat_map(|allomorph| &allomorph.rhs);
        let compound_actions = g
            .mrules
            .iter()
            .filter_map(|rule| match rule {
                MorphRuleDef::Compounding(rule) => Some(&rule.subrules),
                _ => None,
            })
            .flatten()
            .flat_map(|subrule| &subrule.rhs);
        for action in affix_actions.chain(compound_actions) {
            let context = match action {
                OutputAction::Modify(_, context) | OutputAction::InsertContext(context) => context,
                _ => continue,
            };
            let pins: Vec<_> = pg_rules::morph::ctx_pins(g, table_id, context)
                .into_iter()
                .filter(|(lane, _)| *lane < width)
                .collect();
            let alpha: Vec<usize> = context.vars.iter().map(|var| var.feature.0 as usize).collect();
            if matches!(action, OutputAction::Modify(..)) {
                modifies.push((pins, alpha));
            } else {
                let mut row = vec![0; width];
                for (lane, bits) in pins {
                    row[lane] = bits;
                }
                row[type_lane] = TYPE_SEGMENT_BITS;
                inserts.push(row);
            }
        }
        for row in inserts {
            self.intern(row, rule)?;
        }
        let mut next = 0;
        while next < self.rows.len() && !modifies.is_empty() {
            let base = self.rows[next].1.clone();
            next += 1;
            for (pins, alpha) in &modifies {
                let mut row = base.clone();
                for &(lane, bits) in pins {
                    row[lane] = bits;
                }
                for &lane in alpha {
                    row[lane] = 0;
                }
                row[type_lane] = TYPE_SEGMENT_BITS;
                self.intern(row, rule)?;
            }
        }
        self.evidence.morphology_rows = self.rows.len() - self.evidence.attested_rows;
        Ok(())
    }

    /// The row's token id, retaining it as a new private row when no row has its lanes.
    pub(super) fn intern(
        &mut self,
        row: Vec<u64>,
        rule: &str,
    ) -> Result<CharDefId, PhonologyLoweringError> {
        if let Some(id) = self.interned.get(&row) {
            return Ok(*id);
        }
        let id = CharDefId(self.next_id);
        self.push(id, row.clone(), rule)?;
        self.interned.insert(row, id);
        self.next_id += 1;
        Ok(id)
    }

    fn rhs_updates(
        g: &Grammar,
        table: &CharDefTable,
        rule: &RewriteRuleDef,
    ) -> Result<Vec<RhsUpdate>, PhonologyLoweringError> {
        let mut updates = Vec::new();
        for subrule in &rule.subrules {
            for node in &subrule.rhs.nodes {
                let alpha_lanes = match node {
                    PatternNode::CharDef(id) if table.get(*id).kind() == CharDefKind::Boundary => {
                        continue
                    }
                    PatternNode::CharDef(_) => Vec::new(),
                    PatternNode::Context(context) => context
                        .vars
                        .iter()
                        .map(|var| var.feature.0 as usize)
                        .collect(),
                    _ => {
                        return Err(PhonologyLoweringError::UnsupportedRewrite(
                            rule.xml_id.clone(),
                        ))
                    }
                };
                updates.push(RhsUpdate {
                    pins: pg_rules::rewrite::node_pins(g, table, node),
                    alpha_lanes,
                    fresh: pg_rules::rewrite::node_full_lanes(g, table, node)
                        [..g.phon_features.len()]
                        .to_vec(),
                });
            }
        }
        Ok(updates)
    }

    fn row_charge(width: usize) -> usize {
        4 * (width * size_of::<u64>() + size_of::<(CharDefId, Vec<u64>)>())
    }

    fn unavailable(rule: &str) -> PhonologyLoweringError {
        PhonologyLoweringError::IntermediateBundleClosureUnavailable(rule.to_owned())
    }

    fn push(
        &mut self,
        id: CharDefId,
        row: Vec<u64>,
        rule: &str,
    ) -> Result<(), PhonologyLoweringError> {
        // Four times lane storage plus headers covers the closure and its transient work rows.
        let charge = Self::row_charge(row.len());
        if self.rows.len() >= MAX_BUNDLES
            || self.charge.saturating_add(charge) > MAX_RETAINED_CHARGE
            || id.0 >= 0x1900
        {
            return Err(Self::unavailable(rule));
        }
        self.charge += charge;
        self.rows.push((id, row));
        Ok(())
    }

    /// Shared pin extraction supplies authored unions; alpha lanes widen independently.
    pub(crate) fn slots(&self, g: &Grammar, pattern: &Pattern) -> Option<Vec<Slot>> {
        let table = &g.char_tables[self.table.0 as usize];
        let members = |node: &PatternNode, source: &CharDefTable| {
            self.member_rows(g, source, node, self.rows.len())
                .into_iter()
                .map(|row| self.rows[row].0)
                .collect()
        };
        let mut slots = Vec::new();
        for node in &pattern.nodes {
            match node {
                PatternNode::CharDef(id) if table.get(*id).kind() == CharDefKind::Boundary => slots
                    .push(Slot::Union(
                        table
                            .iter()
                            .filter(|(_, cd)| cd.kind() == CharDefKind::Boundary)
                            .map(|(id, _)| id)
                            .collect(),
                    )),
                PatternNode::CharDef(_) | PatternNode::Context(_) => {
                    slots.push(Slot::Union(members(node, table)))
                }
                PatternNode::Anchor(_) => slots.push(Slot::Anchor),
                PatternNode::Quantifier { min, max, children } => {
                    if max.is_some_and(|max| *min > max) || children.is_empty() {
                        return None;
                    }
                    slots.push(Slot::Repeat {
                        min: *min,
                        max: *max,
                        children: self.slots(
                            g,
                            &Pattern {
                                nodes: children.clone(),
                            },
                        )?,
                    });
                }
                PatternNode::Segments {
                    table: source,
                    shape,
                } => {
                    let source = &g.char_tables[source.0 as usize];
                    for (_, _, cd, _) in shape.shape.interior() {
                        let cd = CharDefId(cd);
                        slots.push(if source.get(cd).kind() == CharDefKind::Boundary {
                            Slot::Union(
                                table
                                    .iter()
                                    .filter(|(_, cd)| cd.kind() == CharDefKind::Boundary)
                                    .map(|(id, _)| id)
                                    .collect(),
                            )
                        } else {
                            Slot::Union(members(&PatternNode::CharDef(cd), source))
                        });
                    }
                }
            }
        }
        Some(slots)
    }

    /// Positions among the first `limit` rows of those compatible with `node`, alpha lanes unconstrained.
    pub(super) fn member_rows(
        &self,
        g: &Grammar,
        source: &CharDefTable,
        node: &PatternNode,
        limit: usize,
    ) -> Vec<usize> {
        if let Some(rows) = self.spelled_rows(g, source, node, limit) {
            return rows;
        }
        let mut lanes = pg_rules::rewrite::node_full_lanes(g, source, node);
        if let PatternNode::Context(context) = node {
            for var in &context.vars {
                lanes[var.feature.0 as usize] = g.phon_features.mask(var.feature);
            }
        }
        (0..limit)
            .filter(|&row| compatible(&self.rows[row].1, &lanes[..self.rows[row].1.len()]))
            .collect()
    }

    /// Without features synthesis matches a segment by its spelling, so a literal or segment-list node stands for the rows sharing a spelling with it; `None` elsewhere.
    fn spelled_rows(
        &self,
        g: &Grammar,
        source: &CharDefTable,
        node: &PatternNode,
        limit: usize,
    ) -> Option<Vec<usize>> {
        if !g.phon_features.is_empty() {
            return None;
        }
        let members: &[CharDefId] = match node {
            PatternNode::CharDef(id) => std::slice::from_ref(id),
            PatternNode::Context(context) => {
                match &g.natural_classes[context.nat_class.0 as usize].kind {
                    NaturalClassKind::Segments(members) => members,
                    NaturalClassKind::Feature(_) => return None,
                }
            }
            _ => return None,
        };
        let spellings: HashSet<&str> = members
            .iter()
            .flat_map(|id| source.get(*id).representations_nfd())
            .map(String::as_str)
            .collect();
        let table = &g.char_tables[self.table.0 as usize];
        Some(
            (0..limit)
                .filter(|&row| {
                    let id = self.rows[row].0;
                    (id.0 as usize) < table.len()
                        && table
                            .get(id)
                            .representations_nfd()
                            .iter()
                            .any(|spelling| spellings.contains(spelling.as_str()))
                })
                .collect(),
        )
    }

    /// Every boundary char-def of the owning table.
    pub(crate) fn boundary_ids(&self, g: &Grammar) -> Vec<CharDefId> {
        g.char_tables[self.table.0 as usize]
            .iter()
            .filter(|(_, cd)| cd.kind() == CharDefKind::Boundary)
            .map(|(id, _)| id)
            .collect()
    }

    /// The tokens a focus or replacement node stands for across the retained rows; `None` for any other node kind.
    pub(crate) fn node_members(&self, g: &Grammar, node: &PatternNode) -> Option<Vec<CharDefId>> {
        let table = &g.char_tables[self.table.0 as usize];
        match node {
            PatternNode::CharDef(id) if table.get(*id).kind() == CharDefKind::Boundary => {
                Some(self.boundary_ids(g))
            }
            PatternNode::CharDef(_) | PatternNode::Context(_) => Some(
                self.member_rows(g, table, node, self.rows.len())
                    .into_iter()
                    .map(|row| self.rows[row].0)
                    .collect(),
            ),
            _ => None,
        }
    }

    /// Focus tuples tried for a block rule, and whether it fell back to the any-value widening.
    pub(crate) fn block_effort(&self, rule: &str) -> (usize, bool) {
        match self.blocks.get(rule) {
            Some(BlockPlan::Exact { tuples, .. }) => (*tuples, false),
            Some(BlockPlan::Widened) => (0, true),
            None => (0, false),
        }
    }

    /// The attested segment ids, then the private ids.
    pub(crate) fn row_ids(&self) -> (Vec<CharDefId>, Vec<CharDefId>) {
        let attested = self.evidence.attested_rows;
        let ids = |rows: &[(CharDefId, Vec<u64>)]| rows.iter().map(|(id, _)| *id).collect();
        (ids(&self.rows[..attested]), ids(&self.rows[attested..]))
    }

    /// Attested segments a row may be spelled as.
    fn row_spellings(&self, g: &Grammar, row: &[u64], id: CharDefId) -> Vec<CharDefId> {
        let table = &g.char_tables[self.table.0 as usize];
        table
            .iter()
            .filter(|(output, definition)| {
                definition.kind() == CharDefKind::Segment
                    && (if g.phon_features.is_empty() {
                        id == *output
                    } else {
                        compatible(row, definition.feature_lanes())
                    })
            })
            .map(|(output, _)| output)
            .collect()
    }

    /// Rows that spell as nothing because no attested segment is compatible.
    pub(crate) fn unspelled_rows(&self, g: &Grammar) -> usize {
        self.rows
            .iter()
            .filter(|(id, row)| self.row_spellings(g, row, *id).is_empty())
            .count()
    }

    /// Spelling is postponed until all rules have seen the actual intermediate bundles.
    pub(crate) fn spelling(&self, opts: &FomaOptions, g: &Grammar) -> Option<Fsm> {
        let table = &g.char_tables[self.table.0 as usize];
        let alphabet = SegAlphabet::new(table);
        let mut pairs = Vec::new();
        for (id, row) in &self.rows {
            let spellings = self.row_spellings(g, row, *id);
            for output in &spellings {
                pairs.push(format!(
                    "{}:{}",
                    alphabet.token(*id),
                    alphabet.token(*output)
                ));
            }
            // A row no character matches spells as nothing, as in the final surface test.
            if spellings.is_empty() {
                pairs.push(format!("{}:0", alphabet.token(*id)));
            }
        }
        for (id, definition) in table.iter() {
            if definition.kind() == CharDefKind::Boundary {
                pairs.push(format!(
                    "[{} | {}:0]",
                    alphabet.token(id),
                    alphabet.token(id)
                ));
            }
        }
        if pairs.is_empty() {
            return fsm_parse_regex(opts, "[0 - 0]*", None, None);
        }
        fsm_parse_regex(opts, &format!("[{}]*", pairs.join(" | ")), None, None)
    }
}

// Zero-bit alpha intermediates are widened, since no concrete matching claim is made for them.
fn compatible(left: &[u64], right: &[u64]) -> bool {
    left.iter()
        .zip(right)
        .all(|(a, b)| *a == 0 || *b == 0 || a & b != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closure_retains_unattested_features_and_uses_private_tokens() {
        let g = pg_conformance_fixtures::require_fixture(
            "edge-cases",
            "unattested-intermediate-bundle",
        )
        .load_grammar();
        let rules: Vec<_> = g
            .prules
            .iter()
            .map(|rule| match rule {
                pg_grammar::model::PhonRuleDef::Rewrite(rule) => rule,
                _ => panic!("rewrite"),
            })
            .collect();
        let closure = IntermediateBundles::build(&g, TableId(0), &rules).unwrap();
        let table = &g.char_tables[0];
        let mut wanted = table
            .get(table.lookup_nfd("a").unwrap())
            .feature_lanes()
            .to_vec();
        let pins = pg_rules::rewrite::node_pins(&g, table, &rules[0].subrules[0].rhs.nodes[0]);
        for (lane, value) in pins {
            if lane < wanted.len() {
                wanted[lane] = value;
            }
        }
        let id = closure
            .rows
            .iter()
            .find(|(_, row)| *row == wanted)
            .expect("retained intermediate")
            .0;
        assert!(id.0 >= table.len() as u32);
        assert!(!table.iter().any(|(_, cd)| cd.feature_lanes() == wanted));
        let next_target = closure.slots(&g, &rules[1].lhs).unwrap();
        assert!(matches!(&next_target[0], Slot::Union(members) if members.contains(&id)));
        let spelling = closure.spelling(&FomaOptions::default(), &g).unwrap();
        let input = SegAlphabet::new(table).token(id).to_string();
        let spelled: Vec<String> = foma::apply::apply_init(&spelling).down(&input).collect();
        assert_eq!(
            spelled,
            vec![String::new()],
            "a row no character matches spells as nothing"
        );
    }

    #[test]
    fn fixed_closure_limits_refuse_before_accepting_another_bundle() {
        let mut closure = IntermediateBundles::empty(TableId(0), 0);
        closure.rows = vec![(CharDefId(0), vec![0]); MAX_BUNDLES];
        assert_eq!(
            closure.push(CharDefId(0), vec![42], "bound"),
            Err(PhonologyLoweringError::IntermediateBundleClosureUnavailable("bound".into()))
        );
        assert_eq!(closure.rows.len(), MAX_BUNDLES);
        closure.rows.clear();
        assert_eq!(
            closure.push(CharDefId(0x1900), vec![42], "tokens"),
            Err(PhonologyLoweringError::IntermediateBundleClosureUnavailable("tokens".into()))
        );
        assert!(closure.rows.is_empty());
        let mut bytes = IntermediateBundles::empty(TableId(0), 0);
        bytes.charge = MAX_RETAINED_CHARGE - IntermediateBundles::row_charge(1);
        bytes.push(CharDefId(0), vec![1], "bytes").unwrap();
        assert_eq!(bytes.charge, MAX_RETAINED_CHARGE);
        assert_eq!(
            bytes.push(CharDefId(1), vec![1], "bytes"),
            Err(PhonologyLoweringError::IntermediateBundleClosureUnavailable("bytes".into()))
        );
        assert_eq!(bytes.rows.len(), 1);
        assert_eq!(bytes.charge, MAX_RETAINED_CHARGE);
    }
}
