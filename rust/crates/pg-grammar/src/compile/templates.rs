//! Affix templates and null-affix synthesis for irregular-form slots. A template is skipped entirely if none of its slots end up with at least one loaded affix rule.

use hashbrown::HashMap;

use pg_snapshot::morphology::{AffixSlot, AffixTemplate, LexEntryInflType, PartOfSpeech};
use pg_snapshot::{InventoryKey, InventoryKind, IssueClass, Snapshot};

use crate::model::{
    AffixTemplateDef, AllomorphId, AllomorphOwner, MRuleId, MorphRuleDef, MorphemeId, MorphemeInfo,
    OutputAction, PartRef, Pattern, ReduplicationHint, SlotDef, StratumId, TemplateId,
    TemplateSlotZone,
};
use crate::GrammarError;

use super::inventory::LineageTarget;
use super::{environment, issue_codes, roles, Acc, Ctx};

pub(crate) fn build(
    snapshot: &Snapshot,
    ctx: &Ctx,
    acc: &mut Acc,
) -> Result<Vec<TemplateId>, GrammarError> {
    let mut slot_registry: HashMap<&str, &AffixSlot> = HashMap::new();
    collect_slots(&snapshot.morphology.parts_of_speech, &mut slot_registry);

    let mut out = Vec::new();
    build_pos(
        &snapshot.morphology.parts_of_speech,
        snapshot,
        ctx,
        acc,
        &slot_registry,
        &mut out,
    )?;
    Ok(out)
}

pub(crate) fn record_unconsidered_slots(snapshot: &Snapshot, ctx: &Ctx) {
    fn walk<'a>(
        items: &'a [PartOfSpeech],
        slots: &mut Vec<&'a AffixSlot>,
        disabled_refs: &mut Vec<(String, String)>,
    ) {
        for pos in items {
            slots.extend(&pos.affix_slots);
            for template in &pos.affix_templates {
                if template.disabled {
                    disabled_refs.extend(
                        template
                            .prefix_slots
                            .iter()
                            .chain(&template.suffix_slots)
                            .map(|slot| (slot.clone(), template.guid.clone())),
                    );
                }
            }
            walk(&pos.children, slots, disabled_refs);
        }
    }

    let mut slots = Vec::new();
    let mut disabled_refs = Vec::new();
    walk(
        &snapshot.morphology.parts_of_speech,
        &mut slots,
        &mut disabled_refs,
    );
    disabled_refs.sort();
    disabled_refs.dedup();
    for (slot_guid, template_guid) in disabled_refs {
        let key = InventoryKey::object(InventoryKind::TemplateSlot, slot_guid);
        ctx.considered(key.clone());
        ctx.not_considered_in_context(
            key,
            format!("template:{template_guid}"),
            pg_snapshot::LoadReasonCode::Disabled,
        );
    }
    for slot in slots {
        let key = InventoryKey::object(InventoryKind::TemplateSlot, slot.guid.clone());
        if ctx.has_load_decision(&key) {
            continue;
        }
        ctx.considered(key.clone());
        ctx.not_considered(key, pg_snapshot::LoadReasonCode::Unreferenced);
    }
}

fn collect_slots<'a>(items: &'a [PartOfSpeech], out: &mut HashMap<&'a str, &'a AffixSlot>) {
    for pos in items {
        for s in &pos.affix_slots {
            out.insert(s.guid.as_str(), s);
        }
        collect_slots(&pos.children, out);
    }
}

fn build_pos(
    items: &[PartOfSpeech],
    snapshot: &Snapshot,
    ctx: &Ctx,
    acc: &mut Acc,
    slot_registry: &HashMap<&str, &AffixSlot>,
    out: &mut Vec<TemplateId>,
) -> Result<(), GrammarError> {
    for pos in items {
        for tmpl in &pos.affix_templates {
            let key = InventoryKey::object(InventoryKind::Template, tmpl.guid.clone());
            ctx.considered(key);
            if tmpl.disabled {
                ctx.record_load_decision(pg_snapshot::LoadDecisionDraft {
                    subject: InventoryKey::object(InventoryKind::Template, tmpl.guid.clone()),
                    pipeline_stage: pg_snapshot::LoadPipelineStage::Compile,
                    context_key: String::new(),
                    disposition: pg_snapshot::LoadDisposition::NotConsidered,
                    loaded: None,
                    reason_code: pg_snapshot::LoadReasonCode::Disabled,
                    effective_value_json: None,
                    issue_code: None,
                });
                continue;
            }
            if let Some(id) = build_template(pos, tmpl, snapshot, ctx, acc, slot_registry)? {
                out.push(id);
            }
        }
        build_pos(&pos.children, snapshot, ctx, acc, slot_registry, out)?;
    }
    Ok(())
}

