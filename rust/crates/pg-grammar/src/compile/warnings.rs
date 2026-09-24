use pg_snapshot::{ConversionIssue, ImportWarningCode, Snapshot, Warning};

use super::issue_codes;

pub(super) fn from_issues(snapshot: &Snapshot, issues: &[ConversionIssue]) -> Vec<Warning> {
    issues
        .iter()
        .map(|issue| from_issue(snapshot, issue))
        .collect()
}

pub(super) fn deduplicate(warnings: impl IntoIterator<Item = Warning>) -> Vec<Warning> {
    let mut unique = Vec::new();
    for warning in warnings {
        if unique.iter().any(|existing| same_fact(existing, &warning)) {
            // Preserve the caller's preferred wording while retaining stable first-seen order.
        } else {
            unique.push(warning);
        }
    }
    unique
}

pub(super) fn from_issue(snapshot: &Snapshot, issue: &ConversionIssue) -> Warning {
    if issue.code == issue_codes::ENVIRONMENT_INVALID {
        if let Some(source) = issue
            .source
            .as_ref()
            .filter(|source| source.kind == pg_snapshot::FwClass::PhEnvironment)
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
                warning.message = issue
                    .message
                    .strip_prefix("environment validation failed: ")
                    .map(|cause| {
                        format!(
                            "Phonological environment '{name}' is invalid: {}.",
                            cause.trim_end_matches('.')
                        )
                    })
                    .unwrap_or_else(|| {
                        format!(
                            "Phonological environment '{name}' is invalid; check its expression."
                        )
                    });
                warning.set_primary_subject_name(name.clone());
                return warning;
            }
        }
    }

    let mut warning = Warning::from_conversion_issue(issue);
    if let Some(source) = issue.source.as_ref() {
        let inferred_segment =
            issue.code == super::issues::SUBSTRATE_INFERRED_SEGMENT_WITH_FEATURE_RULE;
        let name = if inferred_segment {
            source.id.strip_prefix("inferred:").map(str::to_string)
        } else {
            name_for_source(snapshot, source)
                .filter(|name| !name.trim().is_empty())
                .or_else(|| {
                    Some(
                        pg_snapshot::warning_metadata::fieldworks_missing_name_fallback(
                            source.kind,
                        )
                        .to_string(),
                    )
                })
        };
        if let Some(name) = name {
            warning.set_primary_subject_name(name.clone());
            if inferred_segment {
                if let Some(subject) = warning.subjects.first_mut() {
                    subject.guid = None;
                }
            }
            match issue.code.clone() {
                ImportWarningCode::MsaBuildFailed => {
                    warning.message = format!(
                        "Grammatical analysis for '{name}' could not be imported; check its part of speech and features."
                    );
                }
                ImportWarningCode::PhonemeNoRepresentation => {
                    warning.message = format!("Phoneme '{name}' has no grapheme representation.");
                }
                ImportWarningCode::PhonemeNfdCollision => {
                    warning.message = match other_phoneme_name_for_collision(snapshot, source) {
                        Some(other_name) => format!(
                            "Phoneme '{name}' has a representation that collides with phoneme '{other_name}'."
                        ),
                        None => format!(
                            "Phoneme '{name}' has a representation that collides with an earlier phoneme or boundary marker."
                        ),
                    };
                }
                ImportWarningCode::PhonemeFeatureUnresolved => {
                    warning.message = format!(
                        "Phoneme '{name}' refers to a phonological feature or value that is not defined."
                    );
                }
                ImportWarningCode::PhonemeComplexFeatureUnsupported => {
                    warning.message = format!(
                        "Phoneme '{name}' has a complex feature value that the importer cannot represent."
                    );
                }
                ImportWarningCode::PhonComplexFeatureUnsupported => {
                    warning.message = format!(
                        "Complex phonological feature '{name}' is not supported and was ignored."
                    );
                }
                ImportWarningCode::NatclassSegmentsMemberUnresolved => {
                    warning.message =
                        format!("Natural class '{name}' refers to a phoneme that is not defined.");
                }
                ImportWarningCode::NatclassFeatureConstraintUnresolved => {
                    warning.message = format!(
                        "Natural class '{name}' refers to a phonological feature or value that is not defined."
                    );
                }
                ImportWarningCode::NatclassComplexFeatureUnsupported => {
                    warning.message = format!(
                        "Natural class '{name}' has a complex phonological feature value that the importer cannot represent."
                    );
                }
                ImportWarningCode::MsaNoAllomorphs => {
                    warning.message = format!("Lexical entry '{name}' has no usable allomorphs.");
                }
                ImportWarningCode::MsaNoRuleFormAllomorphs => {
                    warning.message = format!(
                        "Grammatical analysis '{name}' has no affix form FieldWorks can use."
                    );
                }
                ImportWarningCode::StemNameBuildFailed => {
                    warning.message = format!(
                        "Stem name '{name}' could not be loaded; check its category and regions."
                    );
                }
                ImportWarningCode::AllomorphMorphTypeUnsupportedAsRuleForm => {
                    warning.message = format!(
                        "Allomorph '{name}' has a morph type that cannot be used by this affix analysis."
                    );
                }
                ImportWarningCode::AllomorphMorphTypeUnsupported => {
                    warning.message = format!(
                        "Allomorph '{name}' has a morph type that cannot be loaded as an affix rule."
                    );
                }
                ImportWarningCode::AllomorphNotRuleForm => {
                    let allomorph_name = if name.trim().is_empty() {
                        pg_snapshot::warning_metadata::fieldworks_missing_name_fallback(
                            pg_snapshot::FwClass::MoForm,
                        )
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
                            kind: pg_snapshot::FwClass::LexEntry,
                            id: entry.guid.clone(),
                        };
                        let entry_name = name_for_source(snapshot, &entry_source)
                            .filter(|entry_name| !entry_name.trim().is_empty())
                            .unwrap_or_else(|| {
                                pg_snapshot::warning_metadata::fieldworks_missing_name_fallback(
                                    pg_snapshot::FwClass::LexEntry,
                                )
                                .to_string()
                            });
                        warning.subjects.push(
                            pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::LexEntry)
                                .guid(entry.guid.clone())
                                .name(entry_name.clone()),
                        );
                        warning.message = format!(
                            "Affix allomorph in lexical entry '{entry_name}' has no form to use in this affix analysis."
                        );
                    } else {
                        warning.message = format!(
                            "Affix allomorph '{allomorph_name}' has no form to use in this affix analysis."
                        );
                    }
                }
                ImportWarningCode::AllomorphReduplicationUnsupported => {
                    warning.message = format!(
                        "Allomorph '{name}' has a reduplication pattern that cannot be loaded as an affix rule."
                    );
                }
                ImportWarningCode::AllomorphProcessBuildFailed => {
                    warning.message = format!(
                        "Affix allomorph '{name}' could not be imported; check its input and output mappings."
                    );
                }
                ImportWarningCode::AllomorphInflectionClassUnresolved => {
                    warning.message = format!(
                        "Allomorph '{name}' refers to an inflection class that is not defined."
                    );
                }
                ImportWarningCode::AllomorphFeatureBuildFailed => {
                    warning.message = format!(
                        "Allomorph '{name}' has morphosyntactic features that cannot be imported."
                    );
                }
                ImportWarningCode::AllomorphEnvironmentBuildFailed => {
                    warning.message = format!(
                        "Allomorph '{name}' could not be built in one of its phonological environments."
                    );
                }
                ImportWarningCode::CircumfixEnvironmentCombinationSkipped => {
                    warning.message = format!(
                        "Circumfix half '{name}' has an environment combination that could not be loaded."
                    );
                }
                ImportWarningCode::CompoundRuleBuildFailed => {
                    warning.message = format!(
                        "Compound rule '{name}' could not be built; check its constituent categories."
                    );
                }
                ImportWarningCode::RuleBuildFailed => {
                    warning.message = format!(
                        "Phonological rule '{name}' could not be loaded; check its structural description and change."
                    );
                }
                ImportWarningCode::TemplateBuildFailed => {
                    warning.message = format!(
                        "Affix template '{name}' could not be loaded; check its slots and inflectional affixes."
                    );
                }
                ImportWarningCode::TemplateNoSlots => {
                    warning.message = format!("Affix template '{name}' has no slots.");
                }
                ImportWarningCode::TemplateSlotNoRules => {
                    warning.message =
                        format!("Affix template slot '{name}' has no loaded inflectional affixes.");
                }
                ImportWarningCode::NullAffixMprUnresolved => {
                    warning.message = format!(
                        "Entry inflection type '{name}' refers to a morphophonological restriction that is not defined."
                    );
                }
                ImportWarningCode::NullAffixSynFsFailed => {
                    warning.message = format!(
                        "Entry inflection type '{name}' has inflection features that could not be loaded."
                    );
                }
                ImportWarningCode::NullAffixSegmentFailed => {
                    warning.message = format!(
                        "Entry inflection type '{name}' cannot be used to build its null-affix form; check its form and phoneme inventory."
                    );
                }
                ImportWarningCode::AllomorphUnsegmentable => {
                    warning.message = format!(
                        "Allomorph '{name}' could not be segmented with this project's phonemes."
                    );
                }
                ImportWarningCode::CircumfixMissingHalf => {
                    if source.kind != pg_snapshot::FwClass::LexEntry {
                        warning.message = format!(
                            "Circumfix half '{name}' is missing its matching prefix or suffix form."
                        );
                    }
                }
                issue_codes::ENVIRONMENT_UNRESOLVED
                    if source.kind == pg_snapshot::FwClass::MoForm =>
                {
                    warning.message = format!(
                        "Allomorph '{name}' refers to a phonological environment that is not defined."
                    );
                }
                ImportWarningCode::SubstrateUnsegmentableForm => {
                    warning.message = format!(
                        "Allomorph '{name}' contains a character that is not defined as a phoneme or boundary marker."
                    );
                }
                ImportWarningCode::SubstrateClassificationAmbiguous => {
                    warning.message = format!(
                        "Allomorph '{name}' contains a character that cannot be classified as a phoneme or word boundary."
                    );
                }
                ImportWarningCode::SubstratePositionUnmapped => {
                    warning.message = format!(
                        "Allomorph '{name}' contains a character sequence that does not match a project phoneme or boundary marker."
                    );
                }
                ImportWarningCode::UnsupportedConstruct => {
                    warning.message = format!(
                        "Allomorph '{name}' has a reduplication pattern that cannot be checked against the phoneme inventory."
                    );
                }
                ImportWarningCode::MigrationInferredSegmentWithFeatureRule => {
                    warning.message = format!(
                        "Character '{name}' is used in an allomorph but is not a project phoneme. Add it in {} before assigning its phonological feature values.",
                        pg_snapshot::fieldworks_paths::GRAMMAR_PHONEMES
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
        InventoryKind::Entry | InventoryKind::EntryReference => pg_snapshot::FwClass::LexEntry,
        InventoryKind::Sense => pg_snapshot::FwClass::LexSense,
        InventoryKind::Msa => snapshot
            .lexicon
            .entries
            .iter()
            .flat_map(|entry| &entry.msas)
            .find(|msa| msa.guid() == guid)
            .map(pg_snapshot::lexicon::Msa::fw_class)?,
        InventoryKind::Allomorph | InventoryKind::AffixProcess => pg_snapshot::FwClass::MoForm,
        InventoryKind::Environment => pg_snapshot::FwClass::PhEnvironment,
        InventoryKind::Phoneme => pg_snapshot::FwClass::PhPhoneme,
        InventoryKind::BoundaryMarker => pg_snapshot::FwClass::PhBdryMarker,
        InventoryKind::NaturalClass => pg_snapshot::FwClass::PhNaturalClass,
        InventoryKind::FeatureDefinition => {
            let systems = [
                &snapshot.feature_systems.phonological,
                &snapshot.feature_systems.morphosyntactic,
            ];
            systems.iter().find_map(|system| {
                if system
                    .closed_features
                    .iter()
                    .any(|feature| feature.guid == *guid)
                {
                    Some(pg_snapshot::FwClass::FsClosedFeature)
                } else if system
                    .complex_features
                    .iter()
                    .any(|feature| feature.guid == *guid)
                {
                    Some(pg_snapshot::FwClass::FsComplexFeature)
                } else {
                    None
                }
            })?
        }
        InventoryKind::FeatureValue => snapshot
            .feature_systems
            .phonological
            .closed_features
            .iter()
            .chain(&snapshot.feature_systems.morphosyntactic.closed_features)
            .any(|feature| feature.values.iter().any(|value| value.guid == *guid))
            .then_some(pg_snapshot::FwClass::FsSymFeatVal)?,
        InventoryKind::Template => pg_snapshot::FwClass::MoInflAffixTemplate,
        InventoryKind::TemplateSlot => pg_snapshot::FwClass::MoInflAffixSlot,
        InventoryKind::StemName => pg_snapshot::FwClass::MoStemName,
        InventoryKind::InflectionClass => pg_snapshot::FwClass::MoInflClass,
        InventoryKind::CompoundRule => pg_snapshot::FwClass::MoCompoundRule,
        InventoryKind::AllomorphCoOccurrence | InventoryKind::MorphemeCoOccurrence => {
            pg_snapshot::FwClass::MoAdhocProhib
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
                        Some(pg_snapshot::FwClass::PhRegularRule)
                    }
                    pg_snapshot::phonology::PhonologicalRule::Metathesis(rule)
                        if rule.guid == *guid =>
                    {
                        Some(pg_snapshot::FwClass::PhMetathesisRule)
                    }
                    _ => None,
                })?
        }
        _ => return None,
    };
    Some(pg_snapshot::SourceRef {
        kind,
        id: guid.clone(),
    })
}

