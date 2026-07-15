//! `hc_grammar::compile`: compile a `pg_snapshot::Snapshot` (a PanGloss-owned, FieldWorks-GUID
//! keyed project snapshot, produced by `pg-fwdata`) into a runnable [`crate::model::Grammar`] —
//! sibling to [`crate::load`] (which compiles the legacy HermitCrab XML export instead), reusing
//! its internal construction machinery (the [`crate::chardef`]/[`crate::featsys`]/[`crate::segment`]
//! modules, `hc_featstruct::Interner`/`FeatureStructBuilder`) rather than duplicating it.
//!
//! Semantically this is a Rust port of FieldWorks' `HCLoader.cs`
//! (`docs/fwdata-import-plan.md` §4) — the *front half* of the pipeline is new (LCM-shaped
//! `Snapshot` data, not XML), but the *back half* (patterns, feature structs, char-def tables,
//! the `Grammar` assembly order) is exactly what [`crate::load`] already builds, so this module
//! leans on the same [`crate::model`] types and the same `chardef`/`featsys`/`segment` helpers.
//!
//! ## Phasing (plan §4)
//! **Phase A** (implemented): feature systems, phonemes/char-def synthesis, stems, environments,
//! inflectional/derivational/unclassified affixes (concatenative and `MoAffixProcess`-style),
//! templates (+ null-affix synthesis for irregular slots), compounding (default + authored),
//! rewrite rules, ad-hoc co-occurrence rules, strata, variants, parser parameters.
//!
//! **Phase B** (not implemented — each occurrence produces a warning, never an error, mirroring
//! the existing loader's managed-fallback lint philosophy): metathesis rules, reduplication
//! (bracket-pattern affix forms), circumfix cross-products, clitic-as-affix-rule
//! (`LoadCliticAffixProcessRule`) and clitic-as-stem stratum placement, user-defined `<Strata>`
//! reorganization strings.
//!
//! Never panics on real data: dangling snapshot references, malformed environment strings, and
//! unsupported constructs all become warnings (an allomorph/entry/rule is dropped, not the whole
//! grammar), except where the *language itself* is unrepresentable (e.g. >64 parts of speech),
//! which mirrors [`crate::load`]'s own [`GrammarError::Unsupported`] hard-stop convention.

mod affixes;
mod chardef;
mod compounding;
mod environment;
mod features;
mod lexicon;
mod mpr;
mod natclass;
mod rules;
mod templates;
#[cfg(test)]
mod tests;

use hashbrown::HashMap;

use hc_featstruct::{FeatureStruct, Interner};

use crate::chardef::{CharDefId, CharDefTable};
use crate::featsys::PhonFeatureSystem;
use crate::model::*;
use crate::GrammarError;

use pg_snapshot::Snapshot;

