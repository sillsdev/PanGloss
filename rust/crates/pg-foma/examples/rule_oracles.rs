//! Measures parser work with today's rule admission and each rule-set oracle (true rules, no unused free or tabled rules, no-analysis).
use foma::options::FomaOptions;
use pg_foma::admission::build_rule_admission;
use pg_foma_runtime::confirm::{build_morpheme_owners, MorphemeOwner};
use pg_grammar::model::{Grammar, MRuleId, MorphRuleDef, OutputAction};
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use pg_rules::stratum::RuleRef;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::Path;

const DEFAULT_WORK_CAP: usize = 2_000_000;

// `--capture-cap-mib N|unbounded`, as the byte ceiling `Morpher::with_capture_cap` takes.
fn parse_capture_cap(value: &str) -> Result<Option<usize>, String> {
    if value == "unbounded" {
        return Ok(None);
    }
    value
        .parse::<usize>()
        .ok()
        .and_then(|mib| mib.checked_mul(1024 * 1024))
        .map(Some)
        .ok_or_else(|| {
            format!("invalid --capture-cap-mib: {value} (expected MiB or \"unbounded\")")
        })
}

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

/// The parse the filter makes: only the rules in `set` may apply; strata and templates always do.
fn restricted(morpher: &Morpher, word: &str, set: &[bool]) -> ParseOutcome {
    let filter = |rule: RuleRef| match rule {
        RuleRef::Stratum(_) | RuleRef::Template(_) => true,
        RuleRef::MRule(id) => set[id.0 as usize],
    };
    morpher.parse_word_selected(word, &ParseOptions::default(), None, Some(&filter))
}

/// Why the plan leaves a rule free, as far as the grammar shows it.
fn free_reason(rule: &MorphRuleDef) -> String {
    let Some(allomorphs) = rule.affix_allomorphs() else {
        return "not an affix rule (compounding): the plan makes no piece for it".into();
    };
    let without_insertion = allomorphs
        .iter()
        .filter(|allo| {
            !allo
                .rhs
                .iter()
                .any(|a| matches!(a, OutputAction::InsertSegments { .. }))
        })
        .count();
    format!(
        "affix rule, {} subrules, {} insert no segments; the plan found no usable literal for some subrule",
        allomorphs.len(),
        without_insertion
    )
}

fn main() -> Result<(), String> {
    let mut args: Vec<String> = Vec::new();
    let mut work_cap = DEFAULT_WORK_CAP;
    let mut capture_cap: Option<Option<usize>> = None;
    let mut per_rule = false;
    let mut raw = std::env::args().skip(1);
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--work-cap" => {
                work_cap = raw
                    .next()
                    .and_then(|text| text.parse().ok())
                    .ok_or("--work-cap needs a number")?
            }
            "--per-rule" => per_rule = true,
            "--capture-cap-mib" => {
                capture_cap = Some(parse_capture_cap(
                    &raw.next().ok_or("--capture-cap-mib needs a value")?,
                )?)
            }
            _ => args.push(arg),
        }
    }
    let [source, words_path, output, limit] = args.as_slice() else {
        return Err("usage: rule_oracles <grammar> <words.txt> <out.jsonl> <max-words> [--work-cap N] [--capture-cap-mib N|unbounded] [--per-rule]".into());
    };
    let limit: usize = limit.parse().map_err(|_| "max-words must be a number")?;
    let g = load(source)?;
    let admission = build_rule_admission(&FomaOptions::default(), &g)
        .map_err(|refusal| format!("admission refused: {}", refusal.reason()))?;
    let free = admission.free();
    let affix: Vec<bool> = g
        .mrules
        .iter()
        .map(|rule| rule.affix_allomorphs().is_some())
        .collect();
    let free_rules: Vec<serde_json::Value> = (0..g.mrules.len())
        .filter(|&i| free[i] && affix[i])
        .map(|i| {
            let item = g.mrules[i]
                .morpheme()
                .map(|m| g.morphemes[m.0 as usize].xml_key.clone());
            serde_json::json!({"rule": i, "item": item, "reason": free_reason(&g.mrules[i])})
        })
        .collect();
    println!(
        "{}",
        serde_json::json!({
            "rules": g.mrules.len(), "affix_rules": affix.iter().filter(|a| **a).count(),
            "free_affix_rules": free_rules,
            "free_non_affix": (0..g.mrules.len()).filter(|&i| free[i] && !affix[i]).count(),
            "work_cap": work_cap,
        })
    );
    let mut morpher = Morpher::new(&g, work_cap);
    if let Some(cap) = capture_cap {
        morpher = morpher.with_capture_cap(cap);
    }
    let owners = build_morpheme_owners(&g);
    let words = fs::read_to_string(words_path).map_err(|error| error.to_string())?;
    let mut out = fs::File::create(output).map_err(|error| error.to_string())?;
    for word in words
        .lines()
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .take(limit)
    {
        let plain = morpher.parse_word_selected(word, &ParseOptions::default(), None, None);
        let truth = signature(&plain);
        let used: BTreeSet<u32> = plain
            .structured
            .iter()
            .flat_map(|analysis| analysis.morpheme_ids.iter())
            .filter_map(
                |&morpheme| match owners.get(morpheme as usize).copied().flatten() {
                    Some(MorphemeOwner::MRule(MRuleId(id))) => Some(id),
                    _ => None,
                },
            )
            .collect();
        let today = admission.admit(word);
        // Rules that are not affix rules (compounding) are never filtered, in every set.
        let oracle: Vec<bool> = (0..affix.len())
            .map(|i| !affix[i] || used.contains(&(i as u32)))
            .collect();
        let no_free: Vec<bool> = (0..affix.len())
            .map(|i| today[i] && !(free[i] && affix[i] && !used.contains(&(i as u32))))
            .collect();
        let no_spotted: Vec<bool> = (0..affix.len())
            .map(|i| today[i] && !(!free[i] && affix[i] && !used.contains(&(i as u32))))
            .collect();
        let a = restricted(&morpher, word, &today);
        let o = restricted(&morpher, word, &oracle);
        let of = restricted(&morpher, word, &no_free);
        let os = restricted(&morpher, word, &no_spotted);
        let unused_free: Vec<usize> = (0..affix.len())
            .filter(|&i| free[i] && affix[i] && !used.contains(&(i as u32)))
            .collect();
        let mut gains = serde_json::Map::new();
        if per_rule && of.work_steps < a.work_steps {
            for &rule in &unused_free {
                let mut set = today.clone();
                set[rule] = false;
                let steps = restricted(&morpher, word, &set).work_steps;
                if steps < a.work_steps {
                    gains.insert(rule.to_string(), (a.work_steps - steps).into());
                }
            }
        }
        let cell = |o: &ParseOutcome| serde_json::json!({"steps": o.work_steps, "same": signature(o) == truth, "capped": o.capped, "capture_capped": o.capture_capped});
        let row = serde_json::json!({
            "word": word, "analyses": truth.len(), "capped": plain.capped, "capture_capped": plain.capture_capped,
            "plain": plain.work_steps, "a": cell(&a), "o": cell(&o), "of": cell(&of), "os": cell(&os),
            "true_rules": used.len(), "unused_free": unused_free.len(),
            "admitted": today.iter().filter(|x| **x).count(),
            "free_gain": gains,
        });
        writeln!(out, "{row}").map_err(|error| error.to_string())?;
    }
    Ok(())
}
