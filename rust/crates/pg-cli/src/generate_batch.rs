//! `generate --requests <file.jsonl|->`: a JSONL batch of `serve` `generate` requests answered one row per line.
//! Format: `docs/generation.md`, "Batch requests".

use std::io::{BufRead, Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};

use pg_grammar::model::Grammar;
use pg_parse::Morpher;
use serde_json::{json, Value};

use crate::generate_target;
use crate::serve::{error_response, generate_response, request_envelope, MAX_REQUEST_BYTES};
use crate::serve_generate::AttestedSet;

/// Most `none` rows the stdout summary quotes.
const SUMMARY_NONE_EXAMPLES: usize = 5;
/// Longest `id` the summary quotes.
const SUMMARY_ID_CHARS: usize = 120;

/// How a batch runs beyond the grammar: the default attested list, whether every `target` row carries a trace, and where to start and stop.
#[derive(Default)]
pub(crate) struct BatchConfig<'a> {
    pub(crate) attested: Option<&'a AttestedSet>,
    pub(crate) trace_all: bool,
    /// First input line (1-based) to answer; earlier lines are skipped.
    pub(crate) start_line: usize,
    /// Stop after the line that brings the rows written to at least this many, when input remains.
    pub(crate) max_rows: Option<usize>,
}

/// What a finished batch counted.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct BatchSummary {
    /// Rows written.
    pub(crate) rows: usize,
    /// Rows with status `error`.
    pub(crate) errors: usize,
    pub(crate) none: usize,
    pub(crate) ok: usize,
    /// Rows whose generation stopped on a cap or deadline.
    pub(crate) incomplete: usize,
    /// Surface entries across every row's seeds.
    pub(crate) surfaces: usize,
    /// Cause (or error) code to row count, for `none` and `error` rows.
    pub(crate) causes: std::collections::BTreeMap<String, usize>,
    pub(crate) first_none: Vec<Value>,
    /// Input lines answered (blank lines excluded).
    pub(crate) lines_answered: usize,
    /// The line to resume from when the batch stopped early.
    pub(crate) next_line: Option<usize>,
}

impl BatchSummary {
    fn count(&mut self, row: &Value) {
        self.rows += 1;
        let (status, code) = classify(row);
        match status {
            "error" => self.errors += 1,
            "none" => self.none += 1,
            _ => self.ok += 1,
        }
        if let Some(code) = code {
            *self.causes.entry(code).or_default() += 1;
        }
        if status == "none" && self.first_none.len() < SUMMARY_NONE_EXAMPLES {
            let id = row["id"]
                .as_str()
                .map(|s| s.chars().take(SUMMARY_ID_CHARS).collect::<String>());
            self.first_none.push(json!({
                "line": row["line"], "id": id, "target": row["target"],
                "comboKey": row["comboKey"], "cause": row["cause"],
            }));
        }
        if row["complete"] == json!(false) {
            self.incomplete += 1;
        }
        self.surfaces += row["seeds"].as_array().map_or(0, |seeds| {
            seeds
                .iter()
                .map(|s| s["surfaces"].as_array().map_or(0, Vec::len))
                .sum()
        });
    }

    /// The bounded stdout summary: one compact JSON line, well under 50 lines and 8 KB.
    pub(crate) fn render(&self, out: Option<&str>) -> Value {
        let mut value = json!({
            "type": "summary",
            "rows": self.rows,
            "status": {"ok": self.ok, "none": self.none, "error": self.errors},
            "causes": self.causes,
            "surfaces": self.surfaces,
            "incomplete": self.incomplete,
            "firstNone": self.first_none,
            "out": out,
            "complete": self.next_line.is_none(),
        });
        if let Some(next) = self.next_line {
            value["nextLine"] = json!(next);
            value["resume"] = json!(format!(
                "capped after {} rows: rerun with --start-line {next} (and --append to extend the same --out file)",
                self.rows
            ));
        }
        value
    }
}

/// A row's status and cause code: a `target` row states both; a `request` row is derived (an error row is `error`, a result with no surface is `none`).
fn classify(row: &Value) -> (&'static str, Option<String>) {
    let status = match row["status"].as_str() {
        Some("ok") => "ok",
        Some("none") => "none",
        Some(_) => "error",
        None if row["type"] == json!("error") => "error",
        None => {
            let any = row["seeds"].as_array().is_some_and(|seeds| {
                seeds
                    .iter()
                    .any(|s| s["surfaces"].as_array().is_some_and(|v| !v.is_empty()))
            });
            if any {
                "ok"
            } else {
                "none"
            }
        }
    };
    let code = match status {
        "ok" => None,
        "error" => row["code"].as_str().map(str::to_string),
        _ => Some(
            row["cause"]["code"]
                .as_str()
                .unwrap_or(if row["stopReason"].is_null() {
                    "generated_empty"
                } else {
                    "cap"
                })
                .to_string(),
        ),
    };
    (status, code)
}

