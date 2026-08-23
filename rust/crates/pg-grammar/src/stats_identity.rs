//! Stable identity resolution for the `--stats` collector: maps each runtime id in a compiled
//! `Grammar` to a durable, human-legible record, so a report can say "look here" rather than
//! print a bare numeric handle that means nothing once the grammar is reloaded.
//!
//! Three tiers of trustworthiness exist because not every runtime id has an author-assigned
//! counterpart: an id backed by an XML `id=`/MSA GUID is [`IdentityQuality::Authored`]; one
//! reconstructed from the object's position in the compiled tables (a stratum has no id field at
//! all) is [`IdentityQuality::Structural`]; one with no grammar-side counterpart whatsoever (the
//! shared root trie, the guesser, the supplied-roots overlay) is [`IdentityQuality::Synthetic`].
//! A caller must never present a `Structural`/`Synthetic` key as if it were authored.
//!
//! `stratum` and `allomorph` are locators rather than `ObjectKind` members: both are dimensions
//! that a report groups by, never objects a report counts on their own, so they get their own
//! identity types ([`StratumIdentity`], [`AllomorphIdentity`]) instead of forcing an
//! `ObjectIdentity` to carry a `kind` that isn't really one.

use crate::model::{
    AllomorphId, AllomorphOwner, Grammar, LexEntryId, MRuleId, MorphRuleDef, MorphemeId, PRuleId,
    PhonRuleDef, StratumId,
};

/// How trustworthy an identity's `key` is as a stable, cross-run identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityQuality {
    /// Backed by an author-assigned id (an XML `id=` attribute, an MSA/entry GUID) that survives
    /// grammar re-authoring.
    Authored,
    /// Reconstructed from the object's position in the compiled tables. Stable across reloads of
    /// the same source, but shifts if the grammar is restructured.
    Structural,
    /// Fabricated by this module; no authored or structural counterpart exists in the grammar.
    Synthetic,
}

/// Which table an [`ObjectIdentity`] names a row in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    MorphRule,
    PhonRule,
    LexEntry,
    RootIndex,
    Guesser,
    Overlay,
}

/// A stable, human-legible identity for one runtime object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectIdentity {
    pub key: String,
    pub kind: ObjectKind,
    pub label: String,
    pub quality: IdentityQuality,
}

/// A stratum's locator identity. Always [`IdentityQuality::Structural`]: `StratumDef` has no id
/// field of its own, only `name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StratumIdentity {
    pub key: String,
    pub label: String,
    pub quality: IdentityQuality,
}

/// An allomorph's locator identity: the owning object's own identity plus its index within that
/// object's `allomorphs` vector. [`IdentityQuality::Structural`], because `AllomorphId` is a dense
/// runtime handle with no authored counterpart — except the guessed-root sentinel, which is
/// [`IdentityQuality::Synthetic`] since it names no grammar row at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllomorphIdentity {
    pub key: String,
    pub label: String,
    pub quality: IdentityQuality,
}

/// Every identity in a grammar, keyed by dimension — enough for a caller to build a lookup table
/// from a runtime id to its resolved identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarIdentities {
    /// One entry per `MRuleId`, `PRuleId`, `LexEntryId`, `root_index` (one per stratum), plus one
    /// `guesser` and one `overlay`.
    pub objects: Vec<ObjectIdentity>,
    pub strata: Vec<StratumIdentity>,
    pub allomorphs: Vec<AllomorphIdentity>,
}

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
        // A bare ordinal is not a locator: pair it with the authored name so the key survives a reorder.
        None => (
            format!("mrule#{}:{}", id.0, morph_rule_name(def).unwrap_or("")),
            IdentityQuality::Structural,
        ),
    }
}

