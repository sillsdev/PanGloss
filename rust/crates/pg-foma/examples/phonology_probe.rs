//! Builds one grammar's proposing phonology cascade and prints the size of every structure.
use foma::options::FomaOptions;
use pg_grammar::model::{Grammar, PhonRuleDef};
use std::fs;
use std::path::Path;

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

fn rule_id(rule: &PhonRuleDef) -> &str {
    match rule {
        PhonRuleDef::Rewrite(rule) => &rule.xml_id,
        PhonRuleDef::Metathesis(rule) => &rule.xml_id,
    }
}

fn main() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: phonology_probe <grammar>")?;
    let grammar = load(&path)?;
    let rules: Vec<&PhonRuleDef> = grammar
        .strata
        .iter()
        .flat_map(|stratum| &stratum.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect();
    let started = std::time::Instant::now();
    let outcome = pg_foma::replace::compile_proposing_cascade(
        &FomaOptions::default(),
        &grammar,
        &rules,
        &mut Vec::new(),
    );
    let report = match outcome {
        Ok((_, evidence)) => serde_json::json!({ "complete": true, "evidence": evidence }),
        Err(error) => serde_json::json!({ "complete": false, "refusal": error.to_string() }),
    };
    for rule in &rules {
        let alone = pg_foma::replace::compile_proposing_cascade(
            &FomaOptions::default(),
            &grammar,
            std::slice::from_ref(rule),
            &mut Vec::new(),
        );
        let outcome = match alone {
            Ok((_, evidence)) => {
                serde_json::json!({ "states": evidence.states, "arcs": evidence.arcs, "rows": evidence.bundles.private_rows })
            }
            Err(error) => serde_json::json!({ "refusal": error.to_string() }),
        };
        println!(
            "{}",
            serde_json::json!({ "rule": rule_id(rule), "alone": outcome })
        );
    }
    println!(
        "{}",
        serde_json::json!({
            "grammar": path, "rules": rules.len(), "tables": grammar.char_tables.len(),
            "elapsed_ms": started.elapsed().as_millis() as u64, "report": report,
        })
    );
    Ok(())
}
