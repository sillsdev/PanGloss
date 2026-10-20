//! `pangloss serve`: loads a project once, then answers line-delimited JSON parse requests on stdin/stdout.
//! Protocol: `docs/formats/serve-protocol.md`.

use std::io::{BufRead, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use pg_grammar::model::Grammar;
use pg_parse::Morpher;
use pg_stats::StepCap;
use serde_json::{json, Value};

use crate::rich_trace;

/// The serve protocol version; bump only for an incompatible wire change.
pub(crate) const SERVE_PROTOCOL_VERSION: u32 = 1;

/// Wall-clock milliseconds of the snapshot read (json) or fwdata import, recorded by the grammar loader for `--timing`.
pub(crate) static IMPORT_MS: AtomicU64 = AtomicU64::new(0);
/// Wall-clock milliseconds of grammar compilation, recorded by the grammar loader for `--timing`.
pub(crate) static COMPILE_MS: AtomicU64 = AtomicU64::new(0);

/// Stores the elapsed milliseconds since `since` into `slot`.
pub(crate) fn record_ms(slot: &AtomicU64, since: Instant) {
    slot.store(since.elapsed().as_millis() as u64, Ordering::Relaxed);
}

struct ServeArgs {
    project: String,
    step_cap: StepCap,
    work_cap: Option<StepCap>,
    word_timeout: Option<Duration>,
    guess: bool,
    timing: bool,
}

fn parse_args(args: &[String]) -> Result<ServeArgs, String> {
    let mut project = None;
    let mut step_cap = crate::DEFAULT_STEP_CAP;
    let mut work_cap = None;
    let mut word_timeout = None;
    let mut guess = false;
    let mut timing = false;
    let ms = |v: &str| -> Result<Duration, String> {
        v.parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| "--word-timeout-ms must be an integer".to_string())
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--project" => {
                project = Some(it.next().ok_or("--project requires a value")?.clone());
            }
            s if s.starts_with("--project=") => project = Some(s["--project=".len()..].to_string()),
            "--guess" => guess = true,
            "--timing" => timing = true,
            "--step-cap" => {
                step_cap = it
                    .next()
                    .ok_or("--step-cap requires a value")?
                    .parse::<StepCap>()?;
            }
            s if s.starts_with("--step-cap=") => {
                step_cap = s["--step-cap=".len()..].parse::<StepCap>()?;
            }
            "--work-cap" => {
                let v = it.next().ok_or("--work-cap requires a value")?;
                work_cap = Some(crate::parse_work_cap(v)?);
            }
            s if s.starts_with("--work-cap=") => {
                work_cap = Some(crate::parse_work_cap(&s["--work-cap=".len()..])?);
            }
            "--word-timeout-ms" => {
                word_timeout = Some(ms(it
                    .next()
                    .ok_or("--word-timeout-ms requires a value")?)?);
            }
            s if s.starts_with("--word-timeout-ms=") => {
                word_timeout = Some(ms(&s["--word-timeout-ms=".len()..])?);
            }
            s => {
                crate::reject_unknown_option("serve", s)?;
                return Err(format!("unexpected argument: {s}"));
            }
        }
    }
    Ok(ServeArgs {
        project: project.ok_or("--project <path> is required")?,
        step_cap,
        work_cap,
        word_timeout,
        guess,
        timing,
    })
}

/// Writes one protocol line and flushes; an `Err` means the host closed stdout.
fn emit(value: &Value) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, value)?;
    out.write_all(b"\n")?;
    out.flush()
}

fn fatal(code: &str, message: &str) -> ExitCode {
    eprintln!("pangloss serve: {code}: {message}");
    let _ = emit(&json!({"type": "fatal", "code": code, "message": message}));
    ExitCode::FAILURE
}

pub(crate) fn run_serve(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args) {
        Ok(p) => p,
        Err(e) => return fatal("bad_arguments", &e),
    };
    let load_started = Instant::now();
    let (grammar, warnings, metadata) =
        match crate::load_grammar_with_trace_metadata(&parsed.project) {
            Ok(loaded) => loaded,
            Err(e) => return fatal("load_failed", &e),
        };
    crate::print_grammar_warnings(&warnings);
    let morpher = Morpher::new(&grammar, parsed.step_cap.as_morpher_cap())
        .with_work_cap(crate::resolved_work_cap(parsed.step_cap, parsed.work_cap))
        .with_word_timeout(parsed.word_timeout);
    let opts = pg_parse::ParseOptions::default().with_guess_root(parsed.guess);
    let load_ms = load_started.elapsed().as_millis() as u64;

    let mut ready = json!({"type": "ready", "protocol": SERVE_PROTOCOL_VERSION, "loadMs": load_ms});
    if parsed.timing {
        ready["timing"] = json!({
            "importMs": IMPORT_MS.load(Ordering::Relaxed),
            "compileMs": COMPILE_MS.load(Ordering::Relaxed),
        });
    }
    if emit(&ready).is_err() {
        return ExitCode::FAILURE;
    }

    let ctx = Context {
        grammar: &grammar,
        morpher: &morpher,
        opts: &opts,
        metadata: &metadata,
        timing: parsed.timing,
    };
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match input.read_until(b'\n', &mut buf) {
            Ok(0) => return ExitCode::SUCCESS,
            Ok(_) => {}
            Err(e) => {
                eprintln!("pangloss serve: read stdin: {e}");
                return ExitCode::FAILURE;
            }
        }
        let (response, stop) = match std::str::from_utf8(&buf) {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => handle_line(&ctx, line.trim()),
            Err(_) => (
                error_response(
                    &Value::Null,
                    "malformed_json",
                    "request line is not valid UTF-8",
                ),
                false,
            ),
        };
        if emit(&response).is_err() {
            return ExitCode::FAILURE;
        }
        if stop {
            return ExitCode::SUCCESS;
        }
    }
}

