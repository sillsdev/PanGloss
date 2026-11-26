//! `pangloss grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids]
//! [--stored-analyses bounded|all|off] [--fst] [--shadowed [--include-selectable]]`: run the ported `hc-*` HermitCrab grammar-authoring checks
//! (`pg_grammar::grammar_health`) and print/serialize the diagnostics.
//!
//! `--shadowed` adds the shadowed-alternates report of `pangloss serve`'s `shadowedAllomorphs` op
//! (`serve_shadowed::report`, the same code): a `grammar.allomorph.shadowed` warning per proven row,
//! and the whole report under the JSON key `shadowed_alternates` with the serve op's rows,
//! verdicts and `removableGuids`. `--include-selectable` also lists the `selectable` rows.
//!
//! The stored-analysis comparison also writes one row per compared wordform under the top-level
//! key `stored_analysis_words` (outcome `same`, `extra`, `missing`, `missing_and_extra` or
//! `incomplete`, with counts), and one `grammar.stored-analysis.extra-reading` finding per parsed
//! Reading no approved analysis matches.
//!
//! The checks that do not parse always run to completion. The FieldWorks stored-analysis
//! comparison parses wordforms, so by default it stops at a fixed total work budget; a wordform it
//! cannot finish, and the wordforms past the budget, are reported as findings, never dropped.
//!
//! `--snapshot-out <p>` and `--facts-out <p> --facts-context <p> [--facts-light]` (a `.fwdata` or
//! `.fwbackup` project only) make the same single import also write the files `pangloss import`
//! and `pangloss facts <that snapshot>` would, so a host that wants all three pays for one import
//! and one health compile. The snapshot is `import`'s exact bytes and the facts database is built
//! from those exact bytes, so its `source_sha256` is the snapshot file's hash.
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
    check_grammar_health_diagnostics, render_json, render_log, FieldWorksProject,
    FieldWorksProjectSource, GrammarHealthDiagnostic, GrammarHealthReport,
};
use pg_snapshot::DiagnosticLevel;

use crate::stored_analysis_health::{Coverage, Limits};

/// `pangloss grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids]
/// [--stored-analyses bounded|all|off] [--fst] [--shadowed [--include-selectable]]`; `<out.json>` omitted prints the versioned report to stdout
/// instead of a file. Diagnostics are also logged one per line on stderr.
pub fn run_grammar_health(args: &[String]) -> Result<(), String> {
    run_grammar_health_with_limits(args, Limits::DEFAULT)
}

