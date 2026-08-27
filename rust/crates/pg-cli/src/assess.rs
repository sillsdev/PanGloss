//! `pangloss assess | compare | golden-diff | investigate`: the grammar-assessment evidence layer, writing caller-owned artifacts under typed exit codes rather than a bare zero-or-one.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use pg_assess::{
    compare, golden_diff, investigate, parse_report, parse_suite, AnalysisIdentity, AnalysisSet,
    AssessmentFailure, AssessmentReport, BudgetDimension, CaseOutcome, CaseRecord, ConstructRef,
    Diagnostic, Evidence, EvidenceAvailability, Execution, FailureKind, HandoffRequest,
    IncompleteReason, MissingAnalysisCause, NarrativeStep, NotAttemptedReason, Provenance,
    ReportDraft, Severity, SourceKind, SuiteRef, ValidatedSuite, IDENTITY_PROFILE,
};
use pg_foma::compose_budget::{ApplyBudget, ApplyDimension};
use pg_foma::composite::{FomaAnalyzer, FomaApplyOutcome};
use pg_grammar::model::{AllomorphOwner, Grammar, MorphemeId};
use pg_parse::{Morpher, ParseOptions};
use pg_rules::trace::{TraceHandle, TraceSource, TreeTraceSink};
use pg_rules::word::Word;

use crate::load_grammar;

/// Typed process outcomes.
pub const EXIT_OK: u8 = 0;
pub const EXIT_INVALID_INPUT: u8 = 2;
pub const EXIT_UNSUPPORTED: u8 = 3;
pub const EXIT_CONTAINED: u8 = 4;
pub const EXIT_INTERNAL: u8 = 70;

/// Which analysis pipeline runs; an unavailable one is an `unsupported_capability` refusal, never a silent fallback to the other pipeline.
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

/// Which loader a grammar path dispatches to, by extension.
pub(crate) fn source_kind_of(path: &str) -> SourceKind {
    if path.ends_with(".json") {
        SourceKind::Snapshot
    } else {
        SourceKind::HcXml
    }
}

/// The effective logical budgets as the report records them, read off the enforced `ApplyBudget` rather than the flags that produced it, so an env-derived envelope is recorded as faithfully as a CLI one; an empty map means unbounded.
pub(crate) fn recorded_budgets(budget: &ApplyBudget) -> BTreeMap<String, u64> {
    let mut budgets = BTreeMap::new();
    if let Some(cap) = budget.path_cap() {
        budgets.insert(
            BudgetDimension::DecodedPaths.as_str().to_string(),
            cap as u64,
        );
    }
    if let Some(cap) = budget.candidate_cap() {
        budgets.insert(BudgetDimension::Candidates.as_str().to_string(), cap as u64);
    }
    budgets
}

/// Exhaustive translation from `pg_foma::ApplyDimension` to the artifact's `BudgetDimension`: a new variant fails to compile here rather than reaching a report unrecognized.
pub(crate) fn budget_dimension(dimension: ApplyDimension) -> BudgetDimension {
    match dimension {
        ApplyDimension::DecodedPaths => BudgetDimension::DecodedPaths,
        ApplyDimension::Candidates => BudgetDimension::Candidates,
    }
}