enum Line {
    Text(Vec<u8>),
    TooLarge,
}

/// Reads one line without holding more than `MAX_REQUEST_BYTES` of it; `None` is end of input.
fn read_line(input: &mut impl BufRead) -> std::io::Result<Option<Line>> {
    let mut buf = Vec::new();
    if input
        .take(MAX_REQUEST_BYTES as u64 + 2)
        .read_until(b'\n', &mut buf)?
        == 0
    {
        return Ok(None);
    }
    let terminated = buf.last() == Some(&b'\n');
    if buf.len() - usize::from(terminated) <= MAX_REQUEST_BYTES {
        return Ok(Some(Line::Text(buf)));
    }
    let mut ended = terminated;
    while !ended {
        buf.clear();
        if input.take(1 << 16).read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        ended = buf.last() == Some(&b'\n');
    }
    Ok(Some(Line::TooLarge))
}

/// The response rows for one non-blank line: the serve `generate` answer, an error row, or, for a `target` row, one row per root MSA and combination.
fn answer(
    grammar: &Grammar,
    morpher: &Morpher<'_>,
    config: &BatchConfig<'_>,
    cell_index: &mut usize,
    line: Line,
) -> Vec<Value> {
    let bytes = match line {
        Line::Text(bytes) => bytes,
        Line::TooLarge => {
            return vec![error_response(
                &Value::Null,
                "request_too_large",
                &format!("request line exceeds {MAX_REQUEST_BYTES} bytes"),
            )]
        }
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return vec![error_response(
            &Value::Null,
            "malformed_json",
            "request line is not valid UTF-8",
        )];
    };
    let (id, object) = match request_envelope(text.trim()) {
        Ok(envelope) => envelope,
        Err(row) => return vec![row],
    };
    match object.get("op").and_then(Value::as_str) {
        Some("generate") if object.contains_key("target") || object.contains_key("cell") => {
            let index = *cell_index;
            if object.contains_key("cell") {
                *cell_index += 1;
            }
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                generate_target::answer(
                    grammar,
                    morpher,
                    &id,
                    &object,
                    config.attested,
                    config.trace_all,
                    index,
                )
            }));
            outcome.unwrap_or_else(|_| {
                vec![error_response(
                    &id,
                    "internal",
                    "generator panicked; see stderr",
                )]
            })
        }
        Some("generate") => vec![generate_response(
            grammar,
            morpher,
            id,
            &object,
            config.attested,
        )],
        Some(other) => vec![error_response(
            &id,
            "unknown_op",
            &format!("generate --requests accepts only op \"generate\", not \"{other}\""),
        )],
        None => vec![error_response(
            &id,
            "bad_request",
            "\"op\" must be a string",
        )],
    }
}

/// Answers every non-blank line of `input` from `config.start_line` in order, writing one JSON row per answered row, each with `line`, its 1-based input line number (batch only; `serve` rows have none). A bad line is an error row and the batch goes on; only an I/O failure stops it, or `config.max_rows` once reached with input left (`next_line` then says where to resume). `cell_index` of a `cell` row is its ordinal among the cell rows of the whole input, including skipped lines, so a resumed run numbers cells as the full run does.
pub(crate) fn run_requests(
    grammar: &Grammar,
    morpher: &Morpher<'_>,
    config: &BatchConfig<'_>,
    mut input: impl BufRead,
    mut out: impl Write,
) -> std::io::Result<BatchSummary> {
    let mut summary = BatchSummary::default();
    let mut line_number = 0usize;
    let mut cell_index = 0usize;
    while let Some(line) = read_line(&mut input)? {
        line_number += 1;
        if matches!(&line, Line::Text(bytes) if std::str::from_utf8(bytes).is_ok_and(|t| t.trim().is_empty()))
        {
            continue;
        }
        if line_number < config.start_line {
            // A skipped line still takes its cell ordinal.
            if let Line::Text(bytes) = &line {
                cell_index += usize::from(
                    std::str::from_utf8(bytes).is_ok_and(|t| t.contains("\"cell\""))
                        && std::str::from_utf8(bytes)
                            .ok()
                            .and_then(|t| serde_json::from_str::<Value>(t.trim()).ok())
                            .is_some_and(|v| {
                                v["op"] == json!("generate") && v.get("cell").is_some()
                            }),
                );
            }
            continue;
        }
        if config.max_rows.is_some_and(|max| summary.rows >= max) {
            summary.next_line = Some(line_number);
            break;
        }
        let rows = answer(grammar, morpher, config, &mut cell_index, line);
        summary.lines_answered += 1;
        for mut row in rows {
            row["line"] = json!(line_number);
            serde_json::to_writer(&mut out, &row)?;
            out.write_all(b"\n")?;
            summary.count(&row);
        }
        out.flush()?;
    }
    Ok(summary)
}
