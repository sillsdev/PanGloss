//! `lexicon` snapshot section — see `docs/snapshot-format.md` §6.

use pg_snapshot::{
    AffixProcess, Allomorph, EntryRef, FeatureSystems, LexEntry, Lexicon, Morphology, Msa,
    RuleMapping, Sense,
};

use super::features::extract_feature_structure;
use super::phonology::{first_code_representation, resolve_phon_context};
use super::Ctx;
use crate::morphtype::{self, MorphTypeLookup};
use crate::xml::Record;

pub fn extract_lexicon(
    ctx: &mut Ctx,
    _feature_systems: &FeatureSystems,
    _morphology: &Morphology,
) -> Lexicon {
    // `LexEntry` is `owner="none"` in LCM (no ordered `Entries` sequence), so the parser tracks file-encounter order directly via `RawGraph::lex_entry_order`.
    let order = ctx.graph.lex_entry_order.clone();
    let entries = order.iter().filter_map(|g| extract_entry(ctx, g)).collect();
    Lexicon { entries }
}

fn extract_entry(ctx: &mut Ctx, guid: &str) -> Option<LexEntry> {
    let rec = ctx.get(guid)?;
    let citation_form = rec.node.ws_forms("CitationForm");
    let lexeme_form_guid = rec.node.objsur_one("LexemeForm");
    if lexeme_form_guid.is_none() {
        ctx.warn(
            super::codes::MISSING_REQUIRED_FIELD,
            format!("lexicon.entries: entry {guid} has no LexemeForm"),
        );
    }
    // Alternates first, lexeme form last (HCLoader.cs:263); this order is disjunctive-ordering-significant, not cosmetic.
    let mut allomorph_guids = rec.node.objsur_list("AlternateForms");
    allomorph_guids.extend(lexeme_form_guid);
    let allomorphs: Vec<Allomorph> = allomorph_guids
        .iter()
        .filter_map(|g| extract_allomorph(ctx, g))
        .collect();
    let lexeme_morph_type = match allomorphs.last() {
        Some(a) => a.morph_type,
        None => {
            ctx.warn(
                super::codes::NO_USABLE_ALLOMORPHS,
                format!(
                    "lexicon.entries: entry {guid} has no usable allomorphs; defaulting \
                     lexemeMorphType to stem"
                ),
            );
            pg_snapshot::MorphType::Stem
        }
    };
    let msas: Vec<Msa> = rec
        .node
        .objsur_list("MorphoSyntaxAnalyses")
        .iter()
        .filter_map(|g| extract_msa(ctx, g))
        .collect();
    // Flattens the whole sense tree (not just direct Senses), pre-order, mirroring C#'s `AllSenses` -- needed so `sense_gloss`'s per-MSA lookup can find a subsense's gloss for its own MSA.
    let senses: Vec<Sense> = extract_senses_recursive(ctx, &rec.node.objsur_list("Senses"));
    let entry_refs: Vec<EntryRef> = rec
        .node
        .objsur_list("EntryRefs")
        .iter()
        .filter_map(|g| extract_entry_ref(ctx, g))
        .collect();
    Some(LexEntry {
        guid: guid.to_string(),
        citation_form,
        lexeme_morph_type,
        allomorphs,
        msas,
        senses,
        entry_refs,
    })
}

fn resolve_morph_type(ctx: &mut Ctx, rec: &Record, label: &str) -> Option<pg_snapshot::MorphType> {
    let Some(mt_guid) = rec.node.objsur_one("MorphType") else {
        ctx.warn(
            super::codes::MISSING_REQUIRED_FIELD,
            format!("{label}: {} has no MorphType", rec.guid),
        );
        return None;
    };
    match morphtype::lookup(&mt_guid) {
        MorphTypeLookup::Known(mt) => Some(mt),
        MorphTypeLookup::UnsupportedWellKnown(name) => {
            ctx.warn(
                super::codes::UNSUPPORTED_MORPH_TYPE,
                format!(
                    "{label}: {} has morph type {name:?} ({mt_guid}), which this format's \
                     MorphType enum has no variant for (model gap — see morphtype module docs); \
                     skipping",
                    rec.guid
                ),
            );
            None
        }
        MorphTypeLookup::Unknown => {
            ctx.warn(
                super::codes::UNKNOWN_MORPH_TYPE_GUID,
                format!(
                    "{label}: {} has unrecognized morph-type guid {mt_guid}; skipping",
                    rec.guid
                ),
            );
            None
        }
    }
}