pub fn run_assess(args: &[String]) -> Result<(), CliError> {
    let args = parse_args(args)?;
    let grammar_path = args.required_positional(0, "grammar")?;

    let pipeline = match args.flag("pipeline") {
        None => Pipeline::FomaConfirm,
        Some(value) => Pipeline::parse(value)?,
    };

    // Budgets stay unbounded unless named; inventing a default would silently truncate analyses on real grammars.
    let budget = ApplyBudget::with_caps(
        args.number("budget-paths")?,
        args.number("budget-candidates")?,
    );
    let budgets = recorded_budgets(&budget);

    let (suite, cases) = load_cases(&args)?;

    let source = read(grammar_path)?;
    let source_kind = source_kind_of(grammar_path);
    let compiler_version = env!("CARGO_PKG_VERSION");
    let provenance = Provenance {
        source_sha256: pg_assess::source_sha256(source.as_bytes()),
        source_kind: source_kind.as_str().to_string(),
        model_fingerprint: pg_assess::model_fingerprint(source_kind, &source, compiler_version)
            .map_err(|e| CliError::internal(format!("model fingerprint: {e}")))?,
        importer_version: compiler_version.to_string(),
        compiler_version: compiler_version.to_string(),
    };

    // A compile/import failure after suite validation is not an error exit: it becomes a `failed` artifact (every case `not_attempted/assessment_setup_failed`) so a CI consumer and `compare` still have something to read.
    let (grammar, warnings) = match crate::load_grammar_coded(grammar_path) {
        Ok(loaded) => loaded,
        Err(message) => {
            let report =
                setup_failed_report(suite, pipeline, budgets, provenance, &cases, &message)?;
            return emit(&args, &report.to_value());
        }
    };
    // Warnings keep the stable code their emission site assigned: `compare` diffs diagnostics by code and count, so collapsing codes would hide a real change behind reworded prose.
    let diagnostics = warnings
        .iter()
        .map(|warning| Diagnostic {
            code: warning.code.to_string(),
            severity: Severity::Warning,
            message: warning.message.clone(),
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

/// The artifact for a safely-failed setup: every case is `not_attempted/assessment_setup_failed`, so `derive_status` reports `failed` rather than reading as a grammar that analyzes nothing.
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
        // A typed top-level reason: a consumer without our exit code would otherwise infer the cause from prose.
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

/// A suite, or a bare word list with synthesized case IDs that are positional: stable across reruns, not across edits; authoring a suite buys identity that survives reordering.
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
                let message = format!("the foma-confirm pipeline cannot run this grammar: {e}");
                match e {
                    pg_foma::analyzer::FomaError::LexcCompileFailed(_)
                    | pg_foma::analyzer::FomaError::Unsupported(_)
                    | pg_foma::analyzer::FomaError::Incomplete(_) => CliError::unsupported(message),
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
            // A morpheme the model does not have is an internal fault here, not the ordinary "grammar deleted something" case.
            CliError::internal(format!("project analysis identity: {e}"))
        })?;
        annotated.push((identity, analysis.guessed));
    }
    Ok(AnalysisSet::from_annotated(annotated))
}

/// The one nonsemantic field in the artifact. It moves `reportId` and nothing else.
pub(crate) fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Dependency-free: the civil time matters to a human reader, not to any digest.
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

    // Without a grammar there is nothing to re-run; the handoff says so rather than implying evidence it lacks.
    let mut request = HandoffRequest {
        case_id: case_id.to_string(),
        ..HandoffRequest::default()
    };
    if let Some(grammar_path) = args.flag("grammar") {
        let source = read(grammar_path)?;
        let source_kind = source_kind_of(grammar_path);
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

        // Attributes a missing analysis to HermitCrab rejection vs. a proposer recall gap by running the case on both pipelines, best-effort, never a fabricated attribution when either is unavailable.
        // A model-fingerprint mismatch against the report is refused first — pinned by `a_different_model_is_refused_rather_than_traced`.
        if let Some(case) = report.cases().iter().find(|c| c.case_id == case_id) {
            if let Ok((grammar, _warnings)) = load_grammar(grammar_path) {
                if let Ok((hc_identities, hc_failures)) =
                    run_hermitcrab_pipeline(&grammar, &case.input)
                {
                    // The pruned narrative is exactly the HermitCrab failure evidence, independent of foma-confirm's availability below.
                    request.narrative = hc_failures.iter().map(|f| f.step.clone()).collect();

                    // `None` means foma-confirm could not produce a trustworthy result right now, so attribution below stays `Undetermined` rather than guessed from a partial run.
                    let foma_identities = run_foma_pipeline(&grammar, &case.input);

                    // Attributes a cause for whatever HermitCrab produces now that the report's recorded outcome does not contain — the "missing analysis" class `investigate` exists to explain.
                    let report_observed: Vec<AnalysisIdentity> = case
                        .outcome
                        .analyses()
                        .map(|set| set.entries().iter().map(|e| e.identity.clone()).collect())
                        .unwrap_or_default();
                    let asked_about: Vec<AnalysisIdentity> = hc_identities
                        .iter()
                        .filter(|identity| !report_observed.contains(identity))
                        .cloned()
                        .collect();

                    if !asked_about.is_empty() {
                        request.causes = attribute_causes(
                            &asked_about,
                            &hc_identities,
                            &hc_failures,
                            foma_identities.as_deref(),
                        );
                        request.asked_about = asked_about;
                    }
                }
            }
        }
    }

    let handoff = investigate(&report, &request).map_err(|e| CliError::invalid(e.to_string()))?;
    emit(&args, &handoff.to_value())
}

