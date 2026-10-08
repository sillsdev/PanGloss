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
use std::collections::HashMap;

pub use pg_grammar_model::stats_identity::OverlayPhase;

/// A morpheme's locator identity, mirroring [`StratumIdentity`]: a morpheme is a dimension a
/// report groups lexical entries by, never a counted object on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorphemeIdentity {
    pub key: String,
    pub label: String,
    pub quality: IdentityQuality,
}

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

/// Identities for every runtime object in one compiled grammar.
#[derive(Debug, Clone)]
pub struct StatsIdentityCatalog {
    morph_rules: Vec<ObjectIdentity>,
    lex_entries: Vec<ObjectIdentity>,
    allomorphs: Vec<AllomorphIdentity>,
}

impl StatsIdentityCatalog {
    /// Builds the mappings once so per-word stats rows do not rescan the grammar.
    pub fn new(grammar: &Grammar) -> Self {
        let morph_rules = morph_rule_identities(grammar);
        let lex_entries = lex_entry_identities(grammar);
        let allomorphs = grammar
            .allomorph_owners
            .iter()
            .map(|owner| allomorph_identity_for_owner(*owner, &lex_entries, &morph_rules))
            .collect();
        Self {
            morph_rules,
            lex_entries,
            allomorphs,
        }
    }

    /// Returns the identity for a compiled morphological rule.
    pub fn morph_rule(&self, id: MRuleId) -> &ObjectIdentity {
        &self.morph_rules[id.0 as usize]
    }

    /// Returns the identity for a compiled lexical entry.
    pub fn lex_entry(&self, id: LexEntryId) -> &ObjectIdentity {
        &self.lex_entries[id.0 as usize]
    }

