//! Renders opt-in rich trace-details v2 envelopes.

use std::time::Duration;

use pg_grammar::model::Grammar;
use pg_parse::{project_parse_analysis, ParseOutcome, ParseProjectionError};
use pg_rules::stats::{self, ObjectKind, StatsRow};
use pg_rules::trace::{FailureContext, TraceHandle, TraceSource, TraceType, TreeTraceSink};
use pg_rules::word::{RuntimeRoot, Word};
use pg_snapshot::{AffixSlot, InflectionClass, Msa, PartOfSpeech, Snapshot};
use serde_json::{json, Value};
#[derive(Clone, Debug)]
pub struct TraceMetadata {
    pub grammar_name: Option<String>,
    pub grammar_hash: Option<String>,
    pub grammar_hash_semantics: Option<String>,
    pub source_kind: String,
    pub project_name: Option<String>,
    pub vernacular_writing_systems: Vec<String>,
    pub analysis_writing_systems: Vec<String>,
    pub snapshot: Option<Snapshot>,
}

impl Default for TraceMetadata {
    fn default() -> Self {
        Self {
            grammar_name: None,
            grammar_hash: None,
            grammar_hash_semantics: None,
            source_kind: "unknown".to_string(),
            project_name: None,
            vernacular_writing_systems: Vec::new(),
            analysis_writing_systems: Vec::new(),
            snapshot: None,
        }
    }
}
pub fn metadata_from_snapshot(snapshot: &Snapshot, source_kind: &str) -> TraceMetadata {
    TraceMetadata {
        grammar_name: Some(snapshot.project.name.clone()),
        grammar_hash: Some(snapshot.grammar_hash()),
        grammar_hash_semantics: Some("snapshot-semantic-sha256-v1".to_string()),
        source_kind: source_kind.to_string(),
        project_name: Some(snapshot.project.name.clone()),
        vernacular_writing_systems: snapshot.project.vernacular_writing_systems.clone(),
        analysis_writing_systems: snapshot.project.analysis_writing_systems.clone(),
        snapshot: Some(snapshot.clone()),
    }
}
pub fn metadata_from_xml(grammar: &Grammar, hash: String) -> TraceMetadata {
    TraceMetadata {
        grammar_name: grammar.name.clone(),
        grammar_hash: Some(hash),
        grammar_hash_semantics: Some("source-bytes-sha256-v1".to_string()),
        source_kind: "xml".to_string(),
        ..TraceMetadata::default()
    }
}
pub fn validate_details(
    trace_requested: bool,
    trace_format: &str,
    details: bool,
    has_text_output_flags: bool,
) -> Result<(), String> {
    if details && !trace_requested {
        return Err("--trace-details requires --trace".into());
    }
    if details && trace_format != "json" {
        return Err("--trace-details requires --trace-format=json".into());
    }
    if details && has_text_output_flags {
        return Err("--trace-details cannot be combined with --gloss or --natural-gloss".into());
    }
    Ok(())
}

fn kind_name(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::MorphRule => "morphRule",
        ObjectKind::PhonRule => "phonRule",
        ObjectKind::LexEntry => "lexEntry",
        ObjectKind::RootIndex => "rootIndex",
        ObjectKind::Guesser => "guesser",
        ObjectKind::Overlay => "overlay",
    }
}

fn stats_json(rows: &[StatsRow]) -> Value {
    let kinds = [
        ObjectKind::MorphRule,
        ObjectKind::PhonRule,
        ObjectKind::LexEntry,
        ObjectKind::RootIndex,
        ObjectKind::Guesser,
        ObjectKind::Overlay,
    ];
    let categories = kinds.into_iter().map(|kind| {
        let counters = stats::summarize_kind(rows, kind);
        let timing_available = stats::self_time_supported(kind);
        (
            kind_name(kind).to_owned(),
            json!({
                "attempts": counters.attempts,
                "work": counters.work,
                "outputs": counters.outputs,
                "notApplied": counters.not_applied,
                "noRoot": counters.no_root,
                "surfaceMismatch": counters.surface_mismatch,
                "uses": counters.uses,
                "timingAvailable": timing_available,
                "selfElapsedNs": timing_available.then_some(counters.self_time_ns),
            }),
        )
    });
    serde_json::Map::from_iter(categories).into()
}

fn projection_error_code(error: &ParseProjectionError) -> String {
    format!("{error:?}")
}
fn analysis_id(index: usize) -> String {
    format!("analysis-{index}")
}

