//! Quantifier / `OptionalSegmentSequence` HONEST-SKIP bail gate builder (pure
//! test-writing -- `pg_foma::replace::pattern_slots` already returns `None` on a `PatternNode::
//! Quantifier` it meets in a REWRITE rule's own LHS/RHS/environment, which
//! `compile_rewrite_rule_subset` turns into `Ok(None)` for the whole rule, and the caller
//! (`compile_and_compose_rules_with_budget`) reports it via `skipped.push(rule.xml_id.clone())` --
//! this builder only needs to mint a LOADABLE `<PhonologicalRule>` whose LHS is a bare
//! `<OptionalSegmentSequence>`, not a working compiler for it.
//!
//! Note: `OptionalSegmentSequence` is a perfectly ordinary, WORKING construct elsewhere in this
//! generator (every root-capturing `MorphologicalRule`'s own `MorphologicalInput`, e.g.
//! [`crate::build::circumfix`], uses it) -- `pattern_slots`'s bail is specific to the P6 REWRITE-
//! RULE compiler (`pg_foma::replace`), not a claim that HermitCrab itself can't represent it.

use crate::build::tables::TableSpec;
use crate::ids::IdMinter;

/// One quantifier-bearing `<PhonologicalRule>`, its own xml id, and the single root's required
/// `<PhoneticShape>` text (reuses `table`'s first segment; the rule is never actually compiled --
/// module doc -- so the root's own spelling is not load-bearing beyond being a valid character).
#[derive(Debug, Clone)]
pub struct QuantifierBuild {
    pub prule_xml: String,
    pub rule_xml_id: String,
    pub root_shape: String,
}

pub fn build(table: &TableSpec, ids: &mut IdMinter) -> QuantifierBuild {
    assert!(
        !table.segments.is_empty(),
        "build_quantifier: table must have at least 1 segment"
    );
    let nc_any = crate::build::tables::nc_any_xml_id();
    let seg0 = &table.segments[0];
    let rule_xml_id = ids.next("pruleQuant");
    let prule_xml = format!(
        "\n      <PhonologicalRule id=\"{rule_xml_id}\">\n        <Name>quantDemo</Name>\n        \
         <PhoneticInput><PhoneticSequence><OptionalSegmentSequence min=\"1\" max=\"-1\">\
         <SimpleContext naturalClass=\"{nc_any}\" /></OptionalSegmentSequence></PhoneticSequence></PhoneticInput>\n        \
         <PhonologicalSubrules>\n          <PhonologicalSubrule>\n            \
         <PhoneticOutput><PhoneticSequence><Segment segment=\"{seg0_id}\" /></PhoneticSequence></PhoneticOutput>\n          \
         </PhonologicalSubrule>\n        </PhonologicalSubrules>\n      </PhonologicalRule>",
        seg0_id = seg0.xml_id,
    );
    QuantifierBuild {
        prule_xml,
        rule_xml_id,
        root_shape: seg0.ch.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::tables;

    #[test]
    fn builds_a_quantifier_bearing_rule() {
        let mut ids = IdMinter::new();
        let tb = tables::build(1, 2, false, false, &mut ids);
        let qb = build(&tb.tables[0], &mut ids);
        assert!(qb.prule_xml.contains("OptionalSegmentSequence"));
        assert_eq!(qb.root_shape.chars().count(), 1);
    }
}
