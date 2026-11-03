//! `featureSystems` snapshot section, plus the `FeatureStructure` resolver used everywhere else a feature structure is attached to something.

use pg_snapshot::{
    ClosedFeature, ComplexFeature, ConversionIssue, FeatureStructure, FeatureSystem,
    FeatureSystems, FeatureValue, FeatureValueKind, FeatureValueSymbol, FwClass, InventoryKey,
    InventoryKind, IssueClass, SourceRef, Warning,
};

use super::Ctx;
use crate::xml::Record;

pub fn extract_feature_systems(ctx: &mut Ctx, lang_project: Option<&Record>) -> FeatureSystems {
    let Some(lang_project) = lang_project else {
        return FeatureSystems::default();
    };
    let phonological = lang_project
        .node
        .objsur_one("PhFeatureSystem")
        .map(|g| {
            extract_feature_system(
                ctx,
                &g,
                "featureSystems.phonological",
                lang_project,
                "PhFeatureSystem",
            )
        })
        .unwrap_or_default();
    let morphosyntactic = lang_project
        .node
        .objsur_one("MsFeatureSystem")
        .map(|g| {
            extract_feature_system(
                ctx,
                &g,
                "featureSystems.morphosyntactic",
                lang_project,
                "MsFeatureSystem",
            )
        })
        .unwrap_or_default();
    FeatureSystems {
        phonological,
        morphosyntactic,
    }
}

fn extract_feature_system(
    ctx: &mut Ctx,
    guid: &str,
    label: &str,
    owner: &Record,
    field: &str,
) -> FeatureSystem {
    let Some(rec) = ctx.require_from(guid, "FsFeatureSystem", label, owner, field) else {
        return FeatureSystem::default();
    };
    let mut closed_features = Vec::new();
    let mut complex_features = Vec::new();
    for feature_guid in rec.node.objsur_list("Features") {
        match ctx.get(&feature_guid) {
            Some(r) if r.class == "FsClosedFeature" => {
                closed_features.push(extract_closed_feature(ctx, r))
            }
            Some(r) if r.class == "FsComplexFeature" => {
                complex_features.push(extract_complex_feature(ctx, r))
            }
            Some(r) => ctx.warn_with_subjects(
                super::codes::UNEXPECTED_CLASS,
                format!(
                    "{label}: feature {feature_guid} has unexpected class {}",
                    r.class
                ),
                [
                    ctx.subject_for_record(rec, Some("Features")),
                    ctx.unresolved_subject(&feature_guid, "FsFeatureDefinition", Some("Features")),
                    ctx.subject_for_record(r, None),
                ],
            ),
            None => ctx.warn_with_subjects(
                super::codes::DANGLING_REFERENCE,
                format!("{label}: dangling feature reference {feature_guid}"),
                [
                    ctx.subject_for_record(rec, Some("Features")),
                    ctx.unresolved_subject(&feature_guid, "FsFeatureDefinition", Some("Features")),
                ],
            ),
        }
    }
    FeatureSystem {
        closed_features,
        complex_features,
    }
}

fn extract_closed_feature(ctx: &mut Ctx, rec: &Record) -> ClosedFeature {
    let key = InventoryKey::object(InventoryKind::FeatureDefinition, rec.guid.clone());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    let name = ctx.best_analysis(&rec.node.ws_forms("Name"));
    let abbreviation = ctx.best_analysis(&rec.node.ws_forms("Abbreviation"));
    let mut values = Vec::new();
    for value_guid in rec.node.objsur_list("Values") {
        let resolution = ctx.resolve_reference(&value_guid, "FsSymFeatVal");
        let resolved = ctx.require_resolution(
            &value_guid,
            "FsSymFeatVal",
            "closedFeature.values",
            resolution,
            (rec, "Values"),
        );
        let attachment = InventoryKey::attachment(
            InventoryKind::FeatureValue,
            rec.guid.clone(),
            value_guid.clone(),
            "value",
        );
        ctx.authored(attachment.clone());
        ctx.considered(attachment.clone());
        ctx.selected(attachment.clone());
        if let Some(v) = resolved {
            ctx.represented(attachment);
            let value_key = InventoryKey::object(InventoryKind::FeatureValue, value_guid.clone());
            ctx.considered(value_key.clone());
            ctx.selected(value_key.clone());
            ctx.represented(value_key);
            values.push(FeatureValueSymbol {
                guid: value_guid,
                name: ctx.best_analysis(&v.node.ws_forms("Name")),
                abbreviation: ctx.best_analysis(&v.node.ws_forms("Abbreviation")),
            });
        } else {
            ctx.record_rejected(
                attachment,
                ConversionIssue {
                    code: resolution.failure_code(),
                    class: IssueClass::InvalidSource,
                    source: Some(SourceRef {
                        kind: pg_snapshot::FwClass::FsSymFeatVal,
                        id: value_guid.clone(),
                    }),
                    fatal: true,
                    message: format!(
                        "closedFeature.values: feature {} references value {value_guid}, \
                         which does not resolve to a FsSymFeatVal",
                        rec.guid
                    ),
                },
            );
        }
    }
    ctx.represented(key);
    ClosedFeature {
        guid: rec.guid.clone(),
        name,
        abbreviation,
        values,
    }
}

