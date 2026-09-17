//! Builds one `AffixProcessRuleDef` per (entry, MSA) pair from concatenative, `MoAffixProcess`-style, and circumfix-cross-product allomorphs.

use pg_snapshot::lexicon::{Allomorph, LexEntry, Msa, RuleMapping};
use pg_snapshot::morphology::MorphType;
use pg_snapshot::phonology::PhonContext;
use pg_snapshot::{
    ConversionIssue, InventoryKey, InventoryKind, IssueClass, SelectionRecorder, Snapshot,
    SourceRef,
};

use crate::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, AllomorphOwner, EnvironmentDef, MRuleId,
    MorphRuleDef, MorphemeId, MorphemeInfo, OutputAction, PartRef, Pattern, PatternNode,
    ReduplicationHint, SimpleContext, SourceMorphPlacement, StratumId,
};

use super::environment;
use super::inventory::LineageTarget;
use super::issue_codes;
use super::roles;
use super::{Acc, Ctx};

/// Which concatenative shape an affix morph type implies; `None` for a type this compiler does not build a rule for (circumfix, bare clitic/particle, phrase-shaped).
#[derive(Copy, Clone)]
pub(crate) enum Shape {
    Prefix,
    Suffix,
    Infix,
}

pub(crate) fn shape_of(mt: MorphType) -> Option<Shape> {
    match mt {
        // Proclitic patterns like a prefix, enclitic like a suffix; clitic-ness lives in stratum placement (`lexicon::build`), not in the allomorph pattern shape.
        MorphType::Prefix | MorphType::PrefixingInterfix | MorphType::Proclitic => {
            Some(Shape::Prefix)
        }
        MorphType::Suffix | MorphType::SuffixingInterfix | MorphType::Enclitic => {
            Some(Shape::Suffix)
        }
        MorphType::Infix | MorphType::InfixingInterfix => Some(Shape::Infix),
        // Bare Clitic/Particle/Phrase are never rule forms; they are stem forms (clitic-stratum lex entries).
        _ => None,
    }
}

