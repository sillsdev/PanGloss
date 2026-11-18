use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use serde::Deserialize;
use serde_json::{json, Value};

mod evidence;
use evidence::{Identity, Recorder};

pub(crate) const ERROR_PREFIX: &str = "PANGLOSS_DIAGNOSTIC_ERROR ";
pub(crate) const SCHEMA_VERSION: &str = "pangloss.diagnostic.v1";
const DEFAULT_PATHS: usize = 16;
const DEFAULT_DEPTH: usize = 32;
const DEFAULT_EVENTS: usize = 128;
const DEFAULT_BYTES: usize = 16 * 1024 * 1024;
const MAX_PATHS: usize = 64;
const MAX_EVENTS: usize = 4096;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const ROW_CONTROL_BYTES: usize = 3072;
const RUN_CONTROL_BYTES: usize = 4096;

pub(crate) const FLAGS: &[crate::surface::FlagSpec] = &[
    crate::surface::FlagSpec {
        name: "--trace-rejections",
        takes_value: true,
        summary: "write a bounded rejection diagnostic JSONL artifact",
    },
    crate::surface::FlagSpec {
        name: "--trace-options",
        takes_value: true,
        summary: "versioned rejection trace request metadata",
    },
    crate::surface::FlagSpec {
        name: "--trace-max-paths",
        takes_value: true,
        summary: "maximum retained local witnesses per row (default 16)",
    },
    crate::surface::FlagSpec {
        name: "--trace-max-events",
        takes_value: true,
        summary: "maximum retained events per row (default 128)",
    },
    crate::surface::FlagSpec {
        name: "--trace-max-bytes",
        takes_value: true,
        summary: "maximum encoded evidence bytes per invocation (default 16 MiB)",
    },
];

pub(crate) fn describe() -> Value {
    json!({
        "schemaVersion": SCHEMA_VERSION,
        "operations": ["parse", "batch"],
        "engine": "hc",
        "modes": ["observations"],
        "prefilter": ["off"],
        "selectors": [],
        "maxFeaturePathDepth": 0,
        "projections": ["parse_outputs", "authored_morphs"],
        "eventTuples": evidence::TUPLES,
        "options": FLAGS.iter().map(|flag| flag.name).collect::<Vec<_>>(),
        "unsupportedCombinations": ["legacy_trace", "fst", "prefilter", "stats", "cache", "stats_manifest"],
        "limits": {
            "maxPaths": {"min": 1, "default": DEFAULT_PATHS, "max": MAX_PATHS},
            "maxDepth": {"min": 1, "default": DEFAULT_DEPTH, "max": 256, "supported": false},
            "maxEvents": {"min": 1, "default": DEFAULT_EVENTS, "max": MAX_EVENTS},
            "maxBytes": {"min": 1024, "default": DEFAULT_BYTES, "max": MAX_BYTES},
            "rowControlBytes": ROW_CONTROL_BYTES,
            "runControlBytes": RUN_CONTROL_BYTES,
            "summaryBytes": 2048,
            "evidenceAllocation": "equal_rows_first_remainder"
        }
    })
}

pub(crate) fn error(code: &str, message: impl std::fmt::Display) -> String {
    let message = message.to_string();
    let mut end = message.len().min(255);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    let message: String = message[..end]
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    format!(
        "{ERROR_PREFIX}{}",
        json!({"code": code, "message": message})
    )
}

