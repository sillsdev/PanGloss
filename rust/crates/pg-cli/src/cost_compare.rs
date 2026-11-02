//! `pangloss cost-compare`: a better-or-worse verdict between two `batch --cost-column` runs.

use std::collections::BTreeMap;
use std::fs;

/// A change this large, in percent, reads as better or worse.
const CLEAR_PERCENT: f64 = 5.0;
/// A change this large, in percent, reads as probably better or worse.
const PROBABLE_PERCENT: f64 = 2.0;

#[derive(Debug, PartialEq, Eq)]
struct Row {
    word: String,
    status: String,
    signature: String,
    cost: u64,
}

/// Whether `fields` is a complete result row with a cost; a `STARTED` sentinel is neither.
fn parse_row(line: &str) -> Result<Option<(usize, Row)>, String> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() < 5 {
        return Ok(None);
    }
    let guessed = usize::from(matches!(fields.get(5), Some(&"true") | Some(&"false")));
    if (fields.len() - 5 - guessed).is_multiple_of(2) {
        return Err("a row has no cost column; rerun `batch` with --cost-column".into());
    }
    let idx = fields[0]
        .parse()
        .map_err(|_| format!("invalid word index {:?}", fields[0]))?;
    let cost = fields[fields.len() - 1]
        .parse()
        .map_err(|_| format!("invalid cost {:?}", fields[fields.len() - 1]))?;
    Ok(Some((
        idx,
        Row {
            word: fields[1].to_string(),
            status: fields[3].to_string(),
            signature: fields[4].to_string(),
            cost,
        },
    )))
}

fn parse_rows(text: &str, path: &str) -> Result<BTreeMap<usize, Row>, String> {
    let mut rows = BTreeMap::new();
    for line in text.lines() {
        if let Some((idx, row)) = parse_row(line).map_err(|e| format!("{path}: {e}"))? {
            if row.status == "TIMEOUT" {
                return Err(format!(
                    "{path}: word {idx} timed out, so its cost depends on this machine; rerun without --word-timeout-ms"
                ));
            }
            rows.insert(idx, row);
        }
    }
    if rows.is_empty() {
        return Err(format!("{path}: no result rows"));
    }
    Ok(rows)
}

#[derive(Debug, PartialEq)]
struct Comparison {
    words: usize,
    before: u64,
    after: u64,
    percent: f64,
    verdict: &'static str,
    parse_changes: usize,
    capped_words: usize,
}

fn verdict(percent: f64) -> &'static str {
    if percent <= -CLEAR_PERCENT {
        "better"
    } else if percent >= CLEAR_PERCENT {
        "worse"
    } else if percent <= -PROBABLE_PERCENT {
        "probably better"
    } else if percent >= PROBABLE_PERCENT {
        "probably worse"
    } else {
        "no clear change"
    }
}

fn compare(
    before: &BTreeMap<usize, Row>,
    after: &BTreeMap<usize, Row>,
) -> Result<Comparison, String> {
    if before.len() != after.len()
        || before
            .iter()
            .zip(after)
            .any(|((i, a), (j, b))| i != j || a.word != b.word)
    {
        return Err("the two runs must cover the same word list".into());
    }
    let total = |rows: &BTreeMap<usize, Row>| rows.values().map(|row| row.cost).sum::<u64>();
    let (total_before, total_after) = (total(before), total(after));
    if total_before == 0 {
        return Err("the first run has no measured cost".into());
    }
    let percent = 100.0 * (total_after as f64 / total_before as f64 - 1.0);
    let pairs = || before.values().zip(after.values());
    Ok(Comparison {
        words: before.len(),
        before: total_before,
        after: total_after,
        percent,
        verdict: verdict(percent),
        parse_changes: pairs()
            .filter(|(a, b)| a.signature != b.signature || a.status != b.status)
            .count(),
        capped_words: pairs()
            .filter(|(a, b)| a.status == "CAP" || b.status == "CAP")
            .count(),
    })
}

pub(crate) fn run_cost_compare(args: &[String]) -> Result<(), String> {
    let mut json = false;
    let mut paths = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}")),
            path => paths.push(path),
        }
    }
    let [before_path, after_path] = paths.as_slice() else {
        return Err("usage: cost-compare <before.tsv> <after.tsv> [--json]".into());
    };
    let read = |path: &str| -> Result<BTreeMap<usize, Row>, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
        parse_rows(&text, path)
    };
    let result = compare(&read(before_path)?, &read(after_path)?)?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "words": result.words,
                "before": result.before,
                "after": result.after,
                "percent": result.percent,
                "verdict": result.verdict,
                "parseChanges": result.parse_changes,
                "cappedWords": result.capped_words,
                "atLeast": result.capped_words > 0,
            })
        );
    } else {
        let at_least = if result.capped_words > 0 {
            "at least "
        } else {
            ""
        };
        println!(
            "{}: {at_least}{:+.2}% cost over {} words",
            result.verdict, result.percent, result.words
        );
        println!("cost: {} -> {}", result.before, result.after);
        println!("parse changes: {}", result.parse_changes);
        println!("capped words: {}", result.capped_words);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
