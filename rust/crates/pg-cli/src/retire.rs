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
pub(crate) struct Parsed {
    pub(crate) readings: Vec<Reading>,
    pub(crate) incomplete: bool,
    pub(crate) unprojected: usize,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
}

const CAPPED: &str = "a parse hit a work, memory, capture or time cap";
const UNPROJECTABLE: &str = "a reading cannot be projected, so it cannot be compared";

fn guid_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// The snapshot with each entry, or allomorph, named by `ids` deleted, and every ad hoc prohibition reference to it dropped as FieldWorks does; errors on an id that names neither, and the ad hoc prohibition references to it dropped; true when an allomorph was retired.
pub(crate) fn retire_from_snapshot(
    snapshot: &mut pg_snapshot::Snapshot,
    ids: &[String],
    flag: &str,
) -> Result<bool, String> {
    // Classify every id against the untouched snapshot, so an MSA of an entry that is also named cannot slip through as already gone.
    for id in ids {
        let known = snapshot
            .lexicon
            .entries
            .iter()
            .any(|e| guid_eq(&e.guid, id) || e.allomorphs.iter().any(|a| guid_eq(&a.guid, id)));
        if !known {
            return Err(if names_other_object(snapshot, id) {
                format!(
                    "{flag}: {id} names an object that is neither an entry nor an allomorph; only lexical entries and allomorphs can be removed"
                )
            } else {
                format!("{flag}: {id} is neither an entry nor an allomorph of the grammar")
            });
        }
    }
    let mut allomorph_retired = false;
    let mut gone: BTreeSet<String> = BTreeSet::new();
    for id in ids {
        if gone.contains(&id.to_ascii_lowercase()) {
            continue;
        }
        let entries = &mut snapshot.lexicon.entries;
        if let Some(at) = entries.iter().position(|e| guid_eq(&e.guid, id)) {
            let entry = entries.remove(at);
            gone.insert(entry.guid.to_ascii_lowercase());
            gone.extend(entry.allomorphs.iter().map(|a| a.guid.to_ascii_lowercase()));
            gone.extend(entry.msas.iter().map(|m| m.guid().to_ascii_lowercase()));
            continue;
        }
        for entry in entries.iter_mut() {
            gone.extend(
                entry
                    .allomorphs
                    .iter()
                    .filter(|a| guid_eq(&a.guid, id))
                    .map(|a| a.guid.to_ascii_lowercase()),
            );
            entry.allomorphs.retain(|a| !guid_eq(&a.guid, id));
        }
        allomorph_retired = true;
    }
    drop_adhoc_references(snapshot, &gone);
    Ok(allomorph_retired)
}

/// True when `id` appears as a string anywhere in the snapshot, so it names a real object of a kind this reduction does not remove (an MSA, a rule, a feature, a template and so on).
fn names_other_object(snapshot: &pg_snapshot::Snapshot, id: &str) -> bool {
    fn walk(value: &serde_json::Value, id: &str) -> bool {
        match value {
            serde_json::Value::String(s) => guid_eq(s, id),
            serde_json::Value::Array(items) => items.iter().any(|v| walk(v, id)),
            serde_json::Value::Object(map) => map.values().any(|v| walk(v, id)),
            _ => false,
        }
    }
    serde_json::to_value(snapshot).is_ok_and(|v| walk(&v, id))
}