/// Builds the `MorphRuleDef::AffixProcess` for one (entry, MSA) pair; returns `None` if it ends up with zero loadable allomorphs (never an error — every dropped allomorph is a pushed warning instead). `allos` is the caller's pre-partitioned allomorph bucket for this stratum.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_affix_rule(
    entry: &LexEntry,
    allos: &[&Allomorph],
    msa: &Msa,
    gloss: Option<String>,
    stratum: StratumId,
    ctx: &Ctx,
    acc: &mut Acc,
    warnings: &mut Vec<String>,
) -> Option<MRuleId> {
    let mut rule_form_allos: Vec<&Allomorph> = Vec::new();
    for &allo in allos {
        if is_valid_rule_form(allo, ctx, warnings) {
            ctx.selected(InventoryKey::object(
                InventoryKind::Allomorph,
                allo.guid.clone(),
            ));
            rule_form_allos.push(allo);
        }
    }
    if rule_form_allos.is_empty() {
        return None;
    }

    let msa_key = InventoryKey::object(InventoryKind::Msa, msa.guid().to_string());
    ctx.selected(msa_key.clone());

    let mrule_id = MRuleId(acc.mrules.len() as u32);

    let (required_syn_fs, out_syn_fs, partial, msa_guid) = match msa {
        Msa::Derivational {
            guid,
            from_part_of_speech,
            to_part_of_speech,
            from_features,
            to_features,
            ..
        } => {
            let req_pos = from_part_of_speech
                .as_deref()
                .map(|p| ctx.pos.bits_with_descendants(std::iter::once(p)));
            let req = match super::features::build_syn_fs(ctx.syn, req_pos, from_features.as_ref())
            {
                Ok(fs) => acc.fs_interner.intern(fs),
                Err(e) => {
                    ctx.reject(
                        warnings,
                        msa_key.clone(),
                        issue_codes::MSA_BUILD_FAILED,
                        IssueClass::UnrepresentableForHc,
                        format!("MSA {guid:?}: {e}; skipped"),
                    );
                    return None;
                }
            };
            let out_pos = to_part_of_speech
                .as_deref()
                .and_then(|p| ctx.pos.bits_single(p));
            let out = match super::features::build_syn_fs(ctx.syn, out_pos, to_features.as_ref()) {
                Ok(fs) => acc.fs_interner.intern(fs),
                Err(e) => {
                    ctx.reject(
                        warnings,
                        msa_key.clone(),
                        issue_codes::MSA_BUILD_FAILED,
                        IssueClass::UnrepresentableForHc,
                        format!("MSA {guid:?}: {e}; skipped"),
                    );
                    return None;
                }
            };
            (req, out, false, guid.clone())
        }
        Msa::Inflectional {
            guid,
            part_of_speech,
            slots,
            features,
            ..
        } => {
            let req_pos = part_of_speech
                .as_deref()
                .map(|p| ctx.pos.bits_with_descendants(std::iter::once(p)));
            let req = match super::features::build_syn_fs(ctx.syn, req_pos, features.as_ref()) {
                Ok(fs) => acc.fs_interner.intern(fs),
                Err(e) => {
                    ctx.reject(
                        warnings,
                        msa_key.clone(),
                        issue_codes::MSA_BUILD_FAILED,
                        IssueClass::UnrepresentableForHc,
                        format!("MSA {guid:?}: {e}; skipped"),
                    );
                    return None;
                }
            };
            let empty = acc.fs_interner.intern(pg_featstruct::FeatureStruct::EMPTY);
            (req, empty, slots.is_empty(), guid.clone())
        }
        Msa::Unclassified {
            guid,
            part_of_speech,
        } => {
            let req_pos = part_of_speech
                .as_deref()
                .map(|p| ctx.pos.bits_with_descendants(std::iter::once(p)));
            let req = match super::features::build_syn_fs(ctx.syn, req_pos, None) {
                Ok(fs) => acc.fs_interner.intern(fs),
                Err(e) => {
                    ctx.reject(
                        warnings,
                        msa_key.clone(),
                        issue_codes::MSA_BUILD_FAILED,
                        IssueClass::UnrepresentableForHc,
                        format!("MSA {guid:?}: {e}; skipped"),
                    );
                    return None;
                }
            };
            let empty = acc.fs_interner.intern(pg_featstruct::FeatureStruct::EMPTY);
            (req, empty, true, guid.clone())
        }
        Msa::Stem {
            guid,
            from_parts_of_speech,
            ..
        } => {
            // A stem MSA reached through the rule path (clitic/mixed affix forms): required FS is just the attachment POS list, nothing else.
            let req_pos = if from_parts_of_speech.is_empty() {
                None
            } else {
                Some(
                    ctx.pos
                        .bits_with_descendants(from_parts_of_speech.iter().map(String::as_str)),
                )
            };
            let req = match super::features::build_syn_fs(ctx.syn, req_pos, None) {
                Ok(fs) => acc.fs_interner.intern(fs),
                Err(e) => {
                    ctx.reject(
                        warnings,
                        msa_key.clone(),
                        issue_codes::MSA_BUILD_FAILED,
                        IssueClass::UnrepresentableForHc,
                        format!("MSA {guid:?}: {e}; skipped"),
                    );
                    return None;
                }
            };
            let empty = acc.fs_interner.intern(pg_featstruct::FeatureStruct::EMPTY);
            (req, empty, false, guid.clone())
        }
    };

    // Required MPR features: exception features, for every affix-rule kind.
    let required_mpr = match msa {
        Msa::Derivational {
            from_exception_features,
            from_inflection_class,
            ..
        } => {
            let mut set = crate::model::MprSet::EMPTY;
            for f in from_exception_features {
                let attachment = InventoryKey::attachment(
                    InventoryKind::RuleFeature,
                    msa_guid.clone(),
                    f.clone(),
                    roles::REQUIRED,
                );
                let resolved = ctx.mpr.exception_feature(f);
                ctx.record_attachment(
                    warnings,
                    attachment,
                    resolved.is_some(),
                    issue_codes::MSA_EXCEPTION_FEATURE_UNRESOLVED,
                    IssueClass::InvalidSource,
                    format!("MSA {msa_guid:?}: exception feature {f:?} does not resolve"),
                );
                if let Some(s) = resolved {
                    set = set.union(s);
                }
            }
            if let Some(ic) = from_inflection_class {
                let attachment = InventoryKey::attachment(
                    InventoryKind::InflectionClass,
                    msa_guid.clone(),
                    ic.clone(),
                    roles::REQUIRED,
                );
                let resolved = ctx.mpr.infl_class_with_descendants(ic);
                ctx.record_attachment(
                    warnings,
                    attachment,
                    resolved.is_some(),
                    issue_codes::MSA_INFLECTION_CLASS_UNRESOLVED,
                    IssueClass::InvalidSource,
                    format!("MSA {msa_guid:?}: inflection class {ic:?} does not resolve"),
                );
                if let Some(s) = resolved {
                    set = set.union(s);
                }
            }
            set
        }
        Msa::Inflectional {
            exception_features, ..
        } => {
            let mut set = crate::model::MprSet::EMPTY;
            for f in exception_features {
                let attachment = InventoryKey::attachment(
                    InventoryKind::RuleFeature,
                    msa_guid.clone(),
                    f.clone(),
                    roles::REQUIRED,
                );
                let resolved = ctx.mpr.exception_feature(f);
                ctx.record_attachment(
                    warnings,
                    attachment,
                    resolved.is_some(),
                    issue_codes::MSA_EXCEPTION_FEATURE_UNRESOLVED,
                    IssueClass::InvalidSource,
                    format!("MSA {msa_guid:?}: exception feature {f:?} does not resolve"),
                );
                if let Some(s) = resolved {
                    set = set.union(s);
                }
            }
            set
        }
        _ => crate::model::MprSet::EMPTY,
    };
    let out_mpr = match msa {
        Msa::Derivational {
            to_exception_features,
            to_inflection_class,
            ..
        } => {
            let mut set = crate::model::MprSet::EMPTY;
            for f in to_exception_features {
                let attachment = InventoryKey::attachment(
                    InventoryKind::RuleFeature,
                    msa_guid.clone(),
                    f.clone(),
                    roles::OUT,
                );
                let resolved = ctx.mpr.exception_feature(f);
                ctx.record_attachment(
                    warnings,
                    attachment,
                    resolved.is_some(),
                    issue_codes::MSA_EXCEPTION_FEATURE_UNRESOLVED,
                    IssueClass::InvalidSource,
                    format!("MSA {msa_guid:?}: exception feature {f:?} does not resolve"),
                );
                if let Some(s) = resolved {
                    set = set.union(s);
                }
            }
            if let Some(ic) = to_inflection_class {
                let attachment = InventoryKey::attachment(
                    InventoryKind::InflectionClass,
                    msa_guid.clone(),
                    ic.clone(),
                    roles::OUT,
                );
                let resolved = ctx.mpr.infl_class_single(ic);
                ctx.record_attachment(
                    warnings,
                    attachment,
                    resolved.is_some(),
                    issue_codes::MSA_INFLECTION_CLASS_UNRESOLVED,
                    IssueClass::InvalidSource,
                    format!("MSA {msa_guid:?}: inflection class {ic:?} does not resolve"),
                );
                if let Some(s) = resolved {
                    set = set.union(s);
                }
            }
            set
        }
        _ => crate::model::MprSet::EMPTY,
    };

    let required_stem_name = match msa {
        Msa::Derivational {
            from_stem_name: Some(sn),
            ..
        } => {
            let attachment = InventoryKey::attachment(
                InventoryKind::StemName,
                msa_guid.clone(),
                sn.clone(),
                roles::REQUIRED,
            );
            let id = ctx.stem_name_by_guid.get(sn).copied();
            ctx.record_attachment(
                warnings,
                attachment,
                id.is_some(),
                issue_codes::MSA_STEM_NAME_UNRESOLVED,
                IssueClass::InvalidSource,
                format!("MSA {msa_guid:?}: stem name {sn:?} does not resolve"),
            );
            id
        }
        _ => None,
    };

    // A circumfix's allomorph set is the cross-product of its halves, not one def per stored form, so it is built from the whole bucket rather than per-allomorph.
    let built: Vec<(Vec<String>, SourceMorphPlacement, AffixAllomorphDef)> =
        if entry.lexeme_morph_type == MorphType::Circumfix {
            build_circumfix_allomorphs(
                entry,
                &rule_form_allos,
                msa,
                required_mpr,
                out_mpr,
                mrule_id,
                ctx,
                acc,
                warnings,
            )
        } else {
            rule_form_allos
                .iter()
                .flat_map(|allo| {
                    let placement = match shape_of(allo.morph_type) {
                        Some(Shape::Infix) => SourceMorphPlacement::InsertBeforeLast,
                        _ => SourceMorphPlacement::Append,
                    };
                    build_affix_allomorphs_for(allo, msa, required_mpr, out_mpr, ctx, acc, warnings)
                        .into_iter()
                        .map(|def| (vec![allo.guid.clone()], placement, def))
                        .collect::<Vec<_>>()
                })
                .collect()
        };

    let mut allomorphs = Vec::new();
    for (source_guids, placement, def) in built {
        let allo_id = AllomorphId(acc.allomorph_owners.len() as u32);
        acc.allomorph_owners
            .push(AllomorphOwner::Affix(mrule_id, allomorphs.len() as u16));
        for guid in &source_guids {
            ctx.represent_via(
                LineageTarget::MRule(mrule_id.0),
                InventoryKey::object(InventoryKind::Allomorph, guid.clone()),
            );
        }
        acc.allomorph_sources.push(crate::model::AllomorphSource {
            form_guids: source_guids.into_iter().map(Some).collect(),
            omitted: false,
            placement,
        });
        // First-wins for a circumfix, whose halves each appear in several pairings; this index only resolves ad-hoc co-occurrence references, where a miss is already a warning.
        if let Some(guid) = acc
            .allomorph_sources
            .last()
            .and_then(|source| source.form_guids.first())
            .and_then(Option::as_deref)
        {
            acc.allomorph_guid_index
                .entry(guid.to_string())
                .or_insert(allo_id);
        }
        allomorphs.push(AffixAllomorphDef { id: allo_id, ..def });
    }
    if allomorphs.is_empty() {
        ctx.reject_quietly(
            msa_key,
            issue_codes::MSA_NO_RULE_FORM_ALLOMORPHS,
            IssueClass::UnrepresentableForHc,
            "affix rule has zero loadable allomorphs",
        );
        return None;
    }

    let morpheme = MorphemeId(acc.morphemes.len() as u32);
    acc.morphemes.push(MorphemeInfo {
        xml_key: msa_guid.clone(),
        source_msa_guid: Some(msa_guid.clone()),
        source_infl_type_guid: None,
        morph_id: None,
        gloss,
        stratum,
        properties: Vec::new(),
        co_occurrence: Vec::new(),
    });

    acc.mrules
        .push(MorphRuleDef::AffixProcess(AffixProcessRuleDef {
            morpheme,
            name: entry.citation_form.first().map(|f| f.form.clone()),
            blockable: true,
            partial,
            max_apps: 1,
            required_syn_fs,
            out_syn_fs,
            obligatory_features: Vec::new(),
            required_stem_name,
            allomorphs,
            is_template_rule: false,
        }));

    // Slot -> rule registry: only `Msa::Inflectional` MSAs declare template slots.
    if let Msa::Inflectional { slots, .. } = msa {
        for slot in slots {
            acc.slot_rules
                .entry(slot.clone())
                .or_default()
                .push(mrule_id);
        }
    }

    ctx.represent_via(LineageTarget::MRule(mrule_id.0), msa_key);
    Some(mrule_id)
}

