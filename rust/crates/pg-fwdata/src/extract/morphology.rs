//! `morphology` snapshot section — see `docs/snapshot-format.md` §5.

use pg_snapshot::{
    AdhocProhibition, Adjacency, AffixSlot, AffixTemplate, CompoundConstituentRequirement,
    CompoundOutcome, CompoundRule, ExceptionFeature, FeatureSystems, InflectionClass,
    LexEntryInflType, Lexicon, Morphology, PartOfSpeech, StemName,
};

use super::features::extract_feature_structure;
use super::Ctx;
use crate::parser_params;
use crate::xml::Record;

pub fn extract_morphology(
    ctx: &mut Ctx,
    lang_project: Option<&Record>,
    _feature_systems: &FeatureSystems,
) -> Morphology {
    let parts_of_speech = lang_project
        .and_then(|lp| lp.node.objsur_one("PartsOfSpeech"))
        .map(|list_guid| extract_pos_forest(ctx, &list_guid))
        .unwrap_or_default();

    let morph_data = lang_project
        .and_then(|lp| lp.node.objsur_one("MorphologicalData"))
        .and_then(|guid| ctx.require(&guid, "MoMorphData", "morphology"));

    let compound_rules = morph_data
        .map(|md| extract_compound_rules(ctx, md))
        .unwrap_or_default();
    let adhoc_prohibitions = morph_data
        .map(|md| extract_adhoc_prohibitions(ctx, md))
        .unwrap_or_default();

    let phon_data = lang_project
        .and_then(|lp| lp.node.objsur_one("PhonologicalData"))
        .and_then(|guid| ctx.get(&guid));

    let exception_features = extract_exception_features(ctx, morph_data, phon_data);

    let lex_db = lang_project
        .and_then(|lp| lp.node.objsur_one("LexDb"))
        .and_then(|guid| ctx.require(&guid, "LexDb", "morphology.lexEntryInflTypes"));
    let lex_entry_infl_types = lex_db
        .map(|db| extract_lex_entry_infl_types(ctx, db))
        .unwrap_or_default();

    let parser_parameters = parser_params::parse(
        morph_data
            .and_then(|md| md.node.uni_text("ParserParameters"))
            .as_deref(),
    );

    Morphology {
        parts_of_speech,
        compound_rules,
        adhoc_prohibitions,
        exception_features,
        lex_entry_infl_types,
        parser_parameters,
    }
}

// ---------------------------------------------------------------------------------------------
// Parts of speech
// ---------------------------------------------------------------------------------------------

fn extract_pos_forest(ctx: &mut Ctx, list_guid: &str) -> Vec<PartOfSpeech> {
    let Some(list) = ctx.require(list_guid, "CmPossibilityList", "morphology.partsOfSpeech") else {
        return Vec::new();
    };
    list.node
        .objsur_list("Possibilities")
        .into_iter()
        .filter_map(|g| extract_pos(ctx, &g))
        .collect()
}