fn extract_allomorph(ctx: &mut Ctx, guid: &str) -> Option<Allomorph> {
    let label = "lexicon.entries.allomorphs";
    let rec = ctx.get(guid)?;
    if !matches!(
        rec.class.as_str(),
        "MoStemAllomorph" | "MoAffixAllomorph" | "MoAffixProcess"
    ) {
        ctx.warn(
            super::codes::UNEXPECTED_CLASS,
            format!("{label}: {guid} has unexpected class {}", rec.class),
        );
        return None;
    }
    let morph_type = resolve_morph_type(ctx, rec, label)?;
    let is_abstract = rec.node.val_bool("IsAbstract").unwrap_or(false);
    let forms = rec.node.ws_forms("Form");
    let environments = rec.node.objsur_list("PhoneEnv");
    let positions = if rec.class == "MoAffixAllomorph" {
        rec.node.objsur_list("Position")
    } else {
        Vec::new()
    };
    let stem_name = if rec.class == "MoStemAllomorph" {
        rec.node.objsur_one("StemName")
    } else {
        None
    };
    let inflection_classes = if rec.class != "MoStemAllomorph" {
        rec.node.objsur_list("InflectionClasses")
    } else {
        Vec::new()
    };
    let ms_env_features = if rec.class == "MoAffixAllomorph" {
        rec.node
            .objsur_one("MsEnvFeatures")
            .and_then(|g| extract_feature_structure(ctx, &g, label))
            .filter(|fs| !fs.values.is_empty())
    } else {
        None
    };
    let ms_env_part_of_speech = if rec.class == "MoAffixAllomorph" {
        rec.node.objsur_one("MsEnvPartOfSpeech")
    } else {
        None
    };
    let process = if rec.class == "MoAffixProcess" {
        Some(extract_affix_process(ctx, rec))
    } else {
        None
    };
    Some(Allomorph {
        guid: guid.to_string(),
        morph_type,
        is_abstract,
        forms,
        environments,
        positions,
        stem_name,
        inflection_classes,
        ms_env_features,
        ms_env_part_of_speech,
        process,
    })
}

fn extract_affix_process(ctx: &mut Ctx, rec: &Record) -> AffixProcess {
    let label = "lexicon.entries.allomorphs.process";
    let input_guids = rec.node.objsur_list("Input");
    let input = input_guids
        .iter()
        .filter_map(|g| resolve_phon_context(ctx, g, label))
        .collect();
    let output = rec
        .node
        .objsur_list("Output")
        .into_iter()
        .filter_map(|g| extract_rule_mapping(ctx, &g, &input_guids, label))
        .collect();
    AffixProcess { input, output }
}

/// `part` fields are 1-based positions into the owning `MoAffixProcess.InputOS` list, found here by position in `input_guids`.
fn extract_rule_mapping(
    ctx: &mut Ctx,
    guid: &str,
    input_guids: &[String],
    label: &str,
) -> Option<RuleMapping> {
    let rec = ctx.get(guid)?;
    let part_index = |ctx: &mut Ctx, content_guid: &str| -> Option<u32> {
        match input_guids.iter().position(|g| g == content_guid) {
            Some(i) => Some((i + 1) as u32),
            None => {
                ctx.warn(
                    super::codes::REFERENCE_NOT_IN_SCOPE,
                    format!(
                        "{label}: {guid} references {content_guid}, which is not a member of \
                         this affix process's Input list"
                    ),
                );
                None
            }
        }
    };
    match rec.class.as_str() {
        "MoInsertNC" => {
            let natural_class = rec.node.objsur_one("Content")?;
            Some(RuleMapping::InsertNaturalClass { natural_class })
        }
        "MoCopyFromInput" => {
            let content_guid = rec.node.objsur_one("Content")?;
            let part = part_index(ctx, &content_guid)?;
            Some(RuleMapping::CopyFromInput { part })
        }
        "MoInsertPhones" => {
            let mut text = String::new();
            for term_guid in rec.node.objsur_list("Content") {
                match first_code_representation(ctx, &term_guid) {
                    Some(s) => text.push_str(&s),
                    None => ctx.warn(
                        super::codes::EMPTY_REPRESENTATION,
                        format!(
                            "{label}: {guid} could not resolve a representation for terminal \
                             unit {term_guid}"
                        ),
                    ),
                }
            }
            Some(RuleMapping::InsertSegments { text })
        }
        "MoModifyFromInput" => {
            let content_guid = rec.node.objsur_one("Content")?;
            let part = part_index(ctx, &content_guid)?;
            let natural_class = rec.node.objsur_one("Modification")?;
            Some(RuleMapping::ModifyFromInput {
                part,
                natural_class,
            })
        }
        other => {
            ctx.warn(
                super::codes::UNEXPECTED_CLASS,
                format!("{label}: {guid} has unexpected class {other}"),
            );
            None
        }
    }
}

