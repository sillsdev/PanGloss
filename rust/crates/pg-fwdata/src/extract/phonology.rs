//! `phonology` snapshot section — see `docs/snapshot-format.md` §4.

use pg_snapshot::{
    BoundaryMarker, Environment, FeatureConstraint, FeatureSystems, FwClass, InventoryKey,
    InventoryKind, IssueClass, MetathesisRule, NaturalClass, PhonContext, Phoneme,
    PhonologicalRule, Phonology, RewriteRhs, RewriteRule, RuleDirection, Warning,
};

use super::features::extract_feature_structure;
use super::Ctx;
use crate::node::strip_dotted_circles;
use crate::xml::Record;

const WORD_BOUNDARY_MARKER_GUID: &str = "7db635e0-9ef3-4167-a594-12551ed89aaa";

pub fn extract_phonology(
    ctx: &mut Ctx,
    lang_project: Option<&Record>,
    feature_systems: &FeatureSystems,
    project_name: &str,
) -> Phonology {
    let _ = feature_systems; // feature guids are resolved lazily by `extract_feature_structure`.
    let Some(lang_project) = lang_project else {
        record_unreferenced_environments(ctx, None);
        return Phonology::default();
    };
    let Some(phon_data_guid) = lang_project.node.objsur_one("PhonologicalData") else {
        record_unreferenced_environments(ctx, None);
        return Phonology::default();
    };
    let Some(phon_data) = ctx.require_from(
        &phon_data_guid,
        "PhPhonData",
        "phonology",
        lang_project,
        "PhonologicalData",
    ) else {
        record_unreferenced_environments(ctx, None);
        return Phonology::default();
    };

    let (phonemes, boundary_markers) = extract_phoneme_set(ctx, phon_data, project_name);
    let natural_classes = extract_natural_classes(ctx, phon_data);
    let environments = extract_environments(ctx, phon_data);
    let feature_constraints = extract_feature_constraints(ctx, phon_data);
    let rules = extract_rules(ctx, phon_data);
    record_unreferenced_environments(ctx, Some(phon_data));

    Phonology {
        phoneme_set: phon_data.node.objsur_list("PhonemeSets").into_iter().next(),
        phonemes,
        boundary_markers,
        natural_classes,
        environments,
        rules,
        feature_constraints,
    }
}

/// `HCLoader` only ever loads the first phoneme set (HCLoader.cs:204); this does the same, warning if there is more than one.
fn extract_phoneme_set(
    ctx: &mut Ctx,
    phon_data: &Record,
    project_name: &str,
) -> (Vec<Phoneme>, Vec<BoundaryMarker>) {
    let set_guids = phon_data.node.objsur_list("PhonemeSets");
    let first_set = set_guids
        .first()
        .map(|guid| (guid.as_str(), ctx.resolve_reference(guid, "PhPhonemeSet")));
    if set_guids.len() > 1 {
        let (guid, resolution) = first_set
            .as_ref()
            .expect("a multi-set list has a first set");
        let subjects = ctx.subjects_for_reference(
            guid,
            "PhPhonemeSet",
            *resolution,
            (phon_data, "PhonemeSets"),
        );
        let message = format!(
            "FieldWorks project '{project_name}' has {} phoneme-set references; only the first is eligible for loading.",
            set_guids.len()
        );
        let warning = subjects.into_iter().fold(
            Warning::new(super::codes::ONLY_FIRST_USED, message),
            Warning::with_subject,
        );
        ctx.warnings.push(warning);
    }
    // Every set beyond the first is looked at only to record it as considered, never selected.
    for skipped_guid in set_guids.iter().skip(1) {
        let Some(skipped) = ctx.get(skipped_guid) else {
            continue;
        };
        for g in skipped.node.objsur_list("Phonemes") {
            if ctx.get(&g).is_some_and(|r| r.class == "PhPhoneme") {
                let key = InventoryKey::object(InventoryKind::Phoneme, g);
                ctx.considered(key.clone());
                ctx.not_considered(
                    key,
                    pg_snapshot::LoadReasonCode::AdditionalPhonemeSetNotSelected,
                );
            }
        }
        for g in skipped.node.objsur_list("BoundaryMarkers") {
            if ctx.get(&g).is_some_and(|r| r.class == "PhBdryMarker") {
                let key = InventoryKey::object(InventoryKind::BoundaryMarker, g);
                ctx.considered(key.clone());
                ctx.not_considered(
                    key,
                    pg_snapshot::LoadReasonCode::AdditionalPhonemeSetNotSelected,
                );
            }
        }
    }
    let Some((set_guid, resolution)) = first_set else {
        return (Vec::new(), Vec::new());
    };
    let Some(set) = ctx.require_resolution(
        set_guid,
        "PhPhonemeSet",
        "phonology.phonemes",
        resolution,
        (phon_data, "PhonemeSets"),
    ) else {
        return (Vec::new(), Vec::new());
    };

    let phonemes = set
        .node
        .objsur_list("Phonemes")
        .into_iter()
        .filter_map(|g| extract_phoneme(ctx, &g, set))
        .collect();
    let boundary_markers = set
        .node
        .objsur_list("BoundaryMarkers")
        .into_iter()
        .filter(|guid| guid != WORD_BOUNDARY_MARKER_GUID)
        .filter_map(|g| extract_boundary_marker(ctx, &g, set))
        .collect();
    (phonemes, boundary_markers)
}

