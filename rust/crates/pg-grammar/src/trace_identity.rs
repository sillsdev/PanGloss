//! Rich-trace identities, kept apart from the stats keys that join to `compiled_output`.

use crate::model::{
    AllomorphId, AllomorphOwner, Grammar, LexEntryId, MRuleId, MorphRuleDef, MorphemeId, PRuleId,
    PhonRuleDef,
};
use crate::stats_identity::{
    guessed_allomorph_identity, morpheme_identity, registered_allomorph, AllomorphIdentity,
    IdentityQuality, ObjectIdentity, ObjectKind,
};

/// The morpheme's authored `xml_key`, or `None` if unresolvable or empty.
fn morpheme_xml_key(grammar: &Grammar, morpheme: MorphemeId) -> Option<String> {
    if morpheme == MorphemeId::GUESSED {
        return None;
    }
    grammar
        .morphemes
        .get(morpheme.0 as usize)
        .map(|m| m.xml_key.clone())
        .filter(|k| !k.is_empty())
}

fn morph_rule_name(def: &MorphRuleDef) -> Option<&str> {
    match def {
        MorphRuleDef::Compounding(c) => c.name.as_deref(),
        MorphRuleDef::AffixProcess(a) => a.name.as_deref(),
        MorphRuleDef::Realizational(r) => r.name.as_deref(),
    }
}

/// Compounding carries its own `xml_id`; the other two rule kinds resolve via their `morpheme`.
fn morph_rule_key_and_quality(grammar: &Grammar, id: MRuleId) -> (String, IdentityQuality) {
    let def = &grammar.mrules[id.0 as usize];
    let authored = match def {
        MorphRuleDef::Compounding(c) => (!c.xml_id.is_empty()).then(|| c.xml_id.clone()),
        MorphRuleDef::AffixProcess(a) => morpheme_xml_key(grammar, a.morpheme),
        MorphRuleDef::Realizational(r) => morpheme_xml_key(grammar, r.morpheme),
    };
    match authored {
        Some(key) => (key, IdentityQuality::Authored),
        None => (
            format!("mrule#{}:{}", id.0, morph_rule_name(def).unwrap_or("")),
            IdentityQuality::Structural,
        ),
    }
}

fn disambiguate_morph_rule_key(
    key: &str,
    index: u32,
    key_count: usize,
    quality: IdentityQuality,
) -> (String, IdentityQuality) {
    if key_count > 1 {
        (format!("{key}#mrule#{index}"), IdentityQuality::Structural)
    } else {
        (key.to_string(), quality)
    }
}

/// Trace identity of a morphological rule.
pub fn trace_morph_rule_identity(grammar: &Grammar, id: MRuleId) -> ObjectIdentity {
    let (key, quality) = morph_rule_key_and_quality(grammar, id);
    let key_count = (0..grammar.mrules.len())
        .filter(|index| morph_rule_key_and_quality(grammar, MRuleId(*index as u32)).0 == key)
        .count();
    let (key, quality) = disambiguate_morph_rule_key(&key, id.0, key_count, quality);
    let def = &grammar.mrules[id.0 as usize];
    let label = morph_rule_name(def)
        .map(str::to_string)
        .unwrap_or_else(|| key.clone());
    ObjectIdentity {
        key,
        kind: ObjectKind::MorphRule,
        label,
        quality,
    }
}

/// Trace identity of a phonological rule.
pub fn trace_phon_rule_identity(grammar: &Grammar, id: PRuleId) -> ObjectIdentity {
    let def = &grammar.prules[id.0 as usize];
    let (xml_id, name) = match def {
        PhonRuleDef::Rewrite(r) => (r.xml_id.as_str(), r.name.as_deref()),
        PhonRuleDef::Metathesis(m) => (m.xml_id.as_str(), m.name.as_deref()),
    };
    let (key, quality) = if xml_id.is_empty() {
        (
            format!("prule#{}:{}", id.0, name.unwrap_or("")),
            IdentityQuality::Structural,
        )
    } else {
        (xml_id.to_string(), IdentityQuality::Authored)
    };
    ObjectIdentity {
        label: name.map(str::to_string).unwrap_or_else(|| key.clone()),
        key,
        kind: ObjectKind::PhonRule,
        quality,
    }
}

fn lex_entry_authored_key(
    authored_id: &str,
    morpheme_key: &str,
    morpheme_quality: IdentityQuality,
    index: u32,
    authored_id_count: usize,
    same_morpheme_count: usize,
) -> (String, IdentityQuality) {
    if authored_id_count == 1 {
        return (authored_id.to_string(), IdentityQuality::Authored);
    }
    if same_morpheme_count == 1 {
        let quality = if morpheme_quality == IdentityQuality::Authored {
            IdentityQuality::Authored
        } else {
            IdentityQuality::Structural
        };
        (format!("{authored_id}#morpheme:{morpheme_key}"), quality)
    } else {
        (
            format!("{authored_id}#morpheme:{morpheme_key}#entry:{index}"),
            IdentityQuality::Structural,
        )
    }
}

