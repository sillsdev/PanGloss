//! Per-cascade feature-bundle closure; private tokens retain material until final spelling.

use std::collections::{HashMap, HashSet};

use super::block::{self, BlockPlan};
use super::Slot;
use crate::replace::{BundleClosureEvidence, PhonologyLoweringError, SegAlphabet};
use crate::admission::meter::Meter;
use foma::{options::FomaOptions, types::Fsm};
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

    /// The joint fixpoint of the cascade plan (section 2.0): rows closed under every rule's rhs updates, every `Modify` pin set and every `InsertContext` row of the grammar, in any order, until nothing new appears.
    ///
    /// Unlike [`IntermediateBundles::build`] no rule uses an `Exact` block plan, so `blocks` stays empty and every rule is closed by its general (widened) updates; and the closure of one source never waits for another, so the result is a superset for any stratum layout. Exceeding the row cap or byte charge is an error naming the first rule. Additive: nothing in `build` reads or calls this.
    pub(crate) fn build_joint(
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
        let width = g.phon_features.len();
        // (rule, update) for every rewrite rule; (pins, alpha lanes) for every `Modify`.
        let mut updates: Vec<(&str, RhsUpdate)> = Vec::new();
        for rule in rules {
            for update in Self::rhs_updates(g, table, rule)? {
                updates.push((rule.xml_id.as_str(), update));
            }
        }
        let (modifies, inserts) = Self::morphology_actions(g, table_id, width, type_lane);
        for (rule, update) in &updates {
            result.evidence.transfer_visits += 1;
            result.intern(update.apply(&update.fresh, type_lane), rule)?;
        }
        for row in inserts {
            result.intern(row, first_rule)?;
        }
        let mut next = 0;
        while next < result.rows.len() {
            let base = result.rows[next].1.clone();
            next += 1;
            for (rule, update) in &updates {
                result.evidence.transfer_visits += 1;
                result.intern(update.apply(&base, type_lane), rule)?;
            }
            for (pins, alpha) in &modifies {
                let mut row = base.clone();
                for &(lane, bits) in pins {
                    row[lane] = bits;
                }
                for &lane in alpha {
                    row[lane] = 0;
                }
                row[type_lane] = TYPE_SEGMENT_BITS;
                result.intern(row, first_rule)?;
            }
        }
        result.evidence.widened_alpha_outputs +=
            updates.iter().map(|(_, update)| update.alpha_lanes.len()).sum::<usize>();
        result.evidence.private_rows = result.rows.len() - result.evidence.attested_rows;
        Ok(result)
    }

    /// Every `Modify` (pins, alpha lanes) and `InsertContext` row (as a full row) the grammar's affix and compounding rules can write.
    fn morphology_actions(
        g: &Grammar,
        table_id: TableId,
        width: usize,
        type_lane: usize,
    ) -> (Vec<(Vec<(usize, u64)>, Vec<usize>)>, Vec<Vec<u64>>) {
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
        (modifies, inserts)
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

    /// A row distinct from every interned row with the same lanes: a private `Modify` token or an EMPTY twin.
    fn push_distinct(&mut self, row: Vec<u64>, rule: &str) -> Result<CharDefId, PhonologyLoweringError> {
        let id = CharDefId(self.next_id);
        self.push(id, row, rule)?;
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

    /// Every retained row with its token id and lanes.
    pub(crate) fn row_table(&self) -> &[(CharDefId, Vec<u64>)] {
        &self.rows
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
    pub(crate) fn spelling(&self, meter: &Meter, opts: &FomaOptions, g: &Grammar) -> Option<Fsm> {
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
            return meter.regex(opts, "[0 - 0]*");
        }
        meter.regex(opts, &format!("[{}]*", pairs.join(" | ")))
    }
}

/// The joint-fixpoint closure of one grammar's rewrite rules, for gates that compare the cascade's class membership and images with the synthesis code (plan section 2.0, KC-0b).
pub struct JointClosure {
    bundles: IntermediateBundles,
    kinds: HashMap<CharDefId, RowKind>,
}

/// What a closure row stands for beyond its lanes (plan 2.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKind {
    /// An attested segment, or a bundle some rule made: spelled by every compatible attested segment.
    Plain,
    /// The image of a `Modify` or `InsertContext` whose class restricts the char-def set to these members: spelled only by the members that unify.
    Private(Vec<CharDefId>),
    /// The EMPTY twin of an alpha-output image: spelled only as nothing.
    EmptyTwin,
}