fn extract_phoneme(ctx: &mut Ctx, guid: &str, owner: &Record) -> Option<Phoneme> {
    let rec = ctx.require_from(guid, "PhPhoneme", "phonology.phonemes", owner, "Phonemes")?;
    let key = InventoryKey::object(InventoryKind::Phoneme, guid.to_string());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    let name = ctx.best_analysis(&rec.node.ws_forms("Name"));
    let representations = code_representations(ctx, rec, "phonology.phonemes");
    if representations.is_empty() {
        let warning = Warning::new(
            super::codes::EMPTY_REPRESENTATION,
            format!(
                "phonology.phonemes: phoneme {guid} ({name:?}) has no representations after \
                 dotted-circle stripping"
            ),
        )
        .with_subject(ctx.subject_for_record(rec, Some("Codes")));
        ctx.reject_with_warning(
            key,
            IssueClass::UnrepresentableForHc,
            false,
            Some(pg_snapshot::SourceRef {
                kind: FwClass::from_wire(&rec.class),
                id: rec.guid.clone(),
            }),
            warning,
        );
    } else {
        ctx.represented(key);
    }
    let features = rec
        .node
        .objsur_one("Features")
        .and_then(|fs_guid| {
            extract_feature_structure(ctx, &fs_guid, "phonology.phonemes", rec, "Features")
        })
        .filter(|fs| !fs.values.is_empty());
    Some(Phoneme {
        guid: guid.to_string(),
        name,
        representations,
        features,
        basic_ipa_symbol: rec.node.child("BasicIPASymbol").map(|c| c.text.clone()),
    })
}

