//! Builds one `AffixProcessRuleDef` per (entry, MSA) pair from concatenative and `MoAffixProcess`-style allomorphs; circumfix cross-products are not implemented (warned, no rule).

use pg_snapshot::lexicon::{Allomorph, LexEntry, Msa, RuleMapping};
use pg_snapshot::morphology::MorphType;
use pg_snapshot::phonology::PhonContext;

use crate::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, AllomorphOwner, EnvironmentDef, MRuleId,
    MorphRuleDef, MorphemeId, MorphemeInfo, OutputAction, PartRef, Pattern, ReduplicationHint,
    SimpleContext, StratumId,
};

use super::environment;
use super::{Acc, Ctx};

/// Which concatenative shape an affix morph type implies; `None` for a type this compiler does not build a rule for (circumfix, bare clitic/particle, phrase-shaped).
#[derive(Copy, Clone)]
enum Shape {
    Prefix,
    Suffix,
    Infix,
}

fn shape_of(mt: MorphType) -> Option<Shape> {
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
    let rule_form_allos: Vec<&Allomorph> = allos
        .iter()
        .copied()
        .filter(|a| is_valid_rule_form(a, warnings))
        .collect();
    if rule_form_allos.is_empty() {
        return None;
    }

    if entry.lexeme_morph_type == MorphType::Circumfix {
        warnings.push(format!(
            "unsupported: circumfix cross-product allomorphs (entry {:?}) not implemented; \
             entry skipped",
            entry.guid
        ));
        return None;
    }

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
                    warnings.push(format!("MSA {guid:?}: {e}; skipped"));
                    return None;
                }
            };
            let out_pos = to_part_of_speech
                .as_deref()
                .and_then(|p| ctx.pos.bits_single(p));
            let out = match super::features::build_syn_fs(ctx.syn, out_pos, to_features.as_ref()) {
                Ok(fs) => acc.fs_interner.intern(fs),
                Err(e) => {
                    warnings.push(format!("MSA {guid:?}: {e}; skipped"));
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
                    warnings.push(format!("MSA {guid:?}: {e}; skipped"));
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
                    warnings.push(format!("MSA {guid:?}: {e}; skipped"));
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
                    warnings.push(format!("MSA {guid:?}: {e}; skipped"));
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
                if let Some(s) = ctx.mpr.exception_feature(f) {
                    set = set.union(s);
                } else {
                    warnings.push(format!(
                        "MSA {msa_guid:?}: exception feature {f:?} does not resolve"
                    ));
                }
            }
            if let Some(ic) = from_inflection_class {
                match ctx.mpr.infl_class_with_descendants(ic) {
                    Some(s) => set = set.union(s),
                    None => warnings.push(format!(
                        "MSA {msa_guid:?}: inflection class {ic:?} does not resolve"
                    )),
                }
            }
            set
        }
        Msa::Inflectional {
            exception_features, ..
        } => {
            let mut set = crate::model::MprSet::EMPTY;
            for f in exception_features {
                if let Some(s) = ctx.mpr.exception_feature(f) {
                    set = set.union(s);
                } else {
                    warnings.push(format!(
                        "MSA {msa_guid:?}: exception feature {f:?} does not resolve"
                    ));
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
                if let Some(s) = ctx.mpr.exception_feature(f) {
                    set = set.union(s);
                } else {
                    warnings.push(format!(
                        "MSA {msa_guid:?}: exception feature {f:?} does not resolve"
                    ));
                }
            }
            if let Some(ic) = to_inflection_class {
                match ctx.mpr.infl_class_single(ic) {
                    Some(s) => set = set.union(s),
                    None => warnings.push(format!(
                        "MSA {msa_guid:?}: inflection class {ic:?} does not resolve"
                    )),
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
        } => match ctx.stem_name_by_guid.get(sn) {
            Some(&id) => Some(id),
            None => {
                warnings.push(format!(
                    "MSA {msa_guid:?}: stem name {sn:?} does not resolve"
                ));
                None
            }
        },
        _ => None,
    };

    let mut allomorphs = Vec::new();
    for allo in rule_form_allos {
        for def in build_affix_allomorphs_for(allo, msa, required_mpr, out_mpr, ctx, acc, warnings)
        {
            let allo_id = AllomorphId(acc.allomorph_owners.len() as u32);
            acc.allomorph_owners
                .push(AllomorphOwner::Affix(mrule_id, allomorphs.len() as u16));
            acc.allomorph_guid_index.insert(allo.guid.clone(), allo_id);
            allomorphs.push(AffixAllomorphDef { id: allo_id, ..def });
        }
    }
    if allomorphs.is_empty() {
        return None;
    }

    let morpheme = MorphemeId(acc.morphemes.len() as u32);
    acc.morphemes.push(MorphemeInfo {
        xml_key: msa_guid,
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

    Some(mrule_id)
}

/// Simplified `IsValidRuleForm`: bracket-pattern (reduplication) forms are not implemented (warned, dropped) rather than gated on environment validity.
fn is_valid_rule_form(allo: &Allomorph, warnings: &mut Vec<String>) -> bool {
    if let Some(process) = &allo.process {
        return process.input.len() > 1 || process.output.len() > 1;
    }
    if allo.is_abstract {
        return false;
    }
    match allo.morph_type {
        MorphType::Infix | MorphType::InfixingInterfix => !allo.positions.is_empty(),
        // Proclitic/Enclitic count as rule forms unconditionally, under the same non-empty/non-abstract gate as prefix/suffix.
        MorphType::Prefix
        | MorphType::PrefixingInterfix
        | MorphType::Suffix
        | MorphType::SuffixingInterfix
        | MorphType::Proclitic
        | MorphType::Enclitic => {
            let form = super::best_ws(&allo.forms, None).unwrap_or("");
            if form.contains('[') {
                warnings.push(format!(
                    "unsupported: reduplication/bracket-pattern affix form {form:?} (allomorph \
                     {:?}) not implemented; allomorph skipped",
                    allo.guid
                ));
                return false;
            }
            !form.trim().is_empty()
        }
        // Bare Clitic/Particle are stem forms (clitic-stratum lex entries), never rule forms; not a warning since they're handled on the stem path.
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
                warnings.push(format!("allomorph {:?}: {e}; skipped", allo.guid));
                Vec::new()
            }
        };
    }

    let Some(shape) = shape_of(allo.morph_type) else {
        warnings.push(format!(
            "unsupported: morph type of allomorph {:?} not implemented as an affix rule; skipped",
            allo.guid
        ));
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
    for pass in resolve_environments(&combined_env_guids, ctx, warnings) {
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
    ctx: &Ctx,
    warnings: &mut Vec<String>,
) -> Vec<Option<(String, String)>> {
    let mut out = Vec::new();
    let mut has_blank = guids.is_empty();
    for g in guids {
        let Some(env) = ctx.env_by_guid.get(g) else {
            warnings.push(format!(
                "environment {g:?} does not resolve; treated as absent"
            ));
            has_blank = true;
            continue;
        };
        // A failing environment is invalid as a whole and lands in the same blank-fallback bucket as a malformed split, rather than being discovered later.
        if let Err(e) = environment::validate_environment(&env.representation, ctx) {
            warnings.push(format!(
                "invalid environment {:?} ({}): {e}; treated as absent",
                env.guid, env.representation
            ));
            has_blank = true;
            continue;
        }
        match environment::split_environment_string(&env.representation) {
            Ok(pair) => out.push(Some(pair)),
            Err(e) => {
                warnings.push(format!(
                    "invalid environment {:?} ({}): {e}; treated as absent",
                    env.guid, env.representation
                ));
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