/// Compile a `pg-snapshot` [`Snapshot`] into a runnable [`Grammar`], returning any non-fatal
/// warnings alongside it (dangling references, unsupported Phase-B constructs, dropped
/// allomorphs/entries — see the module doc). Only a handful of hard limits (plan-inherited from
/// [`crate::load`]: >64 symbols in a feature, >64 total MPR features) surface as `Err`.
pub fn compile_project(snapshot: &Snapshot) -> Result<(Grammar, Vec<String>), GrammarError> {
    let mut warnings: Vec<String> = Vec::new();

    // --- MPR feature groups: inflection classes, exception features, lexEntryInflTypes --------
    let mpr = mpr::build(snapshot, &mut warnings)?;

    // --- POS + syntactic feature system (POS = feature 0; head = feature 1, always present) ---
    let (syn, pos) = features::build_syn_features(snapshot)?;

    // --- phonological feature system -----------------------------------------------------------
    let phon_features = features::build_phon_features(snapshot, &mut warnings)?;

    // --- character-definition table from phonemes ----------------------------------------------
    let chardef::CharDefBuild {
        table: char_table,
        phoneme_of,
        boundary_of,
        null_bdry,
        morph_bdry,
    } = chardef::build(snapshot, &phon_features, &mut warnings)?;
    let table_id = TableId(0);

    // --- natural classes (+ synthetic "Any") ----------------------------------------------------
    let natclass::NatClassBuild {
        defs: natural_classes,
        by_guid: natclass_by_guid,
        by_name: natclass_by_name,
        any: any_nc,
    } = natclass::build(snapshot, &phon_features, &phoneme_of, &mut warnings);

    // --- grammar-tier FS interner: the empty FS is interned first (FsId 0) ---------------------
    let mut fs_interner: Interner<FeatureStruct> = Interner::with_capacity(64);
    let empty = fs_interner.intern(FeatureStruct::EMPTY);
    debug_assert_eq!(empty, hc_featstruct::FsId(0));

    // --- stem names ------------------------------------------------------------------------------
    let (stem_names, stem_name_by_guid) =
        features::build_stem_names(snapshot, &syn, &pos, &mut fs_interner, &mut warnings);

    let mut env_by_guid = HashMap::new();
    for e in &snapshot.phonology.environments {
        env_by_guid.insert(e.guid.as_str(), e);
    }

    let ctx = Ctx {
        phon: &phon_features,
        table: &char_table,
        table_id,
        natclass_by_guid: &natclass_by_guid,
        natclass_by_name: &natclass_by_name,
        natural_class_defs: &natural_classes,
        any_nc,
        null_bdry,
        morph_bdry,
        phoneme_of: &phoneme_of,
        boundary_of: &boundary_of,
        syn: &syn,
        pos: &pos,
        stem_name_by_guid: &stem_name_by_guid,
        mpr: &mpr,
        env_by_guid: &env_by_guid,
        default_vernacular_ws: snapshot.project.vernacular_writing_systems.first().cloned(),
        default_analysis_ws: snapshot.project.analysis_writing_systems.first().cloned(),
    };

    let mut acc = Acc {
        fs_interner,
        mrules: Vec::new(),
        morphemes: Vec::new(),
        allomorph_owners: Vec::new(),
        templates: Vec::new(),
        entries: Vec::new(),
        allomorph_guid_index: HashMap::new(),
        msa_guid_index: HashMap::new(),
        slot_rules: HashMap::new(),
    };

    // --- strata: Morphology (unordered), Clitics (unordered), Surface (linear) -----------------
    // HCLoader.cs:227-233. Custom `<Strata>` reorganization (plan §4 Phase B) is not implemented;
    // a snapshot that declares one gets a warning and the default 3-stratum layout regardless.
    // A present-but-EMPTY `<Strata />` element (Amharic authors one) parses to zero stratum rule
    // lists in HCLoader too (`m_strata.Count > 0` gates `CreateStrata`, HCLoader.cs:353-356) —
    // that is the default layout, not a custom reorganization, so no warning for it.
    if snapshot
        .morphology
        .parser_parameters
        .strata
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        warnings.push(
            "unsupported: custom Strata parser-parameter reorganization not implemented; using \
             the default Morphology/Clitics/Surface layout"
                .to_string(),
        );
    }
    let morphology_stratum = StratumId(0);
    let clitic_stratum = StratumId(1);
    let surface_stratum = StratumId(2);

    // --- compound rules (defaults if none authored) ----------------------------------------------
    let mut morphology_mrules: Vec<MRuleId> = Vec::new();
    compounding::build(snapshot, &ctx, &mut acc, &mut morphology_mrules, &mut warnings)?;

    // --- lexicon: stems + variants + affix rules -------------------------------------------------
    let mut clitic_mrules: Vec<MRuleId> = Vec::new();
    let mut morphology_entries: Vec<LexEntryId> = Vec::new();
    let mut clitic_entries: Vec<LexEntryId> = Vec::new();
    lexicon::build(
        snapshot,
        &ctx,
        &mut acc,
        &mut morphology_mrules,
        &mut clitic_mrules,
        &mut morphology_entries,
        &mut clitic_entries,
        &mut warnings,
    )?;

    // --- affix templates (+ null-affix synthesis for irregular-form slots) ---------------------
    let morphology_templates =
        templates::build(snapshot, &ctx, &mut acc, &mut warnings)?;

    // --- phonological rules ----------------------------------------------------------------------
    let (prules, morphology_prules, clitic_prules) = rules::build(snapshot, &ctx, &mut warnings)?;

    // --- ad-hoc co-occurrence rules ---------------------------------------------------------------
    // Post-hoc, mirroring HCLoader's own placement (after every entry/rule is loaded, so the
    // guid -> registry maps `acc.allomorph_guid_index`/`acc.msa_guid_index` are fully populated).
    // `xml_key` doubles as the MSA/entry guid this morpheme was built from (see lexicon.rs /
    // affixes.rs, which set it to exactly that guid).
    for (i, m) in acc.morphemes.iter().enumerate() {
        acc.msa_guid_index.insert(m.xml_key.clone(), MorphemeId(i as u32));
    }
    strata_assign_co_occurrence(snapshot, &mut acc, &mut warnings);

    let strata = vec![
        StratumDef {
            name: Some("Morphology".to_string()),
            table: table_id,
            mrule_order: MorphRuleOrder::Unordered,
            prules: morphology_prules,
            mrules: morphology_mrules,
            templates: morphology_templates,
            entries: morphology_entries,
        },
        StratumDef {
            name: Some("Clitics".to_string()),
            table: table_id,
            mrule_order: MorphRuleOrder::Unordered,
            prules: clitic_prules,
            mrules: clitic_mrules,
            templates: Vec::new(),
            entries: clitic_entries,
        },
        StratumDef {
            name: Some("Surface".to_string()),
            table: table_id,
            mrule_order: MorphRuleOrder::Linear,
            prules: Vec::new(),
            mrules: Vec::new(),
            templates: Vec::new(),
            entries: Vec::new(),
        },
    ];
    let _ = (clitic_stratum, surface_stratum, morphology_stratum);

    // `IsTemplateRule` post-pass, exactly mirroring `crate::load::load`'s own post-pass.
    let mut is_template_rule = vec![false; acc.mrules.len()];
    for t in &acc.templates {
        for slot in &t.slots {
            for &mid in &slot.rules {
                is_template_rule[mid.0 as usize] = true;
            }
        }
    }
    for (mid, flag) in is_template_rule.into_iter().enumerate() {
        if let MorphRuleDef::AffixProcess(def) = &mut acc.mrules[mid] {
            def.is_template_rule = flag;
        }
    }

    let grammar = Grammar {
        name: Some(snapshot.project.name.clone()),
        phon_features,
        char_tables: vec![char_table],
        syn_features: syn,
        fs_interner: acc.fs_interner,
        mpr_names: mpr.mpr_names,
        mpr_groups: mpr.mpr_groups,
        stem_names,
        families: Vec::new(),
        natural_classes,
        morphemes: acc.morphemes,
        allomorph_owners: acc.allomorph_owners,
        prules,
        mrules: acc.mrules,
        templates: acc.templates,
        entries: acc.entries,
        strata,
    };

    Ok((grammar, warnings))
}

