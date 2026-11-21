//! `pangloss why-not`: one word along given morph candidates, or a JSONL suite of such queries.

use std::io::Read;

use std::time::Duration;

use pg_parse::why_not::{
    expand_guid, why_not, Candidate, CandidateResult, ExpectedMorph, GuidRef, WhyNotOutcome,
    WhyNotReport, WhyNotRequest,
};
use pg_parse::Morpher;
use pg_stats::WorkCap;

pub(crate) const USAGE: &str = "usage: why-not <grammar> <word> (--expect <spec> | --expect-guid <spec> | --expect-json <file|->)... [--allomorphs <spec>] [--json] [--work-cap N|unbounded] [--word-timeout-ms N]\n       why-not <grammar> <queries.jsonl> <output.jsonl> [--work-cap N|unbounded] [--word-timeout-ms N]";

/// One candidate before its ids are resolved against a grammar.
enum Spec {
    Ids(Candidate),
    Guids(Vec<Vec<GuidRef>>),
    Json(Vec<Vec<JsonChoice>>),
}

/// A `--expect-json` choice: a morpheme id, a guid, or both (which must agree).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonChoice {
    morpheme: Option<String>,
    allomorph: Option<String>,
    guid: Option<GuidRef>,
}

struct Parsed<'a> {
    grammar: &'a str,
    word: &'a str,
    specs: Vec<Spec>,
    /// A third positional: the suite runner's output file.
    suite_output: Option<&'a str>,
    json: bool,
    work_cap: WorkCap,
    word_timeout: Option<Duration>,
}

/// A grammar.xml id token: non-empty (ids are NCNames, so they never contain `,`, `|` or `@`).
fn parse_id(token: &str, what: &str) -> Result<String, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err(format!("{what} is empty"));
    }
    Ok(token.to_string())
}

/// `le1,sfx3|sfx4@a2,le2`: `,` between positions, `|` between choices, `@id` for an allomorph.
fn parse_expect(spec: &str) -> Result<Candidate, String> {
    let mut positions = Vec::new();
    for position in spec.split(',') {
        let mut choices = Vec::new();
        for choice in position.split('|') {
            let (m, a) = match choice.split_once('@') {
                Some((m, a)) => (m, Some(a)),
                None => (choice, None),
            };
            let mut morph = ExpectedMorph::new(parse_id(m, "morpheme id")?);
            morph.allomorph = a.map(|a| parse_id(a, "allomorph id")).transpose()?;
            choices.push(morph);
        }
        positions.push(choices);
    }
    Ok(Candidate { positions })
}

/// `a1,,a3|a4`: one allomorph id per choice, an empty token leaving that choice unpinned.
fn apply_allomorphs(candidate: &mut Candidate, spec: &str) -> Result<(), String> {
    let positions: Vec<&str> = spec.split(',').collect();
    if positions.len() != candidate.positions.len() {
        return Err(format!(
            "--allomorphs lists {} positions but the --expect before it has {}",
            positions.len(),
            candidate.positions.len()
        ));
    }
    for (pi, (position, choices)) in positions.iter().zip(&mut candidate.positions).enumerate() {
        let ids: Vec<&str> = position.split('|').collect();
        if ids.len() != choices.len() {
            return Err(format!(
                "--allomorphs position {pi} lists {} choices but the --expect before it has {}",
                ids.len(),
                choices.len()
            ));
        }
        for (id, choice) in ids.iter().zip(choices.iter_mut()) {
            let id = id.trim();
            if id.is_empty() {
                continue;
            }
            if choice.allomorph.is_some() {
                return Err(format!(
                    "morpheme {} already names an allomorph with `@`; drop it from --allomorphs",
                    choice.morpheme
                ));
            }
            choice.allomorph = Some(id.to_string());
        }
    }
    Ok(())
}

/// `entry_guid:msa_guid` choices with the same `,` and `|` structure.
fn parse_expect_guid(spec: &str) -> Result<Vec<Vec<GuidRef>>, String> {
    spec.split(',')
        .map(|position| {
            position
                .split('|')
                .map(|choice| match choice.split_once(':') {
                    Some((e, m)) if !e.is_empty() && !m.is_empty() => Ok(GuidRef {
                        entry_guid: Some(e.to_string()),
                        msa_guid: Some(m.to_string()),
                    }),
                    _ => Err(format!(
                        "--expect-guid choice `{choice}` must be entry_guid:msa_guid"
                    )),
                })
                .collect()
        })
        .collect()
}

