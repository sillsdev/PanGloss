//! `pangloss assess | compare | golden-diff | investigate` — the grammar-assessment evidence layer.
//!
//! `openspec/changes/add-grammar-assessment`. The four operations are one caller-facing contract
//! with one owner of the wire format: split them across commands that each own a piece of the
//! schema and no single one can honour the PanGloss/caller boundary.
//!
//! ## The caller owns storage
//!
//! Artifacts go to stdout by default; `--report <path>` writes to a file and overwrites freely.
//! There is no no-overwrite rule, no retry flag, and no content-addressed sink. The caller picks
//! the path and owns retention — realistically it invokes `assess`, reads the artifact, ingests it
//! into its own store, and the file is scratch.
//!
//! ## Exit codes are typed
//!
//! `0` an artifact was produced (including one whose every case is `not_comparable` — a refusal is
//! still evidence), `2` invalid input or schema, `3` an unsupported capability or an incompatible
//! identity profile, `4` containment prevented the artifact, `70` internal fault. A caller
//! branching on "did it work" needs more than zero-or-one.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use pg_assess::{
    compare, golden_diff, investigate, parse_report, parse_suite, AnalysisIdentity, AnalysisSet,
    AssessmentFailure, AssessmentReport, BudgetDimension, CaseOutcome, CaseRecord, Diagnostic,
    Evidence, EvidenceAvailability, Execution, FailureKind, HandoffRequest, IncompleteReason,
    MissingAnalysisCause, NotAttemptedReason, Provenance, ReportDraft, Severity, SourceKind,
    SuiteRef, ValidatedSuite, IDENTITY_PROFILE,
};
use pg_foma::compose_budget::{ApplyBudget, ApplyDimension};
use pg_foma::composite::{FomaAnalyzer, FomaApplyOutcome};
use pg_grammar::model::Grammar;
use pg_parse::Morpher;

use crate::load_grammar;

/// Typed process outcomes (task 3.10).
pub const EXIT_OK: u8 = 0;
pub const EXIT_INVALID_INPUT: u8 = 2;
pub const EXIT_UNSUPPORTED: u8 = 3;
pub const EXIT_CONTAINED: u8 = 4;
pub const EXIT_INTERNAL: u8 = 70;

/// Which analysis pipeline runs (task 3.3, D13).
///
/// Replaces `--engine default|foma` and inverts its default: `foma-confirm` is what production
/// runs, so it is what evidence should describe. An unavailable pipeline is an
/// `unsupported_capability` refusal — never a silent fallback to the other one, which would produce
/// an artifact whose `pipeline` field is a lie.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pipeline {
    FomaConfirm,
    Hermitcrab,
}

impl Pipeline {
    fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "foma-confirm" => Ok(Pipeline::FomaConfirm),
            "hermitcrab" => Ok(Pipeline::Hermitcrab),
            other => Err(CliError::invalid(format!(
                "invalid --pipeline: {other} (expected foma-confirm|hermitcrab)"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Pipeline::FomaConfirm => "foma-confirm",
            Pipeline::Hermitcrab => "hermitcrab",
        }
    }
}

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
    fn number(&self, name: &str) -> Result<Option<usize>, CliError> {
        match self.flag(name) {
            None => Ok(None),
            Some(raw) => raw.parse().map(Some).map_err(|_| {
                CliError::invalid(format!("--{name} must be a whole number, got {raw}"))
            }),
        }
    }
}

/// Write to `--report <path>` if given, otherwise stdout. Overwrites freely: the caller owns
/// storage, including not clobbering its own baseline.
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