/// Resolve a morphological rule's stable identity.
pub fn morph_rule_identity(grammar: &Grammar, id: MRuleId) -> ObjectIdentity {
    let (key, quality) = morph_rule_key_and_quality(grammar, id);
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

/// Resolve a phonological rule's stable identity. Both `PhonRuleDef` variants (`Rewrite`,
/// `Metathesis`) carry their own `xml_id`/`name` directly.
pub fn phon_rule_identity(grammar: &Grammar, id: PRuleId) -> ObjectIdentity {
    let def = &grammar.prules[id.0 as usize];
    let (xml_id, name) = match def {
        PhonRuleDef::Rewrite(r) => (r.xml_id.as_str(), r.name.as_deref()),
        PhonRuleDef::Metathesis(m) => (m.xml_id.as_str(), m.name.as_deref()),
    };
    // An absent `xml_id` must not be published as an authored key: an empty string is not an identity.
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

/// Resolve a lexical entry's stable identity. The label prefers the entry's morpheme's gloss
/// (what a human recognizes in FLEx) and falls back to the authored id when no gloss is reachable.
pub fn lex_entry_identity(grammar: &Grammar, id: LexEntryId) -> ObjectIdentity {
    let entry = &grammar.entries[id.0 as usize];
    let label = grammar
        .morphemes
        .get(entry.morpheme.0 as usize)
        .and_then(|m| m.gloss.clone())
        .filter(|g| !g.is_empty())
        .unwrap_or_else(|| entry.authored_id.clone());
    ObjectIdentity {
        key: entry.authored_id.clone(),
        kind: ObjectKind::LexEntry,
        label,
        quality: IdentityQuality::Authored,
    }
}

/// Resolve a stratum's structural locator: `StratumDef` has no id field, so identity is index
/// plus name.
pub fn stratum_identity(grammar: &Grammar, id: StratumId) -> StratumIdentity {
    let def = &grammar.strata[id.0 as usize];
    let label = def
        .name
        .clone()
        .unwrap_or_else(|| format!("stratum {}", id.0));
    StratumIdentity {
        key: format!("stratum#{}:{}", id.0, def.name.as_deref().unwrap_or("")),
        label,
        quality: IdentityQuality::Structural,
    }
}

/// Resolve an allomorph's structural locator from its owner (`AllomorphOwner` already carries
/// both the owning object's id and the allomorph's index within it).
pub fn allomorph_identity_for_owner(grammar: &Grammar, owner: AllomorphOwner) -> AllomorphIdentity {
    let (owner_key, owner_label, index) = match owner {
        AllomorphOwner::Root(entry_id, idx) => {
            let owner_identity = lex_entry_identity(grammar, entry_id);
            (owner_identity.key, owner_identity.label, idx)
        }
        AllomorphOwner::Affix(mrule_id, idx) => {
            let owner_identity = morph_rule_identity(grammar, mrule_id);
            (owner_identity.key, owner_identity.label, idx)
        }
    };
    AllomorphIdentity {
        key: format!("{owner_key}#allo{index}"),
        label: format!("{owner_label} allomorph {index}"),
        quality: IdentityQuality::Structural,
    }
}

/// Resolve an allomorph's structural locator by its dense runtime id, via the grammar's
/// allomorph registry.
pub fn allomorph_identity(grammar: &Grammar, id: AllomorphId) -> AllomorphIdentity {
    // `AllomorphId::GUESSED` indexes no registry row; `model.rs` requires every resolution site to special-case it.
    match grammar.allomorph_owners.get(id.0 as usize) {
        Some(owner) => allomorph_identity_for_owner(grammar, *owner),
        None => AllomorphIdentity {
            key: "guesser#allo".to_string(),
            label: "guessed root allomorph".to_string(),
            quality: IdentityQuality::Synthetic,
        },
    }
}

/// The stratum's shared root trie: not an authored object (the trie is one structure serving
/// every lexical entry in the stratum), so a synthetic key is fabricated per stratum.
pub fn root_index_identity(grammar: &Grammar, stratum: StratumId) -> ObjectIdentity {
    let def = &grammar.strata[stratum.0 as usize];
    let stratum_label = def
        .name
        .clone()
        .unwrap_or_else(|| format!("stratum {}", stratum.0));
    ObjectIdentity {
        key: format!("root_index#{}", stratum.0),
        kind: ObjectKind::RootIndex,
        label: format!("root trie ({stratum_label})"),
        quality: IdentityQuality::Synthetic,
    }
}

/// The grammar-wide root-guessing pseudo-object: a fabricated root has no `Grammar` table row at
/// all (see `MorphemeId::GUESSED`/`AllomorphId::GUESSED`), so this is a single synthetic key.
pub fn guesser_identity(_grammar: &Grammar) -> ObjectIdentity {
    ObjectIdentity {
        key: "guesser".to_string(),
        kind: ObjectKind::Guesser,
        label: "root guesser".to_string(),
        quality: IdentityQuality::Synthetic,
    }
}

/// The grammar-wide supplied-roots overlay pseudo-object.
pub fn overlay_identity(_grammar: &Grammar) -> ObjectIdentity {
    ObjectIdentity {
        key: "overlay".to_string(),
        kind: ObjectKind::Overlay,
        label: "supplied roots".to_string(),
        quality: IdentityQuality::Synthetic,
    }
}

/// Resolve every identity in `grammar`, so a caller can build a lookup table from every runtime
/// id (`MRuleId`, `PRuleId`, `LexEntryId`, `StratumId`, `AllomorphId`) to its resolved identity in
/// one pass.
pub fn resolve_all(grammar: &Grammar) -> GrammarIdentities {
    let mut objects = Vec::with_capacity(
        grammar.mrules.len()
            + grammar.prules.len()
            + grammar.entries.len()
            + grammar.strata.len()
            + 2,
    );
    for i in 0..grammar.mrules.len() {
        objects.push(morph_rule_identity(grammar, MRuleId(i as u32)));
    }
    for i in 0..grammar.prules.len() {
        objects.push(phon_rule_identity(grammar, PRuleId(i as u32)));
    }
    for i in 0..grammar.entries.len() {
        objects.push(lex_entry_identity(grammar, LexEntryId(i as u32)));
    }
    for i in 0..grammar.strata.len() {
        objects.push(root_index_identity(grammar, StratumId(i as u8)));
    }
    objects.push(guesser_identity(grammar));
    objects.push(overlay_identity(grammar));

    let strata = (0..grammar.strata.len())
        .map(|i| stratum_identity(grammar, StratumId(i as u8)))
        .collect();

    let allomorphs = (0..grammar.allomorph_owners.len())
        .map(|i| allomorph_identity(grammar, AllomorphId(i as u32)))
        .collect();

    GrammarIdentities {
        objects,
        strata,
        allomorphs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_conformance_fixtures::{discover, FixtureRef};

    fn find_fixture<'a>(fixtures: &'a [FixtureRef], category: &str, name: &str) -> &'a FixtureRef {
        fixtures
            .iter()
            .find(|f| f.category == category && f.name == name)
            .unwrap_or_else(|| panic!("fixture {category}/{name} must be discoverable"))
    }

    /// Two structurally different fixtures already used by `pg-parse`'s own conformance gate.
    fn two_sample_grammars() -> [crate::model::Grammar; 2] {
        let fixtures = discover();
        assert!(!fixtures.is_empty(), "no conformance fixtures discovered");
        let a = find_fixture(&fixtures, "languages", "metathesis-phase-isolation");
        let b = find_fixture(&fixtures, "edge-cases", "truncate-morphotactic");
        [
            crate::load(&a.load_grammar_xml()).expect("fixture a must load"),
            crate::load(&b.load_grammar_xml()).expect("fixture b must load"),
        ]
    }

    /// Resolving the guessed-root sentinel must not index the allomorph registry out of bounds.
    #[test]
    fn the_guessed_allomorph_sentinel_resolves_instead_of_panicking() {
        let [grammar, _] = two_sample_grammars();
        let identity = allomorph_identity(&grammar, crate::model::AllomorphId::GUESSED);
        assert!(!identity.key.is_empty());
        assert_eq!(identity.quality, IdentityQuality::Synthetic);
    }

    fn assert_all_non_empty_and_expected_quality(grammar: &crate::model::Grammar, label: &str) {
        let identities = resolve_all(grammar);

        for oi in &identities.objects {
            assert!(!oi.key.is_empty(), "{label}: empty key for {oi:?}");
            assert!(!oi.label.is_empty(), "{label}: empty label for {oi:?}");
            match oi.kind {
                ObjectKind::MorphRule | ObjectKind::PhonRule | ObjectKind::LexEntry => {
                    assert!(
                        matches!(
                            oi.quality,
                            IdentityQuality::Authored | IdentityQuality::Structural
                        ),
                        "{label}: unexpected quality for authored-capable kind {oi:?}"
                    );
                }
                ObjectKind::RootIndex | ObjectKind::Guesser | ObjectKind::Overlay => {
                    assert_eq!(
                        oi.quality,
                        IdentityQuality::Synthetic,
                        "{label}: synthetic kind must be Synthetic: {oi:?}"
                    );
                }
            }
        }

        for si in &identities.strata {
            assert!(!si.key.is_empty(), "{label}: empty stratum key");
            assert!(!si.label.is_empty(), "{label}: empty stratum label");
            assert_eq!(si.quality, IdentityQuality::Structural);
        }

        for ai in &identities.allomorphs {
            assert!(!ai.key.is_empty(), "{label}: empty allomorph key");
            assert!(!ai.label.is_empty(), "{label}: empty allomorph label");
            assert_eq!(ai.quality, IdentityQuality::Structural);
        }

        // Guards against a fixture change silently emptying what this test actually exercises.
        assert!(
            identities
                .objects
                .iter()
                .any(|o| o.kind == ObjectKind::LexEntry),
            "{label}: no lex entries resolved"
        );
        assert!(
            identities
                .objects
                .iter()
                .any(|o| o.kind == ObjectKind::MorphRule),
            "{label}: no morph rules resolved"
        );
        assert!(
            !identities.allomorphs.is_empty(),
            "{label}: no allomorphs resolved"
        );
    }

    #[test]
    fn every_object_in_two_fixtures_resolves_non_empty_with_expected_quality() {
        let [a, b] = two_sample_grammars();
        assert_all_non_empty_and_expected_quality(&a, "metathesis-phase-isolation");
        assert_all_non_empty_and_expected_quality(&b, "truncate-morphotactic");
    }

    #[test]
    fn identities_are_stable_across_two_loads_of_the_same_grammar() {
        let fixtures = discover();
        let f = find_fixture(&fixtures, "languages", "metathesis-phase-isolation");
        let xml = f.load_grammar_xml();
        let g1 = crate::load(&xml).unwrap();
        let g2 = crate::load(&xml).unwrap();
        assert_eq!(resolve_all(&g1), resolve_all(&g2));
    }
}
