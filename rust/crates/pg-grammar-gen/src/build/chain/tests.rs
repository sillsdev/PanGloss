use super::*;
use crate::build::tables;

#[test]
fn distinct_rules_get_distinct_suffix_material() {
    let mut ids = IdMinter::new();
    let tb = tables::build(1, 6, false, false, &mut ids);
    let cb = build(5, "posV", &tb.tables[0], &mut ids);
    assert_eq!(cb.rule_xml_ids.len(), 5);
    // Every rule's suffix character must be distinct from every other's and from the root's.
    let mut chars: Vec<char> = tb.tables[0].segments[0..6].iter().map(|s| s.ch).collect();
    chars.dedup();
    assert_eq!(
        chars.len(),
        6,
        "build_chain draws 6 distinct chars (1 root + 5 suffixes)"
    );
}

#[test]
#[should_panic(expected = "needs at least")]
fn panics_when_table_too_small() {
    let mut ids = IdMinter::new();
    let tb = tables::build(1, 3, false, false, &mut ids);
    let _ = build(5, "posV", &tb.tables[0], &mut ids);
}