/// One HermitCrab trace node that carried a `FailureReason`: the pruning unit for both the failure narrative and the "did HermitCrab reject a matching candidate" evidence below.
struct HermitcrabFailure {
    step: NarrativeStep,
    /// The rejected candidate's ordered stable morpheme keys, `None` per slot exactly where `AnalysisIdentity` would (a guessed root has no `Grammar::morphemes` row); used only to match against an asked-about identity.
    candidate_morphemes: Vec<Option<String>>,
}

/// Runs the HermitCrab pipeline on one case's input with a real trace sink, returning every analysis plus the pruned failure evidence.
fn run_hermitcrab_pipeline(
    grammar: &Grammar,
    input: &str,
) -> Result<(Vec<AnalysisIdentity>, Vec<HermitcrabFailure>), CliError> {
    let morpher = Morpher::new(grammar, usize::MAX);
    let sink = TreeTraceSink::new();
    let outcome = morpher.parse_word_traced(input, &ParseOptions::default(), &sink);
    let identities = project_identities(&outcome.structured, grammar)?;
    let failures = match sink.root() {
        Some(root) => collect_hermitcrab_failures(grammar, &sink, root),
        None => Vec::new(),
    };
    Ok((identities, failures))
}

/// Runs foma-confirm on the same input; `None` means no trustworthy result right now (compile failure or budget trip), never a fabricated "zero analyses".
fn run_foma_pipeline(grammar: &Grammar, input: &str) -> Option<Vec<AnalysisIdentity>> {
    let mut analyzer = FomaAnalyzer::new(grammar).ok()?;
    let budget = ApplyBudget::with_caps(None, None);
    match analyzer.analyze_word_budgeted(input, &budget) {
        FomaApplyOutcome::Complete(outcome) => {
            project_identities(&outcome.structured, grammar).ok()
        }
        FomaApplyOutcome::Incomplete { .. } => None,
    }
}

fn project_identities(
    analyses: &[pg_parse::WordAnalysis],
    grammar: &Grammar,
) -> Result<Vec<AnalysisIdentity>, CliError> {
    analyses
        .iter()
        .map(|a| {
            AnalysisIdentity::project(a, grammar)
                .map_err(|e| CliError::internal(format!("project analysis identity: {e}")))
        })
        .collect()
}

