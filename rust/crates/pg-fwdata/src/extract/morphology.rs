//! `morphology` snapshot section — see `docs/snapshot-format.md` §5.

use pg_snapshot::{
    AdhocProhibition, AdhocProhibitionGroup, Adjacency, AffixSlot, AffixTemplate,
    CompoundConstituentRequirement, CompoundOutcome, CompoundRule, ConversionIssue,
    ExceptionFeature, FeatureSystems, FwClass, FwObjectRef, InflectionClass, InventoryKey,
    InventoryKind, IssueClass, LexEntryInflType, Lexicon, Morphology, ParserParameters,
    PartOfSpeech, SourceRef, StemName,
};

use super::features::extract_feature_structure;
use super::Ctx;
use crate::xml::Record;
use crate::{parser_params, ImportError};

pub fn extract_morphology(
    ctx: &mut Ctx,
    lang_project: Option<&Record>,
    _feature_systems: &FeatureSystems,
    project_name: &str,
) -> Result<Morphology, ImportError> {
    let parts_of_speech = lang_project
        .and_then(|lp| {
            lp.node
                .objsur_one("PartsOfSpeech")
                .map(|list_guid| extract_pos_forest(ctx, &list_guid, lp))
        })
        .unwrap_or_default();
    // Every affix slot across the whole forest is represented by now, so a template's slot references — which may cross POS boundaries — can be checked in one pass over the finished tree.
    record_template_slot_attachments(ctx, &parts_of_speech);

    let morph_data = lang_project.and_then(|lp| {
        lp.node.objsur_one("MorphologicalData").and_then(|guid| {
            ctx.require_from(&guid, "MoMorphData", "morphology", lp, "MorphologicalData")
        })
    });

    let compound_rules = morph_data
        .map(|md| extract_compound_rules(ctx, md))
        .unwrap_or_default();
    let adhoc_prohibitions = morph_data
        .map(|md| extract_adhoc_prohibitions(ctx, md))
        .unwrap_or_default();
    let adhoc_prohibition_groups = Some(extract_adhoc_prohibition_groups(ctx));

    let phon_data = lang_project
        .and_then(|lp| lp.node.objsur_one("PhonologicalData"))
        .and_then(|guid| ctx.get(&guid));

    let exception_features = extract_exception_features(ctx, morph_data, phon_data);

    let lex_db = lang_project.and_then(|lp| {
        lp.node.objsur_one("LexDb").and_then(|guid| {
            ctx.require_from(&guid, "LexDb", "morphology.lexEntryInflTypes", lp, "LexDb")
        })
    });
    let lex_entry_infl_types = lex_db
        .map(|db| extract_lex_entry_infl_types(ctx, db))
        .unwrap_or_default();

    let parser_raw = morph_data.and_then(|md| {
        md.node.child("ParserParameters").map(|field| {
            field
                .child("Uni")
                .map(|uni| uni.text.clone())
                .unwrap_or_default()
        })
    });
    let (parser_parameters, parser_issues, settings_presence) =
        parser_params::parse_with_issues(parser_raw.as_deref())?;
    record_parser_settings(ctx, &parser_parameters, &settings_presence);
    ctx.warnings
        .extend(parser_issues.into_iter().map(|warning| {
            if warning.subjects.is_empty() {
                warning.with_subject(
                    FwObjectRef::new(FwClass::Project)
                        .name(project_name)
                        .project_settings(),
                )
            } else {
                let mut warning = warning;
                if let Some(subject) = warning.subjects.first_mut() {
                    subject.name = Some(project_name.to_string());
                }
                warning
            }
        }));

    Ok(Morphology {
        parts_of_speech,
        compound_rules,
        adhoc_prohibitions,
        adhoc_prohibition_groups,
        exception_features,
        lex_entry_infl_types,
        parser_parameters,
    })
}

