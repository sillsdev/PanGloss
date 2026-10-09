//! `pg_grammar::compile`: compile a `pg_snapshot::Snapshot` (a PanGloss-owned, FieldWorks-GUID
//! keyed project snapshot, produced by `pg-fwdata`) into a runnable `crate::model::Grammar` —
//! sibling to `mod@crate::load` (which compiles the legacy HermitCrab XML export instead), reusing
//! its internal construction machinery (the `crate::chardef`/`crate::featsys`/`crate::segment`
//! modules, `pg_featstruct::Interner`/`FeatureStructBuilder`) rather than duplicating it.
//!
//! Semantically this is a Rust port of FieldWorks' `HCLoader.cs` — the *front half* of the pipeline
//! is new (LCM-shaped `Snapshot` data, not XML), but the *back half* (patterns, feature structs,
//! char-def tables, the `Grammar` assembly order) is exactly what `mod@crate::load` already builds,
//! so this module leans on the same `crate::model` types and the same
//! `chardef`/`featsys`/`segment` helpers.
//!
//! ## Coverage
//! **Implemented**: feature systems, phonemes/char-def synthesis, stems, environments,
//! inflectional/derivational/unclassified affixes (concatenative and `MoAffixProcess`-style),
//! templates (+ null-affix synthesis for irregular slots), compounding (default + authored),
//! rewrite rules, ad-hoc co-occurrence rules, strata, variants, parser parameters.
//!
//! **Not implemented** (each occurrence produces a warning, never an error, mirroring the existing
//! loader's managed-fallback lint philosophy): metathesis rules, reduplication
//! (bracket-pattern affix forms), circumfix cross-products, clitic-as-affix-rule
//! (`LoadCliticAffixProcessRule`) and clitic-as-stem stratum placement, user-defined `<Strata>`
//! reorganization strings.
//!
//! Never panics on real data: dangling snapshot references, malformed environment strings, and
//! unsupported constructs all become warnings (an allomorph/entry/rule is dropped, not the whole
//! grammar), except where the *language itself* is unrepresentable (e.g. >64 parts of speech),
//! which mirrors `mod@crate::load`'s own `GrammarError::Unsupported` hard-stop convention.

mod affixes;
mod chardef;
mod compounding;
mod environment;
mod features;
mod gates;
pub(crate) mod inventory;
pub(crate) mod issue_codes;
pub mod issues;
mod lexicon;
mod lineage;
mod mpr;
mod natclass;
pub mod options;
mod reachability;
pub(crate) mod roles;
mod rules;
mod substrate;
mod templates;
#[cfg(feature = "test-support")]
pub mod test_support;
#[cfg(test)]
mod tests;
mod warnings;

use std::cell::RefCell;
use std::collections::BTreeMap;

use hashbrown::HashMap;

use pg_featstruct::{FeatureStruct, Interner};

use crate::chardef::{CharDefId, CharDefTable};
use crate::featsys::PhonFeatureSystem;
use crate::model::*;
use crate::GrammarError;

use pg_snapshot::{
    ConversionIssue, ImportWarningCode, InventoryDelta, InventoryKey, IssueClass,
    SelectionRecorder, Snapshot, SourceInventoryStatus, SourceRef,
};

pub use environment::{
    environment_side_elements, EnvironmentResolution, EnvironmentResolutionStatus,
    EnvironmentSideElement,
};
use gates::GateOutcomes;
pub use gates::{AllomorphGateEffect, AllomorphGateKind, AllomorphGateOutcome};
use inventory::{Lineage, LineageTarget};
pub use issues::{CompileOutput, CompiledAllomorphOrder, CompiledMapping};
use issues::{ConversionError, SubstrateReport};
pub use lineage::{
    compiled_outputs, AllomorphConditioning, CompiledOutput, CompiledOutputKind, CompiledOutputs,
};
pub use options::{CompileOptions, SemanticLossPolicy};

/// Compile a `pg-snapshot` `Snapshot` into a runnable `Grammar`, returning any non-fatal
/// warnings alongside it (dangling references, unsupported Phase-B constructs, dropped
/// allomorphs/entries — see the module doc). A handful of hard limits inherited from
/// `mod@crate::load` (>64 symbols in a feature, >64 total MPR features) surface as `Err`, and so
/// does [`SemanticLossPolicy::Refuse`]'s own fatal-conversion-issue check — see
/// [`compile_project_with`], which this is a thin, source-compatible wrapper over.
pub fn compile_project(
    snapshot: &Snapshot,
) -> Result<(Grammar, Vec<pg_snapshot::Warning>), GrammarError> {
    let out = compile_project_with(snapshot, CompileOptions::default())?;
    Ok((out.grammar, out.warnings))
}

/// As [`compile_project`], but also returns the conversion-loss measurement derived from the same
/// recorder -- a violated recorder invariant is a bug in this compiler's own bookkeeping, so it
/// panics naming the violation rather than returning a measurement that cannot be trusted.
///
/// Discards the provisional-definition report and uses
/// [`SemanticLossPolicy::MeasureOnly`] so fatal conversion issues remain visible in the measured
/// result without refusing the compilation.
pub fn compile_project_measured(
    snapshot: &Snapshot,
) -> Result<
    (
        Grammar,
        Vec<pg_snapshot::Warning>,
        pg_snapshot::InventoryDelta,
    ),
    GrammarError,