    /// Returns a compiled allomorph identity or the synthetic guessed-root sentinel.
    pub fn allomorph(&self, id: AllomorphId) -> AllomorphIdentity {
        self.allomorphs
            .get(id.0 as usize)
            .cloned()
            .unwrap_or_else(guessed_allomorph_identity)
    }
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
/// Reused authored keys receive structural ordinals so each runtime rule stays distinct.
pub fn morph_rule_identity(grammar: &Grammar, id: MRuleId) -> ObjectIdentity {
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

fn morph_rule_identities(grammar: &Grammar) -> Vec<ObjectIdentity> {
    let base = (0..grammar.mrules.len())
        .map(|index| morph_rule_key_and_quality(grammar, MRuleId(index as u32)))
        .collect::<Vec<_>>();
    let mut counts = HashMap::new();
    for (key, _) in &base {
        *counts.entry(key.clone()).or_insert(0usize) += 1;
    }
    base.into_iter()
        .enumerate()
        .map(|(index, (key, quality))| {
            let (key, quality) =
                disambiguate_morph_rule_key(&key, index as u32, counts[&key], quality);
            let def = &grammar.mrules[index];
            let label = morph_rule_name(def)
                .map(str::to_string)
                .unwrap_or_else(|| key.clone());
            ObjectIdentity {
                key,
                kind: ObjectKind::MorphRule,
                label,
                quality,
            }
        })
        .collect()
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

/// Resolve a lexical entry's stable identity. Duplicate source GUIDs include the MSA identity;
/// repeated entry/MSA pairs use a structural ordinal. The label prefers the MSA gloss.
pub fn lex_entry_identity(grammar: &Grammar, id: LexEntryId) -> ObjectIdentity {
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

fn lex_entry_identities(grammar: &Grammar) -> Vec<ObjectIdentity> {
    let mut authored_counts = HashMap::new();
    let mut morpheme_counts = HashMap::new();
    let morphemes = grammar
        .entries
        .iter()
        .map(|entry| morpheme_identity(grammar, entry.morpheme))
        .collect::<Vec<_>>();
    for (index, entry) in grammar.entries.iter().enumerate() {
        if entry.authored_id.trim().is_empty() {
            continue;
        }
        *authored_counts
            .entry(entry.authored_id.as_str())
            .or_insert(0usize) += 1;
        *morpheme_counts
            .entry((entry.authored_id.as_str(), morphemes[index].key.as_str()))
            .or_insert(0usize) += 1;
    }
    grammar
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let (key, quality) = if entry.authored_id.trim().is_empty() {
                (format!("lex_entry#{}", index), IdentityQuality::Structural)
            } else {
                lex_entry_authored_key(
                    &entry.authored_id,
                    &morphemes[index].key,
                    morphemes[index].quality,
                    index as u32,
                    authored_counts[entry.authored_id.as_str()],
                    morpheme_counts[&(entry.authored_id.as_str(), morphemes[index].key.as_str())],
                )
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
        })
        .collect()
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

/// Resolve an allomorph's structural locator from its owner's identity and index within it.
fn allomorph_identity_for_owner(
    owner: AllomorphOwner,
    lex_entries: &[ObjectIdentity],
    morph_rules: &[ObjectIdentity],
) -> AllomorphIdentity {
    let (owner_kind, owner_key, owner_label, index) = match owner {
        AllomorphOwner::Root(entry_id, idx) => {
            let owner_identity = &lex_entries[entry_id.0 as usize];
            (
                "lex_entry",
                owner_identity.key.clone(),
                owner_identity.label.clone(),
                idx,
            )
        }
        AllomorphOwner::Affix(mrule_id, idx) => {
            let owner_identity = &morph_rules[mrule_id.0 as usize];
            (
                "morph_rule",
                owner_identity.key.clone(),
                owner_identity.label.clone(),
                idx,
            )
        }
    };
    allomorph_identity_for_owner_fields(owner_kind, &owner_key, &owner_label, index)
}

fn allomorph_identity_for_owner_fields(
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

/// Resolve an allomorph's structural locator by its dense runtime id, via the grammar's
/// allomorph registry.
pub fn allomorph_identity(grammar: &Grammar, id: AllomorphId) -> AllomorphIdentity {
    match grammar.allomorph_owners.get(id.0 as usize) {
        Some(AllomorphOwner::Root(entry_id, index)) => {
            let owner = lex_entry_identity(grammar, *entry_id);
            allomorph_identity_for_owner_fields("lex_entry", &owner.key, &owner.label, *index)
        }
        Some(AllomorphOwner::Affix(rule_id, index)) => {
            let owner = morph_rule_identity(grammar, *rule_id);
            allomorph_identity_for_owner_fields("morph_rule", &owner.key, &owner.label, *index)
        }
        None => guessed_allomorph_identity(),
    }
}

fn guessed_allomorph_identity() -> AllomorphIdentity {
    AllomorphIdentity {
        key: "guesser#allo".to_string(),
        label: "guessed root allomorph".to_string(),
        quality: IdentityQuality::Synthetic,
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

/// The grammar-wide supplied-roots overlay pseudo-object, one per [`OverlayPhase`].
pub fn overlay_identity(_grammar: &Grammar, phase: OverlayPhase) -> ObjectIdentity {
    ObjectIdentity {
        key: format!("overlay:{}", phase.name()),
        kind: ObjectKind::Overlay,
        label: format!("supplied roots: {}", phase.name()),
        quality: IdentityQuality::Synthetic,
    }
}

/// Resolve a morpheme's locator identity, so a `lex_entry` report can group scattered entries and
/// allomorphs back to the single morpheme they realize. `MorphemeId::GUESSED` names no grammar row
/// (a fabricated root has no morpheme at all), so it resolves to a synthetic sentinel rather than
/// indexing `grammar.morphemes` out of bounds.
pub fn morpheme_identity(grammar: &Grammar, id: MorphemeId) -> MorphemeIdentity {
    if id == MorphemeId::GUESSED {
        return MorphemeIdentity {
            key: "morpheme#guessed".to_string(),
            label: "guessed morpheme".to_string(),
            quality: IdentityQuality::Synthetic,
        };
    }
    match grammar.morphemes.get(id.0 as usize) {
        Some(info) if !info.xml_key.is_empty() => MorphemeIdentity {
            label: info
                .gloss
                .clone()
                .filter(|g| !g.is_empty())
                .unwrap_or_else(|| info.xml_key.clone()),
            key: info.xml_key.clone(),
            quality: IdentityQuality::Authored,
        },
        Some(info) => MorphemeIdentity {
            key: format!("morpheme#{}", id.0),
            label: info
                .gloss
                .clone()
                .filter(|g| !g.is_empty())
                .unwrap_or_else(|| format!("morpheme#{}", id.0)),
            quality: IdentityQuality::Structural,
        },
        None => MorphemeIdentity {
            key: format!("morpheme#{}", id.0),
            label: format!("morpheme#{}", id.0),
            quality: IdentityQuality::Structural,
        },
    }
}

#[cfg(test)]
mod tests;