/// `run_grammar_health` with the stored-analysis comparison's work limits.
pub(crate) fn run_grammar_health_with_limits(
    args: &[String],
    limits: Limits,
) -> Result<(), String> {
    #[cfg(feature = "foma-tools")]
    let mut fst_config = pg_foma::construction::FstCompileConfig::default();
    let mut fst_report: Option<serde_json::Value> = None;
    let mut positionals = Vec::new();
    let mut fieldworks_project = None;
    let mut log_guids = false;
    let mut fst = false;
    let mut shadowed = false;
    let mut include_selectable = false;
    let mut shadowed_report: Option<serde_json::Map<String, serde_json::Value>> = None;
    let mut stored_words = Vec::new();
    let mut fst_check_error = None;
    let mut coverage = Coverage::Budget(limits.work_budget);
    let mut artifacts = ArtifactArgs::default();
    let mut index = 0;
    while index < args.len() {
        #[cfg(feature = "foma-tools")]
        if fst_config.parse_option(args, &mut index)? {
            fst = true;
            continue;
        }
        match args[index].as_str() {
            "--fst" => {
                if !cfg!(feature = "foma-tools") {
                    return Err("--fst requires a build with the foma-tools feature".to_string());
                }
                fst = true;
                index += 1;
            }
            "--shadowed" => {
                shadowed = true;
                index += 1;
            }
            "--include-selectable" => {
                include_selectable = true;
                index += 1;
            }
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
            "--snapshot-out" => {
                artifacts.snapshot_out = Some(path_value(args, index, "--snapshot-out")?);
                index += 2;
            }
            "--facts-out" => {
                artifacts.facts_out = Some(path_value(args, index, "--facts-out")?);
                index += 2;
            }
            "--facts-context" => {
                artifacts.facts_context = Some(path_value(args, index, "--facts-context")?);
                index += 2;
            }
            "--facts-light" => {
                artifacts.facts_light = true;
                index += 1;
            }
            "--stored-analyses" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--stored-analyses requires bounded, all or off".to_string())?;
                coverage = Coverage::parse(value, limits)?;
                index += 2;
            }
            arg if arg.starts_with("--stored-analyses=") => {
                coverage = Coverage::parse(arg.trim_start_matches("--stored-analyses="), limits)?;
                index += 1;
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
                "usage: grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids] [--stored-analyses bounded|all|off] [--fst] [--shadowed [--include-selectable]] [--snapshot-out <snapshot.json>] [--facts-out <facts.sqlite> --facts-context <context.json> [--facts-light]]"
                    .to_string(),
            );
        }
    };

    if include_selectable && !shadowed {
        return Err("--include-selectable requires --shadowed".to_string());
    }
    artifacts.validate(grammar_path, out_path)?;
    let mut capture = crate::ImportCapture {
        keep_compile: artifacts.facts_out.is_some(),
        ..Default::default()
    };
    let project = fieldworks_project_for_path(grammar_path, fieldworks_project);
    let loaded = crate::load_grammar_capturing(
        grammar_path,
        false,
        crate::StatsIdentityRequest::None,
        artifacts.requested().then_some(&mut capture),
    );
    // Written even when the compile fails.
    let artifact_error;
    let diagnostics = match loaded {
        Ok(loaded) => {
            artifact_error = artifacts.write(&capture, Some(&loaded.grammar));
            crate::print_substrate_report(&loaded.substrate);
            #[cfg(feature = "foma-tools")]
            if fst {
                fst_report = Some(crate::fst_health::compile_report(
                    &loaded.grammar,
                    &fst_config,
                )?);
            }
            let checks = Checks {
                shadowed,
                include_selectable,
                coverage,
                work_cap: limits.work_cap,
            };
            let run = run_checks(
                &loaded.grammar,
                &loaded.stored_analyses,
                &loaded.warnings,
                checks,
                |diagnostics| {
                    if fst {
                        fst_check_error = extend_fst_diagnostics(&loaded.grammar, diagnostics);
                    }
                },
            )?;
            shadowed_report = run.shadowed_report;
            stored_words = run.stored_words;
            run.diagnostics
        }
        Err(crate::compile_failure::GrammarLoadError::Compile(failure)) => {
            artifact_error = artifacts.write(&capture, None);
            if fst {
                fst_check_error = Some(
                    "FST inspection requires a converted Grammar; grammar compilation failed."
                        .to_string(),
                );
            }
            failure.diagnostics
        }
        Err(error) => {
            artifacts.write(&capture, None);
            return Err(error.to_string());
        }
    };
    let (report, json) = render_report(
        diagnostics,
        shadowed_report,
        stored_words,
        project,
        FstSection {
            requested: fst,
            report: fst_report.take(),
            check_error: fst_check_error.clone(),
        },
    )?;

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
    eprintln!(
        "FST: blocked by {} labelled finding(s); capability inspection {}",
        report.fst_blocking_count(),
        if report.fst_checked() {
            "complete"
        } else {
            "not complete (see fst_check_error; use --fst when not requested)"
        }
    );
    if let Some(reason) = fst_check_error {
        return Err(format!(
            "FST grammar-health inspection incomplete: {reason}"
        ));
    }
    if let Some(error) = artifact_error {
        return Err(error);
    }
    let errors = count_level(report.diagnostics(), DiagnosticLevel::Error);
    if errors > 0 {
        return Err(format!("{errors} error(s); fix them before parsing"));
    }
    Ok(())
}

/// Which of the checks that need no extra input run, shared by `grammar-health` and `serve`'s `health` op.
#[derive(Clone, Copy)]
pub(crate) struct Checks {
    pub(crate) shadowed: bool,
    pub(crate) include_selectable: bool,
    pub(crate) coverage: Coverage,
    pub(crate) work_cap: usize,
}