/// Removes retired guids from ad hoc prohibitions, dropping those that lose their primary or every other item, and their group memberships.
fn drop_adhoc_references(snapshot: &mut pg_snapshot::Snapshot, gone: &BTreeSet<String>) {
    use pg_snapshot::AdhocProhibition;
    let hit = |g: &String| gone.contains(&g.to_ascii_lowercase());
    let mut dropped: BTreeSet<String> = BTreeSet::new();
    snapshot.morphology.adhoc_prohibitions.retain_mut(|rule| {
        let (guid, primary, others) = match rule {
            AdhocProhibition::Allomorph {
                guid,
                primary,
                others,
                ..
            }
            | AdhocProhibition::Morpheme {
                guid,
                primary,
                others,
                ..
            } => (guid, primary, others),
        };
        others.retain(|o| !hit(o));
        let keep = !hit(primary) && !others.is_empty();
        if !keep {
            dropped.insert(guid.to_ascii_lowercase());
        }
        keep
    });
    if let Some(groups) = &mut snapshot.morphology.adhoc_prohibition_groups {
        for group in groups {
            group
                .members
                .retain(|m| !dropped.contains(&m.to_ascii_lowercase()));
        }
    }
}

/// The snapshot as JSON without the lexical entries, which is what a retire must leave untouched.
fn without_lexicon(snapshot: &pg_snapshot::Snapshot) -> serde_json::Value {
    let mut value = serde_json::to_value(snapshot).expect("snapshot serializes");
    if let Some(root) = value.as_object_mut() {
        root.remove("conversionProvenance");
        if let Some(lexicon) = root.get_mut("lexicon").and_then(|l| l.as_object_mut()) {
            lexicon.remove("entries");
        }
    }
    value
}

/// Every affix-like rule keyed by its owning entry, MSA and form guids, and the kept rule keys of each stratum and template slot, with the rules in `only` (when given) the only ones kept.
fn structure(grammar: &Grammar, only: Option<&BTreeSet<String>>) -> (BTreeSet<String>, String) {
    let keys: Vec<Option<String>> = grammar
        .mrules
        .iter()
        .enumerate()
        .map(|(i, rule)| {
            let (morpheme, allomorphs) = match rule {
                pg_grammar::model::MorphRuleDef::AffixProcess(r) => (r.morpheme, &r.allomorphs),
                pg_grammar::model::MorphRuleDef::Realizational(r) => (r.morpheme, &r.allomorphs),
                pg_grammar::model::MorphRuleDef::Compounding(_) => {
                    return Some(format!("rule#{i}"))
                }
            };
            let info = grammar.morphemes.get(morpheme.0 as usize)?;
            let forms: Vec<_> = allomorphs
                .iter()
                .map(|a| {
                    grammar
                        .allomorph_sources
                        .get(a.id.0 as usize)
                        .map(|s| &s.form_guids)
                })
                .collect();
            Some(format!(
                "{:?}|{:?}|{:?}|{forms:?}",
                info.owner.as_ref().map(|o| &o.entry_guid),
                info.source_msa_guid,
                info.source_infl_type_guid
            ))
        })
        .collect();
    let all: BTreeSet<String> = keys.iter().flatten().cloned().collect();
    let kept = |id: &pg_grammar::model::MRuleId| {
        keys.get(id.0 as usize)
            .and_then(|k| k.as_ref())
            .filter(|k| only.is_none_or(|o| o.contains(*k)))
            .cloned()
    };
    let mut text = String::new();
    for stratum in &grammar.strata {
        let rules: Vec<_> = stratum.mrules.iter().filter_map(kept).collect();
        text += &format!(
            "stratum {:?} {:?} {:?} {:?} {:?} {rules:?}\n",
            stratum.name, stratum.table, stratum.mrule_order, stratum.prules, stratum.templates
        );
    }
    for template in &grammar.templates {
        text += &format!(
            "template {:?} {:?} {} {:?}\n",
            template.source_guid, template.name, template.is_final, template.required_syn_fs
        );
        for slot in &template.slots {
            let rules: Vec<_> = slot.rules.iter().filter_map(kept).collect();
            text += &format!(
                " slot {:?} {} {:?} {rules:?}\n",
                slot.name, slot.optional, slot.zone
            );
        }
    }
    text += &format!(
        "{} {} {} {} {} {} {} {} {}",
        grammar.prules.len(),
        grammar.natural_classes.len(),
        grammar.stem_names.len(),
        grammar.families.len(),
        grammar.max_stem_count,
        grammar.deletion_reapplications,
        grammar.always_enforce_final_templates,
        grammar.merge_equivalent_analyses,
        grammar.guess_roots
    );
    (all, text)
}

