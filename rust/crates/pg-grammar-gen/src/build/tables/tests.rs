use super::*;

#[test]
fn aligned_tables_share_polarity_per_index() {
    let mut ids = IdMinter::new();
    let tb = build(2, 2, false, false, &mut ids);
    assert_eq!(
        tb.tables[0].segments[0].voice_plus,
        tb.tables[1].segments[0].voice_plus
    );
}

#[test]
fn misaligned_tables_flip_polarity_per_index_after_the_first() {
    let mut ids = IdMinter::new();
    let tb = build(2, 2, true, false, &mut ids);
    assert_ne!(
        tb.tables[0].segments[0].voice_plus,
        tb.tables[1].segments[0].voice_plus
    );
}

#[test]
fn tables_never_share_a_character() {
    let mut ids = IdMinter::new();
    let tb = build(3, 2, true, false, &mut ids);
    let mut seen = std::collections::HashSet::new();
    for t in &tb.tables {
        for s in &t.segments {
            assert!(
                seen.insert(s.ch),
                "character {:?} reused across tables",
                s.ch
            );
        }
    }
}

#[test]
fn single_table_has_no_devoice_demo_rule() {
    let mut ids = IdMinter::new();
    let tb = build(1, 2, false, false, &mut ids);
    assert!(tb.devoice_rule_xml.is_none());
    assert!(tb.devoice_rule_xml_id.is_none());
}
