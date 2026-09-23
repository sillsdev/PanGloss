use super::*;
use crate::build::tables;

#[test]
fn builds_one_compounding_rule_and_two_roots() {
    let mut ids = IdMinter::new();
    let tb = tables::build(1, 2, false, true, &mut ids);
    let cb = build(
        "posV",
        tb.boundary_xml_id.as_deref().unwrap(),
        &tb.tables[0],
        &mut ids,
    );
    assert!(cb.rule_xml.contains("CompoundingRule"));
    assert_ne!(cb.head_entry_xml_id, cb.nonhead_entry_xml_id);
}
