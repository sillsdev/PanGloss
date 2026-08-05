//! `Dir::RightToLeft` HONEST-SKIP bail gate builder (same detection-wiring dependency as
//! `crate::build::simultaneous`).
//!
//! XML shape: identical to `crate::build::simultaneous` but `multipleApplicationOrder=
//! "rightToLeftIterative"` instead of `"simultaneous"` (`pg_grammar::load::load_rewrite_rule`'s own
//! parsing: the SAME attribute drives both `RewriteMode` and `Dir`, mutually exclusively by value --
//! `"simultaneous"` sets `RewriteMode::Simultaneous` with `Dir` staying the default `LeftToRight`;
//! `"rightToLeftIterative"` sets `Dir::RightToLeft` with `RewriteMode` staying the default
//! `Iterative` -- so this builder and `build::simultaneous` each exercise exactly one of the two
//! unsupported dimensions `is_fully_supported_shape` checks, never both at once).

use crate::build::tables::TableSpec;
use crate::ids::IdMinter;

/// One `multipleApplicationOrder="rightToLeftIterative"`-tagged rule, its own xml id, and the
/// single root's required `<PhoneticShape>` (never actually compiled -- module doc).
#[derive(Debug, Clone)]
pub struct RightToLeftBuild {
    pub prule_xml: String,
    pub rule_xml_id: String,
    pub root_shape: String,
}

pub fn build(table: &TableSpec, ids: &mut IdMinter) -> RightToLeftBuild {
    assert!(
        table.segments.len() >= 2,
        "build_right_to_left: table needs at least 2 segments (LHS target + RHS output), has {}",
        table.segments.len()
    );
    let seg0 = &table.segments[0];
    let seg1 = &table.segments[1];
    let rule_xml_id = ids.next("pruleRtl");
    let prule_xml = format!(
        "\n      <PhonologicalRule id=\"{rule_xml_id}\" multipleApplicationOrder=\"rightToLeftIterative\">\n        \
         <Name>rtlDemo</Name>\n        \
         <PhoneticInput><PhoneticSequence><Segment segment=\"{seg0_id}\" /></PhoneticSequence></PhoneticInput>\n        \
         <PhonologicalSubrules>\n          <PhonologicalSubrule>\n            \
         <PhoneticOutput><PhoneticSequence><Segment segment=\"{seg1_id}\" /></PhoneticSequence></PhoneticOutput>\n          \
         </PhonologicalSubrule>\n        </PhonologicalSubrules>\n      </PhonologicalRule>",
        seg0_id = seg0.xml_id,
        seg1_id = seg1.xml_id,
    );
    RightToLeftBuild {
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
    fn builds_a_right_to_left_tagged_rule() {
        let mut ids = IdMinter::new();
        let tb = tables::build(1, 2, false, false, &mut ids);
        let rb = build(&tb.tables[0], &mut ids);
        assert!(rb
            .prule_xml
            .contains(r#"multipleApplicationOrder="rightToLeftIterative""#));
    }
}
