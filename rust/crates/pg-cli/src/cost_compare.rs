//! `pangloss cost-compare`: a better-or-worse verdict between two `batch --cost-column` runs.

use std::collections::BTreeMap;
use std::fs;

/// A change this large, in percent, reads as better or worse.
const CLEAR_PERCENT: f64 = 5.0;
/// A change this large, in percent, reads as probably better or worse.
const PROBABLE_PERCENT: f64 = 2.0;

/// The values of the status field; a tab inside a word shifts the fields and breaks this.
const STATUSES: [&str; 5] = ["ok", "CAP", "MEMCAP", "TIMEOUT", "SKIPPED"];

#[derive(Debug, PartialEq, Eq)]
struct Row {
    word: String,
    status: String,
    signature: String,
    cost: u64,
}

/// A cost row is the five result fields, an optional `guessed`, then `work_steps` and `cost`: `--cost-column` always writes `work_steps` too, so a lone `--work-columns` run cannot be mistaken for it. A `STARTED` sentinel is not a row.
fn parse_row(line: &str) -> Result<Option<(usize, Row)>, String> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() < 5 {
        return Ok(None);
    }
    if !STATUSES.contains(&fields[3]) {
        return Err("a row has an unexpected shape".into());
    }
    let guessed = usize::from(matches!(fields.get(5), Some(&"true") | Some(&"false")));
    if fields.len() - 5 - guessed != 2 {
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
    /// Words with a different signature, among those totalled.
    parse_changes: usize,
    /// Words left out of the totals because they were not `ok` on both sides.
    excluded_words: usize,
    /// Words whose status differs between the runs.
    status_changes: usize,
}

const INCONCLUSIVE: &str = "inconclusive";

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
    let pairs = || before.values().zip(after.values());
    let status_changes = pairs().filter(|(a, b)| a.status != b.status).count();
    let counted = || pairs().filter(|(a, b)| a.status == "ok" && b.status == "ok");
    let sum = |costs: Vec<u64>| {
        costs
            .into_iter()
            .try_fold(0u64, u64::checked_add)
            .ok_or_else(|| "a total cost is too large to compare".to_string())
    };
    let total_before = sum(counted().map(|(a, _)| a.cost).collect())?;
    let total_after = sum(counted().map(|(_, b)| b.cost).collect())?;
    if total_before == 0 && status_changes == 0 {
        return Err("no word with a measured cost finished ok on both runs".into());
    }
    let percent = if total_before == 0 {
        0.0
    } else {
        100.0 * (total_after as f64 / total_before as f64 - 1.0)
    };
    Ok(Comparison {
        words: before.len(),
        before: total_before,
        after: total_after,
        percent,
        verdict: if status_changes > 0 {
            INCONCLUSIVE
        } else {
            verdict(percent)
        },
        parse_changes: counted()
            .filter(|(a, b)| a.signature != b.signature)
            .count(),
        excluded_words: before.len() - counted().count(),
        status_changes,
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
    let percent = (result.before > 0).then_some(result.percent);
    if json {
        println!(
            "{}",
            serde_json::json!({
                "words": result.words,
                "before": result.before,
                "after": result.after,
                "percent": percent,
                "verdict": result.verdict,
                "parseChanges": result.parse_changes,
                "excludedWords": result.excluded_words,
                "statusChanges": result.status_changes,
            })
        );
    } else {
        let change = percent.map_or("no comparable".to_string(), |p| format!("{p:+.2}%"));
        println!(
            "{}: {change} cost over {} of {} words",
            result.verdict,
            result.words - result.excluded_words,
            result.words
        );
        println!("cost: {} -> {}", result.before, result.after);
        println!("parse changes: {}", result.parse_changes);
        println!("excluded words: {}", result.excluded_words);
        println!("status changes: {}", result.status_changes);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