pub(crate) fn io_error(error_value: io::Error) -> String {
    error("io", error_value)
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    schema_version: String,
    expected_grammar: Option<ExpectedGrammar>,
    writing_system: Option<String>,
    spelling_policy: Option<SpellingPolicy>,
    pass_id: Option<String>,
    #[serde(default)]
    targets: Vec<RowRequest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExpectedGrammar {
    hash: String,
    semantics: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SpellingPolicy {
    Exact,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RowRequest {
    row: usize,
    request_id: Option<String>,
    selector: Option<TargetSelector>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TargetSelector {
    root_index: usize,
    morphs: Vec<MorphSelector>,
    #[serde(default)]
    features: Vec<FeatureSelector>,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum MorphSelector {
    Authored {
        entry_guid: String,
        msa_guid: String,
        sense_guid: Option<String>,
        form_guids: Option<Vec<String>>,
        infl_type_guid: Option<String>,
        parts: Option<Vec<PartSelector>>,
    },
    StructuralZero {
        infl_type_guid: String,
    },
    Guessed,
    Unavailable,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PartSelector {
    form_guid: String,
    placement: Placement,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Placement {
    Prefix,
    Suffix,
    Infix,
    Root,
    CircumfixLeft,
    CircumfixRight,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FeatureSelector {
    domain: FeatureDomain,
    feature_path_guids: Vec<String>,
    value_guids: Vec<String>,
    #[serde(rename = "match")]
    relation: FeatureMatch,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum FeatureDomain {
    Syntactic,
    Realizational,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum FeatureMatch {
    Exact,
}

impl TargetSelector {
    fn valid(&self) -> bool {
        self.root_index < self.morphs.len()
            && self.morphs.iter().all(MorphSelector::valid)
            && self.features.iter().all(FeatureSelector::valid)
    }
}

fn is_guid(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(i, byte)| match i {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
        })
}

impl MorphSelector {
    fn valid(&self) -> bool {
        match self {
            Self::Authored {
                entry_guid,
                msa_guid,
                sense_guid,
                form_guids,
                infl_type_guid,
                parts,
            } => {
                is_guid(entry_guid)
                    && is_guid(msa_guid)
                    && sense_guid.as_deref().is_none_or(is_guid)
                    && infl_type_guid.as_deref().is_none_or(is_guid)
                    && form_guids.iter().flatten().all(|guid| is_guid(guid))
                    && parts.iter().flatten().all(|part| {
                        let _placement = &part.placement;
                        is_guid(&part.form_guid)
                    })
            }
            Self::StructuralZero { infl_type_guid } => is_guid(infl_type_guid),
            Self::Guessed | Self::Unavailable => true,
        }
    }
}

impl FeatureSelector {
    fn valid(&self) -> bool {
        let _domain = &self.domain;
        let _relation = &self.relation;
        let distinct = self.value_guids.iter().collect::<BTreeSet<_>>();
        !self.feature_path_guids.is_empty()
            && !distinct.is_empty()
            && distinct.len() == self.value_guids.len()
            && self
                .feature_path_guids
                .iter()
                .chain(&self.value_guids)
                .all(|guid| is_guid(guid))
    }
}

pub(crate) struct Options<'a> {
    published: &'a AtomicBool,
    dest: PathBuf,
    request_path: Option<PathBuf>,
    request: Request,
    paths: usize,
    events: usize,
    bytes: usize,
}

/// The ordinary invocation reuses its argument slice without constructing trace state.
pub(crate) fn with_options<T>(
    args: &[String],
    run: impl FnOnce(&[String], Option<Options<'_>>) -> Result<T, String>,
) -> Result<T, String> {
    fn name(arg: &str) -> &str {
        arg.split_once('=').map_or(arg, |(name, _)| name)
    }
    fn trace_flag(flag: &str) -> bool {
        matches!(
            flag,
            "--trace-rejections"
                | "--trace-options"
                | "--trace-max-paths"
                | "--trace-max-events"
                | "--trace-max-bytes"
                | "--trace-max-depth"
        )
    }
    if !args.iter().any(|arg| trace_flag(name(arg))) {
        return run(args, None);
    }
    let mut forwarded = Vec::with_capacity(args.len());
    let mut flags = BTreeMap::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let flag = name(arg);
        if !trace_flag(flag) {
            forwarded.push(arg.clone());
            continue;
        }
        let value = arg
            .split_once('=')
            .map(|(_, value)| value)
            .or_else(|| {
                args.next()
                    .filter(|value| !value.starts_with("--"))
                    .map(String::as_str)
            })
            .ok_or_else(|| error("bad_request", format!("{flag} requires a value")))?;
        if flags.insert(flag, value).is_some() {
            return Err(error("bad_request", format!("duplicate {flag}")));
        }
    }
    let dest = flags
        .get("--trace-rejections")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            error(
                "bad_request",
                "trace options require --trace-rejections <file>",
            )
        })?;
    if flags.contains_key("--trace-max-depth") {
        return Err(error(
            "unsupported",
            "--trace-max-depth requires lineage coverage",
        ));
    }
    let cap = |flag: &str, default, min, max| {
        flags.get(flag).map_or(Ok(default), |value| {
            value
                .parse::<usize>()
                .ok()
                .filter(|n| (min..=max).contains(n))
                .ok_or_else(|| error("bad_request", format!("{flag} must be in {min}..={max}")))
        })
    };
    let request_path = flags.get("--trace-options").map(PathBuf::from);
    let request = match &request_path {
        Some(path) => {
            let data = fs::read(path).map_err(io_error)?;
            let request: Request =
                serde_json::from_slice(&data).map_err(|e| error("bad_request", e))?;
            if request.schema_version != SCHEMA_VERSION {
                return Err(error(
                    "unsupported",
                    "unrecognized diagnostic request schema",
                ));
            }
            request.validate()?;
            request
        }
        None => Request::default(),
    };
    if forwarded.iter().any(|arg| arg == "--engine=fst")
        || forwarded
            .windows(2)
            .any(|args| args[0] == "--engine" && args[1] == "fst")
    {
        return Err(error(
            "unsupported",
            "rejection trace requires the HC engine",
        ));
    }
    if forwarded.iter().any(|arg| {
        matches!(
            name(arg),
            "--stats" | "--stats-compact" | "--cache" | "--cache-pack" | "--stats-manifest"
        ) || matches!(arg.as_str(), "--prefilter=on" | "--prefilter=auto")
    }) || forwarded
        .windows(2)
        .any(|args| args[0] == "--prefilter" && matches!(args[1].as_str(), "on" | "auto"))
    {
        return Err(error(
            "unsupported",
            "rejection trace requires cold HC with prefilter off",
        ));
    }
    let published = AtomicBool::new(false);
    run(
        &forwarded,
        Some(Options {
            published: &published,
            dest: PathBuf::from(dest),
            request_path,
            request,
            paths: cap("--trace-max-paths", DEFAULT_PATHS, 1, MAX_PATHS)?,
            events: cap("--trace-max-events", DEFAULT_EVENTS, 1, MAX_EVENTS)?,
            bytes: cap("--trace-max-bytes", DEFAULT_BYTES, 1024, MAX_BYTES)?,
        }),
    )
    .map_err(|failure| {
        if published.load(Ordering::Relaxed) || failure.starts_with(ERROR_PREFIX) {
            failure
        } else {
            error("bad_request", failure)
        }
    })
}

impl Request {
    fn validate(&self) -> Result<(), String> {
        let id_fits = |id: &str| {
            id.len() <= 128 && serde_json::to_vec(id).expect("serialize string").len() <= 256
        };
        let _spelling_policy = &self.spelling_policy;
        let mut rows = BTreeSet::new();
        if self.pass_id.as_deref().is_some_and(|id| !id_fits(id))
            || self
                .writing_system
                .as_ref()
                .is_some_and(|ws| ws.len() > 128)
            || self.targets.iter().any(|target| {
                !rows.insert(target.row)
                    || target.request_id.as_deref().is_some_and(|id| !id_fits(id))
                    || target
                        .selector
                        .as_ref()
                        .is_some_and(|selector| !selector.valid())
            })
        {
            return Err(error(
                "bad_request",
                "invalid trace metadata or duplicate row ordinal",
            ));
        }
        if let Some(grammar) = &self.expected_grammar {
            if !matches!(
                grammar.semantics.as_str(),
                "snapshot-semantic-sha256-v1" | "source-bytes-sha256-v1"
            ) || grammar.hash.len() != 64
                || !grammar
                    .hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(error(
                    "bad_request",
                    "invalid expected grammar digest or semantics",
                ));
            }
        }
        Ok(())
    }
}

impl Options<'_> {
    pub(crate) fn distinct_from<'a>(
        &'a self,
        others: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), String> {
        let destination = crate::path_key(&self.dest);
        if others
            .into_iter()
            .map(Path::new)
            .chain(self.request_path.as_deref())
            .any(|other| crate::same_path(&destination, &crate::path_key(other)))
        {
            return Err(error(
                "bad_request",
                "--trace-rejections must have a distinct destination",
            ));
        }
        Ok(())
    }
}

pub(crate) struct SearchOptions {
    pub guess: bool,
    pub final_templates: bool,
    pub work_cap: crate::WorkCap,
    pub timeout_ms: Option<u64>,
    pub memory_bytes: Option<usize>,
    pub capture_bytes: Option<Option<usize>>,
}

impl SearchOptions {
    fn json(&self) -> Value {
        fn limit(value: Option<impl serde::Serialize>) -> Value {
            value.map_or_else(
                || json!({"state": "unbounded"}),
                |n| json!({"state": "bounded", "value": n}),
            )
        }
        let work_cap = match self.work_cap {
            crate::WorkCap::Finite(n) => limit(Some(n.get())),
            crate::WorkCap::Unbounded => limit(None::<usize>),
        };
        json!({
            "guessRoots": self.guess,
            "alwaysEnforceFinalTemplates": self.final_templates,
            "prefilter": "off",
            "workCap": work_cap,
            "wordTimeoutMs": limit(self.timeout_ms),
            "wordMemoryCapBytes": limit(self.memory_bytes),
            "searchCaptureCapBytes": self.capture_bytes.map_or_else(
                || json!({"state": "unsupported"}), limit,
            )
        })
    }
}

pub(crate) struct Scope<'a> {
    pub source: &'a str,
    pub row_count: usize,
    pub start: usize,
}

pub(crate) struct Run<'a> {
    published: &'a AtomicBool,
    temporary: tempfile::NamedTempFile,
    dest: PathBuf,
    identity: Identity<'a>,
    targets: BTreeMap<usize, RowRequest>,
    paths: usize,
    events: usize,
    bytes: usize,
    start: usize,
    row_count: usize,
    processed_rows: usize,
    statuses: [usize; 4],
    totals: Tally,
    reason_counts: BTreeMap<&'static str, usize>,
    header_bytes: usize,
}

pub(crate) struct Row {
    ordinal: usize,
    evidence: Vec<u8>,
    status: Status,
    stops: Vec<&'static str>,
    work: usize,
    outputs: usize,
    tally: Tally,
    reason_counts: BTreeMap<&'static str, usize>,
}

#[derive(Clone, Copy)]
enum Status {
    Complete,
    Stopped,
    Invalid,
    Refused,
}

impl Status {
    fn name(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Stopped => "stopped",
            Self::Invalid => "invalid",
            Self::Refused => "refused",
        }
    }

    const ALL: [Self; 4] = [Self::Complete, Self::Stopped, Self::Invalid, Self::Refused];
}