/// Row counts of a [`JointClosure`] against the token cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JointClosureReport {
    pub attested_rows: usize,
    pub private_rows: usize,
    pub total_rows: usize,
    /// Rows no attested segment can spell, so spelled as nothing.
    pub epsilon_rows: usize,
    /// Private `Modify`/`InsertContext` tokens, and EMPTY twins, among the private rows.
    pub private_modify_rows: usize,
    pub empty_twin_rows: usize,
    /// The token-id cap: 0x1900 ids (6,400), and the row bound.
    pub token_cap: usize,
}

impl JointClosure {
    /// Closes the grammar's segment rows under every rule in `rules`, every `Modify` and every `InsertContext`; `Err` when the row cap is exceeded.
    pub fn build(
        g: &Grammar,
        table: TableId,
        rules: &[&RewriteRuleDef],
    ) -> Result<Self, PhonologyLoweringError> {
        let mut bundles = IntermediateBundles::build_joint(g, table, rules)?;
        let kinds = Self::add_private_rows(g, table, rules, &mut bundles)?;
        Ok(Self { bundles, kinds })
    }

    /// Adds the rows that carry more than lanes: a private token `(lanes, members)` for each image of a `Modify`/`InsertContext` whose class restricts the char-def set (unless every compatible segment is a member, the static check of 2.6), and an EMPTY twin of every image of a rule whose alpha variable is read with one polarity and written with the other.
    fn add_private_rows(
        g: &Grammar,
        table_id: TableId,
        rules: &[&RewriteRuleDef],
        bundles: &mut IntermediateBundles,
    ) -> Result<HashMap<CharDefId, RowKind>, PhonologyLoweringError> {
        let table = &g.char_tables[table_id.0 as usize];
        let first_rule = rules.first().map_or("", |rule| rule.xml_id.as_str());
        let width = g.phon_features.len();
        let type_lane = g.phon_features.type_flat().0 as usize;
        let mut kinds = HashMap::new();
        let plain: Vec<Vec<u64>> = bundles.rows.iter().map(|(_, row)| row.clone()).collect();
        let compatible_segments = |lanes: &[u64]| -> Vec<CharDefId> {
            table
                .iter()
                .filter(|(_, cd)| cd.kind() == CharDefKind::Segment && compatible(lanes, cd.feature_lanes()))
                .map(|(id, _)| id)
                .collect()
        };
        let mut seen: HashSet<(Vec<u64>, Vec<CharDefId>)> = HashSet::new();
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
            let (context, modify) = match action {
                OutputAction::Modify(_, context) => (context, true),
                OutputAction::InsertContext(context) => (context, false),
                _ => continue,
            };
            let class = &g.natural_classes[context.nat_class.0 as usize];
            let all_pins = pg_rules::morph::ctx_pins(g, table_id, context);
            let flat: Vec<_> = all_pins
                .iter()
                .map(|(lane, bits)| (pg_grammar::featsys::FlatIndex(*lane as u32), pg_featstruct::SymbolBits(*bits)))
                .collect();
            let members: Vec<CharDefId> = match pg_grammar::segment::nat_class_cd_set_with_constraints(table, class, &flat) {
                pg_shape::CdSet::Unrestricted => continue,
                pg_shape::CdSet::Members(bits) => table.iter().map(|(id, _)| id).filter(|id| bits.contains(id.0)).collect(),
            };
            if members.is_empty() && matches!(class.kind, NaturalClassKind::Segments(_)) {
                // E4: the pins would read as any-value, so this is refused by name instead of modelled.
                return Err(PhonologyLoweringError::UnsupportedRewrite(format!(
                    "class {} has no eligible member",
                    class.xml_id
                )));
            }
            let pins: Vec<_> = all_pins.into_iter().filter(|(lane, _)| *lane < width).collect();
            let alpha: Vec<usize> = context.vars.iter().map(|var| var.feature.0 as usize).collect();
            let images: Vec<Vec<u64>> = if modify {
                plain
                    .iter()
                    .map(|base| {
                        let mut row = base.clone();
                        for &(lane, bits) in &pins {
                            row[lane] = bits;
                        }
                        for &lane in &alpha {
                            row[lane] = 0;
                        }
                        row[type_lane] = TYPE_SEGMENT_BITS;
                        row
                    })
                    .collect()
            } else {
                let mut row = vec![0; plain.first().map_or(0, Vec::len)];
                for &(lane, bits) in &pins {
                    row[lane] = bits;
                }
                row[type_lane] = TYPE_SEGMENT_BITS;
                vec![row]
            };
            for image in images {
                if compatible_segments(&image).iter().all(|cd| members.contains(cd)) {
                    continue;
                }
                if seen.insert((image.clone(), members.clone())) {
                    let id = bundles.push_distinct(image, first_rule)?;
                    kinds.insert(id, RowKind::Private(members.clone()));
                }
            }
        }
        // EMPTY twins: a rule reads a variable with one polarity and writes it with the other.
        for rule in rules {
            fn vars_of(nodes: &[PatternNode], out: &mut Vec<(pg_grammar::model::VarId, bool)>) {
                for node in nodes {
                    match node {
                        PatternNode::Context(context) => {
                            out.extend(context.vars.iter().map(|v| (v.var, v.plus)))
                        }
                        PatternNode::Quantifier { children, .. } => vars_of(children, out),
                        _ => {}
                    }
                }
            }
            let (mut read, mut written) = (Vec::new(), Vec::new());
            vars_of(&rule.lhs.nodes, &mut read);
            for sub in &rule.subrules {
                vars_of(&sub.rhs.nodes, &mut written);
                for env in [&sub.left_env, &sub.right_env].into_iter().flatten() {
                    vars_of(&env.nodes, &mut read);
                }
            }
            if !written.iter().any(|(var, plus)| read.iter().any(|(v, p)| v == var && p != plus)) {
                continue;
            }
            let updates = IntermediateBundles::rhs_updates(g, table, rule)?;
            let mut images: HashSet<Vec<u64>> = HashSet::new();
            for update in &updates {
                images.insert(update.apply(&update.fresh, type_lane));
                for base in &plain {
                    images.insert(update.apply(base, type_lane));
                }
            }
            let mut images: Vec<Vec<u64>> = images.into_iter().collect();
            images.sort();
            for image in images {
                let id = bundles.push_distinct(image, &rule.xml_id)?;
                kinds.insert(id, RowKind::EmptyTwin);
            }
        }
        Ok(kinds)
    }

    pub fn row_kind(&self, id: CharDefId) -> RowKind {
        self.kinds.get(&id).cloned().unwrap_or(RowKind::Plain)
    }

    /// Every row with its token id, attested rows first.
    pub fn rows(&self) -> &[(CharDefId, Vec<u64>)] {
        &self.bundles.rows
    }

    pub fn attested_rows(&self) -> usize {
        self.bundles.evidence.attested_rows
    }

    /// The tokens a `CharDef` or `Context` node matches across the closure; `None` for any other node.
    pub fn node_members(&self, g: &Grammar, node: &PatternNode) -> Option<Vec<CharDefId>> {
        self.bundles.node_members(g, node)
    }

    /// Every boundary char-def of the owning table.
    pub fn boundary_ids(&self, g: &Grammar) -> Vec<CharDefId> {
        self.bundles.boundary_ids(g)
    }

    /// Whether some row has these lanes, a zero lane in the row standing for any value in `lanes`.
    pub fn covers(&self, lanes: &[u64]) -> bool {
        self.bundles.rows.iter().any(|(_, row)| {
            row.len() == lanes.len() && row.iter().zip(lanes).all(|(r, l)| *r == 0 || r == l)
        })
    }

    /// The concrete lane vectors row `idx` stands for: every zero (any-value) phonological lane takes each single value, each complement and the whole set. `None` above `cap` completions (the token then gets the epsilon spelling, plan 2.6).
    pub fn completions(&self, g: &Grammar, idx: usize, cap: usize) -> Option<Vec<Vec<u64>>> {
        let row = &self.bundles.rows[idx].1;
        let mut out = vec![row.clone()];
        for lane in (0..g.phon_features.len().min(row.len())).filter(|&l| row[l] == 0) {
            let mask = g.phon_features.mask(pg_grammar::featsys::FlatIndex(lane as u32));
            let mut values = vec![mask];
            for bit in 0..64 {
                let one = 1u64 << bit;
                if mask & one != 0 {
                    values.push(one);
                    if mask & !one != 0 {
                        values.push(mask & !one);
                    }
                }
            }
            let mut next = Vec::new();
            for base in &out {
                for value in &values {
                    let mut r = base.clone();
                    r[lane] = *value;
                    next.push(r);
                }
            }
            if next.len() > cap {
                return None;
            }
            out = next;
        }
        Some(out)
    }

    /// The attested segments row `idx` may be spelled as, and whether it may also be spelled as nothing: when no segment is compatible, or when some concrete completion of an any-value lane unifies with no segment, or when there are more than 256 completions (plan 2.6).
    pub fn spellings(&self, g: &Grammar, idx: usize) -> (Vec<CharDefId>, bool) {
        let (id, row) = &self.bundles.rows[idx];
        let table = &g.char_tables[self.bundles.table.0 as usize];
        let kind = self.row_kind(*id);
        if kind == RowKind::EmptyTwin {
            return (Vec::new(), true);
        }
        let mut spelled = self.bundles.row_spellings(g, row, *id);
        if let RowKind::Private(members) = &kind {
            if g.phon_features.is_empty() {
                // A featureless table spells by identity: a private token is spelled by its members.
                spelled = members
                    .iter()
                    .copied()
                    .filter(|cd| table.get(*cd).kind() == CharDefKind::Segment)
                    .collect();
            } else {
                spelled.retain(|cd| members.contains(cd));
            }
        }
        let allowed = |cd: CharDefId| match &kind {
            RowKind::Private(members) => members.contains(&cd),
            _ => true,
        };
        let epsilon = spelled.is_empty()
            || (!g.phon_features.is_empty()
                && match self.completions(g, idx, 256) {
                None => true,
                Some(completions) => completions.iter().any(|c| {
                    !table.iter().any(|(cd_id, cd)| {
                        cd.kind() == CharDefKind::Segment
                            && allowed(cd_id)
                            && c.iter().zip(cd.feature_lanes()).take(g.phon_features.len()).all(|(a, b)| a & b != 0)
                    })
                }),
            });
        (spelled, epsilon)
    }

    pub fn report(&self, g: &Grammar) -> JointClosureReport {
        JointClosureReport {
            attested_rows: self.bundles.evidence.attested_rows,
            private_rows: self.bundles.rows.len() - self.bundles.evidence.attested_rows,
            total_rows: self.bundles.rows.len(),
            epsilon_rows: (0..self.bundles.rows.len()).filter(|&i| self.spellings(g, i).1).count(),
            private_modify_rows: self.kinds.values().filter(|k| matches!(k, RowKind::Private(_))).count(),
            empty_twin_rows: self.kinds.values().filter(|k| **k == RowKind::EmptyTwin).count(),
            token_cap: 0x1900,
        }
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
        let spelling = closure.spelling(&Meter::default(), &FomaOptions::default(), &g).unwrap();
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
