//! `batch --retire`: an exact what-if-retire evaluation, one JSONL row per word, from a second parse of the grammar with entries or allomorphs removed in memory.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use pg_grammar::model::Grammar;
use pg_parse::morpher::Morpher;
use pg_parse::{project_parse_analysis_with_morphemes, ParseMorph};
use serde::Serialize;

/// The schema tag on every report row.
pub(crate) const RETIRE_SCHEMA: &str = "pangloss-retire/v1";

/// The parse settings both passes share.
pub(crate) struct Settings<'a> {
    pub words: &'a [String],
    pub threads: usize,
    pub work_cap: usize,
    pub capture_cap: Option<usize>,
    pub word_mem_cap: Option<usize>,
    pub word_timeout: Option<std::time::Duration>,
    pub always_enforce_final_templates: bool,
    pub guess: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct Reading {
    /// Entry guids among the reading's morphs, sorted.
    entries: Vec<String>,
    morphs: Vec<ParseMorph>,
}

/// One word's parse, reduced to what the comparison needs.
struct Parsed {
    readings: Vec<Reading>,
    incomplete: bool,
    unprojected: usize,
}

#[derive(Serialize)]
struct Cause {
    kind: &'static str,
    via: Vec<String>,
}

#[derive(Serialize)]
struct Row<'a> {
    schema: &'static str,
    index: usize,
    word: &'a str,
    status: &'static str,
    reparsed: bool,
    #[serde(rename = "baselineReadings")]
    baseline_readings: usize,
    #[serde(rename = "retiredReadings")]
    retired_readings: usize,
    lost: Vec<Reading>,
    gained: Vec<Reading>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cause: Option<Cause>,
}

fn guid_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// The snapshot with each entry, or allomorph, named by `ids` deleted; errors on an id that names neither.
pub(crate) fn retire_from_snapshot(
    snapshot: &mut pg_snapshot::Snapshot,
    ids: &[String],
) -> Result<bool, String> {
    let mut allomorph_retired = false;
    for id in ids {
        let entries = &mut snapshot.lexicon.entries;
        if let Some(at) = entries.iter().position(|e| guid_eq(&e.guid, id)) {
            entries.remove(at);
            continue;
        }
        let mut found = false;
        for entry in entries.iter_mut() {
            let before = entry.allomorphs.len();
            entry.allomorphs.retain(|a| !guid_eq(&a.guid, id));
            found |= entry.allomorphs.len() != before;
        }
        if !found {
            return Err(format!(
                "--retire: {id} is neither an entry nor an allomorph of the grammar"
            ));
        }
        allomorph_retired = true;
    }
    Ok(allomorph_retired)
}

/// The retired grammar, whether an allomorph was retired, and the owning entry of every allomorph and MSA guid of the original.
fn load_retired(
    path: &str,
    ids: &[String],
) -> Result<(Grammar, bool, BTreeMap<String, String>), String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let (mut snapshot, mut warnings) = if ext == "json" {
        let json = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
        let snapshot = pg_snapshot::Snapshot::from_json(&json)
            .map_err(|e| format!("parse snapshot {path}: {e}"))?;
        (snapshot, Vec::new())
    } else if ext.eq_ignore_ascii_case("fwdata") || ext.eq_ignore_ascii_case("fwbackup") {
        let (snapshot, report) = pg_fwdata::import_file_lean(std::path::Path::new(path))
            .map_err(|e| format!("import {path}: {e}"))?;
        let mut warnings = report.warnings;
        warnings.extend(snapshot.validate());
        (snapshot, warnings)
    } else {
        return Err(
            "--retire needs a grammar with entry guids: a .fwdata, .fwbackup or snapshot .json"
                .into(),
        );
    };
    let form_owner: BTreeMap<String, String> = snapshot
        .lexicon
        .entries
        .iter()
        .flat_map(|e| {
            let forms = e.allomorphs.iter().map(|a| a.guid.as_str());
            let msas = e.msas.iter().map(|m| m.guid());
            forms
                .chain(msas)
                .map(|g| (g.to_ascii_lowercase(), e.guid.clone()))
        })
        .collect();
    let allomorph = retire_from_snapshot(&mut snapshot, ids)?;
    if ext == "json" {
        warnings = Vec::new();
    }
    let output = pg_grammar::compile_project_grammar_only_with_import_warnings(
        &snapshot,
        Default::default(),
        warnings,
    )
    .map_err(|e| format!("compile {path} without the retired ids: {e}"))?;
    Ok((output.grammar, allomorph, form_owner))
}

fn parse_all(grammar: &Grammar, s: &Settings<'_>, words: &[&String]) -> Vec<Parsed> {
    let mut morpher = Morpher::new(grammar, s.work_cap)
        .with_capture_cap(s.capture_cap)
        .with_word_mem_cap(s.word_mem_cap)
        .with_word_timeout(s.word_timeout)
        .with_always_enforce_final_templates(s.always_enforce_final_templates)
        .with_guess_roots(s.guess);
    let _ = &mut morpher;
    let opts = morpher.project_parse_options();
    let owned: Vec<String> = words.iter().map(|w| (*w).clone()).collect();
    crate::parse_batch_in_order(&owned, s.threads, |_, word| {
        let outcome = morpher.parse_word_opts(word, &opts);
        let incomplete = outcome.timed_out
            || outcome.capped
            || outcome.capture_capped
            || outcome.word_mem_capped;
        let mut readings = Vec::new();
        let mut unprojected = 0;
        for analysis in &outcome.structured {
            match project_parse_analysis_with_morphemes(analysis, grammar) {
                Ok(projected) => {
                    let entries: BTreeSet<String> = projected
                        .morphemes
                        .iter()
                        .filter_map(|m| grammar.morphemes.get(*m as usize)?.owner.as_ref())
                        .map(|o| o.entry_guid.clone())
                        .collect();
                    readings.push(Reading {
                        entries: entries.into_iter().collect(),
                        morphs: projected.analysis.morphs,
                    });
                }
                Err(_) => unprojected += 1,
            }
        }
        Parsed {
            readings,
            incomplete,
            unprojected,
        }
    })
}

