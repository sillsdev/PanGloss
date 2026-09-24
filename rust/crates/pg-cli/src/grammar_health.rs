//! `pangloss grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids]`: run
//! the ported `hc-*` HermitCrab grammar-authoring checks (`pg_grammar::grammar_health`) and
//! print/serialize the findings.
//!
//! Deliberately a SEPARATE command from `fst-health`, not a section added to it: the two answer
//! different questions (grammar authoring correctness vs. FST compilation/production readiness),
//! and `fst-health`'s JSON is a versioned wire shape (`pg_health::HEALTH_SCHEMA_VERSION`) read by
//! `pg-pack`/`pg-wasm` -- folding a second vocabulary into it would need a version bump for a
//! question those readers never asked. This command emits one versioned structured report and a
//! lossless plain-text log.

use std::fs;
use std::path::Path;

use pg_grammar::grammar_health::{
    check_grammar_health_findings, render_json, render_log, FieldWorksProject,
    FieldWorksProjectSource, GrammarHealthCheckFinding, GrammarHealthReport, GrammarHealthSeverity,
};

/// `pangloss grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids]`;
/// `<out.json>` omitted prints the versioned report to stdout instead of a file. Findings are
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
    let mut findings = check_grammar_health_findings(&grammar)
        .map_err(|error| format!("run grammar health checks: {error}"))?;
    findings.extend(
        warnings
            .iter()
            .map(GrammarHealthCheckFinding::from_import_warning),
    );
    let report = GrammarHealthReport::new(findings)
        .map_err(|error| format!("assemble grammar health report: {error}"))?
        .with_fieldworks_project(project);
    let json =
        render_json(&report).map_err(|e| format!("serialize grammar health findings: {e}"))?;

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
        "grammar-health complete: {} finding(s) ({})",
        report.len(),
        render_severity_counts(report.findings()),
    );
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
    let path = Path::new(grammar_path);
    let is_fwdata = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("fwdata"));
    let name = is_fwdata
        .then(|| path.file_stem().and_then(|stem| stem.to_str()))
        .flatten();
    FieldWorksProject {
        name: name.map(str::to_string),
        source: name.map(|_| FieldWorksProjectSource::FwdataPath),
    }
}

/// The `N error(s), M warning(s)` fragment of `run_grammar_health`'s completion message.
fn render_severity_counts(findings: &[GrammarHealthCheckFinding]) -> String {
    let errors = findings
        .iter()
        .filter(|f| f.severity == GrammarHealthSeverity::Error)
        .count();
    let warnings = findings.len() - errors;
    format!("{errors} error(s), {warnings} warning(s)")
}

#[cfg(test)]
mod tests;