#[derive(Default)]
struct Tally {
    bytes: usize,
    paths_cut: bool,
    events_cut: bool,
    bytes_cut: bool,
    omitted_paths: usize,
    omitted_events: usize,
    omitted_outputs: usize,
    omitted_inputs: usize,
}

impl Tally {
    fn collect(&mut self, other: &Self) {
        self.bytes += other.bytes;
        self.paths_cut |= other.paths_cut;
        self.events_cut |= other.events_cut;
        self.bytes_cut |= other.bytes_cut;
        self.omitted_paths = self.omitted_paths.saturating_add(other.omitted_paths);
        self.omitted_events = self.omitted_events.saturating_add(other.omitted_events);
        self.omitted_outputs = self.omitted_outputs.saturating_add(other.omitted_outputs);
        self.omitted_inputs = self.omitted_inputs.saturating_add(other.omitted_inputs);
    }

    fn json(&self) -> Value {
        let count = |value| json!({"value": value, "accuracy": "exact"});
        let truncated: Vec<_> = [
            (self.paths_cut, "paths"),
            (self.events_cut, "events"),
            (self.bytes_cut, "bytes"),
        ]
        .into_iter()
        .filter_map(|(cut, limit)| cut.then_some(limit))
        .collect();
        json!({
            "truncatedBy": truncated,
            "omittedPaths": count(self.omitted_paths),
            "omittedEvents": count(self.omitted_events),
            "omittedOutputs": count(self.omitted_outputs),
            "omittedInputs": count(self.omitted_inputs)
        })
    }
}