/// Classifies each asked-about identity from both pipelines plus the HermitCrab trace's failure evidence; an identity it cannot place with confidence stays `NeitherPipelineProduces` rather than an asserted rejection, and the whole set stays `Undetermined` when foma-confirm cannot be trusted.
fn attribute_causes(
    asked_about: &[AnalysisIdentity],
    hc_identities: &[AnalysisIdentity],
    hc_failures: &[HermitcrabFailure],
    foma_identities: Option<&[AnalysisIdentity]>,
) -> Vec<(AnalysisIdentity, MissingAnalysisCause)> {
    asked_about
        .iter()
        .map(|identity| {
            let produced_by_hc = hc_identities.contains(identity);
            let cause = match foma_identities {
                // Attribution needs both pipelines; only one was available. Never guess.
                None => MissingAnalysisCause::Undetermined,
                Some(foma) => {
                    let produced_by_foma = foma.contains(identity);
                    match (produced_by_hc, produced_by_foma) {
                        // HermitCrab alone produces it: the recall gap the propose-and-confirm invariant exists to prevent.
                        (true, false) => MissingAnalysisCause::ProposerRecallGap,
                        // A real grammar fact, unless the HermitCrab trace shows it explicitly rejecting this exact candidate — the more specific fact to report if so.
                        (false, false) => {
                            let rejected = hc_failures
                                .iter()
                                .any(|f| f.candidate_morphemes == identity.morphemes);
                            if rejected {
                                MissingAnalysisCause::HermitcrabRejected
                            } else {
                                MissingAnalysisCause::NeitherPipelineProduces
                            }
                        }
                        // foma-confirm produces it regardless of HermitCrab: not the recall/rejection question this function answers, so said honestly rather than guessed.
                        (_, true) => MissingAnalysisCause::Undetermined,
                    }
                }
            };
            (identity.clone(), cause)
        })
        .collect()
}

/// Walks from `root`, keeping only the nodes that carried a `FailureReason` — the entire prune behind the failure narrative and rejection evidence above.
fn collect_hermitcrab_failures(
    grammar: &Grammar,
    sink: &TreeTraceSink,
    root: TraceHandle,
) -> Vec<HermitcrabFailure> {
    let mut out = Vec::new();
    collect_hermitcrab_failures_node(grammar, sink, root, &mut out);
    out
}

fn collect_hermitcrab_failures_node(
    grammar: &Grammar,
    sink: &TreeTraceSink,
    handle: TraceHandle,
    out: &mut Vec<HermitcrabFailure>,
) {
    let node = sink.node(handle);
    if let Some(reason) = node.failure_reason {
        let word = node.output.as_ref().or(node.input.as_ref());
        let candidate_morphemes = word
            .map(|w| word_morpheme_keys(w, grammar))
            .unwrap_or_default();
        let candidate = word
            .map(|w| display_candidate(grammar, w))
            .unwrap_or_else(|| "(no candidate word captured)".to_string());
        let at = narrative_construct_ref(grammar, node.source, word);
        out.push(HermitcrabFailure {
            step: NarrativeStep {
                candidate,
                at,
                // `pg_rules::trace::FailureReason`'s variant name, carried verbatim.
                failure_reason: format!("{reason:?}"),
                // Factual: what was observed, never why the grammar is wrong or what to change.
                detail: format!(
                    "HermitCrab produced this candidate and rejected it at a {:?} node",
                    node.type_
                ),
            },
            candidate_morphemes,
        });
    }
    for &child in &node.children {
        collect_hermitcrab_failures_node(grammar, sink, child, out);
    }
}

/// The rejected candidate's morpheme keys in `AnalysisIdentity::morphemes`'s own shape, so a failure can be matched against an asked-about identity by simple equality.
fn word_morpheme_keys(word: &Word, grammar: &Grammar) -> Vec<Option<String>> {
    word.morpheme_sequence()
        .into_iter()
        .map(|m| {
            if m == MorphemeId::GUESSED {
                None
            } else {
                grammar
                    .morphemes
                    .get(m.0 as usize)
                    .map(|info| info.xml_key.clone())
            }
        })
        .collect()
}

