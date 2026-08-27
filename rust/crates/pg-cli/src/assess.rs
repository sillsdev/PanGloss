//! `pangloss compare | golden-diff | investigate`: the assessment evidence layer, reading caller-owned artifacts under typed exit codes rather than a bare zero-or-one.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use pg_assess::{
    compare, golden_diff, investigate, parse_report, parse_suite, AssessmentReport, HandoffRequest,
};

/// Typed process outcomes.
pub const EXIT_OK: u8 = 0;
pub const EXIT_INVALID_INPUT: u8 = 2;
pub const EXIT_UNSUPPORTED: u8 = 3;
pub const EXIT_INTERNAL: u8 = 70;

#[derive(Debug)]
pub struct CliError {
    pub code: u8,
    pub message: String,
}

impl CliError {
    fn invalid(message: impl Into<String>) -> Self {
        CliError {
            code: EXIT_INVALID_INPUT,
            message: message.into(),
        }
    }
    fn unsupported(message: impl Into<String>) -> Self {
        CliError {
            code: EXIT_UNSUPPORTED,
            message: message.into(),
        }
    }
    fn internal(message: impl Into<String>) -> Self {
        CliError {
            code: EXIT_INTERNAL,
            message: message.into(),
        }
    }
}

pub fn exit(result: Result<(), CliError>, command: &str) -> ExitCode {
    match result {
        Ok(()) => ExitCode::from(EXIT_OK),
        Err(e) => {
            eprintln!("pangloss {command}: {e_message}", e_message = e.message);
            ExitCode::from(e.code)
        }
    }
}

/// Minimal flag parsing, matching this binary's existing hand-rolled convention.
struct Args {
    positional: Vec<String>,
    flags: BTreeMap<String, String>,
}

fn parse_args(args: &[String]) -> Result<Args, CliError> {
    let mut positional = Vec::new();
    let mut flags = BTreeMap::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if let Some(name) = arg.strip_prefix("--") {
            let (name, value) = match name.split_once('=') {
                Some((n, v)) => (n.to_string(), v.to_string()),
                None => {
                    i += 1;
                    let value = args
                        .get(i)
                        .ok_or_else(|| CliError::invalid(format!("--{name} needs a value")))?;
                    (name.to_string(), value.clone())
                }
            };
            flags.insert(name, value);
        } else {
            positional.push(arg.clone());
        }
        i += 1;
    }
    Ok(Args { positional, flags })
}

impl Args {
    fn required_positional(&self, index: usize, name: &str) -> Result<&str, CliError> {
        self.positional
            .get(index)
            .map(String::as_str)
            .ok_or_else(|| CliError::invalid(format!("missing <{name}>")))
    }
    fn flag(&self, name: &str) -> Option<&str> {
        self.flags.get(name).map(String::as_str)
    }

}

/// Writes to `--report <path>` if given, else stdout; overwrites freely since the caller owns storage.
fn emit(args: &Args, value: &serde_json::Value) -> Result<(), CliError> {
    let json = pg_assess::canonicalize(value)
        .map_err(|e| CliError::internal(format!("canonicalize artifact: {e}")))?;
    match args.flag("report") {
        None => {
            println!("{json}");
            Ok(())
        }
        Some(path) => write_atomically(Path::new(path), json.as_bytes()),
    }
}

/// Writes via a same-directory temp file plus rename, so a crash leaves either no destination or one complete artifact, never a truncated one; the temp file must be a sibling because rename is atomic only within one filesystem.
fn write_atomically(destination: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let directory = match destination.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let file_name = destination.file_name().ok_or_else(|| {
        CliError::invalid(format!("--report {} names no file", destination.display()))
    })?;
    // The pid keeps two concurrent runs from sharing a temp file; each still-atomic rename publishes one complete artifact.
    let temporary = directory.join(format!(
        ".{}.{}.tmp",
        file_name.to_string_lossy(),
        std::process::id()
    ));

    let write = || -> std::io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        // Flushed before the rename so a crash right after cannot leave the destination pointing at unsynced content.
        file.sync_all()
    };
    if let Err(e) = write() {
        let _ = fs::remove_file(&temporary);
        return Err(CliError::invalid(format!(
            "write {}: {e}",
            temporary.display()
        )));
    }

    fs::rename(&temporary, destination).map_err(|e| {
        let _ = fs::remove_file(&temporary);
        CliError::invalid(format!("publish {}: {e}", destination.display()))
    })
}

fn read(path: &str) -> Result<String, CliError> {
    std::fs::read_to_string(path).map_err(|e| CliError::invalid(format!("read {path}: {e}")))
}

pub fn run_compare(args: &[String]) -> Result<(), CliError> {
    let args = parse_args(args)?;
    let baseline = load_report(args.required_positional(0, "baseline.json")?)?;
    let candidate = load_report(args.required_positional(1, "candidate.json")?)?;
    let delta =
        compare(&baseline, &candidate).map_err(|e| CliError::internal(format!("compare: {e}")))?;
    // Exit 0 even when every case is `not_comparable`: a typed refusal is still evidence.
    emit(&args, &delta.to_value())
}

fn load_report(path: &str) -> Result<AssessmentReport, CliError> {
    let document = read(path)?;
    parse_report(&document).map_err(|e| {
        // A report from another identity profile is an unsupported capability, not malformed input.
        let profile_mismatch = matches!(e, pg_assess::ReportError::ForeignIdentityProfile(_));
        let message = format!("report {path}: {e}");
        if profile_mismatch {
            CliError::unsupported(message)
        } else {
            CliError::invalid(message)
        }
    })
}

pub fn run_golden_diff(args: &[String]) -> Result<(), CliError> {
    let args = parse_args(args)?;
    let report = load_report(args.required_positional(0, "report.json")?)?;
    let suite_path = args
        .flag("suite")
        .ok_or_else(|| CliError::invalid("missing --suite <path>"))?;
    let suite = parse_suite(&read(suite_path)?)
        .map_err(|e| CliError::invalid(format!("suite {suite_path}: {e}")))?;

    let diff = golden_diff(&report, &suite).map_err(|e| CliError::invalid(e.to_string()))?;
    emit(&args, &diff.to_value())
}

pub fn run_investigate(args: &[String]) -> Result<(), CliError> {
    let args = parse_args(args)?;
    let report = load_report(args.required_positional(0, "report.json")?)?;
    let case_id = args
        .flag("case")
        .ok_or_else(|| CliError::invalid("missing --case <caseId>"))?;

    let request = HandoffRequest {
        case_id: case_id.to_string(),
        ..HandoffRequest::default()
    };

    let handoff = investigate(&report, &request).map_err(|e| CliError::invalid(e.to_string()))?;
    emit(&args, &handoff.to_value())
}
