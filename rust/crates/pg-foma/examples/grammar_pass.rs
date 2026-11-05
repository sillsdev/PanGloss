//! Streaming stage-0 differential; every word occurrence survives a compile refusal.
use pg_foma::construction::FstCompileConfig;
use pg_foma::differential::measure_word;
use pg_foma_runtime::compose_budget::ApplyBudget;
use pg_grammar::model::Grammar;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::Path;

fn load(path: &str) -> Result<Grammar, String> {
    match Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
    {
        "fwdata" | "fwbackup" => {
            let (snapshot, report) =
                pg_fwdata::import_file(Path::new(path)).map_err(|e| e.to_string())?;
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
        "json" => {
            let snapshot = pg_snapshot::Snapshot::from_json(
                &fs::read_to_string(path).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            pg_grammar::compile_project_with(&snapshot, Default::default())
                .map(|out| out.grammar)
                .map_err(|error| format!("{error:?}"))
        }
        _ => pg_grammar::load(&fs::read_to_string(path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("{e:?}")),
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut config = FstCompileConfig::default();
    let mut positionals = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if config.parse_option(&args, &mut index)? {
            continue;
        }
        positionals.push(args[index].as_str());
        index += 1;
    }
    let [source, words_path, output] = positionals.as_slice() else {
        return Err("usage: grammar_pass <grammar> <words.txt> <out.jsonl> [--fst-strategy surface-enum|factored|auto] [--fst-auto-order factored,surface-enum] [--fst-closure-work N] [--fst-structural-work N] [--fst-structural-bytes N] [--fst-closure-depth N] [--fst-derivation-depth N]".to_string());
    };
    config.strategies()?;
    let source_bytes = fs::read(source).map_err(|e| e.to_string())?;
    let words_bytes = fs::read(words_path).map_err(|e| e.to_string())?;
    let words: Vec<String> = String::from_utf8(words_bytes.clone())
        .map_err(|e| e.to_string())?
        .lines()
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .map(String::from)
        .collect();
    let pin = std::process::Command::new("git")
        .args(["rev-parse", "HEAD:machine"])
        .output()
        .map_err(|e| e.to_string())?;
    if !pin.status.success() {
        return Err("could not identify machine pin".to_string());
    }
    let source_hash = format!("{:x}", Sha256::digest(&source_bytes));
    let mut manifest = serde_json::json!({
        "schema_version": 1, "source": source, "source_sha256": source_hash,
        "xml_sha256": if source.ends_with(".xml") { Some(source_hash.as_str()) } else { None },
        "words_source": words_path, "words_sha256": format!("{:x}", Sha256::digest(&words_bytes)),
        "words": words, "machine_pin": String::from_utf8_lossy(&pin.stdout).trim(),
        "config": config, "phase": "load", "oracle": "HC-Rust (C# replay is separate)",
        "query_bounds": { "hc_work": 5_000_000_000usize, "apply_paths": 1_000_000, "apply_candidates": 1_000_000, "word_timeout": null },
    });
    let manifest_path = format!("{output}.manifest.json");
    let write_manifest = |value: &serde_json::Value| -> Result<(), String> {
        let pending_path = format!("{manifest_path}.pending");
        fs::write(
            &pending_path,
            serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(&pending_path, &manifest_path).map_err(|e| e.to_string())
    };
    write_manifest(&manifest)?;
    let grammar = load(source)?;
    manifest["language_bounds"] = serde_json::json!({
        "max_stem_count": grammar.max_stem_count, "deletion_reapplications": grammar.deletion_reapplications,
        "rule_max_applications": grammar.mrules.iter().map(|rule| rule.max_apps()).collect::<Vec<_>>(),
    });
    manifest["routing_witness"] =
        serde_json::to_value(pg_foma::emit::composite_candidate_rules(&grammar))
            .map_err(|error| error.to_string())?;
    manifest["phase"] = "compile".into();
    write_manifest(&manifest)?;
    let (compiled, profile) = pg_foma::analyzer::compile_proposer_with_config(&grammar, &config);
    let refusal = compiled.as_ref().err().map(|error| error.to_string());
    manifest["compile_complete"] = compiled.is_ok().into();
    manifest["compile_error"] = serde_json::to_value(&refusal).map_err(|e| e.to_string())?;
    manifest["profile"] = serde_json::to_value(&profile).map_err(|e| e.to_string())?;
    manifest["phase"] = "query".into();
    write_manifest(&manifest)?;
    let mut summary = pg_foma::differential::DifferentialSummary::default();
    let mut analyzer = compiled.ok().map(|proposer| {
        pg_foma::composite::FomaAnalyzer::from_precompiled_proposer(&grammar, proposer)
    });
    manifest["query_bounds"]["peel_chain_depth"] = serde_json::to_value(
        analyzer
            .as_ref()
            .map(|analyzer| analyzer.peel_chain_depth_cap()),
    )
    .map_err(|error| error.to_string())?;
    write_manifest(&manifest)?;
    let unimplemented = !profile.construction.attempts.is_empty()
        && profile.construction.attempts.iter().all(|attempt| {
            attempt.refusal == Some(pg_foma::construction::StrategyRefusal::NotImplemented)
        });
    let morpher = (!unimplemented).then(|| pg_parse::Morpher::new(&grammar, 5_000_000_000));
    let budget = ApplyBudget::with_caps(Some(1_000_000), Some(1_000_000));
    let mut writer = std::io::BufWriter::new(fs::File::create(output).map_err(|e| e.to_string())?);
    for (word_id, word) in words.iter().enumerate() {
        let row = match &morpher {
            Some(morpher) => measure_word(
                &grammar,
                morpher,
                analyzer.as_mut(),
                word_id,
                word,
                &budget,
                refusal.as_deref(),
            )?,
            None => pg_foma::differential::DifferentialRow::not_implemented(
                word_id,
                word,
                refusal
                    .as_deref()
                    .expect("unimplemented construction has a refusal"),
            ),
        };
        summary.record(&row);
        serde_json::to_writer(&mut writer, &row).map_err(|e| e.to_string())?;
        writeln!(writer).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        manifest["completed_rows"] = (word_id + 1).into();
        write_manifest(&manifest)?;
    }
    manifest["summary"] = serde_json::to_value(&summary).map_err(|error| error.to_string())?;
    manifest["phase"] = "complete".into();
    write_manifest(&manifest)?;
    println!(
        "{} words recorded; strategy={:?}; compile_complete={}",
        words.len(),
        config.strategy,
        analyzer.is_some()
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