/// True when the retired grammar differs from the baseline in anything but the removed lexicon: other snapshot parts, or how the compiler lays out templates and strata around the surviving rules.
fn rest_of_grammar_changed(
    before: &serde_json::Value,
    after: &serde_json::Value,
    baseline: &Grammar,
    retired: &Grammar,
) -> bool {
    if before != after {
        return true;
    }
    let (retired_keys, retired_text) = structure(retired, None);
    let (_, baseline_text) = structure(baseline, Some(&retired_keys));
    baseline_text != retired_text
}

/// The grammar's snapshot and import warnings, read from a `.fwdata`, `.fwbackup` or snapshot `.json`; `flag` names the option in the refusal for any other grammar.
fn load_snapshot(
    path: &str,
    flag: &str,
) -> Result<(pg_snapshot::Snapshot, Vec<pg_snapshot::Warning>), String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    if ext == "json" {
        let json = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
        let snapshot = pg_snapshot::Snapshot::from_json(&json)
            .map_err(|e| format!("parse snapshot {path}: {e}"))?;
        Ok((snapshot, Vec::new()))
    } else if ext.eq_ignore_ascii_case("fwdata") || ext.eq_ignore_ascii_case("fwbackup") {
        let (snapshot, report) = pg_fwdata::import_file_lean(std::path::Path::new(path))
            .map_err(|e| format!("import {path}: {e}"))?;
        let mut warnings = report.warnings;
        warnings.extend(snapshot.validate());
        Ok((snapshot, warnings))
    } else {
        Err(format!(
            "{flag} needs a grammar with entry guids: a .fwdata, .fwbackup or snapshot .json"
        ))
    }
}

/// The grammar of `path` with the entries and allomorphs named by `ids` deleted in memory, as `--retire` builds its second grammar; `flag` names the option in errors.
pub(crate) fn load_without(
    path: &str,
    ids: &[String],
    flag: &str,
) -> Result<(Grammar, Vec<pg_snapshot::Warning>), String> {
    let ext_json = std::path::Path::new(path)
        .extension()
        .is_some_and(|e| e == "json");
    let (mut snapshot, mut warnings) = load_snapshot(path, flag)?;
    retire_from_snapshot(&mut snapshot, ids, flag)?;
    if ext_json {
        warnings = Vec::new();
    }
    let output = pg_grammar::compile_project_grammar_only_with_import_warnings(
        &snapshot,
        Default::default(),
        warnings.clone(),
    )
    .map_err(|e| format!("compile {path} without the {flag} ids: {e}"))?;
    Ok((output.grammar, warnings))
}

/// The retired grammar, whether every word must be re-parsed, and the owning entry of every allomorph and MSA guid of the original.
fn load_retired(
    path: &str,
    ids: &[String],
) -> Result<(Grammar, bool, BTreeMap<String, String>), String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let (mut snapshot, mut warnings) = load_snapshot(path, "--retire")?;
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
    let before = without_lexicon(&snapshot);
    let unmodified = snapshot.clone();
    let allomorph = retire_from_snapshot(&mut snapshot, ids, "--retire")?;
    if ext == "json" {
        warnings = Vec::new();
    }
    let after = without_lexicon(&snapshot);
    let baseline = pg_grammar::compile_project_grammar_only_with_import_warnings(
        &unmodified,
        Default::default(),
        warnings.clone(),
    )
    .map_err(|e| format!("compile {path}: {e}"))?
    .grammar;
    let output = pg_grammar::compile_project_grammar_only_with_import_warnings(
        &snapshot,
        Default::default(),
        warnings,
    )
    .map_err(|e| format!("compile {path} without the retired ids: {e}"))?;
    let reparse_all =
        allomorph || rest_of_grammar_changed(&before, &after, &baseline, &output.grammar);
    Ok((output.grammar, reparse_all, form_owner))
}