fn extract_msa(ctx: &mut Ctx, guid: &str) -> Option<Msa> {
    let label = "lexicon.entries.msas";
    let rec = ctx.get(guid)?;
    match rec.class.as_str() {
        "MoStemMsa" => {
            let features = rec
                .node
                .objsur_one("MsFeatures")
                .and_then(|g| extract_feature_structure(ctx, &g, label))
                .filter(|fs| !fs.values.is_empty());
            Some(Msa::Stem {
                guid: guid.to_string(),
                part_of_speech: rec.node.objsur_one("PartOfSpeech"),
                inflection_class: rec.node.objsur_one("InflectionClass"),
                features,
                exception_features: rec.node.objsur_list("ProdRestrict"),
                from_parts_of_speech: rec.node.objsur_list("FromPartsOfSpeech"),
                slots: rec.node.objsur_list("Slots"),
            })
        }
        "MoInflAffMsa" => {
            let features = rec
                .node
                .objsur_one("InflFeats")
                .and_then(|g| extract_feature_structure(ctx, &g, label))
                .filter(|fs| !fs.values.is_empty());
            Some(Msa::Inflectional {
                guid: guid.to_string(),
                part_of_speech: rec.node.objsur_one("PartOfSpeech"),
                slots: rec.node.objsur_list("Slots"),
                features,
                exception_features: rec.node.objsur_list("FromProdRestrict"),
            })
        }
        "MoDerivAffMsa" => {
            let from_features = rec
                .node
                .objsur_one("FromMsFeatures")
                .and_then(|g| extract_feature_structure(ctx, &g, label))
                .filter(|fs| !fs.values.is_empty());
            let to_features = rec
                .node
                .objsur_one("ToMsFeatures")
                .and_then(|g| extract_feature_structure(ctx, &g, label))
                .filter(|fs| !fs.values.is_empty());
            Some(Msa::Derivational {
                guid: guid.to_string(),
                from_part_of_speech: rec.node.objsur_one("FromPartOfSpeech"),
                to_part_of_speech: rec.node.objsur_one("ToPartOfSpeech"),
                from_features,
                to_features,
                from_inflection_class: rec.node.objsur_one("FromInflectionClass"),
                to_inflection_class: rec.node.objsur_one("ToInflectionClass"),
                from_exception_features: rec.node.objsur_list("FromProdRestrict"),
                to_exception_features: rec.node.objsur_list("ToProdRestrict"),
                from_stem_name: rec.node.objsur_one("FromStemName"),
            })
        }
        "MoUnclassifiedAffixMsa" => Some(Msa::Unclassified {
            guid: guid.to_string(),
            part_of_speech: rec.node.objsur_one("PartOfSpeech"),
        }),
        other => {
            ctx.warn(
                super::codes::UNEXPECTED_CLASS,
                format!("{label}: {guid} has unexpected class {other}"),
            );
            None
        }
    }
}

/// Flattens top-level sense guids and every transitively-owned subsense into one pre-order `Vec<Sense>`, mirroring HCLoader's recursive `AllSenses`.
fn extract_senses_recursive(ctx: &mut Ctx, guids: &[String]) -> Vec<Sense> {
    let mut out = Vec::new();
    for g in guids {
        let Some(rec) = ctx.get(g) else { continue };
        let sub_guids = rec.node.objsur_list("Senses");
        if let Some(sense) = extract_sense(ctx, g) {
            out.push(sense);
        }
        out.extend(extract_senses_recursive(ctx, &sub_guids));
    }
    out
}

fn extract_sense(ctx: &mut Ctx, guid: &str) -> Option<Sense> {
    let rec = ctx.get(guid)?;
    Some(Sense {
        guid: guid.to_string(),
        gloss: rec.node.ws_forms("Gloss"),
        definition: rec.node.ws_forms("Definition"),
        msa: rec.node.objsur_one("MorphoSyntaxAnalysis"),
    })
}

fn extract_entry_ref(ctx: &mut Ctx, guid: &str) -> Option<EntryRef> {
    let rec = ctx.require(guid, "LexEntryRef", "lexicon.entries.entryRefs")?;
    let component_lexemes = rec.node.objsur_list("ComponentLexemes");
    let variant_entry_types = rec.node.objsur_list("VariantEntryTypes");
    let complex_entry_types = rec.node.objsur_list("ComplexEntryTypes");
    // `variant` wins when both, or neither, type list is populated -- see docs/snapshot-format.md §6.
    // An empty-typed `Variant` is the more common shape for an otherwise-unclassified `LexEntryRef` in real data.
    if !complex_entry_types.is_empty() && variant_entry_types.is_empty() {
        Some(EntryRef::ComplexForm {
            guid: guid.to_string(),
            component_lexemes,
            complex_entry_types,
        })
    } else {
        Some(EntryRef::Variant {
            guid: guid.to_string(),
            component_lexemes,
            variant_entry_types,
        })
    }
}
