use std::collections::{BTreeMap, BTreeSet};

use pg_grammar::chardef::{CharDef, CharDefKind, CharDefTable};
use pg_grammar::compile::{
    environment_side_elements, EnvironmentResolution, EnvironmentResolutionStatus,
    EnvironmentSideElement,
};
use pg_grammar::model::{
    NaturalClassKind as CompiledNaturalClassKind, Pattern, PatternNode, PhonRuleDef,
};
use pg_snapshot::conversion::LoadPipelineStage;
use pg_snapshot::phonology::{NaturalClass, PhonContext, PhonologicalRule, RuleDirection};
use pg_snapshot::{InventoryKey, InventoryKind, LoadDecision, LoadDisposition, Snapshot};
use rusqlite::{params, Transaction};

use crate::features::FeatureStructureIds;
use crate::load::subject_key;
use crate::morphology::checked_guid;
use crate::FactsError;

const COMPILE_CONTEXT: &str = "production";

pub(crate) fn validate_authored_guids(snapshot: &Snapshot) -> Result<(), FactsError> {
    if let Some(guid) = &snapshot.phonology.phoneme_set {
        checked_guid(guid, "phonology.phonemeSet")?;
    }
    for phoneme in &snapshot.phonology.phonemes {
        checked_guid(&phoneme.guid, "phonology.phoneme.guid")?;
    }
    for boundary in &snapshot.phonology.boundary_markers {
        checked_guid(&boundary.guid, "phonology.boundaryMarker.guid")?;
    }
    for class in &snapshot.phonology.natural_classes {
        checked_guid(natural_class_guid(class), "phonology.naturalClass.guid")?;
    }
    for environment in &snapshot.phonology.environments {
        checked_guid(&environment.guid, "phonology.environment.guid")?;
    }
    for constraint in &snapshot.phonology.feature_constraints {
        checked_guid(&constraint.guid, "phonology.featureConstraint.guid")?;
    }
    for rule in &snapshot.phonology.rules {
        checked_guid(phonological_rule_guid(rule), "phonology.rule.guid")?;
    }
    for system in [
        &snapshot.feature_systems.phonological,
        &snapshot.feature_systems.morphosyntactic,
    ] {
        for feature in &system.closed_features {
            checked_guid(&feature.guid, "feature.guid")?;
            for value in &feature.values {
                checked_guid(&value.guid, "featureValue.guid")?;
            }
        }
        for feature in &system.complex_features {
            checked_guid(&feature.guid, "feature.guid")?;
        }
    }
    Ok(())
}

pub(crate) fn insert_authored(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    feature_structures: &FeatureStructureIds,
    environment_resolutions: &[EnvironmentResolution],
    decisions: &[LoadDecision],
    grammar: Option<&pg_grammar::model::Grammar>,
) -> Result<(), FactsError> {
    insert_phoneme_set(tx, snapshot)?;
    insert_phonemes(tx, snapshot, feature_structures)?;
    insert_boundaries(tx, snapshot)?;
    insert_natural_classes(tx, snapshot, feature_structures)?;
    insert_feature_constraints(tx, snapshot)?;
    insert_environments(tx, snapshot, environment_resolutions)?;
    insert_allomorph_environments(tx, snapshot)?;
    insert_environment_usage(tx, snapshot, environment_resolutions, decisions)?;
    insert_effective_members(tx, snapshot, grammar)?;
    insert_environment_sides(tx, snapshot, environment_resolutions, grammar)?;
    insert_phonological_rules(tx, snapshot, grammar)?;
    insert_environment_patterns(tx, environment_resolutions, grammar)?;
    insert_rule_patterns(tx, snapshot)?;
    Ok(())
}

fn insert_phoneme_set(tx: &Transaction<'_>, snapshot: &Snapshot) -> Result<(), FactsError> {
    let guid = snapshot
        .phonology
        .phoneme_set
        .as_deref()
        .map(|value| checked_guid(value, "phonology.phonemeSet"))
        .transpose()?;
    tx.execute(
        "INSERT INTO phoneme_set(singleton, guid) VALUES (1, ?1)",
        [guid],
    )?;
    Ok(())
}

fn insert_phonemes(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    feature_structures: &FeatureStructureIds,
) -> Result<(), FactsError> {
    for phoneme in &snapshot.phonology.phonemes {
        let guid = checked_guid(&phoneme.guid, "phonology.phoneme.guid")?;
        let fs_id = feature_structures
            .get(&("phoneme".into(), guid.clone(), "features".into()))
            .copied();
        tx.execute(
            "INSERT INTO phoneme(guid, feature_structure_id, name, basic_ipa_symbol) VALUES (?1, ?2, ?3, ?4)",
            params![guid, fs_id, phoneme.name, phoneme.basic_ipa_symbol],
        )?;
        for (ordinal, form) in phoneme.representations.iter().enumerate() {
            tx.execute(
                "INSERT INTO phoneme_grapheme(phoneme_guid, ordinal, writing_system, grapheme) VALUES (?1, ?2, ?3, ?4)",
                params![guid, ordinal as i64, form.ws, form.form],
            )?;
        }
    }
    Ok(())
}

fn insert_boundaries(tx: &Transaction<'_>, snapshot: &Snapshot) -> Result<(), FactsError> {
    for boundary in &snapshot.phonology.boundary_markers {
        let guid = checked_guid(&boundary.guid, "phonology.boundaryMarker.guid")?;
        tx.execute(
            "INSERT INTO boundary_marker(guid, name) VALUES (?1, ?2)",
            params![guid, boundary.name],
        )?;
        for (ordinal, form) in boundary.representations.iter().enumerate() {
            tx.execute(
                "INSERT INTO boundary_grapheme(boundary_guid, ordinal, writing_system, grapheme) VALUES (?1, ?2, ?3, ?4)",
                params![guid, ordinal as i64, form.ws, form.form],
            )?;
        }
    }
    Ok(())
}