/// Ad-hoc co-occurrence rules resolved against the now-complete `acc.allomorph_guid_index` /
/// `acc.msa_guid_index` registries — mirrors `crate::load::load`'s post-strata pass, but simpler:
/// the snapshot references allomorphs/MSAs by GUID directly (no XML-id indirection to resolve
/// first). See `docs/fwdata-import-plan.md` §1: a dangling reference here is exactly the "stale
/// `MoMorphAdhocProhib`" tolerance case — a warning, never a hard failure.
fn strata_assign_co_occurrence(snapshot: &Snapshot, acc: &mut Acc, warnings: &mut Vec<String>) {
    use pg_snapshot::morphology::AdhocProhibition;
    use pg_snapshot::morphology::Adjacency as SnapAdjacency;

    fn adjacency(a: SnapAdjacency) -> CoOccurrenceAdjacency {
        match a {
            SnapAdjacency::Anywhere => CoOccurrenceAdjacency::Anywhere,
            SnapAdjacency::SomewhereToLeft => CoOccurrenceAdjacency::SomewhereToLeft,
            SnapAdjacency::SomewhereToRight => CoOccurrenceAdjacency::SomewhereToRight,
            SnapAdjacency::AdjacentToLeft => CoOccurrenceAdjacency::AdjacentToLeft,
            SnapAdjacency::AdjacentToRight => CoOccurrenceAdjacency::AdjacentToRight,
        }
    }

    for rule in &snapshot.morphology.adhoc_prohibitions {
        match rule {
            AdhocProhibition::Allomorph {
                disabled,
                primary,
                others,
                adjacency: adj,
                ..
            } => {
                if *disabled {
                    continue;
                }
                let Some(&primary_id) = acc.allomorph_guid_index.get(primary) else {
                    warnings.push(format!(
                        "ad-hoc allomorph prohibition: primary allomorph {primary:?} does not \
                         resolve; skipped"
                    ));
                    continue;
                };
                let mut other_ids = Vec::with_capacity(others.len());
                let mut ok = true;
                for o in others {
                    match acc.allomorph_guid_index.get(o) {
                        Some(&id) => other_ids.push(id),
                        None => {
                            warnings.push(format!(
                                "ad-hoc allomorph prohibition: allomorph {o:?} does not resolve; \
                                 rule skipped"
                            ));
                            ok = false;
                        }
                    }
                }
                if !ok || other_ids.is_empty() {
                    continue;
                }
                let def = AllomorphCoOccurrenceRuleDef {
                    require: false,
                    others: other_ids,
                    adjacency: adjacency(*adj),
                };
                match acc.allomorph_owners[primary_id.0 as usize] {
                    AllomorphOwner::Root(le, idx) => {
                        acc.entries[le.0 as usize].allomorphs[idx as usize]
                            .co_occurrence
                            .push(def);
                    }
                    AllomorphOwner::Affix(mr, idx) => match &mut acc.mrules[mr.0 as usize] {
                        MorphRuleDef::AffixProcess(d) => d.allomorphs[idx as usize].co_occurrence.push(def),
                        MorphRuleDef::Realizational(d) => d.allomorphs[idx as usize].co_occurrence.push(def),
                        MorphRuleDef::Compounding(_) => {}
                    },
                }
            }
            AdhocProhibition::Morpheme {
                disabled,
                primary,
                others,
                adjacency: adj,
                ..
            } => {
                if *disabled {
                    continue;
                }
                let Some(&primary_id) = acc.msa_guid_index.get(primary) else {
                    warnings.push(format!(
                        "ad-hoc morpheme prohibition: primary morpheme {primary:?} does not \
                         resolve; skipped"
                    ));
                    continue;
                };
                let mut other_ids = Vec::with_capacity(others.len());
                let mut ok = true;
                for o in others {
                    match acc.msa_guid_index.get(o) {
                        Some(&id) => other_ids.push(id),
                        None => {
                            warnings.push(format!(
                                "ad-hoc morpheme prohibition: morpheme {o:?} does not resolve; \
                                 rule skipped"
                            ));
                            ok = false;
                        }
                    }
                }
                if !ok || other_ids.is_empty() {
                    continue;
                }
                acc.morphemes[primary_id.0 as usize]
                    .co_occurrence
                    .push(MorphemeCoOccurrenceRuleDef {
                        require: false,
                        others: other_ids,
                        adjacency: adjacency(*adj),
                    });
            }
        }
    }
}