/// Whether `mt` is a circumfix's leading half.
pub(crate) fn is_circumfix_prefix_half(mt: MorphType) -> bool {
    matches!(
        mt,
        MorphType::Prefix | MorphType::PrefixingInterfix | MorphType::Proclitic
    )
}

/// Whether `mt` is a circumfix's trailing half.
pub(crate) fn is_circumfix_suffix_half(mt: MorphType) -> bool {
    matches!(
        mt,
        MorphType::Suffix | MorphType::SuffixingInterfix | MorphType::Enclitic
    )
}

/// One allomorph per (prefix, prefix-env) x (suffix, suffix-env) combination, HCLoader's own cross product (HCLoader.cs:1060-1332); see docs/divergences/039.
#[allow(clippy::too_many_arguments)]
fn build_circumfix_allomorphs(
    entry: &LexEntry,
    allos: &[&Allomorph],
    msa: &Msa,
    required_mpr: crate::model::MprSet,
    out_mpr: crate::model::MprSet,
    mrule_id: MRuleId,
    ctx: &Ctx,
    acc: &mut Acc,
    warnings: &mut Vec<String>,
) -> Vec<(Vec<String>, SourceMorphPlacement, AffixAllomorphDef)> {
    let halves = |pick: fn(MorphType) -> bool| -> Vec<&Allomorph> {
        allos
            .iter()
            .copied()
            .filter(|a| a.process.is_none() && pick(a.morph_type))
            .collect()
    };
    let prefixes = halves(is_circumfix_prefix_half);
    let suffixes = halves(is_circumfix_suffix_half);

    if prefixes.is_empty() || suffixes.is_empty() {
        warnings.push(format!(
            "circumfix entry {:?}: found {} prefix half/halves and {} suffix half/halves; a \
             circumfix needs at least one of each, so no rule is built",
            entry.guid,
            prefixes.len(),
            suffixes.len()
        ));
        for allo in allos {
            ctx.reject_quietly(
                InventoryKey::object(InventoryKind::Allomorph, allo.guid.clone()),
                issue_codes::CIRCUMFIX_MISSING_HALF,
                IssueClass::UnrepresentableForHc,
                "circumfix entry is missing a prefix or suffix half",
            );
        }
        return Vec::new();
    }

    let mut out = Vec::new();
    for prefix in &prefixes {
        let prefix_form = super::format_form(
            super::best_ws(&prefix.forms, ctx.default_vernacular_ws.as_deref()).unwrap_or(""),
        );
        for suffix in &suffixes {
            let expansion = InventoryKey::expansion(
                InventoryKind::Allomorph,
                prefix.guid.clone(),
                vec![suffix.guid.clone()],
                roles::CIRCUMFIX_CROSS_PRODUCT,
            );
            ctx.synthesized(expansion.clone());
            ctx.considered(expansion.clone());
            ctx.selected(expansion.clone());
            let suffix_form = super::format_form(
                super::best_ws(&suffix.forms, ctx.default_vernacular_ws.as_deref()).unwrap_or(""),
            );
            let lead = match insert_segments(&format!("{prefix_form}+"), ctx) {
                Ok(a) => a,
                Err(e) => {
                    ctx.reject_quietly(
                        expansion,
                        issue_codes::ALLOMORPH_UNSEGMENTABLE,
                        IssueClass::UnrepresentableForHc,
                        format!("circumfix allomorph {:?}: {e}; skipped", prefix.guid),
                    );
                    warnings.push(format!(
                        "circumfix allomorph {:?}: {e}; skipped",
                        prefix.guid
                    ));
                    continue;
                }
            };
            let trail = match insert_segments(&format!("+{suffix_form}"), ctx) {
                Ok(a) => a,
                Err(e) => {
                    ctx.reject_quietly(
                        expansion,
                        issue_codes::ALLOMORPH_UNSEGMENTABLE,
                        IssueClass::UnrepresentableForHc,
                        format!("circumfix allomorph {:?}: {e}; skipped", suffix.guid),
                    );
                    warnings.push(format!(
                        "circumfix allomorph {:?}: {e}; skipped",
                        suffix.guid
                    ));
                    continue;
                }
            };
            ctx.represent_via(LineageTarget::MRule(mrule_id.0), expansion);
            // The suffix half is built into this pairing too, not just claimed via the prefix's own returned guid.
            ctx.represent_via(
                LineageTarget::MRule(mrule_id.0),
                InventoryKey::object(InventoryKind::Allomorph, suffix.guid.clone()),
            );

            // `GetAffixAllomorphEnvironments` (HCLoader.cs:1167-1170): `positions` chained onto `environments`, one pass per resolved environment plus a trailing blank pass if the list was empty or any entry failed.
            let prefix_env_guids: Vec<&str> = prefix
                .environments
                .iter()
                .chain(&prefix.positions)
                .map(String::as_str)
                .collect();
            let prefix_passes =
                resolve_environments(&prefix_env_guids, &prefix.guid, ctx, warnings);
            let suffix_env_guids: Vec<&str> = suffix
                .environments
                .iter()
                .chain(&suffix.positions)
                .map(String::as_str)
                .collect();
            let suffix_passes =
                resolve_environments(&suffix_env_guids, &suffix.guid, ctx, warnings);

            for prefix_pass in &prefix_passes {
                for suffix_pass in &suffix_passes {
                    let (lhs_nodes, environments) = match build_circumfix_lhs(
                        prefix_pass.as_ref(),
                        suffix_pass.as_ref(),
                        ctx,
                    ) {
                        Ok(v) => v,
                        Err(e) => {
                            warnings.push(format!(
                                "circumfix allomorph {:?}/{:?}: {e}; one environment combination \
                                 skipped",
                                prefix.guid, suffix.guid
                            ));
                            continue;
                        }
                    };
                    out.push((
                        vec![prefix.guid.clone(), suffix.guid.clone()],
                        SourceMorphPlacement::Append,
                        AffixAllomorphDef {
                            id: AllomorphId(0),
                            environments,
                            co_occurrence: Vec::new(),
                            required_syn_fs: acc
                                .fs_interner
                                .intern(pg_featstruct::FeatureStruct::EMPTY),
                            vars: crate::model::VarTable::default(),
                            required_mpr,
                            excluded_mpr: crate::model::MprSet::EMPTY,
                            out_mpr,
                            redup_hint: ReduplicationHint::Implicit,
                            lhs: vec![Pattern { nodes: lhs_nodes }],
                            // Leading AND trailing insert around one copy: what `pg_foma::emit::classify_affix` reads as `Role::CircumfixPrefix`.
                            rhs: vec![
                                lead.clone(),
                                OutputAction::Copy(PartRef::Input(0)),
                                trail.clone(),
                            ],
                            properties: Vec::new(),
                        },
                    ));
                }
            }
        }
    }
    let _ = msa;
    out
}

