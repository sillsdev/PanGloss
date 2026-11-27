//! Renders opt-in rich trace-details envelopes: v5, or v6 when `ruleCosts` is requested.

use std::time::Duration;

use pg_grammar::model::Grammar;
use pg_grammar::stats_identity::StatsIdentityCatalog;
use pg_parse::{project_parse_analysis, ParseOutcome, ParseProjectionError};
use pg_rules::stats::{self, Direction, ObjectKind, OverlayPhase, StatsRow};
use pg_rules::trace::{
    FailureContext, LookupMode, RejectionEvidence, TraceHandle, TraceObject, TraceSource,
    TraceType, TreeTraceSink,
};
use pg_rules::word::{RuntimeRoot, Word};
use pg_snapshot::{
    AffixSlot, FeatureStructure, InflectionClass, Msa, PartOfSpeech, Snapshot, WsForm,
};
use serde_json::{json, Value};

// Only data consulted while rendering trace details survives the grammar load.
#[derive(Clone, Debug)]
pub struct TraceSnapshot {
    parts_of_speech: Vec<PartOfSpeech>,
    entries: Vec<TraceEntry>,
}

#[derive(Clone, Debug)]
struct TraceEntry {
    guid: String,
    citation_form: Vec<WsForm>,
    allomorphs: Vec<TraceAllomorph>,
    msas: Vec<TraceMsa>,
    senses: Vec<TraceSense>,
}

#[derive(Clone, Debug)]
struct TraceAllomorph {
    guid: String,
    forms: Vec<WsForm>,
}

#[derive(Clone, Debug)]
struct TraceSense {
    guid: String,
    gloss: Vec<WsForm>,
    msa: Option<String>,
}

#[derive(Clone, Debug)]
enum TraceMsa {
    Stem {
        guid: String,
        part_of_speech: Option<String>,
        inflection_class: Option<String>,
        features: Option<FeatureStructure>,
        from_parts_of_speech: Vec<String>,
        slots: Vec<String>,
    },
    Inflectional {
        guid: String,
        part_of_speech: Option<String>,
        slots: Vec<String>,
        features: Option<FeatureStructure>,
    },
    Derivational {
        guid: String,
        from_part_of_speech: Option<String>,
        to_part_of_speech: Option<String>,
        from_inflection_class: Option<String>,
        to_inflection_class: Option<String>,
        from_features: Option<FeatureStructure>,
        to_features: Option<FeatureStructure>,
    },
    Unclassified {
        guid: String,
        part_of_speech: Option<String>,
    },
}

impl TraceMsa {
    fn guid(&self) -> &str {
        match self {
            Self::Stem { guid, .. }
            | Self::Inflectional { guid, .. }
            | Self::Derivational { guid, .. }
            | Self::Unclassified { guid, .. } => guid,
        }
    }
}

impl From<Msa> for TraceMsa {
    fn from(msa: Msa) -> Self {
        match msa {
            Msa::Stem {
                guid,
                part_of_speech,
                inflection_class,
                features,
                from_parts_of_speech,
                slots,
                ..
            } => Self::Stem {
                guid,
                part_of_speech,
                inflection_class,
                features,
                from_parts_of_speech,
                slots,
            },
            Msa::Inflectional {
                guid,
                part_of_speech,
                slots,
                features,
                ..
            } => Self::Inflectional {
                guid,
                part_of_speech,
                slots,
                features,
            },
            Msa::Derivational {
                guid,
                from_part_of_speech,
                to_part_of_speech,
                from_inflection_class,
                to_inflection_class,
                from_features,
                to_features,
                ..
            } => Self::Derivational {
                guid,
                from_part_of_speech,
                to_part_of_speech,
                from_inflection_class,
                to_inflection_class,
                from_features,
                to_features,
            },
            Msa::Unclassified {
                guid,
                part_of_speech,
            } => Self::Unclassified {
                guid,
                part_of_speech,
            },
        }
    }
}

