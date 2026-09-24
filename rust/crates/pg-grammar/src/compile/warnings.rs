use pg_snapshot::{ConversionIssue, Snapshot, Warning};

use super::issue_codes;

pub(super) fn from_issues(snapshot: &Snapshot, issues: &[ConversionIssue]) -> Vec<Warning> {
    let mut warnings = Vec::new();
    for issue in issues {
        let warning = from_issue(snapshot, issue);
        if let Some(index) = warnings
            .iter()
            .position(|existing| same_fact(existing, &warning))
        {
            if warning.message < warnings[index].message {
                warnings[index] = warning;
            }
        } else {
            warnings.push(warning);
        }
    }
    warnings
}

fn from_issue(snapshot: &Snapshot, issue: &ConversionIssue) -> Warning {
    if issue.code == issue_codes::ENVIRONMENT_INVALID {
        if let Some(source) = issue
            .source
            .as_ref()
            .filter(|source| source.kind == "PhEnvironment")
        {
            if let Some(environment) = snapshot
                .phonology
                .environments
                .iter()
                .find(|environment| environment.guid == source.id)
            {
                let name = if environment.name.is_empty() {
                    environment.representation.clone()
                } else {
                    environment.name.clone()
                };
                let mut warning = Warning::from_conversion_issue(issue);
                warning.message =
                    format!("Phonological environment '{name}' is invalid; check its expression.");
                warning.set_primary_subject_name(name.clone());
                return warning;
            }
        }
    }

    let mut warning = Warning::from_conversion_issue(issue);
    if let Some(source) = issue.source.as_ref() {
        let name = if issue.code == super::issues::SUBSTRATE_INFERRED_SEGMENT_WITH_FEATURE_RULE {
            source.id.strip_prefix("inferred:").map(str::to_string)
        } else {
            name_for_source(snapshot, source)
        }
        .or_else(|| fallback_name(issue.code.as_str()).map(str::to_string));
        if let Some(name) = name {
            warning.set_primary_subject_name(name.clone());
            match issue.code.as_str() {
                issue_codes::MSA_BUILD_FAILED => {
                    warning.message = format!(
                        "Grammatical analysis for '{name}' could not be imported; check its part of speech and features."
                    );
                }
                issue_codes::PHONEME_NO_REPRESENTATION => {
                    warning.message = format!("Phoneme '{name}' has no grapheme representation.");
                }
                issue_codes::PHONEME_NFD_COLLISION => {
                    warning.message = format!(
                        "Phoneme '{name}' has a representation that collides with an earlier phoneme or boundary marker."
                    );
                }
                issue_codes::PHONEME_FEATURE_UNRESOLVED => {
                    warning.message = format!(
                        "Phoneme '{name}' refers to a phonological feature or value that is not defined."
                    );
                }
                issue_codes::PHONEME_COMPLEX_FEATURE_UNSUPPORTED => {
                    warning.message = format!(
                        "Phoneme '{name}' has a complex feature value that the importer cannot represent."
                    );
                }
                issue_codes::PHON_COMPLEX_FEATURE_UNSUPPORTED => {
                    warning.message = format!(
                        "Complex phonological feature '{name}' is not supported and was ignored."
                    );
                }
                issue_codes::NATCLASS_SEGMENTS_MEMBER_UNRESOLVED => {
                    warning.message =
                        format!("Natural class '{name}' refers to a phoneme that is not defined.");
                }
                issue_codes::NATCLASS_FEATURE_CONSTRAINT_UNRESOLVED => {
                    warning.message = format!(
                        "Natural class '{name}' refers to a phonological feature or value that is not defined."
                    );
                }
                issue_codes::NATCLASS_COMPLEX_FEATURE_UNSUPPORTED => {
                    warning.message = format!(
                        "Natural class '{name}' has a complex phonological feature value that the importer cannot represent."
                    );
                }
                issue_codes::MSA_NO_ALLOMORPHS => {
                    warning.message = format!("Lexical entry '{name}' has no loadable allomorphs.");
                }
                issue_codes::MSA_NO_RULE_FORM_ALLOMORPHS => {
                    warning.message = format!(
                        "Grammatical analysis '{name}' has no allomorph that can serve as a rule form."
                    );
                }
                issue_codes::STEM_NAME_BUILD_FAILED => {
                    warning.message = format!(
                        "Stem name '{name}' could not be loaded; check its category and regions."
                    );
                }
                issue_codes::ALLOMORPH_MORPH_TYPE_UNSUPPORTED_AS_RULE_FORM => {
                    warning.message = format!(
                        "Allomorph '{name}' has a morph type that cannot be used as a rule form."
                    );
                }
                issue_codes::ALLOMORPH_MORPH_TYPE_UNSUPPORTED => {
                    warning.message = format!(
                        "Allomorph '{name}' has a morph type that cannot be loaded as an affix rule."
                    );
                }
                issue_codes::ALLOMORPH_NOT_RULE_FORM => {
                    let allomorph_name = if name.trim().is_empty() {
                        "Unnamed affix allomorph"
                    } else {
                        name.as_str()
                    };
                    warning.set_primary_subject_name(allomorph_name.to_string());
                    let entry = snapshot.lexicon.entries.iter().find(|entry| {
                        entry
                            .allomorphs
                            .iter()
                            .any(|allomorph| allomorph.guid == source.id)
                    });
                    if let Some(entry) = entry {
                        let entry_source = pg_snapshot::SourceRef {
                            kind: "LexEntry".to_string(),
                            id: entry.guid.clone(),
                        };
                        let entry_name = name_for_source(snapshot, &entry_source)
                            .filter(|entry_name| !entry_name.trim().is_empty())
                            .unwrap_or_else(|| "unnamed lexical entry".to_string());
                        warning.subjects.push(
                            pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::LexEntry)
                                .guid(entry.guid.clone())
                                .name(entry_name.clone()),
                        );
                        warning.message = format!(
                            "Affix allomorph in lexical entry '{entry_name}' has no non-empty form and cannot be used as an inflectional rule."
                        );
                    } else {
                        warning.message = format!(
                            "Affix allomorph '{allomorph_name}' has no non-empty form and cannot be used as an inflectional rule."
                        );
                    }
                }
                issue_codes::ALLOMORPH_REDUPLICATION_UNSUPPORTED => {
                    warning.message = format!(
                        "Allomorph '{name}' has a reduplication pattern that cannot be loaded as an affix rule."
                    );
                }
                issue_codes::ALLOMORPH_PROCESS_BUILD_FAILED => {
                    warning.message = format!(
                        "Affix allomorph '{name}' could not be imported; check its input and output mappings."
                    );
                }
                issue_codes::ALLOMORPH_INFLECTION_CLASS_UNRESOLVED => {
                    warning.message = format!(
                        "Allomorph '{name}' refers to an inflection class that is not defined."
                    );
                }
                issue_codes::ALLOMORPH_FEATURE_BUILD_FAILED => {
                    warning.message = format!(
                        "Allomorph '{name}' has morphosyntactic features that cannot be imported."
                    );
                }
                issue_codes::ALLOMORPH_ENVIRONMENT_BUILD_FAILED => {
                    warning.message = format!(
                        "Allomorph '{name}' could not be built in one of its phonological environments."
                    );
                }
                issue_codes::CIRCUMFIX_ENVIRONMENT_COMBINATION_SKIPPED => {
                    warning.message = format!(
                        "Circumfix half '{name}' has an environment combination that could not be loaded."
                    );
                }
                issue_codes::COMPOUND_RULE_BUILD_FAILED => {
                    warning.message = format!(
                        "Compound rule '{name}' could not be built; check its constituent categories."
                    );
                }
                issue_codes::RULE_BUILD_FAILED => {
                    warning.message = format!(
                        "Phonological rule '{name}' could not be loaded; check its structural description and change."
                    );
                }
                issue_codes::TEMPLATE_BUILD_FAILED => {
                    warning.message = format!(
                        "Affix template '{name}' could not be loaded; check its slots and inflectional affixes."
                    );
                }
                issue_codes::TEMPLATE_NO_SLOTS => {
                    warning.message = format!("Affix template '{name}' has no slots.");
                }
                issue_codes::TEMPLATE_SLOT_NO_RULES => {
                    warning.message =
                        format!("Affix template slot '{name}' has no loaded inflectional affixes.");
                }
                issue_codes::NULL_AFFIX_MPR_UNRESOLVED => {
                    warning.message = format!(
                        "Entry inflection type '{name}' refers to a morphophonological restriction that is not defined."
                    );
                }
                issue_codes::NULL_AFFIX_SYN_FS_FAILED => {
                    warning.message = format!(
                        "Entry inflection type '{name}' has inflection features that could not be loaded."
                    );
                }
                issue_codes::NULL_AFFIX_SEGMENT_FAILED => {
                    warning.message = format!(
                        "Entry inflection type '{name}' cannot be used to build its null-affix form; check its form and phoneme inventory."
                    );
                }
                issue_codes::ALLOMORPH_UNSEGMENTABLE => {
                    warning.message = format!(
                        "Allomorph '{name}' could not be segmented with this project's phonemes."
                    );
                }
                issue_codes::CIRCUMFIX_MISSING_HALF => {
                    warning.message = format!(
                        "Circumfix half '{name}' is missing its matching prefix or suffix form."
                    );
                }
                issue_codes::ENVIRONMENT_UNRESOLVED if source.kind == "MoForm" => {
                    warning.message = format!(
                        "Allomorph '{name}' refers to a phonological environment that is not defined."
                    );
                }
                issue_codes::SUBSTRATE_UNSEGMENTABLE_FORM => {
                    warning.message = format!(
                        "Allomorph '{name}' contains a character that is not defined as a phoneme or boundary marker."
                    );
                }
                issue_codes::SUBSTRATE_CLASSIFICATION_AMBIGUOUS => {
                    warning.message = format!(
                        "Allomorph '{name}' contains a character that cannot be classified as a phoneme or word boundary."
                    );
                }
                issue_codes::SUBSTRATE_POSITION_UNMAPPED => {
                    warning.message = format!(
                        "Allomorph '{name}' contains a decomposed character sequence that could not be aligned with the phoneme inventory."
                    );
                }
                issue_codes::UNSUPPORTED_CONSTRUCT => {
                    warning.message = format!(
                        "Allomorph '{name}' has a reduplication pattern that cannot be checked against the phoneme inventory."
                    );
                }
                super::issues::SUBSTRATE_INFERRED_SEGMENT_WITH_FEATURE_RULE => {
                    warning.message = format!(
                        "Inferred segment '{name}' has no authored feature values; check its phonological features and matching feature-based natural classes."
                    );
                }
                _ => {}
            }
        }
    }
    warning
}