fn build_template(
    pos: &PartOfSpeech,
    tmpl: &AffixTemplate,
    snapshot: &Snapshot,
    ctx: &Ctx,
    acc: &mut Acc,
    slot_registry: &HashMap<&str, &AffixSlot>,
) -> Result<Option<TemplateId>, GrammarError> {
    // Combined slot order: suffix slots as declared, then prefix slots reversed.
    let mut combined: Vec<(&str, bool, &str, usize)> = tmpl
        .suffix_slots
        .iter()
        .enumerate()
        .map(|(ordinal, guid)| (guid.as_str(), false, "suffix", ordinal))
        .collect();
    combined.extend(
        tmpl.prefix_slots
            .iter()
            .enumerate()
            .rev()
            .map(|(ordinal, guid)| (guid.as_str(), true, "prefix", ordinal)),
    );

    let template_key = InventoryKey::object(InventoryKind::Template, tmpl.guid.clone());
    ctx.selected(template_key.clone());

    let mut slot_defs = Vec::new();
    let mut represented_slots = Vec::new();
    for (slot_guid, is_prefix, side, authored_ordinal) in combined {
        let slot_key = InventoryKey::object(InventoryKind::TemplateSlot, slot_guid.to_string());
        let attachment = InventoryKey::attachment(
            InventoryKind::TemplateSlot,
            tmpl.guid.clone(),
            slot_guid.to_string(),
            roles::SLOT,
        );
        ctx.authored(attachment.clone());
        ctx.considered(attachment.clone());
        let Some(&affix_slot) = slot_registry.get(slot_guid) else {
            ctx.selected(attachment.clone());
            let field = if is_prefix {
                "PrefixSlots"
            } else {
                "SuffixSlots"
            };
            let warning = pg_snapshot::Warning::new(
                issue_codes::TEMPLATE_SLOT_UNRESOLVED,
                "Affix template references a slot that does not resolve; the slot was skipped.",
            )
            .with_subject(
                pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::MoInflAffixTemplate)
                    .guid(tmpl.guid.clone())
                    .name(tmpl.name.clone())
                    .field(field),
            )
            .with_subject(
                pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::MoInflAffixSlot)
                    .guid(slot_guid)
                    .unresolved_reference()
                    .field(field),
            );
            ctx.reject_with_subjects(
                attachment,
                issue_codes::TEMPLATE_SLOT_UNRESOLVED,
                IssueClass::InvalidSource,
                warning,
            );
            continue;
        };
        ctx.considered(slot_key.clone());
        ctx.selected(slot_key.clone());
        ctx.selected(attachment.clone());
        let mut rules = acc.slot_rules.get(slot_guid).cloned().unwrap_or_default();
        if rules.is_empty() {
            // No loaded affix at all references this slot — HCLoader drops it entirely.
            ctx.reject(
                slot_key,
                issue_codes::TEMPLATE_SLOT_NO_RULES,
                IssueClass::UnrepresentableForHc,
                format!(
                    "Affix template slot '{}' has no loaded inflectional affixes.",
                    affix_slot.name
                ),
            );
            ctx.reject_with_source(
                attachment,
                issue_codes::TEMPLATE_SLOT_NO_RULES,
                IssueClass::UnrepresentableForHc,
                Some(pg_snapshot::SourceRef {
                    kind: pg_snapshot::FwClass::MoInflAffixSlot,
                    id: slot_guid.to_string(),
                }),
                format!(
                    "Affix template slot '{}' has no loaded inflectional affixes.",
                    affix_slot.name
                ),
            );
            continue;
        }
        let infl_types_for_slot: Vec<&LexEntryInflType> = snapshot
            .morphology
            .lex_entry_infl_types
            .iter()
            .filter(|t| t.slots.iter().any(|s| s == slot_guid))
            .collect();

        if !infl_types_for_slot.is_empty() {
            // Block ordinary affixes in this slot from applying to irregularly-inflected forms.
            let mut excl = crate::model::MprSet::EMPTY;
            for it in &infl_types_for_slot {
                if let Some(s) = ctx.mpr.lex_entry_infl_type(&it.guid) {
                    excl = excl.union(s);
                }
            }
            exclude_from_rules(&rules, excl, acc);

            // Null-affix synthesis, unless the slot is optional (an irregular form can leave it empty).
            if !affix_slot.optional {
                for it in &infl_types_for_slot {
                    if let Some(id) = build_null_affix_rule(it, is_prefix, ctx, acc) {
                        rules.push(id);
                    }
                }
            }
        }

        let compiled_order = slot_defs.len();
        represented_slots.push((
            slot_key,
            attachment,
            side,
            authored_ordinal,
            slot_guid,
            compiled_order,
        ));
        slot_defs.push(SlotDef {
            name: Some(affix_slot.name.clone()),
            optional: affix_slot.optional,
            zone: if is_prefix {
                TemplateSlotZone::Prefix
            } else {
                TemplateSlotZone::Suffix
            },
            rules,
        });
    }

    if slot_defs.is_empty() {
        ctx.reject(
            template_key,
            issue_codes::TEMPLATE_NO_SLOTS,
            IssueClass::UnrepresentableForHc,
            "affix template has no slots with any loaded affix rule",
        );
        return Ok(None);
    }

    let pos_bits = ctx
        .pos
        .bits_with_descendants(std::iter::once(pos.guid.as_str()));
    let required_syn_fs = match super::features::build_syn_fs(ctx.syn, Some(pos_bits), None) {
        Ok(fs) => acc.fs_interner.intern(fs),
        Err(e) => {
            ctx.reject(
                template_key,
                issue_codes::TEMPLATE_BUILD_FAILED,
                IssueClass::UnrepresentableForHc,
                format!("affix template {:?}: {e}; skipped", tmpl.guid),
            );
            return Ok(None);
        }
    };

    let id = TemplateId(acc.templates.len() as u32);
    acc.templates.push(AffixTemplateDef {
        source_guid: Some(tmpl.guid.clone()),
        name: Some(tmpl.name.clone()),
        is_final: tmpl.is_final,
        required_syn_fs,
        slots: slot_defs,
    });
    for (slot_key, attachment, side, authored_ordinal, slot_guid, compiled_order) in
        represented_slots
    {
        ctx.represented(slot_key);
        ctx.represented_with_effective_value(
            attachment,
            serde_json::json!({
                "compiledOrder": compiled_order,
                "ordinal": authored_ordinal,
                "side": side,
                "slotGuid": slot_guid,
            })
            .to_string(),
        );
    }
    ctx.represented(template_key);
    Ok(Some(id))
}

