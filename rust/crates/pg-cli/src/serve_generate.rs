//! `pangloss serve`'s `generate` op: authored-identity generation with a per-seed rule trace.
//! Protocol: `docs/formats/serve-protocol.md`.

use std::collections::HashSet;
use std::time::Duration;

use pg_grammar::model::Grammar;
use pg_parse::morpher::{
    GenerateStep, GeneratedSeed, Morpher, SeedItem, SeedSurface, SegmentSpan, SpanAnalysis,
    SpanReason, StepRule, TracedGenerateLimits, TracedGenerateOutcome, TracedStopReason,
};
use serde_json::{json, Map, Value};
use unicode_normalization::UnicodeNormalization;

use crate::generate_request::{self, GenerateRequestError};

/// Version of the `generate` op's request and response shapes.
const OP_VERSION: u32 = 1;
/// Version of the `generate --request` document the op nests under `request`.
const REQUEST_SCHEMA_VERSION: u32 = 1;

pub(crate) const DEFAULT_MAX_SEEDS: usize = 10_000;
pub(crate) const MAX_MAX_SEEDS: usize = 1_000_000;
pub(crate) const DEFAULT_MAX_TRACE_STEPS: usize = 50_000;
pub(crate) const MAX_MAX_TRACE_STEPS: usize = 5_000_000;
/// Most words an attested list may hold, inline or in a file.
pub(crate) const MAX_ATTESTED_WORDS: usize = 1_000_000;

/// A set of attested words, each in NFD so a composed and a decomposed spelling of one word agree.
pub(crate) type AttestedSet = HashSet<String>;

/// Reads a word list, one word per line, blank lines skipped, into an [`AttestedSet`].
pub(crate) fn load_attested_file(path: &str) -> Result<AttestedSet, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("cannot read word list {path}: {e}"))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut set = AttestedSet::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if set.len() >= MAX_ATTESTED_WORDS && !set.contains(&line.nfd().collect::<String>()) {
            return Err(format!(
                "word list {path} has more than {MAX_ATTESTED_WORDS} words"
            ));
        }
        set.insert(line.nfd().collect());
    }
    Ok(set)
}

/// Every `stopReason` the op can emit, in the order `--describe` lists them.
pub(crate) const STOP_REASONS: [&str; 5] = [
    "seed_product_exceeds_budget",
    "work_limit",
    "deadline",
    "trace_limit",
    "seed_limit",
];

/// Every `reason` a morpheme's null spans or an unattributed run can carry, in the order `--describe` lists them.
pub(crate) const SPAN_REASONS: [&str; 11] = [
    "span_unknown_truncation",
    "span_ambiguous_copy",
    "span_lost_by_rule",
    "nested_morph",
    "compound_unsupported",
    "repeated_morpheme",
    "morph_not_recorded",
    "segments_not_aligned",
    "inserted_by_rule",
    "no_morph_owner",
    "unrequested_morpheme",
];

