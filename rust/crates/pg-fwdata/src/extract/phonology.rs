//! `phonology` snapshot section — see `docs/snapshot-format.md` §4.

use pg_snapshot::{
    BoundaryMarker, Environment, FeatureConstraint, FeatureSystems, MetathesisRule, NaturalClass,
    PhonContext, Phoneme, PhonologicalRule, Phonology, RewriteRhs, RewriteRule, RuleDirection,
};

use super::features::extract_feature_structure;
use super::Ctx;
use crate::node::strip_dotted_circles;
use crate::xml::Record;

pub fn extract_phonology(
    ctx: &mut Ctx,
    lang_project: Option<&Record>,
    feature_systems: &FeatureSystems,
) -> Phonology {
    let _ = feature_systems; // feature guids are resolved lazily by `extract_feature_structure`.
    let Some(lang_project) = lang_project else {
        return Phonology::default();
    };
    let Some(phon_data_guid) = lang_project.node.objsur_one("PhonologicalData") else {
        return Phonology::default();
    };
    let Some(phon_data) = ctx.require(&phon_data_guid, "PhPhonData", "phonology") else {
        return Phonology::default();
    };

    let (phonemes, boundary_markers) = extract_phoneme_set(ctx, phon_data);
    let natural_classes = extract_natural_classes(ctx, phon_data);
    let environments = extract_environments(ctx, phon_data);
    let feature_constraints = extract_feature_constraints(ctx, phon_data);
    let rules = extract_rules(ctx, phon_data);

    Phonology {
        phonemes,
        boundary_markers,
        natural_classes,
        environments,
        rules,
        feature_constraints,
    }
}

/// `HCLoader` only ever loads the first phoneme set (HCLoader.cs:204); this does the same, warning if there is more than one.
fn extract_phoneme_set(ctx: &mut Ctx, phon_data: &Record) -> (Vec<Phoneme>, Vec<BoundaryMarker>) {
    let set_guids = phon_data.node.objsur_list("PhonemeSets");
    if set_guids.len() > 1 {
        ctx.warn(
            super::codes::ONLY_FIRST_USED,
            format!(
                "phonology: {} phoneme sets present; only the first is used (matches HCLoader)",
                set_guids.len()
            ),
        );
    }
    let Some(set_guid) = set_guids.first() else {
        return (Vec::new(), Vec::new());
    };
    let Some(set) = ctx.require(set_guid, "PhPhonemeSet", "phonology.phonemes") else {
        return (Vec::new(), Vec::new());
    };

    let phonemes = set
        .node
        .objsur_list("Phonemes")
        .into_iter()
        .filter_map(|g| extract_phoneme(ctx, &g))
        .collect();
    let boundary_markers = set
        .node
        .objsur_list("BoundaryMarkers")
        .into_iter()
        .filter_map(|g| extract_boundary_marker(ctx, &g))
        .collect();
    (phonemes, boundary_markers)
}

fn extract_phoneme(ctx: &mut Ctx, guid: &str) -> Option<Phoneme> {
    let rec = ctx.require(guid, "PhPhoneme", "phonology.phonemes")?;
    let name = ctx.best_analysis(&rec.node.ws_forms("Name"));
    let representations = code_representations(ctx, rec, "phonology.phonemes");
    if representations.is_empty() {
        ctx.warn(
            super::codes::EMPTY_REPRESENTATION,
            format!(
                "phonology.phonemes: phoneme {guid} ({name:?}) has no representations after \
                 dotted-circle stripping"
            ),
        );
    }
    let features = rec
        .node
        .objsur_one("Features")
        .and_then(|fs_guid| extract_feature_structure(ctx, &fs_guid, "phonology.phonemes"))
        .filter(|fs| !fs.values.is_empty());
    Some(Phoneme {
        guid: guid.to_string(),
        name,
        representations,
        features,
        basic_ipa_symbol: rec.node.child("BasicIPASymbol").map(|c| c.text.clone()),
    })
}