fn first_ws(forms: &[pg_snapshot::WsForm], source_id: Option<&str>) -> Value {
    forms.first().map_or(Value::Null, |form| {
        json!({
            "text": form.form,
            "writingSystem": form.ws,
            "sourceId": source_id,
        })
    })
}

fn find_pos<'a>(parts: &'a [PartOfSpeech], id: &str) -> Option<&'a PartOfSpeech> {
    parts.iter().find_map(|part| {
        (part.guid == id)
            .then_some(part)
            .or_else(|| find_pos(&part.children, id))
    })
}

fn find_infl<'a>(parts: &'a [PartOfSpeech], id: &str) -> Option<&'a InflectionClass> {
    parts.iter().find_map(|part| {
        part.inflection_classes
            .iter()
            .find_map(|class| {
                (class.guid == id)
                    .then_some(class)
                    .or_else(|| find_infl_class(&class.children, id))
            })
            .or_else(|| find_infl(&part.children, id))
    })
}

fn find_infl_class<'a>(classes: &'a [InflectionClass], id: &str) -> Option<&'a InflectionClass> {
    classes.iter().find_map(|class| {
        (class.guid == id)
            .then_some(class)
            .or_else(|| find_infl_class(&class.children, id))
    })
}

fn find_slot<'a>(parts: &'a [PartOfSpeech], id: &str) -> Option<&'a AffixSlot> {
    parts.iter().find_map(|part| {
        part.affix_slots
            .iter()
            .find_map(|slot| (slot.guid == id).then_some(slot))
            .or_else(|| find_slot(&part.children, id))
    })
}

fn pos_json(snapshot: &Snapshot, id: Option<&str>) -> Value {
    id.and_then(|id| find_pos(&snapshot.morphology.parts_of_speech, id))
        .map_or_else(
            || {
                id.map(|id| json!({ "id": id, "quality": "source-id-only" }))
                    .unwrap_or(Value::Null)
            },
            |pos| json!({ "id": pos.guid, "name": pos.name, "abbreviation": pos.abbreviation }),
        )
}

fn class_json(snapshot: &Snapshot, id: Option<&str>) -> Value {
    id.and_then(|id| find_infl(&snapshot.morphology.parts_of_speech, id)).map_or_else(
        || id.map(|id| json!({ "id": id, "quality": "source-id-only" })).unwrap_or(Value::Null),
        |class| json!({ "id": class.guid, "name": class.name, "abbreviation": class.abbreviation }),
    )
}

fn slots_json(snapshot: &Snapshot, ids: &[String]) -> Value {
    let values: Vec<Value> = ids
        .iter()
        .map(|id| {
            find_slot(&snapshot.morphology.parts_of_speech, id).map_or_else(
                || json!({ "id": id, "quality": "source-id-only" }),
                |slot| json!({ "id": slot.guid, "name": slot.name, "optional": slot.optional }),
            )
        })
        .collect();
    Value::Array(values)
}

fn features_json(features: Option<&pg_snapshot::FeatureStructure>) -> Value {
    features.map_or_else(
        || json!({ "status": "unavailable" }),
        |value| json!({ "status": "captured", "source": value }),
    )
}