/// `LoadCircumfixAffixProcessAllomorph`'s Lhs/environment split (HCLoader.cs:1276-1323): each conditioned half's inner (stem-adjacent) context becomes literal nodes next to its `PrefixNull`/`SuffixNull`, and only the outer contexts become one `AllomorphEnvironment`.
fn build_circumfix_lhs(
    prefix_env: Option<&(String, String)>,
    suffix_env: Option<&(String, String)>,
    ctx: &Ctx,
) -> Result<(Vec<PatternNode>, Vec<EnvironmentDef>), String> {
    let mut nodes = Vec::new();
    let mut left_env_pattern = None;
    let mut right_env_pattern = None;
    if prefix_env.is_none() && suffix_env.is_none() {
        nodes.extend(environment::any_plus(ctx));
    } else {
        if let Some((left_str, right_str)) = prefix_env {
            nodes.push(environment::prefix_null(ctx));
            nodes.extend(environment::pattern_nodes(right_str, ctx)?);
            if !left_str.is_empty() {
                left_env_pattern = environment::load_environment_pattern(left_str, true, ctx)?;
            }
        }
        nodes.extend(environment::any_star(ctx));
        if let Some((left_str, right_str)) = suffix_env {
            nodes.extend(environment::pattern_nodes(left_str, ctx)?);
            nodes.push(environment::suffix_null(ctx));
            if !right_str.is_empty() {
                right_env_pattern = environment::load_environment_pattern(right_str, false, ctx)?;
            }
        }
    }
    let mut environments = Vec::new();
    if left_env_pattern.is_some() || right_env_pattern.is_some() {
        environments.push(EnvironmentDef {
            require: true,
            left: left_env_pattern,
            right: right_env_pattern,
        });
    }
    Ok((nodes, environments))
}