/// A human-readable morpheme join for the narrative's `candidate` field (e.g. `"walk + ed"`); display only, not for identity matching.
fn display_candidate(grammar: &Grammar, word: &Word) -> String {
    let ids = word.morpheme_sequence();
    if ids.is_empty() {
        return "(no morphemes)".to_string();
    }
    ids.iter()
        .map(|m| {
            if *m == MorphemeId::GUESSED {
                "?".to_string()
            } else {
                grammar
                    .morphemes
                    .get(m.0 as usize)
                    .map(|info| {
                        info.morph_id
                            .clone()
                            .unwrap_or_else(|| info.xml_key.clone())
                    })
                    .unwrap_or_else(|| format!("morpheme#{}", m.0))
            }
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Where a trace node's failure lives, as a `ConstructRef`: rule/stratum/template sources are always `compilerAssigned` dense ordinals, but a leaf node whose root allomorph resolves to a real lexical entry gets `sourceId` instead, since that stable FieldWorks identity is the more useful reference.
fn narrative_construct_ref(
    grammar: &Grammar,
    source: TraceSource,
    word: Option<&Word>,
) -> ConstructRef {
    match source {
        TraceSource::MorphRule(id) => ConstructRef::compiler_assigned(
            "morphologicalRule",
            id.0 as usize,
            Some(hermitcrab_mrule_name(grammar, id)),
        ),
        TraceSource::PhonRule(id) => ConstructRef::compiler_assigned(
            "phonologicalRule",
            id.0 as usize,
            Some(hermitcrab_prule_name(grammar, id)),
        ),
        TraceSource::Stratum(id) => ConstructRef::compiler_assigned(
            "stratum",
            id.0 as usize,
            Some(hermitcrab_stratum_name(grammar, id)),
        ),
        TraceSource::Template(id) => ConstructRef::compiler_assigned(
            "template",
            id.0 as usize,
            Some(hermitcrab_template_name(grammar, id)),
        ),
        TraceSource::Language | TraceSource::None => {
            match word.and_then(|w| lexical_entry_ref(grammar, w)) {
                Some(entry_ref) => entry_ref,
                None => {
                    let stratum = word.map(|w| w.stratum.0).unwrap_or(0);
                    ConstructRef::compiler_assigned("stratum", stratum as usize, None)
                }
            }
        }
    }
}

fn lexical_entry_ref(grammar: &Grammar, word: &Word) -> Option<ConstructRef> {
    let allo = word.root_allomorph?;
    if allo == pg_grammar::model::AllomorphId::GUESSED {
        return None;
    }
    match grammar.allomorph_owners.get(allo.0 as usize)? {
        AllomorphOwner::Root(entry_id, _) => {
            let entry = grammar.entries.get(entry_id.0 as usize)?;
            Some(ConstructRef::source(
                "lexicalEntry",
                entry.authored_id.clone(),
                None,
            ))
        }
        AllomorphOwner::Affix(_, _) => None,
    }
}

fn hermitcrab_mrule_name(g: &Grammar, id: pg_grammar::model::MRuleId) -> String {
    let idx = id.0 as usize;
    let Some(rule) = g.mrules.get(idx) else {
        return format!("mrule#{idx}");
    };
    let name = match rule {
        pg_grammar::model::MorphRuleDef::AffixProcess(d) => d.name.as_deref(),
        pg_grammar::model::MorphRuleDef::Realizational(d) => d.name.as_deref(),
        pg_grammar::model::MorphRuleDef::Compounding(d) => d.name.as_deref(),
    };
    name.map(str::to_string)
        .unwrap_or_else(|| format!("mrule#{idx}"))
}

fn hermitcrab_prule_name(g: &Grammar, id: pg_grammar::model::PRuleId) -> String {
    let idx = id.0 as usize;
    let Some(rule) = g.prules.get(idx) else {
        return format!("prule#{idx}");
    };
    let name = match rule {
        pg_grammar::model::PhonRuleDef::Rewrite(d) => d.name.as_deref(),
        pg_grammar::model::PhonRuleDef::Metathesis(d) => d.name.as_deref(),
    };
    name.map(str::to_string)
        .unwrap_or_else(|| format!("prule#{idx}"))
}

fn hermitcrab_stratum_name(g: &Grammar, id: pg_grammar::model::StratumId) -> String {
    g.strata
        .get(id.0 as usize)
        .and_then(|s| s.name.clone())
        .unwrap_or_else(|| format!("stratum#{}", id.0))
}

fn hermitcrab_template_name(g: &Grammar, id: pg_grammar::model::TemplateId) -> String {
    g.templates
        .get(id.0 as usize)
        .and_then(|t| t.name.clone())
        .unwrap_or_else(|| format!("template#{}", id.0))
}