fn name_for_source(snapshot: &Snapshot, source: &pg_snapshot::SourceRef) -> Option<String> {
    match source.kind {
        pg_snapshot::FwClass::MoForm => snapshot
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
            }),
        pg_snapshot::FwClass::LexEntry => snapshot
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
        pg_snapshot::FwClass::PhEnvironment => snapshot
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
        pg_snapshot::FwClass::PhRegularRule => {
            snapshot.phonology.rules.iter().find_map(|rule| match rule {
                pg_snapshot::phonology::PhonologicalRule::Rewrite(rule)
                    if rule.guid == source.id =>
                {
                    Some(rule.name.clone())
                }
                _ => None,
            })
        }
        pg_snapshot::FwClass::PhMetathesisRule => {
            snapshot.phonology.rules.iter().find_map(|rule| match rule {
                pg_snapshot::phonology::PhonologicalRule::Metathesis(rule)
                    if rule.guid == source.id =>
                {
                    Some(rule.name.clone())
                }
                _ => None,
            })
        }
        pg_snapshot::FwClass::PhPhoneme => snapshot
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
                    .unwrap_or_else(|| {
                        pg_snapshot::warning_metadata::fieldworks_missing_name_fallback(
                            pg_snapshot::FwClass::PhPhoneme,
                        )
                    })
                    .to_string()
                } else {
                    phoneme.name.clone()
                }
            }),
        pg_snapshot::FwClass::PhBdryMarker => snapshot
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
                    .unwrap_or_else(|| {
                        pg_snapshot::warning_metadata::fieldworks_missing_name_fallback(
                            pg_snapshot::FwClass::PhBdryMarker,
                        )
                    })
                    .to_string()
                } else {
                    marker.name.clone()
                }
            }),
        pg_snapshot::FwClass::PhNaturalClass => snapshot.phonology.natural_classes.iter().find_map(
            |natural_class| match natural_class {
                pg_snapshot::phonology::NaturalClass::Segments { guid, name, .. }
                | pg_snapshot::phonology::NaturalClass::Features { guid, name, .. }
                    if guid == &source.id =>
                {
                    Some(if name.is_empty() {
                        pg_snapshot::warning_metadata::fieldworks_missing_name_fallback(
                            pg_snapshot::FwClass::PhNaturalClass,
                        )
                        .to_string()
                    } else {
                        name.clone()
                    })
                }
                _ => None,
            },
        ),
        pg_snapshot::FwClass::MoStemMsa
        | pg_snapshot::FwClass::MoInflAffMsa
        | pg_snapshot::FwClass::MoDerivAffMsa
        | pg_snapshot::FwClass::MoUnclassifiedAffixMsa => {
            snapshot.lexicon.entries.iter().find_map(|entry| {
                entry
                    .msas
                    .iter()
                    .find(|msa| msa.guid() == source.id && msa.fw_class() == source.kind)
                    .and_then(|_| {
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
        pg_snapshot::FwClass::MoInflAffixTemplate => template_name(snapshot, &source.id, true),
        pg_snapshot::FwClass::MoInflAffixSlot => template_name(snapshot, &source.id, false),
        pg_snapshot::FwClass::MoStemName => snapshot
            .morphology
            .parts_of_speech
            .iter()
            .find_map(|pos| stem_name(pos, &source.id)),
        pg_snapshot::FwClass::LexEntryInflType => snapshot
            .morphology
            .lex_entry_infl_types
            .iter()
            .find(|infl_type| infl_type.guid == source.id)
            .map(|infl_type| infl_type.name.clone()),
        pg_snapshot::FwClass::FsComplexFeature => snapshot
            .feature_systems
            .phonological
            .complex_features
            .iter()
            .find(|feature| feature.guid == source.id)
            .map(|feature| feature.name.clone()),
        pg_snapshot::FwClass::FsClosedFeature => snapshot
            .feature_systems
            .phonological
            .closed_features
            .iter()
            .chain(&snapshot.feature_systems.morphosyntactic.closed_features)
            .find(|feature| feature.guid == source.id)
            .map(|feature| feature.name.clone()),
        pg_snapshot::FwClass::FsSymFeatVal => snapshot
            .feature_systems
            .phonological
            .closed_features
            .iter()
            .chain(&snapshot.feature_systems.morphosyntactic.closed_features)
            .flat_map(|feature| &feature.values)
            .find(|value| value.guid == source.id)
            .map(|value| value.name.clone()),
        pg_snapshot::FwClass::MoInflClass => snapshot
            .morphology
            .parts_of_speech
            .iter()
            .find_map(|pos| inflection_class(pos, &source.id)),
        pg_snapshot::FwClass::MoAdhocProhib => adhoc_prohibition_name(snapshot, &source.id),
        pg_snapshot::FwClass::MoCompoundRule => snapshot
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

fn other_phoneme_name_for_collision(
    snapshot: &Snapshot,
    source: &pg_snapshot::SourceRef,
) -> Option<String> {
    let current_index = snapshot
        .phonology
        .phonemes
        .iter()
        .position(|phoneme| phoneme.guid == source.id)?;
    let current = &snapshot.phonology.phonemes[current_index];
    let preferred_ws = snapshot
        .project
        .vernacular_writing_systems
        .first()
        .map(String::as_str);
    let current_representations = super::ws_forms(&current.representations, preferred_ws);
    snapshot.phonology.phonemes[..current_index]
        .iter()
        .find(|previous| {
            let previous_representations = super::ws_forms(&previous.representations, preferred_ws);
            previous_representations.iter().any(|representation| {
                current_representations
                    .iter()
                    .any(|current| crate::nfd::nfd(current) == crate::nfd::nfd(representation))
            })
        })
        .map(|previous| {
            if previous.name.trim().is_empty() {
                super::best_ws(&previous.representations, preferred_ws)
                    .unwrap_or_else(|| {
                        pg_snapshot::warning_metadata::fieldworks_missing_name_fallback(
                            pg_snapshot::FwClass::PhPhoneme,
                        )
                    })
                    .to_string()
            } else {
                previous.name.clone()
            }
        })
}

fn stem_name(pos: &pg_snapshot::morphology::PartOfSpeech, guid: &str) -> Option<String> {
    pos.stem_names
        .iter()
        .find(|stem_name| stem_name.guid == guid)
        .map(|stem_name| stem_name.name.clone())
        .or_else(|| pos.children.iter().find_map(|child| stem_name(child, guid)))
}

fn inflection_class(pos: &pg_snapshot::morphology::PartOfSpeech, guid: &str) -> Option<String> {
    pos.inflection_classes
        .iter()
        .find(|class| class.guid == guid)
        .map(|class| class.name.clone())
        .or_else(|| {
            pos.children
                .iter()
                .find_map(|child| inflection_class(child, guid))
        })
}

fn adhoc_prohibition_name(snapshot: &Snapshot, guid: &str) -> Option<String> {
    let primary =
        snapshot.morphology.adhoc_prohibitions.iter().find_map(
            |prohibition| match prohibition {
                pg_snapshot::morphology::AdhocProhibition::Allomorph {
                    guid: prohibition_guid,
                    primary,
                    ..
                }
                | pg_snapshot::morphology::AdhocProhibition::Morpheme {
                    guid: prohibition_guid,
                    primary,
                    ..
                } if prohibition_guid == guid => Some(primary.as_str()),
                _ => None,
            },
        )?;
    snapshot.lexicon.entries.iter().find_map(|entry| {
        let refers_to_primary = entry
            .allomorphs
            .iter()
            .any(|allomorph| allomorph.guid == primary)
            || entry.msas.iter().any(|msa| msa.guid() == primary);
        refers_to_primary.then(|| {
            super::best_ws(
                &entry.citation_form,
                snapshot
                    .project
                    .vernacular_writing_systems
                    .first()
                    .map(String::as_str),
            )
            .map(str::to_string)
        })?
    })
}

fn template_name(snapshot: &Snapshot, guid: &str, want_template: bool) -> Option<String> {
    fn find(
        pos: &pg_snapshot::morphology::PartOfSpeech,
        guid: &str,
        want_template: bool,
    ) -> Option<String> {
        for template in &pos.affix_templates {
            if want_template && template.guid == guid {
                return Some(if template.name.trim().is_empty() {
                    pg_snapshot::warning_metadata::fieldworks_missing_name_fallback(
                        pg_snapshot::FwClass::MoInflAffixTemplate,
                    )
                    .to_string()
                } else {
                    template.name.clone()
                });
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