/// Whether `form` is a reduplication/bracket-pattern affix shape rather than literal text -- shared by `is_valid_rule_form`'s rejection and by `collect_text_uses`'s substrate-usage collection, so the two classify the same shape identically.
pub(crate) fn is_bracket_pattern_form(form: &str) -> bool {
    form.contains('[')
}

/// Publishes literal text from every affix-shaped allomorph (concatenative, circumfix half, or an `MoAffixProcess`'s `RuleMapping::InsertSegments`); a bracket-pattern form publishes `conversion.unsupported-construct` instead.
pub(crate) fn collect_text_uses(
    snapshot: &Snapshot,
    recorder: &mut SelectionRecorder,
    issues: &mut Vec<ConversionIssue>,
) {
    let default_ws = snapshot
        .project
        .vernacular_writing_systems
        .first()
        .map(String::as_str);
    for entry in &snapshot.lexicon.entries {
        for allo in &entry.allomorphs {
            let source = SourceRef {
                kind: "allomorph".to_string(),
                id: allo.guid.clone(),
            };

            if let Some(process) = &allo.process {
                for step in &process.output {
                    if let RuleMapping::InsertSegments { text } = step {
                        let text = text.trim();
                        if !text.is_empty() {
                            recorder.record_text_use(source.clone(), text);
                        }
                    }
                }
                continue;
            }

            if allo.is_abstract {
                continue;
            }
            let Some(shape) = shape_of(allo.morph_type) else {
                continue;
            };
            if matches!(shape, Shape::Infix) && allo.positions.is_empty() {
                continue;
            }

            let form = super::best_ws(&allo.forms, default_ws).unwrap_or("");
            let form = super::format_form(form);
            if form.trim().is_empty() {
                continue;
            }
            if is_bracket_pattern_form(&form) {
                issues.push(ConversionIssue {
                    code: super::issues::UNSUPPORTED_CONSTRUCT.to_string(),
                    class: IssueClass::UnrepresentableForHc,
                    source: Some(source),
                    fatal: false,
                    message: format!(
                        "allomorph {:?}: reduplication/bracket-pattern affix form {form:?} is not \
                         literal text; substrate completion cannot check or infer from it",
                        allo.guid
                    ),
                });
                continue;
            }
            recorder.record_text_use(source, &form);
        }
    }
}

/// Simplified `IsValidRuleForm`: bracket-pattern (reduplication) forms are not implemented (warned, dropped) rather than gated on environment validity. Records the allomorph rejected only where this filter is the allomorph's one plausible route to a rule form (infix/prefix/suffix-shaped); a morph type that structurally can never be a rule form (bare stem/clitic/particle/phrase) is left considered-but-not-selected, mirroring a disabled compound rule rather than a failure.
fn is_valid_rule_form(allo: &Allomorph, ctx: &Ctx, warnings: &mut Vec<String>) -> bool {
    if let Some(process) = &allo.process {
        return process.input.len() > 1 || process.output.len() > 1;
    }
    if allo.is_abstract {
        return false;
    }
    let key = InventoryKey::object(InventoryKind::Allomorph, allo.guid.clone());
    match allo.morph_type {
        MorphType::Infix | MorphType::InfixingInterfix => {
            if allo.positions.is_empty() {
                // `selected` before `reject_quietly`, not once for the whole function: the catch-all arm below must stay unselected (see its own comment).
                ctx.selected(key.clone());
                ctx.reject_quietly(
                    key,
                    issue_codes::ALLOMORPH_NOT_RULE_FORM,
                    IssueClass::UnrepresentableForHc,
                    "infix allomorph has no position environment",
                );
                false
            } else {
                true
            }
        }
        // Proclitic/Enclitic count as rule forms unconditionally, under the same non-empty/non-abstract gate as prefix/suffix.
        MorphType::Prefix
        | MorphType::PrefixingInterfix
        | MorphType::Suffix
        | MorphType::SuffixingInterfix
        | MorphType::Proclitic
        | MorphType::Enclitic => {
            let form = super::best_ws(&allo.forms, None).unwrap_or("");
            if is_bracket_pattern_form(form) {
                ctx.selected(key.clone());
                ctx.reject(
                    warnings,
                    key,
                    issue_codes::ALLOMORPH_REDUPLICATION_UNSUPPORTED,
                    IssueClass::UnrepresentableForHc,
                    format!(
                        "unsupported: reduplication/bracket-pattern affix form {form:?} \
                         (allomorph {:?}) not implemented; allomorph skipped",
                        allo.guid
                    ),
                );
                return false;
            }
            if form.trim().is_empty() {
                ctx.selected(key.clone());
                ctx.reject_quietly(
                    key,
                    issue_codes::ALLOMORPH_NOT_RULE_FORM,
                    IssueClass::UnrepresentableForHc,
                    "affix allomorph has no non-empty form",
                );
                false
            } else {
                true
            }
        }
        // A circumfix/discontiguous-phrase allomorph is never a valid rule form on its own (a circumfix is built as a prefix/suffix-half cross-product instead); selected then rejected here since neither half-shape check above nor `lexicon.rs`'s stem/clitic bucket ever claims this guid.
        MorphType::Circumfix | MorphType::DiscontigPhrase => {
            ctx.selected(key.clone());
            ctx.reject_quietly(
                key,
                issue_codes::ALLOMORPH_MORPH_TYPE_UNSUPPORTED_AS_RULE_FORM,
                IssueClass::UnrepresentableForHc,
                "allomorph morph type is not a valid rule form on its own",
            );
            false
        }
        // Bare Clitic/Particle/Stem/Root/BoundRoot/BoundStem/Phrase are never rule forms for this filter, and no `selected`/`reject_quietly` here: `lexicon.rs`'s stem/clitic bucket owns this allomorph guid's selected/represented/rejected identity instead.
        _ => false,
    }
}