fn msa_fields(snapshot: &Snapshot, msa_id: Option<&str>) -> Value {
    let Some(msa_id) = msa_id else {
        return Value::Null;
    };
    for entry in &snapshot.lexicon.entries {
        for msa in &entry.msas {
            if msa.guid() != msa_id {
                continue;
            }
            let mut object = serde_json::Map::new();
            object.insert("id".to_string(), json!(msa_id));
            match msa {
                Msa::Stem {
                    part_of_speech,
                    inflection_class,
                    features,
                    from_parts_of_speech,
                    slots,
                    ..
                } => {
                    let slot_ids: Vec<String> = slots.clone();
                    object.insert("kind".to_string(), json!("stem"));
                    object.insert(
                        "category".to_string(),
                        pos_json(snapshot, part_of_speech.as_deref()),
                    );
                    object.insert(
                        "slot".to_string(),
                        slots_json(snapshot, &slot_ids)
                            .as_array()
                            .and_then(|values| values.first())
                            .cloned()
                            .unwrap_or(Value::Null),
                    );
                    object.insert("slots".to_string(), slots_json(snapshot, &slot_ids));
                    object.insert(
                        "inflectionClass".to_string(),
                        class_json(snapshot, inflection_class.as_deref()),
                    );
                    object.insert("features".to_string(), features_json(features.as_ref()));
                    object.insert(
                        "attachesTo".to_string(),
                        json!({
                            "partOfSpeechIds": from_parts_of_speech,
                            "slotIds": slots,
                        }),
                    );
                }
                Msa::Inflectional {
                    part_of_speech,
                    slots,
                    features,
                    ..
                } => {
                    let slot_ids: Vec<String> = slots.clone();
                    object.insert("kind".to_string(), json!("inflectional"));
                    object.insert(
                        "category".to_string(),
                        pos_json(snapshot, part_of_speech.as_deref()),
                    );
                    object.insert(
                        "slot".to_string(),
                        slots_json(snapshot, &slot_ids)
                            .as_array()
                            .and_then(|values| values.first())
                            .cloned()
                            .unwrap_or(Value::Null),
                    );
                    object.insert("slots".to_string(), slots_json(snapshot, &slot_ids));
                    object.insert("features".to_string(), features_json(features.as_ref()));
                }
                Msa::Derivational {
                    from_part_of_speech,
                    to_part_of_speech,
                    from_inflection_class,
                    to_inflection_class,
                    from_features,
                    to_features,
                    ..
                } => {
                    object.insert("kind".to_string(), json!("derivational"));
                    object.insert(
                        "fromCategory".to_string(),
                        pos_json(snapshot, from_part_of_speech.as_deref()),
                    );
                    object.insert(
                        "toCategory".to_string(),
                        pos_json(snapshot, to_part_of_speech.as_deref()),
                    );
                    object.insert(
                        "fromInflectionClass".to_string(),
                        class_json(snapshot, from_inflection_class.as_deref()),
                    );
                    object.insert(
                        "toInflectionClass".to_string(),
                        class_json(snapshot, to_inflection_class.as_deref()),
                    );
                    object.insert(
                        "fromFeatures".to_string(),
                        features_json(from_features.as_ref()),
                    );
                    object.insert(
                        "toFeatures".to_string(),
                        features_json(to_features.as_ref()),
                    );
                    object.insert(
                        "category".to_string(),
                        object.get("toCategory").cloned().unwrap_or(Value::Null),
                    );
                    object.insert("slot".to_string(), Value::Null);
                }
                Msa::Unclassified { part_of_speech, .. } => {
                    object.insert("kind".to_string(), json!("unclassified"));
                    object.insert(
                        "category".to_string(),
                        pos_json(snapshot, part_of_speech.as_deref()),
                    );
                    object.insert("slot".to_string(), Value::Null);
                }
            }
            return Value::Object(object);
        }
    }
    json!({ "id": msa_id, "quality": "source-id-only" })
}
fn rich_morph(snapshot: Option<&Snapshot>, morph: &pg_parse::ParseMorph) -> Value {
    let mut entry_id: Option<String> = None;
    let mut form_value = Value::Null;
    let mut headword = Value::Null;
    let mut gloss = Value::Null;
    let mut msa_value = Value::Null;
    let mut slot_value = Value::Null;
    let mut features_value = json!({ "status": "unavailable" });
    let mut infl_value = Value::Null;

    if let (Some(snapshot), Some(form_id)) = (snapshot, morph.form.as_deref()) {
        'entry: for entry in &snapshot.lexicon.entries {
            if let Some(allomorph) = entry.allomorphs.iter().find(|a| a.guid == form_id) {
                entry_id = Some(entry.guid.clone());
                form_value = first_ws(&allomorph.forms, Some(&allomorph.guid));
                headword = if entry.citation_form.is_empty() {
                    entry.allomorphs.last().map_or(Value::Null, |lexeme| {
                        first_ws(&lexeme.forms, Some(&lexeme.guid))
                    })
                } else {
                    first_ws(&entry.citation_form, Some(&entry.guid))
                };
                if let Some(msa_id) = morph.msa.as_deref() {
                    if let Some(sense) = entry
                        .senses
                        .iter()
                        .find(|sense| sense.msa.as_deref() == Some(msa_id))
                    {
                        gloss = first_ws(&sense.gloss, Some(&sense.guid));
                    }
                }
                break 'entry;
            }
        }
        let msa = msa_fields(snapshot, morph.msa.as_deref());
        if let Some(object) = msa.as_object() {
            slot_value = object.get("slot").cloned().unwrap_or(Value::Null);
            features_value = object.get("features").cloned().unwrap_or(features_value);
            infl_value = object
                .get("inflectionClass")
                .cloned()
                .unwrap_or(Value::Null);
        }
        msa_value = msa;
        if gloss.is_null() {
            if let Some(msa_id) = morph.msa.as_deref() {
                if let Some(sense) = snapshot
                    .lexicon
                    .entries
                    .iter()
                    .flat_map(|entry| &entry.senses)
                    .find(|sense| sense.msa.as_deref() == Some(msa_id))
                {
                    gloss = first_ws(&sense.gloss, Some(&sense.guid));
                }
            }
        }
    }

    let quality = if entry_id.is_some() || morph.msa.is_some() {
        if snapshot.is_some() {
            "authored"
        } else {
            "grammar-local"
        }
    } else if morph.guessed_string.is_some() {
        "synthetic"
    } else {
        "unknown"
    };
    json!({
        "identity": {
            "formId": morph.form,
            "entryId": entry_id,
            "msaId": morph.msa,
            "inflTypeId": morph.infl_type,
            "quality": quality,
        },
        "form": form_value,
        "headword": headword,
        "gloss": gloss,
        "msa": msa_value,
        "slot": slot_value,
        "features": features_value,
        "inflectionClass": infl_value,
        "guessedString": morph.guessed_string,
    })
}