fn exclude_from_rules(rule_ids: &[MRuleId], excl: crate::model::MprSet, acc: &mut Acc) {
    for &rid in rule_ids {
        if let MorphRuleDef::AffixProcess(def) = &mut acc.mrules[rid.0 as usize] {
            for allo in &mut def.allomorphs {
                allo.excluded_mpr = allo.excluded_mpr.union(excl);
            }
        }
    }
}

/// A rule that matches any word and inserts nothing but the null-boundary marker, gated on the irregular-form's MPR feature so it only "fills" the slot for words tagged as that inflection type.
fn build_null_affix_rule(
    it: &LexEntryInflType,
    is_prefix: bool,
    ctx: &Ctx,
    acc: &mut Acc,
) -> Option<MRuleId> {
    let key = InventoryKey::synthetic(InventoryKind::Msa, format!("null-affix#{}", it.guid));
    ctx.synthesized(key.clone());
    ctx.considered(key.clone());
    let Some(required_mpr) = ctx.mpr.lex_entry_infl_type(&it.guid) else {
        ctx.selected(key.clone());
        ctx.reject_with_source(
            key,
            issue_codes::NULL_AFFIX_MPR_UNRESOLVED,
            IssueClass::InvalidSource,
            Some(pg_snapshot::SourceRef {
                kind: pg_snapshot::FwClass::LexEntryInflType,
                id: it.guid.clone(),
            }),
            format!(
                "lexEntryInflType {:?}: does not resolve in the MPR registry; null-affix rule skipped",
                it.guid
            ),
        );
        return None;
    };
    ctx.selected(key.clone());

    let out_syn_fs = match &it.inflection_features {
        Some(fs) if !fs.values.is_empty() => {
            match super::features::build_syn_fs(ctx.syn, None, Some(fs)) {
                Ok(v) => acc.fs_interner.intern(v),
                Err(e) => {
                    ctx.reject_with_source(
                        key,
                        issue_codes::NULL_AFFIX_SYN_FS_FAILED,
                        IssueClass::UnrepresentableForHc,
                        Some(pg_snapshot::SourceRef {
                            kind: pg_snapshot::FwClass::LexEntryInflType,
                            id: it.guid.clone(),
                        }),
                        format!(
                            "lexEntryInflType {:?}: {e}; null-affix rule skipped",
                            it.guid
                        ),
                    );
                    return None;
                }
            }
        }
        _ => acc.fs_interner.intern(pg_featstruct::FeatureStruct::EMPTY),
    };

    let stem_pattern = Pattern {
        nodes: environment::any_plus(ctx),
    };
    let null_ins = if is_prefix { "^0+" } else { "+^0" };
    let Ok(insert) = insert_segments(null_ins, ctx) else {
        ctx.reject_with_source(
            key,
            issue_codes::NULL_AFFIX_SEGMENT_FAILED,
            IssueClass::UnrepresentableForHc,
            Some(pg_snapshot::SourceRef {
                kind: pg_snapshot::FwClass::LexEntryInflType,
                id: it.guid.clone(),
            }),
            format!(
                "lexEntryInflType {:?}: cannot segment null-affix marker {null_ins:?}; rule skipped",
                it.guid
            ),
        );
        return None;
    };
    let rhs = if is_prefix {
        vec![insert, OutputAction::Copy(PartRef::Input(0))]
    } else {
        vec![OutputAction::Copy(PartRef::Input(0)), insert]
    };

    let mrule_id = MRuleId(acc.mrules.len() as u32);
    let allo_id = AllomorphId(acc.allomorph_owners.len() as u32);
    acc.allomorph_owners
        .push(AllomorphOwner::Affix(mrule_id, 0));
    acc.allomorph_sources.push(crate::model::AllomorphSource {
        form_guids: Vec::new(),
        omitted: true,
        placement: crate::model::SourceMorphPlacement::Append,
    });

    let allo = crate::model::AffixAllomorphDef {
        id: allo_id,
        environments: Vec::new(),
        co_occurrence: Vec::new(),
        required_syn_fs: acc.fs_interner.intern(pg_featstruct::FeatureStruct::EMPTY),
        vars: crate::model::VarTable::default(),
        required_mpr,
        excluded_mpr: crate::model::MprSet::EMPTY,
        out_mpr: crate::model::MprSet::EMPTY,
        redup_hint: ReduplicationHint::Implicit,
        lhs: vec![stem_pattern],
        rhs,
        properties: Vec::new(),
    };

    let morpheme = MorphemeId(acc.morphemes.len() as u32);
    acc.morphemes.push(MorphemeInfo {
        xml_key: format!("null-affix#{}", it.guid),
        source_msa_guid: None,
        source_msa_class: None,
        source_infl_type_guid: Some(it.guid.clone()),
        owner: None,
        morph_id: None,
        gloss: None,
        stratum: StratumId(0),
        properties: Vec::new(),
        co_occurrence: Vec::new(),
    });

    acc.mrules.push(MorphRuleDef::AffixProcess(
        crate::model::AffixProcessRuleDef {
            morpheme,
            name: Some("Null".to_string()),
            blockable: true,
            partial_reason: None,
            max_apps: 1,
            required_syn_fs: acc.fs_interner.intern(pg_featstruct::FeatureStruct::EMPTY),
            out_syn_fs,
            obligatory_features: Vec::new(),
            required_stem_name: None,
            allomorphs: vec![allo],
            is_template_rule: false,
        },
    ));
    ctx.represent_via(LineageTarget::MRule(mrule_id.0), key);
    Some(mrule_id)
}

fn insert_segments(text: &str, ctx: &Ctx) -> Result<OutputAction, String> {
    let shape = crate::segment::segment(ctx.table, text).map_err(|e| e.to_string())?;
    Ok(OutputAction::InsertSegments {
        table: ctx.table_id,
        shape: crate::model::SegmentedText {
            text: text.to_string(),
            shape,
        },
    })
}
