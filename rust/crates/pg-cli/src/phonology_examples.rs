//! The `phonology-examples` command: versioned JSON rows saying what ordered phonology does to each example.

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

use pg_parse::{
    measure_phonology, Morpher, PhonologyContext, PhonologyMeasurement, PhonologyRequest,
    PhonologyResult, PhonologyStopReason, PhonologyUnavailableReason,
};
use pg_snapshot::FwClass;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// The only wire version this command reads or writes.
const WIRE_VERSION: u32 = 1;
const MAX_EXAMPLES: usize = 500;
const MAX_ID_CHARS: usize = 128;
const MAX_CONTEXT_ID_CHARS: usize = 256;
const MAX_FORM_CHARS: usize = 256;
const MAX_EXPECTED: usize = 16;
const MAX_CITATION_CHARS: usize = 512;
const MAX_STEP_CAP: u64 = 50_000_000;
const MAX_WORK_CAP: u64 = 5_000_000_000;
const MAX_TIMEOUT_MS: u64 = 600_000;
const MAX_REQUEST_BYTES: u64 = 32 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const MAX_RULE_IDS_PER_ROW: usize = 1_024;
const MAX_TOTAL_RULE_IDS: usize = 4_096;
const MAX_RULE_ID_CHARS: usize = 256;
const MAX_OBSERVED_CHARS: usize = 512;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    version: u32,
    limits: Limits,
    examples: Vec<Example>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Limits {
    step_cap: u64,
    work_cap: u64,
    timeout_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Example {
    id: String,
    context: Context,
    underlying: String,
    expected: Vec<String>,
    /// Review metadata owned by Motif; excluded from executable request identity.
    citation: Option<Citation>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Context {
    #[serde(rename = "hcXmlEntry", rename_all = "camelCase")]
    HcXmlEntry { entry_id: String },
    #[serde(rename = "fwdataEntry", rename_all = "camelCase")]
    FwdataEntry {
        entry_guid: String,
        msa_guid: String,
        msa_class: String,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Citation {
    source: String,
    locator: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    version: u32,
    grammar_sha256: String,
    grammar_dependencies: Vec<GrammarDependency>,
    request_sha256: String,
    request_file_sha256: String,
    build: String,
    limits: EffectiveLimits,
    measured_entry_count: u64,
    rows: Vec<Row>,
    counts: Counts,
    completion_counts: CompletionCounts,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GrammarDependency {
    path: String,
    sha256: String,
}

struct GrammarSnapshot {
    grammar: crate::Grammar,
    dependencies: Vec<GrammarDependency>,
    source_paths: Vec<std::path::PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EffectiveLimits {
    scope: &'static str,
    step_cap: u64,
    work_cap: u64,
    timeout_ms: u64,
    step_allocation: &'static str,
    time_allocation: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RowLimits {
    step_cap: u64,
    work_cap: u64,
    timeout_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: String,
    requested_context: Context,
    resolved_context: Option<Context>,
    underlying: String,
    expected: Vec<String>,
    effective_limits: RowLimits,
    disposition: Disposition,
    completion: Completion,
    expected_observed: Vec<bool>,
    observed: Option<String>,
    reason: Option<Reason>,
    steps: u64,
    work_steps: u64,
    capped: bool,
    timed_out: bool,
    /// Declared rule schedule for strata entered; does not imply visitation or application.
    ordered_rule_ids: Vec<String>,
    /// Exact rules whose budgeted invocation began.
    visited_rule_ids: Vec<String>,
    /// Exact visited rules that produced an output shape.
    applied_rule_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Disposition {
    Agree,
    Disagree,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Completion {
    Complete,
    Incomplete,
    NotStarted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Reason {
    EmptyLexicon,
    UnknownEntry,
    AmbiguousContext,
    MsaNotOwnedByEntry,
    InvalidShape,
    InvalidStratum,
    ResultLimit,
    WorkCap,
    StepCap,
    Timeout,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Counts {
    agree: u64,
    disagree: u64,
    unavailable: u64,
    total: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CompletionCounts {
    complete: u64,
    incomplete: u64,
    not_started: u64,
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The producer's form normalization: NFD, so composed and decomposed spellings of an expectation agree.
fn normalize(text: &str) -> String {
    text.nfd().collect()
}

fn char_len_in(text: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&text.chars().count())
}

/// Every bound is checked here, before the grammar is loaded or any search runs.
fn validate(request: &Request) -> Result<(), String> {
    if request.version != WIRE_VERSION {
        return Err(format!(
            "request version {} is not supported; this command reads version {WIRE_VERSION}",
            request.version
        ));
    }
    let limits = request.limits;
    if limits.step_cap == 0 || limits.step_cap > MAX_STEP_CAP {
        return Err(format!(
            "limits.stepCap must be in 1..={MAX_STEP_CAP}, got {}",
            limits.step_cap
        ));
    }
    if limits.work_cap == 0 || limits.work_cap > MAX_WORK_CAP {
        return Err(format!(
            "limits.workCap must be in 1..={MAX_WORK_CAP}, got {}",
            limits.work_cap
        ));
    }
    if limits.timeout_ms == 0 || limits.timeout_ms > MAX_TIMEOUT_MS {
        return Err(format!(
            "limits.timeoutMs must be in 1..={MAX_TIMEOUT_MS}, got {}",
            limits.timeout_ms
        ));
    }
    if usize::try_from(limits.step_cap).is_err() || usize::try_from(limits.work_cap).is_err() {
        return Err(format!(
            "limits.stepCap and limits.workCap must fit this platform's usize ({} bits)",
            usize::BITS
        ));
    }
    if request.examples.is_empty() || request.examples.len() > MAX_EXAMPLES {
        return Err(format!(
            "examples must hold 1..={MAX_EXAMPLES} items, got {}",
            request.examples.len()
        ));
    }
    let mut seen = HashSet::new();
    for (index, example) in request.examples.iter().enumerate() {
        if !char_len_in(&example.id, 1, MAX_ID_CHARS) {
            return Err(format!(
                "examples[{index}].id must be 1..={MAX_ID_CHARS} characters"
            ));
        }
        if !seen.insert(example.id.as_str()) {
            return Err(format!("duplicate example id `{}`", example.id));
        }
        let at = &example.id;
        if !char_len_in(&example.underlying, 1, MAX_FORM_CHARS) {
            return Err(format!(
                "example `{at}`: underlying must be 1..={MAX_FORM_CHARS} characters"
            ));
        }
        if example.expected.is_empty() || example.expected.len() > MAX_EXPECTED {
            return Err(format!(
                "example `{at}`: expected must hold 1..={MAX_EXPECTED} items"
            ));
        }
        if let Some(bad) = example
            .expected
            .iter()
            .find(|form| !char_len_in(form, 1, MAX_FORM_CHARS))
        {
            return Err(format!(
                "example `{at}`: expected form `{bad}` must be 1..={MAX_FORM_CHARS} characters"
            ));
        }
        if let Some(citation) = &example.citation {
            if !char_len_in(&citation.source, 1, MAX_CITATION_CHARS) {
                return Err(format!(
                    "example `{at}`: citation.source must be 1..={MAX_CITATION_CHARS} characters"
                ));
            }
            if citation
                .locator
                .as_ref()
                .is_some_and(|locator| !char_len_in(locator, 0, MAX_CITATION_CHARS))
            {
                return Err(format!(
                    "example `{at}`: citation.locator must be at most {MAX_CITATION_CHARS} characters"
                ));
            }
        }
        match &example.context {
            Context::HcXmlEntry { entry_id } if !char_len_in(entry_id, 1, MAX_CONTEXT_ID_CHARS) => {
                return Err(format!(
                    "example `{at}`: context.entryId must be 1..={MAX_CONTEXT_ID_CHARS} characters"
                ));
            }
            Context::FwdataEntry {
                entry_guid,
                msa_guid,
                msa_class,
            } => {
                if !char_len_in(entry_guid, 1, MAX_CONTEXT_ID_CHARS)
                    || !char_len_in(msa_guid, 1, MAX_CONTEXT_ID_CHARS)
                {
                    return Err(format!(
                        "example `{at}`: context entryGuid and msaGuid must be 1..={MAX_CONTEXT_ID_CHARS} characters"
                    ));
                }
                if msa_class.chars().count() > 64
                    || !matches!(
                        FwClass::from_wire(msa_class),
                        FwClass::MoStemMsa
                            | FwClass::MoInflAffMsa
                            | FwClass::MoDerivAffMsa
                            | FwClass::MoUnclassifiedAffixMsa
                    )
                {
                    return Err(format!(
                        "example `{at}`: context.msaClass `{msa_class}` is not a supported FieldWorks MSA class"
                    ));
                }
            }
            Context::HcXmlEntry { .. } => {}
        }
    }
    Ok(())
}

fn to_context(context: &Context) -> PhonologyContext {
    match context {
        Context::HcXmlEntry { entry_id } => PhonologyContext::HcXmlEntry {
            entry_id: entry_id.clone(),
        },
        Context::FwdataEntry {
            entry_guid,
            msa_guid,
            msa_class,
        } => PhonologyContext::FwdataEntry {
            entry_guid: entry_guid.clone(),
            msa_guid: msa_guid.clone(),
            msa_class: FwClass::from_wire(msa_class),
        },
    }
}

fn unavailable_reason(reason: PhonologyUnavailableReason) -> Reason {
    match reason {
        PhonologyUnavailableReason::EmptyLexicon => Reason::EmptyLexicon,
        PhonologyUnavailableReason::UnknownEntry => Reason::UnknownEntry,
        PhonologyUnavailableReason::AmbiguousContext => Reason::AmbiguousContext,
        PhonologyUnavailableReason::MsaNotOwnedByEntry => Reason::MsaNotOwnedByEntry,
        PhonologyUnavailableReason::InvalidShape => Reason::InvalidShape,
        PhonologyUnavailableReason::InvalidStratum => Reason::InvalidStratum,
        PhonologyUnavailableReason::ResultLimit => Reason::ResultLimit,
    }
}

fn stop_reason(reason: PhonologyStopReason) -> Reason {
    match reason {
        PhonologyStopReason::WorkCap => Reason::WorkCap,
        PhonologyStopReason::Timeout => Reason::Timeout,
        PhonologyStopReason::AttemptCap => Reason::StepCap,
    }
}

/// Only a completed surface can agree or disagree; every authored expectation must match it.
fn classify(
    result: &PhonologyResult,
    expected: &[String],
) -> (Disposition, Option<String>, Vec<bool>) {
    match result {
        PhonologyResult::Complete { surface } => {
            let observed = normalize(surface);
            let expected_observed: Vec<bool> =
                expected.iter().map(|form| form == &observed).collect();
            let disposition = if expected_observed.iter().all(|&matched| matched) {
                Disposition::Agree
            } else {
                Disposition::Disagree
            };
            (disposition, Some(observed), expected_observed)
        }
        PhonologyResult::Incomplete { .. } | PhonologyResult::Unavailable { .. } => {
            (Disposition::Unavailable, None, vec![false; expected.len()])
        }
    }
}

fn row_for(example: &Example, limits: RowLimits, morpher: &Morpher<'_>) -> Result<Row, String> {
    let normalized_expected: Vec<String> = example
        .expected
        .iter()
        .map(|form| normalize(form))
        .collect();
    let measurement: PhonologyMeasurement = measure_phonology(
        morpher,
        &PhonologyRequest {
            context: to_context(&example.context),
            underlying_shape: normalize(&example.underlying),
            work_cap: usize::try_from(limits.work_cap)
                .map_err(|_| "limits.workCap does not fit this platform".to_string())?,
            timeout: Duration::from_millis(limits.timeout_ms),
            attempt_cap: usize::try_from(limits.step_cap)
                .map_err(|_| "limits.stepCap does not fit this platform".to_string())?,
            max_ordered_rule_ids: MAX_RULE_IDS_PER_ROW,
            max_rule_id_chars: MAX_RULE_ID_CHARS,
        },
    );
    let resolved_context = match &measurement.resolved_context {
        Some(actual) if actual == &to_context(&example.context) => Some(example.context.clone()),
        Some(_) => {
            return Err(format!(
                "example `{}` resolved to a different source context than requested",
                example.id
            ));
        }
        None => None,
    };
    let (disposition, observed, expected_observed) =
        classify(&measurement.result, &normalized_expected);
    if observed
        .as_ref()
        .is_some_and(|surface| surface.chars().count() > MAX_OBSERVED_CHARS)
    {
        return Err(format!(
            "example `{}` observed surface exceeds {MAX_OBSERVED_CHARS} characters",
            example.id
        ));
    }
    let completion = match measurement.completion() {
        pg_parse::PhonologyCompletion::Complete => Completion::Complete,
        pg_parse::PhonologyCompletion::Incomplete => Completion::Incomplete,
        pg_parse::PhonologyCompletion::NotStarted => Completion::NotStarted,
    };
    let reason = match &measurement.result {
        PhonologyResult::Unavailable { reason } => Some(unavailable_reason(*reason)),
        PhonologyResult::Incomplete { reason } => Some(stop_reason(*reason)),
        PhonologyResult::Complete { .. } => None,
    };
    Ok(Row {
        id: example.id.clone(),
        requested_context: example.context.clone(),
        resolved_context,
        underlying: example.underlying.clone(),
        expected: example.expected.clone(),
        effective_limits: limits,
        disposition,
        completion,
        observed,
        expected_observed,
        reason,
        steps: measurement.steps as u64,
        work_steps: measurement.work_steps as u64,
        capped: measurement.capped,
        timed_out: measurement.timed_out,
        ordered_rule_ids: measurement.ordered_rule_ids,
        visited_rule_ids: measurement.visited_rule_ids,
        applied_rule_ids: measurement.applied_rule_ids,
    })
}

fn count(rows: &[Row], disposition: Disposition) -> u64 {
    rows.iter().filter(|r| r.disposition == disposition).count() as u64
}

fn count_completion(rows: &[Row], completion: Completion) -> u64 {
    rows.iter().filter(|r| r.completion == completion).count() as u64
}

fn cap_for_row(total: u64, row_count: u64, row_index: u64) -> u64 {
    total / row_count + u64::from(row_index < total % row_count)
}

fn update_hashed_string(hasher: &mut Sha256, value: &str) -> Result<(), String> {
    let length = u32::try_from(value.len())
        .map_err(|_| "executable request string is too long to identify".to_string())?;
    hasher.update(length.to_be_bytes());
    hasher.update(value.as_bytes());
    Ok(())
}

fn executable_request_sha256(request: &Request) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(b"pangloss phonology examples request v1\0");
    hasher.update(request.version.to_be_bytes());
    hasher.update(request.limits.step_cap.to_be_bytes());
    hasher.update(request.limits.work_cap.to_be_bytes());
    hasher.update(request.limits.timeout_ms.to_be_bytes());
    let example_count = u32::try_from(request.examples.len())
        .map_err(|_| "example count is too large to identify".to_string())?;
    hasher.update(example_count.to_be_bytes());
    for example in &request.examples {
        update_hashed_string(&mut hasher, &example.id)?;
        match &example.context {
            Context::HcXmlEntry { entry_id } => {
                hasher.update([0]);
                update_hashed_string(&mut hasher, entry_id)?;
            }
            Context::FwdataEntry {
                entry_guid,
                msa_guid,
                msa_class,
            } => {
                hasher.update([1]);
                update_hashed_string(&mut hasher, entry_guid)?;
                update_hashed_string(&mut hasher, msa_guid)?;
                update_hashed_string(&mut hasher, msa_class)?;
            }
        }
        update_hashed_string(&mut hasher, &example.underlying)?;
        let expected_count = u32::try_from(example.expected.len())
            .map_err(|_| "expected-form count is too large to identify".to_string())?;
        hasher.update(expected_count.to_be_bytes());
        for expected in &example.expected {
            update_hashed_string(&mut hasher, expected)?;
        }
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn load_grammar_snapshot(
    grammar_path: &str,
    grammar_bytes: &[u8],
) -> Result<GrammarSnapshot, String> {
    let temp_dir =
        tempfile::tempdir().map_err(|e| format!("create private grammar snapshot: {e}"))?;
    let extension = Path::new(grammar_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty());
    let snapshot_name = Path::new(grammar_path)
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_else(|| {
            extension.map_or_else(|| "grammar".into(), |ext| format!("grammar.{ext}").into())
        });
    let snapshot_path = temp_dir.path().join(snapshot_name);
    std::fs::write(&snapshot_path, grammar_bytes)
        .map_err(|e| format!("write private grammar snapshot: {e}"))?;
    let mut dependencies = Vec::new();
    let mut source_paths = Vec::new();
    if extension.is_some_and(|ext| ext.eq_ignore_ascii_case("fwdata")) {
        let source_dir = Path::new(grammar_path)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join("WritingSystemStore");
        match std::fs::read_dir(&source_dir) {
            Ok(entries) => {
                let mut entries = entries
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("read {}: {e}", source_dir.display()))?;
                entries.sort_by_key(|entry| entry.file_name().to_string_lossy().into_owned());
                let snapshot_store = temp_dir.path().join("WritingSystemStore");
                std::fs::create_dir(&snapshot_store)
                    .map_err(|e| format!("create private writing-system snapshot: {e}"))?;
                for entry in entries {
                    let source = entry.path();
                    if source.extension().and_then(|ext| ext.to_str()) != Some("ldml") {
                        continue;
                    }
                    let Some(file_name) = source.file_name() else {
                        continue;
                    };
                    if file_name.to_str().is_none() {
                        continue;
                    }
                    let bytes = std::fs::read(&source).map_err(|e| {
                        format!("read grammar dependency {}: {e}", source.display())
                    })?;
                    std::str::from_utf8(&bytes).map_err(|e| {
                        format!("grammar dependency {} is not UTF-8: {e}", source.display())
                    })?;
                    std::fs::write(snapshot_store.join(file_name), &bytes).map_err(|e| {
                        format!("write private grammar dependency {}: {e}", source.display())
                    })?;
                    dependencies.push(GrammarDependency {
                        path: Path::new("WritingSystemStore")
                            .join(file_name)
                            .to_string_lossy()
                            .replace('\\', "/"),
                        sha256: sha256_hex(&bytes),
                    });
                    source_paths.push(source);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!("read {}: {error}", source_dir.display()));
            }
        }
    }
    let path = snapshot_path.to_string_lossy();
    let (grammar, _warnings) = crate::load_grammar(&path)?;
    Ok(GrammarSnapshot {
        grammar,
        dependencies,
        source_paths,
    })
}

fn build_response(
    grammar_path: &str,
    grammar_bytes: &[u8],
    request_bytes: &[u8],
    request: &Request,
    out_path: Option<&Path>,
) -> Result<Response, String> {
    let snapshot = load_grammar_snapshot(grammar_path, grammar_bytes)?;
    if let Some(out) = out_path {
        for input in &snapshot.source_paths {
            if same_file(out, input) {
                return Err(format!(
                    "refusing to overwrite grammar dependency {} with the result",
                    input.display()
                ));
            }
        }
    }
    let grammar_dependencies = snapshot.dependencies;
    let grammar = snapshot.grammar;
    // One Morpher serves every row: it builds the whole rule cache and root tries once.
    let morpher = Morpher::new(&grammar, usize::MAX);
    let row_count = u64::try_from(request.examples.len())
        .map_err(|_| "example count does not fit the result format".to_string())?;
    let measurement_deadline = Instant::now() + Duration::from_millis(request.limits.timeout_ms);
    let mut rows = Vec::with_capacity(request.examples.len());
    let mut total_rule_ids = 0usize;
    for (index, example) in request.examples.iter().enumerate() {
        let row_index = u64::try_from(index)
            .map_err(|_| "example index does not fit the result format".to_string())?;
        let timeout_ms = u64::try_from(
            measurement_deadline
                .saturating_duration_since(Instant::now())
                .as_millis(),
        )
        .unwrap_or(u64::MAX);
        let row_limits = RowLimits {
            step_cap: cap_for_row(request.limits.step_cap, row_count, row_index),
            work_cap: cap_for_row(request.limits.work_cap, row_count, row_index),
            timeout_ms,
        };
        let row = row_for(example, row_limits, &morpher)?;
        total_rule_ids = total_rule_ids
            .saturating_add(row.ordered_rule_ids.len())
            .saturating_add(row.visited_rule_ids.len())
            .saturating_add(row.applied_rule_ids.len());
        if total_rule_ids > MAX_TOTAL_RULE_IDS {
            return Err(format!(
                "rule identity entries exceed the {MAX_TOTAL_RULE_IDS}-item result bound"
            ));
        }
        rows.push(row);
    }
    let counts = Counts {
        agree: count(&rows, Disposition::Agree),
        disagree: count(&rows, Disposition::Disagree),
        unavailable: count(&rows, Disposition::Unavailable),
        total: rows.len() as u64,
    };
    let completion_counts = CompletionCounts {
        complete: count_completion(&rows, Completion::Complete),
        incomplete: count_completion(&rows, Completion::Incomplete),
        not_started: count_completion(&rows, Completion::NotStarted),
    };
    Ok(Response {
        version: WIRE_VERSION,
        grammar_sha256: sha256_hex(grammar_bytes),
        grammar_dependencies,
        request_sha256: executable_request_sha256(request)?,
        request_file_sha256: sha256_hex(request_bytes),
        build: crate::build_info::embedded_build_info().to_string(),
        limits: EffectiveLimits {
            scope: "orderedPhonologyFolds",
            step_cap: request.limits.step_cap,
            work_cap: request.limits.work_cap,
            timeout_ms: request.limits.timeout_ms,
            step_allocation: "balancedByRequestOrder",
            time_allocation: "sharedMonotonicDeadline",
        },
        measured_entry_count: u64::try_from(grammar.entries.len())
            .map_err(|_| "compiled entry count does not fit the result format".to_string())?,
        rows,
        counts,
        completion_counts,
    })
}

fn same_file(a: &Path, b: &Path) -> bool {
    let a = crate::path_key(a);
    let b = crate::path_key(b);
    crate::same_path(&a, &b)
}

fn read_request_bounded(path: &str) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("read {path}: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read {path}: {e}"))?;
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        return Err(format!(
            "request {path} exceeds the {MAX_REQUEST_BYTES}-byte limit"
        ));
    }
    Ok(bytes)
}

/// Written through a temporary sibling and renamed, so a reader never sees half a result.
fn write_atomically(out: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    let dir = match out.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| format!("create a temporary file beside {}: {e}", out.display()))?;
    temp.write_all(text.as_bytes())
        .and_then(|()| temp.as_file().sync_all())
        .map_err(|e| format!("write {}: {e}", out.display()))?;
    temp.persist(out)
        .map_err(|e| format!("replace {}: {}", out.display(), e.error))?;
    verify_written_output(out, text.as_bytes())
}

fn verify_written_output(out: &Path, expected: &[u8]) -> Result<(), String> {
    let actual = std::fs::read(out)
        .map_err(|e| format!("verify result {} after publication: {e}", out.display()))?;
    if actual != expected {
        return Err(format!(
            "result {} changed or was missing after publication",
            out.display()
        ));
    }
    Ok(())
}

pub(crate) fn run_phonology_examples(args: &[String]) -> Result<(), String> {
    if let Some(flag) = args.iter().find(|a| a.starts_with("--")) {
        return Err(format!("unknown option: {flag}"));
    }
    let (grammar_path, request_path, out_path) = match args {
        [g, r] => (g, r, None),
        [g, r, o] => (g, r, Some(o)),
        _ => {
            return Err(
                "usage: phonology-examples <grammar> <request.json> [<out.json>]".to_string(),
            )
        }
    };
    if let Some(out) = out_path {
        let out = Path::new(out);
        if out.is_dir() {
            return Err(format!("{} is a directory", out.display()));
        }
        for input in [grammar_path, request_path] {
            if same_file(out, Path::new(input)) {
                return Err(format!(
                    "refusing to overwrite the input {input} with the result"
                ));
            }
        }
    }
    let request_bytes = read_request_bounded(request_path)?;
    let request: Request = serde_json::from_slice(&request_bytes)
        .map_err(|e| format!("invalid request {request_path}: {e}"))?;
    validate(&request)?;
    let grammar_bytes =
        std::fs::read(grammar_path).map_err(|e| format!("read {grammar_path}: {e}"))?;
    let response = build_response(
        grammar_path,
        &grammar_bytes,
        &request_bytes,
        &request,
        out_path.map(|path| Path::new(path.as_str())),
    )?;
    let mut text = serde_json::to_string_pretty(&response)
        .map_err(|e| format!("serialize the result: {e}"))?;
    text.push('\n');
    if text.len() > MAX_RESPONSE_BYTES {
        return Err(format!(
            "serialized result exceeds the {MAX_RESPONSE_BYTES}-byte limit"
        ));
    }
    match out_path {
        Some(out) => write_atomically(Path::new(out), &text),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