/// Trace identity of a lexical entry.
pub fn trace_lex_entry_identity(grammar: &Grammar, id: LexEntryId) -> ObjectIdentity {
    let entry = &grammar.entries[id.0 as usize];
    let (key, quality) = if entry.authored_id.trim().is_empty() {
        (format!("lex_entry#{}", id.0), IdentityQuality::Structural)
    } else {
        let peers = grammar
            .entries
            .iter()
            .enumerate()
            .filter(|(_, peer)| peer.authored_id == entry.authored_id)
            .collect::<Vec<_>>();
        if peers.len() == 1 {
            (entry.authored_id.clone(), IdentityQuality::Authored)
        } else {
            let morpheme = morpheme_identity(grammar, entry.morpheme);
            let same_morpheme_count = peers
                .iter()
                .filter(|(_, peer)| morpheme_identity(grammar, peer.morpheme).key == morpheme.key)
                .count();
            lex_entry_authored_key(
                &entry.authored_id,
                &morpheme.key,
                morpheme.quality,
                id.0,
                peers.len(),
                same_morpheme_count,
            )
        }
    };
    let label = grammar
        .morphemes
        .get(entry.morpheme.0 as usize)
        .and_then(|m| m.gloss.clone())
        .filter(|g| !g.is_empty())
        .unwrap_or_else(|| key.clone());
    ObjectIdentity {
        key,
        kind: ObjectKind::LexEntry,
        label,
        quality,
    }
}

/// Trace identity of an allomorph: its owner's trace identity plus its index.
pub fn trace_allomorph_identity(grammar: &Grammar, id: AllomorphId) -> AllomorphIdentity {
    match registered_allomorph(&grammar.allomorph_owners, id) {
        Some(AllomorphOwner::Root(entry_id, index)) => {
            let owner = trace_lex_entry_identity(grammar, *entry_id);
            allomorph_fields("lex_entry", &owner.key, &owner.label, *index)
        }
        Some(AllomorphOwner::Affix(rule_id, index)) => {
            let owner = trace_morph_rule_identity(grammar, *rule_id);
            allomorph_fields("morph_rule", &owner.key, &owner.label, *index)
        }
        None => guessed_allomorph_identity(),
    }
}

fn allomorph_fields(
    owner_kind: &str,
    owner_key: &str,
    owner_label: &str,
    index: u16,
) -> AllomorphIdentity {
    AllomorphIdentity {
        key: format!("{owner_kind}:{owner_key}#allo{index}"),
        label: format!("{owner_label} allomorph {index}"),
        quality: IdentityQuality::Structural,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grammar() -> Grammar {
        let fixture =
            pg_conformance_fixtures::require_fixture("languages", "metathesis-phase-isolation");
        crate::load(&fixture.load_grammar_xml()).unwrap()
    }

    #[test]
    fn the_guessed_trace_allomorph_sentinel_resolves_instead_of_panicking() {
        let identity = trace_allomorph_identity(&grammar(), AllomorphId::GUESSED);
        assert_eq!(identity, guessed_allomorph_identity());
    }

    #[test]
    #[should_panic(expected = "allomorph id 4294967294 has no registry owner")]
    fn an_unresolved_non_guessed_trace_allomorph_id_panics_with_its_id() {
        trace_allomorph_identity(&grammar(), AllomorphId(u32::MAX - 1));
    }

    #[test]
    fn repeated_authored_source_ids_keep_runtime_objects_distinct() {
        let first_rule = disambiguate_morph_rule_key("msa-guid", 2, 2, IdentityQuality::Authored);
        let second_rule = disambiguate_morph_rule_key("msa-guid", 5, 2, IdentityQuality::Authored);
        assert_ne!(first_rule.0, second_rule.0);
        assert_eq!(first_rule.1, IdentityQuality::Structural);
        assert_eq!(second_rule.1, IdentityQuality::Structural);

        let first_msa_entry =
            lex_entry_authored_key("entry-guid", "msa-a", IdentityQuality::Authored, 2, 2, 1);
        let second_msa_entry =
            lex_entry_authored_key("entry-guid", "msa-b", IdentityQuality::Authored, 5, 2, 1);
        assert_ne!(first_msa_entry.0, second_msa_entry.0);
        assert_eq!(first_msa_entry.1, IdentityQuality::Authored);
        assert_eq!(second_msa_entry.1, IdentityQuality::Authored);

        let repeated_msa_entry =
            lex_entry_authored_key("entry-guid", "msa-a", IdentityQuality::Authored, 5, 2, 2);
        assert_ne!(first_msa_entry.0, repeated_msa_entry.0);
        assert_eq!(repeated_msa_entry.1, IdentityQuality::Structural);
    }
}
