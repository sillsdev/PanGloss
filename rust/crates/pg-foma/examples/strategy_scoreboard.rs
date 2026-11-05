//! One column set per construction, using every word in the shared fixture inventory.
use pg_foma::construction::{ConstructionStrategy, FstCompileConfig};
use pg_foma::differential::{measure_word, DifferentialSummary};
use pg_foma_runtime::compose_budget::ApplyBudget;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Write;

fn run() -> Result<(), String> {
    let output = std::env::args()
        .nth(1)
        .ok_or("usage: strategy_scoreboard <out.jsonl>")?;
    let mut writer = std::io::BufWriter::new(File::create(output).map_err(|e| e.to_string())?);
    let pin = std::process::Command::new("git")
        .args(["rev-parse", "HEAD:machine"])
        .output()
        .map_err(|error| error.to_string())?;
    if !pin.status.success() {
        return Err("cannot identify machine pin".to_string());
    }
    serde_json::to_writer(&mut writer, &serde_json::json!({"machine_pin": String::from_utf8_lossy(&pin.stdout).trim(), "scope": "all words in discovered base fixtures", "config": FstCompileConfig::default()})).map_err(|error| error.to_string())?;
    writeln!(writer).map_err(|error| error.to_string())?;
    print!("| Fixture | Words |");
    for strategy in ConstructionStrategy::ALL {
        print!(" {} outcome | ms | process peak KiB | states | arcs | proposal recall | spurious confirmed | multiset equal/different | incomplete |", strategy.label());
    }
    println!();
    print!("|---|---:|");
    for _ in ConstructionStrategy::ALL {
        print!("---|---:|---:|---:|---:|---|---:|---|---:|");
    }
    println!();
    let budget = ApplyBudget::with_caps(Some(1_000_000), Some(1_000_000));
    for fixture in pg_conformance_fixtures::discover() {
        let words_yaml = fixture.load_words_yaml();
        if words_yaml.expect_crash {
            serde_json::to_writer(
                &mut writer,
                &serde_json::json!({"fixture": fixture.label(), "excluded": "expect_crash"}),
            )
            .map_err(|e| e.to_string())?;
            writeln!(writer).map_err(|e| e.to_string())?;
            println!(
                "| {} | {} | expect_crash |",
                fixture.label(),
                words_yaml.words.len()
            );
            continue;
        }
        let xml = fixture.load_grammar_xml();
        let grammar = fixture.load_grammar();
        let morpher = pg_parse::Morpher::new(&grammar, 5_000_000_000);
        print!("| {} | {} |", fixture.label(), words_yaml.words.len());
        for strategy in ConstructionStrategy::ALL {
            let (result, profile) = pg_foma::analyzer::compile_proposer_with_config(
                &grammar,
                &FstCompileConfig::for_strategy(strategy),
            );
            let refusal = result.as_ref().err().map(ToString::to_string);
            let mut analyzer = result.ok().map(|proposer| {
                pg_foma::composite::FomaAnalyzer::from_precompiled_proposer(&grammar, proposer)
            });
            let mut rows = Vec::new();
            let mut summary = DifferentialSummary::default();
            for (id, word) in words_yaml.words.iter().enumerate() {
                let row = measure_word(
                    &grammar,
                    &morpher,
                    analyzer.as_mut(),
                    id,
                    &word.word,
                    &budget,
                    refusal.as_deref(),
                )?;
                summary.record(&row);
                rows.push(row);
            }
            let attempt = &profile.construction.attempts[0];
            let (recall, spurious, parity) = if summary.complete_rows == 0 {
                (
                    "unmeasured".to_string(),
                    "unmeasured".to_string(),
                    "unmeasured".to_string(),
                )
            } else {
                (
                    format!(
                        "{}/{}",
                        summary
                            .oracle_identities
                            .saturating_sub(summary.proposal_misses),
                        summary.oracle_identities
                    ),
                    summary.spurious_confirmed.to_string(),
                    format!(
                        "{}/{}",
                        summary.multiset_equal_rows, summary.multiset_different_rows
                    ),
                )
            };
            let number = |value: Option<i64>| {
                value.map_or_else(|| "—".to_string(), |value| value.to_string())
            };
            print!(
                " {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                attempt
                    .refusal
                    .as_ref()
                    .map_or("complete".to_string(), |refusal| format!("{refusal:?}")),
                profile.total_elapsed_millis,
                attempt
                    .process_peak_rss_kib
                    .map_or_else(|| "—".to_string(), |value| value.to_string()),
                number(attempt.states),
                number(attempt.arcs),
                recall,
                spurious,
                parity,
                summary.incomplete_rows
            );
            serde_json::to_writer(&mut writer, &serde_json::json!({
                "fixture": fixture.label(), "xml_sha256": format!("{:x}", Sha256::digest(xml.as_bytes())),
                "strategy": strategy, "profile": profile, "summary": summary, "rows": rows,
                "oracle": "HC-Rust; fixture expectations and pinned C# replay are independent evidence"
            })).map_err(|e| e.to_string())?;
            writeln!(writer).map_err(|e| e.to_string())?;
            writer.flush().map_err(|e| e.to_string())?;
        }
        println!();
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