impl From<Snapshot> for TraceSnapshot {
    fn from(snapshot: Snapshot) -> Self {
        Self {
            parts_of_speech: snapshot.morphology.parts_of_speech,
            entries: snapshot
                .lexicon
                .entries
                .into_iter()
                .map(|entry| TraceEntry {
                    guid: entry.guid,
                    citation_form: entry.citation_form,
                    allomorphs: entry
                        .allomorphs
                        .into_iter()
                        .map(|form| TraceAllomorph {
                            guid: form.guid,
                            forms: form.forms,
                        })
                        .collect(),
                    msas: entry.msas.into_iter().map(TraceMsa::from).collect(),
                    senses: entry
                        .senses
                        .into_iter()
                        .map(|sense| TraceSense {
                            guid: sense.guid,
                            gloss: sense.gloss,
                            msa: sense.msa,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct TraceMetadata {
    pub grammar_name: Option<String>,
    pub grammar_hash: Option<String>,
    pub grammar_hash_semantics: Option<String>,
    pub source_kind: String,
    pub project_name: Option<String>,
    pub vernacular_writing_systems: Vec<String>,
    pub analysis_writing_systems: Vec<String>,
    pub snapshot: Option<TraceSnapshot>,
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
impl TraceMetadata {
    pub(crate) fn diagnostic_senses<'a>(
        &'a self,
        grammar: &'a Grammar,
    ) -> std::collections::HashMap<(&'a str, &'a str), Vec<&'a str>> {
        let mut by_owner = std::collections::HashMap::<_, Vec<_>>::new();
        if let Some(snapshot) = &self.snapshot {
            for entry in &snapshot.entries {
                for sense in &entry.senses {
                    if let Some(msa) = &sense.msa {
                        by_owner
                            .entry((entry.guid.as_str(), grammar.representative_msa(msa)))
                            .or_default()
                            .push(sense.guid.as_str());
                    }
                }
            }
        }
        by_owner
    }
}

pub fn metadata_from_snapshot(snapshot: Snapshot, source_kind: &str) -> TraceMetadata {
    TraceMetadata {
        grammar_name: Some(snapshot.project.name.clone()),
        grammar_hash: Some(snapshot.grammar_hash()),
        grammar_hash_semantics: Some("snapshot-semantic-sha256-v1".to_string()),
        source_kind: source_kind.to_string(),
        project_name: Some(snapshot.project.name.clone()),
        vernacular_writing_systems: snapshot.project.vernacular_writing_systems.clone(),
        analysis_writing_systems: snapshot.project.analysis_writing_systems.clone(),
        snapshot: Some(snapshot.into()),
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
        ObjectKind::Parser => "parser",
    }
}

fn stats_json(rows: &[StatsRow], work_steps: usize) -> Value {
    let kinds = [
        ObjectKind::MorphRule,
        ObjectKind::PhonRule,
        ObjectKind::LexEntry,
        ObjectKind::RootIndex,
        ObjectKind::Guesser,
        ObjectKind::Overlay,
        ObjectKind::Parser,
    ];
    let categories = kinds.into_iter().map(|kind| {
        let counters = stats::summarize_kind(rows, kind);
        let timing_available = stats::self_time_supported(kind);
        let direction_ns = |direction| {
            stats::self_time_supported_in_direction(kind, direction).then(|| {
                rows.iter()
                    .filter(|row| row.kind == kind && row.direction == direction)
                    .fold(0u64, |total, row| {
                        total.saturating_add(row.counters.self_time_ns)
                    })
            })
        };
        let mut category = json!({
            "attempts": counters.attempts,
            "work": counters.work,
            "work_steps": counters.work_steps,
            "outputs": counters.outputs,
            "notApplied": counters.not_applied,
            "noRoot": counters.no_root,
            "surfaceMismatch": counters.surface_mismatch,
            "uses": counters.uses,
            "timingAvailable": timing_available,
            "selfElapsedNs": timing_available.then_some(counters.self_time_ns),
            "analysisSelfElapsedNs": direction_ns(Direction::Analysis),
            "synthesisSelfElapsedNs": direction_ns(Direction::Synthesis),
        });
        if kind == ObjectKind::Overlay {
            category["phases"] = overlay_phases_json(rows);
        }
        if kind == ObjectKind::Parser {
            let work = pg_rules::stats::ParserWork::ALL.into_iter().map(|phase| {
                let steps: u64 = rows
                    .iter()
                    .filter(|row| row.kind == kind && row.object_index == phase.index())
                    .map(|row| row.counters.work_steps)
                    .sum();
                (phase.name().to_owned(), json!(steps))
            });
            category["workStepsByPhase"] = Value::Object(serde_json::Map::from_iter(work));
        }
        (kind_name(kind).to_owned(), category)
    });
    let mut categories = serde_json::Map::from_iter(categories);
    let attributed: u64 = rows.iter().map(|row| row.counters.work_steps).sum();
    let unattributed = (work_steps as u64).saturating_sub(attributed);
    categories.insert(
        "unattributed".to_owned(),
        json!({"work_steps": unattributed}),
    );
    categories.into()
}

/// How many objects `ruleCosts` lists when the request does not say.
pub const DEFAULT_RULE_COSTS: usize = 8;

/// The `ruleCosts` object: the `top_n` costliest objects of one word's parse, ranked by the work steps they reserved, plus the remainder the figures leave unattributed. One entry per `(kind, object, stratum, direction)`, summed over its allomorph rows, so a rule that spent the budget is named whether or not the word finished. `stopped` marks a capped, timed-out or invalid-shape parse; a timed-out word's figures depend on this computer's speed, so `reproducible` is false for it. Self time is wall-clock and present only for kinds that measure it.
pub fn rule_costs_json(
    grammar: &Grammar,
    identities: &StatsIdentityCatalog,
    rows: &[StatsRow],
    outcome: &ParseOutcome,
    top_n: usize,
) -> Result<Value, String> {
    type Key = (ObjectKind, u32, u8, Direction);
    let mut grouped: std::collections::BTreeMap<Key, (&StatsRow, stats::Counters)> =
        std::collections::BTreeMap::new();
    for row in rows {
        let key = (row.kind, row.object_index, row.stratum.0, row.direction);
        let entry = grouped
            .entry(key)
            .or_insert((row, stats::Counters::default()));
        let c = &mut entry.1;
        c.attempts += row.counters.attempts;
        c.work_steps += row.counters.work_steps;
        c.outputs += row.counters.outputs;
        c.self_time_ns = c.self_time_ns.saturating_add(row.counters.self_time_ns);
    }
    let attributed: u64 = rows.iter().map(|row| row.counters.work_steps).sum();
    let unattributed = (outcome.work_steps as u64).saturating_sub(attributed);
    let mut ranked: Vec<_> = grouped
        .into_iter()
        .filter(|(_, (_, c))| c.work_steps > 0)
        .collect();
    // BTreeMap order is the tie-break: kind, object, stratum, direction.
    ranked.sort_by_key(|(_, (_, c))| std::cmp::Reverse(c.work_steps));
    let object_count = ranked.len();
    let objects = ranked
        .into_iter()
        .take(top_n)
        .map(|(_, (row, c))| {
            let identity = crate::stats_cmd::stats_row_object_identity(grammar, identities, row)?;
            let stratum = pg_grammar::stats_identity::stratum_identity(grammar, row.stratum);
            let timed = stats::self_time_supported(row.kind);
            Ok(json!({
                "kind": kind_name(row.kind),
                "key": identity.key,
                "label": identity.label,
                "stratum": stratum.label,
                "direction": row.direction.as_str(),
                "work_steps": c.work_steps,
                "attempts": c.attempts,
                "outputs": c.outputs,
                "selfElapsedNs": timed.then_some(c.self_time_ns),
            }))
        })
        .collect::<Result<Vec<Value>, String>>()?;
    Ok(json!({
        "topN": top_n,
        "objectCount": object_count,
        "workSteps": outcome.work_steps,
        "attributedWorkSteps": attributed,
        "unattributedWorkSteps": unattributed,
        "stopped": outcome.capped || outcome.timed_out || outcome.invalid_shape,
        "reproducible": !outcome.timed_out,
        "objects": objects,
    }))
}

fn overlay_phases_json(rows: &[StatsRow]) -> Value {
    let phases = OverlayPhase::ALL.into_iter().map(|phase| {
        let (attempts, work, self_time_ns) = rows
            .iter()
            .filter(|row| row.kind == ObjectKind::Overlay && row.object_index == phase.index())
            .fold((0u64, 0u64, 0u64), |(a, w, t), row| {
                (
                    a.saturating_add(row.counters.attempts),
                    w.saturating_add(row.counters.work),
                    t.saturating_add(row.counters.self_time_ns),
                )
            });
        (
            phase.name().to_owned(),
            json!({ "attempts": attempts, "work": work, "selfElapsedNs": self_time_ns }),
        )
    });
    serde_json::Map::from_iter(phases).into()
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
            .find(|slot| slot.guid == id)
            .or_else(|| find_slot(&part.children, id))
    })
}

fn pos_json(snapshot: &TraceSnapshot, id: Option<&str>) -> Value {
    id.and_then(|id| find_pos(&snapshot.parts_of_speech, id))
        .map_or_else(
            || {
                id.map(|id| json!({ "id": id, "quality": "source-id-only" }))
                    .unwrap_or(Value::Null)
            },
            |pos| json!({ "id": pos.guid, "name": pos.name, "abbreviation": pos.abbreviation }),
        )
}

fn class_json(snapshot: &TraceSnapshot, id: Option<&str>) -> Value {
    id.and_then(|id| find_infl(&snapshot.parts_of_speech, id)).map_or_else(
        || id.map(|id| json!({ "id": id, "quality": "source-id-only" })).unwrap_or(Value::Null),
        |class| json!({ "id": class.guid, "name": class.name, "abbreviation": class.abbreviation }),
    )
}

fn slots_json(snapshot: &TraceSnapshot, ids: &[String]) -> Value {
    let values: Vec<Value> = ids
        .iter()
        .map(|id| {
            find_slot(&snapshot.parts_of_speech, id).map_or_else(
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

/// Per-render guid indexes (first match wins) and a rich_morph memo.
struct RichCtx<'a> {
    snapshot: Option<&'a TraceSnapshot>,
    allomorph_by_guid: std::collections::HashMap<&'a str, (&'a TraceEntry, &'a TraceAllomorph)>,
    msa_by_guid: std::collections::HashMap<&'a str, &'a TraceMsa>,
    sense_by_msa: std::collections::HashMap<&'a str, &'a TraceSense>,
    memo: std::cell::RefCell<std::collections::HashMap<MemoKey, Value>>,
}

type MemoKey = (String, Option<String>, Option<String>, Option<String>);

impl<'a> RichCtx<'a> {
    fn new(snapshot: Option<&'a TraceSnapshot>) -> Self {
        let mut ctx = Self {
            snapshot,
            allomorph_by_guid: Default::default(),
            msa_by_guid: Default::default(),
            sense_by_msa: Default::default(),
            memo: Default::default(),
        };
        if let Some(snapshot) = snapshot {
            for entry in &snapshot.entries {
                for allomorph in &entry.allomorphs {
                    ctx.allomorph_by_guid
                        .entry(allomorph.guid.as_str())
                        .or_insert((entry, allomorph));
                }
                for msa in &entry.msas {
                    ctx.msa_by_guid.entry(msa.guid()).or_insert(msa);
                }
                for sense in &entry.senses {
                    if let Some(msa) = sense.msa.as_deref() {
                        ctx.sense_by_msa.entry(msa).or_insert(sense);
                    }
                }
            }
        }
        ctx
    }
}

fn msa_fields(ctx: &RichCtx<'_>, snapshot: &TraceSnapshot, msa_id: Option<&str>) -> Value {
    let Some(msa_id) = msa_id else {
        return Value::Null;
    };
    {
        if let Some(&msa) = ctx.msa_by_guid.get(msa_id) {
            let mut object = serde_json::Map::new();
            object.insert("id".to_string(), json!(msa_id));
            match msa {
                TraceMsa::Stem {
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
                TraceMsa::Inflectional {
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
                TraceMsa::Derivational {
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
                TraceMsa::Unclassified { part_of_speech, .. } => {
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
fn rich_morph(ctx: &RichCtx<'_>, morph: &pg_parse::ParseMorph) -> Value {
    let snapshot = ctx.snapshot;
    let mut entry_id: Option<String> = None;
    let mut form_value = Value::Null;
    let mut headword = Value::Null;
    let mut gloss = Value::Null;
    let mut msa_value = Value::Null;
    let mut slot_value = Value::Null;
    let mut features_value = json!({ "status": "unavailable" });
    let mut infl_value = Value::Null;

    if let (Some(snapshot), Some(form_id)) = (snapshot, morph.form.as_deref()) {
        {
            if let Some(&(entry, allomorph)) = ctx.allomorph_by_guid.get(form_id) {
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
            }
        }
        let msa = msa_fields(ctx, snapshot, morph.msa.as_deref());
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
                if let Some(sense) = ctx.sense_by_msa.get(msa_id) {
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

fn attempted_morphs(grammar: &Grammar, ctx: &RichCtx<'_>, word: &Word) -> Vec<Value> {
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
            let key = (form.clone(), source_msa.clone(), source_infl_type.clone(), guessed_string.clone());
            let cached = ctx.memo.borrow().get(&key).cloned();
            cached.unwrap_or_else(|| {
                let value = rich_morph(ctx, &pg_parse::ParseMorph {
                    form: Some(form),
                    msa: source_msa.clone(),
                    infl_type: source_infl_type.clone(),
                    guessed_string: guessed_string.clone(),
                });
                ctx.memo.borrow_mut().insert(key, value.clone());
                value
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
    use pg_grammar::trace_identity::{
        trace_morph_rule_identity as morph_rule_identity,
        trace_phon_rule_identity as phon_rule_identity,
    };
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
    json!({ "kind": kind, "id": identity.key, "quality": identity_quality(identity.quality) })
}
fn identity_quality(quality: pg_grammar::stats_identity::IdentityQuality) -> &'static str {
    use pg_grammar::stats_identity::IdentityQuality;
    match quality {
        IdentityQuality::Authored => "authored",
        IdentityQuality::Structural => "grammar-local",
        IdentityQuality::Synthetic => "synthetic",
    }
}
fn lex_entry_identity_json(grammar: &Grammar, entry: pg_grammar::model::LexEntryId) -> Value {
    let identity = pg_grammar::trace_identity::trace_lex_entry_identity(grammar, entry);
    json!({ "kind": "lexEntry", "id": identity.key, "quality": identity_quality(identity.quality) })
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
        | pg_rules::trace::FailureReason::NonHeadRequiredSyntacticFeatureStruct
        | pg_rules::trace::FailureReason::LexicalLookupFeatures => "syntacticFeatures",
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

fn trace_object_identity(grammar: &Grammar, object: TraceObject) -> Value {
    match object {
        TraceObject::MorphRule(id) => source_identity(grammar, TraceSource::MorphRule(id)),
        TraceObject::Allomorph(id) => {
            let identity = pg_grammar::trace_identity::trace_allomorph_identity(grammar, id);
            let mut value = json!({"kind": "allomorph", "id": identity.key, "quality": identity_quality(identity.quality)});
            if let Some(source) = grammar.allomorph_sources.get(id.0 as usize) {
                value["sourceFormIds"] = json!(source.form_guids);
            }
            value
        }
        TraceObject::Morpheme(id) => {
            let identity = pg_grammar::stats_identity::morpheme_identity(grammar, id);
            json!({"kind": "morpheme", "id": identity.key, "quality": identity_quality(identity.quality)})
        }
    }
}

fn mpr_operands(grammar: &Grammar, set: &pg_grammar::model::MprSet) -> Value {
    json!(set
        .iter()
        .map(|id| {
            let index = id.0 as usize;
            match grammar.mpr_features.get(index) {
                Some(feature) => {
                    json!({"id": feature.xml_id, "name": feature.name, "quality": "authored"})
                }
                None => json!({"index": index, "quality": "grammar-local"}),
            }
        })
        .collect::<Vec<_>>())
}

fn rejection_evidence(grammar: &Grammar, evidence: &RejectionEvidence) -> Value {
    match evidence {
        RejectionEvidence::SyntacticFeatures {
            object,
            required,
            actual,
        } => json!({
            "kind": "syntacticFeatures", "failedObject": trace_object_identity(grammar, *object),
            "operands": {"representation": "grammar-local-feature-struct", "required": required, "actual": actual},
        }),
        RejectionEvidence::MprFeatures {
            object,
            required,
            excluded,
            actual,
        } => json!({
            "kind": "mprFeatures", "failedObject": trace_object_identity(grammar, *object),
            "operands": {"required": mpr_operands(grammar, required), "excluded": mpr_operands(grammar, excluded), "actual": mpr_operands(grammar, actual)},
        }),
        RejectionEvidence::CoOccurrence {
            object,
            constraint_owner,
            rule_index,
            require,
            adjacency,
            others,
            actual,
        } => json!({
            "kind": "coOccurrence", "failedObject": trace_object_identity(grammar, *object),
            "constraintOwner": trace_object_identity(grammar, *constraint_owner),
            "ruleIndex": rule_index, "require": require, "adjacency": format!("{adjacency:?}"),
            "others": others.iter().map(|id| trace_object_identity(grammar, *id)).collect::<Vec<_>>(),
            "actual": actual.iter().map(|id| trace_object_identity(grammar, *id)).collect::<Vec<_>>(),
        }),
        RejectionEvidence::AllomorphBlocked { blocked, blocker } => json!({
            "kind": "allomorphBlocked", "failedObject": trace_object_identity(grammar, TraceObject::Allomorph(*blocked)),
            "blocker": trace_object_identity(grammar, TraceObject::Allomorph(*blocker)),
        }),
        RejectionEvidence::Environments {
            object,
            constraint_owner,
            start,
            end,
            alternatives,
        } => json!({
            "kind": "environments", "failedObject": trace_object_identity(grammar, *object),
            "constraintOwner": trace_object_identity(grammar, *constraint_owner),
            "span": {"start": start, "end": end, "coordinateSystem": "interior-inclusive"},
            "alternatives": alternatives.iter().map(|env| json!({
                "index": env.index, "require": env.require, "accepted": env.accepted,
                "sourceIdentity": env.source_id.as_ref().map(|id| json!({"kind":"environment","id":id,"quality":"authored"})),
                "authoredText": env.authored_text,
                "sourceStatus": if env.source_id.is_some() || env.authored_text.is_some() {"captured"} else {"unavailable"},
            })).collect::<Vec<_>>(),
        }),
    }
}

fn captured_failure_context(
    grammar: &Grammar,
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
        if let Some(evidence) = &context.evidence {
            object.insert("evidence".into(), rejection_evidence(grammar, evidence));
        }
    }
    value
}
fn decorate_trace_node(
    grammar: &Grammar,
    ctx: &RichCtx<'_>,
    sink: &TreeTraceSink,
    handle: TraceHandle,
    value: &mut Value,
) -> Result<(), String> {
    let node = sink.node(handle);
    let Some(object) = value.as_object_mut() else {
        return Ok(());
    };
    object.insert(
        "stepId".to_string(),
        json!(format!("step-{}", handle.index())),
    );
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
    if let Some(entry) = node.blocked_by_entry {
        object.insert("blockReason".to_string(), json!("LexicalFamilyReplacement"));
        object.insert(
            "blockedByEntry".to_string(),
            lex_entry_identity_json(grammar, entry),
        );
    }
    if let Some(result) = node.lookup_result {
        object.insert(
            "lookupResult".to_string(),
            json!({
                "status": if result.match_count == 0 { "zeroMatches" } else { "matches" },
                "completed": true,
                "matchCount": result.match_count,
                "mode": match result.mode {
                    LookupMode::Lexicon => "lexicon",
                    LookupMode::Guesser => "guesser",
                },
            }),
        );
    }
    if let Some(cause) = node.partial_parse_cause {
        object.insert("partialParseCause".to_string(), json!(format!("{cause:?}")));
    }
    if let Some(slots) = &node.template_slots {
        let TraceSource::Template(tid) = node.source else {
            return Err("slot outcomes require a template event".into());
        };
        object.insert("slots".to_string(), json!(slots.iter().map(|slot| json!({
            "slotIndex": slot.slot_index,
            "slotIdentity": {"kind": "templateSlot", "template": source_identity(grammar, node.source), "index": slot.slot_index, "quality": "grammar-local"},
            "name": grammar.templates[tid.0 as usize].slots[slot.slot_index].name,
            "status": format!("{:?}", slot.status),
            "selectedRule": slot.selected_rule.map(|id| source_identity(grammar, TraceSource::MorphRule(id))),
        })).collect::<Vec<_>>()));
    }
    if node.type_ == TraceType::PhonologicalRuleAnalysis && node.output.is_none() {
        object.insert(
            "nonUnapplicationReason".to_string(),
            json!({
                "status": "unavailable", "unavailableReason": "evaluator-returned-boolean-only",
            }),
        );
    }
    if let Some(reason) = node.failure_reason {
        let context = node.failure_context.as_ref().map_or_else(
            || failure_context(reason),
            |context| captured_failure_context(grammar, reason, context),
        );
        object.insert("failureContext".to_string(), context);
    } else {
        object.insert("failureContext".to_string(), Value::Null);
    }
    let word = node.output.as_ref().or(node.input.as_ref());
    object.insert(
        "attemptedMorphs".to_string(),
        word.map_or_else(Vec::new, |word| attempted_morphs(grammar, ctx, word))
            .into(),
    );
    let children = node
        .children
        .iter()
        .map(|child| {
            let shallow = crate::trace_render::render_json_node_shallow(grammar, sink, *child);
            let mut child_value: Value = serde_json::from_str(&shallow)
                .map_err(|error| format!("serialize trace node JSON: {error}"))?;
            decorate_trace_node(grammar, ctx, sink, *child, &mut child_value)?;
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
    envelope_json(
        trace, word, outcome, rows, elapsed, metadata, analyses, None,
    )
}
fn grammar_analyses(outcome: &ParseOutcome, grammar: &Grammar, ctx: &RichCtx<'_>) -> Vec<Value> {
    outcome.analyses.iter().enumerate().map(|(index, (morphemes, surface))| {
        let structured = outcome.structured.get(index);
        let (projection, morphs) = match structured {
            Some(analysis) => match project_parse_analysis(analysis, grammar) {
                Ok(projected) => (
                    json!({ "profile": pg_parse::PARSE_ANALYSIS_PROFILE, "status": "available", "error": Value::Null }),
                    projected.morphs.iter().map(|morph| rich_morph(ctx, morph)).collect(),
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
    }).collect()
}

#[allow(clippy::too_many_arguments)]
fn envelope_json(
    trace: Value,
    word: &str,
    outcome: &ParseOutcome,
    rows: &[StatsRow],
    elapsed: Duration,
    metadata: &TraceMetadata,
    analyses: Vec<Value>,
    rule_costs: Option<Value>,
) -> Result<String, String> {
    // v5 stays the default so a reader pinned to it keeps working; v6 is only the version that carries ruleCosts.
    let schema = if rule_costs.is_some() {
        "pangloss.trace-details.v6"
    } else {
        "pangloss.trace-details.v5"
    };
    let elapsed_ns = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
    let timed_ns = rows.iter().fold(0u64, |total, row| {
        total.saturating_add(row.counters.self_time_ns)
    });
    let mut document = json!({
        "schemaVersion": schema,
        "word": word,
        "provenance": {
            "parser": {
                "name": "pangloss",
                "version": env!("CARGO_PKG_VERSION"),
                "traceProfile": schema
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
            "workSteps": outcome.work_steps,
            "elapsedNs": elapsed_ns,
            "timedNs": timed_ns,
            "unattributedNs": elapsed_ns.saturating_sub(timed_ns),
            "timingOverrunNs": timed_ns.saturating_sub(elapsed_ns),
        },
        "result": {
            "signature": outcome.signature(),
            "guessed": outcome.guessed,
            "analyses": analyses,
        },
        "categories": stats_json(rows, outcome.work_steps),
        "trace": trace,
    });
    if let Some(costs) = rule_costs {
        document["ruleCosts"] = costs;
    }
    serde_json::to_string(&document).map_err(|error| format!("serialize rich trace JSON: {error}"))
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    grammar: &Grammar,
    sink: &TreeTraceSink,
    root: Option<TraceHandle>,
    word: &str,
    outcome: &ParseOutcome,
    rows: &[StatsRow],
    elapsed: Duration,
    metadata: &TraceMetadata,
    rule_costs: Option<usize>,
) -> Result<String, String> {
    let ctx = RichCtx::new(metadata.snapshot.as_ref());
    let mut trace = match root {
        Some(root) => serde_json::from_str::<Value>(
            &crate::trace_render::render_json_node_shallow(grammar, sink, root),
        )
        .map_err(|error| format!("serialize trace JSON: {error}"))?,
        None => Value::Null,
    };
    if let Some(root) = root {
        decorate_trace_node(grammar, &ctx, sink, root, &mut trace)?;
    }
    let analyses = grammar_analyses(outcome, grammar, &ctx);
    let rule_costs = match rule_costs {
        Some(top_n) => Some(rule_costs_json(
            grammar,
            &StatsIdentityCatalog::new(grammar),
            rows,
            outcome,
            top_n,
        )?),
        None => None,
    };
    envelope_json(
        trace, word, outcome, rows, elapsed, metadata, analyses, rule_costs,
    )
}

#[cfg(test)]
mod tests;