/// What the checks found, before it is rendered.
pub(crate) struct CheckRun {
    pub(crate) diagnostics: Vec<GrammarHealthDiagnostic>,
    pub(crate) shadowed_report: Option<serde_json::Map<String, serde_json::Value>>,
    pub(crate) stored_words: Vec<crate::stored_analysis_health::WordRow>,
}

/// Runs the grammar checks on a compiled grammar; `extend` adds the FST findings after the base checks, where the command puts them.
pub(crate) fn run_checks(
    grammar: &pg_grammar::model::Grammar,
    stored_analyses: &[pg_fwdata::StoredAnalysis],
    warnings: &[pg_snapshot::Warning],
    checks: Checks,
    extend: impl FnOnce(&mut Vec<GrammarHealthDiagnostic>),
) -> Result<CheckRun, String> {
    let mut diagnostics = check_grammar_health_diagnostics(grammar)
        .map_err(|error| format!("run grammar health checks: {error}"))?;
    extend(&mut diagnostics);
    let mut shadowed_report = None;
    if checks.shadowed {
        let report = crate::serve_shadowed::report(grammar, checks.include_selectable);
        diagnostics.extend(crate::serve_shadowed::findings(grammar, &report));
        shadowed_report = Some(report);
    }
    let compared = crate::stored_analysis_health::check(
        grammar,
        stored_analyses,
        checks.work_cap,
        checks.coverage,
    );
    diagnostics.extend(compared.diagnostics);
    diagnostics.extend(
        warnings
            .iter()
            .map(GrammarHealthDiagnostic::from_import_warning),
    );
    Ok(CheckRun {
        diagnostics,
        shadowed_report,
        stored_words: compared.words,
    })
}

/// The `--fst` part of a report: whether it was asked for, its compile report, and why it could not finish.
pub(crate) struct FstSection {
    pub(crate) requested: bool,
    pub(crate) report: Option<serde_json::Value>,
    pub(crate) check_error: Option<String>,
}

/// Builds the versioned report and its JSON text, the one place both callers do it.
pub(crate) fn render_report(
    diagnostics: Vec<GrammarHealthDiagnostic>,
    shadowed_report: Option<serde_json::Map<String, serde_json::Value>>,
    stored_words: Vec<crate::stored_analysis_health::WordRow>,
    project: FieldWorksProject,
    fst: FstSection,
) -> Result<(GrammarHealthReport, String), String> {
    let mut report = GrammarHealthReport::new(diagnostics)
        .map_err(|error| format!("assemble grammar health report: {error}"))?
        .with_fieldworks_project(project);
    if let Some(reason) = &fst.check_error {
        report = report.with_fst_check_error(reason.clone());
    } else if fst.requested {
        report = report.with_fst_checked();
    }
    let mut json =
        render_json(&report).map_err(|e| format!("serialize grammar health diagnostics: {e}"))?;
    if fst.report.is_some() || shadowed_report.is_some() || !stored_words.is_empty() {
        let mut value: serde_json::Value =
            serde_json::from_str(&json).map_err(|error| error.to_string())?;
        if let Some(fst) = fst.report {
            value["fst"] = fst;
        }
        if !stored_words.is_empty() {
            value["stored_analysis_words"] =
                serde_json::to_value(&stored_words).map_err(|error| error.to_string())?;
        }
        if let Some(shadowed) = shadowed_report {
            value["shadowed_alternates"] = serde_json::Value::Object(shadowed);
        }
        json = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
    }
    Ok((report, json))
}

/// The extra outputs of one grammar-health run: `import`'s snapshot and `facts`' database.
#[derive(Default)]
struct ArtifactArgs<'a> {
    snapshot_out: Option<&'a str>,
    facts_out: Option<&'a str>,
    facts_context: Option<&'a str>,
    facts_light: bool,
}

fn path_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    match args.get(index + 1) {
        Some(value) if !value.starts_with("--") && !value.trim().is_empty() => Ok(value),
        _ => Err(format!("{flag} requires a path")),
    }
}