impl<'a> Run<'a> {
    pub(crate) fn begin(
        options: Options<'a>,
        identity: Identity<'a>,
        search: SearchOptions,
        scope: Scope<'_>,
    ) -> Result<Self, String> {
        let metadata = identity.metadata;
        let source_kind = identity.source_kind();
        let hash = identity.hash;
        let semantics = identity.semantics;
        if let Some(expected) = &options.request.expected_grammar {
            if expected.hash != hash || expected.semantics != semantics {
                return Err(error(
                    "grammar_mismatch",
                    format!("loaded {semantics} hash {hash}"),
                ));
            }
        }
        if let Some(ws) = &options.request.writing_system {
            match metadata.vernacular_writing_systems.as_slice() {
                [actual] if actual == ws => {}
                [_] => {
                    return Err(error(
                        "grammar_mismatch",
                        "writing system differs from the imported grammar",
                    ))
                }
                _ => {
                    return Err(error(
                        "unsupported",
                        "writing system expectation is unverifiable for this import",
                    ))
                }
            }
        }
        let start = scope.start.min(scope.row_count);
        let row_count = scope.row_count - start;
        if options
            .request
            .targets
            .iter()
            .any(|target| target.row >= scope.row_count)
        {
            return Err(error("bad_request", "trace target row outside input scope"));
        }
        row_count
            .checked_mul(ROW_CONTROL_BYTES)
            .and_then(|size| size.checked_add(options.bytes + RUN_CONTROL_BYTES))
            .ok_or_else(|| error("resource", "diagnostic artifact size bound overflows"))?;
        let parent = options
            .dest
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let temporary = tempfile::Builder::new()
            .prefix(".pangloss-diagnostic-")
            .tempfile_in(parent)
            .map_err(io_error)?;
        let mut run = Self {
            published: options.published,
            temporary,
            dest: options.dest,
            identity,
            targets: options
                .request
                .targets
                .into_iter()
                .map(|row| (row.row, row))
                .collect(),
            paths: options.paths,
            events: options.events,
            bytes: options.bytes,
            start,
            row_count,
            processed_rows: 0,
            statuses: [0; 4],
            totals: Tally::default(),
            reason_counts: BTreeMap::new(),
            header_bytes: 0,
        };
        let header = json!({
            "type": "header", "schemaVersion": SCHEMA_VERSION, "operation": "parse",
            "mode": "observations", "engine": "hc",
            "parserVersion": env!("CARGO_PKG_VERSION"), "parserBuild": env!("PANGLOSS_BUILD_REVISION"),
            "grammar": {"hash": hash, "semantics": semantics, "sourceKind": source_kind},
            "searchOptions": search.json(),
            "captureLimits": {"maxPaths": run.paths, "maxDepth": null, "maxEvents": run.events, "maxBytes": run.bytes},
            "scope": {
                "startRow": start, "sourceRowCount": scope.row_count, "requestedRowCount": row_count,
                "passId": options.request.pass_id, "inputSha256": crate::stats_manifest::sha256_hex(scope.source.as_bytes()),
                "writingSystem": match metadata.vernacular_writing_systems.as_slice() {
                    [actual] if actual.len() <= 128 => Some(actual.as_str()),
                    _ => None,
                },
                "spellingPolicy": "exact"
            }
        });
        run.header_bytes = run.write_control(header, RUN_CONTROL_BYTES)?;
        Ok(run)
    }