fn insert_natural_classes(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    feature_structures: &FeatureStructureIds,
) -> Result<(), FactsError> {
    for class in &snapshot.phonology.natural_classes {
        let guid = checked_guid(natural_class_guid(class), "phonology.naturalClass.guid")?;
        match class {
            NaturalClass::Segments {
                name,
                display_name,
                phonemes,
                ..
            } => {
                tx.execute(
                    "INSERT INTO natural_class(guid, kind, name, display_name, feature_structure_id) VALUES (?1, 'segments', ?2, ?3, NULL)",
                    params![guid, name, display_name],
                )?;
                for (ordinal, phoneme_guid) in phonemes.iter().enumerate() {
                    tx.execute(
                        "INSERT INTO natural_class_member(natural_class_guid, ordinal, phoneme_guid) VALUES (?1, ?2, ?3)",
                        params![guid, ordinal as i64, phoneme_guid],
                    )?;
                }
            }
            NaturalClass::Features {
                name, display_name, ..
            } => {
                let fs_id = feature_structures
                    .get(&("naturalClass".into(), guid.clone(), "features".into()))
                    .copied()
                    .ok_or_else(|| {
                        FactsError::Serialization(format!(
                            "feature class {guid} has no feature-structure projection"
                        ))
                    })?;
                tx.execute(
                    "INSERT INTO natural_class(guid, kind, name, display_name, feature_structure_id) VALUES (?1, 'features', ?2, ?3, ?4)",
                    params![guid, name, display_name, fs_id],
                )?;
            }
        }
    }
    Ok(())
}

fn insert_feature_constraints(tx: &Transaction<'_>, snapshot: &Snapshot) -> Result<(), FactsError> {
    for constraint in &snapshot.phonology.feature_constraints {
        let guid = checked_guid(&constraint.guid, "phonology.featureConstraint.guid")?;
        tx.execute(
            "INSERT INTO feature_constraint(guid, feature_guid) VALUES (?1, ?2)",
            params![guid, constraint.feature],
        )?;
    }
    Ok(())
}

fn insert_environments(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    resolutions: &[EnvironmentResolution],
) -> Result<(), FactsError> {
    let by_guid: BTreeMap<_, _> = resolutions
        .iter()
        .map(|resolution| (resolution.environment_guid.as_str(), resolution))
        .collect();
    for environment in &snapshot.phonology.environments {
        let guid = checked_guid(&environment.guid, "phonology.environment.guid")?;
        let resolution = by_guid.get(guid.as_str()).copied();
        let status = resolution
            .map(|value| match value.status {
                EnvironmentResolutionStatus::Valid => "valid",
                EnvironmentResolutionStatus::Invalid => "invalid",
            })
            .unwrap_or("not_attempted");
        let error = resolution.and_then(|value| value.error.as_deref());
        let error_code = resolution.and_then(|value| value.error_code.as_deref());
        tx.execute(
            "INSERT INTO environment(guid, name, representation, parse_status, parse_error_code, parse_error_text) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                guid,
                environment.name,
                environment.representation,
                status,
                error_code,
                error
            ],
        )?;
        if let Some(resolution) = resolution {
            for token in &resolution.class_tokens {
                let result = if token.natural_class_guid.is_some() {
                    "resolved"
                } else {
                    "unresolved"
                };
                tx.execute(
                    "INSERT INTO environment_natural_class(environment_guid, compile_context_key, side, token_path, token_text, source_start, source_end, natural_class_guid, result) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        guid,
                        COMPILE_CONTEXT,
                        token.side,
                        token.token_path,
                        token.token_text,
                        token.source_start as i64,
                        token.source_end as i64,
                        token.natural_class_guid,
                        result
                    ],
                )?;
            }
        }
    }
    Ok(())
}