/// Every error code the op can emit for a request that never ran.
pub(crate) const ERROR_CODES: [&str; 12] = [
    "bad_request",
    "request_too_large",
    "authored_identity_unavailable",
    "unknown_entry",
    "unknown_msa",
    "msa_not_owned_by_entry",
    "ambiguous_identity",
    "unsupported_msa_class",
    "unsupported_form_selection",
    "unknown_form",
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
            "request": "the generate --request v1 object; a root or stem morpheme may carry formGuid to run only the seeds that use that form (an affix may not)",
            "attestedWords": "array of strings; each surface then carries attested, true when its NFD form is in the list",
            "attestedWordsFile": "path to a word list, one word per line; as attestedWords. At most one of the two",
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
        "surfaceFields": ["surface", "derivations", "attested", "steps", "stepsUnresolved", "alternatives", "normalized", "morphSpans"],
        "morphSpanFields": ["derivations", "segmentCount", "morphemes", "unattributed"],
        "morphemeSpanFields": ["item", "spans", "reason", "rule"],
        "spanFields": ["segments", "offsets"],
        "unattributedFields": ["segments", "offsets", "reason", "rule"],
        "spanReasons": SPAN_REASONS,
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
    /// The list each surface is marked against; `None` adds no `attested` field.
    attested: Option<AttestedSet>,
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

fn parse_attested(
    object: &Map<String, Value>,
    default: Option<&AttestedSet>,
) -> Result<Option<AttestedSet>, OpError> {
    match (object.get("attestedWords"), object.get("attestedWordsFile")) {
        (Some(_), Some(_)) => Err(bad(
            "send at most one of \"attestedWords\" and \"attestedWordsFile\"",
        )),
        (Some(words), None) => {
            let words = words
                .as_array()
                .ok_or_else(|| bad("\"attestedWords\" must be an array of strings"))?;
            if words.len() > MAX_ATTESTED_WORDS {
                return Err(bad(format!(
                    "\"attestedWords\" holds at most {MAX_ATTESTED_WORDS} words"
                )));
            }
            words
                .iter()
                .map(|word| {
                    word.as_str()
                        .map(|w| w.nfd().collect::<String>())
                        .ok_or_else(|| bad("\"attestedWords\" must be an array of strings"))
                })
                .collect::<Result<AttestedSet, _>>()
                .map(Some)
        }
        (None, Some(path)) => {
            let path = path
                .as_str()
                .ok_or_else(|| bad("\"attestedWordsFile\" must be a path string"))?;
            load_attested_file(path).map(Some).map_err(bad)
        }
        (None, None) => Ok(default.cloned()),
    }
}

fn parse_options(
    object: &Map<String, Value>,
    default_attested: Option<&AttestedSet>,
) -> Result<Options, OpError> {
    const KNOWN: [&str; 10] = [
        "id",
        "op",
        "request",
        "trace",
        "maxSeeds",
        "maxTraceSteps",
        "maxWork",
        "wordTimeoutMs",
        "attestedWords",
        "attestedWordsFile",
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
        attested: parse_attested(object, default_attested)?,
    })
}

fn request_error(error: GenerateRequestError) -> OpError {
    let message = error.to_string();
    let code = match error {
        GenerateRequestError::RequestTooLarge { .. } => "request_too_large",
        GenerateRequestError::UnsupportedSchemaVersion { .. } => "unsupported_schema_version",
        GenerateRequestError::UnsupportedMsaClass { .. } => "unsupported_msa_class",
        GenerateRequestError::UnsupportedFormSelection { .. } => "unsupported_form_selection",
        GenerateRequestError::UnknownForm { .. } => "unknown_form",
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
    default_attested: Option<&AttestedSet>,
) -> Result<Value, OpError> {
    let options = parse_options(object, default_attested)?;
    let request = object
        .get("request")
        .ok_or_else(|| bad("\"request\" is required"))?;
    let bytes = serde_json::to_vec(request).map_err(|e| bad(e.to_string()))?;
    let validated = generate_request::parse_request_json(&bytes).map_err(request_error)?;
    let generate_request::ResolvedRequest {
        root,
        others,
        filter,
    } = generate_request::resolve_request(grammar, &validated).map_err(request_error)?;
    let limits = TracedGenerateLimits {
        work_limit: options.max_work,
        timeout: options.timeout.or(morpher.word_timeout()),
        max_seeds: options.max_seeds,
        max_trace_nodes: options.trace.then_some(options.max_trace_steps),
    };
    let outcome = morpher.generate_words_traced_filtered(
        root,
        &others,
        pg_featstruct::FeatureStruct::EMPTY,
        &limits,
        &filter,
    );
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
    if let Some(attested) = &options.attested {
        value["attested"] =
            Value::Bool(attested.contains(&surface.surface.nfd().collect::<String>()));
    }
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
        value["normalized"] = json!(surface.normalized);
        value["morphSpans"] =
            Value::Array(surface.morph_spans.iter().map(render_analysis).collect());
    }
    value
}

fn render_analysis(analysis: &SpanAnalysis) -> Value {
    let mut value = render_surface_spans(&analysis.spans);
    value["derivations"] = json!(analysis.derivations);
    value
}

