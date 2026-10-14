//! Exact resolution of authored FieldWorks MSA identities to compiled owners.

use pg_grammar_model::model::{Grammar, LexEntryId, MRuleId};
use pg_snapshot::FwClass;

/// Why an authored entry/MSA address cannot be resolved to exactly one compiled morpheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceIdentityError {
    /// The grammar contains no lexical entries.
    EmptyLexicon,
    /// The grammar has no retained FieldWorks source identity carriers.
    AuthoredIdentityUnavailable,
    /// No compiled source owner matches the requested entry GUID.
    UnknownEntry,
    /// The MSA GUID is not represented in the compiled grammar.
    UnknownMsa,
    /// The MSA exists but is not owned by the requested entry/class tuple.
    MsaNotOwnedByEntry,
    /// More than one compiled morpheme has the exact requested identity.
    AmbiguousIdentity,
    /// The FieldWorks class is not one of the four supported MSA classes.
    UnsupportedMsaClass,
}

/// Runtime owner selected by one exact authored entry/MSA/class identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedMorphemeOwner {
    LexicalEntry(LexEntryId),
    MorphologicalRule(MRuleId),
}

/// One exact compiled source morpheme identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedAuthoredMorpheme {
    pub owner: ResolvedMorphemeOwner,
}

/// Resolve a root entry addressed by its exact FieldWorks entry GUID and stem MSA GUID.
///
/// Phonology measurement and authored generation share this lookup so both commands enforce the
/// same entry, MSA, class, and owner checks.
pub fn resolve_fw_entry_msa(
    grammar: &Grammar,
    entry_guid: &str,
    msa_guid: &str,
    msa_class: FwClass,
) -> Result<LexEntryId, SourceIdentityError> {
    if grammar.entries.is_empty() {
        return Err(SourceIdentityError::EmptyLexicon);
    }
    if !has_authored_identity(grammar) {
        return Err(SourceIdentityError::AuthoredIdentityUnavailable);
    }
    if !grammar
        .entries
        .iter()
        .any(|entry| entry.source_guid.as_deref() == Some(entry_guid))
    {
        return Err(SourceIdentityError::UnknownEntry);
    }
    if msa_class != FwClass::MoStemMsa {
        return Err(SourceIdentityError::MsaNotOwnedByEntry);
    }
    match resolve_authored_morpheme(grammar, entry_guid, msa_guid, msa_class)?.owner {
        ResolvedMorphemeOwner::LexicalEntry(entry) => Ok(entry),
        ResolvedMorphemeOwner::MorphologicalRule(_) => Err(SourceIdentityError::MsaNotOwnedByEntry),
    }
}

/// Resolve any supported FieldWorks MSA to one exact lexical entry or affix rule owner.
///
/// Stem MSAs resolve through `LexEntryDef.source_guid`; affix MSAs resolve through the source
/// entry owner retained on the rule's `MorphemeInfo`. The MSA GUID, MSA class, and owner entry
/// must all match. Headwords, XML morph IDs, and compiled vector positions are not source keys.
pub fn resolve_authored_morpheme(
    grammar: &Grammar,
    entry_guid: &str,
    msa_guid: &str,
    msa_class: FwClass,
) -> Result<ResolvedAuthoredMorpheme, SourceIdentityError> {
    use SourceIdentityError as Error;

    let stem = msa_class == FwClass::MoStemMsa;
    let affix = matches!(
        msa_class,
        FwClass::MoInflAffMsa | FwClass::MoDerivAffMsa | FwClass::MoUnclassifiedAffixMsa
    );
    if !stem && !affix {
        return Err(Error::UnsupportedMsaClass);
    }
    if !has_authored_identity(grammar) {
        return Err(Error::AuthoredIdentityUnavailable);
    }

    let entry_exists = grammar
        .entries
        .iter()
        .any(|entry| entry.source_guid.as_deref() == Some(entry_guid))
        || grammar.morphemes.iter().any(|info| {
            info.owner
                .as_ref()
                .is_some_and(|owner| owner.entry_guid == entry_guid)
        });
    if !entry_exists {
        return Err(Error::UnknownEntry);
    }

    let mut matches = Vec::new();
    if stem {
        for (index, entry) in grammar.entries.iter().enumerate() {
            if entry.source_guid.as_deref() != Some(entry_guid) {
                continue;
            }
            let Some(info) = grammar.morphemes.get(entry.morpheme.0 as usize) else {
                continue;
            };
            if info.source_msa_guid.as_deref() == Some(msa_guid)
                && info.source_msa_class == Some(msa_class)
                && owner_matches(info, entry_guid)
            {
                matches.push(ResolvedMorphemeOwner::LexicalEntry(LexEntryId(
                    index as u32,
                )));
            }
        }
    } else {
        for (index, rule) in grammar.mrules.iter().enumerate() {
            let Some(morpheme) = rule.morpheme() else {
                continue;
            };
            let Some(info) = grammar.morphemes.get(morpheme.0 as usize) else {
                continue;
            };
            if info.source_msa_guid.as_deref() == Some(msa_guid)
                && info.source_msa_class == Some(msa_class)
                && owner_matches(info, entry_guid)
            {
                matches.push(ResolvedMorphemeOwner::MorphologicalRule(MRuleId(
                    index as u32,
                )));
            }
        }
    }

    match matches.as_slice() {
        [owner] => Ok(ResolvedAuthoredMorpheme { owner: *owner }),
        [_, _, ..] => Err(Error::AmbiguousIdentity),
        [] if grammar
            .morphemes
            .iter()
            .any(|info| info.source_msa_guid.as_deref() == Some(msa_guid)) =>
        {
            Err(Error::MsaNotOwnedByEntry)
        }
        [] => Err(Error::UnknownMsa),
    }
}

