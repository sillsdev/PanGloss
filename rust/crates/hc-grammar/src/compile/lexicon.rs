//! Stems (`LoadLexEntries`/`LoadLexEntry`, HCLoader.cs:626-731), variants
//! (`LoadLexEntryOfVariant`, HCLoader.cs:733-807), root allomorphs (HCLoader.cs:809-845), and the
//! per-entry dispatch into affix rules (`affixes::build_affix_rule`).
//!
//! One `LexEntryDef` is built per (LexEntry, `Msa::Stem`) pair — mirroring HCLoader's own
//! `LexEntry`-per-MSA fan-out (`LoadLexEntry`, HCLoader.cs:691-731): an LCM entry with two stem
//! MSAs becomes two `Grammar` lexical entries sharing the same root-allomorph list.

use hashbrown::HashMap;

use pg_snapshot::lexicon::{Allomorph, EntryRef, LexEntry, Msa, Sense};
use pg_snapshot::morphology::{LexEntryInflType, MorphType};
use pg_snapshot::Snapshot;

use crate::model::{LexEntryDef, LexEntryId, MRuleId, RootAllomorphDef, StratumId};
use crate::GrammarError;

use super::{affixes, Acc, Ctx};

pub(crate) fn build(
    snapshot: &Snapshot,
    ctx: &Ctx,
    acc: &mut Acc,
    morphology_mrules: &mut Vec<MRuleId>,
    clitic_mrules: &mut Vec<MRuleId>,
    morphology_entries: &mut Vec<LexEntryId>,
    warnings: &mut Vec<String>,
) -> Result<(), GrammarError> {
    let _ = clitic_mrules; // Clitics stratum is intentionally left empty — see affixes::shape_of.

    let infl_type_by_guid: HashMap<&str, &LexEntryInflType> = snapshot
        .morphology
        .lex_entry_infl_types
        .iter()
        .map(|t| (t.guid.as_str(), t))
        .collect();
    let entry_by_guid: HashMap<&str, &LexEntry> =
        snapshot.lexicon.entries.iter().map(|e| (e.guid.as_str(), e)).collect();
    let sense_owner: HashMap<&str, (&LexEntry, &Sense)> = snapshot
        .lexicon
        .entries
        .iter()
        .flat_map(|e| e.senses.iter().map(move |s| (s.guid.as_str(), (e, s))))
        .collect();

    for entry in &snapshot.lexicon.entries {
        warn_unsupported_clitic_forms(entry, warnings);

        // --- stems: one Grammar LexEntryDef per (entry, Msa::Stem) pair -------------------------
        for msa in &entry.msas {
            if let Msa::Stem { .. } = msa {
                if let Some(id) = build_stem_entry(entry, msa, None, ctx, acc, warnings) {
                    morphology_entries.push(id);
                }
            }
        }

        // --- affix rules: one MorphRuleDef per (entry, non-stem MSA) pair -----------------------
        for msa in &entry.msas {
            if matches!(msa, Msa::Stem { .. }) {
                continue;
            }
            let gloss = sense_gloss(entry, msa_guid(msa), ctx);
            let stratum = StratumId(0); // "Morphology" — see `mod.rs`'s stratum layout.
            if let Some(id) =
                affixes::build_affix_rule(entry, msa, gloss.map(str::to_string), stratum, ctx, acc, warnings)
            {
                morphology_mrules.push(id);
            }
        }

        // --- variants: entries with no senses of their own, linked via `EntryRef::Variant` ------
        if entry.senses.is_empty() {
            for er in &entry.entry_refs {
                if let EntryRef::Variant {
                    component_lexemes,
                    variant_entry_types,
                    ..
                } = er
                {
                    build_variant(
                        entry,
                        component_lexemes,
                        variant_entry_types,
                        &entry_by_guid,
                        &sense_owner,
                        &infl_type_by_guid,
                        ctx,
                        acc,
                        morphology_entries,
                        warnings,
                    );
                }
            }
        }
    }

    Ok(())
}

fn msa_guid(msa: &Msa) -> &str {
    msa.guid()
}