fn insert_allomorph_environments(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
) -> Result<(), FactsError> {
    for entry in &snapshot.lexicon.entries {
        for allomorph in &entry.allomorphs {
            let allomorph_guid = checked_guid(&allomorph.guid, "allomorph.guid")?;
            for (role, environments) in [
                ("phone", &allomorph.environments),
                ("position", &allomorph.positions),
            ] {
                for (ordinal, environment_guid) in environments.iter().enumerate() {
                    tx.execute(
                        "INSERT INTO allomorph_environment(allomorph_guid, role, ordinal, environment_guid) VALUES (?1, ?2, ?3, ?4)",
                        params![allomorph_guid, role, ordinal as i64, environment_guid],
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn insert_environment_usage(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    resolutions: &[EnvironmentResolution],
    decisions: &[LoadDecision],
) -> Result<(), FactsError> {
    let resolved: BTreeMap<_, _> = resolutions
        .iter()
        .map(|resolution| (resolution.environment_guid.as_str(), resolution))
        .collect();
    let environments: BTreeSet<_> = snapshot
        .phonology
        .environments
        .iter()
        .map(|environment| environment.guid.as_str())
        .collect();
    for entry in &snapshot.lexicon.entries {
        for allomorph in &entry.allomorphs {
            for (role, guids) in [
                ("phone", &allomorph.environments),
                ("position", &allomorph.positions),
            ] {
                for (ordinal, environment_guid) in guids.iter().enumerate() {
                    let attachment = InventoryKey::attachment(
                        InventoryKind::Environment,
                        allomorph.guid.clone(),
                        environment_guid.clone(),
                        "environment",
                    );
                    let decision = latest_decision(decisions, &attachment);
                    let owner_key =
                        InventoryKey::object(InventoryKind::Allomorph, allomorph.guid.clone());
                    let owner_decision = latest_decision(decisions, &owner_key);
                    let owner_loaded = owner_decision
                        .and_then(|value| value.loaded)
                        .or_else(|| inventory_owner_loaded(&owner_key, decisions));
                    let outcome = environment_usage_outcome(
                        environments.contains(environment_guid.as_str()),
                        resolved.get(environment_guid.as_str()).copied(),
                        decision,
                        owner_loaded,
                    )?;
                    let link = decision.map(load_decision_link).transpose()?;
                    tx.execute(
                        "INSERT INTO environment_usage(allomorph_guid, role, ordinal, compile_context_key, environment_guid, resolved_environment_guid, compiled, result, load_subject_kind, load_subject_key, load_pipeline_stage, load_context_key, load_decision_ordinal) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                        params![
                            allomorph.guid,
                            role,
                            ordinal as i64,
                            COMPILE_CONTEXT,
                            environment_guid,
                            if environments.contains(environment_guid.as_str()) {
                                Some(environment_guid.as_str())
                            } else {
                                None
                            },
                            if outcome.compiled { 1_i64 } else { 0_i64 },
                            outcome.result,
                            link.as_ref().map(|value| value.0.as_str()),
                            link.as_ref().map(|value| value.1.as_str()),
                            link.as_ref().map(|value| value.2.as_str()),
                            link.as_ref().map(|value| value.3.as_str()),
                            link.as_ref().map(|value| value.4),
                        ],
                    )?;
                }
            }
        }
    }
    Ok(())
}

struct EnvironmentUsageOutcome {
    result: &'static str,
    compiled: bool,
}

fn environment_usage_outcome(
    environment_exists: bool,
    resolution: Option<&EnvironmentResolution>,
    decision: Option<&LoadDecision>,
    owner_loaded: Option<bool>,
) -> Result<EnvironmentUsageOutcome, FactsError> {
    if decision.is_none() && owner_loaded == Some(false) {
        return Ok(EnvironmentUsageOutcome {
            result: "owner_not_loaded",
            compiled: false,
        });
    }
    if !environment_exists {
        return Ok(EnvironmentUsageOutcome {
            result: if decision.is_some() {
                "unresolved"
            } else {
                "not_attempted"
            },
            compiled: false,
        });
    }
    if resolution.is_some_and(|value| value.status == EnvironmentResolutionStatus::Invalid) {
        return match decision.map(|value| value.disposition) {
            Some(LoadDisposition::Rejected) => Ok(EnvironmentUsageOutcome {
                result: "invalid",
                compiled: false,
            }),
            Some(disposition) => Err(FactsError::Integrity(format!(
                "environment {} parsed invalid but compiler decision was {}",
                resolution.map_or("<missing>", |value| value.environment_guid.as_str()),
                disposition.as_str()
            ))),
            None => Err(FactsError::Integrity(format!(
                "environment {} parsed invalid without a compiler rejection decision",
                resolution.map_or("<missing>", |value| value.environment_guid.as_str())
            ))),
        };
    }
    match decision.map(|value| value.disposition) {
        Some(LoadDisposition::Rejected) => Ok(EnvironmentUsageOutcome {
            result: "invalid",
            compiled: false,
        }),
        Some(LoadDisposition::Represented) if owner_loaded != Some(false) => {
            Ok(EnvironmentUsageOutcome {
                result: "represented",
                compiled: true,
            })
        }
        Some(_) if owner_loaded == Some(false) => Ok(EnvironmentUsageOutcome {
            result: "owner_not_loaded",
            compiled: false,
        }),
        Some(_) => Ok(EnvironmentUsageOutcome {
            result: "not_attempted",
            compiled: false,
        }),
        None => Ok(EnvironmentUsageOutcome {
            result: "not_attempted",
            compiled: false,
        }),
    }
}

fn latest_decision<'a>(
    decisions: &'a [LoadDecision],
    subject: &InventoryKey,
) -> Option<&'a LoadDecision> {
    decisions
        .iter()
        .filter(|decision| {
            decision.subject == *subject && decision.pipeline_stage == LoadPipelineStage::Compile
        })
        .max_by_key(|decision| decision.decision_ordinal)
}

fn inventory_owner_loaded(subject: &InventoryKey, decisions: &[LoadDecision]) -> Option<bool> {
    let value = decisions
        .iter()
        .filter(|decision| decision.subject == *subject)
        .max_by_key(|decision| decision.decision_ordinal)?;
    value.loaded
}

type LoadDecisionLink = (String, String, String, String, i64);

fn load_decision_link(decision: &LoadDecision) -> Result<LoadDecisionLink, FactsError> {
    let (kind, key, _) = subject_key(&decision.subject)?;
    Ok((
        kind,
        key,
        "compile".into(),
        decision.context_key.clone(),
        i64::from(decision.decision_ordinal),
    ))
}

/// Canonical source GUIDs of the phonemes and boundary markers, to match a definition to its source.
pub(crate) struct SourceGuids {
    phonemes: BTreeSet<String>,
    boundaries: BTreeSet<String>,
}

impl SourceGuids {
    pub(crate) fn new(snapshot: &Snapshot) -> Self {
        Self {
            phonemes: snapshot
                .phonology
                .phonemes
                .iter()
                .map(|phoneme| canonical_key(&phoneme.guid))
                .collect(),
            boundaries: snapshot
                .phonology
                .boundary_markers
                .iter()
                .map(|boundary| canonical_key(&boundary.guid))
                .collect(),
        }
    }
}

/// How one character definition is keyed: its kind and the member key shared across tables.
pub(crate) struct SegmentIdentity {
    pub(crate) member_kind: &'static str,
    pub(crate) member_key: String,
    pub(crate) phoneme_guid: Option<String>,
    pub(crate) boundary_guid: Option<String>,
}

impl SegmentIdentity {
    fn word_boundary() -> Result<Self, FactsError> {
        Ok(Self {
            member_kind: "boundary",
            member_key: serde_json::to_string(&serde_json::json!({"kind":"word_boundary"}))
                .map_err(|error| FactsError::Serialization(error.to_string()))?,
            phoneme_guid: None,
            boundary_guid: None,
        })
    }
}

pub(crate) fn segment_identity(
    definition: &CharDef,
    sources: &SourceGuids,
) -> Result<SegmentIdentity, FactsError> {
    if let Some(guid) = definition.source_guid().map(canonical_key) {
        match definition.kind() {
            CharDefKind::Segment if sources.phonemes.contains(&guid) => {
                return Ok(SegmentIdentity {
                    member_kind: "phoneme",
                    member_key: object_identity(&guid)?,
                    phoneme_guid: Some(guid),
                    boundary_guid: None,
                })
            }
            CharDefKind::Boundary if sources.boundaries.contains(&guid) => {
                return Ok(SegmentIdentity {
                    member_kind: "boundary",
                    member_key: object_identity(&guid)?,
                    phoneme_guid: None,
                    boundary_guid: Some(guid),
                })
            }
            _ => {}
        }
    }
    Ok(SegmentIdentity {
        member_kind: "synthetic",
        member_key: synthetic_identity(definition.xml_id())?,
        phoneme_guid: None,
        boundary_guid: None,
    })
}

fn natural_class_def(
    grammar: &pg_grammar::model::Grammar,
    class: pg_grammar::model::NatClassId,
) -> Result<&pg_grammar::model::NaturalClass, FactsError> {
    grammar
        .natural_classes
        .get(class.0 as usize)
        .ok_or_else(|| {
            FactsError::Serialization(format!("natural class index {} is not defined", class.0))
        })
}

fn insert_effective_members(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    grammar: Option<&pg_grammar::model::Grammar>,
) -> Result<(), FactsError> {
    let Some(grammar) = grammar else {
        return Ok(());
    };
    let authored: BTreeSet<_> = snapshot
        .phonology
        .natural_classes
        .iter()
        .map(natural_class_guid)
        .collect();
    let sources = SourceGuids::new(snapshot);
    for class in &grammar.natural_classes {
        if !authored.contains(class.xml_id.as_str()) {
            continue;
        }
        for table in &grammar.char_tables {
            let effective_members: BTreeSet<_> =
                pg_grammar::segment::nat_class_member_ids(table, class)
                    .into_iter()
                    .collect();
            for (char_id, definition) in table.iter() {
                if !effective_members.contains(&char_id) {
                    continue;
                }
                let identity = segment_identity(definition, &sources)?;
                let identity_quality = if identity.member_kind == "synthetic" {
                    "synthetic"
                } else {
                    "sourceGuid"
                };
                tx.execute(
                    "INSERT INTO natural_class_effective_member(natural_class_guid, table_key, member_key, phoneme_guid, identity_quality, match_kind, match_basis) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        class.xml_id,
                        table.xml_id(),
                        identity.member_key,
                        identity.phoneme_guid,
                        identity_quality,
                        match_kind(class),
                        match_basis(class, definition, grammar),
                    ],
                )?;
            }
        }
    }
    Ok(())
}

/// Why a member is in its class: listed by a segment list, specified by its features, or underspecified.
fn match_basis(
    class: &pg_grammar::model::NaturalClass,
    definition: &CharDef,
    grammar: &pg_grammar::model::Grammar,
) -> &'static str {
    match &class.kind {
        CompiledNaturalClassKind::Segments(_) => "listed",
        CompiledNaturalClassKind::Feature(pairs) => {
            let lanes = definition.feature_lanes();
            let defaulted = pairs.iter().any(|&(feature, _)| {
                lanes[feature.0 as usize] == grammar.phon_features.mask(feature)
            });
            if defaulted {
                "underspecified"
            } else {
                "specified"
            }
        }
    }
}

/// One row per valid environment side. A refused compile has no grammar, so it publishes none.
fn insert_environment_sides(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    resolutions: &[EnvironmentResolution],
    grammar: Option<&pg_grammar::model::Grammar>,
) -> Result<(), FactsError> {
    let Some(grammar) = grammar else {
        return Ok(());
    };
    let sources = SourceGuids::new(snapshot);
    for resolution in resolutions
        .iter()
        .filter(|resolution| resolution.status == EnvironmentResolutionStatus::Valid)
    {
        let environment = checked_guid(&resolution.environment_guid, "phonology.environment.guid")?;
        let table = grammar
            .char_tables
            .get(resolution.table.0 as usize)
            .ok_or_else(|| {
                FactsError::Serialization(format!(
                    "environment {environment} resolves against a missing character table"
                ))
            })?;
        for (side, pattern) in [("left", &resolution.left), ("right", &resolution.right)] {
            let elements = environment_side_elements(pattern.as_ref());
            let tokens = elements
                .iter()
                .map(|element| side_token(element, table, grammar, &sources))
                .collect::<Result<Vec<_>, _>>()?;
            let encoded = serde_json::to_string(&tokens)
                .map_err(|error| FactsError::Serialization(error.to_string()))?;
            let shape = side_shape(&elements);
            tx.execute(
                "INSERT INTO environment_side(environment_guid, side, canonical_key, shape) VALUES (?1, ?2, ?3, ?4)",
                params![
                    environment,
                    side,
                    pg_assess::sha256_bytes(encoded.as_bytes()).trim_start_matches("sha256:"),
                    shape,
                ],
            )?;
            if matches!(shape, "single_segment" | "word_boundary") {
                for member in side_members(&elements, table, grammar, &sources)? {
                    tx.execute(
                        "INSERT INTO environment_side_member(environment_guid, side, member_kind, member_key, phoneme_guid, boundary_guid) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            environment,
                            side,
                            member.member_kind,
                            member.member_key,
                            member.phoneme_guid,
                            member.boundary_guid,
                        ],
                    )?;
                }
            }
        }
    }
    Ok(())
}

