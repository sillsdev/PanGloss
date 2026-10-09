//! Stable identity resolution for the `--stats` collector: maps each runtime id in a compiled
//! `Grammar` to the same output key the compiled-output table publishes, so a stats row joins to
//! its compiled object by `output_id`.
//!
//! Keys for compiled objects come from [`crate::compile::CompiledOutputs`]. Only the synthetic
//! pseudo-objects (the shared root trie, the guesser, the supplied-roots overlay) and the stratum
//! locator are fabricated here.

use crate::compile::{compiled_outputs, CompiledOutput, CompiledOutputs};
use crate::model::{
    AllomorphId, AllomorphOwner, Grammar, LexEntryId, MRuleId, MorphRuleDef, MorphemeId, PRuleId,
    PhonRuleDef, StratumId,
};

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

/// An allomorph's identity: its compiled output key, or the synthetic guessed-root sentinel.
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
    phon_rules: Vec<ObjectIdentity>,
    allomorphs: Vec<AllomorphIdentity>,
}

impl StatsIdentityCatalog {
    /// Builds the mappings once so per-word stats rows do not rescan the grammar.
    pub fn new(grammar: &Grammar) -> Self {
        let outputs = compiled_outputs(grammar);
        let morph_rules = (0..grammar.mrules.len())
            .map(|index| {
                let output = outputs.output(outputs.mrule(MRuleId(index as u32)));
                object_identity(
                    ObjectKind::MorphRule,
                    output,
                    morph_rule_label(grammar, index),
                )
            })
            .collect();
        let lex_entries = (0..grammar.entries.len())
            .map(|index| {
                let output = outputs.output(outputs.entry(LexEntryId(index as u32)));
                let label = lex_entry_label(grammar, index);
                object_identity(ObjectKind::LexEntry, output, label)
            })
            .collect();
        let phon_rules = (0..grammar.prules.len())
            .map(|index| {
                let output = outputs.output(outputs.prule(PRuleId(index as u32)));
                object_identity(
                    ObjectKind::PhonRule,
                    output,
                    phon_rule_label(grammar, index),
                )
            })
            .collect();
        let allomorphs = grammar
            .allomorph_owners
            .iter()
            .enumerate()
            .map(|(index, owner)| allomorph_identity_for(&outputs, grammar, index, *owner))
            .collect();
        Self {
            morph_rules,
            lex_entries,
            phon_rules,
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

    /// Returns the identity for a compiled phonological rule.
    pub fn phon_rule(&self, id: PRuleId) -> &ObjectIdentity {
        &self.phon_rules[id.0 as usize]
    }

    /// Returns a compiled allomorph identity or the synthetic guessed-root sentinel.
    pub fn allomorph(&self, id: AllomorphId) -> AllomorphIdentity {
        registered_allomorph(&self.allomorphs, id)
            .cloned()
            .unwrap_or_else(guessed_allomorph_identity)
    }
}

fn quality_of(text: &str) -> IdentityQuality {
    match text {
        "authored" => IdentityQuality::Authored,
        "synthetic" => IdentityQuality::Synthetic,
        _ => IdentityQuality::Structural,
    }
}

fn object_identity(kind: ObjectKind, output: &CompiledOutput, label: String) -> ObjectIdentity {
    ObjectIdentity {
        key: output.key.clone(),
        kind,
        label: if label.is_empty() {
            output.key.clone()
        } else {
            label
        },
        quality: quality_of(output.identity_quality),
    }
}

fn morph_rule_label(grammar: &Grammar, index: usize) -> String {
    let def = &grammar.mrules[index];
    let name = match def {
        MorphRuleDef::Compounding(c) => c.name.as_deref(),
        MorphRuleDef::AffixProcess(a) => a.name.as_deref(),
        MorphRuleDef::Realizational(r) => r.name.as_deref(),
    };
    name.map(str::to_string).unwrap_or_default()
}

fn phon_rule_label(grammar: &Grammar, index: usize) -> String {
    match &grammar.prules[index] {
        PhonRuleDef::Rewrite(r) => r.name.clone(),
        PhonRuleDef::Metathesis(m) => m.name.clone(),
    }
    .unwrap_or_default()
}

fn lex_entry_label(grammar: &Grammar, index: usize) -> String {
    grammar
        .morphemes
        .get(grammar.entries[index].morpheme.0 as usize)
        .and_then(|m| m.gloss.clone())
        .filter(|g| !g.is_empty())
        .unwrap_or_default()
}

fn allomorph_identity_for(
    outputs: &CompiledOutputs,
    grammar: &Grammar,
    index: usize,
    owner: AllomorphOwner,
) -> AllomorphIdentity {
    let id = AllomorphId(index as u32);
    let output_id = outputs
        .allomorph(id)
        .unwrap_or_else(|| panic!("allomorph id {} has no compiled output", id.0));
    let output = outputs.output(output_id);
    let (owner_label, position) = match owner {
        AllomorphOwner::Root(entry, position) => {
            (lex_entry_label(grammar, entry.0 as usize), position)
        }
        AllomorphOwner::Affix(rule, position) => {
            (morph_rule_label(grammar, rule.0 as usize), position)
        }
    };
    let label = if owner_label.is_empty() {
        format!("allomorph {position}")
    } else {
        format!("{owner_label} allomorph {position}")
    };
    AllomorphIdentity {
        key: output.key.clone(),
        label,
        quality: IdentityQuality::Structural,
    }
}

/// Resolves a registered allomorph, returning `None` only for the guessed-root sentinel.
/// An unknown ordinary ID is an invariant violation and panics with that ID.
pub(crate) fn registered_allomorph<T>(registry: &[T], id: AllomorphId) -> Option<&T> {
    if id == AllomorphId::GUESSED {
        return None;
    }
    Some(
        registry
            .get(id.0 as usize)
            .unwrap_or_else(|| panic!("allomorph id {} has no registry owner", id.0)),
    )
}

pub(crate) fn guessed_allomorph_identity() -> AllomorphIdentity {
    AllomorphIdentity {
        key: "guesser#allo".to_string(),
        label: "guessed root allomorph".to_string(),
        quality: IdentityQuality::Synthetic,
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