fn sense_gloss<'a>(entry: &'a LexEntry, msa: &str, ctx: &Ctx) -> Option<&'a str> {
    entry
        .senses
        .iter()
        .find(|s| s.msa.as_deref() == Some(msa))
        .and_then(|s| super::best_ws(&s.gloss, ctx.default_analysis_ws.as_deref()))
}

/// Whether an allomorph is a valid "lex entry form" — `IsValidLexEntryForm`, HCLoader.cs:579-589:
/// stem-type, non-abstract, non-empty. Clitic-typed forms (`Clitic`/`Enclitic`/`Proclitic`) are
/// deliberately excluded here even though HCLoader treats them as forms too: HCLoader places
/// clitics on the dedicated Clitics stratum with dual stem+rule handling, which this compiler does
/// not implement (see `affixes::shape_of`, also Phase B). Compiling them onto the Morphology
/// stratum as plain stems would be a silent semantic drift rather than an honest "unsupported"
/// skip, so [`build`] warns and drops them instead — see `warn_unsupported_clitic_forms`.
/// `Particle` remains accepted: it has no dedicated stratum handling in HCLoader beyond being a
/// plain stem-shaped form.
fn is_lex_entry_form(allo: &Allomorph) -> bool {
    if allo.is_abstract {
        return false;
    }
    let has_form = !allo.forms.is_empty() && allo.forms.iter().any(|f| !f.form.trim().is_empty());
    if !has_form {
        return false;
    }
    matches!(
        allo.morph_type,
        MorphType::Root | MorphType::Stem | MorphType::BoundRoot | MorphType::BoundStem | MorphType::Phrase | MorphType::Particle
    )
}