/// Builds every `AffixAllomorphDef` a single LCM allomorph expands to: one per valid environment (or a single environment-less pass) for a concatenative form, or exactly one for an `MoAffixProcess`. Returns a placeholder with `id` overwritten immediately by the caller.
fn build_affix_allomorphs_for(
    allo: &Allomorph,
    msa: &Msa,
    required_mpr: crate::model::MprSet,
    out_mpr: crate::model::MprSet,
    ctx: &Ctx,
    acc: &mut Acc,
    warnings: &mut Vec<String>,
) -> Vec<AffixAllomorphDef> {
    if let Some(process) = &allo.process {
        return match build_process_allomorph(allo, process, required_mpr, out_mpr, ctx, acc) {
            Ok(def) => vec![def],
            Err(e) => {
                ctx.reject(
                    warnings,
                    InventoryKey::object(InventoryKind::Allomorph, allo.guid.clone()),
                    issue_codes::ALLOMORPH_PROCESS_BUILD_FAILED,
                    IssueClass::UnrepresentableForHc,
                    format!("allomorph {:?}: {e}; skipped", allo.guid),
                );
                Vec::new()
            }
        };
    }

    let Some(shape) = shape_of(allo.morph_type) else {
        ctx.reject(
            warnings,
            InventoryKey::object(InventoryKind::Allomorph, allo.guid.clone()),
            issue_codes::ALLOMORPH_MORPH_TYPE_UNSUPPORTED,
            IssueClass::UnrepresentableForHc,
            format!(
                "unsupported: morph type of allomorph {:?} not implemented as an affix rule; skipped",
                allo.guid
            ),
        );
        return Vec::new();
    };

    let form = super::best_ws(&allo.forms, ctx.default_vernacular_ws.as_deref()).unwrap_or("");
    let form = super::format_form(form);

    let allo_infl_mpr = if matches!(msa, Msa::Inflectional { .. }) {
        let mut set = crate::model::MprSet::EMPTY;
        for ic in &allo.inflection_classes {
            match ctx.mpr.infl_class_with_descendants(ic) {
                Some(s) => set = set.union(s),
                None => warnings.push(format!(
                    "allomorph {:?}: inflection class {ic:?} does not resolve",
                    allo.guid
                )),
            }
        }
        set
    } else {
        crate::model::MprSet::EMPTY
    };

    let combined_env_guids: Vec<&str> = allo
        .environments
        .iter()
        .chain(&allo.positions)
        .map(String::as_str)
        .collect();

    let mut out = Vec::new();
    for pass in resolve_environments(&combined_env_guids, &allo.guid, ctx, warnings) {
        let (left_str, right_str) = pass.unwrap_or_default();
        match build_concatenative(&form, &left_str, &right_str, shape, ctx) {
            Ok((lhs, rhs, environments)) => {
                let required_syn_fs = match &allo.ms_env_features {
                    Some(fs) => match super::features::build_syn_fs(ctx.syn, None, Some(fs)) {
                        Ok(v) => acc.fs_interner.intern(v),
                        Err(e) => {
                            warnings.push(format!("allomorph {:?}: {e}; skipped", allo.guid));
                            continue;
                        }
                    },
                    None => acc.fs_interner.intern(pg_featstruct::FeatureStruct::EMPTY),
                };
                out.push(AffixAllomorphDef {
                    id: AllomorphId(0),
                    environments,
                    co_occurrence: Vec::new(),
                    required_syn_fs,
                    vars: crate::model::VarTable::default(),
                    required_mpr: required_mpr.union(allo_infl_mpr),
                    excluded_mpr: crate::model::MprSet::EMPTY,
                    out_mpr,
                    redup_hint: match allo.morph_type {
                        MorphType::Prefix => ReduplicationHint::Prefix,
                        MorphType::Suffix => ReduplicationHint::Suffix,
                        _ => ReduplicationHint::Implicit,
                    },
                    lhs,
                    rhs,
                    properties: Vec::new(),
                });
            }
            Err(e) => warnings.push(format!(
                "allomorph {:?}: {e}; one environment skipped",
                allo.guid
            )),
        }
    }
    // Selected as a rule form but every pass failed to build one: reject it rather than leave it silently unrepresented.
    if out.is_empty() {
        ctx.reject_quietly(
            InventoryKey::object(InventoryKind::Allomorph, allo.guid.clone()),
            issue_codes::ALLOMORPH_UNSEGMENTABLE,
            IssueClass::UnrepresentableForHc,
            "every environment pass failed to build a concatenative allomorph",
        );
    }
    out
}

