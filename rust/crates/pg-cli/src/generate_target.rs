//! Batch `generate` rows addressed by `target` (entry, optional sense and MSA) and an optional explicit `cell` of affix MSA combinations.
//! Format: `docs/generation.md`, "Targets and cells".

use std::time::{Duration, Instant};

use pg_grammar::model::Grammar;
use pg_parse::morpher::{Morpher, TracedGenerateLimits, TracedGenerateOutcome, TracedStopReason};
use pg_parse::source_identity::{stem_msas_of_entry, SourceIdentityError};
use pg_snapshot::FwClass;
use serde_json::{json, Map, Value};

use crate::generate_request::{
    self, is_canonical_guid, morpheme_ref_from_value, ValidatedGenerateRequest,
    ValidatedMorphemeRef,
};
use crate::serve_generate::{
    bad, parse_options, render, render_item, AttestedSet, OpError, Options,
};

/// Most combinations one `cell` may list.
pub(crate) const MAX_CELL_COMBOS: usize = 10_000;
/// Most seeds a `trace` object lists before it says it is truncated.
const TRACE_SEEDS_SHOWN: usize = 50;

/// Every cause `code` a `none` row can carry, in the order `--describe` lists them.
pub(crate) const NONE_CAUSES: [&str; 3] = ["no_stem_msa", "generated_empty", "cap"];
/// Error codes only a `target` row adds to the serve set.
pub(crate) const TARGET_ERROR_CODES: [&str; 2] = ["unknown_sense", "sense_not_owned_by_entry"];

struct Target {
    entry: String,
    sense: Option<String>,
    msa: Option<String>,
}

struct Combo {
    key: String,
    refs: Vec<ValidatedMorphemeRef>,
}

fn guid_field(object: &Map<String, Value>, key: &str) -> Result<Option<String>, OpError> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(s)) if is_canonical_guid(s) => Ok(Some(s.clone())),
        Some(_) => Err(bad(format!(
            "target.{key} must be a lowercase hyphenated GUID string"
        ))),
    }
}

fn parse_target(value: &Value) -> Result<Target, OpError> {
    let object = value
        .as_object()
        .ok_or_else(|| bad("\"target\" must be an object"))?;
    if let Some(unknown) = object
        .keys()
        .find(|k| !["entryGuid", "senseGuid", "msaGuid"].contains(&k.as_str()))
    {
        return Err(bad(format!("unknown target field: {unknown}")));
    }
    Ok(Target {
        entry: guid_field(object, "entryGuid")?
            .ok_or_else(|| bad("\"target.entryGuid\" is required"))?,
        sense: guid_field(object, "senseGuid")?,
        msa: guid_field(object, "msaGuid")?,
    })
}