// =================================================================================================
// Shared read-only context + mutable accumulator (mirrors `crate::load`'s `Ro`/`Acc` split).
// =================================================================================================

/// Read-only tables built once, up front, and shared by every later compilation phase.
pub(crate) struct Ctx<'a> {
    pub phon: &'a PhonFeatureSystem,
    pub table: &'a CharDefTable,
    pub table_id: TableId,
    pub natclass_by_guid: &'a HashMap<String, NatClassId>,
    pub natclass_by_name: &'a HashMap<String, NatClassId>,
    pub natural_class_defs: &'a [NaturalClass],
    pub any_nc: NatClassId,
    pub null_bdry: CharDefId,
    pub morph_bdry: CharDefId,
    pub phoneme_of: &'a HashMap<String, CharDefId>,
    pub boundary_of: &'a HashMap<String, CharDefId>,
    pub syn: &'a SynFeatureSystem,
    pub pos: &'a features::PosTable,
    pub stem_name_by_guid: &'a HashMap<String, StemNameId>,
    pub mpr: &'a mpr::MprTables,
    /// Every declared environment, by guid — resolved lazily wherever an allomorph/MSA
    /// references one (`docs/fwdata-import-plan.md`'s environment-string tokenization).
    pub env_by_guid: &'a HashMap<&'a str, &'a pg_snapshot::phonology::Environment>,
    pub default_vernacular_ws: Option<String>,
    pub default_analysis_ws: Option<String>,
}

