//! `MetathesisRuleDef` HONEST-SKIP bail gate builder (pure test-writing -- `pg_foma::replace::
//! compile_and_compose_rules_with_budget`'s own match on `pg_grammar::model::PhonRuleDef` already
//! routes EVERY `PhonRuleDef::Metathesis` straight to `skipped.push(format!("{} (metathesis,
//! unhandled)", m.xml_id))`, with no compile attempt at all -- this builder only needs a LOADABLE
//! `<MetathesisRule>`, following `machine/conformance/languages/metathesis-phase-isolation/grammar.xml`'s
//! own `mrSimpleMeta` shape (the only real `<MetathesisRule>` fixture in this repo) exactly: two
//! switch-tagged pattern nodes (`leftSwitch`/`rightSwitch` IDREFs into the rule's own
//! `StructuralDescription/PhoneticTemplate/PhoneticSequence`), here plain `<Segment id=".."
//! segment="..">` nodes (simpler than `metathesis-phase-isolation`'s natural-class-based switches, and
//! equally DTD-legal per `pg_grammar::load::load_one_pattern_node`'s generic per-element `id`-
//! attribute switch-tagging, which does not care which pattern-node kind carries the tag).

use crate::build::tables::TableSpec;
use crate::ids::IdMinter;

/// One metathesis rule, its own xml id, and the single root's required `<PhoneticShape>` text
/// (the two switch segments, concatenated -- never actually compiled, module doc, so this is not
/// load-bearing beyond being valid characters).
#[derive(Debug, Clone)]
pub struct MetathesisBuild {
    pub prule_xml: String,
    pub rule_xml_id: String,
    pub root_shape: String,
}

pub fn build(table: &TableSpec, ids: &mut IdMinter) -> MetathesisBuild {
    assert!(
        table.segments.len() >= 2,
        "build_metathesis: table needs at least 2 segments (two distinct switch positions), has {}",
        table.segments.len()
    );
    let seg0 = &table.segments[0];
    let seg1 = &table.segments[1];
    let rule_xml_id = ids.next("mrMeta");
    let left_switch_xml_id = ids.next("swL");
    let right_switch_xml_id = ids.next("swR");
    let prule_xml = format!(
        "\n      <MetathesisRule id=\"{rule_xml_id}\" leftSwitch=\"{left_switch_xml_id}\" rightSwitch=\"{right_switch_xml_id}\">\n        \
         <Name>metaDemo</Name>\n        <StructuralDescription>\n          <PhoneticTemplate>\n            <PhoneticSequence>\n              \
         <Segment id=\"{left_switch_xml_id}\" segment=\"{seg0_id}\" />\n              \
         <Segment id=\"{right_switch_xml_id}\" segment=\"{seg1_id}\" />\n            \
         </PhoneticSequence>\n          </PhoneticTemplate>\n        </StructuralDescription>\n      </MetathesisRule>",
        seg0_id = seg0.xml_id,
        seg1_id = seg1.xml_id,
    );
    MetathesisBuild {
        prule_xml,
        rule_xml_id,
        root_shape: format!("{}{}", seg0.ch, seg1.ch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::tables;

    #[test]
    fn builds_a_switch_tagged_metathesis_rule() {
        let mut ids = IdMinter::new();
        let tb = tables::build(1, 2, false, false, &mut ids);
        let mb = build(&tb.tables[0], &mut ids);
        assert!(mb.prule_xml.contains("leftSwitch="));
        assert!(mb.prule_xml.contains("rightSwitch="));
        assert_eq!(mb.root_shape.chars().count(), 2);
    }
}
