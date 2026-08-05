//! `RewriteMode::Simultaneous` HONEST-SKIP bail gate builder. Needs the detection wiring
//! (`pg_foma::replace::compile_rewrite_rule_subset`'s `is_fully_supported_shape` check) to route
//! an unsupported mode to `skipped` rather than silently compiling it as if it were `Iterative`
//! -- see `pg-foma/src/replace.rs`'s own updated doc.
//!
//! XML shape: an ordinary unconditional `<PhonologicalRule>` (no environment, no MPR/POS gating --
//! nothing here should trip any OTHER budget/skip path first) with
//! `multipleApplicationOrder="simultaneous"` (`machine/conformance/languages/templatic-root-modification/
//! grammar.xml`'s own `prSimulFeeding`/`prEpenthesis` shape, reduced to the minimal unconditional
//! case). The whole rule is screened out on shape alone, upstream of any LHS/RHS lowering, so the
//! rule's own content is never compiled and any two distinct segments suffice.

use crate::build::tables::TableSpec;
use crate::ids::IdMinter;

/// One `multipleApplicationOrder="simultaneous"`-tagged rule, its own xml id, and the single root's
/// required `<PhoneticShape>` (the rule's own LHS target character -- never actually compiled,
/// module doc).
#[derive(Debug, Clone)]
pub struct SimultaneousBuild {
    pub prule_xml: String,
    pub rule_xml_id: String,
    pub root_shape: String,
}

pub fn build(table: &TableSpec, ids: &mut IdMinter) -> SimultaneousBuild {
    assert!(
        table.segments.len() >= 2,
        "build_simultaneous: table needs at least 2 segments (LHS target + RHS output), has {}",
        table.segments.len()
    );
    let seg0 = &table.segments[0];
    let seg1 = &table.segments[1];
    let rule_xml_id = ids.next("pruleSimul");
    let prule_xml = format!(
        "\n      <PhonologicalRule id=\"{rule_xml_id}\" multipleApplicationOrder=\"simultaneous\">\n        \
         <Name>simulDemo</Name>\n        \
         <PhoneticInput><PhoneticSequence><Segment segment=\"{seg0_id}\" /></PhoneticSequence></PhoneticInput>\n        \
         <PhonologicalSubrules>\n          <PhonologicalSubrule>\n            \
         <PhoneticOutput><PhoneticSequence><Segment segment=\"{seg1_id}\" /></PhoneticSequence></PhoneticOutput>\n          \
         </PhonologicalSubrule>\n        </PhonologicalSubrules>\n      </PhonologicalRule>",
        seg0_id = seg0.xml_id,
        seg1_id = seg1.xml_id,
    );
    SimultaneousBuild {
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
    fn builds_a_simultaneous_tagged_rule() {
        let mut ids = IdMinter::new();
        let tb = tables::build(1, 2, false, false, &mut ids);
        let sb = build(&tb.tables[0], &mut ids);
        assert!(sb
            .prule_xml
            .contains(r#"multipleApplicationOrder="simultaneous""#));
    }
}