fn parse_all(grammar: &Grammar, s: &Settings<'_>, words: &[&String]) -> Vec<Parsed> {
    let morpher = Morpher::new(grammar, s.work_cap)
        .with_capture_cap(s.capture_cap)
        .with_word_mem_cap(s.word_mem_cap)
        .with_word_timeout(s.word_timeout)
        .with_always_enforce_final_templates(s.always_enforce_final_templates)
        .with_guess_roots(s.guess);
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
    let (retired, reparse_all, form_owner) = load_retired(grammar_path, ids)?;
    let all: Vec<&String> = settings.words.iter().collect();
    let mut base = parse_all(baseline, settings, &all);
    base.iter_mut()
        .for_each(|p| add_form_owners(p, &form_owner));
    // Narrowing holds only while the rest of the grammar is unchanged; an allomorph retire or any other change can alter a word no reading names.
    let candidates: Vec<usize> = (0..base.len())
        .filter(|&i| {
            reparse_all
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
        let (retired_readings, lost, gained, reason) = compare(b, a.as_ref());
        let lost: Vec<Reading> = dedup(lost);
        let gained: Vec<Reading> = dedup(gained);
        let status = if reason.is_some() {
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
            kind: match status {
                "lost" => "no-alternative-reading",
                "changed" if retired_readings == gained.len() => "replaced",
                _ => "alternative-route",
            },
            via: lost
                .iter()
                .flat_map(|r| {
                    let named = r.entries.iter().cloned();
                    let forms = r.morphs.iter().filter_map(|m| m.form.clone());
                    named
                        .chain(forms)
                        .filter(|g| ids.iter().any(|i| guid_eq(g, i)))
                })
                .map(|g| g.to_ascii_lowercase())
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
            reason,
        };
        serde_json::to_writer(&mut w, &row).map_err(|e| e.to_string())?;
        w.write_all(b"\n").map_err(|e| e.to_string())?;
    }
    w.flush().map_err(|e| e.to_string())?;
    Ok(counts)
}

/// The retired reading count, the readings lost and gained, and why no verdict is possible; `after` is `None` for a word that was not re-parsed.
pub(crate) fn compare(
    b: &Parsed,
    after: Option<&Parsed>,
) -> (usize, Vec<Reading>, Vec<Reading>, Option<&'static str>) {
    let Some(a) = after else {
        return (
            b.readings.len(),
            Vec::new(),
            Vec::new(),
            b.incomplete.then_some(CAPPED),
        );
    };
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
    let reason = if b.incomplete || a.incomplete {
        Some(CAPPED)
    } else if b.unprojected > 0 || a.unprojected > 0 {
        Some(UNPROJECTABLE)
    } else {
        None
    };
    (a.readings.len(), lost, gained, reason)
}

fn dedup(readings: Vec<Reading>) -> Vec<Reading> {
    let mut seen = BTreeSet::new();
    readings
        .into_iter()
        .filter(|r| seen.insert(key(r)))
        .collect()
}

/// Splits a `--retire` or `--without` value into guids, refusing an empty list.
pub(crate) fn parse_ids(value: &str, flag: &str) -> Result<Vec<String>, String> {
    let ids: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .fold(Vec::new(), |mut ids: Vec<String>, id| {
            if !ids.iter().any(|seen| guid_eq(seen, &id)) {
                ids.push(id);
            }
            ids
        });
    if ids.is_empty() {
        return Err(format!("{flag} requires at least one guid"));
    }
    Ok(ids)
}

/// Adds `more` to the ids of a repeated flag, keeping the first spelling of each guid.
pub(crate) fn merge_ids(into: &mut Option<Vec<String>>, more: Vec<String>) {
    let ids = into.get_or_insert_with(Vec::new);
    for id in more {
        if !ids.iter().any(|seen| guid_eq(seen, &id)) {
            ids.push(id);
        }
    }
}