/// The canonical token for one resolved element; a class is its sorted member keys, never its name.
fn side_token(
    element: &EnvironmentSideElement,
    table: &CharDefTable,
    grammar: &pg_grammar::model::Grammar,
    sources: &SourceGuids,
) -> Result<serde_json::Value, FactsError> {
    Ok(match element {
        EnvironmentSideElement::WordBoundary => serde_json::json!("word_boundary"),
        EnvironmentSideElement::Segment(id) => {
            let definition = table.get(*id);
            let key = segment_identity(definition, sources)?.member_key;
            match definition.kind() {
                CharDefKind::Boundary => serde_json::json!({ "boundary": key }),
                CharDefKind::Segment => serde_json::json!({ "segment": key }),
            }
        }
        EnvironmentSideElement::NaturalClass(class) => {
            let keys = class_member_keys(table, grammar, *class, sources)?;
            serde_json::json!({ "class": keys })
        }
        EnvironmentSideElement::Optional { min, max, children } => {
            let children = children
                .iter()
                .map(|child| side_token(child, table, grammar, sources))
                .collect::<Result<Vec<_>, _>>()?;
            serde_json::json!({ "optional": { "min": min, "max": max, "children": children } })
        }
    })
}

fn class_member_keys(
    table: &CharDefTable,
    grammar: &pg_grammar::model::Grammar,
    class: pg_grammar::model::NatClassId,
    sources: &SourceGuids,
) -> Result<Vec<String>, FactsError> {
    let definition = natural_class_def(grammar, class)?;
    let keys = pg_grammar::segment::nat_class_member_ids(table, definition)
        .into_iter()
        .map(|id| segment_identity(table.get(id), sources).map(|identity| identity.member_key))
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(keys.into_iter().collect())
}

