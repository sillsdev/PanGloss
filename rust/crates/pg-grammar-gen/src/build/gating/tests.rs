use super::*;
use crate::build::tables;

#[test]
fn two_gated_rules_realize_four_combinations() {
    let mut ids = IdMinter::new();
    let tb = tables::build(1, 5, false, false, &mut ids);
    let gb = build(2, "posV", &tb.tables[0], &mut ids);
    assert_eq!(gb.rule_xml_ids.len(), 2);
    assert_eq!(gb.entry_xml_ids.len(), 4);
    assert!(
        gb.entries_xml
            .contains(r#"ruleFeatures="mprGate0 mprGate1""#),
        "entry 3 (bits 0,1 set) must carry both mpr features:\n{}",
        gb.entries_xml
    );
    // Entry 0 (no bits set) must carry no ruleFeatures attribute at all.
    let entry0_id = &gb.entry_xml_ids[0];
    let entry0_pos = gb
        .entries_xml
        .find(entry0_id.as_str())
        .expect("entry0 present");
    let entry0_line = &gb.entries_xml[entry0_pos..entry0_pos + 120];
    assert!(
        !entry0_line.contains("ruleFeatures"),
        "entry 0 must have no ruleFeatures: {entry0_line}"
    );
}

#[test]
fn bit_set_matches_shift_convention() {
    assert!(!bit_set(0, 0));
    assert!(bit_set(1, 0));
    assert!(!bit_set(1, 1));
    assert!(bit_set(2, 1));
    assert!(bit_set(3, 0));
    assert!(bit_set(3, 1));
}