fn combo_key(refs: &[ValidatedMorphemeRef]) -> String {
    if refs.is_empty() {
        return "-".into();
    }
    refs.iter()
        .map(|r| match &r.form_guid {
            Some(form) => format!("{}@{form}", r.msa_guid),
            None => r.msa_guid.clone(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn parse_combo(value: &Value, path: &str) -> Result<Combo, OpError> {
    let items = value
        .as_array()
        .ok_or_else(|| bad(format!("{path} must be an array of MSA references")))?;
    if items.len() >= generate_request::MAX_GENERATE_REQUEST_MORPHEMES {
        return Err(bad(format!(
            "{path} exceeds {} morphemes",
            generate_request::MAX_GENERATE_REQUEST_MORPHEMES - 1
        )));
    }
    let refs = items
        .iter()
        .enumerate()
        .map(|(i, item)| morpheme_ref_from_value(item, &format!("{path}[{i}]")))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ("bad_request", e.to_string()))?;
    Ok(Combo {
        key: combo_key(&refs),
        refs,
    })
}

/// `cell.msas`: an array of MSA reference objects is one combination; an array of arrays is one combination each.
fn parse_cell(value: &Value) -> Result<Vec<Combo>, OpError> {
    let object = value
        .as_object()
        .ok_or_else(|| bad("\"cell\" must be an object"))?;
    if let Some(unknown) = object.keys().find(|k| k.as_str() != "msas") {
        return Err(bad(format!(
            "unknown cell field: {unknown}; only \"msas\" is supported"
        )));
    }
    let msas = object
        .get("msas")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("\"cell.msas\" must be an array"))?;
    if msas.is_empty() {
        return Err(bad(
            "\"cell.msas\" must not be empty; send [[]] for the root alone",
        ));
    }
    let combos = if msas.iter().all(Value::is_object) {
        vec![parse_combo(&Value::Array(msas.clone()), "cell.msas")?]
    } else if msas.iter().all(Value::is_array) {
        if msas.len() > MAX_CELL_COMBOS {
            return Err(bad(format!(
                "\"cell.msas\" lists more than {MAX_CELL_COMBOS} combinations"
            )));
        }
        msas.iter()
            .enumerate()
            .map(|(i, c)| parse_combo(c, &format!("cell.msas[{i}]")))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        return Err(bad(
            "\"cell.msas\" holds MSA objects (one combination) or arrays of them (many), not a mix",
        ));
    };
    Ok(combos)
}

fn identity_error(error: SourceIdentityError, target: &Target) -> OpError {
    match error {
        SourceIdentityError::EmptyLexicon | SourceIdentityError::AuthoredIdentityUnavailable => (
            "authored_identity_unavailable",
            "this grammar has no FieldWorks entry/MSA identities for target mode".into(),
        ),
        SourceIdentityError::UnknownEntry => (
            "unknown_entry",
            format!("no compiled source entry {}", target.entry),
        ),
        _ => ("bad_request", format!("{error:?}")),
    }
}

/// The root MSA GUIDs a target addresses: the one it names (directly or through its sense), else every stem MSA of the entry.
fn root_msas(grammar: &Grammar, target: &Target) -> Result<Vec<String>, OpError> {
    let stems =
        stem_msas_of_entry(grammar, &target.entry).map_err(|e| identity_error(e, target))?;
    let sense_msa = match &target.sense {
        None => None,
        Some(sense) => {
            let (owner, msa) = grammar
                .sense_msas
                .get(sense)
                .ok_or(("unknown_sense", format!("no compiled source sense {sense}")))?;
            if *owner != target.entry {
                return Err((
                    "sense_not_owned_by_entry",
                    format!("sense {sense} is not owned by entry {}", target.entry),
                ));
            }
            Some(grammar.representative_msa(msa).to_string())
        }
    };
    match (&target.msa, sense_msa) {
        (Some(msa), Some(from_sense)) => {
            if grammar.representative_msa(msa) != from_sense {
                return Err((
                    "msa_not_owned_by_entry",
                    format!("sense's MSA is {from_sense}, not {msa}"),
                ));
            }
            Ok(vec![msa.clone()])
        }
        (Some(msa), None) => Ok(vec![msa.clone()]),
        (None, Some(from_sense)) => Ok(vec![from_sense]),
        (None, None) => Ok(stems),
    }
}

fn target_json(target: &Target, msa: Option<&str>) -> Value {
    let mut value = json!({"entryGuid": target.entry, "msaGuid": msa});
    if let Some(sense) = &target.sense {
        value["senseGuid"] = json!(sense);
    }
    value
}

fn error_row(id: &Value, code: &str, message: &str, target: Option<Value>) -> Value {
    let mut row = json!({
        "id": id, "type": "error", "code": code, "message": message,
        "status": "error", "cause": {"code": code, "message": message},
    });
    if let Some(target) = target {
        row["target"] = target;
    }
    row
}

fn empty_outcome() -> TracedGenerateOutcome {
    TracedGenerateOutcome {
        seeds: Vec::new(),
        stop_reason: None,
        work_units: 0,
        seed_choices: 0,
        trace_nodes: 0,
    }
}

fn trace_object(outcome: &TracedGenerateOutcome) -> Value {
    json!({
        "seedChoices": outcome.seed_choices,
        "workUnits": outcome.work_units,
        "traceSteps": outcome.trace_nodes,
        "seedsTried": outcome.seeds.iter().take(TRACE_SEEDS_SHOWN).map(|seed| json!({
            "seedKey": seed.seed_key,
            "allomorphs": seed.items.iter().map(render_item).collect::<Vec<_>>(),
            "surfaces": seed.surfaces.len(),
        })).collect::<Vec<_>>(),
        "seedsTriedTruncated": outcome.seeds.len() > TRACE_SEEDS_SHOWN,
    })
}

/// One output row for one (root MSA, combination) run.
#[allow(clippy::too_many_arguments)]
fn result_row(
    options: &Options,
    id: &Value,
    target: &Target,
    msa: Option<&str>,
    cell: Option<(usize, &str)>,
    outcome: &TracedGenerateOutcome,
    cause_override: Option<Value>,
) -> Value {
    let surfaces: usize = outcome.seeds.iter().map(|s| s.surfaces.len()).sum();
    let (status, cause) = if surfaces > 0 {
        ("ok", Value::Null)
    } else if let Some(cause) = cause_override {
        ("none", cause)
    } else if let Some(stop) = outcome.stop_reason {
        (
            "none",
            json!({"code": "cap", "stopReason": stop.code(), "detail": "generation stopped before any surface was found"}),
        )
    } else {
        (
            "none",
            json!({"code": "generated_empty", "seedChoices": outcome.seed_choices,
                "detail": format!("{} seed choices ran and none produced a surface", outcome.seed_choices)}),
        )
    };
    let mut row = render(options, outcome);
    row["id"] = id.clone();
    row["type"] = json!("result");
    row["op"] = json!("generate");
    row["status"] = json!(status);
    row["cause"] = cause;
    row["target"] = target_json(target, msa);
    if let Some((index, key)) = cell {
        row["cellIndex"] = json!(index);
        row["comboKey"] = json!(key);
    }
    if status == "none" || options.trace {
        row["trace"] = trace_object(outcome);
    }
    row
}

fn skipped(stop: TracedStopReason) -> TracedGenerateOutcome {
    TracedGenerateOutcome {
        stop_reason: Some(stop),
        ..empty_outcome()
    }
}

/// Answers one `target` row with one row per (root MSA, combination), in that order. `cell_index` is the row's ordinal among the batch's cell rows. A request that cannot run is a single error row.
pub(crate) fn answer(
    grammar: &Grammar,
    morpher: &Morpher<'_>,
    id: &Value,
    object: &Map<String, Value>,
    default_attested: Option<&AttestedSet>,
    trace_all: bool,
    cell_index: usize,
) -> Vec<Value> {
    match run(
        grammar,
        morpher,
        id,
        object,
        default_attested,
        trace_all,
        cell_index,
    ) {
        Ok(rows) => rows,
        Err((code, message)) => vec![error_row(id, code, &message, None)],
    }
}

fn run(
    grammar: &Grammar,
    morpher: &Morpher<'_>,
    id: &Value,
    object: &Map<String, Value>,
    default_attested: Option<&AttestedSet>,
    trace_all: bool,
    cell_index: usize,
) -> Result<Vec<Value>, OpError> {
    let mut options = parse_options(object, default_attested, true)?;
    options.trace |= trace_all;
    if object.contains_key("request") {
        return Err(bad("send \"target\" or \"request\", not both"));
    }
    let target = parse_target(
        object
            .get("target")
            .ok_or_else(|| bad("\"target\" is required"))?,
    )?;
    let combos = match object.get("cell") {
        Some(cell) => Some(parse_cell(cell)?),
        None => None,
    };
    let roots = root_msas(grammar, &target)?;
    if roots.is_empty() {
        let outcome = empty_outcome();
        let cause =
            json!({"code": "no_stem_msa", "detail": "the entry has no stem MSA to generate from"});
        return Ok(vec![result_row(
            &options,
            id,
            &target,
            None,
            None,
            &outcome,
            Some(cause),
        )]);
    }
    let plain = [Combo {
        key: "-".into(),
        refs: Vec::new(),
    }];
    let list: &[Combo] = combos.as_deref().unwrap_or(&plain);

    // Resolve every (root, combination) before generating anything, so a bad reference refuses the line whole.
    let mut planned = Vec::new();
    let mut rows = Vec::new();
    for msa in &roots {
        let mut per_root = Vec::new();
        let mut failed = None;
        for combo in list {
            let request = ValidatedGenerateRequest {
                root: ValidatedMorphemeRef {
                    entry_guid: target.entry.clone(),
                    msa_guid: msa.clone(),
                    msa_class: FwClass::MoStemMsa,
                    form_guid: None,
                },
                morphemes: combo.refs.clone(),
            };
            match generate_request::resolve_request(grammar, &request) {
                Ok(resolved) => per_root.push(resolved),
                Err(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        match failed {
            None => planned.push((msa.as_str(), per_root)),
            Some(e) => {
                let (code, message) = crate::serve_generate::request_error(e);
                if roots.len() > 1 && target.msa.is_none() && combos.is_none() {
                    rows.push(error_row(
                        id,
                        code,
                        &message,
                        Some(target_json(&target, Some(msa))),
                    ));
                } else {
                    return Err((code, message));
                }
            }
        }
    }

    let started = Instant::now();
    let total_timeout = options.timeout.or(morpher.word_timeout());
    let mut remaining = options.max_work;
    for (msa, resolved_combos) in planned {
        for (combo, resolved) in list.iter().zip(resolved_combos) {
            let cell = combos.as_ref().map(|_| (cell_index, combo.key.as_str()));
            let time_left = total_timeout.map(|t| t.saturating_sub(started.elapsed()));
            let outcome = if remaining == 0 {
                skipped(TracedStopReason::WorkLimit {
                    limit: options.max_work,
                })
            } else if time_left == Some(Duration::ZERO) {
                skipped(TracedStopReason::Deadline)
            } else {
                let limit = remaining.min(options.max_work_per_combo.unwrap_or(usize::MAX));
                let limits = TracedGenerateLimits {
                    work_limit: limit,
                    timeout: time_left,
                    max_seeds: options.max_seeds,
                    max_trace_nodes: options.trace.then_some(options.max_trace_steps),
                };
                morpher.generate_words_traced_filtered(
                    resolved.root,
                    &resolved.others,
                    pg_featstruct::FeatureStruct::EMPTY,
                    &limits,
                    &resolved.filter,
                )
            };
            remaining = remaining.saturating_sub(outcome.work_units);
            rows.push(result_row(
                &options,
                id,
                &target,
                Some(msa),
                cell,
                &outcome,
                None,
            ));
        }
    }
    Ok(rows)
}
