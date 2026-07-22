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

#[allow(clippy::too_many_arguments)]
pub(crate) fn build(
    snapshot: &Snapshot,
    ctx: &Ctx,
    acc: &mut Acc,
    morphology_mrules: &mut Vec<MRuleId>,
    clitic_mrules: &mut Vec<MRuleId>,
    morphology_entries: &mut Vec<LexEntryId>,
    clitic_entries: &mut Vec<LexEntryId>,
    warnings: &mut Vec<String>,
) -> Result<(), GrammarError> {
    let infl_type_by_guid: HashMap<&str, &LexEntryInflType> = snapshot
        .morphology
        .lex_entry_infl_types
        .iter()
        .map(|t| (t.guid.as_str(), t))
        .collect();
    let entry_by_guid: HashMap<&str, &LexEntry> = snapshot
        .lexicon
        .entries
        .iter()
        .map(|e| (e.guid.as_str(), e))
        .collect();
    let sense_owner: HashMap<&str, (&LexEntry, &Sense)> = snapshot
        .lexicon
        .entries
        .iter()
        .flat_map(|e| e.senses.iter().map(move |s| (s.guid.as_str(), (e, s))))
        .collect();

    for entry in &snapshot.lexicon.entries {
        // Form partition (HCLoader.cs:256-293): each of an entry's forms lands in the stem or
        // rule bucket for either the Morphology (m_morphophonemic) or Clitics (m_clitic)
        // stratum by morph type. `IsCliticType` = Clitic/Enclitic/Proclitic/Particle; of those,
        // only Enclitic/Proclitic are also rule forms (`IsValidRuleForm`, HCLoader.cs:550-552),
        // so an enclitic form is deliberately in BOTH clitic buckets — HCLoader loads it both as
        // a clitic-stratum lex entry and as a clitic-stratum affix-process rule.
        let clitic = |a: &Allomorph| {
            matches!(
                a.morph_type,
                MorphType::Clitic
                    | MorphType::Enclitic
                    | MorphType::Proclitic
                    | MorphType::Particle
            )
        };
        let affix_allos: Vec<&Allomorph> = entry.allomorphs.iter().filter(|a| !clitic(a)).collect();
        let clitic_affix_allos: Vec<&Allomorph> =
            entry.allomorphs.iter().filter(|a| clitic(a)).collect();
        let has_clitic_stem_form = entry.allomorphs.iter().any(|a| is_lex_entry_form(a, true));
        let has_stem_form = entry.allomorphs.iter().any(|a| is_lex_entry_form(a, false));

        // --- stems: one Grammar LexEntryDef per (entry, Msa::Stem, stratum-bucket) --------------
        for msa in &entry.msas {
            if let Msa::Stem { .. } = msa {
                if has_stem_form {
                    if let Some(id) =
                        build_stem_entry(entry, msa, None, StratumId(0), ctx, acc, warnings)
                    {
                        morphology_entries.push(id);
                    }
                }
                if has_clitic_stem_form {
                    if let Some(id) =
                        build_stem_entry(entry, msa, None, StratumId(1), ctx, acc, warnings)
                    {
                        clitic_entries.push(id);
                    }
                }
            }
        }

        // --- affix rules: one MorphRuleDef per (entry, MSA, non-empty rule bucket) --------------
        // Every MSA kind participates (`LoadMorphologicalRule`'s switch, HCLoader.cs:877-908):
        // deriv/infl/unclassified become ordinary affix rules; a *stem* MSA reached through a
        // rule bucket becomes a clitic affix-process rule (`LoadCliticAffixProcessRule`).
        for msa in &entry.msas {
            let gloss = sense_gloss(entry, msa_guid(msa), ctx);
            // `LoadMorphologicalRule` (HCLoader.cs:887-892): an inflectional MSA with slots
            // sets `s = null` — the rule is reachable ONLY through its template slot(s),
            // never from the stratum's own rule list. Adding it to both would let it apply
            // twice (template + free-floating), and a slot-scoped no-op subrule (e.g. Sena's
            // unrestricted `^0+` noun-class prefixes) applying freely at the stratum level
            // makes the analysis search space explode. Every other affix-rule kind (deriv,
            // unclassified, slotless inflectional = partial, clitic) goes on the stratum list.
            let template_only = matches!(msa, Msa::Inflectional { slots, .. } if !slots.is_empty());
            for (bucket, stratum, mrules) in [
                (&affix_allos, StratumId(0), &mut *morphology_mrules),
                (&clitic_affix_allos, StratumId(1), &mut *clitic_mrules),
            ] {
                if bucket.is_empty() {
                    continue;
                }
                if let Some(id) = affixes::build_affix_rule(
                    entry,
                    bucket,
                    msa,
                    gloss.map(str::to_string),
                    stratum,
                    ctx,
                    acc,
                    warnings,
                ) {
                    if !template_only {
                        mrules.push(id);
                    }
                }
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
                        &affix_allos,
                        ctx,
                        acc,
                        morphology_entries,
                        &mut *morphology_mrules,
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

/// Whether an allomorph is a valid "lex entry form" for the given stratum bucket —
/// `IsValidLexEntryForm` (HCLoader.cs:579-589: non-abstract, non-empty, stem-or-clitic-typed)
/// combined with the caller's clitic-vs-stem bucket split (HCLoader.cs:268-274:
/// `IsCliticType(form.MorphTypeRA)` routes the form to `cliticStemAllos` → the Clitics stratum;
/// stem types — `IsStemType`: root/stem/bound-root/bound-stem/phrase — to `stemAllos` → the
/// Morphology stratum). `Particle` and bare `Clitic` are clitic-typed (HCLoader.cs:609-624), as
/// are `Enclitic`/`Proclitic` — the latter two are *also* rule forms and additionally become
/// clitic-stratum affix rules ([`build`]'s rule buckets).
fn is_lex_entry_form(allo: &Allomorph, clitic: bool) -> bool {
    if allo.is_abstract {
        return false;
    }
    let has_form = !allo.forms.is_empty() && allo.forms.iter().any(|f| !f.form.trim().is_empty());
    if !has_form {
        return false;
    }
    if clitic {
        matches!(
            allo.morph_type,
            MorphType::Clitic | MorphType::Enclitic | MorphType::Proclitic | MorphType::Particle
        )
    } else {
        matches!(
            allo.morph_type,
            MorphType::Root
                | MorphType::Stem
                | MorphType::BoundRoot
                | MorphType::BoundStem
                | MorphType::Phrase
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn build_stem_entry(
    entry: &LexEntry,
    msa: &Msa,
    infl_type: Option<&LexEntryInflType>,
    stratum: StratumId,
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

    let pos_bits = part_of_speech
        .as_deref()
        .and_then(|p| ctx.pos.bits_single(p));
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
    let infl_class_guid = inflection_class.clone().or_else(|| {
        part_of_speech
            .as_deref()
            .and_then(|p| ctx.pos.default_inflection_class(p))
    });
    let mut mpr = crate::model::MprSet::EMPTY;
    if let Some(ic) = &infl_class_guid {
        match ctx.mpr.infl_class_single(ic) {
            Some(s) => mpr = mpr.union(s),
            None => warnings.push(format!(
                "MSA {guid:?}: inflection class {ic:?} does not resolve"
            )),
        }
    }
    for f in exception_features {
        match ctx.mpr.exception_feature(f) {
            Some(s) => mpr = mpr.union(s),
            None => warnings.push(format!(
                "MSA {guid:?}: exception feature {f:?} does not resolve"
            )),
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
    let clitic = stratum == StratumId(1);
    for allo in entry
        .allomorphs
        .iter()
        .filter(|a| is_lex_entry_form(a, clitic))
    {
        match build_root_allomorph(allo, ctx, warnings) {
            Ok(def) => {
                let allo_id = crate::model::AllomorphId(acc.allomorph_owners.len() as u32);
                acc.allomorph_owners
                    .push(crate::model::AllomorphOwner::Root(
                        lex_id,
                        allomorphs.len() as u16,
                    ));
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
        authored_id: entry.guid.clone(),
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
        stratum,
        properties: Vec::new(),
        co_occurrence: Vec::new(),
    });
    Some(lex_id)
}

/// `LoadRootAllomorph` (HCLoader.cs:809-845). Uses the pattern-aware segmenter (finding N3 in
/// `crate::segment`, mirroring `crate::load::load_root_allomorph`'s own choice) since a root
/// form may carry a lexical lookup pattern (`[C]`-style underspecified segments).
fn build_root_allomorph(
    allo: &Allomorph,
    ctx: &Ctx,
    warnings: &mut Vec<String>,
) -> Result<RootAllomorphDef, String> {
    let form = super::best_ws(&allo.forms, ctx.default_vernacular_ws.as_deref()).unwrap_or("");
    let form = super::format_form(form);
    let shape = crate::segment::segment_with_patterns(ctx.table, natural_class_defs(ctx), &form)
        .map_err(|e| format!("cannot segment {form:?}: {e}"))?;
    if shape
        .interior()
        .all(|(_, k, _, _)| k == pg_shape::NodeKind::Boundary)
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

    let stem_name = allo
        .stem_name
        .as_deref()
        .and_then(|sn| ctx.stem_name_by_guid.get(sn).copied());

    let is_bound = matches!(allo.morph_type, MorphType::BoundRoot | MorphType::BoundStem);

    let is_pattern = shape.interior().any(|(_, kind, _, flags)| {
        flags.is_iterative() || (flags.is_optional() && kind != pg_shape::NodeKind::Boundary)
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
///
/// A main-entry MSA that is *not* `Msa::Stem` (derivational/inflectional/unclassified) instead
/// goes through [`build_variant_affix_rule`] — `LoadMorphologicalRules`'s senses-empty branch
/// (HCLoader.cs:852-871) walks *every* `mainEntry.MorphoSyntaxAnalysesOC`, not just stem MSAs, and
/// `LoadMorphologicalRule`'s dispatch (HCLoader.cs:877-908) builds an ordinary affix-process rule
/// for each — using the *variant's own* allomorphs as the rule's forms but the main entry's MSA
/// for every rule-level field. Confirmed against the gate's own diff (not merely inferred): legacy
/// really does end up with a **second, distinct** `AffixProcessRule` object for the same gloss in
/// this case (`LoadInflAffixProcessRule` always `new AffixProcessRule{...}`, HCLoader.cs:979-1006;
/// both rules end up in `m_morphemes[msa]`, and `LoadAffixTemplate`'s `slot.Affixes` walk,
/// HCLoader.cs:1704, pulls in both) — never a merged allomorph list on the pre-existing rule.
#[allow(clippy::too_many_arguments)]
fn build_variant(
    variant_entry: &LexEntry,
    component_lexemes: &[String],
    variant_entry_types: &[String],
    entry_by_guid: &HashMap<&str, &LexEntry>,
    sense_owner: &HashMap<&str, (&LexEntry, &Sense)>,
    infl_type_by_guid: &HashMap<&str, &LexEntryInflType>,
    variant_affix_allos: &[&Allomorph],
    ctx: &Ctx,
    acc: &mut Acc,
    morphology_entries: &mut Vec<LexEntryId>,
    morphology_mrules: &mut Vec<MRuleId>,
    warnings: &mut Vec<String>,
) {
    let infl_types = get_infl_types(variant_entry_types, infl_type_by_guid);

    for component in component_lexemes {
        let main_msas: Vec<(&LexEntry, &Msa)> =
            if let Some(&e) = entry_by_guid.get(component.as_str()) {
                e.msas.iter().map(|m| (e, m)).collect()
            } else if let Some(&(e, sense)) = sense_owner.get(component.as_str()) {
                e.msas
                    .iter()
                    .filter(|m| Some(m.guid()) == sense.msa.as_deref())
                    .map(|m| (e, m))
                    .collect()
            } else {
                warnings.push(format!(
                "variant entry {:?}: component {component:?} does not resolve to an entry or sense",
                variant_entry.guid
            ));
                Vec::new()
            };

        // Stem MSAs: one variant LexEntryDef per (infl_type, msa) pair -- infl_type-sensitive
        // gloss prepend/append + inflFeats merge (`build_variant_stem_entry`).
        for &infl_type in &infl_types {
            for &(main_entry, msa) in &main_msas {
                if !matches!(msa, Msa::Stem { .. }) {
                    continue;
                }
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

        // Non-stem MSAs: a second affix-process rule using the variant's own forms (see this
        // fn's doc). Not infl_type-sensitive at all -- `LoadMorphologicalRule`'s senses-empty
        // branch never touches `variant_entry_types`, confirmed by the gate diff rendering the
        // plain gloss "3f", not an infl-type-decorated one -- so this runs once per (main_entry,
        // msa), independent of the infl_type loop above.
        for &(main_entry, msa) in &main_msas {
            if matches!(msa, Msa::Stem { .. }) {
                continue;
            }
            if let Some(id) = build_variant_affix_rule(
                variant_entry,
                main_entry,
                msa,
                variant_affix_allos,
                ctx,
                acc,
                warnings,
            ) {
                let template_only =
                    matches!(msa, Msa::Inflectional { slots, .. } if !slots.is_empty());
                if !template_only {
                    morphology_mrules.push(id);
                }
            }
        }
    }
}

/// The non-stem counterpart of [`build_variant_stem_entry`] (see [`build_variant`]'s doc for the
/// full HCLoader rationale): builds a full second `AffixProcessRule` for `msa` using
/// `variant_entry`'s own rule-form allomorphs, reusing [`affixes::build_affix_rule`] verbatim so
/// every mrule-level field (reqfs/outfs/partial/stratum/slot registration) is derived exactly the
/// way the main entry's own rule for this same MSA was derived. `entry.ShortName`/circumfix
/// classification in HCLoader always come from the entry passed to `LoadMorphologicalRule` --
/// which stays the *variant* entry throughout (`LoadMorphologicalRules(stratum, entry, allos)`'s
/// `entry` parameter is never reassigned to the main entry) -- so `variant_entry`, not
/// `main_entry`, is passed as `build_affix_rule`'s `entry` argument here.
fn build_variant_affix_rule(
    variant_entry: &LexEntry,
    main_entry: &LexEntry,
    msa: &Msa,
    variant_affix_allos: &[&Allomorph],
    ctx: &Ctx,
    acc: &mut Acc,
    warnings: &mut Vec<String>,
) -> Option<MRuleId> {
    let gloss = sense_gloss(main_entry, msa_guid(msa), ctx).map(str::to_string);
    affixes::build_affix_rule(
        variant_entry,
        variant_affix_allos,
        msa,
        gloss,
        StratumId(0),
        ctx,
        acc,
        warnings,
    )
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

    let pos_bits = part_of_speech
        .as_deref()
        .and_then(|p| ctx.pos.bits_single(p));

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

    let syn_fs = match super::features::build_syn_fs(ctx.syn, pos_bits, merged_ms_features.as_ref())
    {
        Ok(fs) => acc.fs_interner.intern(fs),
        Err(e) => {
            warnings.push(format!("MSA {guid:?}: {e}; variant entry skipped"));
            return None;
        }
    };
    let partial = part_of_speech.is_none();

    let infl_class_guid = inflection_class.clone().or_else(|| {
        part_of_speech
            .as_deref()
            .and_then(|p| ctx.pos.default_inflection_class(p))
    });
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
    // Variants are built for the Morphology bucket only (clitic-typed variant forms are a known
    // gap — no reference corpus exercises them; see `build`'s partition doc).
    for allo in variant_entry
        .allomorphs
        .iter()
        .filter(|a| is_lex_entry_form(a, false))
    {
        match build_root_allomorph(allo, ctx, warnings) {
            Ok(def) => {
                let allo_id = crate::model::AllomorphId(acc.allomorph_owners.len() as u32);
                acc.allomorph_owners
                    .push(crate::model::AllomorphOwner::Root(
                        lex_id,
                        allomorphs.len() as u16,
                    ));
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
        authored_id: variant_entry.guid.clone(),
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