fn extract_pos(ctx: &mut Ctx, guid: &str) -> Option<PartOfSpeech> {
    let rec = ctx.require(guid, "PartOfSpeech", "morphology.partsOfSpeech")?;
    let name = ctx.best_analysis(&rec.node.ws_forms("Name"));
    let abbreviation = ctx.best_analysis(&rec.node.ws_forms("Abbreviation"));
    let children = rec
        .node
        .objsur_list("SubPossibilities")
        .into_iter()
        .filter_map(|g| extract_pos(ctx, &g))
        .collect();
    let inflection_classes = rec
        .node
        .objsur_list("InflectionClasses")
        .into_iter()
        .filter_map(|g| extract_inflection_class(ctx, &g))
        .collect();
    let default_inflection_class = rec.node.objsur_one("DefaultInflectionClass");
    let inflectable_features = rec.node.objsur_list("InflectableFeats");
    let stem_names = rec
        .node
        .objsur_list("StemNames")
        .into_iter()
        .filter_map(|g| extract_stem_name(ctx, &g))
        .collect();
    let affix_slots = rec
        .node
        .objsur_list("AffixSlots")
        .into_iter()
        .filter_map(|g| extract_affix_slot(ctx, &g))
        .collect();
    let affix_templates = rec
        .node
        .objsur_list("AffixTemplates")
        .into_iter()
        .filter_map(|g| extract_affix_template(ctx, &g))
        .collect();
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

fn extract_inflection_class(ctx: &mut Ctx, guid: &str) -> Option<InflectionClass> {
    let rec = ctx.require(
        guid,
        "MoInflClass",
        "morphology.partsOfSpeech.inflectionClasses",
    )?;
    let children = rec
        .node
        .objsur_list("Subclasses")
        .into_iter()
        .filter_map(|g| extract_inflection_class(ctx, &g))
        .collect();
    Some(InflectionClass {
        guid: guid.to_string(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        abbreviation: ctx.best_analysis(&rec.node.ws_forms("Abbreviation")),
        children,
    })
}

fn extract_stem_name(ctx: &mut Ctx, guid: &str) -> Option<StemName> {
    let rec = ctx.require(guid, "MoStemName", "morphology.partsOfSpeech.stemNames")?;
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
        .filter_map(|g| extract_feature_structure(ctx, &g, "morphology.partsOfSpeech.stemNames"))
        .collect();
    Some(StemName {
        guid: guid.to_string(),
        name,
        abbreviation,
        regions,
    })
}

fn extract_affix_slot(ctx: &mut Ctx, guid: &str) -> Option<AffixSlot> {
    let rec = ctx.require(
        guid,
        "MoInflAffixSlot",
        "morphology.partsOfSpeech.affixSlots",
    )?;
    Some(AffixSlot {
        guid: guid.to_string(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        optional: rec.node.val_bool("Optional").unwrap_or(false),
    })
}

fn extract_affix_template(ctx: &mut Ctx, guid: &str) -> Option<AffixTemplate> {
    let rec = ctx.require(
        guid,
        "MoInflAffixTemplate",
        "morphology.partsOfSpeech.affixTemplates",
    )?;
    Some(AffixTemplate {
        guid: guid.to_string(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        disabled: rec.node.val_bool("Disabled").unwrap_or(false),
        prefix_slots: rec.node.objsur_list("PrefixSlots"),
        suffix_slots: rec.node.objsur_list("SuffixSlots"),
        is_final: rec.node.val_bool("Final").unwrap_or(false),
    })
}

// ---------------------------------------------------------------------------------------------
// Compound rules
// ---------------------------------------------------------------------------------------------

fn extract_compound_rules(ctx: &mut Ctx, morph_data: &Record) -> Vec<CompoundRule> {
    morph_data
        .node
        .objsur_list("CompoundRules")
        .into_iter()
        .filter_map(|g| extract_compound_rule(ctx, &g))
        .collect()
}

fn extract_compound_rule(ctx: &mut Ctx, guid: &str) -> Option<CompoundRule> {
    let rec = ctx.get(guid)?;
    let name = ctx.best_analysis(&rec.node.ws_forms("Name"));
    let disabled = rec.node.val_bool("Disabled").unwrap_or(false);
    let label = "morphology.compoundRules";
    match rec.class.as_str() {
        "MoEndoCompound" => {
            let head_last = rec.node.val_bool("HeadLast").unwrap_or(false);
            let left = compound_side(ctx, rec.node.objsur_one("LeftMsa"), label);
            let right = compound_side(ctx, rec.node.objsur_one("RightMsa"), label);
            let overriding = compound_outcome(ctx, rec.node.objsur_one("OverridingMsa"), label);
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
            let left = compound_side(ctx, rec.node.objsur_one("LeftMsa"), label);
            let right = compound_side(ctx, rec.node.objsur_one("RightMsa"), label);
            let to = compound_outcome(ctx, rec.node.objsur_one("ToMsa"), label);
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
            ctx.warn(format!("{label}: {guid} has unexpected class {other}"));
            None
        }
    }
}

/// A compound side/outcome is always an `MoStemMsa`, but `HCLoader` only ever reads its
/// `PartOfSpeechRA`/`ProdRestrictRC` pair for a *side* requirement (HCLoader.cs:1848-1941).
fn compound_side(
    ctx: &mut Ctx,
    msa_guid: Option<String>,
    label: &str,
) -> CompoundConstituentRequirement {
    let Some(guid) = msa_guid else {
        return CompoundConstituentRequirement::default();
    };
    let Some(rec) = ctx.require(&guid, "MoStemMsa", label) else {
        return CompoundConstituentRequirement::default();
    };
    CompoundConstituentRequirement {
        part_of_speech: rec.node.objsur_one("PartOfSpeech"),
        exception_features: rec.node.objsur_list("ProdRestrict"),
    }
}

fn compound_outcome(ctx: &mut Ctx, msa_guid: Option<String>, label: &str) -> CompoundOutcome {
    let Some(guid) = msa_guid else {
        return CompoundOutcome::default();
    };
    let Some(rec) = ctx.require(&guid, "MoStemMsa", label) else {
        return CompoundOutcome::default();
    };
    CompoundOutcome {
        part_of_speech: rec.node.objsur_one("PartOfSpeech"),
        inflection_class: rec.node.objsur_one("InflectionClass"),
    }
}

// ---------------------------------------------------------------------------------------------
// Ad-hoc co-occurrence prohibitions
// ---------------------------------------------------------------------------------------------

fn extract_adhoc_prohibitions(ctx: &mut Ctx, morph_data: &Record) -> Vec<AdhocProhibition> {
    morph_data
        .node
        .objsur_list("AdhocCoProhibitions")
        .into_iter()
        .filter_map(|g| extract_adhoc_prohibition(ctx, &g))
        .collect()
}

fn extract_adhoc_prohibition(ctx: &mut Ctx, guid: &str) -> Option<AdhocProhibition> {
    let rec = ctx.get(guid)?;
    let disabled = rec.node.val_bool("Disabled").unwrap_or(false);
    let adjacency = match rec.node.val_int("Adjacency") {
        Some(0) => Adjacency::Anywhere,
        Some(1) => Adjacency::SomewhereToLeft,
        Some(2) => Adjacency::SomewhereToRight,
        Some(3) => Adjacency::AdjacentToLeft,
        Some(4) => Adjacency::AdjacentToRight,
        other => {
            ctx.warn(format!(
                "morphology.adhocProhibitions: {guid} has unexpected Adjacency {other:?}, \
                 defaulting to anywhere"
            ));
            Adjacency::Anywhere
        }
    };
    match rec.class.as_str() {
        "MoAlloAdhocProhib" => {
            let primary = rec.node.objsur_one("FirstAllomorph")?;
            let others = rec.node.objsur_list("RestOfAllos");
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
            Some(AdhocProhibition::Morpheme {
                guid: guid.to_string(),
                disabled,
                primary,
                others,
                adjacency,
            })
        }
        other => {
            ctx.warn(format!(
                "morphology.adhocProhibitions: {guid} has unexpected class {other}"
            ));
            None
        }
    }
}

/// Beyond the raw-guid dangling-reference check `Snapshot::validate()` already performs, cross
/// -check whether an *enabled* `Msa::Inflectional` ad-hoc "morpheme" prohibition can ever
/// actually be realized: does the MSA's slot appear in at least one non-disabled
/// `AffixTemplate`? An inflectional affix confined to a slot that only ever appears in disabled
/// templates can never be placed in any stratum — this is precisely the situation the FieldWorks
/// exporter's `WriteMorphemeCoOccurrenceRule` crashes on (a `KeyNotFoundException` when a rule
/// references a morpheme the writer never emitted; see `docs/fwdata-import-plan.md` §1 and the
/// `machine` submodule's `XmlLanguageWriter.cs`, whose already-applied fix comment describes
/// exactly this scenario: "an ad-hoc prohibition still targets the affix in a slot whose owning
/// inflectional-affix template was disabled"). `pg-fwdata` cannot avoid this the way the writer
/// does (silently drop the rule) without contradicting the "snapshot keeps the full authored
/// data" principle, so it surfaces it as an import warning instead.
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

    let find_msa = |guid: &str| -> Option<&Msa> {
        lexicon
            .entries
            .iter()
            .flat_map(|e| &e.msas)
            .find(|m| m.guid() == guid)
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
        for msa_guid in std::iter::once(primary).chain(others.iter()) {
            if let Some(Msa::Inflectional { slots, .. }) = find_msa(msa_guid) {
                if !slots.is_empty() && !slots.iter().any(|s| enabled_slots.contains(s.as_str())) {
                    ctx.warn(format!(
                        "morphology.adhocProhibitions: ad-hoc prohibition {guid} references \
                         inflectional affix {msa_guid}, whose slot(s) are not part of any \
                         enabled affix template (stale/unreachable rule)"
                    ));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Exception features ("production restriction" registry)
// ---------------------------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------------------------
// LexEntryInflType (irregular-inflection variant types)
// ---------------------------------------------------------------------------------------------

fn extract_lex_entry_infl_types(ctx: &mut Ctx, lex_db: &Record) -> Vec<LexEntryInflType> {
    let mut out = Vec::new();
    for field in ["VariantEntryTypes", "ComplexEntryTypes"] {
        if let Some(list_guid) = lex_db.node.objsur_one(field) {
            walk_possibility_list(
                ctx,
                &list_guid,
                "morphology.lexEntryInflTypes",
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
    let inflection_features = rec
        .node
        .objsur_one("InflFeats")
        .and_then(|g| extract_feature_structure(ctx, &g, "morphology.lexEntryInflTypes"))
        .filter(|fs| !fs.values.is_empty());
    Some(LexEntryInflType {
        guid: rec.guid.clone(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        abbreviation: ctx.best_analysis(&rec.node.ws_forms("Abbreviation")),
        gloss_prepend: ctx.best_analysis(&rec.node.ws_forms("GlossPrepend")),
        gloss_append: ctx.best_analysis(&rec.node.ws_forms("GlossAppend")),
        slots: rec.node.objsur_list("Slots"),
        inflection_features,
    })
}

// ---------------------------------------------------------------------------------------------
// Shared `CmPossibilityList` pre-order walk (used by exception features + LexEntryInflType)
// ---------------------------------------------------------------------------------------------

/// Pre-order walk of a `CmPossibilityList`'s `Possibilities`, recursing into each visited item's
/// own `SubPossibilities` — the generic shape `ReallyReallyAllPossibilities` flattens for
/// `MorphologicalData.ProdRestrict`/`PhPhonData.PhonRuleFeats`, and the shape the
/// `VariantEntryTypes`/`ComplexEntryTypes` hierarchies share. Calls `visit` for every resolved
/// item regardless of concrete class (callers filter); a dangling item guid is warned and simply
/// not descended into.
fn walk_possibility_list(
    ctx: &mut Ctx,
    list_guid: &str,
    label: &str,
    visit: &mut dyn FnMut(&mut Ctx, &Record),
) {
    let Some(list) = ctx.require(list_guid, "CmPossibilityList", label) else {
        return;
    };
    let item_guids = list.node.objsur_list("Possibilities");
    for item_guid in item_guids {
        walk_possibility_item(ctx, &item_guid, label, visit);
    }
}

fn walk_possibility_item(
    ctx: &mut Ctx,
    guid: &str,
    label: &str,
    visit: &mut dyn FnMut(&mut Ctx, &Record),
) {
    let Some(rec) = ctx.get(guid) else {
        ctx.warn(format!("{label}: dangling possibility-list item {guid}"));
        return;
    };
    visit(ctx, rec);
    let child_guids = rec.node.objsur_list("SubPossibilities");
    for child_guid in child_guids {
        walk_possibility_item(ctx, &child_guid, label, visit);
    }
}