/// Records each parser-parameter setting present in the source `<Uni>`; a malformed `XAmple` field is rejected using the warning `parser_params` already emitted, never a second one.
fn record_parser_settings(
    ctx: &mut Ctx,
    parsed: &ParserParameters,
    presence: &parser_params::ParserSettingsPresence,
) {
    if presence.active_parser {
        record_present_setting(ctx, "ActiveParser");
    }
    if presence.accept_unspecified_graphemes {
        record_present_setting(ctx, "AcceptUnspecifiedGraphemes");
    }
    for (field, parsed_ok) in &presence.xample_fields {
        let name = format!("XAmple.{field}");
        let key = InventoryKey::setting(InventoryKind::ParserSetting, name);
        ctx.authored(key.clone());
        ctx.considered(key.clone());
        ctx.selected(key.clone());
        if *parsed_ok {
            ctx.represented(key);
        } else {
            ctx.record_rejected(
                key,
                ConversionIssue {
                    code: super::codes::INVALID_PARSER_PARAMETER,
                    class: IssueClass::MalformedSource,
                    source: None,
                    fatal: false,
                    message: format!(
                        "morphology.parserParameters: XAmple {field} is present but malformed"
                    ),
                },
            );
        }
    }
    if parsed.strata.is_some() {
        record_present_setting_kind(ctx, InventoryKind::StrataConfiguration, "Strata");
    }
}

fn record_present_setting(ctx: &mut Ctx, name: &str) {
    record_present_setting_kind(ctx, InventoryKind::ParserSetting, name);
}

fn record_present_setting_kind(ctx: &mut Ctx, kind: InventoryKind, name: &str) {
    let key = InventoryKey::setting(kind, name.to_string());
    ctx.authored(key.clone());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    ctx.represented(key);
}

// Parts of speech

fn extract_pos_forest(ctx: &mut Ctx, list_guid: &str, owner: &Record) -> Vec<PartOfSpeech> {
    let Some(list) = ctx.require_from(
        list_guid,
        "CmPossibilityList",
        "morphology.partsOfSpeech",
        owner,
        "PartsOfSpeech",
    ) else {
        return Vec::new();
    };
    list.node
        .objsur_list("Possibilities")
        .into_iter()
        .filter_map(|g| extract_pos(ctx, &g, list, "Possibilities"))
        .collect()
}

fn extract_pos(ctx: &mut Ctx, guid: &str, owner: &Record, field: &str) -> Option<PartOfSpeech> {
    let rec = ctx.require_from(
        guid,
        "PartOfSpeech",
        "morphology.partsOfSpeech",
        owner,
        field,
    )?;
    let key = InventoryKey::object(InventoryKind::PartOfSpeech, guid.to_string());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    let name = ctx.best_analysis(&rec.node.ws_forms("Name"));
    let abbreviation = ctx.best_analysis(&rec.node.ws_forms("Abbreviation"));
    let children = rec
        .node
        .objsur_list("SubPossibilities")
        .into_iter()
        .filter_map(|g| extract_pos(ctx, &g, rec, "SubPossibilities"))
        .collect();
    let inflection_classes = rec
        .node
        .objsur_list("InflectionClasses")
        .into_iter()
        .filter_map(|g| extract_inflection_class(ctx, &g, rec, "InflectionClasses"))
        .collect();
    let default_inflection_class = rec.node.objsur_one("DefaultInflectionClass");
    let inflectable_features = rec.node.objsur_list("InflectableFeats");
    let stem_names = rec
        .node
        .objsur_list("StemNames")
        .into_iter()
        .filter_map(|g| extract_stem_name(ctx, &g, rec, "StemNames"))
        .collect();
    let affix_slots = rec
        .node
        .objsur_list("AffixSlots")
        .into_iter()
        .filter_map(|g| extract_affix_slot(ctx, &g, rec, "AffixSlots"))
        .collect();
    let affix_templates = rec
        .node
        .objsur_list("AffixTemplates")
        .into_iter()
        .filter_map(|g| extract_affix_template(ctx, &g, rec, "AffixTemplates"))
        .collect();
    ctx.represented(key);
    Some(PartOfSpeech {
        guid: guid.to_string(),
        name,
        abbreviation,
        children,
        inflection_classes,
        default_inflection_class,
        inflectable_features,
        stem_names,
        affix_slots,
        affix_templates,
    })
}

