//! `pangloss serve`'s `generate` op: authored-identity generation with a per-seed rule trace.
//! Protocol: `docs/formats/serve-protocol.md`.

use std::time::Duration;

use pg_grammar::model::Grammar;
use pg_parse::morpher::{
    GenerateStep, GeneratedSeed, Morpher, SeedItem, SeedSurface, StepRule, TracedGenerateLimits,
    TracedGenerateOutcome, TracedStopReason,
};
use serde_json::{json, Map, Value};

use crate::generate_request::{self, GenerateRequestError};

/// Version of the `generate` op's request and response shapes.
const OP_VERSION: u32 = 1;
/// Version of the `generate --request` document the op nests under `request`.
const REQUEST_SCHEMA_VERSION: u32 = 1;

pub(crate) const DEFAULT_MAX_SEEDS: usize = 10_000;
pub(crate) const MAX_MAX_SEEDS: usize = 1_000_000;
pub(crate) const DEFAULT_MAX_TRACE_STEPS: usize = 50_000;
pub(crate) const MAX_MAX_TRACE_STEPS: usize = 5_000_000;

/// Every `stopReason` the op can emit, in the order `--describe` lists them.
pub(crate) const STOP_REASONS: [&str; 5] = [
    "seed_product_exceeds_budget",
    "work_limit",
    "deadline",
    "trace_limit",
    "seed_limit",
];

/// Every error code the op can emit for a request that never ran.
pub(crate) const ERROR_CODES: [&str; 11] = [
    "bad_request",
    "request_too_large",
    "authored_identity_unavailable",
    "unknown_entry",
    "unknown_msa",
    "msa_not_owned_by_entry",
    "ambiguous_identity",
    "unsupported_msa_class",
    "unsupported_form_selection",
    "unsupported_schema_version",
    "internal",
];

/// The `interfaces.serveOps.generate` capability document.
pub(crate) fn describe() -> Value {
    json!({
        "version": OP_VERSION,
        "trace": true,
        "requestSchema": REQUEST_SCHEMA_VERSION,
        "requestFields": {
            "request": "the generate --request v1 object",
            "trace": "boolean, default false; true records each surface's rule steps",
            "maxSeeds": "integer >= 1",
            "maxTraceSteps": "integer >= 1; counts retained trace nodes across every seed",
            "maxWork": "integer >= 1; lowers the generation cap for this request",
            "wordTimeoutMs": "integer >= 0; wall-clock deadline for this request",
        },
        "limits": {
            "generationCap": crate::DEFAULT_GENERATION_WORK_CAP,
            "maxSeeds": {"default": DEFAULT_MAX_SEEDS, "max": MAX_MAX_SEEDS},
            "maxTraceSteps": {"default": DEFAULT_MAX_TRACE_STEPS, "max": MAX_MAX_TRACE_STEPS},
        },
        "stopReasons": STOP_REASONS,
        "errorCodes": ERROR_CODES,
        "resultFields": ["complete", "stopReason", "workUnits", "seedChoices", "traceSteps", "limits", "seeds"],
        "seedFields": ["seedKey", "allomorphs", "surfaces", "traceTruncated"],
        "surfaceFields": ["surface", "derivations", "steps", "stepsUnresolved", "alternatives"],
        "allomorphFields": ["role", "entryGuid", "msaGuid", "allomorphId", "formGuid", "shape", "ruleName"],
        "ruleFields": ["kind", "id", "name", "guid"],
        "stepFields": ["stratum", "rule", "input", "output", "subrule"],
    })
}

/// A `generate` request that never ran: the error code and message.
pub(crate) type OpError = (&'static str, String);

struct Options {
    trace: bool,
    max_seeds: usize,
    max_trace_steps: usize,
    max_work: usize,
    timeout: Option<Duration>,
}

fn bad(message: impl Into<String>) -> OpError {
    ("bad_request", message.into())
}

fn bounded_count(
    object: &Map<String, Value>,
    key: &str,
    default: usize,
    max: usize,
    min: usize,
) -> Result<usize, OpError> {
    match object.get(key) {
        None => Ok(default),
        Some(value) => {
            let n = value
                .as_u64()
                .ok_or_else(|| bad(format!("\"{key}\" must be a non-negative integer")))?;
            if (n as usize) < min || n > max as u64 {
                return Err(bad(format!("\"{key}\" must be between {min} and {max}")));
            }
            Ok(n as usize)
        }
    }
}

fn parse_options(object: &Map<String, Value>) -> Result<Options, OpError> {
    const KNOWN: [&str; 8] = [
        "id",
        "op",
        "request",
        "trace",
        "maxSeeds",
        "maxTraceSteps",
        "maxWork",
        "wordTimeoutMs",
    ];
    if let Some(unknown) = object.keys().find(|key| !KNOWN.contains(&key.as_str())) {
        return Err(bad(format!("unknown generate field: {unknown}")));
    }
    let trace = match object.get("trace") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(bad("\"trace\" must be a boolean")),
    };
    let timeout = match object.get("wordTimeoutMs") {
        None => None,
        Some(value) => {
            Some(Duration::from_millis(value.as_u64().ok_or_else(|| {
                bad("\"wordTimeoutMs\" must be a non-negative integer")
            })?))
        }
    };
    Ok(Options {
        trace,
        max_seeds: bounded_count(object, "maxSeeds", DEFAULT_MAX_SEEDS, MAX_MAX_SEEDS, 1)?,
        max_trace_steps: bounded_count(
            object,
            "maxTraceSteps",
            DEFAULT_MAX_TRACE_STEPS,
            MAX_MAX_TRACE_STEPS,
            1,
        )?,
        max_work: bounded_count(
            object,
            "maxWork",
            crate::DEFAULT_GENERATION_WORK_CAP,
            crate::DEFAULT_GENERATION_WORK_CAP,
            1,
        )?,
        timeout,
    })
}