fn attempted_morphs(grammar: &Grammar, metadata: &TraceMetadata, word: &Word) -> Vec<Value> {
    word.morphs.iter().map(|record| {
        let source = grammar.morphemes.get(record.morpheme.0 as usize);
        let source_form_ids: Vec<Value> = grammar.allomorph_sources.get(record.allomorph.0 as usize)
            .map(|source| source.form_guids.iter().map(|id| json!(id)).collect())
            .unwrap_or_default();
        let single_form = if source_form_ids.len() == 1 {
            source_form_ids.first().and_then(Value::as_str).map(str::to_string)
        } else {
            None
        };
        let source_msa = source.and_then(|m| m.source_msa_guid.clone());
        let source_infl_type = source.and_then(|m| m.source_infl_type_guid.clone());
        let synthetic = matches!(record.runtime_root.as_deref(), Some(RuntimeRoot::Guessed(_) | RuntimeRoot::Supplied(_)));
        let quality = if synthetic { "synthetic" } else if source_form_ids.iter().any(|id| !id.is_null()) || source_msa.is_some() { "authored" } else { "grammar-local" };
        let runtime = record.runtime_root.as_deref().map(|root| match root {
            RuntimeRoot::Guessed(root) => json!({ "kind": "guessed", "text": root.text }),
            RuntimeRoot::Supplied(root) => json!({
                "kind": "supplied",
                "entryId": root.entry_id,
                "realizationId": root.realization_id,
                "lexicalSpelling": root.lexical_spelling,
                "gloss": root.gloss,
            }),
        }).unwrap_or(Value::Null);
        let guessed_string = record.runtime_root.as_deref().and_then(|root| match root {
            RuntimeRoot::Guessed(root) => Some(root.text.clone()),
            RuntimeRoot::Supplied(_) => None,
        });
        let mut morph = if let Some(form) = single_form.clone() {
            rich_morph(metadata.snapshot.as_ref(), &pg_parse::ParseMorph {
                form: Some(form),
                msa: source_msa.clone(),
                infl_type: source_infl_type.clone(),
                guessed_string: guessed_string.clone(),
            })
        } else {
            json!({
                "identity": {
                    "formId": Value::Null,
                    "sourceFormIds": source_form_ids.clone(),
                    "entryId": Value::Null,
                    "msaId": source_msa.clone(),
                    "inflTypeId": source_infl_type,
                    "quality": quality,
                },
                "form": Value::Null,
                "headword": Value::Null,
                "gloss": Value::Null,
                "msa": source_msa.map_or(Value::Null, |id| json!({ "id": id, "quality": "source-id-only" })),
                "slot": Value::Null,
                "features": { "status": "unavailable" },
                "inflectionClass": Value::Null,
                "guessedString": guessed_string,
            })
        };
        if let Some(object) = morph.as_object_mut() {
            if let Some(identity) = object.get_mut("identity").and_then(Value::as_object_mut) {
                identity.insert("morphemeId".to_string(), json!(record.morpheme.0));
                identity.insert("allomorphId".to_string(), json!(record.allomorph.0));
                identity.insert("quality".to_string(), json!(quality));
                if !identity.contains_key("sourceFormIds") {
                    identity.insert("sourceFormIds".to_string(), json!(source_form_ids));
                }
            }
            object.insert("order".to_string(), json!(record.order));
            object.insert("status".to_string(), json!(format!("{:?}", record.status)));
            object.insert("runtime".to_string(), runtime);
        }
        morph
    }).collect()
}
fn source_identity(grammar: &Grammar, source: TraceSource) -> Value {
    use pg_grammar::stats_identity::{morph_rule_identity, phon_rule_identity, IdentityQuality};
    let (kind, identity) = match source {
        TraceSource::Language | TraceSource::None => return Value::Null,
        TraceSource::Stratum(id) => {
            return json!({ "kind": "stratum", "id": id.0.to_string(), "quality": "grammar-local" })
        }
        TraceSource::Template(id) => {
            return json!({ "kind": "template", "id": id.0.to_string(), "quality": "grammar-local" })
        }
        TraceSource::MorphRule(id) => ("morphRule", morph_rule_identity(grammar, id)),
        TraceSource::PhonRule(id) => ("phonRule", phon_rule_identity(grammar, id)),
    };
    let quality = match identity.quality {
        IdentityQuality::Authored => "authored",
        IdentityQuality::Structural => "grammar-local",
        IdentityQuality::Synthetic => "synthetic",
    };
    json!({ "kind": kind, "id": identity.key, "quality": quality })
}
fn outcome_status(type_: TraceType) -> &'static str {
    match type_ {
        TraceType::Successful => "successful",
        TraceType::Failed => "failed",
        TraceType::Blocked => "blocked",
        _ => "attempted",
    }
}