fn owner_matches(info: &pg_grammar_model::model::MorphemeInfo, entry_guid: &str) -> bool {
    info.owner
        .as_ref()
        .is_some_and(|owner| owner.entry_guid == entry_guid)
}

fn has_authored_identity(grammar: &Grammar) -> bool {
    grammar
        .entries
        .iter()
        .any(|entry| entry.source_guid.is_some())
        || grammar
            .morphemes
            .iter()
            .any(|info| info.source_msa_guid.is_some() || info.owner.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_grammar_model::model::MorphemeOwner;

    const ROOT_ENTRY: &str = "00000000-0000-0000-0000-000000000030";
    const ROOT_MSA: &str = "00000000-0000-0000-0000-000000000032";
    const OTHER_ENTRY: &str = "00000000-0000-0000-0000-000000000040";
    const OTHER_MSA: &str = "00000000-0000-0000-0000-000000000042";
    const AFFIX_ENTRY: &str = "00000000-0000-0000-0000-000000000050";
    const AFFIX_MSA: &str = "00000000-0000-0000-0000-000000000052";

    fn fixture() -> Grammar {
        let snapshot = pg_snapshot::Snapshot::from_json(include_str!(
            "../../../../docs/divergences/evidence/071/kat-snapshot.json"
        ))
        .expect("committed synthetic snapshot parses");
        pg_grammar::compile_project(&snapshot)
            .expect("committed synthetic snapshot compiles")
            .0
    }

    #[test]
    fn authored_morpheme_identity_resolves_exact_entry_and_affix_owners() {
        let grammar = fixture();

        let root = resolve_fw_entry_msa(&grammar, ROOT_ENTRY, ROOT_MSA, FwClass::MoStemMsa)
            .expect("root identity resolves");
        assert_eq!(root, LexEntryId(0));

        let other_root =
            resolve_authored_morpheme(&grammar, OTHER_ENTRY, OTHER_MSA, FwClass::MoStemMsa)
                .expect("additional stem identity resolves");
        assert_eq!(
            other_root.owner,
            ResolvedMorphemeOwner::LexicalEntry(LexEntryId(1))
        );

        let affix =
            resolve_authored_morpheme(&grammar, AFFIX_ENTRY, AFFIX_MSA, FwClass::MoInflAffMsa)
                .expect("affix identity resolves");
        let rule = grammar
            .mrules
            .iter()
            .position(|rule| matches!(rule, pg_grammar_model::model::MorphRuleDef::AffixProcess(_)))
            .expect("fixture includes an affix process rule");
        assert_eq!(
            affix.owner,
            ResolvedMorphemeOwner::MorphologicalRule(MRuleId(rule as u32))
        );
    }

    #[test]
    fn authored_morpheme_identity_refuses_foreign_owner_and_ambiguity() {
        let mut grammar = fixture();
        assert_eq!(
            resolve_authored_morpheme(&grammar, ROOT_ENTRY, OTHER_MSA, FwClass::MoStemMsa),
            Err(SourceIdentityError::MsaNotOwnedByEntry)
        );

        grammar.entries[1].source_guid = Some(ROOT_ENTRY.into());
        let morpheme = grammar.entries[1].morpheme;
        grammar.morphemes[morpheme.0 as usize].source_msa_guid = Some(ROOT_MSA.into());
        grammar.morphemes[morpheme.0 as usize].source_msa_class = Some(FwClass::MoStemMsa);
        grammar.morphemes[morpheme.0 as usize].owner = Some(MorphemeOwner {
            entry_guid: ROOT_ENTRY.into(),
            keys_by_infl_type: false,
        });
        assert_eq!(
            resolve_fw_entry_msa(&grammar, ROOT_ENTRY, ROOT_MSA, FwClass::MoStemMsa),
            Err(SourceIdentityError::AmbiguousIdentity)
        );
    }
}
