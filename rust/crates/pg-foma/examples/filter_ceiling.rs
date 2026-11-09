//! Measures how much HC parser work a root and rule pre-filter of a given precision could save.
use pg_foma_runtime::confirm::{build_morpheme_owners, MorphemeOwner};
use pg_grammar::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef};
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use pg_rules::stratum::RuleRef;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::Path;

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

struct Selection {
    entries: Option<BTreeSet<u32>>,
    rules: Option<BTreeSet<u32>>,
}

fn restricted(g: &Grammar, morpher: &Morpher, word: &str, selection: &Selection) -> ParseOutcome {
    let lex = |entry: LexEntryId| {
        selection
            .entries
            .as_ref()
            .is_none_or(|set| set.contains(&entry.0))
    };
    let rule = |rule: RuleRef| match rule {
        RuleRef::Stratum(_) | RuleRef::Template(_) => true,
        RuleRef::MRule(id) => {
            matches!(g.mrules[id.0 as usize], MorphRuleDef::Compounding(_))
                || selection
                    .rules
                    .as_ref()
                    .is_none_or(|set| set.contains(&id.0))
        }
    };
    morpher.parse_word_selected(word, &ParseOptions::default(), Some(&lex), Some(&rule))
}

fn padded(truth: &BTreeSet<u32>, extra: usize, universe: usize, seed: &mut u64) -> BTreeSet<u32> {
    let mut set = truth.clone();
    for _ in 0..extra.min(universe) {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        set.insert(((*seed >> 33) % universe as u64) as u32);
    }
    set
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [source, words_path, output, limit] = args.as_slice() else {
        return Err("usage: filter_ceiling <grammar> <words.txt> <out.jsonl> <max-words>".into());
    };
    let limit: usize = limit.parse().map_err(|_| "max-words must be a number")?;
    let g = load(source)?;
    let owners = build_morpheme_owners(&g);
    let morpher = Morpher::new(&g, WORK_CAP);
    let words = fs::read_to_string(words_path).map_err(|error| error.to_string())?;
    let mut out = fs::File::create(output).map_err(|error| error.to_string())?;
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for word in words
        .lines()
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .take(limit)
    {
        let plain = morpher.parse_word_selected(word, &ParseOptions::default(), None, None);
        let truth = signature(&plain);
        let mut entries = BTreeSet::new();
        let mut rules = BTreeSet::new();
        for analysis in &plain.structured {
            for &morpheme in &analysis.morpheme_ids {
                match owners.get(morpheme as usize).copied().flatten() {
                    Some(MorphemeOwner::LexEntry(LexEntryId(id))) => entries.insert(id),
                    Some(MorphemeOwner::MRule(MRuleId(id))) => rules.insert(id),
                    None => false,
                };
            }
        }
        let variants = [
            ("roots", Some(entries.clone()), None),
            ("rules", None, Some(rules.clone())),
            ("both", Some(entries.clone()), Some(rules.clone())),
            (
                "roots30",
                Some(padded(&entries, 30, g.entries.len(), &mut seed)),
                None,
            ),
            (
                "both30",
                Some(padded(&entries, 30, g.entries.len(), &mut seed)),
                Some(padded(&rules, 30, g.mrules.len(), &mut seed)),
            ),
            ("norules", Some(entries.clone()), Some(BTreeSet::new())),
        ];
        let mut row = serde_json::json!({
            "word": word, "plain_steps": plain.work_steps, "analyses": truth.len(),
            "capped": plain.capped, "true_roots": entries.len(), "true_rules": rules.len(),
        });
        for (name, entries, rules) in variants {
            let outcome = restricted(&g, &morpher, word, &Selection { entries, rules });
            row[name] = serde_json::json!({
                "steps": outcome.work_steps,
                "same": signature(&outcome) == truth,
                "capped": outcome.capped,
            });
        }
        writeln!(out, "{row}").map_err(|error| error.to_string())?;
    }
    Ok(())
}