fn failure_context(reason: pg_rules::trace::FailureReason) -> Value {
    let code = format!("{reason:?}");
    let kind = match reason {
        pg_rules::trace::FailureReason::Environments => "environment",
        pg_rules::trace::FailureReason::SurfaceFormMismatch => "surfaceMismatch",
        pg_rules::trace::FailureReason::ObligatorySyntacticFeatures
        | pg_rules::trace::FailureReason::RequiredSyntacticFeatureStruct
        | pg_rules::trace::FailureReason::HeadRequiredSyntacticFeatureStruct
        | pg_rules::trace::FailureReason::NonHeadRequiredSyntacticFeatureStruct => {
            "syntacticFeatures"
        }
        pg_rules::trace::FailureReason::RequiredMprFeatures
        | pg_rules::trace::FailureReason::ExcludedMprFeatures
        | pg_rules::trace::FailureReason::HeadProdRestrictMprFeatures
        | pg_rules::trace::FailureReason::NonHeadProdRestrictMprFeatures => "mprFeatures",
        _ => "decisionGate",
    };
    json!({
        "status": "unavailable",
        "kind": kind,
        "reasonCode": code,
        "required": Value::Null,
        "actual": Value::Null,
        "environment": Value::Null,
        "source": "TraceSink.failure_reason",
        "unavailableReason": "owner-payload-not-captured",
    })
}

