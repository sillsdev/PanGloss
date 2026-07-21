//! Affix templates (`LoadAffixTemplate`, HCLoader.cs:1673-1735) and null-affix synthesis for
//! irregular-form slots (`LoadNullAffixProcessRule`, HCLoader.cs:1771-1806).
//!
//! A template is skipped entirely if none of its slots end up with at least one loaded affix
//! rule (HCLoader.cs:297-299: `slots.Where(s => s.Affixes.Any(msa => m_morphemes.ContainsKey
//! (msa))`) — this compiler mirrors that by dropping any slot whose [`Acc::slot_rules`] entry is
//! empty/absent, then dropping the whole template if every slot was dropped.

use hashbrown::HashMap;

use pg_snapshot::morphology::{AffixSlot, AffixTemplate, LexEntryInflType, PartOfSpeech};
use pg_snapshot::Snapshot;

use crate::model::{
    AffixTemplateDef, AllomorphId, AllomorphOwner, MRuleId, MorphRuleDef, MorphemeId, MorphemeInfo,
    OutputAction, PartRef, Pattern, ReduplicationHint, SlotDef, StratumId, TemplateId,
};
use crate::GrammarError;

use super::{environment, Acc, Ctx};

pub(crate) fn build(
    snapshot: &Snapshot,
    ctx: &Ctx,
    acc: &mut Acc,
    warnings: &mut Vec<String>,
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
        warnings,
    )?;
    Ok(out)
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
    warnings: &mut Vec<String>,
) -> Result<(), GrammarError> {
    for pos in items {
        for tmpl in &pos.affix_templates {
            if tmpl.disabled {
                continue;
            }
            if let Some(id) =
                build_template(pos, tmpl, snapshot, ctx, acc, slot_registry, warnings)?
            {
                out.push(id);
            }
        }
        build_pos(
            &pos.children,
            snapshot,
            ctx,
            acc,
            slot_registry,
            out,
            warnings,
        )?;
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
    warnings: &mut Vec<String>,
) -> Result<Option<TemplateId>, GrammarError> {
    // Combined slot order: suffix slots as declared, then prefix slots reversed
    // (`SuffixSlotsRS.Concat(PrefixSlotsRS.Reverse())`, HCLoader.cs:297).
    let mut combined: Vec<(&str, bool)> = tmpl
        .suffix_slots
        .iter()
        .map(|g| (g.as_str(), false))
        .collect();
    combined.extend(tmpl.prefix_slots.iter().rev().map(|g| (g.as_str(), true)));

    let mut slot_defs = Vec::new();
    for (slot_guid, is_prefix) in combined {
        let Some(&affix_slot) = slot_registry.get(slot_guid) else {
            warnings.push(format!(
                "affix template {:?}: slot {slot_guid:?} does not resolve; skipped",
                tmpl.guid
            ));
            continue;
        };
        let mut rules = acc.slot_rules.get(slot_guid).cloned().unwrap_or_default();
        if rules.is_empty() {
            // No loaded affix at all references this slot — HCLoader drops it entirely.
            continue;
        }

        let infl_types_for_slot: Vec<&LexEntryInflType> = snapshot
            .morphology
            .lex_entry_infl_types
            .iter()
            .filter(|t| t.slots.iter().any(|s| s == slot_guid))
            .collect();

        if !infl_types_for_slot.is_empty() {
            // Block ordinary affixes in this slot from applying to irregularly-inflected forms
            // (HCLoader.cs:1710-1719).
            let mut excl = crate::model::MprSet::EMPTY;
            for it in &infl_types_for_slot {
                if let Some(s) = ctx.mpr.lex_entry_infl_type(&it.guid) {
                    excl = excl.union(s);
                }
            }
            exclude_from_rules(&rules, excl, acc);

            // Null-affix synthesis (HCLoader.cs:1725-1729), unless the slot is optional (an
            // irregular form can simply leave an optional slot empty).
            if !affix_slot.optional {
                for it in &infl_types_for_slot {
                    if let Some(id) = build_null_affix_rule(it, is_prefix, ctx, acc, warnings) {
                        rules.push(id);
                    }
                }
            }
        }

        slot_defs.push(SlotDef {
            name: Some(affix_slot.name.clone()),
            optional: affix_slot.optional,
            rules,
        });
    }

    if slot_defs.is_empty() {
        return Ok(None);
    }

    let pos_bits = ctx
        .pos
        .bits_with_descendants(std::iter::once(pos.guid.as_str()));
    let required_syn_fs = match super::features::build_syn_fs(ctx.syn, Some(pos_bits), None) {
        Ok(fs) => acc.fs_interner.intern(fs),
        Err(e) => {
            warnings.push(format!("affix template {:?}: {e}; skipped", tmpl.guid));
            return Ok(None);
        }
    };

    let id = TemplateId(acc.templates.len() as u32);
    acc.templates.push(AffixTemplateDef {
        name: Some(tmpl.name.clone()),
        is_final: tmpl.is_final,
        required_syn_fs,
        slots: slot_defs,
    });
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

/// `LoadNullAffixProcessRule` (HCLoader.cs:1771-1806): a rule that matches any word and inserts
/// nothing but the null-boundary marker, gated on the irregular-form's own MPR feature so it
/// only "fills" the slot for words already tagged as that inflection type.
fn build_null_affix_rule(
    it: &LexEntryInflType,
    is_prefix: bool,
    ctx: &Ctx,
    acc: &mut Acc,
    warnings: &mut Vec<String>,
) -> Option<MRuleId> {
    let Some(required_mpr) = ctx.mpr.lex_entry_infl_type(&it.guid) else {
        warnings.push(format!(
            "lexEntryInflType {:?}: does not resolve in the MPR registry; null-affix rule skipped",
            it.guid
        ));
        return None;
    };

    let out_syn_fs = match &it.inflection_features {
        Some(fs) if !fs.values.is_empty() => {
            match super::features::build_syn_fs(ctx.syn, None, Some(fs)) {
                Ok(v) => acc.fs_interner.intern(v),
                Err(e) => {
                    warnings.push(format!(
                        "lexEntryInflType {:?}: {e}; null-affix rule skipped",
                        it.guid
                    ));
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
        warnings.push(format!(
            "lexEntryInflType {:?}: cannot segment null-affix marker {null_ins:?}; rule skipped",
            it.guid
        ));
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
            partial: false,
            max_apps: 1,
            required_syn_fs: acc.fs_interner.intern(pg_featstruct::FeatureStruct::EMPTY),
            out_syn_fs,
            obligatory_features: Vec::new(),
            required_stem_name: None,
            allomorphs: vec![allo],
            is_template_rule: false,
        },
    ));
    Some(mrule_id)
}

fn insert_segments(text: &str, ctx: &Ctx) -> Result<OutputAction, String> {
    let shape = crate::segment::segment(ctx.table, text)
        .map_err(|e| format!("cannot segment {text:?}: {e}"))?;
    Ok(OutputAction::InsertSegments {
        table: ctx.table_id,
        shape: crate::model::SegmentedText {
            text: text.to_string(),
            shape,
        },
    })
}