fn read_expect_json(source: &str) -> Result<Vec<Vec<Vec<JsonChoice>>>, String> {
    let text = if source == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| format!("read stdin: {e}"))?;
        text
    } else {
        std::fs::read_to_string(source).map_err(|e| format!("read {source}: {e}"))?
    };
    let candidates = serde_json::from_str::<Vec<Vec<Vec<JsonChoice>>>>(&text)
        .map_err(|e| format!("--expect-json must be an array of candidates, each an array of positions, each an array of {{\"morpheme\":\"id\"}} or {{\"guid\":{{\"entry_guid\":..,\"msa_guid\":..}}}} choices: {e}"))?;
    for choice in candidates.iter().flatten().flatten() {
        match (&choice.morpheme, &choice.guid) {
            (None, None) => return Err("--expect-json choice needs a `morpheme` id or a `guid`".into()),
            (None, Some(g)) if g.entry_guid.is_none() || g.msa_guid.is_none() => {
                return Err("--expect-json choice without a `morpheme` needs both guid.entry_guid and guid.msa_guid".into())
            }
            _ => {}
        }
    }
    Ok(candidates)
}

fn parse_args(args: &[String]) -> Result<Parsed<'_>, String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut specs: Vec<Spec> = Vec::new();
    let mut expect_count = 0usize;
    let mut json_sources: Vec<String> = Vec::new();
    let mut json = false;
    let mut work_cap = crate::DEFAULT_WORK_CAP;
    let mut word_timeout: Option<Duration> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let (name, inline) = match a.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
            _ => (a.as_str(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            match inline.clone() {
                Some(v) => Ok(v),
                None => it
                    .next()
                    .cloned()
                    .ok_or_else(|| format!("{name} requires a value")),
            }
        };
        match name {
            "--json" => json = true,
            "--expect" => {
                specs.push(Spec::Ids(parse_expect(&value(name)?)?));
                expect_count += 1;
            }
            "--expect-guid" => specs.push(Spec::Guids(parse_expect_guid(&value(name)?)?)),
            "--allomorphs" => {
                let spec = value(name)?;
                match specs.last_mut() {
                    Some(Spec::Ids(c)) => apply_allomorphs(c, &spec)?,
                    Some(Spec::Guids(_) | Spec::Json(_)) => {
                        return Err(
                            "--allomorphs pairs with --expect, not --expect-guid; name an allomorph in --expect-json instead"
                                .into(),
                        )
                    }
                    None => return Err("--allomorphs needs an --expect before it".into()),
                }
            }
            "--expect-json" => {
                json_sources.push(value(name)?);
            }
            "--work-cap" => work_cap = crate::parse_work_cap(&value(name)?)?,
            "--word-timeout-ms" => {
                word_timeout = Some(Duration::from_millis(
                    value(name)?
                        .parse::<u64>()
                        .map_err(|_| "--word-timeout-ms must be an integer")?,
                ));
            }
            s => {
                crate::reject_unknown_option("why-not", s)?;
                positional.push(s);
            }
        }
    }
    let used_json = !json_sources.is_empty();
    if used_json && (expect_count > 0 || specs.iter().any(|s| matches!(s, Spec::Guids(_)))) {
        return Err("--expect-json cannot be combined with --expect or --expect-guid".into());
    }
    for source in &json_sources {
        specs.extend(read_expect_json(source)?.into_iter().map(Spec::Json));
    }
    let (grammar, word, suite_output) = match positional[..] {
        [grammar, queries, output] if specs.is_empty() => (grammar, queries, Some(output)),
        [grammar, word] => (grammar, word, None),
        _ => return Err(USAGE.into()),
    };
    if suite_output.is_some() && (json || !specs.is_empty()) {
        return Err(format!(
            "the suite form takes no --json or --expect flags: its output file is JSONL already\n{USAGE}"
        ));
    }
    if suite_output.is_none() && specs.is_empty() {
        return Err(format!(
            "why-not needs at least one candidate: --expect, --expect-guid or --expect-json\n{USAGE}"
        ));
    }
    Ok(Parsed {
        grammar,
        word,
        specs,
        suite_output,
        json,
        work_cap,
        word_timeout,
    })
}