pub(super) fn source_for_key(
    snapshot: &Snapshot,
    key: &pg_snapshot::InventoryKey,
) -> Option<pg_snapshot::SourceRef> {
    use pg_snapshot::{InventoryIdentity, InventoryKind};

    let InventoryIdentity::Object { guid } = &key.identity else {
        return None;
    };
    let kind = match key.kind {
        InventoryKind::Allomorph | InventoryKind::AffixProcess => "MoForm",
        InventoryKind::Entry => "LexEntry",
        InventoryKind::Sense => "LexSense",
        InventoryKind::Msa => snapshot.lexicon.entries.iter().find_map(|entry| {
            entry
                .msas
                .iter()
                .find(|msa| msa.guid() == guid)
                .map(|msa| match msa {
                    pg_snapshot::lexicon::Msa::Stem { .. } => "MoStemMsa",
                    pg_snapshot::lexicon::Msa::Inflectional { .. } => "MoInflAffMsa",
                    pg_snapshot::lexicon::Msa::Derivational { .. } => "MoDerivAffMsa",
                    pg_snapshot::lexicon::Msa::Unclassified { .. } => "MoUnclassifiedAffixMsa",
                })
        })?,
        InventoryKind::Environment => "PhEnvironment",
        InventoryKind::Phoneme => "PhPhoneme",
        InventoryKind::NaturalClass => "PhNaturalClass",
        InventoryKind::Template => "MoInflAffixTemplate",
        InventoryKind::TemplateSlot => "MoInflAffixSlot",
        InventoryKind::StemName => "MoStemName",
        InventoryKind::CompoundRule => "MoCompoundRule",
        InventoryKind::AllomorphCoOccurrence | InventoryKind::MorphemeCoOccurrence => {
            "MoAdhocProhib"
        }
        InventoryKind::PhonologicalRule => {
            snapshot
                .phonology
                .rules
                .iter()
                .find_map(|rule| match rule {
                    pg_snapshot::phonology::PhonologicalRule::Rewrite(rule)
                        if rule.guid == *guid =>
                    {
                        Some("PhRegularRule")
                    }
                    pg_snapshot::phonology::PhonologicalRule::Metathesis(rule)
                        if rule.guid == *guid =>
                    {
                        Some("PhMetathesisRule")
                    }
                    _ => None,
                })?
        }
        _ => return None,
    };
    Some(pg_snapshot::SourceRef {
        kind: kind.to_string(),
        id: guid.clone(),
    })
}

