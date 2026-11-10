//! Measures how much parser work the rule-admission transducer saves on one grammar's words.
use foma::options::FomaOptions;
use pg_foma::admission::build_rule_admission;
use pg_foma::admission::RuleAdmission;
use pg_foma_runtime::confirm::{build_morpheme_owners, MorphemeOwner};
use pg_grammar::model::{Grammar, LexEntryId};
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use pg_rules::stratum::RuleRef;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

const DEFAULT_WORK_CAP: usize = 2_000_000;

fn load(path: &str) -> Result<Grammar, String> {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
    {
        "fwdata" | "fwbackup" => {
            let (snapshot, report) =
                pg_fwdata::import_file(Path::new(path)).map_err(|error| error.to_string())?;
            let mut warnings = report.warnings;
            warnings.extend(snapshot.validate());
            pg_grammar::compile_project_with_options_and_import_warnings(
                &snapshot,
                Default::default(),
                warnings,
            )
            .map(|out| out.grammar)
            .map_err(|error| format!("{error:?}"))
        }
        _ => pg_grammar::load(&fs::read_to_string(path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("{error:?}")),
    }
}

fn signature(outcome: &ParseOutcome) -> Vec<(Vec<u32>, i32)> {
    let mut keys: Vec<_> = outcome
        .structured
        .iter()
        .map(|analysis| (analysis.morpheme_ids.clone(), analysis.root_morpheme_index))
        .collect();
    keys.sort();
    keys
}

// The three stages are separate non-inlined functions so a callgrind run can attribute instructions to each.
#[inline(never)]
fn stage_plain(morpher: &Morpher, word: &str) -> ParseOutcome {
    morpher.parse_word_selected(word, &ParseOptions::default(), None, None)
}

#[inline(never)]
fn stage_filter(admission: &RuleAdmission, word: &str) -> Vec<bool> {
    let admitted = admission.admit(word);
    std::hint::black_box(&admitted);
    admitted
}

#[inline(never)]
fn stage_confirm(morpher: &Morpher, word: &str, set: &[bool]) -> ParseOutcome {
    let filter = |rule: RuleRef| match rule {
        RuleRef::Stratum(_) | RuleRef::Template(_) => true,
        RuleRef::MRule(id) => set[id.0 as usize],
    };
    morpher.parse_word_selected(word, &ParseOptions::default(), None, Some(&filter))
}

// A callgrind run dumps its counts on entering this marker, once after each stage, while collection is off.
#[inline(never)]
fn stage_mark() {
    std::hint::black_box(0u8);
}

// One more restricted parse: admitted rules plus exactly the lexical entries of the plain analyses.
fn oracle_roots(
    morpher: &Morpher,
    owners: &[Option<MorphemeOwner>],
    word: &str,
    set: &[bool],
    plain: &ParseOutcome,
) -> ParseOutcome {
    let entries: BTreeSet<u32> = plain
        .structured
        .iter()
        .flat_map(|analysis| analysis.morpheme_ids.iter())
        .filter_map(
            |&morpheme| match owners.get(morpheme as usize).copied().flatten() {
                Some(MorphemeOwner::LexEntry(LexEntryId(id))) => Some(id),
                _ => None,
            },
        )
        .collect();
    let lex = |entry: LexEntryId| entries.contains(&entry.0);
    let rule = |rule: RuleRef| match rule {
        RuleRef::Stratum(_) | RuleRef::Template(_) => true,
        RuleRef::MRule(id) => set[id.0 as usize],
    };
    morpher.parse_word_selected(word, &ParseOptions::default(), Some(&lex), Some(&rule))
}

fn main() -> Result<(), String> {
    let mut args: Vec<String> = Vec::new();
    let mut work_cap = DEFAULT_WORK_CAP;
    let mut with_oracle = false;
    let mut raw = std::env::args().skip(1);
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--work-cap" => {
                work_cap = raw
                    .next()
                    .and_then(|text| text.parse().ok())
                    .ok_or("--work-cap needs a number")?
            }
            "--oracle-roots" => with_oracle = true,
            _ => args.push(arg),
        }
    }
    let (source, words_path, output, limit) = match args.as_slice() {
        [a, b, c, d] => (a, b, c, d),
        _ => return Err("usage: admission_measure <grammar> <words.txt> <out.jsonl> <max-words> [--work-cap N] [--oracle-roots]".into()),
    };
    let limit: usize = limit.parse().map_err(|_| "max-words must be a number")?;
    let g = load(source)?;
    let started = Instant::now();
    let built = build_rule_admission(&FomaOptions::default(), &g);
    let build_ms = started.elapsed().as_millis() as u64;
    let header = match &built {
        Ok(admission) => serde_json::json!({
            "evidence": admission.evidence(), "warnings": admission.warnings(),
            "refusal": null, "build_ms": build_ms, "work_cap": work_cap,
        }),
        Err(refusal) => serde_json::json!({
            "evidence": null, "warnings": [], "refusal": refusal.reason(), "build_ms": build_ms, "work_cap": work_cap,
        }),
    };
    println!("{header}");
    let Ok(admission) = built else {
        return Ok(());
    };
    let free_count = admission.free().iter().filter(|f| **f).count();
    let morpher = Morpher::new(&g, work_cap);
    let owners = build_morpheme_owners(&g);
    let words = fs::read_to_string(words_path).map_err(|error| error.to_string())?;
    let mut out = fs::File::create(output).map_err(|error| error.to_string())?;
    for word in words
        .lines()
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .take(limit)
    {
        let plain = stage_plain(&morpher, word);
        stage_mark();
        let admitted = stage_filter(&admission, word);
        stage_mark();
        let confirm = stage_confirm(&morpher, word, &admitted);
        stage_mark();
        let count = admitted.iter().filter(|a| **a).count();
        let oracle = with_oracle.then(|| oracle_roots(&morpher, &owners, word, &admitted, &plain));
        let same = signature(&confirm) == signature(&plain);
        let kept = signature(&confirm);
        let superset = signature(&plain).iter().all(|key| kept.contains(key));
        let row = serde_json::json!({
            "word": word, "plain_steps": plain.work_steps, "analyses": plain.structured.len(),
            "capped": plain.capped, "admitted": count,
            "spotted": count.saturating_sub(free_count),
            "fallback": false, "confirm_steps": confirm.work_steps, "fallback_steps": 0,
            "same": same, "superset": superset, "confirm_capped": confirm.capped,
            "oracle_root_steps": oracle.as_ref().map(|o| o.work_steps),
            "oracle_root_same": oracle.as_ref().map(|o| signature(o) == signature(&plain)),
        });
        writeln!(out, "{row}").map_err(|error| error.to_string())?;
    }
    Ok(())
}