> {
    let out = compile_project_with(
        snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
        },
    )?;
    Ok((out.grammar, out.warnings, out.inventory))
}

/// Compiles under an explicit [`CompileOptions`], returning every conversion issue (import-stage,
/// every owner's own recorder issue, and this compile's substrate issues) alongside the `Grammar`.
/// Provisional definitions apply to every project; every warning producer records
/// its own coded issue at the point where it decides to warn.
///
/// The issue collector starts from `snapshot.conversion_provenance` (import-stage issues, plus a
/// synthesized fatal `issues::SOURCE_PROVENANCE_UNKNOWN` issue when the source provenance itself
/// is unresolved) before any compiler owner runs, so a graph-to-snapshot failure is never dropped
/// just because this compile stage found nothing wrong of its own. Under
/// [`SemanticLossPolicy::Refuse`], any fatal issue in that combined collection refuses the compile
/// with [`GrammarError::Conversion`]; under [`SemanticLossPolicy::MeasureOnly`] every issue is
/// still returned, but never refused -- reserved for the structural inventory gate, never a
/// production entry point.
pub fn compile_project_with(
    snapshot: &Snapshot,
    options: CompileOptions,
) -> Result<CompileOutput, GrammarError> {
    compile_project_with_options_and_import_warnings(snapshot, options, std::iter::empty())
}

/// Compile a FieldWorks snapshot and merge importer warnings into the compiler's warning set.
/// Importer warnings come first so their linguist-facing wording wins when the compiler reports
/// the same code and source subjects.
pub fn compile_project_with_import_warnings(
    snapshot: &Snapshot,
    import_warnings: impl IntoIterator<Item = pg_snapshot::Warning>,
) -> Result<(Grammar, Vec<pg_snapshot::Warning>), GrammarError> {
    let output = compile_project_with_options_and_import_warnings(
        snapshot,
        CompileOptions::default(),
        import_warnings,
    )?;
    Ok((output.grammar, output.warnings))
}

/// Compile with explicit options and importer warnings, preserving the full compiler report.
/// Importer wording wins when the compiler reports the same code and source subjects.
pub fn compile_project_with_options_and_import_warnings(
    snapshot: &Snapshot,
    options: CompileOptions,
    import_warnings: impl IntoIterator<Item = pg_snapshot::Warning>,
) -> Result<CompileOutput, GrammarError> {
    let mut external_issues: Vec<ConversionIssue> =
        snapshot.conversion_provenance.import_issues.clone();
    let provenance_error = snapshot
        .conversion_provenance
        .validate()
        .err()
        .map(|error| format!("invalid conversion provenance: {error:?}"));
    if provenance_error.is_some()
        || snapshot.conversion_provenance.source_inventory_status == SourceInventoryStatus::Unknown
    {
        external_issues.push(ConversionIssue {
            code: issues::SOURCE_PROVENANCE_UNKNOWN,
            class: IssueClass::AmbiguousSource,
            source: None,
            fatal: true,
            message: provenance_error.unwrap_or_else(|| "conversion provenance is unknown; cannot certify this conversion's completeness".to_string()),
        });
    }
    let mut issues = external_issues.clone();

    let (
        grammar,
        recorder,
        substrate,
        substrate_issues,
        owner_warnings,
        environment_resolutions,
        allomorph_gates,
    ) = compile_project_recording(snapshot)?;
    if let Err(violation) = recorder.check_invariants() {
        panic!("compile_project_with: selection recorder invariant violated: {violation}");
    }
    let (recorded_inventory, recorded_issues, load_decisions) =
        recorder.finish_with_load_decisions();
    let compiled_outputs = lineage::compiled_outputs(&grammar);
    let allomorph_output_ids = (0..grammar.allomorph_owners.len())
        .map(|index| compiled_outputs.allomorph(AllomorphId(index as u32)))
        .collect();
    let (compiled_mappings, compiled_allomorph_order) =
        lineage::publish(snapshot, &grammar, &compiled_outputs, &load_decisions);
    let compiled_outputs = compiled_outputs.outputs().to_vec();
    issues.extend(recorded_issues.iter().cloned());
    issues.extend(substrate_issues.iter().cloned());
    let mut warnings: Vec<_> = import_warnings.into_iter().collect();
    warnings.extend(warnings::from_import_issues(
        snapshot,
        &external_issues,
        &warnings,
    ));
    warnings.extend(warnings::from_import_issues(
        snapshot,
        &recorded_issues,
        &owner_warnings,
    ));
    warnings.extend(owner_warnings);
    warnings.extend(warnings::from_issues(snapshot, &substrate_issues));
    warnings::add_open_targets(snapshot, &mut warnings);
    let warnings = warnings::deduplicate(warnings);
    let inventory = InventoryDelta::from_stage(recorded_inventory, recorded_issues);

    let refuses = options.semantic_loss == SemanticLossPolicy::Refuse
        && issues.iter().any(|issue| issue.fatal);
    if refuses {
        return Err(ConversionError {
            issues,
            warnings,
            substrate,
            inventory,
            load_decisions,
            environment_resolutions,
            compiled_outputs,
            compiled_mappings,
            compiled_allomorph_order,
            allomorph_gates,
        }
        .into());
    }

    Ok(CompileOutput {
        grammar,
        issues,
        warnings,
        substrate,
        inventory,
        load_decisions,
        environment_resolutions,
        compiled_outputs,
        allomorph_output_ids,
        compiled_mappings,
        compiled_allomorph_order,
        allomorph_gates,
    })
}