fn name_for_source(snapshot: &Snapshot, source: &pg_snapshot::SourceRef) -> Option<String> {
    use pg_snapshot::lexicon::Msa;
    match source.kind.as_str() {
        "MoForm" | "MoStemAllomorph" | "MoAffixAllomorph" | "MoAffixProcess" | "allomorph" => {
            snapshot
                .lexicon
                .entries
                .iter()
                .flat_map(|entry| &entry.allomorphs)
                .find(|allomorph| allomorph.guid == source.id)
                .and_then(|allomorph| {
                    super::best_ws(
                        &allomorph.forms,
                        snapshot
                            .project
                            .vernacular_writing_systems
                            .first()
                            .map(String::as_str),
                    )
                    .map(str::to_string)
                })
        }
        "LexEntry" => snapshot
            .lexicon
            .entries
            .iter()
            .find(|entry| entry.guid == source.id)
            .and_then(|entry| {
                super::best_ws(
                    &entry.citation_form,
                    snapshot
                        .project
                        .vernacular_writing_systems
                        .first()
                        .map(String::as_str),
                )
                .map(str::to_string)
            }),
        "PhEnvironment" => snapshot
            .phonology
            .environments
            .iter()
            .find(|environment| environment.guid == source.id)
            .map(|environment| {
                if environment.name.is_empty() {
                    environment.representation.clone()
                } else {
                    environment.name.clone()
                }
            }),
        "PhRegularRule" => snapshot.phonology.rules.iter().find_map(|rule| match rule {
            pg_snapshot::phonology::PhonologicalRule::Rewrite(rule) if rule.guid == source.id => {
                Some(rule.name.clone())
            }
            _ => None,
        }),
        "PhMetathesisRule" => snapshot.phonology.rules.iter().find_map(|rule| match rule {
            pg_snapshot::phonology::PhonologicalRule::Metathesis(rule)
                if rule.guid == source.id =>
            {
                Some(rule.name.clone())
            }
            _ => None,
        }),
        "PhPhoneme" => snapshot
            .phonology
            .phonemes
            .iter()
            .find(|phoneme| phoneme.guid == source.id)
            .map(|phoneme| {
                if phoneme.name.is_empty() {
                    super::best_ws(
                        &phoneme.representations,
                        snapshot
                            .project
                            .vernacular_writing_systems
                            .first()
                            .map(String::as_str),
                    )
                    .unwrap_or("unnamed phoneme")
                    .to_string()
                } else {
                    phoneme.name.clone()
                }
            }),
        "PhBdryMarker" => snapshot
            .phonology
            .boundary_markers
            .iter()
            .find(|marker| marker.guid == source.id)
            .map(|marker| {
                if marker.name.is_empty() {
                    super::best_ws(
                        &marker.representations,
                        snapshot
                            .project
                            .vernacular_writing_systems
                            .first()
                            .map(String::as_str),
                    )
                    .unwrap_or("unnamed boundary marker")
                    .to_string()
                } else {
                    marker.name.clone()
                }
            }),
        "PhNaturalClass" => {
            snapshot.phonology.natural_classes.iter().find_map(
                |natural_class| match natural_class {
                    pg_snapshot::phonology::NaturalClass::Segments { guid, name, .. }
                    | pg_snapshot::phonology::NaturalClass::Features { guid, name, .. }
                        if guid == &source.id =>
                    {
                        Some(if name.is_empty() {
                            "unnamed natural class".to_string()
                        } else {
                            name.clone()
                        })
                    }
                    _ => None,
                },
            )
        }
        "MoStemMsa" | "MoInflAffMsa" | "MoDerivAffMsa" | "MoUnclassifiedAffixMsa" => {
            snapshot.lexicon.entries.iter().find_map(|entry| {
                entry
                    .msas
                    .iter()
                    .find(|msa| msa.guid() == source.id)
                    .and_then(|msa| {
                        let _class = match msa {
                            Msa::Stem { .. } => "MoStemMsa",
                            Msa::Inflectional { .. } => "MoInflAffMsa",
                            Msa::Derivational { .. } => "MoDerivAffMsa",
                            Msa::Unclassified { .. } => "MoUnclassifiedAffixMsa",
                        };
                        super::best_ws(
                            &entry.citation_form,
                            snapshot
                                .project
                                .vernacular_writing_systems
                                .first()
                                .map(String::as_str),
                        )
                        .map(str::to_string)
                    })
            })
        }
        "MoInflAffixTemplate" => template_name(snapshot, &source.id, true),
        "MoInflAffixSlot" => template_name(snapshot, &source.id, false),
        "MoStemName" => snapshot
            .morphology
            .parts_of_speech
            .iter()
            .find_map(|pos| stem_name(pos, &source.id)),
        "LexEntryInflType" => snapshot
            .morphology
            .lex_entry_infl_types
            .iter()
            .find(|infl_type| infl_type.guid == source.id)
            .map(|infl_type| infl_type.name.clone()),
        "FsComplexFeature" => snapshot
            .feature_systems
            .phonological
            .complex_features
            .iter()
            .find(|feature| feature.guid == source.id)
            .map(|feature| feature.name.clone()),
        "MoCompoundRule" => snapshot
            .morphology
            .compound_rules
            .iter()
            .find(|rule| rule.guid() == source.id)
            .map(|rule| match rule {
                pg_snapshot::morphology::CompoundRule::Endocentric { name, .. }
                | pg_snapshot::morphology::CompoundRule::Exocentric { name, .. } => name.clone(),
            }),
        _ => None,
    }
}

