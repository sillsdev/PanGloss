//! Per grammar on the command line: how many root allomorphs enumerate past `REP_VARIANT_CAP`, how far past, and how many are unbounded (`pg_foma::emit::root_variant_census`, which multiplies counts instead of materializing strings).

use std::path::Path;

use pg_foma::emit::{
    eager_route_drops_root_spellings, root_variant_census, RootVariantFact,
    REP_VARIANT_WARN_THRESHOLD,
};
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

/// Buckets by absolute magnitude, not by the current cap, so rows stay comparable across a cap change.
fn bucket(fact: &RootVariantFact) -> &'static str {
    match fact.variants {
        None => "unbounded",
        Some(n) if n <= 64 => "<=64",
        Some(n) if n <= 1_000 => "65..1e3",
        Some(n) if n <= 1_000_000 => "1e3..1e6",
        Some(_) => ">1e6",
    }
}

fn report(name: &str, g: &Grammar) {
    let facts = root_variant_census(g);
    let notable = facts.iter().filter(|f| f.notable()).count();
    let unbounded = facts.iter().filter(|f| f.unbounded()).count();
    let worst_finite = facts.iter().filter_map(|f| f.variants).max().unwrap_or(0);

    println!("\n=== {name} ===");
    println!(
        "root allomorphs: {}  above advisory threshold: {notable}  unbounded: {unbounded}  worst finite product: {worst_finite}",
        facts.len()
    );
    // The gate the capability envelope actually reads, not the census's own arithmetic about it. It fires on ABSENT spellings only, so breadth alone must leave it false.
    println!(
        "  warn-threshold={REP_VARIANT_WARN_THRESHOLD}  eager_route_drops_root_spellings = {}",
        eager_route_drops_root_spellings(g)
    );

    for label in ["<=64", "65..1e3", "1e3..1e6", ">1e6", "unbounded"] {
        let n = facts.iter().filter(|f| bucket(f) == label).count();
        if n > 0 {
            println!("  {label:<12} {n}");
        }
    }

    let mut worst: Vec<&RootVariantFact> = facts.iter().filter(|f| f.notable()).collect();
    // Unbounded first, then by descending product: only the unbounded rows lose spellings.
    worst.sort_by_key(|f| (f.variants.is_some(), std::cmp::Reverse(f.variants.unwrap_or(0))));
    for fact in worst.iter().take(10) {
        let magnitude = match fact.variants {
            None => "UNBOUNDED (Kleene star)".to_string(),
            Some(n) => n.to_string(),
        };
        println!(
            "  {:<44} nodes={:<3} pattern={:<5} variants={}",
            if fact.text.chars().count() > 42 {
                format!("{}...", fact.text.chars().take(39).collect::<String>())
            } else {
                fact.text.clone()
            },
            fact.nodes,
            fact.is_pattern,
            magnitude
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: rep_variant_census <grammar>...");
        std::process::exit(2);
    }
    for path in &args {
        match load(path) {
            Ok(g) => report(path, &g),
            Err(e) => println!("\n=== {path} ===\n  LOAD FAILED: {e}"),
        }
    }
}