/// `single_segment`: one phoneme or class with at most one `#`; `word_boundary`: a lone `#`.
fn side_shape(elements: &[EnvironmentSideElement]) -> &'static str {
    if elements.is_empty() {
        return "empty";
    }
    let boundaries = elements
        .iter()
        .filter(|element| **element == EnvironmentSideElement::WordBoundary)
        .count();
    if boundaries == elements.len() {
        return if boundaries == 1 {
            "word_boundary"
        } else {
            "complex"
        };
    }
    let phonemic: Vec<_> = elements
        .iter()
        .filter(|element| **element != EnvironmentSideElement::WordBoundary)
        .collect();
    match phonemic.as_slice() {
        [EnvironmentSideElement::Segment(_) | EnvironmentSideElement::NaturalClass(_)]
            if boundaries <= 1 =>
        {
            "single_segment"
        }
        _ => "complex",
    }
}

/// The member rows of a `single_segment` or `word_boundary` side; a side's `#` is a member too.
fn side_members(
    elements: &[EnvironmentSideElement],
    table: &CharDefTable,
    grammar: &pg_grammar::model::Grammar,
    sources: &SourceGuids,
) -> Result<Vec<SegmentIdentity>, FactsError> {
    let mut members = BTreeMap::new();
    for element in elements {
        let identities = match element {
            EnvironmentSideElement::WordBoundary => vec![SegmentIdentity::word_boundary()?],
            EnvironmentSideElement::Segment(id) => vec![segment_identity(table.get(*id), sources)?],
            EnvironmentSideElement::NaturalClass(class) => {
                let definition = natural_class_def(grammar, *class)?;
                pg_grammar::segment::nat_class_member_ids(table, definition)
                    .into_iter()
                    .map(|id| segment_identity(table.get(id), sources))
                    .collect::<Result<Vec<_>, _>>()?
            }
            EnvironmentSideElement::Optional { .. } => Vec::new(),
        };
        for identity in identities {
            members.insert(identity.member_key.clone(), identity);
        }
    }
    Ok(members.into_values().collect())
}

fn match_kind(class: &pg_grammar::model::NaturalClass) -> &'static str {
    match &class.kind {
        CompiledNaturalClassKind::Segments(_) => "segments",
        CompiledNaturalClassKind::Feature(_) => "features",
    }
}

fn object_identity(guid: &str) -> Result<String, FactsError> {
    serde_json::to_string(&serde_json::json!({"kind":"object", "guid":guid}))
        .map_err(|error| FactsError::Serialization(error.to_string()))
}

fn synthetic_identity(key: &str) -> Result<String, FactsError> {
    serde_json::to_string(&serde_json::json!({"kind":"synthetic", "key":key}))
        .map_err(|error| FactsError::Serialization(error.to_string()))
}

