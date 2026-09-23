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
    let (grammar_path, out_path): (&str, Option<&str>) = match args {
        [g] => (g.as_str(), None),
        [g, o] => (g.as_str(), Some(o.as_str())),
        _ => {
            return Err("usage: fst-health <grammar> [<out.json>]".to_string());
        }
    };

    let (grammar, warnings) = crate::load_grammar(grammar_path)?;
    crate::print_grammar_warnings(&warnings);

    let report = build_health_report(&grammar);
    let json = report
        .to_json()
        .map_err(|e| format!("serialize health report: {e}"))?;

    match out_path {
        Some(path) => {
            fs::write(path, &json).map_err(|e| format!("write {path}: {e}"))?;
        }
        None => println!("{json}"),
    }

    eprintln!(
        "fst-health complete: {} finding(s), admission={} (characterization only)",
        report.findings.len(),
        render_admission_summary(&report),
    );
    Ok(())
}

#[cfg(test)]
mod tests;
