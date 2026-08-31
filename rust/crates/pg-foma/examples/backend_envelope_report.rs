//! Each `EmissionStrategy`'s capability verdict per grammar path, with the typed diagnostics behind a refusal; takes arbitrary paths where `conf_matrix` walks discovered fixtures. Envelope only -- compiles no artifact, so it cannot say a build would succeed.

use std::path::Path;

use pg_foma::backend_selection::select_backends;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::strategy_coverage::ALL_STRATEGIES;
use pg_grammar::model::Grammar;

fn load(path: &str) -> Result<Grammar, String> {
    match Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
    {
        "json" => {
            let json = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            let snapshot = pg_snapshot::Snapshot::from_json(&json)
                .map_err(|e| format!("parse snapshot {path}: {e}"))?;
            pg_grammar::compile_project(&snapshot)
                .map(|(g, _)| g)
                .map_err(|e| format!("compile {path}: {e:?}"))
        }
        "fwdata" => {
            let (snapshot, _) =
                pg_fwdata::import_file(Path::new(path)).map_err(|e| format!("import {path}: {e}"))?;
            pg_grammar::compile_project(&snapshot)
                .map(|(g, _)| g)
                .map_err(|e| format!("compile {path}: {e:?}"))
        }
        _ => {
            let xml = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            pg_grammar::load(&xml).map_err(|e| format!("load {path}: {e:?}"))
        }
    }
}

fn report(name: &str, g: &Grammar) {
    let semantics = GrammarSemantics::derive(g);
    let selection = select_backends(&semantics);
    println!("\n=== {name} ===");
    let mut admitted = 0usize;
    for &strategy in ALL_STRATEGIES {
        match selection.report_for(strategy) {
            // Absent from the envelope is not a refusal: the gate admits an unreported strategy rather than inventing a verdict.
            None => println!("  {strategy:?}: no report (admitted by default)"),
            Some(report) if report.can_represent() => {
                admitted += 1;
                println!("  {strategy:?}: ADMITTED");
            }
            Some(report) => {
                println!("  {strategy:?}: REFUSED");
                for d in report.declined_on() {
                    println!("      predicate={} construct={}", d.predicate, d.construct);
                }
            }
        }
    }
    println!("  -> {admitted} of {} backends admitted", ALL_STRATEGIES.len());
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: backend_envelope_report <grammar>...");
        std::process::exit(2);
    }
    for path in &args {
        match load(path) {
            Ok(g) => report(path, &g),
            Err(e) => println!("\n=== {path} ===\n  LOAD FAILED: {e}"),
        }
    }
}