fn insert_phonological_rules(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    grammar: Option<&pg_grammar::model::Grammar>,
) -> Result<(), FactsError> {
    let strata = grammar.map(effective_strata).unwrap_or_default();
    if let Some(grammar) = grammar {
        for (stratum_ordinal, stratum) in grammar.strata.iter().enumerate() {
            let key = stratum_key(stratum_ordinal);
            tx.execute(
                "INSERT INTO stratum(stratum_key, ordinal, name, table_key) VALUES (?1, ?2, ?3, ?4)",
                params![key, stratum_ordinal as i64, stratum.name, format!("table#{}", stratum.table.0)],
            )?;
        }
    }
    for (ordinal, rule) in snapshot.phonology.rules.iter().enumerate() {
        let guid = checked_guid(phonological_rule_guid(rule), "phonology.rule.guid")?;
        let (kind, direction) = match rule {
            PhonologicalRule::Rewrite(rule) => ("rewrite", direction(rule.direction)),
            PhonologicalRule::Metathesis(rule) => ("metathesis", direction(rule.direction)),
        };
        let stratum = strata.get(&guid);
        let name = match rule {
            PhonologicalRule::Rewrite(rule) => &rule.name,
            PhonologicalRule::Metathesis(rule) => &rule.name,
        };
        tx.execute(
            "INSERT INTO phonological_rule(guid, name, kind, direction, order_index, effective_stratum_key) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![guid, name, kind, direction, ordinal as i64, stratum],
        )?;
        if let PhonologicalRule::Rewrite(rewrite) = rule {
            for (variable_ordinal, feature_constraint_guid) in
                rewrite.feature_constraint_variables.iter().enumerate()
            {
                tx.execute(
                    "INSERT INTO phonological_rule_variable(rule_guid, ordinal, feature_constraint_guid) VALUES (?1, ?2, ?3)",
                    params![guid, variable_ordinal as i64, feature_constraint_guid],
                )?;
            }
            for (rhs_ordinal, rhs) in rewrite.right_hand_sides.iter().enumerate() {
                tx.execute(
                    "INSERT INTO rewrite_rhs(rule_guid, ordinal) VALUES (?1, ?2)",
                    params![guid, rhs_ordinal as i64],
                )?;
                for (pos_ordinal, pos_guid) in rhs.required_parts_of_speech.iter().enumerate() {
                    tx.execute(
                        "INSERT INTO rewrite_rhs_pos(rule_guid, rhs_ordinal, ordinal, category_guid) VALUES (?1, ?2, ?3, ?4)",
                        params![guid, rhs_ordinal as i64, pos_ordinal as i64, pos_guid],
                    )?;
                }
                for (polarity, targets) in [
                    ("required", &rhs.required_rule_features),
                    ("excluded", &rhs.excluded_rule_features),
                ] {
                    for (target_ordinal, target_guid) in targets.iter().enumerate() {
                        tx.execute(
                            "INSERT INTO rewrite_rhs_rule_feature(rule_guid, rhs_ordinal, polarity, ordinal, target_guid, target_kind) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            params![
                                guid,
                                rhs_ordinal as i64,
                                polarity,
                                target_ordinal as i64,
                                target_guid,
                                rule_feature_target_kind(snapshot, target_guid)
                            ],
                        )?;
                    }
                }
            }
        }
    }
    if let Some(grammar) = grammar {
        for (stratum_ordinal, stratum) in grammar.strata.iter().enumerate() {
            let key = stratum_key(stratum_ordinal);
            for (rule_ordinal, id) in stratum.prules.iter().enumerate() {
                let Some(PhonRuleDef::Rewrite(compiled)) = grammar.prules.get(id.0 as usize) else {
                    continue;
                };
                tx.execute(
                    "INSERT INTO rule_stratum(rule_guid, stratum_key, ordinal) VALUES (?1, ?2, ?3)",
                    params![canonical_key(&compiled.xml_id), key, rule_ordinal as i64],
                )?;
            }
        }
    }
    Ok(())
}

fn effective_strata(grammar: &pg_grammar::model::Grammar) -> BTreeMap<String, String> {
    let mut strata = BTreeMap::new();
    for (stratum_ordinal, stratum) in grammar.strata.iter().enumerate() {
        let key = stratum_key(stratum_ordinal);
        for id in &stratum.prules {
            if let Some(PhonRuleDef::Rewrite(rule)) = grammar.prules.get(id.0 as usize) {
                strata.insert(canonical_key(&rule.xml_id), key.clone());
            }
        }
    }
    strata
}

/// Map keys are canonical GUIDs, because lookups use `checked_guid`'s lowercase form.
pub(crate) fn canonical_key(raw: &str) -> String {
    pg_snapshot::canonical_guid(raw).unwrap_or_else(|| raw.to_string())
}

fn stratum_key(ordinal: usize) -> String {
    format!("stratum#{ordinal}")
}

fn rule_feature_target_kind(snapshot: &Snapshot, target_guid: &str) -> &'static str {
    if has_inflection_class(&snapshot.morphology.parts_of_speech, target_guid) {
        return "inflectionClass";
    }
    if snapshot
        .morphology
        .exception_features
        .iter()
        .any(|feature| feature.guid == target_guid)
    {
        return "exceptionFeature";
    }
    if snapshot
        .feature_systems
        .phonological
        .closed_features
        .iter()
        .flat_map(|feature| &feature.values)
        .chain(
            snapshot
                .feature_systems
                .morphosyntactic
                .closed_features
                .iter()
                .flat_map(|feature| &feature.values),
        )
        .any(|value| value.guid == target_guid)
    {
        return "featureValue";
    }
    "unresolved"
}

fn has_inflection_class(
    categories: &[pg_snapshot::morphology::PartOfSpeech],
    target_guid: &str,
) -> bool {
    categories.iter().any(|category| {
        category
            .inflection_classes
            .iter()
            .any(|class| has_class(class, target_guid))
            || has_inflection_class(&category.children, target_guid)
    })
}

fn has_class(class: &pg_snapshot::morphology::InflectionClass, target_guid: &str) -> bool {
    class.guid == target_guid
        || class
            .children
            .iter()
            .any(|child| has_class(child, target_guid))
}

fn insert_environment_patterns(
    tx: &Transaction<'_>,
    resolutions: &[EnvironmentResolution],
    grammar: Option<&pg_grammar::model::Grammar>,
) -> Result<(), FactsError> {
    for resolution in resolutions {
        if resolution.status != EnvironmentResolutionStatus::Valid {
            continue;
        }
        let class_ids: BTreeMap<_, _> = resolution
            .class_tokens
            .iter()
            .filter_map(|token| {
                token
                    .natural_class_index
                    .zip(token.natural_class_guid.as_ref())
                    .map(|(id, guid)| (id, guid.clone()))
            })
            .collect();
        for (side, role, pattern) in [
            ("left", "environment_left", resolution.left.as_ref()),
            ("right", "environment_right", resolution.right.as_ref()),
        ] {
            let Some(pattern) = pattern else {
                continue;
            };
            let root_id = insert_pattern_root(
                tx,
                "environment",
                &resolution.environment_guid,
                role,
                0,
                "resolved_environment",
            )?;
            insert_compiled_pattern(
                tx,
                root_id,
                pattern,
                CompiledPatternContext {
                    class_ids: &class_ids,
                    grammar,
                    side,
                },
            )?;
        }
    }
    Ok(())
}