/// What [`compile_project_recording`] yields: the grammar, recording seams, and owner-published warning subjects.
pub(crate) type CompiledProject = (
    Grammar,
    SelectionRecorder,
    SubstrateReport,
    Vec<ConversionIssue>,
    Vec<pg_snapshot::Warning>,
    Vec<EnvironmentResolution>,
    Vec<AllomorphGateOutcome>,
);

/// As [`compile_project`], but also returns the [`SelectionRecorder`], [`SubstrateReport`], and
/// substrate-only issues and owner warnings -- the seams the measured API and [`compile_project_with`]
/// read. Recording happens before
/// `reachability::compact_mrules`/`trim_unreachable_morpheme_coocurrence`/`natclass::compact_to_referenced`
/// run, so `represented` is a pre-compaction claim, not a claim about the returned `Grammar` after
/// compaction.
pub(crate) fn compile_project_recording(
    snapshot: &Snapshot,
) -> Result<CompiledProject, GrammarError> {
    let mut recorder = SelectionRecorder::for_stage(pg_snapshot::LoadPipelineStage::Compile);
    let mut lineage = Lineage::default();
    inventory::seed_authored_from_snapshot(&mut recorder, snapshot);

    // --- MPR feature groups: inflection classes, exception features, lexEntryInflTypes --------
    let mpr = mpr::build(snapshot, &mut recorder)?;

    // --- POS + syntactic feature system (POS = feature 0; head = feature 1, always present) ---
    let (syn, pos) = features::build_syn_features(snapshot, &mut recorder)?;

    // --- phonological feature system -----------------------------------------------------------
    let phon_features = features::build_phon_features(snapshot, &mut recorder)?;

    // --- text usage: owners publish literal text they already selected, for substrate inference -
    let mut substrate_issues: Vec<ConversionIssue> = Vec::new();
    lexicon::collect_text_uses(snapshot, &mut recorder);
    affixes::collect_text_uses(snapshot, &mut recorder, &mut substrate_issues);

    // Authored definitions take precedence over provisional definitions.
    let raw = chardef::build_raw(snapshot, &phon_features, &mut recorder)?;
    let authored_boundary_reps: hashbrown::HashSet<String> = snapshot
        .phonology
        .boundary_markers
        .iter()
        .flat_map(|b| {
            chardef::boundary_representations(
                b,
                snapshot
                    .project
                    .vernacular_writing_systems
                    .first()
                    .map(String::as_str),
            )
        })
        .map(|representation| crate::nfd::nfd(&representation))
        .collect();
    let substrate::SubstrateCompletion {
        raw: completed_raw,
        report: substrate_report,
        issues: complete_issues,
    } = substrate::complete(
        recorder.text_uses(),
        &snapshot.project.exemplar_characters,
        &authored_boundary_reps,
        raw,
        &phon_features,
    );
    substrate_issues.extend(complete_issues);
    let chardef::CharDefBuild {
        table: mut char_table,
        phoneme_of,
        boundary_of,
        null_bdry,
        morph_bdry,
    } = chardef::finalize(snapshot, completed_raw, &phon_features, &mut recorder)?;
    let table_id = TableId(0);
    for definition in substrate_report
        .inferred_segments
        .iter()
        .chain(&substrate_report.inferred_boundaries)
    {
        let id = char_table
            .lookup_nfd(&crate::nfd::nfd(&definition.representation))
            .expect("provisional definition was finalized");
        char_table.mark_provisional(id);
    }

    // --- natural classes (+ synthetic "Any") ----------------------------------------------------
    let natclass::NatClassBuild {
        defs: natural_classes,
        by_guid: natclass_by_guid,
        by_name: natclass_by_name,
        any: any_nc,
        last_unnamed: natclass_last_unnamed,
    } = natclass::build(
        snapshot,
        &phon_features,
        &phoneme_of,
        &mut recorder,
        &mut lineage,
    );

    // --- grammar-tier FS interner: the empty FS is interned first (FsId 0) ---------------------
    let mut fs_interner: Interner<FeatureStruct> = Interner::with_capacity(64);
    let empty = fs_interner.intern(FeatureStruct::EMPTY);
    debug_assert_eq!(empty, pg_featstruct::FsId(0));

    // --- stem names ------------------------------------------------------------------------------
    let (stem_names, stem_name_by_guid) =
        features::build_stem_names(snapshot, &syn, &pos, &mut fs_interner, &mut recorder);

    let mut env_by_guid = HashMap::new();
    for e in &snapshot.phonology.environments {
        env_by_guid.insert(e.guid.as_str(), e);
    }

    let ctx = Ctx {
        snapshot,
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
        recorder: RefCell::new(recorder),
        environment_resolutions: RefCell::new(BTreeMap::new()),
        owner_warnings: RefCell::new(Vec::new()),
        lineage: RefCell::new(lineage),
        pending_rule_refusals: RefCell::new(Vec::new()),
        allomorph_gates: RefCell::new(GateOutcomes::default()),
    };

    let mut acc = Acc {
        fs_interner,
        mrules: Vec::new(),
        morphemes: Vec::new(),
        allomorph_owners: Vec::new(),
        allomorph_sources: Vec::new(),
        templates: Vec::new(),
        entries: Vec::new(),
        allomorph_guid_index: HashMap::new(),
        msa_guid_index: HashMap::new(),
        slot_rules: HashMap::new(),
    };

    // Strata: Morphology (unordered), Clitics (unordered), Surface (linear). Custom `<Strata>` reorganization is not implemented; a snapshot that declares one gets a warning and the default 3-stratum layout regardless.
    if snapshot
        .morphology
        .parser_parameters
        .strata
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        let key = InventoryKey::setting(pg_snapshot::InventoryKind::StrataConfiguration, "Strata");
        ctx.considered(key.clone());
        ctx.selected(key.clone());
        ctx.reject(
            key,
            issue_codes::STRATA_CUSTOM_UNSUPPORTED,
            IssueClass::UnrepresentableForHc,
            "unsupported: custom Strata parser-parameter reorganization not implemented; using \
             the default Morphology/Clitics/Surface layout",
        );
    }
    {
        let not_on_clitics_key =
            InventoryKey::setting(pg_snapshot::InventoryKind::ParserSetting, "notOnClitics");
        ctx.considered(not_on_clitics_key.clone());
        ctx.selected(not_on_clitics_key.clone());
        ctx.represented(not_on_clitics_key);
        let no_default_compounding_key = InventoryKey::setting(
            pg_snapshot::InventoryKind::ParserSetting,
            "noDefaultCompounding",
        );
        ctx.considered(no_default_compounding_key.clone());
        ctx.selected(no_default_compounding_key.clone());
        ctx.represented(no_default_compounding_key);
    }
    let morphology_stratum = StratumId(0);
    let clitic_stratum = StratumId(1);
    let surface_stratum = StratumId(2);

    // --- compound rules (defaults if none authored) ----------------------------------------------
    let mut morphology_mrules: Vec<MRuleId> = Vec::new();
    compounding::build(snapshot, &ctx, &mut acc, &mut morphology_mrules)?;

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
    )?;

    // --- affix templates (+ null-affix synthesis for irregular-form slots) ---------------------
    let morphology_templates = templates::build(snapshot, &ctx, &mut acc)?;
    templates::record_unconsidered_slots(snapshot, &ctx);

    // --- phonological rules ----------------------------------------------------------------------
    let (prules, morphology_prules, clitic_prules) = rules::build(snapshot, &ctx)?;
    rules::record_unreferenced_feature_constraints(snapshot, &ctx);

    // Ad-hoc co-occurrence rules: post-hoc, run after every entry/rule is loaded so the guid -> registry maps are fully populated; `xml_key` doubles as the MSA/entry guid this morpheme was built from.
    for (i, m) in acc.morphemes.iter().enumerate() {
        acc.msa_guid_index
            .insert(m.xml_key.clone(), MorphemeId(i as u32));
    }
    strata_assign_co_occurrence(snapshot, &ctx, &mut acc);
    environment::record_unattempted(snapshot, &ctx);
    // The recorder and lineage must leave `ctx` before `Grammar` takes ownership of what `ctx` borrows.
    let owner_warnings = ctx.owner_warnings.into_inner();
    let environment_resolutions = ctx
        .environment_resolutions
        .into_inner()
        .into_values()
        .collect();
    let mut recorder = ctx.recorder.into_inner();
    let lineage = ctx.lineage.into_inner();
    let pending_rule_refusals = ctx.pending_rule_refusals.into_inner();
    let allomorph_gates = ctx.allomorph_gates.into_inner().into_outcomes();

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

    let mut grammar = Grammar {
        name: Some(snapshot.project.name.clone()),
        phon_features,
        char_tables: vec![char_table],
        syn_features: syn,
        fs_interner: acc.fs_interner,
        mpr_names: mpr.mpr_names,
        mpr_features: mpr.mpr_features,
        mpr_groups: mpr.mpr_groups,
        stem_names,
        families: Vec::new(),
        natural_classes,
        morphemes: acc.morphemes,
        allomorph_owners: acc.allomorph_owners,
        allomorph_sources: acc.allomorph_sources,
        prules,
        mrules: acc.mrules,
        templates: acc.templates,
        entries: acc.entries,
        strata,
    };

    // Mrule + morpheme-co-occurrence reachability compaction (see `reachability::compact_mrules`'s own doc); runs before the natural-class compaction below so an orphan rule's class is correctly treated as unreferenced too.
    let (removed_mrules, removed_allomorph_cooccurrence) =
        reachability::compact_mrules(&mut grammar);
    resolve_pending_rule_refusals(&mut recorder, pending_rule_refusals, &removed_mrules);
    let removed_cooccurrence = reachability::trim_unreachable_morpheme_coocurrence(&mut grammar);

    // `pg-fwdata` extracts every declared natural class unconditionally, so compact to only those actually referenced now that every other compile step has had its chance to resolve one (see `natclass::compact_to_referenced`'s own doc).
    let removed_natclasses =
        natclass::compact_to_referenced(&mut grammar, any_nc, natclass_last_unnamed);

    // Revokes exactly what the four finalizers above report they dropped, via the lineage every owner published at push time -- see `inventory::finalize`'s own doc.
    inventory::finalize(
        snapshot,
        &mut recorder,
        &lineage,
        removed_mrules,
        removed_allomorph_cooccurrence,
        removed_cooccurrence,
        removed_natclasses,
    );

    grammar.final_template_prune_facts()?;

    Ok((
        grammar,
        recorder,
        substrate_report,
        substrate_issues,
        owner_warnings,
        environment_resolutions,
        allomorph_gates,
    ))
}