/// Emits one warning per allomorph that is otherwise a valid, non-empty form but was excluded from
/// [`is_lex_entry_form`] solely because of its clitic morph type — so clitic forms are dropped
/// loudly (Phase B) rather than silently disappearing from the compiled `Grammar`.
fn warn_unsupported_clitic_forms(entry: &LexEntry, warnings: &mut Vec<String>) {
    for allo in &entry.allomorphs {
        if allo.is_abstract {
            continue;
        }
        let has_form = !allo.forms.is_empty() && allo.forms.iter().any(|f| !f.form.trim().is_empty());
        if !has_form {
            continue;
        }
        if matches!(
            allo.morph_type,
            MorphType::Clitic | MorphType::Enclitic | MorphType::Proclitic
        ) {
            warnings.push(format!(
                "unsupported: allomorph {:?} of entry {:?} has clitic morph type {:?}; clitics stratum \
                 is not implemented, form skipped",
                allo.guid, entry.guid, allo.morph_type
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_stem_entry(
    entry: &LexEntry,
    msa: &Msa,
    infl_type: Option<&LexEntryInflType>,
    ctx: &Ctx,
    acc: &mut Acc,
    warnings: &mut Vec<String>,
) -> Option<LexEntryId> {
    let Msa::Stem {
        guid,
        part_of_speech,
        inflection_class,
        features,
        exception_features,
        ..
    } = msa
    else {
        return None;
    };

    let pos_bits = part_of_speech.as_deref().and_then(|p| ctx.pos.bits_single(p));
    let syn_fs = match super::features::build_syn_fs(ctx.syn, pos_bits, features.as_ref()) {
        Ok(fs) => acc.fs_interner.intern(fs),
        Err(e) => {
            warnings.push(format!("MSA {guid:?}: {e}; entry skipped"));
            return None;
        }
    };
    let partial = part_of_speech.is_none();

    // `GetInflClass` (HCLoader.cs:2625-2632): explicit inflection class, else the owning POS's
    // default walked up the ownership chain (`GetDefaultInflClass`).
    let infl_class_guid = inflection_class
        .clone()
        .or_else(|| part_of_speech.as_deref().and_then(|p| ctx.pos.default_inflection_class(p)));
    let mut mpr = crate::model::MprSet::EMPTY;
    if let Some(ic) = &infl_class_guid {
        match ctx.mpr.infl_class_single(ic) {
            Some(s) => mpr = mpr.union(s),
            None => warnings.push(format!("MSA {guid:?}: inflection class {ic:?} does not resolve")),
        }
    }
    for f in exception_features {
        match ctx.mpr.exception_feature(f) {
            Some(s) => mpr = mpr.union(s),
            None => warnings.push(format!("MSA {guid:?}: exception feature {f:?} does not resolve")),
        }
    }
    if let Some(it) = infl_type {
        match ctx.mpr.lex_entry_infl_type(&it.guid) {
            Some(s) => mpr = mpr.union(s),
            None => warnings.push(format!(
                "lexEntryInflType {:?} does not resolve in the MPR registry",
                it.guid
            )),
        }
    }

    let lex_id = LexEntryId(acc.entries.len() as u32);
    let mut allomorphs = Vec::new();
    for allo in entry.allomorphs.iter().filter(|a| is_lex_entry_form(a)) {
        match build_root_allomorph(allo, ctx, warnings) {
            Ok(def) => {
                let allo_id = crate::model::AllomorphId(acc.allomorph_owners.len() as u32);
                acc.allomorph_owners
                    .push(crate::model::AllomorphOwner::Root(lex_id, allomorphs.len() as u16));
                acc.allomorph_guid_index.insert(allo.guid.clone(), allo_id);
                allomorphs.push(RootAllomorphDef { id: allo_id, ..def });
            }
            Err(e) => warnings.push(format!("allomorph {:?}: {e}; skipped", allo.guid)),
        }
    }
    if allomorphs.is_empty() {
        return None;
    }

    acc.entries.push(LexEntryDef {
        morpheme: crate::model::MorphemeId(acc.morphemes.len() as u32),
        syn_fs,
        mpr,
        partial,
        allomorphs,
        family: None,
    });
    acc.morphemes.push(crate::model::MorphemeInfo {
        xml_key: guid.clone(),
        morph_id: None,
        gloss: sense_gloss(entry, guid, ctx).map(str::to_string),
        stratum: StratumId(0),
        properties: Vec::new(),
        co_occurrence: Vec::new(),
    });
    Some(lex_id)
}

/// `LoadRootAllomorph` (HCLoader.cs:809-845). Uses the pattern-aware segmenter (finding N3 in
/// `crate::segment`, mirroring `crate::load::load_root_allomorph`'s own choice) since a root
/// form may carry a lexical lookup pattern (`[C]`-style underspecified segments).
fn build_root_allomorph(allo: &Allomorph, ctx: &Ctx, warnings: &mut Vec<String>) -> Result<RootAllomorphDef, String> {
    let form = super::best_ws(&allo.forms, ctx.default_vernacular_ws.as_deref()).unwrap_or("");
    let form = super::format_form(form);
    let shape = crate::segment::segment_with_patterns(ctx.table, natural_class_defs(ctx), &form)
        .map_err(|e| format!("cannot segment {form:?}: {e}"))?;
    if shape
        .interior()
        .all(|(_, k, _, _)| k == hc_shape::NodeKind::Boundary)
    {
        return Err(format!("root allomorph shape {form:?} is all boundaries"));
    }

    let mut environments = Vec::new();
    for env_guid in &allo.environments {
        let Some(env) = ctx.env_by_guid.get(env_guid.as_str()) else {
            continue;
        };
        // Invalid environment: HCLoader logs and drops just this one context.
        match super::environment::parse_environment(&env.representation, ctx) {
            Ok((left, right)) => environments.push(crate::model::EnvironmentDef {
                require: true,
                left,
                right,
            }),
            Err(e) => warnings.push(format!(
                "allomorph {:?}: invalid environment {:?} ({}): {e}; treated as absent",
                allo.guid, env.guid, env.representation
            )),
        }
    }

    let stem_name = allo.stem_name.as_deref().and_then(|sn| ctx.stem_name_by_guid.get(sn).copied());

    let is_bound = matches!(allo.morph_type, MorphType::BoundRoot | MorphType::BoundStem);

    let is_pattern = shape.interior().any(|(_, kind, _, flags)| {
        flags.is_iterative() || (flags.is_optional() && kind != hc_shape::NodeKind::Boundary)
    });

    Ok(RootAllomorphDef {
        id: crate::model::AllomorphId(0),
        shape: crate::model::SegmentedText { text: form, shape },
        is_bound,
        environments,
        co_occurrence: Vec::new(),
        properties: Vec::new(),
        stem_name,
        is_pattern,
    })
}

/// Natural classes are only needed by [`build_root_allomorph`]'s pattern-language segmentation;
/// exposed via a tiny accessor since `Ctx` does not carry the definitions slice directly (only
/// guid/name lookups) — this crate's [`crate::segment::segment_with_patterns`] needs the full
/// `&[NaturalClass]` list, so `mod.rs` stashes it on `Ctx` for this one call site.
fn natural_class_defs<'a>(ctx: &Ctx<'a>) -> &'a [crate::model::NaturalClass] {
    ctx.natural_class_defs
}

/// `LoadLexEntryOfVariant` (HCLoader.cs:733-807): a variant entry (no senses of its own) borrows
/// its own allomorphs but the *referenced* main entry/sense's stem MSA, gloss (with
/// prepend/append), and inflection features.
#[allow(clippy::too_many_arguments)]
fn build_variant(
    variant_entry: &LexEntry,
    component_lexemes: &[String],
    variant_entry_types: &[String],
    entry_by_guid: &HashMap<&str, &LexEntry>,
    sense_owner: &HashMap<&str, (&LexEntry, &Sense)>,
    infl_type_by_guid: &HashMap<&str, &LexEntryInflType>,
    ctx: &Ctx,
    acc: &mut Acc,
    morphology_entries: &mut Vec<LexEntryId>,
    warnings: &mut Vec<String>,
) {
    let infl_types = get_infl_types(variant_entry_types, infl_type_by_guid);

    for component in component_lexemes {
        let main_msas: Vec<(&LexEntry, &Msa)> = if let Some(&e) = entry_by_guid.get(component.as_str()) {
            e.msas.iter().filter(|m| matches!(m, Msa::Stem { .. })).map(|m| (e, m)).collect()
        } else if let Some(&(e, sense)) = sense_owner.get(component.as_str()) {
            e.msas
                .iter()
                .filter(|m| matches!(m, Msa::Stem { .. }) && Some(m.guid()) == sense.msa.as_deref())
                .map(|m| (e, m))
                .collect()
        } else {
            warnings.push(format!(
                "variant entry {:?}: component {component:?} does not resolve to an entry or sense",
                variant_entry.guid
            ));
            Vec::new()
        };

        for &infl_type in &infl_types {
            for &(main_entry, msa) in &main_msas {
                if let Some(id) = build_variant_stem_entry(
                    variant_entry,
                    main_entry,
                    msa,
                    infl_type,
                    ctx,
                    acc,
                    warnings,
                ) {
                    morphology_entries.push(id);
                }
            }
        }
    }
}

fn get_infl_types<'a>(
    variant_entry_types: &[String],
    infl_type_by_guid: &HashMap<&str, &'a LexEntryInflType>,
) -> Vec<Option<&'a LexEntryInflType>> {
    if variant_entry_types.is_empty() {
        return vec![None];
    }
    let mut out = Vec::new();
    let mut normal_found = false;
    for t in variant_entry_types {
        if let Some(&it) = infl_type_by_guid.get(t.as_str()) {
            out.push(Some(it));
        } else if !normal_found {
            out.push(None);
            normal_found = true;
        }
    }
    if out.is_empty() {
        out.push(None);
    }
    out
}

/// The variant-entry counterpart of [`build_stem_entry`]: allomorphs come from `variant_entry`,
/// everything else (POS, inflection class, base gloss, inflection features) from `main_entry`'s
/// MSA, plus the `infl_type`'s gloss prepend/append and inflection-feature merge
/// (HCLoader.cs:748-786).
fn build_variant_stem_entry(
    variant_entry: &LexEntry,
    main_entry: &LexEntry,
    msa: &Msa,
    infl_type: Option<&LexEntryInflType>,
    ctx: &Ctx,
    acc: &mut Acc,
    warnings: &mut Vec<String>,
) -> Option<LexEntryId> {
    let Msa::Stem {
        guid,
        part_of_speech,
        inflection_class,
        features,
        exception_features,
        ..
    } = msa
    else {
        return None;
    };

    let pos_bits = part_of_speech.as_deref().and_then(|p| ctx.pos.bits_single(p));

    // Merge MsFeatures with the inflType's own InflFeats (HCLoader.cs:769-782: the inflType's FS
    // is added to/replaces the head feature's value; both are `{closed feature -> value}` lists
    // over the same feature system, so a plain value-list concatenation mirrors `FeatureStruct.Add`
    // closely enough for the non-conflicting case every reference grammar exercises).
    let mut merged_ms_features = features.clone();
    if let Some(it) = infl_type {
        if let Some(inflfs) = &it.inflection_features {
            if !inflfs.values.is_empty() {
                let base = merged_ms_features.get_or_insert_with(Default::default);
                base.values.extend(inflfs.values.iter().cloned());
            }
        }
    }

    let syn_fs = match super::features::build_syn_fs(ctx.syn, pos_bits, merged_ms_features.as_ref()) {
        Ok(fs) => acc.fs_interner.intern(fs),
        Err(e) => {
            warnings.push(format!("MSA {guid:?}: {e}; variant entry skipped"));
            return None;
        }
    };
    let partial = part_of_speech.is_none();

    let infl_class_guid = inflection_class
        .clone()
        .or_else(|| part_of_speech.as_deref().and_then(|p| ctx.pos.default_inflection_class(p)));
    let mut mpr = crate::model::MprSet::EMPTY;
    if let Some(ic) = &infl_class_guid {
        if let Some(s) = ctx.mpr.infl_class_single(ic) {
            mpr = mpr.union(s);
        }
    }
    for f in exception_features {
        if let Some(s) = ctx.mpr.exception_feature(f) {
            mpr = mpr.union(s);
        }
    }
    if let Some(it) = infl_type {
        if let Some(s) = ctx.mpr.lex_entry_infl_type(&it.guid) {
            mpr = mpr.union(s);
        }
    }

    let lex_id = LexEntryId(acc.entries.len() as u32);
    let mut allomorphs = Vec::new();
    for allo in variant_entry.allomorphs.iter().filter(|a| is_lex_entry_form(a)) {
        match build_root_allomorph(allo, ctx, warnings) {
            Ok(def) => {
                let allo_id = crate::model::AllomorphId(acc.allomorph_owners.len() as u32);
                acc.allomorph_owners
                    .push(crate::model::AllomorphOwner::Root(lex_id, allomorphs.len() as u16));
                acc.allomorph_guid_index.insert(allo.guid.clone(), allo_id);
                allomorphs.push(RootAllomorphDef { id: allo_id, ..def });
            }
            Err(e) => warnings.push(format!("allomorph {:?}: {e}; skipped", allo.guid)),
        }
    }
    if allomorphs.is_empty() {
        return None;
    }

    // Gloss: `inflType.GlossPrepend` (unless the literal "***" sentinel) + base gloss + `GlossAppend`.
    let base_gloss = sense_gloss(main_entry, guid, ctx).unwrap_or("");
    let mut gloss = String::new();
    if let Some(it) = infl_type {
        if it.gloss_prepend != "***" {
            gloss.push_str(&it.gloss_prepend);
        }
    }
    gloss.push_str(base_gloss);
    if let Some(it) = infl_type {
        if it.gloss_append != "***" {
            gloss.push_str(&it.gloss_append);
        }
    }

    acc.entries.push(LexEntryDef {
        morpheme: crate::model::MorphemeId(acc.morphemes.len() as u32),
        syn_fs,
        mpr,
        partial,
        allomorphs,
        family: None,
    });
    acc.morphemes.push(crate::model::MorphemeInfo {
        xml_key: format!("{}#{}", variant_entry.guid, guid),
        morph_id: None,
        gloss: (!gloss.is_empty()).then_some(gloss),
        stratum: StratumId(0),
        properties: Vec::new(),
        co_occurrence: Vec::new(),
    });
    Some(lex_id)
}