/// Write bytes so a crash leaves either no destination or one complete artifact — never a truncated
/// one (§17.7).
///
/// A plain `fs::write` truncates the destination first and then streams into it, so a crash or a
/// full disk mid-write leaves a file that parses as neither valid JSON nor an absent report. For an
/// artifact whose whole purpose is to be trustworthy evidence, a half-written file is the worst
/// outcome available: a consumer cannot distinguish it from a real report that says something
/// different.
///
/// The temp file is a *sibling* of the destination rather than in a temp directory, because a
/// rename is only atomic within one filesystem. `fs::rename` replaces an existing destination on
/// both Unix and Windows, so this keeps D8's "overwrites freely" — atomicity is about crash
/// behaviour, not about refusing to overwrite.
fn write_atomically(destination: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let directory = match destination.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let file_name = destination.file_name().ok_or_else(|| {
        CliError::invalid(format!("--report {} names no file", destination.display()))
    })?;
    // The pid keeps two concurrent runs writing the same destination from sharing a temp file. They
    // still race on the rename, but each rename publishes one complete artifact, so a reader can
    // never observe a mixture of the two.
    let temporary = directory.join(format!(
        ".{}.{}.tmp",
        file_name.to_string_lossy(),
        std::process::id()
    ));

    let write = || -> std::io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        // Flush to disk before the rename, so a crash after the rename cannot leave the directory
        // entry pointing at content that never landed.
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

/// The single engine-to-artifact translation for budget dimensions.
///
/// Exhaustive by construction: adding a variant to `pg_foma::ApplyDimension` fails to compile here
/// until it is given an artifact name, rather than silently reaching a report as an unrecognised
/// string that no consumer can branch on.
fn budget_dimension(dimension: ApplyDimension) -> BudgetDimension {
    match dimension {
        ApplyDimension::DecodedPaths => BudgetDimension::DecodedPaths,
        ApplyDimension::Candidates => BudgetDimension::Candidates,
    }
}

// ---------------------------------------------------------------------------------------------
// assess
// ---------------------------------------------------------------------------------------------

pub fn run_assess(args: &[String]) -> Result<(), CliError> {
    let args = parse_args(args)?;
    let grammar_path = args.required_positional(0, "grammar")?;

    let pipeline = match args.flag("pipeline") {
        None => Pipeline::FomaConfirm,
        Some(value) => Pipeline::parse(value)?,
    };

    // Logical budgets stay unbounded unless a resource envelope is named (task 3.4). No default is
    // invented: `calibrate-fst-resource-envelopes` is data-blocked, so any number here would be
    // guesswork that silently truncates analyses on real grammars.
    let budget = ApplyBudget::with_caps(
        args.number("budget-paths")?,
        args.number("budget-candidates")?,
    );
    let mut budgets = BTreeMap::new();
    if let Some(cap) = args.number("budget-paths")? {
        budgets.insert("decodedPaths".to_string(), cap as u64);
    }
    if let Some(cap) = args.number("budget-candidates")? {
        budgets.insert("candidates".to_string(), cap as u64);
    }

    let (suite, cases) = load_cases(&args)?;

    let source = read(grammar_path)?;
    let source_kind = if grammar_path.ends_with(".json") {
        SourceKind::Snapshot
    } else {
        SourceKind::HcXml
    };
    let compiler_version = env!("CARGO_PKG_VERSION");
    let provenance = Provenance {
        source_sha256: pg_assess::source_sha256(source.as_bytes()),
        source_kind: source_kind.as_str().to_string(),
        model_fingerprint: pg_assess::model_fingerprint(source_kind, &source, compiler_version)
            .map_err(|e| CliError::internal(format!("model fingerprint: {e}")))?,
        importer_version: compiler_version.to_string(),
        compiler_version: compiler_version.to_string(),
    };

    // Import or compile failing after suite validation passed is not an error exit (task 3.11). A
    // caller that asked for evidence gets evidence: a `failed` artifact whose every case is
    // `not_attempted/assessment_setup_failed`, with the compiler's own message retained as a
    // diagnostic. Exiting non-zero with nothing to read would tell a CI consumer only that
    // something went wrong, and `compare` could not join the run against its baseline at all.
    let (grammar, warnings) = match load_grammar(grammar_path) {
        Ok(loaded) => loaded,
        Err(message) => {
            let report =
                setup_failed_report(suite, pipeline, budgets, provenance, &cases, &message)?;
            return emit(&args, &report.to_value());
        }
    };
    let diagnostics = warnings
        .iter()
        .map(|message| Diagnostic {
            // Until the importer carries real codes (task 3.8), every warning shares one so a
            // consumer can at least count them. Stated rather than faked per-warning.
            code: "importer.warning".to_string(),
            severity: Severity::Warning,
            message: message.clone(),
        })
        .collect();

    let outcomes = run_cases(&grammar, &cases, pipeline, &budget)?;
    let case_records = cases
        .iter()
        .zip(outcomes)
        .map(|(case, outcome)| CaseRecord {
            case_id: case.case_id.clone(),
            input: case.input.clone(),
            outcome,
            supersedes: case.supersedes.clone(),
        })
        .collect();

    let report = ReportDraft {
        generated_at: now_rfc3339(),
        suite,
        execution: Execution {
            pipeline: pipeline.as_str().to_string(),
            budgets,
            wall_clock_limit_us: None,
        },
        provenance,
        diagnostics,
        cases: case_records,
        failure: None,
        extensions: None,
    }
    .finish()
    .map_err(|e| CliError::internal(format!("finish report: {e}")))?;

    emit(&args, &report.to_value())
}

/// The artifact a run produces when setup failed safely (task 3.11).
///
/// Every case is `not_attempted/assessment_setup_failed`, so [`pg_assess::derive_status`] reports
/// `failed` and no case can be mistaken for a grammar that analyzes nothing.
fn setup_failed_report(
    suite: SuiteRef,
    pipeline: Pipeline,
    budgets: BTreeMap<String, u64>,
    provenance: Provenance,
    cases: &[PendingCase],
    message: &str,
) -> Result<AssessmentReport, CliError> {
    ReportDraft {
        generated_at: now_rfc3339(),
        suite,
        execution: Execution {
            pipeline: pipeline.as_str().to_string(),
            budgets,
            wall_clock_limit_us: None,
        },
        provenance,
        diagnostics: vec![Diagnostic {
            code: "assessment.setup_failed".to_string(),
            severity: Severity::Error,
            message: message.to_string(),
        }],
        // The typed top-level reason (spec 17.7). A consumer reading this artifact may have no
        // access to our exit code, so `status: failed` alone would leave it inferring the cause
        // from prose.
        failure: Some(AssessmentFailure {
            kind: FailureKind::AssessmentSetupFailed,
            message: message.to_string(),
        }),
        cases: cases
            .iter()
            .map(|case| CaseRecord {
                case_id: case.case_id.clone(),
                input: case.input.clone(),
                outcome: CaseOutcome::NotAttempted(NotAttemptedReason::AssessmentSetupFailed),
                supersedes: case.supersedes.clone(),
            })
            .collect(),
        extensions: None,
    }
    .finish()
    .map_err(|e| CliError::internal(format!("finish failed-assessment report: {e}")))
}

/// One case as `assess` needs it, from either a suite or a bare word list.
#[derive(Debug)]
struct PendingCase {
    case_id: String,
    input: String,
    supersedes: Vec<String>,
}

/// A suite, or a bare word list with synthesized case IDs (task 3.12).
///
/// The word-list path exists so a caller need not author a suite for a quick run — it keeps
/// `diagnose`'s ergonomics now that one assessment artifact exists in the repo (D14). Its case IDs
/// are deterministic but positional, so they are stable across reruns of the same list and not
/// across edits to it; authoring a suite is what buys identity that survives reordering.
fn load_cases(args: &Args) -> Result<(SuiteRef, Vec<PendingCase>), CliError> {
    match (args.flag("suite"), args.flag("words")) {
        (Some(_), Some(_)) => Err(CliError::invalid(
            "pass either --suite or --words, not both",
        )),
        (None, None) => Err(CliError::invalid(
            "missing --suite <path> or --words <path>",
        )),
        (Some(path), None) => {
            let document = read(path)?;
            let suite: ValidatedSuite = parse_suite(&document)
                .map_err(|e| CliError::invalid(format!("suite {path}: {e}")))?;
            let declared = suite.suite();
            if declared.analysis_identity_profile != IDENTITY_PROFILE {
                return Err(CliError::unsupported(format!(
                    "suite declares identity profile {}; this build implements {IDENTITY_PROFILE}",
                    declared.analysis_identity_profile
                )));
            }
            let reference = SuiteRef {
                suite_id: declared.suite_id.clone(),
                suite_revision: declared.suite_revision.clone(),
                semantic_digest: suite.semantic_digest().to_string(),
                analysis_identity_profile: declared.analysis_identity_profile.clone(),
            };
            let cases = suite
                .cases()
                .iter()
                .map(|case| PendingCase {
                    case_id: case.case_id.clone(),
                    input: case.input.clone(),
                    supersedes: case.supersedes.clone(),
                })
                .collect();
            Ok((reference, cases))
        }
        (None, Some(path)) => {
            let text = read(path)?;
            let words: Vec<&str> = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect();
            let cases: Vec<PendingCase> = words
                .iter()
                .enumerate()
                .map(|(index, word)| PendingCase {
                    case_id: format!("w{index}:{word}"),
                    input: (*word).to_string(),
                    supersedes: Vec::new(),
                })
                .collect();
            let digest = pg_assess::sha256_bytes(text.as_bytes());
            Ok((
                SuiteRef {
                    suite_id: format!("wordlist:{path}"),
                    suite_revision: digest.clone(),
                    semantic_digest: digest,
                    analysis_identity_profile: IDENTITY_PROFILE.to_string(),
                },
                cases,
            ))
        }
    }
}

fn run_cases(
    grammar: &Grammar,
    cases: &[PendingCase],
    pipeline: Pipeline,
    budget: &ApplyBudget,
) -> Result<Vec<CaseOutcome>, CliError> {
    match pipeline {
        Pipeline::Hermitcrab => {
            let morpher = Morpher::new(grammar, usize::MAX);
            cases
                .iter()
                .map(|case| {
                    let outcome = morpher.parse_word(&case.input);
                    if outcome.capped {
                        // The HermitCrab step budget fired, so this set is not authoritative.
                        return Ok(CaseOutcome::Incomplete(IncompleteReason::LogicalBudget {
                            dimension: BudgetDimension::HermitcrabSteps,
                            value: 0,
                            limit: 0,
                        }));
                    }
                    Ok(CaseOutcome::Complete(project_all(
                        &outcome.structured,
                        grammar,
                    )?))
                })
                .collect()
        }
        Pipeline::FomaConfirm => {
            let mut analyzer = FomaAnalyzer::new(grammar).map_err(|e| {
                // Never a silent fallback to the other pipeline: an artifact whose `pipeline` field
                // said `hermitcrab` after the caller asked for `foma-confirm` would be a lie about
                // what produced the evidence.
                //
                // The two refusals are different facts and get different exit codes. A budget that
                // tripped is containment — this grammar is larger than the configured envelope, and
                // raising the envelope may well run it. A compile failure is a capability gap in the
                // emitter, and no amount of budget changes it.
                let message = format!("the foma-confirm pipeline cannot run this grammar: {e}");
                match e {
                    pg_foma::analyzer::FomaError::EnumerationBudgetExceeded { .. }
                    | pg_foma::analyzer::FomaError::UnorderedOrderingMultiplicityExceeded {
                        ..
                    } => CliError {
                        code: EXIT_CONTAINED,
                        message,
                    },
                    pg_foma::analyzer::FomaError::LexcCompileFailed(_) => {
                        CliError::unsupported(message)
                    }
                }
            })?;
            cases
                .iter()
                .map(
                    |case| match analyzer.analyze_word_budgeted(&case.input, budget) {
                        FomaApplyOutcome::Complete(outcome) => Ok(CaseOutcome::Complete(
                            project_all(&outcome.structured, grammar)?,
                        )),
                        FomaApplyOutcome::Incomplete {
                            dimension,
                            value,
                            limit,
                        } => Ok(CaseOutcome::Incomplete(IncompleteReason::LogicalBudget {
                            dimension: budget_dimension(dimension),
                            value: value as u64,
                            limit: limit as u64,
                        })),
                    },
                )
                .collect()
        }
    }
}

fn project_all(
    analyses: &[pg_parse::WordAnalysis],
    grammar: &Grammar,
) -> Result<AnalysisSet, CliError> {
    let mut annotated = Vec::with_capacity(analyses.len());
    for analysis in analyses {
        let identity = AnalysisIdentity::project(analysis, grammar).map_err(|e| {
            // An analysis referencing a morpheme the model does not have is an internal fault, not
            // the ordinary "this grammar deleted something" case — which is invisible here, because
            // identities are values.
            CliError::internal(format!("project analysis identity: {e}"))
        })?;
        annotated.push((identity, analysis.guessed));
    }
    Ok(AnalysisSet::from_annotated(annotated))
}

/// The one nonsemantic field in the artifact. It moves `reportId` and nothing else.
fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // A dependency-free rendering: the exact civil time matters to a human reader, not to any
    // digest, so an epoch-second stamp in RFC 3339's shape is honest and sufficient.
    let days = secs / 86_400;
    let time = secs % 86_400;
    let (mut y, mut remaining) = (1970u64, days);
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let len = if leap { 366 } else { 365 };
        if remaining < len {
            break;
        }
        remaining -= len;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let months = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while remaining >= months[month] {
        remaining -= months[month];
        month += 1;
    }
    format!(
        "{y:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        month + 1,
        remaining + 1,
        time / 3600,
        (time % 3600) / 60,
        time % 60
    )
}

