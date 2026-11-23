//! `pangloss why-not`: parse one word along given morph candidates; the search is a stub that answers unknown.

use std::io::Read;

use pg_parse::why_not::{
    resolve_guid, why_not, Candidate, CandidateResult, ExpectedMorph, GuidRef, WhyNotError,
    WhyNotOutcome, WhyNotReport, WhyNotRequest,
};

pub(crate) const USAGE: &str = "usage: why-not <grammar> <word> (--expect <spec> | --expect-guid <spec> | --expect-json <file|->)... [--allomorphs <spec>] [--json] [--work-cap N|unbounded] [--word-timeout-ms N]";

/// One candidate before its ids are resolved against a grammar.
enum Spec {
    Ids(Candidate),
    Guids(Vec<Vec<GuidRef>>),
}

struct Parsed<'a> {
    grammar: &'a str,
    word: &'a str,
    specs: Vec<Spec>,
    json: bool,
    work_cap_given: bool,
    timeout_given: bool,
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

/// `entryGuid:msaGuid` choices with the same `,` and `|` structure.
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
                        "--expect-guid choice `{choice}` must be entryGuid:msaGuid"
                    )),
                })
                .collect()
        })
        .collect()
}

fn read_expect_json(source: &str) -> Result<Vec<Candidate>, String> {
    let text = if source == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| format!("read stdin: {e}"))?;
        text
    } else {
        std::fs::read_to_string(source).map_err(|e| format!("read {source}: {e}"))?
    };
    serde_json::from_str::<Vec<Candidate>>(&text)
        .map_err(|e| format!("--expect-json must be an array of candidates, each an array of positions, each an array of {{\"morpheme\":\"id\"}} choices: {e}"))
}

fn parse_args(args: &[String]) -> Result<Parsed<'_>, String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut specs: Vec<Spec> = Vec::new();
    let mut expect_count = 0usize;
    let mut json_sources: Vec<String> = Vec::new();
    let mut json = false;
    let mut work_cap_given = false;
    let mut timeout_given = false;
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
            "--allomorphs" => return Err(WhyNotError::AllomorphSelectionUnavailable.to_string()),
            "--expect-json" => {
                json_sources.push(value(name)?);
            }
            "--work-cap" => {
                work_cap_given = true;
                crate::parse_work_cap(&value(name)?)?;
            }
            "--word-timeout-ms" => {
                timeout_given = true;
                value(name)?
                    .parse::<u64>()
                    .map_err(|_| "--word-timeout-ms must be an integer")?;
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
        specs.extend(read_expect_json(source)?.into_iter().map(Spec::Ids));
    }
    let [grammar, word] = positional[..] else {
        return Err(USAGE.into());
    };
    if specs.is_empty() {
        return Err(format!(
            "why-not needs at least one candidate: --expect, --expect-guid or --expect-json\n{USAGE}"
        ));
    }
    Ok(Parsed {
        grammar,
        word,
        specs,
        json,
        work_cap_given,
        timeout_given,
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
                "unknown ({})",
                serde_json::to_value(s)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
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

pub(crate) fn run_why_not(args: &[String]) -> Result<(), String> {
    let parsed = parse_args(args)?;
    let (grammar, warnings) = crate::load_grammar(parsed.grammar)?;
    crate::print_grammar_warnings(&warnings);

    let mut candidates = Vec::with_capacity(parsed.specs.len());
    for spec in parsed.specs {
        candidates.push(match spec {
            Spec::Ids(c) => c,
            Spec::Guids(positions) => {
                let mut out = Vec::with_capacity(positions.len());
                for position in positions {
                    let mut choices = Vec::new();
                    for guid in position {
                        let entry = guid.entry_guid.clone().unwrap_or_default();
                        let msa = guid.msa_guid.clone().unwrap_or_default();
                        let ids = resolve_guid(&grammar, &entry, &msa);
                        if ids.is_empty() {
                            return Err(WhyNotError::UnknownGuid {
                                entry_guid: entry,
                                msa_guid: msa,
                            }
                            .to_string());
                        }
                        choices.extend(ids.into_iter().map(|m| {
                            let mut morph = ExpectedMorph::new(m);
                            morph.guid = Some(guid.clone());
                            morph
                        }));
                    }
                    out.push(choices);
                }
                Candidate { positions: out }
            }
        });
    }

    if parsed.work_cap_given || parsed.timeout_given {
        eprintln!(
            "why-not: slice 0 is a stub; --work-cap and --word-timeout-ms are validated but not yet applied"
        );
    }
    let report = why_not(
        &grammar,
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