impl ArtifactArgs<'_> {
    fn requested(&self) -> bool {
        self.snapshot_out.is_some() || self.facts_out.is_some()
    }

    fn validate(&self, grammar_path: &str, out_path: Option<&str>) -> Result<(), String> {
        if self.facts_out.is_some() != self.facts_context.is_some() {
            return Err("--facts-out and --facts-context must be supplied together".to_string());
        }
        if self.facts_light && self.facts_out.is_none() {
            return Err("--facts-light requires --facts-out".to_string());
        }
        if !self.requested() {
            return Ok(());
        }
        let extension = Path::new(grammar_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("");
        if !(extension.eq_ignore_ascii_case("fwdata") || extension.eq_ignore_ascii_case("fwbackup"))
        {
            return Err(
                "--snapshot-out and --facts-out need a .fwdata or .fwbackup project: they write what `import` and `facts` would write from one"
                    .to_string(),
            );
        }
        let paths = [
            Some(grammar_path),
            out_path,
            self.snapshot_out,
            self.facts_out,
            self.facts_context,
        ];
        for (left_index, left) in paths.iter().enumerate() {
            for right in paths.iter().skip(left_index + 1) {
                if let (Some(left), Some(right)) = (left, right) {
                    if crate::facts_cmd::same_path(Path::new(left), Path::new(right)) {
                        return Err(format!(
                            "input, report, snapshot, facts and context paths must differ: {left} and {right} are the same file"
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Writes the snapshot and facts the one import made possible; returns the error to report last.
    fn write(
        &self,
        capture: &crate::ImportCapture,
        grammar: Option<&pg_grammar::model::Grammar>,
    ) -> Option<String> {
        if !self.requested() {
            return None;
        }
        if capture.import_refused {
            return Some(
                "import refused (fatal conversion issues): snapshot and facts not written, as `import` would not write them"
                    .to_string(),
            );
        }
        let Some(snapshot) = capture.snapshot_json.as_deref() else {
            return Some("no snapshot was produced; snapshot and facts not written".to_string());
        };
        if let Some(path) = self.snapshot_out {
            if let Err(error) = fs::write(path, snapshot) {
                return Some(format!("write {path}: {error}"));
            }
        }
        let (Some(out), Some(context)) = (self.facts_out, self.facts_context) else {
            return None;
        };
        let context = match fs::read(context) {
            Ok(bytes) => bytes,
            Err(error) => return Some(format!("read {context}: {error}")),
        };
        let options = pg_facts::FactsOptions {
            light: self.facts_light,
            stats: None,
        };
        let producer = crate::facts_cmd::producer_identity();
        // The compile this run already did stands in for the one `facts` would repeat.
        let reused = capture.compiled.is_some() && grammar.is_some();
        let written = match (capture.compiled.as_ref(), grammar) {
            (Some(compiled), Some(grammar)) => pg_facts::write_facts_with_compiled(
                snapshot.as_bytes(),
                &context,
                out,
                producer,
                options,
                (compiled, grammar),
            ),
            _ => pg_facts::write_facts_with_options(
                snapshot.as_bytes(),
                &context,
                out,
                producer,
                options,
            ),
        };
        match written {
            Ok(result) => {
                eprintln!(
                    "facts written: {out} ({} bytes, {}, schema {}, compile {}{})",
                    result.output_bytes,
                    result.output_sha256,
                    result.schema_version,
                    result.compile_status,
                    if reused { ", reused from this run" } else { "" }
                );
                None
            }
            Err(error) => Some(format!("facts {}: {error}", error.code())),
        }
    }
}

#[cfg(feature = "foma-tools")]
fn extend_fst_diagnostics(
    grammar: &pg_grammar::model::Grammar,
    diagnostics: &mut Vec<GrammarHealthDiagnostic>,
) -> Option<String> {
    match pg_foma::grammar_health::extend_diagnostics(grammar, diagnostics) {
        Ok(reason) => reason,
        Err(error) => Some(error.to_string()),
    }
}

#[cfg(not(feature = "foma-tools"))]
fn extend_fst_diagnostics(
    _grammar: &pg_grammar::model::Grammar,
    _diagnostics: &mut Vec<GrammarHealthDiagnostic>,
) -> Option<String> {
    Some("FST inspection requires the foma-tools feature.".to_string())
}

pub(crate) fn fieldworks_project_for_path(
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