/// Everything the compiler appends to as it walks the snapshot's strata-worth of content.
pub(crate) struct Acc {
    pub fs_interner: Interner<FeatureStruct>,
    pub mrules: Vec<MorphRuleDef>,
    pub morphemes: Vec<MorphemeInfo>,
    pub allomorph_owners: Vec<AllomorphOwner>,
    pub templates: Vec<AffixTemplateDef>,
    pub entries: Vec<LexEntryDef>,
    /// Allomorph guid -> registry id, for ad-hoc allomorph-prohibition resolution.
    pub allomorph_guid_index: HashMap<String, AllomorphId>,
    /// MSA/entry guid -> morpheme registry id, for ad-hoc morpheme-prohibition resolution.
    /// Rebuilt from `morphemes[*].xml_key` right before the ad-hoc pass runs (see
    /// `compile_project`); unpopulated (and unused) before then.
    pub msa_guid_index: HashMap<String, MorphemeId>,
    /// Affix-template slot guid -> every loaded inflectional-affix rule whose MSA declared that
    /// slot (`MoInflAffMsa.SlotsRC`), in the order those rules were built. Populated by
    /// `affixes::build_affix_rule` while building each `Msa::Inflectional` rule; consumed by
    /// `templates::build` once every entry/MSA has been processed (mirrors HCLoader's own
    /// `slot.Affixes` reverse-reference walk, HCLoader.cs:1704).
    pub slot_rules: HashMap<String, Vec<MRuleId>>,
}

/// The best available string for a `WsForm` list: prefer `preferred_ws`, else the first entry.
/// Mirrors HCLoader's `BestAnalysisAlternative`/`VernacularDefaultWritingSystem`/
/// `BestVernacularAlternative` fallback conventions, which this snapshot format flattens to "the
/// writing system the project declared as default, else whatever's there" (`docs/snapshot-format.md`).
pub(crate) fn best_ws<'a>(forms: &'a [pg_snapshot::WsForm], preferred_ws: Option<&str>) -> Option<&'a str> {
    if let Some(ws) = preferred_ws {
        if let Some(f) = forms.iter().find(|f| f.ws == ws) {
            return Some(&f.form);
        }
    }
    forms.first().map(|f| f.form.as_str())
}

/// Every representation tagged with `preferred_ws` (there may be several — multiple `PhCode`s in
/// the same writing system, e.g. Sena's `m`/`n` phoneme). Falls back to every representation if
/// none matches (tolerant — see [`Ctx::default_vernacular_ws`]'s doc).
pub(crate) fn ws_forms<'a>(forms: &'a [pg_snapshot::WsForm], preferred_ws: Option<&str>) -> Vec<&'a str> {
    if let Some(ws) = preferred_ws {
        let matched: Vec<&str> = forms
            .iter()
            .filter(|f| f.ws == ws)
            .map(|f| f.form.as_str())
            .collect();
        if !matched.is_empty() {
            return matched;
        }
    }
    forms.iter().map(|f| f.form.as_str()).collect()
}

/// `HCLoader.FormatForm` (HCLoader.cs:2573-2576): trim, then replace every space with `.`.
pub(crate) fn format_form(s: &str) -> String {
    s.trim().replace(' ', ".")
}
