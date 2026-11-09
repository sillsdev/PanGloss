//! Per-cascade feature-bundle closure; private tokens retain material until final spelling.

use std::collections::HashSet;

use super::Slot;
use crate::replace::{BundleClosureEvidence, PhonologyLoweringError, SegAlphabet};
use foma::{options::FomaOptions, regex::fsm_parse_regex, types::Fsm};
use pg_grammar::{
    chardef::{CharDefId, CharDefKind, CharDefTable},
    featsys::TYPE_SEGMENT_BITS,
    model::{Grammar, Pattern, PatternNode, RewriteRuleDef, TableId},
};

/// Resource containment, not a representability or production-readiness verdict.
const MAX_BUNDLES: usize = 65536;
const MAX_RETAINED_CHARGE: usize = 100 * 1024 * 1024;

pub(crate) struct IntermediateBundles {
    table: TableId,
    rows: Vec<(CharDefId, Vec<u64>)>,
    next_id: u32,
    charge: usize,
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
    pub(crate) fn build(
        g: &Grammar,
        table_id: TableId,
        rules: &[&RewriteRuleDef],
    ) -> Result<Self, PhonologyLoweringError> {
        let table = &g.char_tables[table_id.0 as usize];
        if table.len() > 0x1900 {
            return Err(Self::unavailable(&rules[0].xml_id));
        }
        let mut result = Self {
            table: table_id,
            rows: Vec::new(),
            next_id: table.len() as u32,
            charge: 0,
            evidence: BundleClosureEvidence::default(),
        };
        let first_rule = &rules[0].xml_id;
        let mut interned = HashSet::new();
        for (id, definition) in table.iter() {
            if definition.kind() == CharDefKind::Segment {
                interned.insert(definition.feature_lanes().to_vec());
                result.push(id, definition.feature_lanes().to_vec(), first_rule)?;
            }
        }
        result.evidence.attested_rows = result.rows.len();
        let type_lane = g.phon_features.type_flat().0 as usize;
        for rule in rules {
            let updates = Self::rhs_updates(g, table, rule)?;
            let mut retain = |result: &mut Self, row: Vec<u64>| {
                if interned.insert(row.clone()) {
                    let id = CharDefId(result.next_id);
                    result.push(id, row, &rule.xml_id)?;
                    result.next_id += 1;
                }
                Ok(())
            };
            // Expansion and epenthesis create a node instead of updating a target.
            for update in &updates {
                result.evidence.transfer_visits += 1;
                retain(&mut result, update.apply(&update.fresh, type_lane))?;
            }
            // Rows this rule appends are visited too, covering repeated site/subrule applications.
            let mut next = 0;
            while next < result.rows.len() {
                let base = result.rows[next].1.clone();
                next += 1;
                for update in &updates {
                    result.evidence.transfer_visits += 1;
                    retain(&mut result, update.apply(&base, type_lane))?;
                }
            }
            result.evidence.widened_alpha_outputs +=
                updates.iter().map(|update| update.alpha_lanes.len()).sum::<usize>();
        }
        result.evidence.private_rows = result.rows.len() - result.evidence.attested_rows;
        Ok(result)
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
            let mut lanes = pg_rules::rewrite::node_full_lanes(g, source, node);
            if let PatternNode::Context(context) = node {
                for var in &context.vars {
                    lanes[var.feature.0 as usize] = g.phon_features.mask(var.feature);
                }
            }
            self.rows
                .iter()
                .filter(|(_, row)| compatible(row, &lanes[..row.len()]))
                .map(|(id, _)| *id)
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

    /// Spelling is postponed until all rules have seen the actual intermediate bundles.
    pub(crate) fn spelling(&self, opts: &FomaOptions, g: &Grammar) -> Option<Fsm> {
        let table = &g.char_tables[self.table.0 as usize];
        let alphabet = SegAlphabet::new(table);
        let mut pairs = Vec::new();
        for (id, row) in &self.rows {
            for (output, definition) in table.iter() {
                if definition.kind() == CharDefKind::Segment
                    && (if g.phon_features.is_empty() {
                        *id == output
                    } else {
                        compatible(row, definition.feature_lanes())
                    })
                {
                    pairs.push(format!(
                        "{}:{}",
                        alphabet.token(*id),
                        alphabet.token(output)
                    ));
                }
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
        assert!(
            foma::apply::apply_init(&spelling)
                .down(&input)
                .next()
                .is_none(),
            "unspellable material must not be erased into a surface"
        );
    }

    #[test]
    fn fixed_closure_limits_refuse_before_accepting_another_bundle() {
        let mut closure = IntermediateBundles {
            table: TableId(0),
            rows: Vec::new(),
            next_id: 0,
            charge: 0,
            evidence: BundleClosureEvidence::default(),
        };
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
        let mut bytes = IntermediateBundles {
            table: TableId(0),
            rows: Vec::new(),
            next_id: 0,
            charge: 0,
            evidence: BundleClosureEvidence::default(),
        };
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