    fn write_control(&mut self, record: Value, limit: usize) -> Result<usize, String> {
        let mut encoded = serde_json::to_vec(&record).expect("serialize diagnostic control");
        encoded.push(b'\n');
        if encoded.len() > limit {
            return Err(error(
                "resource",
                "encoded diagnostic control exceeds its reserve",
            ));
        }
        self.temporary
            .as_file_mut()
            .write_all(&encoded)
            .map_err(io_error)?;
        Ok(encoded.len())
    }

    pub(crate) fn parse(
        &self,
        morpher: &Morpher<'_>,
        opts: &ParseOptions,
        ordinal: usize,
        word: &str,
    ) -> (ParseOutcome, Row) {
        let relative = ordinal - self.start;
        // Reserve deterministic, disjoint portions of the invocation budget in input order.
        let allowance =
            self.bytes / self.row_count + usize::from(relative < self.bytes % self.row_count);
        let mut recorder =
            Recorder::new(&self.identity, ordinal, self.paths, self.events, allowance);
        let request_id = self
            .targets
            .get(&ordinal)
            .and_then(|row| row.request_id.as_deref());
        recorder.input(word, request_id);
        let (outcome, work_limited) = morpher.parse_word_observed(word, opts, &recorder);
        let evidence = recorder.complete(&outcome, work_limited);
        (outcome, evidence)
    }

