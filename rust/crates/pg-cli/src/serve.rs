//! `pangloss serve`: loads a project once, then answers line-delimited JSON parse requests on stdin/stdout.
//! Protocol: `docs/formats/serve-protocol.md`.

use std::io::{BufRead, Read, Write};
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
    guess_arg: Option<bool>,
    timing: bool,
}

fn parse_args(args: &[String]) -> Result<ServeArgs, String> {
    let mut project = None;
    let mut step_cap = crate::DEFAULT_STEP_CAP;
    let mut work_cap = None;
    let mut word_timeout = None;
    let mut guess_arg = None;
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
            s if s == "--guess" || s.starts_with("--guess=") => {
                guess_arg = Some(crate::parse_guess_flag(s)?)
            }
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
        guess_arg,
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
    let morpher = crate::with_guess_override(
        Morpher::new(&grammar, parsed.step_cap.as_morpher_cap())
            .with_work_cap(crate::resolved_work_cap(parsed.step_cap, parsed.work_cap))
            .with_word_timeout(parsed.word_timeout),
        parsed.guess_arg,
    );
    let opts = morpher.project_parse_options();
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
        guess: parsed.guess_arg == Some(true),
    };
    let requests = spawn_reader();
    loop {
        let Ok(item) = requests.recv() else {
            return ExitCode::SUCCESS;
        };
        let (response, stop) = match item {
            Request::Line(buf) => match std::str::from_utf8(&buf) {
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
            },
            Request::TooLarge => (
                error_response(
                    &Value::Null,
                    "request_too_large",
                    &format!("request line exceeds {MAX_REQUEST_BYTES} bytes"),
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

/// The longest accepted request line, newline excluded.
const MAX_REQUEST_BYTES: usize = 1 << 20;

enum Request {
    Line(Vec<u8>),
    TooLarge,
}

/// Reads stdin on its own thread so end of input is noticed even while a parse runs: EOF exits the whole process at once, abandoning any in-flight or queued request.
fn spawn_reader() -> std::sync::mpsc::Receiver<Request> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("serve-stdin".into())
        .spawn(move || {
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            let mut buf = Vec::new();
            loop {
                buf.clear();
                let read = (&mut input)
                    .take(MAX_REQUEST_BYTES as u64 + 2)
                    .read_until(b'\n', &mut buf);
                match read {
                    Ok(0) => std::process::exit(0),
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("pangloss serve: read stdin: {e}");
                        std::process::exit(1);
                    }
                }
                let terminated = buf.last() == Some(&b'\n');
                let body_len = buf.len() - usize::from(terminated);
                let item = if body_len > MAX_REQUEST_BYTES {
                    let mut ended = terminated;
                    while !ended {
                        buf.clear();
                        match (&mut input).take(1 << 16).read_until(b'\n', &mut buf) {
                            Ok(0) | Err(_) => std::process::exit(0),
                            Ok(_) => ended = buf.last() == Some(&b'\n'),
                        }
                    }
                    Request::TooLarge
                } else {
                    Request::Line(std::mem::take(&mut buf))
                };
                if tx.send(item).is_err() {
                    return;
                }
            }
        })
        .expect("spawn serve stdin reader");
    rx
}

struct Context<'a> {
    grammar: &'a Grammar,
    morpher: &'a Morpher<'a>,
    opts: &'a pg_parse::ParseOptions,
    metadata: &'a rich_trace::TraceMetadata,
    timing: bool,
    guess: bool,
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
                Ok(Ok(served)) => {
                    let mut r = json!({
                        "id": id,
                        "type": "result",
                        "exitCode": served.exit_code,
                        "status": served.status,
                        "steps": served.steps,
                        "workSteps": served.work_steps,
                        "guessed": served.guessed,
                        "result": served.result,
                    });
                    if ctx.timing {
                        r["timing"] =
                            json!({"parseMs": served.parse_ms, "renderMs": served.render_ms});
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

/// One answered parse: the cold command's stdout and exit code, the word's search counts, and the phase timings.
struct Served {
    result: Value,
    exit_code: i32,
    status: &'static str,
    /// Whether the guesser found the analyses, whatever the `--guess` flag.
    guessed: bool,
    steps: usize,
    work_steps: usize,
    parse_ms: u64,
    render_ms: u64,
}

/// The same work `pangloss parse` does after loading. `traceDetails: true` is `parse <p> <w> --trace --trace-format json --trace-details`; `false` is `parse <p> <w> --trace --trace-format json`. `result` is the cold stdout: a JSON document, or the verbatim text for `false`.
fn parse_word(ctx: &Context, word: &str, details: bool) -> Result<Served, (&'static str, String)> {
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
        return Ok(Served {
            result: value,
            exit_code: 0,
            status: crate::outcome_status(&outcome),
            guessed: outcome.guessed,
            steps: outcome.steps,
            work_steps: outcome.work_steps,
            parse_ms,
            render_ms,
        });
    }
    let sink = pg_rules::trace::TreeTraceSink::new();
    let started = Instant::now();
    let outcome = ctx.morpher.parse_word_traced(word, ctx.opts, &sink);
    let parse_ms = started.elapsed().as_millis() as u64;
    let render_started = Instant::now();
    let mut stdout = format!("{}\t{}\n", word, outcome.signature());
    if ctx.guess {
        stdout.push_str(&format!("guessed:\t{}\n", outcome.guessed));
    }
    if let Some(root) = sink.root() {
        stdout.push_str(&crate::trace_render::render_json(ctx.grammar, &sink, root));
    }
    let exit_code = i32::from(crate::require_complete_parse(&outcome).is_err());
    Ok(Served {
        result: Value::String(stdout),
        exit_code,
        status: crate::outcome_status(&outcome),
        guessed: outcome.guessed,
        steps: outcome.steps,
        work_steps: outcome.work_steps,
        parse_ms,
        render_ms: render_started.elapsed().as_millis() as u64,
    })
}