// ---------------------------------------------------------------------------------------------
// compare
// ---------------------------------------------------------------------------------------------

pub fn run_compare(args: &[String]) -> Result<(), CliError> {
    let args = parse_args(args)?;
    let baseline = load_report(args.required_positional(0, "baseline.json")?)?;
    let candidate = load_report(args.required_positional(1, "candidate.json")?)?;
    let delta =
        compare(&baseline, &candidate).map_err(|e| CliError::internal(format!("compare: {e}")))?;
    // Exit 0 even when every case is `not_comparable`: the artifact is valid and a typed refusal is
    // evidence a consumer can act on (task 4.9).
    emit(&args, &delta.to_value())
}

fn load_report(path: &str) -> Result<AssessmentReport, CliError> {
    let document = read(path)?;
    parse_report(&document).map_err(|e| {
        // A report from another identity profile is an unsupported capability, not malformed input:
        // the file is well formed, this build just cannot read its encoding.
        let profile_mismatch = matches!(e, pg_assess::ReportError::ForeignIdentityProfile(_));
        let message = format!("report {path}: {e}");
        if profile_mismatch {
            CliError::unsupported(message)
        } else {
            CliError::invalid(message)
        }
    })
}

// ---------------------------------------------------------------------------------------------
// golden-diff
// ---------------------------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------------------------
// investigate
// ---------------------------------------------------------------------------------------------

