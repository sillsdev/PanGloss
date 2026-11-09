//! Measures how much parser work the rule-admission transducer saves on one grammar's words.
use foma::options::FomaOptions;
use pg_foma::admission::build_rule_admission;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use pg_rules::stratum::RuleRef;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

const WORK_CAP: usize = 2_000_000;

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

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (source, words_path, output, limit) = match args.as_slice() {
        [a, b, c, d] | [a, b, c, d, _] => (a, b, c, d),
        _ => return Err("usage: admission_measure <grammar> <words.txt> <out.jsonl> <max-words> [path-allowance=4096]".into()),
    };
    let limit: usize = limit.parse().map_err(|_| "max-words must be a number")?;
    let allowance: usize = match args.get(4) {
        Some(text) => text
            .parse()
            .map_err(|_| "path-allowance must be a number")?,
        None => 4096,
    };
    let g = load(source)?;
    let started = Instant::now();
    let built = build_rule_admission(&FomaOptions::default(), &g);
    let build_ms = started.elapsed().as_millis() as u64;
    let header = match &built {
        Ok(admission) => serde_json::json!({
            "evidence": admission.evidence(), "warnings": admission.warnings(),
            "refusal": null, "build_ms": build_ms,
        }),
        Err(refusal) => serde_json::json!({
            "evidence": null, "warnings": [], "refusal": refusal.reason(), "build_ms": build_ms,
        }),
    };
    println!("{header}");
    let Ok(admission) = built else {
        return Ok(());
    };
    let free: Vec<bool> = admission.plan().free.clone();
    let free_count = free.iter().filter(|f| **f).count();
    let morpher = Morpher::new(&g, WORK_CAP);
    let words = fs::read_to_string(words_path).map_err(|error| error.to_string())?;
    let mut out = fs::File::create(output).map_err(|error| error.to_string())?;
    for word in words
        .lines()
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .take(limit)
    {
        let plain = morpher.parse_word_selected(word, &ParseOptions::default(), None, None);
        let (admitted, raw_paths) = admission.admit_counted(word, allowance);
        let (confirm, fallback, count) = match &admitted {
            Some(set) => {
                let filter = |rule: RuleRef| match rule {
                    RuleRef::Stratum(_) | RuleRef::Template(_) => true,
                    RuleRef::MRule(id) => set[id.0 as usize],
                };
                let narrowed = morpher.parse_word_selected(
                    word,
                    &ParseOptions::default(),
                    None,
                    Some(&filter),
                );
                (Some(narrowed), false, set.iter().filter(|a| **a).count())
            }
            None => (None, true, g.mrules.len()),
        };
        let same = confirm
            .as_ref()
            .is_none_or(|narrowed| signature(narrowed) == signature(&plain));
        let confirm_steps = confirm.as_ref().map_or(0, |o| o.work_steps);
        let row = serde_json::json!({
            "word": word, "plain_steps": plain.work_steps, "analyses": plain.structured.len(),
            "capped": plain.capped, "admitted": count,
            "spotted": if fallback { 0 } else { count.saturating_sub(free_count) },
            "raw_paths": raw_paths, "fallback": fallback, "confirm_steps": confirm_steps,
            "fallback_steps": if fallback { plain.work_steps } else { 0 }, "same": same,
        });
        writeln!(out, "{row}").map_err(|error| error.to_string())?;
    }
    Ok(())
}