fn captured_failure_context(
    reason: pg_rules::trace::FailureReason,
    context: &FailureContext,
) -> Value {
    let mut value = failure_context(reason);
    if let Some(object) = value.as_object_mut() {
        object.insert("status".to_string(), json!("captured"));
        object.insert("required".to_string(), json!(context.required));
        object.insert("actual".to_string(), json!(context.actual));
        object.insert("environment".to_string(), json!(context.environment));
        object.insert("source".to_string(), json!("rejection-owner"));
        object.remove("unavailableReason");
    }
    value
}
fn decorate_trace_node(
    grammar: &Grammar,
    metadata: &TraceMetadata,
    sink: &TreeTraceSink,
    handle: TraceHandle,
    value: &mut Value,
) -> Result<(), String> {
    let node = sink.node(handle);
    let Some(object) = value.as_object_mut() else {
        return Ok(());
    };
    object.insert(
        "sourceIdentity".to_string(),
        source_identity(grammar, node.source),
    );
    object.insert(
        "outcome".to_string(),
        json!({
            "status": outcome_status(node.type_),
            "eventType": format!("{:?}", node.type_),
        }),
    );
    if let Some(reason) = node.failure_reason {
        let context = node.failure_context.as_ref().map_or_else(
            || failure_context(reason),
            |context| captured_failure_context(reason, context),
        );
        object.insert("failureContext".to_string(), context);
    } else {
        object.insert("failureContext".to_string(), Value::Null);
    }
    let word = node.output.as_ref().or(node.input.as_ref());
    object.insert(
        "attemptedMorphs".to_string(),
        word.map_or_else(Vec::new, |word| attempted_morphs(grammar, metadata, word))
            .into(),
    );
    let children = node
        .children
        .iter()
        .map(|child| {
            let shallow = crate::trace_render::render_json_node_shallow(grammar, sink, *child);
            let mut child_value: Value = serde_json::from_str(&shallow)
                .map_err(|error| format!("serialize trace node JSON: {error}"))?;
            decorate_trace_node(grammar, metadata, sink, *child, &mut child_value)?;
            Ok(child_value)
        })
        .collect::<Result<Vec<_>, String>>()?;
    object.insert("children".to_string(), Value::Array(children));
    Ok(())
}
#[cfg(test)]
fn render_envelope_v2(
    trace: Value,
    word: &str,
    outcome: &ParseOutcome,
    rows: &[StatsRow],
    elapsed: Duration,
    metadata: &TraceMetadata,
) -> Result<String, String> {
    let analyses: Vec<Value> = outcome.analyses.iter().enumerate().map(|(index, (morphemes, surface))| {
        json!({
            "analysisId": analysis_id(index),
            "index": index,
            "morphemes": morphemes,
            "surface": surface,
            "projection": {
                "profile": pg_parse::PARSE_ANALYSIS_PROFILE,
                "status": "unavailable",
                "error": "analysis projection requires the loaded grammar", "errorCode": "GrammarUnavailable"
            },
            "morphs": []
        })
    }).collect();
    envelope_json(trace, word, outcome, rows, elapsed, metadata, analyses)
}
fn render_envelope_v2_with_grammar(
    trace: Value,
    word: &str,
    outcome: &ParseOutcome,
    rows: &[StatsRow],
    elapsed: Duration,
    metadata: &TraceMetadata,
    grammar: &Grammar,
) -> Result<String, String> {
    let analyses: Vec<Value> = outcome.analyses.iter().enumerate().map(|(index, (morphemes, surface))| {
        let structured = outcome.structured.get(index);
        let (projection, morphs) = match structured {
            Some(analysis) => match project_parse_analysis(analysis, grammar) {
                Ok(projected) => (
                    json!({ "profile": pg_parse::PARSE_ANALYSIS_PROFILE, "status": "available", "error": Value::Null }),
                    projected.morphs.iter().map(|morph| rich_morph(metadata.snapshot.as_ref(), morph)).collect(),
                ),
                Err(error) => (
                    json!({ "profile": pg_parse::PARSE_ANALYSIS_PROFILE, "status": "unavailable", "error": error.to_string(), "errorCode": projection_error_code(&error) }),
                    Vec::<Value>::new(),
                ),
            },
            None => (
                json!({ "profile": pg_parse::PARSE_ANALYSIS_PROFILE, "status": "unavailable", "error": "structured analysis missing", "errorCode": "StructuredAnalysisMissing" }),
                Vec::new(),
            ),
        };
        json!({ "analysisId": analysis_id(index), "index": index, "morphemes": morphemes, "surface": surface, "projection": projection, "morphs": morphs })
    }).collect();
    envelope_json(trace, word, outcome, rows, elapsed, metadata, analyses)
}

