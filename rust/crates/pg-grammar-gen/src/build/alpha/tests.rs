use super::*;
use crate::build::tables;

#[test]
fn three_vars_mint_three_independent_rules() {
    let mut ids = IdMinter::new();
    let tb = tables::build(1, 5, false, false, &mut ids);
    let ab = build(3, &tb.tables[0], &mut ids);
    assert_eq!(ab.rule_xml_ids.len(), 3);
    assert_eq!(ab.root_shape.chars().count(), 3);
    assert_eq!(ab.prules_xml.matches("<VariableFeature ").count(), 3);
    assert_eq!(
        ab.prules_xml
            .matches("<PhoneticInput><PhoneticSequence><SimpleContext")
            .count(),
        3
    );
}