/// One surface's spans as the wire object, without `derivations`.
pub(crate) fn render_surface_spans(spans: &pg_parse::morpher::SurfaceSpans) -> Value {
    json!({
        "segmentCount": spans.segment_count,
        "morphemes": spans.morphemes.iter().map(|m| json!({
            "item": m.item,
            "spans": m.spans.as_ref().map(|list| list.iter().map(render_span).collect::<Vec<_>>()),
            "reason": m.reason.as_ref().map(|r| r.code),
            "rule": m.reason.as_ref().and_then(|r| r.rule.as_ref()).map(render_rule),
        })).collect::<Vec<_>>(),
        "unattributed": spans.unattributed.iter().map(|u| {
            let mut value = render_span(&u.span);
            let SpanReason { code, rule } = &u.reason;
            value["reason"] = json!(code);
            value["rule"] = json!(rule.as_ref().map(render_rule));
            value
        }).collect::<Vec<_>>(),
    })
}

pub(crate) fn render_span(span: &SegmentSpan) -> Value {
    json!({
        "segments": [span.segments.0, span.segments.1],
        "offsets": span.offsets.map(|(start, end)| [start, end]),
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use pg_parse::morpher::{ItemSpans, SurfaceSpans, UnattributedSpan};

    fn keys(value: &Value) -> Vec<String> {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    }

    fn declared(field: &str) -> Vec<String> {
        let mut names: Vec<String> = describe()[field]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn attested_compares_composed_and_decomposed_spellings_as_one_word() {
        let mut object = Map::new();
        object.insert("attestedWords".into(), json!(["cafe\u{301}"]));
        let attested = parse_attested(&object, None).unwrap();
        let options = Options {
            trace: false,
            max_seeds: 1,
            max_trace_steps: 1,
            max_work: 1,
            timeout: None,
            attested,
        };
        let surface = |text: &str| SeedSurface {
            surface: text.into(),
            steps: Vec::new(),
            derivations: 1,
            steps_unresolved: false,
            alternatives: Vec::new(),
            morph_spans: Vec::new(),
            normalized: None,
        };
        assert_eq!(
            render_surface(&options, &surface("caf\u{e9}"))["attested"],
            true
        );
        assert_eq!(
            render_surface(&options, &surface("cafe\u{301}"))["attested"],
            true
        );
        assert_eq!(
            render_surface(&options, &surface("cafes"))["attested"],
            false
        );
    }

    #[test]
    fn a_word_list_file_ignores_a_leading_byte_order_mark() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("words.txt");
        std::fs::write(&path, "\u{feff}rannas\nother\n").unwrap();
        let set = load_attested_file(path.to_str().unwrap()).unwrap();
        assert!(set.contains("rannas") && set.contains("other"), "{set:?}");
    }

    #[test]
    fn unattributed_runs_and_null_morphemes_carry_exactly_the_declared_fields() {
        let rule = StepRule {
            kind: "phon",
            id: "p".into(),
            name: Some("Epenthesis".into()),
            guid: None,
        };
        let span = SegmentSpan {
            segments: (1, 2),
            offsets: Some((1, 2)),
        };
        let analysis = SpanAnalysis {
            derivations: 1,
            spans: SurfaceSpans {
                segment_count: 3,
                morphemes: vec![ItemSpans {
                    item: 0,
                    spans: None,
                    reason: Some(SpanReason {
                        code: "span_lost_by_rule",
                        rule: Some(rule.clone()),
                    }),
                }],
                unattributed: vec![UnattributedSpan {
                    span,
                    reason: SpanReason {
                        code: "inserted_by_rule",
                        rule: Some(rule),
                    },
                }],
            },
        };
        let value = render_analysis(&analysis);
        assert_eq!(keys(&value), declared("morphSpanFields"));
        assert_eq!(keys(&value["morphemes"][0]), declared("morphemeSpanFields"));
        assert_eq!(value["morphemes"][0]["spans"], Value::Null);
        assert_eq!(value["morphemes"][0]["rule"]["name"], "Epenthesis");
        assert_eq!(
            keys(&value["unattributed"][0]),
            declared("unattributedFields")
        );
        assert_eq!(
            keys(&render_span(&span)),
            declared("spanFields"),
            "a run's own fields"
        );
        for code in ["span_lost_by_rule", "inserted_by_rule"] {
            assert!(SPAN_REASONS.contains(&code));
        }
    }
}
