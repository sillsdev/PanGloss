//! Grammar-site lookups the structural-pass tests decide against, read off the fixture grammar rather than hard-coded.

use pg_grammar::model::{Grammar, MRuleId, MorphemeId};

/// The morpheme carried by the fixture element with this `id` attribute.
pub fn morpheme_of(g: &Grammar, xml_key: &str) -> MorphemeId {
    let index = g
        .morphemes
        .iter()
        .position(|m| m.xml_key == xml_key)
        .unwrap_or_else(|| panic!("no morpheme with xml id {xml_key:?}"));
    MorphemeId(index as u32)
}

/// The rule that owns `morpheme`, read straight off `g.mrules`.
pub fn rule_of(g: &Grammar, morpheme: MorphemeId) -> MRuleId {
    for (index, rule) in g.mrules.iter().enumerate() {
        let owned = match rule {
            pg_grammar::model::MorphRuleDef::AffixProcess(def) => Some(def.morpheme),
            pg_grammar::model::MorphRuleDef::Realizational(def) => Some(def.morpheme),
            pg_grammar::model::MorphRuleDef::Compounding(_) => None,
        };
        if owned == Some(morpheme) {
            return MRuleId(index as u32);
        }
    }
    panic!("no rule owns morpheme {morpheme:?}");
}

/// The `(template, slot)` site listing `rule`, read straight off `g.templates`.
pub fn site_of(g: &Grammar, rule: MRuleId) -> (u16, u8) {
    for (template, def) in g.templates.iter().enumerate() {
        for (slot, def) in def.slots.iter().enumerate() {
            if def.rules.contains(&rule) {
                return (template as u16, slot as u8);
            }
        }
    }
    panic!("no template slot lists rule {rule:?}");
}

/// The stratum that declares `template`, read straight off `g.strata`.
pub fn stratum_of_template(g: &Grammar, template: u16) -> u8 {
    for (stratum, def) in g.strata.iter().enumerate() {
        if def.templates.iter().any(|t| t.0 == u32::from(template)) {
            return stratum as u8;
        }
    }
    panic!("no stratum declares template {template}");
}