fn extract_boundary_marker(ctx: &mut Ctx, guid: &str) -> Option<BoundaryMarker> {
    let rec = ctx.require(guid, "PhBdryMarker", "phonology.boundaryMarkers")?;
    Some(BoundaryMarker {
        guid: guid.to_string(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        representations: code_representations(ctx, rec, "phonology.boundaryMarkers"),
    })
}

/// `PhPhoneme.CodesOS`/`PhBdryMarker.CodesOS`: flattens every code's forms, dotted-circle stripped.
fn code_representations(ctx: &mut Ctx, rec: &Record, label: &str) -> Vec<pg_snapshot::WsForm> {
    let mut out = Vec::new();
    for code_guid in rec.node.objsur_list("Codes") {
        let Some(code) = ctx.get(&code_guid) else {
            ctx.warn(
                super::codes::DANGLING_REFERENCE,
                format!("{label}: dangling PhCode reference {code_guid}"),
            );
            continue;
        };
        for form in code.node.ws_forms("Representation") {
            out.push(pg_snapshot::WsForm {
                ws: form.ws,
                form: strip_dotted_circles(&form.form),
            });
        }
    }
    out
}

/// The first `PhCode`'s representation for a phoneme/boundary-marker guid.
/// See `docs/research/pg-fwdata-phonology-extract-notes.md`.
pub(crate) fn first_code_representation(ctx: &mut Ctx, guid: &str) -> Option<String> {
    let rec = ctx.get(guid)?;
    let first_code_guid = rec.node.objsur_list("Codes").into_iter().next()?;
    let code = ctx.get(&first_code_guid)?;
    let forms = code.node.ws_forms("Representation");
    let forms: Vec<_> = forms
        .into_iter()
        .map(|f| pg_snapshot::WsForm {
            ws: f.ws,
            form: strip_dotted_circles(&f.form),
        })
        .collect();
    let text = if rec.class == "PhBdryMarker" {
        ctx.best_vernacular(&forms)
    } else {
        // "vernacular default" is simply the first (default) vernacular writing system.
        ctx.vernacular_ws
            .first()
            .and_then(|ws| forms.iter().find(|f| &f.ws == ws))
            .map(|f| f.form.clone())
            .unwrap_or_else(|| ctx.best_vernacular(&forms))
    };
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn extract_natural_classes(ctx: &mut Ctx, phon_data: &Record) -> Vec<NaturalClass> {
    phon_data
        .node
        .objsur_list("NaturalClasses")
        .into_iter()
        .filter_map(|guid| extract_natural_class(ctx, &guid))
        .collect()
}

fn extract_natural_class(ctx: &mut Ctx, guid: &str) -> Option<NaturalClass> {
    let rec = ctx.get(guid)?;
    let name = ctx.best_analysis(&rec.node.ws_forms("Abbreviation"));
    match rec.class.as_str() {
        "PhNCSegments" => {
            let phonemes = rec.node.objsur_list("Segments");
            Some(NaturalClass::Segments {
                guid: guid.to_string(),
                name,
                phonemes,
            })
        }
        "PhNCFeatures" => {
            let features = rec
                .node
                .objsur_one("Features")
                .and_then(|fs| extract_feature_structure(ctx, &fs, "phonology.naturalClasses"))
                .unwrap_or_default();
            Some(NaturalClass::Features {
                guid: guid.to_string(),
                name,
                features,
            })
        }
        other => {
            ctx.warn(
                super::codes::UNEXPECTED_CLASS,
                format!("phonology.naturalClasses: {guid} has unexpected class {other}"),
            );
            None
        }
    }
}

fn extract_environments(ctx: &mut Ctx, phon_data: &Record) -> Vec<Environment> {
    phon_data
        .node
        .objsur_list("Environments")
        .into_iter()
        .filter_map(|guid| extract_environment(ctx, &guid))
        .collect()
}

fn extract_environment(ctx: &mut Ctx, guid: &str) -> Option<Environment> {
    let rec = ctx.require(guid, "PhEnvironment", "phonology.environments")?;
    Some(Environment {
        guid: guid.to_string(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        representation: rec
            .node
            .str_text("StringRepresentation")
            .unwrap_or_default(),
    })
}

fn extract_feature_constraints(ctx: &mut Ctx, phon_data: &Record) -> Vec<FeatureConstraint> {
    phon_data
        .node
        .objsur_list("FeatConstraints")
        .into_iter()
        .filter_map(|guid| {
            let rec = ctx.require(&guid, "PhFeatureConstraint", "phonology.featureConstraints")?;
            let feature = rec.node.objsur_one("Feature")?;
            Some(FeatureConstraint {
                guid: guid.clone(),
                feature,
            })
        })
        .collect()
}

fn extract_rules(ctx: &mut Ctx, phon_data: &Record) -> Vec<PhonologicalRule> {
    phon_data
        .node
        .objsur_list("PhonRules")
        .into_iter()
        .filter_map(|guid| {
            let rec = ctx.get(&guid)?;
            if rec.node.val_bool("Disabled").unwrap_or(false) {
                return None;
            }
            match rec.class.as_str() {
                "PhRegularRule" => extract_rewrite_rule(ctx, rec).map(PhonologicalRule::Rewrite),
                "PhMetathesisRule" => {
                    extract_metathesis_rule(ctx, rec).map(PhonologicalRule::Metathesis)
                }
                other => {
                    ctx.warn(
                        super::codes::UNEXPECTED_CLASS,
                        format!("phonology.rules: {guid} has unexpected class {other}"),
                    );
                    None
                }
            }
        })
        .collect()
}

fn rule_direction(rec: &Record, ctx: &mut Ctx, label: &str) -> RuleDirection {
    match rec.node.val_int("Direction") {
        Some(0) => RuleDirection::LeftToRight,
        Some(1) => RuleDirection::RightToLeft,
        Some(2) => RuleDirection::Simultaneous,
        other => {
            ctx.warn(
                super::codes::UNRECOGNIZED_ENUM_VALUE,
                format!(
                    "{label}: unexpected Direction {other:?} on rule {}, defaulting to leftToRight",
                    rec.guid
                ),
            );
            RuleDirection::LeftToRight
        }
    }
}

fn extract_rewrite_rule(ctx: &mut Ctx, rec: &Record) -> Option<RewriteRule> {
    let label = "phonology.rules(rewrite)";
    let direction = rule_direction(rec, ctx, label);
    let structural_description: Vec<PhonContext> = rec
        .node
        .objsur_list("StrucDesc")
        .into_iter()
        .filter_map(|g| resolve_phon_context(ctx, &g, label))
        .collect();
    let right_hand_sides: Vec<RewriteRhs> = rec
        .node
        .objsur_list("RightHandSides")
        .into_iter()
        .filter_map(|g| extract_rewrite_rhs(ctx, &g))
        .collect();
    // `PhRegularRule.FeatureConstraints` is a virtual LCM property; recomputed here to match HCLoader's own collection order.
    // See `docs/research/pg-fwdata-phonology-extract-notes.md`.
    let mut feature_constraint_variables: Vec<String> = Vec::new();
    for c in &structural_description {
        collect_feature_constraint_vars(c, &mut feature_constraint_variables);
    }
    for rhs in &right_hand_sides {
        for c in &rhs.structural_change {
            collect_feature_constraint_vars(c, &mut feature_constraint_variables);
        }
        if let Some(c) = &rhs.left_context {
            collect_feature_constraint_vars(c, &mut feature_constraint_variables);
        }
        if let Some(c) = &rhs.right_context {
            collect_feature_constraint_vars(c, &mut feature_constraint_variables);
        }
    }
    Some(RewriteRule {
        guid: rec.guid.clone(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        direction,
        structural_description,
        feature_constraint_variables,
        right_hand_sides,
    })
}

/// The recursive walk `PhRegularRule.CollectVars` does; first occurrence wins (deduplicated).
fn collect_feature_constraint_vars(c: &PhonContext, out: &mut Vec<String>) {
    match c {
        PhonContext::Sequence { members } => {
            for m in members {
                collect_feature_constraint_vars(m, out);
            }
        }
        PhonContext::Iteration { member, .. } => collect_feature_constraint_vars(member, out),
        PhonContext::NaturalClass {
            plus_variables,
            minus_variables,
            ..
        } => {
            for g in plus_variables.iter().chain(minus_variables) {
                if !out.contains(g) {
                    out.push(g.clone());
                }
            }
        }
        _ => {}
    }
}

fn extract_rewrite_rhs(ctx: &mut Ctx, guid: &str) -> Option<RewriteRhs> {
    let label = "phonology.rules(rewrite).rightHandSides";
    let rec = ctx.require(guid, "PhSegRuleRHS", label)?;
    let structural_change = rec
        .node
        .objsur_list("StrucChange")
        .into_iter()
        .filter_map(|g| resolve_phon_context(ctx, &g, label))
        .collect();
    let left_context = rec
        .node
        .objsur_one("LeftContext")
        .and_then(|g| resolve_phon_context(ctx, &g, label));
    let right_context = rec
        .node
        .objsur_one("RightContext")
        .and_then(|g| resolve_phon_context(ctx, &g, label));
    let required_parts_of_speech = rec.node.objsur_list("InputPOSes");
    let required_rule_features = resolve_rule_features(ctx, rec, "ReqRuleFeats", label);
    let excluded_rule_features = resolve_rule_features(ctx, rec, "ExclRuleFeats", label);
    Some(RewriteRhs {
        structural_change,
        left_context,
        right_context,
        required_parts_of_speech,
        required_rule_features,
        excluded_rule_features,
    })
}

/// `ReqRuleFeats`/`ExclRuleFeats` are `PhPhonRuleFeat` wrapper guids; the wanted guid is each wrapper's own `Item`.
fn resolve_rule_features(ctx: &mut Ctx, rec: &Record, field: &str, label: &str) -> Vec<String> {
    rec.node
        .objsur_list(field)
        .into_iter()
        .filter_map(|wrapper_guid| {
            let wrapper = ctx.require(&wrapper_guid, "PhPhonRuleFeat", label)?;
            let item = wrapper.node.objsur_one("Item");
            if item.is_none() {
                ctx.warn(
                    super::codes::MISSING_REQUIRED_FIELD,
                    format!("{label}: PhPhonRuleFeat {wrapper_guid} has no Item reference"),
                );
            }
            item
        })
        .collect()
}

/// Model gap: the LCM schema has no switch-index integer fields, only a `StrucChange` string this parses into an approximate two-element swap.
/// See `docs/research/pg-fwdata-phonology-extract-notes.md`.
fn extract_metathesis_rule(ctx: &mut Ctx, rec: &Record) -> Option<MetathesisRule> {
    let label = "phonology.rules(metathesis)";
    let direction = rule_direction(rec, ctx, label);
    let structural_description: Vec<_> = rec
        .node
        .objsur_list("StrucDesc")
        .into_iter()
        .filter_map(|g| resolve_phon_context(ctx, &g, label))
        .collect();

    let struc_change_text = rec
        .node
        .child("StrucChange")
        .map(|c| c.text.clone())
        .unwrap_or_default();
    let permutation: Vec<usize> = struc_change_text
        .split_whitespace()
        .filter_map(|tok| tok.parse::<usize>().ok())
        .collect();
    if permutation.len() != structural_description.len() {
        ctx.warn(
            super::codes::METATHESIS_APPROXIMATION,
            format!(
                "{label}: rule {} StrucChange {:?} does not enumerate all {} structural-description \
                 positions; switch indices may be wrong",
                rec.guid,
                struc_change_text,
                structural_description.len()
            ),
        );
    }
    let differing: Vec<usize> = permutation
        .iter()
        .enumerate()
        .filter(|(i, &v)| v != i + 1)
        .map(|(i, _)| i)
        .collect();
    if !differing.is_empty() && differing.len() != 2 {
        ctx.warn(
            super::codes::METATHESIS_APPROXIMATION,
            format!(
                "{label}: rule {} has a StrucChange permutation more complex than a simple two-part \
                 swap ({:?}); left/right switch indices are an approximation",
                rec.guid, struc_change_text
            ),
        );
    }
    let left_switch_index = differing.first().copied().unwrap_or(0) as i32;
    let right_switch_index = differing.last().copied().unwrap_or(0) as i32;

    Some(MetathesisRule {
        guid: rec.guid.clone(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        direction,
        structural_description,
        left_switch_index,
        right_switch_index,
    })
}

/// Resolve a `PhContextOrVar` guid into a `PhonContext` tree, shared by phonological rules and `extract::lexicon`.
/// See `docs/research/pg-fwdata-phonology-extract-notes.md`.
pub(crate) fn resolve_phon_context(ctx: &mut Ctx, guid: &str, label: &str) -> Option<PhonContext> {
    let rec = ctx.get(guid)?;
    match rec.class.as_str() {
        "PhSequenceContext" => {
            let members = rec
                .node
                .objsur_list("Members")
                .into_iter()
                .filter_map(|g| resolve_phon_context(ctx, &g, label))
                .collect();
            Some(PhonContext::Sequence { members })
        }
        "PhIterationContext" => {
            let min = rec.node.val_int("Minimum").unwrap_or(0) as i32;
            let max = rec.node.val_int("Maximum").unwrap_or(-1) as i32;
            let member_guid = rec.node.objsur_one("Member")?;
            let member = resolve_phon_context(ctx, &member_guid, label)?;
            Some(PhonContext::Iteration {
                min,
                max,
                member: Box::new(member),
            })
        }
        "PhSimpleContextSeg" => {
            let phoneme = rec.node.objsur_one("FeatureStructure")?;
            Some(PhonContext::Segment { phoneme })
        }
        "PhSimpleContextNC" => {
            let natural_class = rec.node.objsur_one("FeatureStructure")?;
            let plus_variables = rec.node.objsur_list("PlusConstr");
            let minus_variables = rec.node.objsur_list("MinusConstr");
            Some(PhonContext::NaturalClass {
                natural_class,
                plus_variables,
                minus_variables,
            })
        }
        "PhSimpleContextBdry" => {
            let marker = rec.node.objsur_one("FeatureStructure")?;
            // The well-known word-boundary marker never appears as its own `PhBdryMarker` record, so failing to resolve one is the `#` anchor's own signature.
            // See `docs/research/pg-fwdata-phonology-extract-notes.md`.
            match ctx.get(&marker) {
                Some(m) if m.class == "PhBdryMarker" => Some(PhonContext::Boundary { marker }),
                _ => Some(PhonContext::WordBoundary),
            }
        }
        "PhVariable" => Some(PhonContext::Variable),
        other => {
            ctx.warn(
                super::codes::UNEXPECTED_CLASS,
                format!("{label}: {guid} has unexpected PhContextOrVar class {other}"),
            );
            None
        }
    }
}
