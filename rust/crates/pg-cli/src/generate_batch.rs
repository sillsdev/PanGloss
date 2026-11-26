//! `generate --requests <file.jsonl|->`: a JSONL batch of `serve` `generate` requests answered one row per line.
//! Format: `docs/generation.md`, "Batch requests".

use std::io::{BufRead, Read, Write};

use pg_grammar::model::Grammar;
use pg_parse::Morpher;
use serde_json::{json, Value};

use crate::serve::{error_response, generate_response, request_envelope, MAX_REQUEST_BYTES};

/// What a finished batch counted: the rows written and how many of them were error rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BatchSummary {
    pub(crate) rows: usize,
    pub(crate) errors: usize,
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

/// The response row for one non-blank line: the serve `generate` answer, or an error row.
fn answer(grammar: &Grammar, morpher: &Morpher<'_>, line: Line) -> Value {
    let bytes = match line {
        Line::Text(bytes) => bytes,
        Line::TooLarge => {
            return error_response(
                &Value::Null,
                "request_too_large",
                &format!("request line exceeds {MAX_REQUEST_BYTES} bytes"),
            )
        }
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return error_response(
            &Value::Null,
            "malformed_json",
            "request line is not valid UTF-8",
        );
    };
    let (id, object) = match request_envelope(text.trim()) {
        Ok(envelope) => envelope,
        Err(row) => return row,
    };
    match object.get("op").and_then(Value::as_str) {
        Some("generate") => generate_response(grammar, morpher, id, &object),
        Some(other) => error_response(
            &id,
            "unknown_op",
            &format!("generate --requests accepts only op \"generate\", not \"{other}\""),
        ),
        None => error_response(&id, "bad_request", "\"op\" must be a string"),
    }
}

/// Answers every non-blank line of `input` in order, writing one JSON row per answered line, each with `line`, its 1-based input line number (batch only; `serve` rows have none). A bad line is an error row and the batch goes on; only an I/O failure stops it.
pub(crate) fn run_requests(
    grammar: &Grammar,
    morpher: &Morpher<'_>,
    mut input: impl BufRead,
    mut out: impl Write,
) -> std::io::Result<BatchSummary> {
    let mut summary = BatchSummary { rows: 0, errors: 0 };
    let mut line_number = 0usize;
    while let Some(line) = read_line(&mut input)? {
        line_number += 1;
        if matches!(&line, Line::Text(bytes) if std::str::from_utf8(bytes).is_ok_and(|t| t.trim().is_empty()))
        {
            continue;
        }
        let mut row = answer(grammar, morpher, line);
        row["line"] = json!(line_number);
        serde_json::to_writer(&mut out, &row)?;
        out.write_all(b"\n")?;
        out.flush()?;
        summary.rows += 1;
        if row["type"] == json!("error") {
            summary.errors += 1;
        }
    }
    Ok(summary)
}
