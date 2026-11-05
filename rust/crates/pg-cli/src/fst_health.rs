//! `pangloss fst-health <grammar> [<out.json>]`: grammar characterization to one
//! `HealthReport`. See `docs/research/pg-cli-make-report-design-notes.md`.

use std::fs;

use pg_foma::characterization::characterization_findings;
use pg_foma_backend::health::HealthReport;
use pg_grammar::model::Grammar;

/// Builds a report from grammar characterization only; no backend compiler or corpus is run.
fn build_health_report(grammar: &Grammar) -> HealthReport {
    HealthReport::new(characterization_findings(grammar))
}

/// The `admission (per-axis breakdown)` fragment of `run_fst_health`'s completion message.
fn render_admission_summary(report: &HealthReport) -> String {
    format!(
        "{:?} ({})",
        report.admission(),
        report.admission_by_class().render()
    )
}

/// `pangloss fst-health <grammar> [<out.json>]`; `<out.json>` omitted writes the canonical JSON to stdout instead of a file, matching this crate's stdout/stderr split.
pub fn run_fst_health(args: &[String]) -> Result<(), String> {
    let mut config = pg_foma::construction::FstCompileConfig::default();
    let mut compile = false;
    let mut positionals = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if config.parse_option(args, &mut index)? {
            compile = true;
        } else {
            if args[index].starts_with("--") {
                return Err(format!("unknown option: {}", args[index]));
            }
            positionals.push(args[index].as_str());
            index += 1;
        }
    }
    config.strategies()?;
    let (grammar_path, out_path) = match positionals.as_slice() {
        [g] => (*g, None),
        [g, o] => (*g, Some(*o)),
        _ => return Err("usage: fst-health <grammar> [<out.json>] [--fst-strategy surface-enum|factored|auto] [--fst-auto-order surface-enum,factored]".to_string()),
    };

    let (grammar, warnings) = crate::load_grammar(grammar_path)?;
    crate::print_grammar_warnings(&warnings);

    let report = build_health_report(&grammar);
    let mut value = serde_json::to_value(&report).map_err(|error| error.to_string())?;
    if compile {
        value["construction"] = compile_report(&grammar, &config)?;
    }
    let json = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;

    match out_path {
        Some(path) => {
            fs::write(path, &json).map_err(|e| format!("write {path}: {e}"))?;
        }
        None => println!("{json}"),
    }

    eprintln!(
        "fst-health complete: {} finding(s), admission={} ({})",
        report.findings.len(),
        render_admission_summary(&report),
        if compile {
            "construction attempt recorded"
        } else {
            "characterization only"
        },
    );
    Ok(())
}

/// Runs the same strategy dispatcher used by the compile API, keeping failures in the report.
pub(crate) fn compile_report(
    grammar: &Grammar,
    config: &pg_foma::construction::FstCompileConfig,
) -> Result<serde_json::Value, String> {
    let (result, profile) = pg_foma::analyzer::compile_proposer_with_config(grammar, config);
    Ok(
        serde_json::json!({ "complete": result.is_ok(), "error": result.err().map(|error| error.to_string()), "profile": profile }),
    )
}

#[cfg(test)]
mod tests;