struct Context<'a> {
    grammar: &'a Grammar,
    morpher: &'a Morpher<'a>,
    opts: &'a pg_parse::ParseOptions,
    metadata: &'a rich_trace::TraceMetadata,
    timing: bool,
}

fn error_response(id: &Value, code: &str, message: &str) -> Value {
    json!({"id": id, "type": "error", "code": code, "message": message})
}

/// Returns the response line and whether the process should exit after writing it.
fn handle_line(ctx: &Context, line: &str) -> (Value, bool) {
    let request: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return (
                error_response(&Value::Null, "malformed_json", &e.to_string()),
                false,
            )
        }
    };
    let Some(object) = request.as_object() else {
        return (
            error_response(&Value::Null, "bad_request", "request must be a JSON object"),
            false,
        );
    };
    let id = match object.get("id") {
        Some(Value::String(s)) => Value::String(s.clone()),
        Some(_) => {
            return (
                error_response(&Value::Null, "bad_request", "\"id\" must be a string"),
                false,
            )
        }
        None => {
            return (
                error_response(&Value::Null, "bad_request", "\"id\" is required"),
                false,
            )
        }
    };
    match object.get("op").and_then(Value::as_str) {
        Some("shutdown") => (json!({"id": id, "type": "bye"}), true),
        Some("parse") => {
            let Some(word) = object.get("word").and_then(Value::as_str) else {
                return (
                    error_response(&id, "bad_request", "\"word\" must be a string"),
                    false,
                );
            };
            if word.is_empty() {
                return (
                    error_response(&id, "bad_request", "\"word\" must not be empty"),
                    false,
                );
            }
            let details = match object.get("traceDetails") {
                None => false,
                Some(Value::Bool(b)) => *b,
                Some(_) => {
                    return (
                        error_response(&id, "bad_request", "\"traceDetails\" must be a boolean"),
                        false,
                    )
                }
            };
            let outcome = catch_unwind(AssertUnwindSafe(|| parse_word(ctx, word, details)));
            let response = match outcome {
                Ok(Ok((result, parse_ms, render_ms))) => {
                    let mut r = json!({"id": id, "type": "result", "result": result});
                    if ctx.timing {
                        r["timing"] = json!({"parseMs": parse_ms, "renderMs": render_ms});
                    }
                    r
                }
                Ok(Err((code, message))) => error_response(&id, code, &message),
                Err(_) => error_response(&id, "internal", "parser panicked; see stderr"),
            };
            (response, false)
        }
        Some(other) => (
            error_response(&id, "unknown_op", &format!("unknown op: {other}")),
            false,
        ),
        None => (
            error_response(&id, "bad_request", "\"op\" must be a string"),
            false,
        ),
    }
}

/// The same work `pangloss parse <grammar> <word> --trace --trace-format json [--trace-details]` does after loading; `Ok` carries the JSON document, `parse_ms` and `render_ms`.
fn parse_word(
    ctx: &Context,
    word: &str,
    details: bool,
) -> Result<(Value, u64, u64), (&'static str, String)> {
    let render_err = |e: String| ("render_failed", e);
    if details {
        let sink = pg_rules::trace::TreeTraceSink::with_failure_context();
        let started = Instant::now();
        let (outcome, rows) = ctx
            .morpher
            .parse_word_traced_with_stats(word, ctx.opts, &sink);
        let parse_ms = started.elapsed().as_millis() as u64;
        let rendered = rich_trace::render(
            ctx.grammar,
            &sink,
            sink.root(),
            word,
            &outcome,
            &rows,
            started.elapsed(),
            ctx.metadata,
        )
        .map_err(render_err)?;
        let render_ms = (started.elapsed().as_millis() as u64).saturating_sub(parse_ms);
        let value = serde_json::from_str(&rendered).map_err(|e| render_err(e.to_string()))?;
        return Ok((value, parse_ms, render_ms));
    }
    let sink = pg_rules::trace::TreeTraceSink::new();
    let started = Instant::now();
    let outcome = ctx.morpher.parse_word_traced(word, ctx.opts, &sink);
    let parse_ms = started.elapsed().as_millis() as u64;
    crate::require_complete_parse(&outcome).map_err(|e| ("parse_incomplete", e))?;
    let render_started = Instant::now();
    let value = match sink.root() {
        Some(root) => {
            let rendered = crate::trace_render::render_json(ctx.grammar, &sink, root);
            serde_json::from_str(&rendered).map_err(|e| render_err(e.to_string()))?
        }
        None => Value::Null,
    };
    Ok((value, parse_ms, render_started.elapsed().as_millis() as u64))
}