fn insert_compiled_pattern(
    tx: &Transaction<'_>,
    root_id: i64,
    pattern: &Pattern,
    writer: CompiledPatternContext<'_>,
) -> Result<(), FactsError> {
    for (ordinal, node) in pattern.nodes.iter().enumerate() {
        insert_compiled_node(tx, root_id, None, ordinal, node, &writer)?;
    }
    Ok(())
}

struct CompiledPatternContext<'a> {
    class_ids: &'a BTreeMap<u32, String>,
    grammar: Option<&'a pg_grammar::model::Grammar>,
    side: &'a str,
}

fn insert_compiled_node(
    tx: &Transaction<'_>,
    root_id: i64,
    parent_node_id: Option<i64>,
    ordinal: usize,
    node: &PatternNode,
    writer: &CompiledPatternContext<'_>,
) -> Result<i64, FactsError> {
    let mut row = PatternNodeRow::new(root_id, parent_node_id, ordinal);
    match node {
        PatternNode::Context(context) => {
            row.kind = "naturalClass";
            row.natural_class_guid = Some(
                writer
                    .class_ids
                    .get(&context.nat_class.0)
                    .ok_or_else(|| {
                        FactsError::Serialization(format!(
                            "resolved environment {} pattern has no source class for compiled class {}",
                            writer.side,
                            context.nat_class.0
                        ))
                    })?
                    .clone(),
            );
        }
        PatternNode::CharDef(id) => {
            row.kind = "characterDefinition";
            if let Some(definition) = writer
                .grammar
                .and_then(|value| value.char_tables.first())
                .map(|table| table.get(*id))
            {
                match definition.kind() {
                    CharDefKind::Segment => {
                        row.phoneme_guid = definition.source_guid().map(str::to_string)
                    }
                    CharDefKind::Boundary => {
                        row.boundary_guid = definition.source_guid().map(str::to_string)
                    }
                }
                row.token_text = Some(definition.xml_id().to_string());
            }
        }
        PatternNode::Quantifier {
            min,
            max,
            children: _,
        } => {
            row.kind = "quantifier";
            row.min = Some(i64::from(*min));
            row.max = max.map(i64::from);
        }
        PatternNode::Segments { shape, .. } => {
            row.kind = "literalSegments";
            row.token_text = Some(shape.text.clone());
        }
        PatternNode::Anchor(anchor) => {
            row.kind = match anchor {
                pg_grammar::model::AnchorSide::Left => "leftAnchor",
                pg_grammar::model::AnchorSide::Right => "rightAnchor",
            };
        }
    }
    let node_id = insert_pattern_node(tx, row)?;
    if let PatternNode::Quantifier { children, .. } = node {
        for (child_ordinal, child) in children.iter().enumerate() {
            insert_compiled_node(tx, root_id, Some(node_id), child_ordinal, child, writer)?;
        }
    }
    if let PatternNode::Segments { table, shape } = node {
        if let Some(char_table) = writer
            .grammar
            .and_then(|grammar| grammar.char_tables.get(usize::from(table.0)))
        {
            for (child_ordinal, (_, _, char_def_id, _)) in shape.shape.interior().enumerate() {
                let definition = char_table.get(pg_grammar::chardef::CharDefId(char_def_id));
                let mut child = PatternNodeRow::new(root_id, Some(node_id), child_ordinal);
                match definition.kind() {
                    CharDefKind::Segment => {
                        child.kind = "phoneme";
                        child.phoneme_guid = definition.source_guid().map(str::to_string);
                    }
                    CharDefKind::Boundary => {
                        child.kind = "boundary";
                        child.boundary_guid = definition.source_guid().map(str::to_string);
                    }
                }
                child.token_text = Some(definition.xml_id().to_string());
                insert_pattern_node(tx, child)?;
            }
        }
    }
    Ok(node_id)
}

fn insert_rule_patterns(tx: &Transaction<'_>, snapshot: &Snapshot) -> Result<(), FactsError> {
    for rule in &snapshot.phonology.rules {
        match rule {
            PhonologicalRule::Rewrite(rule) => {
                let guid = checked_guid(&rule.guid, "phonology.rule.guid")?;
                let lhs = insert_pattern_root(
                    tx,
                    "phonologicalRule",
                    &guid,
                    "rewrite_lhs",
                    0,
                    "authored",
                )?;
                insert_contexts(tx, lhs, &rule.structural_description)?;
                for (ordinal, rhs) in rule.right_hand_sides.iter().enumerate() {
                    let change = insert_pattern_root(
                        tx,
                        "phonologicalRule",
                        &guid,
                        "rewrite_sc",
                        ordinal,
                        "authored",
                    )?;
                    insert_contexts(tx, change, &rhs.structural_change)?;
                    link_rhs_root(tx, &guid, ordinal, "change_root_id", change)?;
                    if let Some(context) = &rhs.left_context {
                        let root = insert_pattern_root(
                            tx,
                            "phonologicalRule",
                            &guid,
                            "rewrite_left_context",
                            ordinal,
                            "authored",
                        )?;
                        insert_context(tx, root, None, 0, context)?;
                        link_rhs_root(tx, &guid, ordinal, "left_context_root_id", root)?;
                    }
                    if let Some(context) = &rhs.right_context {
                        let root = insert_pattern_root(
                            tx,
                            "phonologicalRule",
                            &guid,
                            "rewrite_right_context",
                            ordinal,
                            "authored",
                        )?;
                        insert_context(tx, root, None, 0, context)?;
                        link_rhs_root(tx, &guid, ordinal, "right_context_root_id", root)?;
                    }
                }
            }
            PhonologicalRule::Metathesis(rule) => {
                let guid = checked_guid(&rule.guid, "phonology.rule.guid")?;
                let root = insert_pattern_root(
                    tx,
                    "phonologicalRule",
                    &guid,
                    "metathesis_pattern",
                    0,
                    "authored",
                )?;
                insert_contexts(tx, root, &rule.structural_description)?;
            }
        }
    }
    Ok(())
}