    pub(crate) fn write_row(&mut self, row: Row) -> Result<(), String> {
        if row.ordinal != self.start + self.processed_rows || self.processed_rows >= self.row_count
        {
            return Err(error(
                "bad_request",
                "diagnostic row arrived out of scope or order",
            ));
        }
        self.temporary
            .as_file_mut()
            .write_all(&row.evidence)
            .map_err(io_error)?;
        let request = self.targets.get(&row.ordinal);
        let target = if request.is_some_and(|r| r.selector.is_some()) {
            json!({"status": "unknown", "membership": "unknown", "coverage": "not_assessed",
                "limitations": ["selector_unassessed"], "error": {"code": "unsupported_selector"}})
        } else {
            json!({"status": "not_requested", "membership": "not_requested", "coverage": "not_assessed",
                "limitations": [], "error": null})
        };
        let mut capture = row.tally.json();
        capture["coverage"] = json!("observations");
        let complete = json!({
            "type": "word_end", "row": row.ordinal,
            "requestId": request.and_then(|r| r.request_id.as_deref()),
            "search": {
                "status": row.status.name(), "stopReasons": row.stops, "workUnits": row.work,
                "outputCount": row.outputs, "outputCensus": if matches!(row.status, Status::Stopped) {"partial"} else {"complete"},
                "error": null
            },
            "capture": capture, "target": target
        });
        self.write_control(complete, ROW_CONTROL_BYTES)?;
        self.processed_rows += 1;
        self.statuses[row.status as usize] += 1;
        self.totals.collect(&row.tally);
        for (reason, count) in row.reason_counts {
            *self.reason_counts.entry(reason).or_default() += count;
        }
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Result<(), String> {
        if self.row_count != self.processed_rows || self.totals.bytes > self.bytes {
            return Err(error(
                "resource",
                "diagnostic row or evidence accounting failed",
            ));
        }
        let mut capture = self.totals.json();
        capture["evidenceBytes"] = json!(self.totals.bytes);
        let search_statuses: BTreeMap<_, _> = Status::ALL
            .into_iter()
            .zip(self.statuses)
            .map(|(status, count)| (status.name(), count))
            .collect();
        let footer = json!({
            "type": "footer", "status": "complete", "processedRows": self.processed_rows,
            "unprocessedRows": 0, "searchStatuses": search_statuses, "error": null, "capture": capture
        });
        self.write_control(footer, RUN_CONTROL_BYTES - self.header_bytes)?;
        self.temporary.as_file_mut().flush().map_err(io_error)?;
        self.temporary
            .persist(&self.dest)
            .map_err(|failure| io_error(failure.error))?;
        self.published.store(true, Ordering::Relaxed);

        let mut reasons: Vec<_> = self.reason_counts.into_iter().collect();
        reasons.sort_unstable_by_key(|(reason, count)| (std::cmp::Reverse(*count), *reason));
        let mut summary = format!(
            "rejection trace: {} complete, {} stopped, {} invalid, {} refused; capture truncated={}",
            self.statuses[0], self.statuses[1], self.statuses[2], self.statuses[3],
            self.totals.paths_cut || self.totals.events_cut || self.totals.bytes_cut,
        );
        for (reason, count) in reasons {
            let entry = format!(" {reason}={count}");
            if summary.len() + entry.len() >= 2048 {
                break;
            }
            summary.push_str(&entry);
        }
        eprintln!("{summary}");
        Ok(())
    }
}

pub(crate) fn identity<'a>(
    grammar: &'a Grammar,
    metadata: &'a crate::rich_trace::TraceMetadata,
    slots: &'a evidence::SlotBindings,
) -> Result<Identity<'a>, String> {
    Identity::new(grammar, metadata, slots)
}