fn stem_name(pos: &pg_snapshot::morphology::PartOfSpeech, guid: &str) -> Option<String> {
    pos.stem_names
        .iter()
        .find(|stem_name| stem_name.guid == guid)
        .map(|stem_name| stem_name.name.clone())
        .or_else(|| pos.children.iter().find_map(|child| stem_name(child, guid)))
}

fn fallback_name(code: &str) -> Option<&'static str> {
    match code {
        issue_codes::MSA_BUILD_FAILED | issue_codes::MSA_NO_RULE_FORM_ALLOMORPHS => {
            Some("unnamed grammatical analysis")
        }
        issue_codes::STEM_NAME_BUILD_FAILED => Some("unnamed stem name"),
        issue_codes::COMPOUND_RULE_BUILD_FAILED => Some("unnamed compound rule"),
        issue_codes::RULE_BUILD_FAILED | issue_codes::RULE_METATHESIS_UNSUPPORTED => {
            Some("unnamed phonological rule")
        }
        issue_codes::TEMPLATE_BUILD_FAILED | issue_codes::TEMPLATE_NO_SLOTS => {
            Some("unnamed affix template")
        }
        issue_codes::TEMPLATE_SLOT_NO_RULES => Some("unnamed affix template slot"),
        issue_codes::NULL_AFFIX_MPR_UNRESOLVED
        | issue_codes::NULL_AFFIX_SYN_FS_FAILED
        | issue_codes::NULL_AFFIX_SEGMENT_FAILED => Some("unnamed entry inflection type"),
        issue_codes::ALLOMORPH_ENVIRONMENT_BUILD_FAILED => Some("unnamed allomorph"),
        issue_codes::ALLOMORPH_NOT_RULE_FORM => Some("Unnamed affix allomorph"),
        issue_codes::CIRCUMFIX_ENVIRONMENT_COMBINATION_SKIPPED => Some("unnamed circumfix half"),
        _ => None,
    }
}

fn template_name(snapshot: &Snapshot, guid: &str, want_template: bool) -> Option<String> {
    fn find(
        pos: &pg_snapshot::morphology::PartOfSpeech,
        guid: &str,
        want_template: bool,
    ) -> Option<String> {
        for template in &pos.affix_templates {
            if want_template && template.guid == guid {
                return Some(template.name.clone());
            }
            if !want_template {
                for slot_guid in template.prefix_slots.iter().chain(&template.suffix_slots) {
                    if slot_guid == guid {
                        return pos
                            .affix_slots
                            .iter()
                            .find(|slot| slot.guid == guid)
                            .map(|slot| slot.name.clone());
                    }
                }
            }
        }
        pos.children
            .iter()
            .find_map(|child| find(child, guid, want_template))
    }
    snapshot
        .morphology
        .parts_of_speech
        .iter()
        .find_map(|pos| find(pos, guid, want_template))
}

fn same_fact(left: &Warning, right: &Warning) -> bool {
    left.same_fact_as(right)
}