/// The column is a literal from this module, never input, so formatting it into SQL is safe.
fn link_rhs_root(
    tx: &Transaction<'_>,
    rule_guid: &str,
    rhs_ordinal: usize,
    column: &str,
    root_id: i64,
) -> Result<(), FactsError> {
    tx.execute(
        &format!("UPDATE rewrite_rhs SET {column} = ?1 WHERE rule_guid = ?2 AND ordinal = ?3"),
        params![root_id, rule_guid, rhs_ordinal as i64],
    )?;
    Ok(())
}

fn insert_contexts(
    tx: &Transaction<'_>,
    root_id: i64,
    contexts: &[PhonContext],
) -> Result<(), FactsError> {
    for (ordinal, context) in contexts.iter().enumerate() {
        insert_context(tx, root_id, None, ordinal, context)?;
    }
    Ok(())
}

pub(crate) fn insert_context(
    tx: &Transaction<'_>,
    root_id: i64,
    parent_node_id: Option<i64>,
    ordinal: usize,
    context: &PhonContext,
) -> Result<i64, FactsError> {
    let mut row = PatternNodeRow::new(root_id, parent_node_id, ordinal);
    match context {
        PhonContext::Sequence { members } => {
            row.kind = "sequence";
            let node_id = insert_pattern_node(tx, row)?;
            for (child_ordinal, child) in members.iter().enumerate() {
                insert_context(tx, root_id, Some(node_id), child_ordinal, child)?;
            }
            return Ok(node_id);
        }
        PhonContext::Iteration { min, max, member } => {
            row.kind = "iteration";
            row.min = Some(i64::from(*min));
            row.max = (*max >= 0).then_some(i64::from(*max));
            let node_id = insert_pattern_node(tx, row)?;
            insert_context(tx, root_id, Some(node_id), 0, member)?;
            return Ok(node_id);
        }
        PhonContext::Segment { phoneme } => {
            row.kind = "phoneme";
            row.phoneme_guid = Some(phoneme.clone());
        }
        PhonContext::NaturalClass {
            natural_class,
            plus_variables,
            minus_variables,
        } => {
            row.kind = "naturalClass";
            row.natural_class_guid = Some(natural_class.clone());
            let node_id = insert_pattern_node(tx, row)?;
            insert_variables(tx, node_id, "plus", plus_variables)?;
            insert_variables(tx, node_id, "minus", minus_variables)?;
            return Ok(node_id);
        }
        PhonContext::Boundary { marker } => {
            row.kind = "boundary";
            row.boundary_guid = Some(marker.clone());
        }
        PhonContext::WordBoundary => row.kind = "wordBoundary",
        PhonContext::Variable => row.kind = "variable",
    }
    insert_pattern_node(tx, row)
}

fn insert_variables(
    tx: &Transaction<'_>,
    node_id: i64,
    polarity: &str,
    variables: &[String],
) -> Result<(), FactsError> {
    for (ordinal, feature_constraint_guid) in variables.iter().enumerate() {
        tx.execute(
            "INSERT INTO pattern_variable(node_id, polarity, ordinal, feature_constraint_guid) VALUES (?1, ?2, ?3, ?4)",
            params![node_id, polarity, ordinal as i64, feature_constraint_guid],
        )?;
    }
    Ok(())
}

struct PatternNodeRow<'a> {
    root_id: i64,
    parent_node_id: Option<i64>,
    ordinal: usize,
    kind: &'a str,
    min: Option<i64>,
    max: Option<i64>,
    phoneme_guid: Option<String>,
    natural_class_guid: Option<String>,
    boundary_guid: Option<String>,
    token_text: Option<String>,
}

impl<'a> PatternNodeRow<'a> {
    fn new(root_id: i64, parent_node_id: Option<i64>, ordinal: usize) -> Self {
        Self {
            root_id,
            parent_node_id,
            ordinal,
            kind: "sequence",
            min: None,
            max: None,
            phoneme_guid: None,
            natural_class_guid: None,
            boundary_guid: None,
            token_text: None,
        }
    }
}

fn insert_pattern_node(tx: &Transaction<'_>, row: PatternNodeRow<'_>) -> Result<i64, FactsError> {
    tx.execute(
        "INSERT INTO pattern_node(root_id, parent_node_id, ordinal, kind, min, max, phoneme_guid, natural_class_guid, boundary_guid, token_text) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            row.root_id,
            row.parent_node_id,
            row.ordinal as i64,
            row.kind,
            row.min,
            row.max,
            row.phoneme_guid,
            row.natural_class_guid,
            row.boundary_guid,
            row.token_text
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

pub(crate) fn insert_pattern_root(
    tx: &Transaction<'_>,
    owner_kind: &str,
    owner_guid: &str,
    role: &str,
    ordinal: usize,
    source_kind: &str,
) -> Result<i64, FactsError> {
    tx.execute(
        "INSERT INTO pattern_root(owner_kind, owner_guid, role, ordinal, source_kind) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![owner_kind, owner_guid, role, ordinal as i64, source_kind],
    )?;
    Ok(tx.last_insert_rowid())
}

fn natural_class_guid(class: &NaturalClass) -> &str {
    match class {
        NaturalClass::Segments { guid, .. } | NaturalClass::Features { guid, .. } => guid,
    }
}

fn phonological_rule_guid(rule: &PhonologicalRule) -> &str {
    match rule {
        PhonologicalRule::Rewrite(rule) => &rule.guid,
        PhonologicalRule::Metathesis(rule) => &rule.guid,
    }
}

fn direction(direction: RuleDirection) -> &'static str {
    match direction {
        RuleDirection::LeftToRight => "leftToRight",
        RuleDirection::RightToLeft => "rightToLeft",
        RuleDirection::Simultaneous => "simultaneous",
    }
}