fn request_error(error: GenerateRequestError) -> OpError {
    let message = error.to_string();
    let code = match error {
        GenerateRequestError::RequestTooLarge { .. } => "request_too_large",
        GenerateRequestError::UnsupportedSchemaVersion { .. } => "unsupported_schema_version",
        GenerateRequestError::UnsupportedMsaClass { .. } => "unsupported_msa_class",
        GenerateRequestError::UnsupportedFormSelection { .. } => "unsupported_form_selection",
        GenerateRequestError::AuthoredIdentityUnavailable => "authored_identity_unavailable",
        GenerateRequestError::UnknownEntry { .. } => "unknown_entry",
        GenerateRequestError::UnknownMsa { .. } => "unknown_msa",
        GenerateRequestError::MsaNotOwnedByEntry { .. } => "msa_not_owned_by_entry",
        GenerateRequestError::AmbiguousIdentity { .. } => "ambiguous_identity",
        _ => "bad_request",
    };
    (code, message)
}

/// Runs one `generate` op. `Err` means the request never ran; a cap or deadline is an `Ok` result with `complete: false`.
pub(crate) fn run(
    grammar: &Grammar,
    morpher: &Morpher<'_>,
    object: &Map<String, Value>,
) -> Result<Value, OpError> {
    let options = parse_options(object)?;
    let request = object
        .get("request")
        .ok_or_else(|| bad("\"request\" is required"))?;
    let bytes = serde_json::to_vec(request).map_err(|e| bad(e.to_string()))?;
    let validated = generate_request::parse_request_json(&bytes).map_err(request_error)?;
    let (root, others) =
        generate_request::resolve_request(grammar, &validated).map_err(request_error)?;
    let limits = TracedGenerateLimits {
        work_limit: options.max_work,
        timeout: options.timeout.or(morpher.word_timeout()),
        max_seeds: options.max_seeds,
        max_trace_nodes: options.trace.then_some(options.max_trace_steps),
    };
    let outcome =
        morpher.generate_words_traced(root, &others, pg_featstruct::FeatureStruct::EMPTY, &limits);
    Ok(render(&options, &outcome))
}

fn render(options: &Options, outcome: &TracedGenerateOutcome) -> Value {
    let stop = outcome.stop_reason.map(TracedStopReason::code);
    json!({
        "complete": outcome.stop_reason.is_none(),
        "stopReason": stop,
        "workUnits": outcome.work_units,
        "seedChoices": outcome.seed_choices,
        "traceSteps": outcome.trace_nodes,
        "limits": {
            "generationCap": options.max_work,
            "maxSeeds": options.max_seeds,
            "maxTraceSteps": options.max_trace_steps,
        },
        "seeds": outcome.seeds.iter().map(|seed| render_seed(options, seed)).collect::<Vec<_>>(),
    })
}

fn render_seed(options: &Options, seed: &GeneratedSeed) -> Value {
    json!({
        "seedKey": seed.seed_key,
        "allomorphs": seed.items.iter().map(render_item).collect::<Vec<_>>(),
        "traceTruncated": seed.trace_truncated,
        "surfaces": seed.surfaces.iter().map(|s| render_surface(options, s)).collect::<Vec<_>>(),
    })
}

fn render_item(item: &SeedItem) -> Value {
    json!({
        "role": item.role.code(),
        "entryGuid": item.entry_guid,
        "msaGuid": item.msa_guid,
        "allomorphId": item.allomorph_id,
        "formGuid": item.form_guid,
        "shape": item.shape,
        "ruleName": item.rule_name,
    })
}

fn render_rule(rule: &StepRule) -> Value {
    json!({"kind": rule.kind, "id": rule.id, "name": rule.name, "guid": rule.guid})
}

fn render_surface(options: &Options, surface: &SeedSurface) -> Value {
    let mut value = json!({"surface": surface.surface, "derivations": surface.derivations});
    if options.trace {
        value["steps"] = render_steps(&surface.steps);
        value["stepsUnresolved"] = Value::Bool(surface.steps_unresolved);
        value["alternatives"] = Value::Array(
            surface
                .alternatives
                .iter()
                .map(|steps| render_steps(steps))
                .collect(),
        );
    }
    value
}

fn render_steps(steps: &[GenerateStep]) -> Value {
    Value::Array(
        steps
            .iter()
            .map(|step| {
                json!({
                    "stratum": step.stratum,
                    "rule": render_rule(&step.rule),
                    "input": step.input,
                    "output": step.output,
                    "subrule": step.subrule,
                })
            })
            .collect(),
    )
}