fn describe_choice(c: &ExpectedMorph) -> String {
    match &c.allomorph {
        Some(a) => format!("{}@{a}", c.morpheme),
        None => c.morpheme.clone(),
    }
}

fn describe_candidate(c: &Candidate) -> String {
    c.positions
        .iter()
        .map(|p| p.iter().map(describe_choice).collect::<Vec<_>>().join("|"))
        .collect::<Vec<_>>()
        .join(",")
}

fn render_text(report: &WhyNotReport) -> String {
    let mut out = format!("why-not {}\n", report.word);
    for (i, CandidateResult { expect, outcome }) in report.results.iter().enumerate() {
        let mark = if report.best == Some(i) {
            " (best)"
        } else {
            ""
        };
        let verdict = match outcome {
            WhyNotOutcome::Accepted(_) => "accepted".to_string(),
            WhyNotOutcome::Rejected(f) => format!("rejected: {}", f.plain),
            WhyNotOutcome::Unknown(s) => format!(
                "unknown ({}){}",
                serde_json::to_value(s.reason)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
                s.note
                    .as_ref()
                    .map(|n| format!(": {n}"))
                    .unwrap_or_default()
            ),
        };
        out.push_str(&format!(
            "  [{i}] {}: {verdict}{mark}\n",
            describe_candidate(expect)
        ));
    }
    out
}

/// The morpher both modes use: the caller's work cap and word timeout bound every reparse.
pub(crate) fn why_not_morpher(
    grammar: &pg_grammar::model::Grammar,
    work_cap: WorkCap,
    word_timeout: Option<Duration>,
) -> Morpher<'_> {
    Morpher::new(grammar, work_cap.as_morpher_cap()).with_word_timeout(word_timeout)
}

pub(crate) fn run_why_not(args: &[String]) -> Result<(), String> {
    let parsed = parse_args(args)?;
    let (grammar, warnings) = crate::load_grammar(parsed.grammar)?;
    crate::print_grammar_warnings(&warnings);
    let morpher = why_not_morpher(&grammar, parsed.work_cap, parsed.word_timeout);
    if let Some(output) = parsed.suite_output {
        return crate::why_not_suite::run_suite(
            &grammar,
            &morpher,
            std::path::Path::new(parsed.word),
            std::path::Path::new(output),
        );
    }

    let mut candidates = Vec::with_capacity(parsed.specs.len());
    for spec in parsed.specs {
        candidates.push(match spec {
            Spec::Ids(c) => c,
            Spec::Json(positions) => {
                let mut out = Vec::with_capacity(positions.len());
                for position in positions {
                    let mut choices = Vec::new();
                    for choice in position {
                        let JsonChoice {
                            morpheme,
                            allomorph,
                            guid,
                        } = choice;
                        let Some(morpheme) = morpheme else {
                            let guid = guid.expect("checked when read");
                            choices.extend(
                                expand_guid(&grammar, &guid, allomorph.as_deref())
                                    .map_err(|e| e.to_string())?,
                            );
                            continue;
                        };
                        let mut morph = ExpectedMorph::new(morpheme);
                        morph.allomorph = allomorph;
                        morph.guid = guid;
                        choices.push(morph);
                    }
                    out.push(choices);
                }
                Candidate { positions: out }
            }
            Spec::Guids(positions) => {
                let mut out = Vec::with_capacity(positions.len());
                for position in positions {
                    let mut choices = Vec::new();
                    for guid in position {
                        choices
                            .extend(expand_guid(&grammar, &guid, None).map_err(|e| e.to_string())?);
                    }
                    out.push(choices);
                }
                Candidate { positions: out }
            }
        });
    }

    let report = why_not(
        &grammar,
        &morpher,
        &WhyNotRequest {
            word: parsed.word,
            candidates,
            root_index: None,
        },
    )
    .map_err(|e| e.to_string())?;
    if parsed.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    } else {
        print!("{}", render_text(&report));
    }
    Ok(())
}
