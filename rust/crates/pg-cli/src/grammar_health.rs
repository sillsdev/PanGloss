//! `pangloss grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids]`: run
//! the ported `hc-*` HermitCrab grammar-authoring checks (`pg_grammar::grammar_health`) and
//! print/serialize the diagnostics.
//!
//! Deliberately a SEPARATE command from `fst-health`, not a section added to it: the two answer
//! different questions (grammar authoring correctness vs. FST compilation/production readiness),
//! and `fst-health`'s JSON is a versioned wire shape (`pg_health::HEALTH_SCHEMA_VERSION`) read by
//! `pg-pack`/`pg-wasm` -- folding a second vocabulary into it would need a version bump for a
//! question those readers never asked. This command emits one versioned structured report and a
//! lossless plain-text log.

use std::fs;

use pg_grammar::grammar_health::{
    check_grammar_health_diagnostics, render_json, render_log, FieldWorksProject,
    FieldWorksProjectSource, GrammarHealthDiagnostic, GrammarHealthReport,
};
use pg_snapshot::DiagnosticLevel;

/// `pangloss grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids]`;
/// `<out.json>` omitted prints the versioned report to stdout instead of a file. Diagnostics are
/// also logged one per line on stderr.
pub fn run_grammar_health(args: &[String]) -> Result<(), String> {
    let mut positionals = Vec::new();
    let mut fieldworks_project = None;
    let mut log_guids = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--log-guids" => {
                log_guids = true;
                index += 1;
            }
            "--fw-project" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--fw-project requires a project name".to_string())?;
                if value.starts_with("--") || value.trim().is_empty() {
                    return Err("--fw-project requires a nonempty project name".to_string());
                }
                fieldworks_project = Some(value.as_str());
                index += 2;
            }
            arg if arg.starts_with("--fw-project=") => {
                let value = arg.trim_start_matches("--fw-project=");
                if value.trim().is_empty() {
                    return Err("--fw-project requires a nonempty project name".to_string());
                }
                fieldworks_project = Some(value);
                index += 1;
            }
            arg if arg.starts_with("--") => {
                return Err(format!("unknown option: {arg}"));
            }
            arg => {
                positionals.push(arg);
                index += 1;
            }
        }
    }
    let (grammar_path, out_path) = match positionals.as_slice() {
        [grammar] => (*grammar, None),
        [grammar, output] => (*grammar, Some(*output)),
        _ => {
            return Err(
                "usage: grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids]"
                    .to_string(),
            );
        }
    };

    let (grammar, warnings) = crate::load_grammar(grammar_path)?;
    let project = fieldworks_project_for_path(grammar_path, fieldworks_project);
    let mut diagnostics = check_grammar_health_diagnostics(&grammar)
        .map_err(|error| format!("run grammar health checks: {error}"))?;
    diagnostics.extend(
        warnings
            .iter()
            .map(GrammarHealthDiagnostic::from_import_warning),
    );
    let report = GrammarHealthReport::new(diagnostics)
        .map_err(|error| format!("assemble grammar health report: {error}"))?
        .with_fieldworks_project(project);
    let json =
        render_json(&report).map_err(|e| format!("serialize grammar health diagnostics: {e}"))?;

    match out_path {
        Some(path) => {
            fs::write(path, &json).map_err(|e| format!("write {path}: {e}"))?;
        }
        None => println!("{json}"),
    }

    let log = render_log(&report, log_guids);
    if !log.is_empty() {
        eprintln!("{log}");
    }
    eprintln!(
        "grammar-health complete: {} diagnostic(s) ({})",
        report.len(),
        render_level_counts(report.diagnostics()),
    );
    let errors = count_level(report.diagnostics(), DiagnosticLevel::Error);
    if errors > 0 {
        return Err(format!("{errors} error(s); fix them before parsing"));
    }
    Ok(())
}

fn fieldworks_project_for_path(
    grammar_path: &str,
    fieldworks_project: Option<&str>,
) -> FieldWorksProject {
    if let Some(name) = fieldworks_project {
        return FieldWorksProject {
            name: Some(name.to_string()),
            source: Some(FieldWorksProjectSource::Argument),
        };
    }
    // Split on both separators: a Windows project path must name its project on every host.
    let file_name = grammar_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(grammar_path);
    let name = file_name
        .rsplit_once('.')
        .filter(|(stem, extension)| !stem.is_empty() && extension.eq_ignore_ascii_case("fwdata"))
        .map(|(stem, _)| stem);
    FieldWorksProject {
        name: name.map(str::to_string),
        source: name.map(|_| FieldWorksProjectSource::FwdataPath),
    }
}

/// The `E error(s), N warning(s), M info` fragment of `run_grammar_health`'s completion message.
fn render_level_counts(diagnostics: &[GrammarHealthDiagnostic]) -> String {
    format!(
        "{} error(s), {} warning(s), {} info",
        count_level(diagnostics, DiagnosticLevel::Error),
        count_level(diagnostics, DiagnosticLevel::Warning),
        count_level(diagnostics, DiagnosticLevel::Info),
    )
}

fn count_level(diagnostics: &[GrammarHealthDiagnostic], level: DiagnosticLevel) -> usize {
    diagnostics.iter().filter(|d| d.level == level).count()
}

#[cfg(test)]
mod tests;