/// A variant's reading borrows its main entry's MSA and names the variant's own form; count each owning entry as a dependency.
fn add_form_owners(parsed: &mut Parsed, form_owner: &BTreeMap<String, String>) {
    for reading in &mut parsed.readings {
        let mut entries: BTreeSet<String> = reading.entries.iter().cloned().collect();
        for morph in &reading.morphs {
            for guid in [&morph.form, &morph.msa].into_iter().flatten() {
                entries.extend(form_owner.get(&guid.to_ascii_lowercase()).cloned());
            }
        }
        reading.entries = entries.into_iter().collect();
    }
}

fn uses(reading: &Reading, ids: &[String]) -> bool {
    ids.iter().any(|id| {
        reading.entries.iter().any(|e| guid_eq(e, id))
            || reading
                .morphs
                .iter()
                .any(|m| m.form.as_deref().is_some_and(|f| guid_eq(f, id)))
    })
}

fn key(reading: &Reading) -> String {
    serde_json::to_string(&reading.morphs).expect("morphs serialize")
}

/// Parses the baseline, parses the retired grammar over the words that can change, and writes the report; returns the status counts.
pub(crate) fn run(
    grammar_path: &str,
    baseline: &Grammar,
    ids: &[String],
    settings: &Settings<'_>,
    out_path: &str,
) -> Result<BTreeMap<&'static str, usize>, String> {
    let (retired, allomorph_retired, form_owner) = load_retired(grammar_path, ids)?;
    let all: Vec<&String> = settings.words.iter().collect();
    let mut base = parse_all(baseline, settings, &all);
    base.iter_mut()
        .for_each(|p| add_form_owners(p, &form_owner));
    // An allomorph's removal can unblock a word that has no reading to name it, so only entry retirement narrows the re-parse.
    let candidates: Vec<usize> = (0..base.len())
        .filter(|&i| {
            allomorph_retired
                || base[i].incomplete
                || base[i].unprojected > 0
                || base[i].readings.iter().any(|r| uses(r, ids))
        })
        .collect();
    let picked: Vec<&String> = candidates.iter().map(|&i| &settings.words[i]).collect();
    let mut after_parsed = parse_all(&retired, settings, &picked);
    after_parsed
        .iter_mut()
        .for_each(|p| add_form_owners(p, &form_owner));
    let mut after: BTreeMap<usize, Parsed> = candidates.into_iter().zip(after_parsed).collect();

    let file = std::fs::File::create(out_path).map_err(|e| format!("create {out_path}: {e}"))?;
    let mut w = std::io::BufWriter::new(file);
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (index, word) in settings.words.iter().enumerate() {
        let b = &base[index];
        let a = after.remove(&index);
        let reparsed = a.is_some();
        let (retired_readings, lost, gained, incomplete) = match &a {
            None => (b.readings.len(), Vec::new(), Vec::new(), b.incomplete),
            Some(a) => {
                let bk: BTreeSet<String> = b.readings.iter().map(key).collect();
                let ak: BTreeSet<String> = a.readings.iter().map(key).collect();
                let lost = b
                    .readings
                    .iter()
                    .filter(|r| !ak.contains(&key(r)))
                    .cloned()
                    .collect();
                let gained = a
                    .readings
                    .iter()
                    .filter(|r| !bk.contains(&key(r)))
                    .cloned()
                    .collect();
                (a.readings.len(), lost, gained, b.incomplete || a.incomplete)
            }
        };
        let lost: Vec<Reading> = dedup(lost);
        let gained: Vec<Reading> = dedup(gained);
        let status = if incomplete {
            "incomplete"
        } else if !lost.is_empty() && !gained.is_empty() {
            "changed"
        } else if !gained.is_empty() {
            "gained"
        } else if !lost.is_empty() && retired_readings == 0 {
            "lost"
        } else if !lost.is_empty() {
            "reduced"
        } else {
            "unchanged"
        };
        let cause = matches!(status, "lost" | "reduced" | "changed").then(|| Cause {
            kind: if status == "lost" {
                "no-alternative-chain"
            } else {
                "alternative-route"
            },
            via: lost
                .iter()
                .flat_map(|r| {
                    r.entries
                        .iter()
                        .filter(|e| ids.iter().any(|i| guid_eq(e, i)))
                })
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        });
        *counts.entry(status).or_default() += 1;
        let row = Row {
            schema: RETIRE_SCHEMA,
            index,
            word,
            status,
            reparsed,
            baseline_readings: b.readings.len(),
            retired_readings,
            lost,
            gained,
            cause,
        };
        serde_json::to_writer(&mut w, &row).map_err(|e| e.to_string())?;
        w.write_all(b"\n").map_err(|e| e.to_string())?;
    }
    w.flush().map_err(|e| e.to_string())?;
    Ok(counts)
}

fn dedup(readings: Vec<Reading>) -> Vec<Reading> {
    let mut seen = BTreeSet::new();
    readings
        .into_iter()
        .filter(|r| seen.insert(key(r)))
        .collect()
}

/// Splits a `--retire` value into guids, refusing an empty list.
pub(crate) fn parse_ids(value: &str) -> Result<Vec<String>, String> {
    let ids: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        return Err("--retire requires at least one guid".into());
    }
    Ok(ids)
}