/// LHS/RHS/environment triple a concatenative shape builds.
type ConcatBuild = (Vec<Pattern>, Vec<OutputAction>, Vec<EnvironmentDef>);

fn build_concatenative(
    form: &str,
    left_str: &str,
    right_str: &str,
    shape: Shape,
    ctx: &Ctx,
) -> Result<ConcatBuild, String> {
    match shape {
        Shape::Suffix => {
            let mut nodes = Vec::new();
            if left_str.is_empty() {
                nodes.extend(environment::any_plus(ctx));
            } else {
                if left_str.starts_with('#') {
                    nodes.push(environment::prefix_null(ctx));
                } else {
                    nodes.extend(environment::any_star(ctx));
                }
                nodes.extend(environment::pattern_nodes(left_str, ctx)?);
                nodes.push(environment::suffix_null(ctx));
            }
            let lhs = vec![Pattern { nodes }];
            let insert = format!("+{form}");
            let rhs = vec![
                OutputAction::Copy(PartRef::Input(0)),
                insert_segments(&insert, ctx)?,
            ];
            let mut environments = Vec::new();
            if !right_str.is_empty() {
                if let Some(p) = environment::load_environment_pattern(right_str, false, ctx)? {
                    environments.push(EnvironmentDef {
                        require: true,
                        left: None,
                        right: Some(p),
                    });
                }
            }
            Ok((lhs, rhs, environments))
        }
        Shape::Prefix => {
            let mut nodes = Vec::new();
            if right_str.is_empty() {
                nodes.extend(environment::any_plus(ctx));
            } else {
                nodes.push(environment::prefix_null(ctx));
                nodes.extend(environment::pattern_nodes(right_str, ctx)?);
                if right_str.ends_with('#') {
                    nodes.push(environment::suffix_null(ctx));
                } else {
                    nodes.extend(environment::any_star(ctx));
                }
            }
            let lhs = vec![Pattern { nodes }];
            let insert = format!("{form}+");
            let rhs = vec![
                insert_segments(&insert, ctx)?,
                OutputAction::Copy(PartRef::Input(0)),
            ];
            let mut environments = Vec::new();
            if !left_str.is_empty() {
                if let Some(p) = environment::load_environment_pattern(left_str, true, ctx)? {
                    environments.push(EnvironmentDef {
                        require: true,
                        left: Some(p),
                        right: None,
                    });
                }
            }
            Ok((lhs, rhs, environments))
        }
        Shape::Infix => {
            let mut left_nodes = if left_str.starts_with('#') {
                vec![environment::prefix_null(ctx)]
            } else {
                environment::any_star(ctx)
            };
            left_nodes.extend(environment::pattern_nodes(left_str, ctx)?);
            let mut right_nodes = environment::pattern_nodes(right_str, ctx)?;
            if right_str.ends_with('#') {
                right_nodes.push(environment::suffix_null(ctx));
            } else {
                right_nodes.extend(environment::any_star(ctx));
            }
            let lhs = vec![
                Pattern { nodes: left_nodes },
                Pattern { nodes: right_nodes },
            ];
            let insert = format!("+{form}+");
            let rhs = vec![
                OutputAction::Copy(PartRef::Input(0)),
                insert_segments(&insert, ctx)?,
                OutputAction::Copy(PartRef::Input(1)),
            ];
            Ok((lhs, rhs, Vec::new()))
        }
    }
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

/// Resolves each environment guid to its split `(left, right)` context strings, yielding one `None` pass whenever the guid list was empty or an entry failed to resolve/parse.
fn resolve_environments(
    guids: &[&str],
    allo_guid: &str,
    ctx: &Ctx,
    warnings: &mut Vec<String>,
) -> Vec<Option<(String, String)>> {
    let mut out = Vec::new();
    let mut has_blank = guids.is_empty();
    for g in guids {
        let attachment = InventoryKey::attachment(
            InventoryKind::Environment,
            allo_guid.to_string(),
            g.to_string(),
            roles::ENVIRONMENT,
        );
        ctx.authored(attachment.clone());
        ctx.considered(attachment.clone());
        let Some(env) = ctx.env_by_guid.get(g) else {
            ctx.selected(attachment.clone());
            ctx.reject(
                warnings,
                attachment,
                issue_codes::ENVIRONMENT_UNRESOLVED,
                IssueClass::InvalidSource,
                format!("environment {g:?} does not resolve; treated as absent"),
            );
            has_blank = true;
            continue;
        };
        ctx.selected(attachment.clone());
        let env_object = InventoryKey::object(InventoryKind::Environment, env.guid.clone());
        ctx.considered(env_object.clone());
        ctx.selected(env_object.clone());
        // A failing environment is invalid as a whole and lands in the same blank-fallback bucket as a malformed split, rather than being discovered later.
        if let Err(e) = environment::validate_environment(&env.representation, ctx) {
            ctx.reject(
                warnings,
                attachment,
                issue_codes::ENVIRONMENT_INVALID,
                IssueClass::InvalidSource,
                format!(
                    "invalid environment {:?} ({}): {e}; treated as absent",
                    env.guid, env.representation
                ),
            );
            ctx.reject_quietly(
                env_object,
                issue_codes::ENVIRONMENT_INVALID,
                IssueClass::InvalidSource,
                "environment representation failed validation",
            );
            has_blank = true;
            continue;
        }
        match environment::split_environment_string(&env.representation) {
            Ok(pair) => {
                out.push(Some(pair));
                ctx.represented(attachment);
                ctx.represented(env_object);
            }
            Err(e) => {
                ctx.reject(
                    warnings,
                    attachment,
                    issue_codes::ENVIRONMENT_INVALID,
                    IssueClass::InvalidSource,
                    format!(
                        "invalid environment {:?} ({}): {e}; treated as absent",
                        env.guid, env.representation
                    ),
                );
                ctx.reject_quietly(
                    env_object,
                    issue_codes::ENVIRONMENT_INVALID,
                    IssueClass::InvalidSource,
                    "environment representation failed to split",
                );
                has_blank = true;
            }
        }
    }
    if has_blank {
        out.push(None);
    }
    out
}

/// A direct transcription of the snapshot's `AffixProcess.input`/`.output`; no environment cross-product (a process allomorph carries no phone-environment/position data in LCM).
fn build_process_allomorph(
    allo: &Allomorph,
    process: &pg_snapshot::lexicon::AffixProcess,
    required_mpr: crate::model::MprSet,
    out_mpr: crate::model::MprSet,
    ctx: &Ctx,
    acc: &mut Acc,
) -> Result<AffixAllomorphDef, String> {
    let mut lhs = Vec::with_capacity(process.input.len());
    for part in &process.input {
        let nodes = match part {
            PhonContext::Variable => environment::any_star(ctx),
            other => phon_context_nodes(other, ctx)?,
        };
        lhs.push(Pattern { nodes });
    }

    let mut rhs = Vec::with_capacity(process.output.len());
    for step in &process.output {
        rhs.push(match step {
            RuleMapping::InsertNaturalClass { natural_class } => {
                let nc = ctx
                    .natclass_by_guid
                    .get(natural_class)
                    .copied()
                    .ok_or_else(|| format!("unknown natural class {natural_class:?}"))?;
                OutputAction::InsertContext(SimpleContext {
                    nat_class: nc,
                    vars: Vec::new(),
                })
            }
            RuleMapping::CopyFromInput { part } => {
                if *part == 0 || *part as usize > process.input.len() {
                    return Err(format!("CopyFromInput part {part} out of range"));
                }
                OutputAction::Copy(PartRef::Input((*part - 1) as u16))
            }
            RuleMapping::InsertSegments { text } => insert_segments(text.trim(), ctx)?,
            RuleMapping::ModifyFromInput {
                part,
                natural_class,
            } => {
                if *part == 0 || *part as usize > process.input.len() {
                    return Err(format!("ModifyFromInput part {part} out of range"));
                }
                let nc = ctx
                    .natclass_by_guid
                    .get(natural_class)
                    .copied()
                    .ok_or_else(|| format!("unknown natural class {natural_class:?}"))?;
                OutputAction::Modify(
                    PartRef::Input((*part - 1) as u16),
                    SimpleContext {
                        nat_class: nc,
                        vars: Vec::new(),
                    },
                )
            }
        });
    }

    let redup_hint = match allo.morph_type {
        MorphType::Prefix => ReduplicationHint::Prefix,
        MorphType::Suffix => ReduplicationHint::Suffix,
        _ => ReduplicationHint::Implicit,
    };

    Ok(AffixAllomorphDef {
        id: AllomorphId(0),
        environments: Vec::new(),
        co_occurrence: Vec::new(),
        required_syn_fs: acc.fs_interner.intern(pg_featstruct::FeatureStruct::EMPTY),
        vars: crate::model::VarTable::default(),
        required_mpr,
        excluded_mpr: crate::model::MprSet::EMPTY,
        out_mpr,
        redup_hint,
        lhs,
        rhs,
        properties: Vec::new(),
    })
}

/// Recursive dispatch over the phonological-context tree shape shared by rewrite rules and `MoAffixProcess` input parts.
pub(crate) fn phon_context_nodes(
    pc: &PhonContext,
    ctx: &Ctx,
) -> Result<Vec<crate::model::PatternNode>, String> {
    use crate::model::PatternNode;
    match pc {
        PhonContext::Sequence { members } => {
            let mut out = Vec::new();
            for m in members {
                out.extend(phon_context_nodes(m, ctx)?);
            }
            Ok(out)
        }
        PhonContext::Iteration { min, max, member } => {
            let children = phon_context_nodes(member, ctx)?;
            Ok(vec![PatternNode::Quantifier {
                min: (*min).max(0) as u32,
                max: if *max < 0 { None } else { Some(*max as u32) },
                children,
            }])
        }
        PhonContext::Segment { phoneme } => {
            let cd = ctx
                .phoneme_of
                .get(phoneme)
                .copied()
                .ok_or_else(|| format!("unknown phoneme {phoneme:?}"))?;
            Ok(vec![PatternNode::CharDef(cd)])
        }
        PhonContext::NaturalClass {
            natural_class,
            plus_variables,
            minus_variables,
        } => {
            if !plus_variables.is_empty() || !minus_variables.is_empty() {
                return Err(
                    "alpha-variable natural-class constraints outside a rewrite rule are not \
                     supported"
                        .to_string(),
                );
            }
            let nc = ctx
                .natclass_by_guid
                .get(natural_class)
                .copied()
                .ok_or_else(|| format!("unknown natural class {natural_class:?}"))?;
            Ok(vec![PatternNode::Context(SimpleContext {
                nat_class: nc,
                vars: Vec::new(),
            })])
        }
        PhonContext::Boundary { marker } => {
            let cd = ctx
                .boundary_of
                .get(marker)
                .copied()
                .ok_or_else(|| format!("unknown boundary marker {marker:?}"))?;
            Ok(vec![PatternNode::CharDef(cd)])
        }
        PhonContext::WordBoundary => Ok(Vec::new()),
        PhonContext::Variable => Ok(environment::any_star(ctx)),
    }
}
