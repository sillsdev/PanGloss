//! `featureSystems` snapshot section, plus the `FeatureStructure` resolver used everywhere else
//! a feature structure is attached to something (phonemes, MSAs, stem-name regions, natural
//! classes, ...). See `docs/snapshot-format.md` §3.

use pg_snapshot::{
    ClosedFeature, ComplexFeature, FeatureStructure, FeatureSystem, FeatureSystems, FeatureValue,
    FeatureValueKind, FeatureValueSymbol,
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
        .map(|g| extract_feature_system(ctx, &g, "featureSystems.phonological"))
        .unwrap_or_default();
    let morphosyntactic = lang_project
        .node
        .objsur_one("MsFeatureSystem")
        .map(|g| extract_feature_system(ctx, &g, "featureSystems.morphosyntactic"))
        .unwrap_or_default();
    FeatureSystems {
        phonological,
        morphosyntactic,
    }
}

fn extract_feature_system(ctx: &mut Ctx, guid: &str, label: &str) -> FeatureSystem {
    let Some(rec) = ctx.require(guid, "FsFeatureSystem", label) else {
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
            Some(r) => ctx.warn(
                super::codes::UNEXPECTED_CLASS,
                format!(
                    "{label}: feature {feature_guid} has unexpected class {}",
                    r.class
                ),
            ),
            None => ctx.warn(
                super::codes::DANGLING_REFERENCE,
                format!("{label}: dangling feature reference {feature_guid}"),
            ),
        }
    }
    FeatureSystem {
        closed_features,
        complex_features,
    }
}

fn extract_closed_feature(ctx: &mut Ctx, rec: &Record) -> ClosedFeature {
    let name = ctx.best_analysis(&rec.node.ws_forms("Name"));
    let abbreviation = ctx.best_analysis(&rec.node.ws_forms("Abbreviation"));
    let mut values = Vec::new();
    for value_guid in rec.node.objsur_list("Values") {
        if let Some(v) = ctx.require(&value_guid, "FsSymFeatVal", "closedFeature.values") {
            values.push(FeatureValueSymbol {
                guid: value_guid,
                name: ctx.best_analysis(&v.node.ws_forms("Name")),
                abbreviation: ctx.best_analysis(&v.node.ws_forms("Abbreviation")),
            });
        }
    }
    ClosedFeature {
        guid: rec.guid.clone(),
        name,
        abbreviation,
        values,
    }
}

fn extract_complex_feature(ctx: &mut Ctx, rec: &Record) -> ComplexFeature {
    ComplexFeature {
        guid: rec.guid.clone(),
        name: ctx.best_analysis(&rec.node.ws_forms("Name")),
        abbreviation: ctx.best_analysis(&rec.node.ws_forms("Abbreviation")),
        feature_type: rec.node.objsur_one("Type"),
    }
}

/// Resolve an `FsFeatStruc` guid into a `FeatureStructure`, recursing into any `FsComplexValue`
/// members. `label` is used only for warning messages. Returns `None` (rather than an empty
/// structure) when `guid` doesn't resolve at all, so callers can distinguish "no feature
/// structure was ever attached" (`None` on the owning field) from "the referenced one was empty"
/// (`Some(FeatureStructure { values: vec![] })`).
pub fn extract_feature_structure(
    ctx: &mut Ctx,
    guid: &str,
    label: &str,
) -> Option<FeatureStructure> {
    let rec = ctx.require(guid, "FsFeatStruc", label)?;
    Some(extract_feature_struct_node(ctx, rec, label))
}

fn extract_feature_struct_node(ctx: &mut Ctx, rec: &Record, label: &str) -> FeatureStructure {
    let mut values = Vec::new();
    for spec_guid in rec.node.objsur_list("FeatureSpecs") {
        let Some(spec) = ctx.get(&spec_guid) else {
            ctx.warn(
                super::codes::DANGLING_REFERENCE,
                format!("{label}: dangling feature-spec reference {spec_guid}"),
            );
            continue;
        };
        let Some(feature) = spec.node.objsur_one("Feature") else {
            ctx.warn(
                super::codes::MISSING_REQUIRED_FIELD,
                format!("{label}: feature spec {spec_guid} has no Feature reference"),
            );
            continue;
        };
        let value = match spec.class.as_str() {
            "FsClosedValue" => match spec.node.objsur_one("Value") {
                Some(v) => FeatureValueKind::Closed { value: v },
                None => {
                    ctx.warn(
                        super::codes::MISSING_REQUIRED_FIELD,
                        format!("{label}: closed feature value {spec_guid} has no Value"),
                    );
                    continue;
                }
            },
            "FsComplexValue" => match spec.node.objsur_one("Value") {
                Some(nested_guid) => match ctx.get(&nested_guid) {
                    Some(nested_rec) => FeatureValueKind::Complex {
                        value: extract_feature_struct_node(ctx, nested_rec, label),
                    },
                    None => {
                        ctx.warn(
                            super::codes::DANGLING_REFERENCE,
                            format!(
                                "{label}: complex feature value {spec_guid} references missing FsFeatStruc {nested_guid}"
                            ),
                        );
                        continue;
                    }
                },
                None => {
                    ctx.warn(
                        super::codes::MISSING_REQUIRED_FIELD,
                        format!("{label}: complex feature value {spec_guid} has no Value"),
                    );
                    continue;
                }
            },
            other => {
                ctx.warn(
                    super::codes::UNEXPECTED_CLASS,
                    format!("{label}: feature spec {spec_guid} has unexpected class {other}"),
                );
                continue;
            }
        };
        values.push(FeatureValue { feature, value });
    }
    FeatureStructure { values }
}
