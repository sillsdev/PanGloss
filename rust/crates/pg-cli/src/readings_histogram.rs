//! `pangloss readings-histogram`: how many words have 0, 1, 2, ... Readings in a `batch` TSV (#396).

use std::collections::BTreeMap;
use std::fs;

/// The values of the status field; a tab inside a word shifts the fields and breaks this.
const STATUSES: [&str; 5] = ["ok", "CAP", "MEMCAP", "TIMEOUT", "SKIPPED"];

/// Words per Reading count among `ok` words, and the words left out because they did not finish `ok`.
#[derive(Debug, Default, PartialEq, Eq)]
struct Histogram {
    /// Reading count to number of `ok` words with that many Readings; 0 is a word with no analysis.
    by_readings: BTreeMap<usize, usize>,
    /// Words whose status is not `ok`, by status: their signature is partial or absent, so they are not binned.
    not_ok: BTreeMap<String, usize>,
}

/// The Readings in a signature: `-` is none, otherwise one per `;`-separated entry, repeats included (`result_signature` never dedupes).
fn reading_count(signature: &str) -> usize {
    if signature == "-" {
        0
    } else {
        signature.split(';').count()
    }
}

fn tally(text: &str, path: &str) -> Result<(Histogram, usize), String> {
    let mut histogram = Histogram::default();
    let mut words = 0;
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        // A `STARTED` sentinel or any shorter line is not a result row.
        if fields.len() < 5 {
            continue;
        }
        if !STATUSES.contains(&fields[3]) {
            return Err(format!("{path}: a row has an unexpected shape"));
        }
        words += 1;
        if fields[3] == "ok" {
            *histogram
                .by_readings
                .entry(reading_count(fields[4]))
                .or_default() += 1;
        } else {
            *histogram.not_ok.entry(fields[3].to_string()).or_default() += 1;
        }
    }
    if words == 0 {
        return Err(format!("{path}: no result rows"));
    }
    Ok((histogram, words))
}

pub(crate) fn run_readings_histogram(args: &[String]) -> Result<(), String> {
    let mut json = false;
    let mut paths = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}")),
            path => paths.push(path),
        }
    }
    let [path] = paths.as_slice() else {
        return Err("usage: readings-histogram <batch.tsv> [--json]".into());
    };
    let text = fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    let (histogram, words) = tally(&text, path)?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "words": words,
                "readings": histogram.by_readings.iter().map(|(n, c)| serde_json::json!({"readings": n, "words": c})).collect::<Vec<_>>(),
                "notOk": histogram.not_ok,
            })
        );
    } else {
        println!("readings\twords");
        for (readings, count) in &histogram.by_readings {
            println!("{readings}\t{count}");
        }
        for (status, count) in &histogram.not_ok {
            println!("{status}\t{count}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