fn extract_inflection_class(
    ctx: &mut Ctx,
    guid: &str,
    owner: &Record,
    field: &str,
) -> Option<InflectionClass> {
    let rec = ctx.require_from(
        guid,
        "MoInflClass",
        "morphology.partsOfSpeech.inflectionClasses",
        owner,
        field,
    )?;
    let key = InventoryKey::object(InventoryKind::InflectionClass, guid.to_string());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    let children = rec
        .node
        .objsur_list("Subclasses")
        .into_iter()
        .filter_map(|g| extract_inflection_class(ctx, &g, rec, "Subclasses"))
        .collect();
    let class = InflectionClass {
        guid: guid.to_string(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        abbreviation: ctx.best_analysis(&rec.node.ws_forms("Abbreviation")),
        children,
    };
    ctx.represented(key);
    Some(class)
}

fn extract_stem_name(ctx: &mut Ctx, guid: &str, owner: &Record, field: &str) -> Option<StemName> {
    let rec = ctx.require_from(
        guid,
        "MoStemName",
        "morphology.partsOfSpeech.stemNames",
        owner,
        field,
    )?;
    let key = InventoryKey::object(InventoryKind::StemName, guid.to_string());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    let name = ctx.best_analysis(&rec.node.ws_forms("Name"));
    let abbrev_forms = rec.node.ws_forms("Abbreviation");
    let abbreviation = if abbrev_forms.is_empty() {
        None
    } else {
        Some(ctx.best_analysis(&abbrev_forms))
    };
    let regions = rec
        .node
        .objsur_list("Regions")
        .into_iter()
        .filter_map(|g| {
            extract_feature_structure(
                ctx,
                &g,
                "morphology.partsOfSpeech.stemNames",
                rec,
                "Regions",
            )
        })
        .collect();
    ctx.represented(key);
    Some(StemName {
        guid: guid.to_string(),
        name,
        abbreviation,
        regions,
    })
}

fn extract_affix_slot(ctx: &mut Ctx, guid: &str, owner: &Record, field: &str) -> Option<AffixSlot> {
    let rec = ctx.require_from(
        guid,
        "MoInflAffixSlot",
        "morphology.partsOfSpeech.affixSlots",
        owner,
        field,
    )?;
    let key = InventoryKey::object(InventoryKind::TemplateSlot, guid.to_string());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    let slot = AffixSlot {
        guid: guid.to_string(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        optional: rec.node.val_bool("Optional").unwrap_or(false),
    };
    ctx.represented(key);
    Some(slot)
}

fn extract_affix_template(
    ctx: &mut Ctx,
    guid: &str,
    owner: &Record,
    field: &str,
) -> Option<AffixTemplate> {
    let rec = ctx.require_from(
        guid,
        "MoInflAffixTemplate",
        "morphology.partsOfSpeech.affixTemplates",
        owner,
        field,
    )?;
    let key = InventoryKey::object(InventoryKind::Template, guid.to_string());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    let prefix_slots = rec.node.objsur_list("PrefixSlots");
    let suffix_slots = rec.node.objsur_list("SuffixSlots");
    let template = AffixTemplate {
        guid: guid.to_string(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        disabled: rec.node.val_bool("Disabled").unwrap_or(false),
        prefix_slots,
        suffix_slots,
        is_final: rec.node.val_bool("Final").unwrap_or(false),
    };
    ctx.represented(key);
    Some(template)
}

/// Records every affix template's slot attachments across the whole POS forest (a template's slots may belong to any POS, so this runs only once every POS's own slots are already represented).
fn record_template_slot_attachments(ctx: &mut Ctx, parts_of_speech: &[PartOfSpeech]) {
    for pos in parts_of_speech {
        for template in &pos.affix_templates {
            let owner_record = ctx.get(&template.guid);
            let fallback_owner =
                pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::MoInflAffixTemplate)
                    .guid(template.guid.clone())
                    .source_class("MoInflAffixTemplate")
                    .name(template.name.clone());
            let owner_subject = owner_record.map_or(fallback_owner.clone(), |record| {
                ctx.subject_for_record(record, None)
            });
            for (field, slots) in [
                ("PrefixSlots", &template.prefix_slots),
                ("SuffixSlots", &template.suffix_slots),
            ] {
                for slot_guid in slots {
                    let attachment = InventoryKey::attachment(
                        InventoryKind::TemplateSlot,
                        template.guid.clone(),
                        slot_guid.clone(),
                        "slot",
                    );
                    ctx.authored(attachment.clone());
                    ctx.considered(attachment.clone());
                    ctx.selected(attachment.clone());
                    let slot_object =
                        InventoryKey::object(InventoryKind::TemplateSlot, slot_guid.clone());
                    if ctx.is_represented(&slot_object) {
                        ctx.represented(attachment);
                    } else {
                        let warning = pg_snapshot::Warning::new(
                            super::codes::DANGLING_REFERENCE,
                            format!(
                                "morphology.partsOfSpeech.affixTemplates: template {} references \
                             slot {slot_guid}, which was not represented",
                                template.guid
                            ),
                        )
                        .with_subject(owner_subject.clone().field(field))
                        .with_subject(ctx.unresolved_subject(
                            slot_guid,
                            "MoInflAffixSlot",
                            Some(field),
                        ));
                        ctx.reject_with_warning(
                            attachment,
                            pg_snapshot::IssueClass::InvalidSource,
                            !template.disabled,
                            Some(pg_snapshot::SourceRef {
                                kind: pg_snapshot::FwClass::MoInflAffixSlot,
                                id: slot_guid.clone(),
                            }),
                            warning,
                        );
                    }
                }
            }
        }
        record_template_slot_attachments(ctx, &pos.children);
    }
}

// Compound rules

fn extract_compound_rules(ctx: &mut Ctx, morph_data: &Record) -> Vec<CompoundRule> {
    morph_data
        .node
        .objsur_list("CompoundRules")
        .into_iter()
        .filter_map(|g| extract_compound_rule(ctx, &g, morph_data))
        .collect()
}

fn extract_compound_rule(ctx: &mut Ctx, guid: &str, owner: &Record) -> Option<CompoundRule> {
    let rec = ctx.get(guid)?;
    let name = ctx.best_analysis(&rec.node.ws_forms("Name"));
    let disabled = rec.node.val_bool("Disabled").unwrap_or(false);
    let label = "morphology.compoundRules";
    let key = InventoryKey::object(InventoryKind::CompoundRule, guid.to_string());
    match rec.class.as_str() {
        "MoEndoCompound" => {
            ctx.considered(key.clone());
            ctx.selected(key.clone());
            let head_last = rec.node.val_bool("HeadLast").unwrap_or(false);
            let left = compound_side(
                ctx,
                rec,
                "LeftMsa",
                "left",
                rec.node.objsur_one("LeftMsa"),
                disabled,
                label,
            );
            let right = compound_side(
                ctx,
                rec,
                "RightMsa",
                "right",
                rec.node.objsur_one("RightMsa"),
                disabled,
                label,
            );
            let overriding = compound_outcome(
                ctx,
                rec,
                "OverridingMsa",
                "output",
                rec.node.objsur_one("OverridingMsa"),
                disabled,
                label,
            );
            ctx.represented(key);
            Some(CompoundRule::Endocentric {
                guid: guid.to_string(),
                name,
                disabled,
                head_last,
                left,
                right,
                overriding,
            })
        }
        "MoExoCompound" => {
            ctx.considered(key.clone());
            ctx.selected(key.clone());
            let left = compound_side(
                ctx,
                rec,
                "LeftMsa",
                "left",
                rec.node.objsur_one("LeftMsa"),
                disabled,
                label,
            );
            let right = compound_side(
                ctx,
                rec,
                "RightMsa",
                "right",
                rec.node.objsur_one("RightMsa"),
                disabled,
                label,
            );
            let to = compound_outcome(
                ctx,
                rec,
                "ToMsa",
                "output",
                rec.node.objsur_one("ToMsa"),
                disabled,
                label,
            );
            ctx.represented(key);
            Some(CompoundRule::Exocentric {
                guid: guid.to_string(),
                name,
                disabled,
                left,
                right,
                to,
            })
        }
        other => {
            ctx.warn_with_subjects(
                super::codes::UNEXPECTED_CLASS,
                format!("{label}: {guid} has unexpected class {other}"),
                [
                    ctx.subject_for_record(owner, Some("CompoundRules")),
                    ctx.unresolved_subject(guid, "MoCompoundRule", Some("CompoundRules")),
                    ctx.subject_for_record(rec, None),
                ],
            );
            None
        }
    }
}

/// Records the `CompoundRule`→`Msa` side/output attachment: represented on a successful `require_from`, otherwise rejected fatal iff the owning rule is enabled (a disabled rule's own dangling reference can never surface at runtime). `ctx.require_from` has already warned on failure, so this records without a second warning.
fn record_compound_side_attachment(
    ctx: &mut Ctx,
    owner: &Record,
    target_guid: &str,
    role: &str,
    rule_disabled: bool,
    resolved: bool,
) {
    let key = InventoryKey::attachment(
        InventoryKind::Msa,
        owner.guid.clone(),
        target_guid.to_string(),
        role.to_string(),
    );
    ctx.authored(key.clone());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    if resolved {
        ctx.represented(key);
        // Compound fields are copied from this MSA into the rule.
        let msa_key = InventoryKey::object(InventoryKind::Msa, target_guid.to_string());
        ctx.considered(msa_key.clone());
        ctx.selected(msa_key.clone());
        ctx.represented(msa_key);
    } else {
        ctx.record_rejected(
            key,
            ConversionIssue {
                code: super::codes::DANGLING_REFERENCE,
                class: IssueClass::InvalidSource,
                source: Some(SourceRef {
                    kind: pg_snapshot::FwClass::MoStemMsa,
                    id: target_guid.to_string(),
                }),
                fatal: !rule_disabled,
                message: format!(
                    "morphology.compoundRules: compound rule {} references {role} \
                     MSA {target_guid}, which does not resolve to a MoStemMsa",
                    owner.guid,
                ),
            },
        );
    }
}

/// A compound side/outcome is always an `MoStemMsa`, but `HCLoader` only ever reads its `PartOfSpeechRA`/`ProdRestrictRC` pair for a side requirement.
fn compound_side(
    ctx: &mut Ctx,
    owner: &Record,
    field: &str,
    role: &str,
    msa_guid: Option<String>,
    rule_disabled: bool,
    label: &str,
) -> CompoundConstituentRequirement {
    let Some(guid) = msa_guid else {
        return CompoundConstituentRequirement::default();
    };
    let rec = ctx.require_from(&guid, "MoStemMsa", label, owner, field);
    record_compound_side_attachment(ctx, owner, &guid, role, rule_disabled, rec.is_some());
    let Some(rec) = rec else {
        return CompoundConstituentRequirement::default();
    };
    CompoundConstituentRequirement {
        part_of_speech: rec.node.objsur_one("PartOfSpeech"),
        exception_features: rec.node.objsur_list("ProdRestrict"),
    }
}

fn compound_outcome(
    ctx: &mut Ctx,
    owner: &Record,
    field: &str,
    role: &str,
    msa_guid: Option<String>,
    rule_disabled: bool,
    label: &str,
) -> CompoundOutcome {
    let Some(guid) = msa_guid else {
        return CompoundOutcome::default();
    };
    let rec = ctx.require_from(&guid, "MoStemMsa", label, owner, field);
    record_compound_side_attachment(ctx, owner, &guid, role, rule_disabled, rec.is_some());
    let Some(rec) = rec else {
        return CompoundOutcome::default();
    };
    CompoundOutcome {
        part_of_speech: rec.node.objsur_one("PartOfSpeech"),
        inflection_class: rec.node.objsur_one("InflectionClass"),
    }
}

// Ad-hoc co-occurrence prohibitions

fn extract_adhoc_prohibitions(ctx: &mut Ctx, morph_data: &Record) -> Vec<AdhocProhibition> {
    validate_adhoc_prohibition_references(ctx, morph_data, "AdhocCoProhibitions");

    // HCLoader reads concrete rule repositories, so a group's Members must not decide extraction.
    let mut groups: Vec<_> = ctx.graph.by_class("MoAdhocProhibGr").collect();
    groups.sort_by(|left, right| left.guid.cmp(&right.guid));
    for group in groups {
        validate_adhoc_prohibition_references(ctx, group, "Members");
    }

    let mut rules: Vec<_> = ctx
        .graph
        .records
        .values()
        .filter(|record| {
            matches!(
                record.class.as_str(),
                "MoMorphAdhocProhib" | "MoAlloAdhocProhib"
            )
        })
        .collect();
    rules.sort_by(|left, right| left.guid.cmp(&right.guid));
    rules
        .into_iter()
        .filter_map(|rule| extract_adhoc_prohibition(ctx, &rule.guid, morph_data))
        .collect()
}

fn extract_adhoc_prohibition_groups(ctx: &Ctx) -> Vec<AdhocProhibitionGroup> {
    let mut groups: Vec<_> = ctx.graph.by_class("MoAdhocProhibGr").collect();
    groups.sort_by(|left, right| left.guid.cmp(&right.guid));
    groups
        .into_iter()
        .map(|group| {
            let mut members = group.node.objsur_list("Members");
            members.sort();
            members.dedup();
            AdhocProhibitionGroup {
                guid: group.guid.clone(),
                name: group.node.ws_forms("Name"),
                description: group.node.ws_forms("Description"),
                members,
            }
        })
        .collect()
}

/// Reports broken group-or-rule links; extraction follows HCLoader's concrete repositories instead.
fn validate_adhoc_prohibition_references(ctx: &mut Ctx, owner: &Record, field: &str) {
    for guid in owner.node.objsur_list(field) {
        match ctx.get(&guid) {
            Some(record)
                if matches!(
                    record.class.as_str(),
                    "MoAdhocProhibGr" | "MoMorphAdhocProhib" | "MoAlloAdhocProhib"
                ) => {}
            Some(record) => {
                ctx.warn_with_subjects(
                    super::codes::UNEXPECTED_CLASS,
                    format!(
                        "morphology.adhocProhibitions: {field} references {guid} with unexpected class {}",
                        record.class
                    ),
                    [
                        ctx.subject_for_record(owner, Some(field)),
                        ctx.unresolved_subject(&guid, "MoAdhocProhib", Some(field)),
                        ctx.subject_for_record(record, None),
                    ],
                );
            }
            None => {
                ctx.warn_with_subjects(
                    super::codes::DANGLING_REFERENCE,
                    format!(
                        "morphology.adhocProhibitions: {field} has dangling reference to {guid}"
                    ),
                    [
                        ctx.subject_for_record(owner, Some(field)),
                        ctx.unresolved_subject(&guid, "MoAdhocProhib", Some(field)),
                    ],
                );
            }
        }
    }
}

fn extract_adhoc_prohibition(
    ctx: &mut Ctx,
    guid: &str,
    owner: &Record,
) -> Option<AdhocProhibition> {
    let rec = ctx.get(guid)?;
    let disabled = rec.node.val_bool("Disabled").unwrap_or(false);
    let adjacency = match rec.node.val_int("Adjacency") {
        Some(0) => Adjacency::Anywhere,
        Some(1) => Adjacency::SomewhereToLeft,
        Some(2) => Adjacency::SomewhereToRight,
        Some(3) => Adjacency::AdjacentToLeft,
        Some(4) => Adjacency::AdjacentToRight,
        other => {
            ctx.warn_with_subjects(
                super::codes::UNRECOGNIZED_ENUM_VALUE,
                format!(
                    "morphology.adhocProhibitions: {guid} has unexpected Adjacency {other:?}, \
                     defaulting to anywhere"
                ),
                [ctx.subject_for_record(rec, Some("Adjacency"))],
            );
            Adjacency::Anywhere
        }
    };
    match rec.class.as_str() {
        "MoAlloAdhocProhib" => {
            let primary = rec.node.objsur_one("FirstAllomorph")?;
            let others = rec.node.objsur_list("RestOfAllos");
            let key = InventoryKey::object(InventoryKind::AllomorphCoOccurrence, guid.to_string());
            ctx.considered(key.clone());
            ctx.selected(key.clone());
            ctx.represented(key);
            Some(AdhocProhibition::Allomorph {
                guid: guid.to_string(),
                disabled,
                primary,
                others,
                adjacency,
            })
        }
        "MoMorphAdhocProhib" => {
            let primary = rec.node.objsur_one("FirstMorpheme")?;
            let others = rec.node.objsur_list("RestOfMorphs");
            let key = InventoryKey::object(InventoryKind::MorphemeCoOccurrence, guid.to_string());
            ctx.considered(key.clone());
            ctx.selected(key.clone());
            ctx.represented(key);
            Some(AdhocProhibition::Morpheme {
                guid: guid.to_string(),
                disabled,
                primary,
                others,
                adjacency,
            })
        }
        other => {
            ctx.warn_with_subjects(
                super::codes::UNEXPECTED_CLASS,
                format!("morphology.adhocProhibitions: {guid} has unexpected class {other}"),
                [
                    ctx.subject_for_record(owner, Some("AdhocCoProhibitions")),
                    ctx.unresolved_subject(guid, "MoAdhocProhib", Some("AdhocCoProhibitions")),
                    ctx.subject_for_record(rec, None),
                ],
            );
            None
        }
    }
}

/// Beyond the raw-guid dangling-reference check `Snapshot::validate()` already performs: cross-check whether an enabled `Msa::Inflectional` ad-hoc prohibition's slot appears in any non-disabled `AffixTemplate` -- exactly the scenario FieldWorks' own exporter crashes on -- surfacing it as an import warning since this crate keeps the full authored data rather than silently dropping the rule.
pub fn check_stale_adhoc_morpheme_rules(ctx: &mut Ctx, morphology: &Morphology, lexicon: &Lexicon) {
    use pg_snapshot::Msa;

    let mut enabled_slots: std::collections::HashSet<&str> = std::collections::HashSet::new();
    fn collect_slots<'a>(
        pos: &'a [PartOfSpeech],
        enabled_slots: &mut std::collections::HashSet<&'a str>,
    ) {
        for p in pos {
            for t in &p.affix_templates {
                if !t.disabled {
                    for s in t.prefix_slots.iter().chain(&t.suffix_slots) {
                        enabled_slots.insert(s.as_str());
                    }
                }
            }
            collect_slots(&p.children, enabled_slots);
        }
    }
    collect_slots(&morphology.parts_of_speech, &mut enabled_slots);

    let find_msa_entry = |guid: &str| -> Option<&pg_snapshot::lexicon::LexEntry> {
        lexicon
            .entries
            .iter()
            .find(|entry| entry.msas.iter().any(|msa| msa.guid() == guid))
    };

    for prohib in &morphology.adhoc_prohibitions {
        let AdhocProhibition::Morpheme {
            guid,
            disabled,
            primary,
            others,
            ..
        } = prohib
        else {
            continue;
        };
        if *disabled {
            continue;
        }
        for (field, msa_guid) in std::iter::once(("FirstMorpheme", primary.as_str()))
            .chain(others.iter().map(|guid| ("RestOfMorphs", guid.as_str())))
        {
            if let Some(entry) = find_msa_entry(msa_guid) {
                let Some(Msa::Inflectional { slots, .. }) =
                    entry.msas.iter().find(|msa| msa.guid() == msa_guid)
                else {
                    continue;
                };
                if !slots.is_empty() && !slots.iter().any(|s| enabled_slots.contains(s.as_str())) {
                    let affix_name = entry
                        .citation_form
                        .first()
                        .map(|form| form.form.as_str())
                        .filter(|name| !name.is_empty())
                        .unwrap_or("unnamed inflectional affix");
                    let entry_title = entry
                        .citation_form
                        .first()
                        .map(|form| form.form.as_str())
                        .filter(|name| !name.is_empty())
                        .or_else(|| {
                            entry
                                .allomorphs
                                .iter()
                                .flat_map(|allomorph| &allomorph.forms)
                                .map(|form| form.form.as_str())
                                .find(|name| !name.is_empty())
                        });
                    let warning = pg_snapshot::Warning::new(
                        super::codes::STALE_ADHOC_PROHIBITION,
                        format!(
                            "Ad hoc prohibition for '{affix_name}' refers to an inflectional affix whose slot is not part of an enabled template."
                        ),
                    )
                    .with_subject(
                        pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::MoAdhocProhib)
                            .guid(guid.clone())
                            .source_class("MoMorphAdhocProhib")
                            .name(format!("Ad hoc prohibition for '{affix_name}'"))
                            .field(field),
                    )
                    .with_subject(
                        pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::MoInflAffMsa)
                            .guid(msa_guid)
                            .source_class("MoInflAffMsa")
                            .name(affix_name)
                            .field("Slots"),
                    )
                    .with_subject(
                        pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::LexEntry)
                            .guid(entry.guid.clone())
                            .source_class("LexEntry")
                            .name(entry_title.unwrap_or("unnamed entry")),
                    );
                    ctx.warnings.push(warning);
                }
            }
        }
    }
}

