//! `AffixTemplate` builder. This module needs exactly the minimal shape
//! GATE 2's circumfix recipe requires: ONE template, ONE slot, wrapping whatever morphological
//! rule ids the caller supplies -- the general slot/template XML shape follows `pg-foma/src/
//! morphotactics.rs`'s own `FIXTURE_SLOTS`/`FIXTURE_STRATA` test fixtures and `machine/
//! conformance/languages/fusional-realizational-morphology/grammar.xml`'s real `<Slot morphologicalRules="...">`
//! shape, reduced here to depth 1 (one template, one slot). A real multi-slot,
//! multi-template builder would need depth/slot-count/optional-fraction scale knobs; not built here.

use crate::ids::IdMinter;

/// One `<AffixTemplate>` with a single slot referencing `rule_xml_ids` (space-joined
/// `morphologicalRules` IDREFS). Returns the template's own XML fragment (an `<AffixTemplate>`
/// element -- the caller wraps it in `<AffixTemplates>...</AffixTemplates>`) and its minted
/// template xml id (unused by GATE 2 today but returned for symmetry with every other builder
/// here, and so a future caller can reference the template itself, e.g. via
/// `requiredPartsOfSpeech`).
pub fn build_single_slot_template(
    rule_xml_ids: &[&str],
    optional: bool,
    ids: &mut IdMinter,
) -> (String, String) {
    let template_xml_id = ids.next("tmpl");
    let slot_xml_id = ids.next("slot");
    let optional_attr = if optional { " optional=\"true\"" } else { "" };
    let rules_attr = rule_xml_ids.join(" ");
    let xml = format!(
        "\n        <AffixTemplate>\n          <Name>{template_xml_id}</Name>\n          \
         <Slot{optional_attr} morphologicalRules=\"{rules_attr}\"><Name>{slot_xml_id}</Name></Slot>\n        </AffixTemplate>"
    );
    (xml, template_xml_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_lists_every_rule_id_space_joined() {
        let mut ids = IdMinter::new();
        let (xml, _tid) = build_single_slot_template(&["mrCirc0", "mrCirc1"], true, &mut ids);
        assert!(xml.contains(r#"morphologicalRules="mrCirc0 mrCirc1""#));
        assert!(xml.contains(r#"optional="true""#));
    }

    #[test]
    fn mandatory_slot_has_no_optional_attribute() {
        let mut ids = IdMinter::new();
        let (xml, _tid) = build_single_slot_template(&["mrCirc0"], false, &mut ids);
        assert!(!xml.contains("optional"));
    }
}
