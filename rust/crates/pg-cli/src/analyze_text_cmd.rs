//! `analyze-text`: prints `pg_analyzer::analyze_text` as JSON; format in `docs/text-analysis-format.md`.

use pg_analyzer::{AnalyzeOptions, LanguagePack, ProfileInput, TextInput, TextRequest};
use pg_parse::Morpher;
use std::time::Duration;

const USAGE: &str = "usage: analyze-text --profile <qaa.ldml|profile.json> --pack <language>=<grammar> [--pack ...] [--work-cap N|unbounded] [--word-timeout-ms N] [--max-checked-occurrences N] <text-file>...";

fn flag_value<'a>(
    flag: &str,
    arg: &'a str,
    it: &mut std::slice::Iter<'a, String>,
) -> Result<Option<&'a str>, String> {
    if arg == flag {
        let value = it.next().ok_or(format!("{flag} requires a value"))?;
        return Ok(Some(value));
    }
    Ok(arg
        .strip_prefix(flag)
        .and_then(|rest| rest.strip_prefix('=')))
}

pub(crate) fn run_analyze_text(args: &[String]) -> Result<(), String> {
    let mut profile_path = None;
    let mut pack_args: Vec<(String, String)> = Vec::new();
    let mut work_cap = crate::DEFAULT_WORK_CAP;
    let mut timeout = None;
    let mut options = AnalyzeOptions::default();
    let mut text_paths: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if let Some(v) = flag_value("--profile", arg, &mut it)? {
            profile_path = Some(v);
        } else if let Some(v) = flag_value("--pack", arg, &mut it)? {
            let (language, path) = v
                .split_once('=')
                .filter(|(l, p)| !l.is_empty() && !p.is_empty())
                .ok_or(format!("--pack expects <language>=<grammar>, got {v}"))?;
            pack_args.push((language.into(), path.into()));
        } else if let Some(v) = flag_value("--work-cap", arg, &mut it)? {
            work_cap = crate::parse_work_cap(v)?;
        } else if let Some(v) = flag_value("--word-timeout-ms", arg, &mut it)? {
            timeout = Some(Duration::from_millis(
                v.parse()
                    .map_err(|_| "--word-timeout-ms must be an integer")?,
            ));
        } else if let Some(v) = flag_value("--max-checked-occurrences", arg, &mut it)? {
            options.max_checked_occurrences = Some(
                v.parse()
                    .map_err(|_| "--max-checked-occurrences must be an integer")?,
            );
        } else {
            crate::reject_unknown_option("analyze-text", arg)?;
            text_paths.push(arg);
        }
    }
    let profile_path = profile_path.ok_or(USAGE)?;
    if text_paths.is_empty() {
        return Err(USAGE.into());
    }

    let profile_text =
        std::fs::read_to_string(profile_path).map_err(|e| format!("read {profile_path}: {e}"))?;
    let profile = if profile_path.ends_with(".json") {
        ProfileInput::Json(&profile_text)
    } else {
        ProfileInput::Ldml {
            identity: profile_path,
            xml: &profile_text,
        }
    };

    let mut grammars = Vec::new();
    for (language, path) in &pack_args {
        let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
        let (grammar, warnings) = crate::load_grammar(path)?;
        crate::print_grammar_warnings(&warnings);
        grammars.push((
            language,
            path,
            crate::stats_manifest::sha256_hex(&bytes),
            grammar,
        ));
    }
    let morphers: Vec<Morpher<'_>> = grammars
        .iter()
        .map(|(.., grammar)| {
            Morpher::new(grammar, work_cap.as_morpher_cap())
                .with_word_timeout(timeout)
                .with_guess_roots(false)
        })
        .collect();
    let packs: Vec<LanguagePack<'_>> = grammars
        .iter()
        .zip(&morphers)
        .map(|((language, path, sha, _), morpher)| LanguagePack {
            language: (*language).clone(),
            source_id: (*path).clone(),
            source_sha256: sha.clone(),
            morpher,
        })
        .collect();

    let contents: Vec<String> = text_paths
        .iter()
        .map(|p| std::fs::read_to_string(p).map_err(|e| format!("read {p}: {e}")))
        .collect::<Result<_, _>>()?;
    let texts: Vec<TextInput<'_>> = text_paths
        .iter()
        .zip(&contents)
        .map(|(path, text)| TextInput {
            id: (*path).into(),
            text,
            styles: &[],
        })
        .collect();

    let result = pg_analyzer::analyze_text(&TextRequest {
        profile,
        packs: &packs,
        texts: &texts,
        options,
    })?;
    let json = serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?;
    println!("{json}");
    Ok(())
}