// Exception features ("production restriction" registry)

fn extract_exception_features(
    ctx: &mut Ctx,
    morph_data: Option<&Record>,
    phon_data: Option<&Record>,
) -> Vec<ExceptionFeature> {
    let mut out = Vec::new();
    if let Some(md) = morph_data {
        if let Some(list_guid) = md.node.objsur_one("ProdRestrict") {
            walk_possibility_list(
                ctx,
                &list_guid,
                "morphology.exceptionFeatures",
                md,
                "ProdRestrict",
                &mut |ctx, rec| {
                    if rec.class == "CmPossibility" {
                        out.push(exception_feature(ctx, rec));
                    }
                },
            );
        }
    }
    if let Some(pd) = phon_data {
        if let Some(list_guid) = pd.node.objsur_one("PhonRuleFeats") {
            walk_possibility_list(
                ctx,
                &list_guid,
                "morphology.exceptionFeatures",
                pd,
                "PhonRuleFeats",
                &mut |ctx, rec| {
                    if rec.class == "CmPossibility" {
                        out.push(exception_feature(ctx, rec));
                    }
                },
            );
        }
    }
    out
}

fn exception_feature(ctx: &mut Ctx, rec: &Record) -> ExceptionFeature {
    ExceptionFeature {
        guid: rec.guid.clone(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        abbreviation: ctx.best_analysis(&rec.node.ws_forms("Abbreviation")),
    }
}

// LexEntryInflType (irregular-inflection variant types)

fn extract_lex_entry_infl_types(ctx: &mut Ctx, lex_db: &Record) -> Vec<LexEntryInflType> {
    let mut out = Vec::new();
    for field in ["VariantEntryTypes", "ComplexEntryTypes"] {
        if let Some(list_guid) = lex_db.node.objsur_one(field) {
            walk_possibility_list(
                ctx,
                &list_guid,
                "morphology.lexEntryInflTypes",
                lex_db,
                field,
                &mut |ctx, rec| {
                    if rec.class == "LexEntryInflType" {
                        if let Some(t) = lex_entry_infl_type(ctx, rec) {
                            out.push(t);
                        }
                    }
                },
            );
        }
    }
    out
}

fn lex_entry_infl_type(ctx: &mut Ctx, rec: &Record) -> Option<LexEntryInflType> {
    let key = InventoryKey::object(InventoryKind::RuleFeature, rec.guid.clone());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    let inflection_features = rec
        .node
        .objsur_one("InflFeats")
        .and_then(|g| {
            extract_feature_structure(ctx, &g, "morphology.lexEntryInflTypes", rec, "InflFeats")
        })
        .filter(|fs| !fs.values.is_empty());
    let result = LexEntryInflType {
        guid: rec.guid.clone(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        abbreviation: ctx.best_analysis(&rec.node.ws_forms("Abbreviation")),
        gloss_prepend: ctx.best_analysis(&rec.node.ws_forms("GlossPrepend")),
        gloss_append: ctx.best_analysis(&rec.node.ws_forms("GlossAppend")),
        slots: rec.node.objsur_list("Slots"),
        inflection_features,
    };
    ctx.represented(key);
    Some(result)
}

// Shared `CmPossibilityList` pre-order walk (used by exception features + LexEntryInflType)

/// Pre-order walk of a `CmPossibilityList`'s `Possibilities`, recursing into each item's own `SubPossibilities`; calls `visit` for every resolved item regardless of class (callers filter), and a dangling item guid is warned and simply not descended into.
fn walk_possibility_list(
    ctx: &mut Ctx,
    list_guid: &str,
    label: &str,
    owner: &Record,
    field: &str,
    visit: &mut dyn FnMut(&mut Ctx, &Record),
) {
    let Some(list) = ctx.require_from(list_guid, "CmPossibilityList", label, owner, field) else {
        return;
    };
    let item_guids = list.node.objsur_list("Possibilities");
    for item_guid in item_guids {
        walk_possibility_item(ctx, &item_guid, label, list, "Possibilities", visit);
    }
}

fn walk_possibility_item(
    ctx: &mut Ctx,
    guid: &str,
    label: &str,
    owner: &Record,
    field: &str,
    visit: &mut dyn FnMut(&mut Ctx, &Record),
) {
    let Some(rec) = ctx.get(guid) else {
        ctx.warn_with_subjects(
            super::codes::DANGLING_REFERENCE,
            format!("{label}: dangling possibility-list item {guid}"),
            [
                ctx.subject_for_record(owner, Some(field)),
                ctx.unresolved_subject(guid, "CmPossibility", Some(field)),
            ],
        );
        return;
    };
    visit(ctx, rec);
    let child_guids = rec.node.objsur_list("SubPossibilities");
    for child_guid in child_guids {
        walk_possibility_item(ctx, &child_guid, label, rec, "SubPossibilities", visit);
    }
}