fn extract_complex_feature(ctx: &mut Ctx, rec: &Record) -> ComplexFeature {
    let key = InventoryKey::object(InventoryKind::FeatureDefinition, rec.guid.clone());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    let feature = ComplexFeature {
        guid: rec.guid.clone(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        abbreviation: ctx.best_analysis(&rec.node.ws_forms("Abbreviation")),
        feature_type: rec.node.objsur_one("Type"),
    };
    ctx.represented(key);
    feature
}

/// Resolves an `FsFeatStruc` guid into a `FeatureStructure`, recursing into any `FsComplexValue` members; returns `None` when `guid` doesn't resolve at all, so callers can distinguish "never attached" from "the referenced one was empty".
pub fn extract_feature_structure(
    ctx: &mut Ctx,
    guid: &str,
    label: &str,
    owner: &Record,
    field: &str,
) -> Option<FeatureStructure> {
    let rec = ctx.require_from(guid, "FsFeatStruc", label, owner, field)?;
    let key = InventoryKey::object(InventoryKind::FeatureStructure, guid.to_string());
    ctx.considered(key.clone());
    ctx.selected(key.clone());
    let (structure, intact) = extract_feature_struct_node(ctx, rec, label);
    if intact {
        ctx.represented(key);
    }
    Some(structure)
}

/// The structure plus `false` when a spec was rejected: a negated value must fail the load, not vanish from the grammar.
fn extract_feature_struct_node(
    ctx: &mut Ctx,
    rec: &Record,
    label: &str,
) -> (FeatureStructure, bool) {
    let mut intact = true;
    let mut values = Vec::new();
    for spec_guid in rec.node.objsur_list("FeatureSpecs") {
        let Some(spec) = ctx.get(&spec_guid) else {
            ctx.warn_with_subjects(
                super::codes::DANGLING_REFERENCE,
                format!("{label}: dangling feature-spec reference {spec_guid}"),
                [
                    ctx.subject_for_record(rec, Some("FeatureSpecs")),
                    ctx.unresolved_subject(&spec_guid, "FsFeatureSpec", Some("FeatureSpecs")),
                ],
            );
            continue;
        };
        let Some(feature) = spec.node.objsur_one("Feature") else {
            ctx.warn_with_subjects(
                super::codes::MISSING_REQUIRED_FIELD,
                format!("{label}: feature spec {spec_guid} has no Feature reference"),
                [ctx.subject_for_record(spec, Some("Feature"))],
            );
            continue;
        };
        let value = match spec.class.as_str() {
            "FsClosedValue" => match spec.node.objsur_one("Value") {
                Some(v) => FeatureValueKind::Closed { value: v },
                None => {
                    ctx.warn_with_subjects(
                        super::codes::MISSING_REQUIRED_FIELD,
                        format!("{label}: closed feature value {spec_guid} has no Value"),
                        [ctx.subject_for_record(spec, Some("Value"))],
                    );
                    continue;
                }
            },
            "FsComplexValue" => match spec.node.objsur_one("Value") {
                Some(nested_guid) => match ctx.get(&nested_guid) {
                    Some(nested_rec) if nested_rec.class == "FsFeatStruc" => {
                        let nested_key = InventoryKey::object(
                            InventoryKind::FeatureStructure,
                            nested_guid.clone(),
                        );
                        ctx.considered(nested_key.clone());
                        ctx.selected(nested_key.clone());
                        let (nested, nested_intact) =
                            extract_feature_struct_node(ctx, nested_rec, label);
                        if nested_intact {
                            ctx.represented(nested_key);
                        } else {
                            intact = false;
                        }
                        FeatureValueKind::Complex { value: nested }
                    }
                    actual => {
                        let mut subjects = vec![
                            ctx.subject_for_record(spec, Some("Value")),
                            ctx.unresolved_subject(&nested_guid, "FsFeatStruc", Some("Value")),
                        ];
                        if let Some(actual) = actual {
                            subjects.push(ctx.subject_for_record(actual, None));
                        }
                        ctx.warn_with_subjects(
                            super::codes::DANGLING_REFERENCE,
                            format!(
                                "{label}: complex feature value {spec_guid} references missing FsFeatStruc {nested_guid}"
                            ),
                            subjects,
                        );
                        continue;
                    }
                },
                None => {
                    ctx.warn_with_subjects(
                        super::codes::MISSING_REQUIRED_FIELD,
                        format!("{label}: complex feature value {spec_guid} has no Value"),
                        [ctx.subject_for_record(spec, Some("Value"))],
                    );
                    continue;
                }
            },
            other => {
                let warning = Warning::new(
                    super::codes::NEGATED_FEATURE_VALUE,
                    format!(
                        "{label}: feature spec {spec_guid} has class {other}, which HermitCrab \
                         cannot load (HCLoader accepts only FsClosedValue and FsComplexValue)"
                    ),
                )
                .with_subject(ctx.subject_for_record(rec, Some("FeatureSpecs")))
                .with_subject(ctx.unresolved_subject(
                    &spec_guid,
                    "FsFeatureSpec",
                    Some("FeatureSpecs"),
                ))
                .with_subject(ctx.subject_for_record(spec, None));
                ctx.reject_with_warning(
                    InventoryKey::object(InventoryKind::FeatureStructure, rec.guid.clone()),
                    IssueClass::UnrepresentableForHc,
                    true,
                    Some(SourceRef {
                        kind: FwClass::from_wire(&spec.class),
                        id: spec_guid.clone(),
                    }),
                    warning,
                );
                intact = false;
                continue;
            }
        };
        values.push(FeatureValue { feature, value });
    }
    (FeatureStructure { values }, intact)
}