fn envelope_json(
    trace: Value,
    word: &str,
    outcome: &ParseOutcome,
    rows: &[StatsRow],
    elapsed: Duration,
    metadata: &TraceMetadata,
    analyses: Vec<Value>,
) -> Result<String, String> {
    let elapsed_ns = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
    serde_json::to_string(&json!({
        "schemaVersion": "pangloss.trace-details.v2",
        "word": word,
        "provenance": {
            "parser": {
                "name": "pangloss",
                "version": env!("CARGO_PKG_VERSION"),
                "traceProfile": "pangloss.trace-details.v2"
            },
            "grammar": {
                "name": metadata.grammar_name,
                "sourceKind": metadata.source_kind,
                "grammarHash": metadata.grammar_hash,
                "grammarHashSemantics": metadata.grammar_hash_semantics
            },
            "project": metadata.project_name,
            "writingSystems": {
                "vernacular": metadata.vernacular_writing_systems,
                "analysis": metadata.analysis_writing_systems
            }
        },
        "hostCapture": Value::Null,
        "search": {
            "completed": !outcome.capped && !outcome.timed_out && !outcome.invalid_shape,
            "capped": outcome.capped,
            "timedOut": outcome.timed_out,
            "invalidShape": outcome.invalid_shape,
            "steps": outcome.steps,
            "elapsedNs": elapsed_ns,
        },
        "result": {
            "signature": outcome.signature(),
            "guessed": outcome.guessed,
            "analyses": analyses,
        },
        "categories": stats_json(rows),
        "trace": trace,
    }))
    .map_err(|error| format!("serialize rich trace JSON: {error}"))
}
pub fn render(
    grammar: &Grammar,
    sink: &TreeTraceSink,
    root: Option<TraceHandle>,
    word: &str,
    outcome: &ParseOutcome,
    rows: &[StatsRow],
    elapsed: Duration,
    metadata: &TraceMetadata,
) -> Result<String, String> {
    let mut trace = match root {
        Some(root) => serde_json::from_str::<Value>(
            &crate::trace_render::render_json_node_shallow(grammar, sink, root),
        )
        .map_err(|error| format!("serialize trace JSON: {error}"))?,
        None => Value::Null,
    };
    if let Some(root) = root {
        decorate_trace_node(grammar, metadata, sink, root, &mut trace)?;
    }
    render_envelope_v2_with_grammar(trace, word, outcome, rows, elapsed, metadata, grammar)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{render_envelope_v2, validate_details, TraceMetadata};
    use pg_grammar::model::StratumId;
    use pg_parse::ParseOutcome;
    use pg_rules::stats::{Counters, Direction, ObjectKind, StatsRow};
    use serde_json::json;

    #[test]
    fn details_requires_trace_and_json() {
        assert!(validate_details(true, "json", true, false).is_ok());
        assert!(validate_details(false, "json", true, false).is_err());
        assert!(validate_details(true, "text", true, false).is_err());
        assert!(validate_details(true, "json", true, true).is_err());
    }

    #[test]
    fn envelope_keeps_tree_result_and_unmeasured_timing_explicit() {
        let outcome = ParseOutcome {
            analyses: vec![("root+past".into(), "sagd".into())],
            structured: Vec::new(),
            capped: false,
            invalid_shape: false,
            steps: 17,
            timed_out: false,
            guessed: false,
            candidates_generated: 1,
        };
        let rows = vec![
            StatsRow {
                kind: ObjectKind::MorphRule,
                object_index: 0,
                stratum: StratumId(0),
                allomorph: 0,
                direction: Direction::Synthesis,
                counters: Counters {
                    attempts: 2,
                    not_applied: 1,
                    self_time_ns: 11,
                    ..Counters::default()
                },
            },
            StatsRow {
                kind: ObjectKind::MorphRule,
                object_index: 0,
                stratum: StratumId(0),
                allomorph: 1,
                direction: Direction::Synthesis,
                counters: Counters {
                    not_applied: 2,
                    self_time_ns: 7,
                    ..Counters::default()
                },
            },
            StatsRow {
                kind: ObjectKind::Overlay,
                object_index: 0,
                stratum: StratumId(0),
                allomorph: 0,
                direction: Direction::Analysis,
                counters: Counters {
                    attempts: 1,
                    self_time_ns: 99,
                    ..Counters::default()
                },
            },
            StatsRow {
                kind: ObjectKind::PhonRule,
                object_index: 0,
                stratum: StratumId(0),
                allomorph: 0,
                direction: Direction::Analysis,
                counters: Counters {
                    attempts: 3,
                    self_time_ns: 77,
                    ..Counters::default()
                },
            },
            StatsRow {
                kind: ObjectKind::RootIndex,
                object_index: 0,
                stratum: StratumId(0),
                allomorph: 0,
                direction: Direction::Analysis,
                counters: Counters {
                    attempts: 4,
                    self_time_ns: 23,
                    ..Counters::default()
                },
            },
        ];
        let tree =
            json!({ "type": "WordAnalysis", "children": [{"type": "Successful", "children": []}] });
        let value: serde_json::Value = serde_json::from_str(
            &render_envelope_v2(
                tree.clone(),
                "sagd",
                &outcome,
                &rows,
                Duration::from_nanos(7),
                &TraceMetadata::default(),
            )
            .expect("rich envelope serializes"),
        )
        .expect("rich envelope is JSON");
        assert_eq!(value["schemaVersion"], "pangloss.trace-details.v2");
        assert_eq!(value["trace"], tree);
        assert_eq!(value["search"]["completed"], true);
        assert_eq!(value["result"]["signature"], "root+past|sagd");
        assert_eq!(value["categories"]["morphRule"]["selfElapsedNs"], 18);
        assert_eq!(value["categories"]["morphRule"]["notApplied"], 1);
        assert_eq!(value["categories"]["morphRule"]["attempts"], 2);
        assert_eq!(value["categories"]["phonRule"]["attempts"], 3);
        assert_eq!(value["categories"]["phonRule"]["selfElapsedNs"], 77);
        assert_eq!(value["categories"]["phonRule"]["timingAvailable"], true);
        assert_eq!(value["categories"]["rootIndex"]["selfElapsedNs"], 23);
        assert_eq!(value["categories"]["rootIndex"]["timingAvailable"], true);
        assert_eq!(
            value["categories"]["overlay"]["selfElapsedNs"],
            serde_json::Value::Null
        );
        assert_eq!(value["categories"]["overlay"]["timingAvailable"], false);
    }

    #[test]
    fn v2_retains_analysis_when_projection_is_unavailable() {
        let outcome = ParseOutcome {
            analyses: vec![("root".into(), "sagd".into())],
            structured: Vec::new(),
            capped: true,
            invalid_shape: false,
            steps: 99,
            timed_out: false,
            guessed: false,
            candidates_generated: 1,
        };
        let value: serde_json::Value = serde_json::from_str(
            &render_envelope_v2(
                json!({"type": "WordAnalysis", "children": []}),
                "sagd",
                &outcome,
                &[],
                Duration::from_nanos(7),
                &TraceMetadata {
                    grammar_name: Some("Test".into()),
                    grammar_hash: Some("abc".into()),
                    grammar_hash_semantics: Some("source-bytes-v1".into()),
                    source_kind: "xml".into(),
                    ..TraceMetadata::default()
                },
            )
            .expect("v2 envelope serializes"),
        )
        .expect("v2 envelope is JSON");
        assert_eq!(value["schemaVersion"], "pangloss.trace-details.v2");
        assert_eq!(value["provenance"]["grammar"]["name"], "Test");
        assert_eq!(
            value["provenance"]["grammar"]["grammarHashSemantics"],
            "source-bytes-v1"
        );
        assert_eq!(value["search"]["completed"], false);
        assert_eq!(
            value["result"]["analyses"][0]["projection"]["status"],
            "unavailable"
        );
        assert_eq!(value["trace"]["children"].as_array().unwrap().len(), 0);
    }
    #[test]
    fn rich_tree_preserves_nodes_beyond_default_json_parse_depth() {
        use pg_rules::trace::TraceSink;
        let grammar = pg_grammar::load(include_str!(
            "../../../../conformance-staging/filter-passes/exact-span/grammar.xml"
        ))
        .unwrap();
        let sink = pg_rules::trace::TreeTraceSink::with_failure_context();
        let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
        let (outcome, rows) = morpher.parse_word_traced_with_stats(
            "matinlu",
            &pg_parse::ParseOptions::default(),
            &sink,
        );
        let root = sink.root().unwrap();
        let input = sink.node(root).input.unwrap();
        let mut cursor = root;
        for _ in 0..80 {
            cursor = sink.begin_apply_stratum(cursor, StratumId(0), &input);
        }
        let ordinary = crate::trace_render::render_json(&grammar, &sink, root);
        assert!(serde_json::from_str::<serde_json::Value>(&ordinary).is_err());
        let rich = super::render(
            &grammar,
            &sink,
            Some(root),
            "matinlu",
            &outcome,
            &rows,
            Duration::ZERO,
            &TraceMetadata::default(),
        )
        .unwrap();
        assert_eq!(rich.matches("\"type\":").count(), sink.len());
        assert_eq!(ordinary.matches("\"type\":").count(), sink.len());
        assert!(rich.contains("\"schemaVersion\":\"pangloss.trace-details.v2\""));
    }

    #[test]
    fn authored_morph_keeps_lexeme_headword_when_citation_is_absent() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../pg-fwdata/tests/data/fixture.fwdata");
        let (mut snapshot, _) = pg_fwdata::import_file(&path).unwrap();
        let entry = snapshot
            .lexicon
            .entries
            .iter_mut()
            .find(|entry| entry.citation_form.iter().any(|form| form.form == "ranna"))
            .unwrap();
        entry.citation_form.clear();
        let lexeme = entry.allomorphs.last().unwrap();
        let form_id = lexeme.guid.clone();
        let expected = lexeme.forms[0].form.clone();
        let msa_id = entry.msas[0].guid().to_string();
        let morph = super::rich_morph(
            Some(&snapshot),
            &pg_parse::ParseMorph {
                form: Some(form_id.clone()),
                msa: Some(msa_id),
                infl_type: None,
                guessed_string: None,
            },
        );
        assert_eq!(morph["headword"]["text"], expected);
        assert_eq!(morph["headword"]["sourceId"], form_id);
        assert_eq!(morph["identity"]["quality"], "authored");
        assert!(!morph["msa"].is_null());
    }
}
