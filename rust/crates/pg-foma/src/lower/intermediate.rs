//! Per-cascade feature-bundle closure; private tokens retain material until final spelling.

use super::Slot;
use crate::replace::{PhonologyLoweringError, SegAlphabet};
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
        };
        let first_rule = &rules[0].xml_id;
        for (id, definition) in table.iter() {
            if definition.kind() == CharDefKind::Segment {
                result.push(id, definition.feature_lanes().to_vec(), first_rule)?;
            }
        }
        for rule in rules {
            // A fixpoint per rule conservatively also covers repeated site/subrule applications.
            loop {
                let previous_len = result.rows.len();
                for subrule in &rule.subrules {
                    for node in &subrule.rhs.nodes {
                        if !matches!(node, PatternNode::CharDef(_) | PatternNode::Context(_)) {
                            return Err(PhonologyLoweringError::UnsupportedRewrite(
                                rule.xml_id.clone(),
                            ));
                        }
                        if matches!(node, PatternNode::CharDef(id) if table.get(*id).kind() == CharDefKind::Boundary)
                        {
                            continue;
                        }
                        let pins = pg_rules::rewrite::node_pins(g, table, node);
                        let mut bases: Vec<_> =
                            result.rows.iter().map(|(_, row)| row.clone()).collect();
                        // Expansion and epenthesis create a node instead of updating a target.
                        bases.push(
                            pg_rules::rewrite::node_full_lanes(g, table, node)
                                [..g.phon_features.len()]
                                .to_vec(),
                        );
                        for mut row in bases {
                            for &(lane, bits) in &pins {
                                if lane < row.len() {
                                    row[lane] = bits;
                                }
                            }
                            row[g.phon_features.type_flat().0 as usize] = TYPE_SEGMENT_BITS;
                            let mut variants = vec![row];
                            if let PatternNode::Context(context) = node {
                                for var in &context.vars {
                                    let mask = g.phon_features.mask(var.feature);
                                    // Every possible binding/negation is included, including zero.
                                    if mask.count_ones() > MAX_BUNDLES.ilog2() {
                                        return Err(Self::unavailable(&rule.xml_id));
                                    }
                                    let mut next = Vec::new();
                                    let mut bits = mask;
                                    loop {
                                        for row in &variants {
                                            let mut row = row.clone();
                                            row[var.feature.0 as usize] = bits;
                                            if next.len() >= MAX_BUNDLES
                                                || (next.len() + 1)
                                                    .saturating_mul(Self::row_charge(row.len()))
                                                    > MAX_RETAINED_CHARGE
                                            {
                                                return Err(Self::unavailable(&rule.xml_id));
                                            }
                                            next.push(row);
                                        }
                                        if bits == 0 {
                                            break;
                                        }
                                        bits = (bits - 1) & mask;
                                    }
                                    variants = next;
                                }
                            }
                            for row in variants {
                                if !result.rows.iter().any(|(_, existing)| existing == &row) {
                                    let id = CharDefId(result.next_id);
                                    result.push(id, row, &rule.xml_id)?;
                                    result.next_id += 1;
                                }
                            }
                        }
                    }
                }
                if result.rows.len() == previous_len {
                    break;
                }
            }
        }
        Ok(result)
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