fn extract_boundary_marker(ctx: &mut Ctx, guid: &str, owner: &Record) -> Option<BoundaryMarker> {
    let rec = ctx.require_from(
        guid,
        "PhBdryMarker",
        "phonology.boundaryMarkers",
        owner,
        "BoundaryMarkers",
    )?;
    let key = InventoryKey::object(InventoryKind::BoundaryMarker, guid.to_string());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    let marker = BoundaryMarker {
        guid: guid.to_string(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        representations: code_representations(ctx, rec, "phonology.boundaryMarkers"),
    };
    ctx.represented(key);
    Some(marker)
}

/// `PhPhoneme.CodesOS`/`PhBdryMarker.CodesOS`: flattens every code's forms, dotted-circle stripped.
fn code_representations(ctx: &mut Ctx, rec: &Record, label: &str) -> Vec<pg_snapshot::WsForm> {
    let mut out = Vec::new();
    for code_guid in rec.node.objsur_list("Codes") {
        let Some(code) = ctx.get(&code_guid) else {
            ctx.warn_with_subjects(
                super::codes::DANGLING_REFERENCE,
                format!("{label}: dangling PhCode reference {code_guid}"),
                [
                    ctx.subject_for_record(rec, Some("Codes")),
                    ctx.unresolved_subject(&code_guid, "PhCode", Some("Codes")),
                ],
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
        .filter_map(|guid| extract_natural_class(ctx, &guid, phon_data))
        .collect()
}

fn extract_natural_class(ctx: &mut Ctx, guid: &str, owner: &Record) -> Option<NaturalClass> {
    let rec = ctx.get(guid)?;
    let name = ctx.best_analysis(&rec.node.ws_forms("Abbreviation"));
    let display_name =
        Some(ctx.best_analysis(&rec.node.ws_forms("Name"))).filter(|n| !n.trim().is_empty());
    let key = InventoryKey::object(InventoryKind::NaturalClass, guid.to_string());
    match rec.class.as_str() {
        "PhNCSegments" => {
            let phonemes = rec.node.objsur_list("Segments");
            ctx.considered(key.clone());
            ctx.selected(key.clone());
            ctx.represented(key);
            Some(NaturalClass::Segments {
                guid: guid.to_string(),
                name,
                display_name,
                phonemes,
            })
        }
        "PhNCFeatures" => {
            let features = rec
                .node
                .objsur_one("Features")
                .and_then(|fs| {
                    extract_feature_structure(ctx, &fs, "phonology.naturalClasses", rec, "Features")
                })
                .unwrap_or_default();
            ctx.considered(key.clone());
            ctx.selected(key.clone());
            ctx.represented(key);
            Some(NaturalClass::Features {
                guid: guid.to_string(),
                name,
                display_name,
                features,
            })
        }
        other => {
            ctx.warn_with_subjects(
                super::codes::UNEXPECTED_CLASS,
                format!("phonology.naturalClasses: {guid} has unexpected class {other}"),
                [
                    ctx.subject_for_record(owner, Some("NaturalClasses")),
                    ctx.unresolved_subject(guid, "PhNaturalClass", Some("NaturalClasses")),
                    ctx.subject_for_record(rec, None),
                ],
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
        .filter_map(|guid| extract_environment(ctx, &guid, phon_data))
        .collect()
}

fn record_unreferenced_environments(ctx: &mut Ctx, phon_data: Option<&Record>) {
    let referenced: std::collections::BTreeSet<_> = phon_data
        .map(|record| {
            record
                .node
                .objsur_list("Environments")
                .into_iter()
                .collect()
        })
        .unwrap_or_default();
    let candidates: Vec<_> = ctx
        .graph
        .headers
        .iter()
        .filter(|header| header.class == "PhEnvironment" && !header.guid.is_empty())
        .map(|header| header.guid.clone())
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    for guid in candidates {
        if referenced.contains(&guid)
            || !seen.insert(guid.clone())
            || !ctx
                .get(&guid)
                .is_some_and(|record| record.class == "PhEnvironment")
        {
            continue;
        }
        ctx.not_considered(
            InventoryKey::object(InventoryKind::Environment, guid),
            pg_snapshot::LoadReasonCode::Unreferenced,
        );
    }
}

fn extract_environment(ctx: &mut Ctx, guid: &str, owner: &Record) -> Option<Environment> {
    let rec = ctx.require_from(
        guid,
        "PhEnvironment",
        "phonology.environments",
        owner,
        "Environments",
    )?;
    let key = InventoryKey::object(InventoryKind::Environment, guid.to_string());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    let environment = Environment {
        guid: guid.to_string(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        representation: rec
            .node
            .str_text("StringRepresentation")
            .unwrap_or_default(),
    };
    ctx.represented(key);
    Some(environment)
}

fn extract_feature_constraints(ctx: &mut Ctx, phon_data: &Record) -> Vec<FeatureConstraint> {
    phon_data
        .node
        .objsur_list("FeatConstraints")
        .into_iter()
        .filter_map(|guid| {
            let rec = ctx.require_from(
                &guid,
                "PhFeatureConstraint",
                "phonology.featureConstraints",
                phon_data,
                "FeatConstraints",
            )?;
            let Some(feature) = rec.node.objsur_one("Feature") else {
                ctx.warn_with_subjects(
                    super::codes::MISSING_REQUIRED_FIELD,
                    format!(
                        "phonology.featureConstraints: feature constraint {guid} has no Feature"
                    ),
                    [ctx.subject_for_record(rec, Some("Feature"))],
                );
                return None;
            };
            let key = InventoryKey::object(InventoryKind::FeatureConstraint, guid.clone());
            ctx.considered(key.clone());
            ctx.selected(key.clone());
            ctx.represented(key);
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
            let key = InventoryKey::object(InventoryKind::PhonologicalRule, guid.clone());
            if rec.node.val_bool("Disabled").unwrap_or(false) {
                if matches!(rec.class.as_str(), "PhRegularRule" | "PhMetathesisRule") {
                    ctx.considered(key.clone());
                    ctx.not_considered(key, pg_snapshot::LoadReasonCode::Disabled);
                }
                return None;
            }
            match rec.class.as_str() {
                "PhRegularRule" => {
                    ctx.considered(key.clone());
                    ctx.selected(key.clone());
                    let rule = extract_rewrite_rule(ctx, rec).map(PhonologicalRule::Rewrite);
                    if rule.is_some() {
                        ctx.represented(key);
                    }
                    rule
                }
                "PhMetathesisRule" => {
                    ctx.considered(key.clone());
                    ctx.selected(key.clone());
                    let rule = extract_metathesis_rule(ctx, rec).map(PhonologicalRule::Metathesis);
                    if rule.is_some() {
                        ctx.represented(key);
                    }
                    rule
                }
                other => {
                    ctx.warn_with_subjects(
                        super::codes::UNEXPECTED_CLASS,
                        format!("phonology.rules: {guid} has unexpected class {other}"),
                        [
                            ctx.subject_for_record(phon_data, Some("PhonRules")),
                            ctx.unresolved_subject(&guid, "PhonologicalRule", Some("PhonRules")),
                            ctx.subject_for_record(rec, None),
                        ],
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
            ctx.warn_with_subjects(
                super::codes::UNRECOGNIZED_ENUM_VALUE,
                format!(
                    "{label}: unexpected Direction {other:?} on rule {}, defaulting to leftToRight",
                    rec.guid
                ),
                [ctx.subject_for_record(rec, Some("Direction"))],
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
        .filter_map(|g| resolve_phon_context(ctx, &g, label, rec, "StrucDesc"))
        .collect();
    let right_hand_sides: Vec<RewriteRhs> = rec
        .node
        .objsur_list("RightHandSides")
        .into_iter()
        .filter_map(|g| extract_rewrite_rhs(ctx, &g, rec))
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

fn extract_rewrite_rhs(ctx: &mut Ctx, guid: &str, owner: &Record) -> Option<RewriteRhs> {
    let label = "phonology.rules(rewrite).rightHandSides";
    let rec = ctx.require_from(guid, "PhSegRuleRHS", label, owner, "RightHandSides")?;
    let structural_change = rec
        .node
        .objsur_list("StrucChange")
        .into_iter()
        .filter_map(|g| resolve_phon_context(ctx, &g, label, rec, "StrucChange"))
        .collect();
    let left_context = rec
        .node
        .objsur_one("LeftContext")
        .and_then(|g| resolve_phon_context(ctx, &g, label, rec, "LeftContext"));
    let right_context = rec
        .node
        .objsur_one("RightContext")
        .and_then(|g| resolve_phon_context(ctx, &g, label, rec, "RightContext"));
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
            let wrapper = ctx.require_from(&wrapper_guid, "PhPhonRuleFeat", label, rec, field)?;
            let key = InventoryKey::object(InventoryKind::RuleFeature, wrapper_guid.clone());
            ctx.considered(key.clone());
            ctx.selected(key.clone());
            let item = wrapper.node.objsur_one("Item");
            if let Some(item) = item {
                ctx.represented(key);
                Some(item)
            } else {
                ctx.warn_with_subjects(
                    super::codes::MISSING_REQUIRED_FIELD,
                    format!("{label}: PhPhonRuleFeat {wrapper_guid} has no Item reference"),
                    [ctx.subject_for_record(wrapper, Some("Item"))],
                );
                ctx.recorder
                    .rejected_without_issue(pg_snapshot::LoadDecisionDraft {
                        subject: key,
                        pipeline_stage: pg_snapshot::LoadPipelineStage::Import,
                        context_key: format!("phonologicalRule:{}:{field}", rec.guid),
                        disposition: pg_snapshot::LoadDisposition::Rejected,
                        loaded: Some(false),
                        reason_code: pg_snapshot::LoadReasonCode::ConversionIssue(
                            super::codes::MISSING_REQUIRED_FIELD.wire().to_string(),
                        ),
                        effective_value_json: Some("{\"missingField\":\"Item\"}".into()),
                        issue_code: None,
                    });
                None
            }
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
        .filter_map(|g| resolve_phon_context(ctx, &g, label, rec, "StrucDesc"))
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
        ctx.warn_with_subjects(
            super::codes::METATHESIS_APPROXIMATION,
            format!(
                "{label}: rule {} StrucChange {:?} does not enumerate all {} structural-description \
                 positions; switch indices may be wrong",
                rec.guid,
                struc_change_text,
                structural_description.len()
            ),
            [ctx.subject_for_record(rec, Some("StrucChange"))],
        );
    }
    let differing: Vec<usize> = permutation
        .iter()
        .enumerate()
        .filter(|(i, &v)| v != i + 1)
        .map(|(i, _)| i)
        .collect();
    if !differing.is_empty() && differing.len() != 2 {
        ctx.warn_with_subjects(
            super::codes::METATHESIS_APPROXIMATION,
            format!(
                "{label}: rule {} has a StrucChange permutation more complex than a simple two-part \
                 swap ({:?}); left/right switch indices are an approximation",
                rec.guid, struc_change_text
            ),
            [ctx.subject_for_record(rec, Some("StrucChange"))],
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
pub(crate) fn resolve_phon_context(
    ctx: &mut Ctx,
    guid: &str,
    label: &str,
    owner: &Record,
    field: &str,
) -> Option<PhonContext> {
    let rec = ctx.get(guid)?;
    let key = InventoryKey::object(InventoryKind::PhonologicalContext, guid.to_string());
    let record_represented = |ctx: &mut Ctx| {
        ctx.considered(key.clone());
        ctx.selected(key.clone());
        ctx.represented(key.clone());
    };
    match rec.class.as_str() {
        "PhSequenceContext" => {
            let members = rec
                .node
                .objsur_list("Members")
                .into_iter()
                .filter_map(|g| resolve_phon_context(ctx, &g, label, rec, "Members"))
                .collect();
            record_represented(ctx);
            Some(PhonContext::Sequence { members })
        }
        "PhIterationContext" => {
            let min = rec.node.val_int("Minimum").unwrap_or(0) as i32;
            let max = rec.node.val_int("Maximum").unwrap_or(-1) as i32;
            let member_guid = required_context_reference(ctx, rec, "Member")?;
            let member = resolve_phon_context(ctx, &member_guid, label, rec, "Member")?;
            record_represented(ctx);
            Some(PhonContext::Iteration {
                min,
                max,
                member: Box::new(member),
            })
        }
        "PhSimpleContextSeg" => {
            let phoneme = required_context_reference(ctx, rec, "FeatureStructure")?;
            record_represented(ctx);
            Some(PhonContext::Segment { phoneme })
        }
        "PhSimpleContextNC" => {
            let natural_class = required_context_reference(ctx, rec, "FeatureStructure")?;
            let plus_variables = rec.node.objsur_list("PlusConstr");
            let minus_variables = rec.node.objsur_list("MinusConstr");
            record_represented(ctx);
            Some(PhonContext::NaturalClass {
                natural_class,
                plus_variables,
                minus_variables,
            })
        }
        "PhSimpleContextBdry" => {
            let marker = required_context_reference(ctx, rec, "FeatureStructure")?;
            record_represented(ctx);
            if marker == WORD_BOUNDARY_MARKER_GUID {
                Some(PhonContext::WordBoundary)
            } else {
                match ctx.get(&marker) {
                    Some(m) if m.class == "PhBdryMarker" => Some(PhonContext::Boundary { marker }),
                    _ => Some(PhonContext::WordBoundary),
                }
            }
        }
        "PhVariable" => Some(PhonContext::Variable),
        other => {
            ctx.warn_with_subjects(
                super::codes::UNEXPECTED_CLASS,
                format!("{label}: {guid} has unexpected PhContextOrVar class {other}"),
                [
                    ctx.subject_for_record(owner, Some(field)),
                    ctx.unresolved_subject(guid, "PhContextOrVar", Some(field)),
                    ctx.subject_for_record(rec, None),
                ],
            );
            None
        }
    }
}

fn required_context_reference(ctx: &mut Ctx<'_>, rec: &Record, field: &str) -> Option<String> {
    let guid = rec.node.objsur_one(field);
    if guid.is_none() {
        let key = InventoryKey::object(InventoryKind::PhonologicalContext, rec.guid.clone());
        ctx.considered(key.clone());
        ctx.selected(key.clone());
        ctx.recorder
            .rejected_without_issue(pg_snapshot::LoadDecisionDraft {
                subject: key,
                pipeline_stage: pg_snapshot::LoadPipelineStage::Import,
                context_key: format!("phonContext:{}:{field}", rec.guid),
                disposition: pg_snapshot::LoadDisposition::Rejected,
                loaded: Some(false),
                reason_code: pg_snapshot::LoadReasonCode::ConversionIssue(
                    super::codes::MISSING_REQUIRED_FIELD.wire().to_string(),
                ),
                effective_value_json: Some(format!("{{\"missingField\":\"{field}\"}}")),
                issue_code: None,
            });
    }
    guid
}