/// Ad-hoc co-occurrence rules resolved against the now-complete `acc.allomorph_guid_index`/`acc.msa_guid_index` registries; a dangling reference is a warning, never a hard failure.
fn strata_assign_co_occurrence(snapshot: &Snapshot, ctx: &Ctx, acc: &mut Acc) {
    use pg_snapshot::morphology::AdhocProhibition;
    use pg_snapshot::morphology::Adjacency as SnapAdjacency;
    use pg_snapshot::InventoryKind::{AllomorphCoOccurrence, MorphemeCoOccurrence};

    fn adjacency(a: SnapAdjacency) -> CoOccurrenceAdjacency {
        match a {
            SnapAdjacency::Anywhere => CoOccurrenceAdjacency::Anywhere,
            SnapAdjacency::SomewhereToLeft => CoOccurrenceAdjacency::SomewhereToLeft,
            SnapAdjacency::SomewhereToRight => CoOccurrenceAdjacency::SomewhereToRight,
            SnapAdjacency::AdjacentToLeft => CoOccurrenceAdjacency::AdjacentToLeft,
            SnapAdjacency::AdjacentToRight => CoOccurrenceAdjacency::AdjacentToRight,
        }
    }

    // A morpheme owned by an affix mrule might still be pruned as unreachable dead code by reachability compaction; a stem-entry morpheme never is.
    let mut morpheme_owning_mrule: HashMap<u32, MRuleId> = HashMap::new();
    for (i, def) in acc.mrules.iter().enumerate() {
        let morpheme = match def {
            MorphRuleDef::AffixProcess(d) => d.morpheme,
            MorphRuleDef::Realizational(d) => d.morpheme,
            MorphRuleDef::Compounding(_) => continue,
        };
        morpheme_owning_mrule.insert(morpheme.0, MRuleId(i as u32));
    }

    for rule in &snapshot.morphology.adhoc_prohibitions {
        match rule {
            AdhocProhibition::Allomorph {
                guid,
                disabled,
                primary,
                others,
                adjacency: adj,
            } => {
                let key = InventoryKey::object(AllomorphCoOccurrence, guid.clone());
                ctx.considered(key.clone());
                if *disabled {
                    ctx.record_load_decision(pg_snapshot::LoadDecisionDraft {
                        subject: key,
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
                ctx.selected(key.clone());
                let Some(&primary_id) = acc.allomorph_guid_index.get(primary) else {
                    ctx.reject(
                        key,
                        issue_codes::ADHOC_PROHIBITION_UNRESOLVED,
                        IssueClass::InvalidSource,
                        format!(
                            "ad-hoc allomorph prohibition: primary allomorph {primary:?} does not \
                             resolve; skipped"
                        ),
                    );
                    continue;
                };
                let mut other_ids = Vec::with_capacity(others.len());
                let mut unresolved = Vec::new();
                for o in others {
                    match acc.allomorph_guid_index.get(o) {
                        Some(&id) => other_ids.push(id),
                        None => unresolved.push(o.clone()),
                    }
                }
                if !unresolved.is_empty() {
                    // primary_id resolved, so dropping this would silently permit a combination it was authored to prohibit.
                    let message = format!(
                        "ad-hoc allomorph prohibition on active allomorph {primary:?}: \
                         'others' target(s) {unresolved:?} do not resolve; refusing rather \
                         than silently dropping a prohibition on a real allomorph"
                    );
                    match acc.allomorph_owners[primary_id.0 as usize] {
                        // A root-owned allomorph always survives reachability compaction, so there is nothing to defer.
                        AllomorphOwner::Root(..) => ctx.refuse(
                            key,
                            issue_codes::ADHOC_PROHIBITION_UNRESOLVED,
                            IssueClass::InvalidSource,
                            message,
                        ),
                        // An affix-owned primary's own mrule might still be pruned as unreachable dead code -- defer until that is known.
                        AllomorphOwner::Affix(mr, _) => ctx.defer_cooccurrence_refusal(
                            key,
                            issue_codes::ADHOC_PROHIBITION_UNRESOLVED,
                            IssueClass::InvalidSource,
                            message,
                            mr,
                        ),
                    }
                    continue;
                }
                if other_ids.is_empty() {
                    ctx.reject(
                        key,
                        issue_codes::ADHOC_PROHIBITION_UNRESOLVED,
                        IssueClass::InvalidSource,
                        "ad-hoc allomorph prohibition: 'others' is empty, nothing to prohibit",
                    );
                    continue;
                }
                let index = allomorph_co_occurrence_len(acc, primary_id);
                ctx.represent_via(
                    LineageTarget::AllomorphCoOccurrence(primary_id.0, index),
                    key,
                );
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
                        MorphRuleDef::AffixProcess(d) => {
                            d.allomorphs[idx as usize].co_occurrence.push(def)
                        }
                        MorphRuleDef::Realizational(d) => {
                            d.allomorphs[idx as usize].co_occurrence.push(def)
                        }
                        MorphRuleDef::Compounding(_) => {}
                    },
                }
            }
            AdhocProhibition::Morpheme {
                guid,
                disabled,
                primary,
                others,
                adjacency: adj,
            } => {
                let key = InventoryKey::object(MorphemeCoOccurrence, guid.clone());
                ctx.considered(key.clone());
                if *disabled {
                    ctx.record_load_decision(pg_snapshot::LoadDecisionDraft {
                        subject: key,
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
                ctx.selected(key.clone());
                let Some(&primary_id) = acc.msa_guid_index.get(primary) else {
                    ctx.reject(
                        key,
                        issue_codes::ADHOC_PROHIBITION_UNRESOLVED,
                        IssueClass::InvalidSource,
                        format!(
                            "ad-hoc morpheme prohibition: primary morpheme {primary:?} does not \
                             resolve; skipped"
                        ),
                    );
                    continue;
                };
                let mut other_ids = Vec::with_capacity(others.len());
                let mut unresolved = Vec::new();
                for o in others {
                    match acc.msa_guid_index.get(o) {
                        Some(&id) => other_ids.push(id),
                        None => unresolved.push(o.clone()),
                    }
                }
                if !unresolved.is_empty() {
                    // primary_id resolved, so dropping this would silently permit a combination it was authored to prohibit.
                    let message = format!(
                        "ad-hoc morpheme prohibition on active morpheme {primary:?}: \
                         'others' target(s) {unresolved:?} do not resolve; refusing rather \
                         than silently dropping a prohibition on a real morpheme"
                    );
                    match morpheme_owning_mrule.get(&primary_id.0) {
                        // A stem-entry morpheme always survives reachability compaction, so there is nothing to defer.
                        None => ctx.refuse(
                            key,
                            issue_codes::ADHOC_PROHIBITION_UNRESOLVED,
                            IssueClass::InvalidSource,
                            message,
                        ),
                        // An affix-owned primary's own mrule might still be pruned as unreachable dead code -- defer until that is known.
                        Some(&mr) => ctx.defer_cooccurrence_refusal(
                            key,
                            issue_codes::ADHOC_PROHIBITION_UNRESOLVED,
                            IssueClass::InvalidSource,
                            message,
                            mr,
                        ),
                    }
                    continue;
                }
                if other_ids.is_empty() {
                    ctx.reject(
                        key,
                        issue_codes::ADHOC_PROHIBITION_UNRESOLVED,
                        IssueClass::InvalidSource,
                        "ad-hoc morpheme prohibition: 'others' is empty, nothing to prohibit",
                    );
                    continue;
                }
                let index = acc.morphemes[primary_id.0 as usize].co_occurrence.len();
                ctx.represent_via(
                    LineageTarget::MorphemeCoOccurrence(primary_id.0, index),
                    key,
                );
                acc.morphemes[primary_id.0 as usize].co_occurrence.push(
                    MorphemeCoOccurrenceRuleDef {
                        require: false,
                        others: other_ids,
                        adjacency: adjacency(*adj),
                    },
                );
            }
        }
    }
}

/// Resolves [`PendingRuleRefusal`] severity from the reachability owner's removed-rule facts.
fn resolve_pending_rule_refusals(
    recorder: &mut SelectionRecorder,
    pending: Vec<PendingRuleRefusal>,
    removed_mrules: &[u32],
) {
    for refusal in pending {
        let refusal = if removed_mrules.contains(&refusal.mrule.0) {
            refusal.unreachable("owning rule is unreachable after reachability compaction")
        } else {
            refusal
        };
        recorder.rejected(refusal.key, refusal.issue);
    }
}

/// The index the about-to-be-pushed co-occurrence rule will occupy on `primary_id`'s own list.
fn allomorph_co_occurrence_len(acc: &Acc, primary_id: AllomorphId) -> usize {
    match acc.allomorph_owners[primary_id.0 as usize] {
        AllomorphOwner::Root(le, idx) => acc.entries[le.0 as usize].allomorphs[idx as usize]
            .co_occurrence
            .len(),
        AllomorphOwner::Affix(mr, idx) => match &acc.mrules[mr.0 as usize] {
            MorphRuleDef::AffixProcess(d) => d.allomorphs[idx as usize].co_occurrence.len(),
            MorphRuleDef::Realizational(d) => d.allomorphs[idx as usize].co_occurrence.len(),
            MorphRuleDef::Compounding(_) => 0,
        },
    }
}

// Shared read-only context + mutable accumulator (mirrors `crate::load`'s `Ro`/`Acc` split).

/// Read-only tables built once, up front, and shared by every later compilation phase.
pub(crate) struct Ctx<'a> {
    pub snapshot: &'a Snapshot,
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
    /// Every declared environment, by guid — resolved lazily wherever an allomorph/MSA references one.
    pub env_by_guid: &'a HashMap<&'a str, &'a pg_snapshot::phonology::Environment>,
    pub environment_resolutions: RefCell<BTreeMap<String, EnvironmentResolution>>,
    pub default_vernacular_ws: Option<String>,
    pub default_analysis_ws: Option<String>,
    /// The snapshot-to-grammar selection recorder every owner below writes its considered/selected/represented/rejected/synthesized calls into; behind a `RefCell` since `Ctx` itself is shared by shared reference everywhere.
    pub recorder: RefCell<SelectionRecorder>,
    /// Rich source facts published by the deciding owner, supplementing the coarse inventory issue.
    pub owner_warnings: RefCell<Vec<pg_snapshot::Warning>>,
    /// Which owner published which `represented` keys, read only by `inventory::finalize`.
    pub lineage: RefCell<Lineage>,
    /// Co-occurrence refusals whose primary is affix-owned, so reachability (running after this
    /// context is done) might still prune the primary's own mrule as dead code -- resolved by
    /// `resolve_pending_rule_refusals` once `removed_mrules` is known.
    pub pending_rule_refusals: RefCell<Vec<PendingRuleRefusal>>,
    /// Per-gate effects the owners that read each allomorph's gates actually applied.
    pub allomorph_gates: RefCell<GateOutcomes>,
}

/// A selected constraint issue whose owning rule may be removed by reachability compaction.
pub(crate) struct PendingRuleRefusal {
    key: InventoryKey,
    issue: ConversionIssue,
    mrule: MRuleId,
}

impl PendingRuleRefusal {
    fn unreachable(mut self, reason: &str) -> Self {
        self.issue.class = IssueClass::UnreachableInGrammar;
        self.issue.fatal = false;
        self.issue.message.push_str("; ");
        self.issue.message.push_str(reason);
        self
    }
}

impl Ctx<'_> {
    pub(crate) fn environment_resolution(
        &self,
        env: &pg_snapshot::phonology::Environment,
    ) -> EnvironmentResolution {
        if let Some(result) = self.environment_resolutions.borrow().get(&env.guid) {
            return result.clone();
        }
        let result =
            environment::resolve_environment_expression(&env.guid, &env.representation, self);
        if let Some(token) = result
            .class_tokens
            .iter()
            .find(|token| token.natural_class_index.is_none())
        {
            self.owner_warnings.borrow_mut().push(
                pg_snapshot::Warning::new(
                    ImportWarningCode::EnvironmentMissingNaturalClass,
                    format!(
                        "Environment {:?} names nonexistent natural class {}; the entire environment is discarded, as in FieldWorks and C# HermitCrab.",
                        env.representation, token.token_text
                    ),
                )
                .with_subject(
                    pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::PhEnvironment)
                        .guid(env.guid.clone())
                        .name(env.representation.clone())
                        .field("StringRepresentation"),
                ),
            );
        }
        self.environment_resolutions
            .borrow_mut()
            .insert(env.guid.clone(), result.clone());
        result
    }

    fn abandon_rule_refusals_since(&self, start: usize, reason: &str) {
        let pending: Vec<_> = self
            .pending_rule_refusals
            .borrow_mut()
            .drain(start..)
            .collect();
        let mut recorder = self.recorder.borrow_mut();
        for refusal in pending {
            let refusal = refusal.unreachable(reason);
            recorder.rejected(refusal.key, refusal.issue);
        }
    }

    /// Records an attachment/expansion/setting `key` as sourced (object identities are seeded once in `inventory::seed_authored_from_snapshot` instead).
    pub(crate) fn authored(&self, key: InventoryKey) {
        self.recorder.borrow_mut().authored(key);
    }

    pub(crate) fn considered(&self, key: InventoryKey) {
        self.recorder.borrow_mut().considered(key);
    }

    pub(crate) fn selected(&self, key: InventoryKey) {
        self.recorder.borrow_mut().selected(key);
    }

    pub(crate) fn represented(&self, key: InventoryKey) {
        self.recorder.borrow_mut().represented(key);
    }

    pub(crate) fn represented_with_effective_value(
        &self,
        key: InventoryKey,
        effective_value_json: String,
    ) {
        self.recorder
            .borrow_mut()
            .represented_with_effective_value(key, effective_value_json);
    }

    pub(crate) fn synthesized(&self, key: InventoryKey) {
        self.recorder.borrow_mut().synthesized(key);
    }

    pub(crate) fn record_load_decision(&self, draft: pg_snapshot::LoadDecisionDraft) {
        self.recorder.borrow_mut().record_load_decision(draft);
    }

    pub(crate) fn has_load_decision(&self, key: &InventoryKey) -> bool {
        self.recorder
            .borrow()
            .has_load_decision(key, pg_snapshot::LoadPipelineStage::Compile)
    }

    pub(crate) fn not_considered(&self, key: InventoryKey, reason: pg_snapshot::LoadReasonCode) {
        self.not_considered_in_context(key, String::new(), reason);
    }

    pub(crate) fn not_considered_in_context(
        &self,
        key: InventoryKey,
        context: String,
        reason: pg_snapshot::LoadReasonCode,
    ) {
        self.recorder
            .borrow_mut()
            .not_considered(key, context, reason);
    }

    /// As [`Ctx::represented`], but also publishes `key` into the lineage under `target`, so a
    /// later reachability/reference compaction pass that removes `target` can revoke it by name.
    pub(crate) fn represent_via(&self, target: LineageTarget, key: InventoryKey) {
        inventory::represent_via(
            &mut self.recorder.borrow_mut(),
            &mut self.lineage.borrow_mut(),
            target,
            key,
        );
    }

    /// Records a nonfatal issue at the selected inventory owner.
    pub(crate) fn reject(
        &self,
        key: InventoryKey,
        code: ImportWarningCode,
        class: IssueClass,
        msg: impl Into<String>,
    ) {
        self.reject_with_source(key, code, class, None, msg);
    }

    pub(crate) fn reject_with_subjects(
        &self,
        key: InventoryKey,
        code: ImportWarningCode,
        class: IssueClass,
        warning: pg_snapshot::Warning,
    ) {
        assert_eq!(warning.code, code.wire());
        self.reject(key, code, class, warning.message.clone());
        self.owner_warnings.borrow_mut().push(warning);
    }

    pub(crate) fn reject_with_source(
        &self,
        key: InventoryKey,
        code: ImportWarningCode,
        class: IssueClass,
        source: Option<pg_snapshot::SourceRef>,
        msg: impl Into<String>,
    ) {
        self.record_rejection(key, code, class, source, msg.into(), false);
    }

    fn record_rejection(
        &self,
        key: InventoryKey,
        code: ImportWarningCode,
        class: IssueClass,
        source: Option<pg_snapshot::SourceRef>,
        message: String,
        fatal: bool,
    ) {
        let source = source.or_else(|| warnings::source_for_key(self.snapshot, &key));
        self.recorder.borrow_mut().rejected(
            key,
            ConversionIssue {
                code,
                class,
                source,
                fatal,
                message,
            },
        );
    }

    /// Publishes one gate effect; see [`GateOutcomes::record`] for how several reads combine.
    pub(crate) fn record_gate(&self, outcome: AllomorphGateOutcome) {
        self.allomorph_gates.borrow_mut().record(outcome);
    }

    pub(crate) fn note(
        &self,
        code: ImportWarningCode,
        class: IssueClass,
        source: SourceRef,
        msg: impl Into<String>,
    ) {
        inventory::note(&mut self.recorder.borrow_mut(), code, class, source, msg);
    }

    /// As [`Ctx::reject`], but fatal -- for a construct attached to something already active, where dropping it would change what the grammar accepts. Fatal refusals are reported through the structured issue result.
    pub(crate) fn refuse(
        &self,
        key: InventoryKey,
        code: ImportWarningCode,
        class: IssueClass,
        msg: impl Into<String>,
    ) {
        self.refuse_with_source(key, code, class, None, msg);
    }

    pub(crate) fn refuse_with_source(
        &self,
        key: InventoryKey,
        code: ImportWarningCode,
        class: IssueClass,
        source: Option<pg_snapshot::SourceRef>,
        msg: impl Into<String>,
    ) {
        self.record_rejection(key, code, class, source, msg.into(), true);
    }

    /// As [`Ctx::refuse`], but for a co-occurrence primary that is affix-owned: reachability
    /// compaction (which runs after this `Ctx` is gone) might still prune the primary's own mrule
    /// as dead code, so the fatal/non-fatal call is deferred to
    /// `resolve_pending_rule_refusals` rather than decided here.
    pub(crate) fn defer_cooccurrence_refusal(
        &self,
        key: InventoryKey,
        code: ImportWarningCode,
        class: IssueClass,
        message: String,
        mrule: MRuleId,
    ) {
        let source = warnings::source_for_key(self.snapshot, &key);
        self.defer_rule_refusal(
            key,
            ConversionIssue {
                code,
                class,
                source,
                fatal: true,
                message,
            },
            mrule,
        );
    }

    pub(crate) fn defer_rule_refusal(
        &self,
        key: InventoryKey,
        issue: ConversionIssue,
        mrule: MRuleId,
    ) {
        self.pending_rule_refusals
            .borrow_mut()
            .push(PendingRuleRefusal { key, issue, mrule });
    }

    /// The `authored → considered → selected → represented|rejected` sequence every attachment-resolution site repeats; `resolved` picks the branch.
    pub(crate) fn record_attachment(
        &self,
        key: InventoryKey,
        resolved: bool,
        code: ImportWarningCode,
        class: IssueClass,
        msg: impl Into<String>,
    ) {
        self.authored(key.clone());
        self.considered(key.clone());
        self.selected(key.clone());
        if resolved {
            self.represented(key);
        } else {
            self.reject(key, code, class, msg);
        }
    }
}

/// Everything the compiler appends to as it walks the snapshot's strata-worth of content.
pub(crate) struct Acc {
    pub fs_interner: Interner<FeatureStruct>,
    pub mrules: Vec<MorphRuleDef>,
    pub morphemes: Vec<MorphemeInfo>,
    pub allomorph_owners: Vec<AllomorphOwner>,
    pub allomorph_sources: Vec<AllomorphSource>,
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
pub(crate) fn best_ws<'a>(
    forms: &'a [pg_snapshot::WsForm],
    preferred_ws: Option<&str>,
) -> Option<&'a str> {
    if let Some(ws) = preferred_ws {
        if let Some(f) = forms.iter().find(|f| f.ws == ws) {
            return Some(&f.form);
        }
    }
    forms.first().map(|f| f.form.as_str())
}

/// FieldWorks' headword: the citation form, else the lexeme form (always the last allomorph).
pub(crate) fn entry_headword<'a>(
    entry: &'a pg_snapshot::lexicon::LexEntry,
    preferred_ws: Option<&str>,
) -> Option<&'a str> {
    let non_empty = |form: &&str| !form.trim().is_empty();
    best_ws(&entry.citation_form, preferred_ws)
        .filter(non_empty)
        .or_else(|| {
            let lexeme_form = entry.allomorphs.last()?;
            best_ws(&lexeme_form.forms, preferred_ws).filter(non_empty)
        })
}

/// Every representation tagged with `preferred_ws` (there may be several — multiple `PhCode`s in
/// the same writing system, e.g. Sena's `m`/`n` phoneme). Falls back to every representation if
/// none matches (tolerant — see `Ctx::default_vernacular_ws`'s doc).
pub(crate) fn ws_forms<'a>(
    forms: &'a [pg_snapshot::WsForm],
    preferred_ws: Option<&str>,
) -> Vec<&'a str> {
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