pub(crate) use evidence::SlotBindings;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worst_case_control_records_fit_their_wire_reserves() {
        let wide = u64::MAX;
        let request_id = "\"".repeat(127);
        let err = json!({"code": "unsupported_selector", "message": "\\".repeat(255)});
        let mut capture = Tally::default().json();
        for key in [
            "omittedEvents",
            "omittedInputs",
            "omittedOutputs",
            "omittedPaths",
        ] {
            capture[key] = json!({"value": wide, "accuracy": "lowerBound"});
        }
        capture["coverage"] = json!("observations");
        capture["truncatedBy"] = json!(["paths", "depth", "events", "bytes"]);
        let word_end = json!({
            "type": "word_end", "row": wide, "requestId": request_id,
            "search": {"status": "complete",
                "stopReasons": ["work_limit", "time_limit", "search_capture_limit", "word_memory_limit",
                    "seed_product_limit", "cancelled"],
                "workUnits": wide, "outputCount": wide, "outputCensus": "unavailable", "error": err},
            "target": {"status": "not_requested", "membership": "not_requested", "coverage": "not_assessed",
                "limitations": ["search_stopped", "identity_unavailable", "lineage_lost", "owner_unobserved",
                    "selector_unassessed", "witness_truncated", "sense_ambiguous"],
                "error": err, "matchedOutputIds": (0..16).map(|i| wide - i).collect::<Vec<_>>(),
                "matchCount": {"value": wide, "accuracy": "lowerBound"}},
            "capture": capture, "expectedSurfaceComparison": "not_requested",
            "enumeration": {"selectorResolution": "unavailable", "chains": "unavailable"}
        });
        assert!(serde_json::to_vec(&word_end).unwrap().len() < ROW_CONTROL_BYTES);
        assert!(serde_json::to_vec(&word_end).unwrap().len() >= 1024);

        let resource = json!({"state": "bounded", "value": wide});
        let digest = "a".repeat(64);
        let header = json!({
            "type": "header", "schemaVersion": SCHEMA_VERSION, "operation": "generate",
            "mode": "observations", "engine": "hc", "parserVersion": "\"".repeat(128),
            "parserBuild": "\"".repeat(128),
            "grammar": {"hash": digest, "semantics": "snapshot-semantic-sha256-v1", "sourceKind": "snapshot"},
            "searchOptions": {"guessRoots": false, "alwaysEnforceFinalTemplates": false, "prefilter": "auto",
                "workCap": resource, "wordTimeoutMs": resource, "wordMemoryCapBytes": resource,
                "searchCaptureCapBytes": resource},
            "captureLimits": {"maxPaths": 64, "maxDepth": 256, "maxEvents": 4096, "maxBytes": 67108864},
            "scope": {"startRow": wide, "sourceRowCount": wide, "requestedRowCount": wide,
                "passId": request_id, "inputSha256": digest, "writingSystem": "\u{0001}".repeat(128),
                "spellingPolicy": "exact"}
        });
        let mut footer_capture = capture;
        footer_capture.as_object_mut().unwrap().remove("coverage");
        footer_capture["evidenceBytes"] = json!(wide);
        let footer = json!({
            "type": "footer", "status": "refused", "processedRows": wide, "unprocessedRows": wide,
            "searchStatuses": {"complete": wide, "stopped": wide, "invalid": wide, "refused": wide},
            "error": err, "capture": footer_capture
        });
        assert!(
            serde_json::to_vec(&header).unwrap().len()
                + serde_json::to_vec(&footer).unwrap().len()
                + 2
                <= RUN_CONTROL_BYTES
        );
    }
}