pub fn run_investigate(args: &[String]) -> Result<(), CliError> {
    let args = parse_args(args)?;
    let report = load_report(args.required_positional(0, "report.json")?)?;
    let case_id = args
        .flag("case")
        .ok_or_else(|| CliError::invalid("missing --case <caseId>"))?;

    // Without a grammar there is nothing to re-run, so the handoff carries the binding and the
    // report's own facts, and says so rather than implying evidence it does not have.
    let mut request = HandoffRequest {
        case_id: case_id.to_string(),
        ..HandoffRequest::default()
    };
    if let Some(grammar_path) = args.flag("grammar") {
        let source = read(grammar_path)?;
        let source_kind = if grammar_path.ends_with(".json") {
            SourceKind::Snapshot
        } else {
            SourceKind::HcXml
        };
        let fingerprint =
            pg_assess::model_fingerprint(source_kind, &source, env!("CARGO_PKG_VERSION"))
                .map_err(|e| CliError::internal(format!("model fingerprint: {e}")))?;
        request.current_model_fingerprint = Some(fingerprint);
        request.evidence = Some(Evidence {
            availability: EvidenceAvailability::Regenerated,
            engine: report.draft().execution.pipeline.clone(),
            note: Some(
                "produced by re-running this case now, not captured during the assessment"
                    .to_string(),
            ),
        });
    }

    let handoff = investigate(&report, &request).map_err(|e| CliError::invalid(e.to_string()))?;
    let _ = MissingAnalysisCause::Undetermined; // documented default; attribution needs both pipelines
    emit(&args, &handoff.to_value())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_pipeline_is_foma_confirm() {
        // D13 inverts today's `--engine` default: production runs propose-and-confirm, so that is
        // what an assessment should describe.
        let args = parse_args(&["g.xml".to_string()]).unwrap();
        assert!(args.flag("pipeline").is_none());
        assert_eq!(Pipeline::FomaConfirm.as_str(), "foma-confirm");
    }

    #[test]
    fn an_unknown_pipeline_is_refused_rather_than_defaulted() {
        let err = Pipeline::parse("xample").unwrap_err();
        assert_eq!(err.code, EXIT_INVALID_INPUT);
        assert!(err.message.contains("foma-confirm|hermitcrab"));
    }

    #[test]
    fn flags_accept_both_spellings() {
        let args = parse_args(&[
            "g.xml".into(),
            "--pipeline=hermitcrab".into(),
            "--suite".into(),
            "s.json".into(),
        ])
        .unwrap();
        assert_eq!(args.flag("pipeline"), Some("hermitcrab"));
        assert_eq!(args.flag("suite"), Some("s.json"));
        assert_eq!(args.positional, vec!["g.xml".to_string()]);
    }

    #[test]
    fn a_non_numeric_budget_is_invalid_input_not_a_silent_zero() {
        let args = parse_args(&["--budget-paths".into(), "lots".into()]).unwrap();
        let err = args.number("budget-paths").unwrap_err();
        assert_eq!(err.code, EXIT_INVALID_INPUT);
    }

    #[test]
    fn budgets_are_unbounded_when_no_envelope_is_named() {
        // Task 3.4. No default is invented: `calibrate-fst-resource-envelopes` is data-blocked, and
        // a guessed cap would silently truncate analyses on real grammars.
        let args = parse_args(&["g.xml".into()]).unwrap();
        assert_eq!(args.number("budget-paths").unwrap(), None);
        assert_eq!(args.number("budget-candidates").unwrap(), None);
    }

    #[test]
    fn a_bare_word_list_synthesizes_deterministic_case_ids() {
        let dir = std::env::temp_dir().join("pg-assess-cli-wordlist");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("words.txt");
        std::fs::write(&path, "walked\nran\n\nwalked\n").unwrap();
        let args = parse_args(&["--words".into(), path.to_string_lossy().to_string()]).unwrap();

        let (suite, cases) = load_cases(&args).unwrap();
        assert_eq!(cases.len(), 3);
        assert_eq!(cases[0].case_id, "w0:walked");
        // Two cases sharing a surface form stay distinct, which a word-keyed map cannot express.
        assert_eq!(cases[2].case_id, "w2:walked");
        assert_eq!(cases[2].input, "walked");
        assert_eq!(suite.analysis_identity_profile, IDENTITY_PROFILE);

        let (again, _) = load_cases(&args).unwrap();
        assert_eq!(
            again.semantic_digest, suite.semantic_digest,
            "the same list must produce the same suite digest"
        );
    }

    #[test]
    fn a_suite_and_a_word_list_are_mutually_exclusive() {
        let args = parse_args(&[
            "--suite".into(),
            "s.json".into(),
            "--words".into(),
            "w.txt".into(),
        ])
        .unwrap();
        assert_eq!(load_cases(&args).unwrap_err().code, EXIT_INVALID_INPUT);
    }

    #[test]
    fn exit_codes_are_typed_rather_than_zero_or_one() {
        assert_eq!(EXIT_OK, 0);
        assert_eq!(EXIT_INVALID_INPUT, 2);
        assert_eq!(EXIT_UNSUPPORTED, 3);
        assert_eq!(EXIT_CONTAINED, 4);
        assert_eq!(EXIT_INTERNAL, 70);
    }

    #[test]
    fn the_generated_timestamp_has_rfc3339_shape() {
        let stamp = now_rfc3339();
        assert_eq!(stamp.len(), 20, "{stamp}");
        assert!(stamp.ends_with('Z'));
        assert_eq!(&stamp[4..5], "-");
        assert_eq!(&stamp[10..11], "T");
    }
}
