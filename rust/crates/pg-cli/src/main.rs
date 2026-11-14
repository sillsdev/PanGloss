//! `pangloss` — the standalone CLI mirroring C# `hc batch`'s TSV protocol so parity diffs against
//! managed golden runs are line-for-line comparable.
//!
//! `batch <grammar.xml> <words.txt> <out.tsv> [--work-cap N|unbounded] [--word-mem-cap-mib N] [--capture-cap-mib N|unbounded] [--word-timeout-ms N] [--threads N] [--analyses <path>] [--always-enforce-final-templates[=true|false]]`
//! loads the grammar once and parses every word, writing the `BatchCommand`-compatible TSV.
//! `--analyses` additionally writes one FieldWorks `ParseAnalysis` JSONL row per input case while
//! preserving partial projections when a cap or timeout fires.
//! `--work-cap N` bounds the work of one word: the running total of work steps across analysis,
//! lookup and confirmation. Omitted, it defaults to `DEFAULT_WORK_CAP` (5,000,000,000) so every
//! batch terminates deterministically; `--work-cap unbounded` opts back into no bound at all.
//! See `docs/research/per-word-search-work-cap.md` for its counting contract and evidence.
//! `--word-mem-cap-mib` independently bounds counted retained parser state (100 MiB by default;
//! 0 disables it). A stopped word reports `MEMCAP`, and the batch continues.
//!
//! ## `--word-timeout-ms`
//! A second, independent bound: `--work-cap` bounds the *amount* of work, but work is not a
//! time measure — some pathological words legitimately spend far longer per work step than
//! others (heavier narrowing/expansion analysis), so a work cap alone cannot bound wall-clock
//! time per word. `--word-timeout-ms N` arms a wall-clock deadline on the same shared
//! `pg_rules::stratum::WordBudget` the work cap already uses; whichever bound fires first wins,
//! and each is reported distinctly — a work-cap-exhausted word writes a `CAP` status row (its
//! signature column is the partial, unconfirmed signature reached before the cap fired, kept for
//! inspection, never a result) alongside the same `CAP` marker on stderr, while a timed-out word
//! writes a `TIMEOUT` row with signature `-`. Omitted (the default) is a complete no-op: no clock
//! is ever read, and every existing invocation's output is unchanged.
//!
//! ## `--threads` and the two TSV-writing modes
//! C#'s own `BatchCommand` has two mutually exclusive dispatch modes with genuinely different TSV
//! behavior:
//! - `RunSequential` (no `--parallel` flag): one word at a time, each line written and flushed
//!   immediately, preceded by a `{idx}\t{word}\tSTARTED` sentinel — crash-resumable, needed by an
//!   interruptible, resumable full-corpus run (e.g. an overnight batch that has historically
//!   crashed a host mid-run).
//! - `RunParallel` (`--parallel[:N]`, `Parallel.ForEach`): results are buffered into an
//!   index-ordered array (`rows[i] = ...`) and the whole file is written **once, sequentially, in
//!   original order** only after every word has finished. No `STARTED` line is ever written in
//!   this mode — there is nothing to resume mid-run.
//!
//! `--threads N` here maps onto that split by **value**, not by flag presence: `--threads 1`
//! (the default-shaped case) keeps the exact legacy per-line/`STARTED`/flush loop — preserving
//! crash-resumability for exactly this kind of long-running resumable batch. `--threads N` for
//! `N > 1` routes through `pg_parse::hc_parse_batch` and writes the buffered result in original
//! order with no `STARTED` lines, mirroring `RunParallel` exactly. This is a deliberate Rust-side
//! choice (C# keys the split on flag presence; we key it on thread count).
//!
//! ## `import` and `.json`/`.fwdata`/`.fwbackup` grammar dispatch
//! `import <project.fwdata/.fwbackup> <out.json>` runs `pg_fwdata::import_file` and writes the resulting
//! `pg_snapshot::Snapshot::to_json()` to `<out.json>`. `ImportReport` warnings (dangling refs,
//! unsupported constructs, log-and-skip decisions) and `Snapshot::validate()` warnings (dangling
//! GUID cross-references *within* the snapshot) are printed to stderr, clearly labeled and kept
//! separate since they come from different stages of the pipeline. A fatal provenance issue
//! refuses publication because the resulting snapshot omitted required source data; nonfatal
//! warnings remain visible while import can complete.
//!
//! ## `fst-health` (see `fst_health.rs`'s own doc for the full contract)
//! `fst-health <grammar> [<out.json>]` runs the cheap, grammar-only
//! `pg_foma::characterization::characterization_findings` pass and writes one canonical
//! `pg_foma_backend::health::HealthReport`. It never compiles a backend or evaluates a corpus; corpus
//! measurements belong to a separate post-build operation over an explicitly completed artifact.
//! `<out.json>` omitted prints the JSON to stdout.
//!
//! ## `grammar-health` (see `grammar_health.rs`'s own doc for the full contract)
//! `grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids]
//! [--stored-analyses bounded|all|off]` runs the ported
//! `hc-*` HermitCrab grammar-authoring checks
//! (`pg_grammar::grammar_health::check_grammar_health`) and prints/writes a versioned JSON
//! report. A separate report from `fst-health`: this one asks whether the grammar is
//! well-formed for its author, not whether a compiled FST is production-ready.
//!
//! Every other subcommand that takes a grammar path (`parse`, `batch`, `generate`)
//! now dispatches on the path's extension via `load_grammar`: `.xml` (or anything else) is the
//! legacy HC-XML path (`pg_grammar::load`, unchanged, no warnings); `.json` loads a `pg-snapshot`
//! `Snapshot` (`Snapshot::from_json`) and compiles it (`pg_grammar::compile_project`); `.fwdata/.fwbackup`
//! imports the FieldWorks project file directly, in-memory, then compiles it -- no intermediate
//! JSON file is written (run the `import` subcommand first if you want to keep the snapshot
//! around, e.g. to inspect it or reuse it without re-importing every time). Compile/import
//! warnings from `.json`/`.fwdata/.fwbackup` dispatch are always printed to stderr, never stdout --
//! `batch`'s TSV rows are parity-sensitive against C# goldens, so warnings must never be
//! interleaved into that output stream.
// `forbid` relaxes only under `alloc-trace` (dev-only, default off): its counting allocator needs one `unsafe impl GlobalAlloc`.
#![cfg_attr(not(feature = "alloc-trace"), forbid(unsafe_code))]

#[cfg(feature = "alloc-trace")]
mod alloc_trace;
mod analyze_text_cmd;
mod batch_prefilter;
#[cfg(test)]
mod test_support;

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

#[cfg(test)]
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use pg_grammar::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef};
use pg_parse::{
    hc_parse_batch,
    parse_morph::{project_parse_analysis, ParseAnalysis, PARSE_ANALYSIS_PROFILE},
    GenMorpheme, Morpher, WordAnalysis,
};
use pg_stats::WorkCap;

#[derive(Clone, Default)]
struct BatchParseCounter {
    #[cfg(test)]
    fires: Arc<AtomicUsize>,
}

impl BatchParseCounter {
    fn record(&self) {
        #[cfg(test)]
        self.fires.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(test)]
    fn count(&self) -> usize {
        self.fires.load(Ordering::Relaxed)
    }
}

pub(crate) mod build_info;
mod compile_failure;
#[cfg(feature = "cost-calibration")]
mod cost_calibrate;
mod cost_compare;
mod facts_cmd;
mod generate_request;
// `pub` changes nothing for a binary crate; it marks these moved library modules' long docs as interface for comment-hygiene.
#[cfg(feature = "foma-tools")]
pub mod backend_report;
#[cfg(feature = "foma-tools")]
mod coverage;
#[cfg(feature = "foma-tools")]
mod fst_health;
// `pub`: see `backend_report`'s note above -- marks this bin module's long docs as interface for comment-hygiene.
pub mod grammar_health;
#[cfg(feature = "foma-tools")]
mod make_report;
mod pack;
mod phonology_examples;
#[cfg(feature = "foma-tools")]
mod plan_diagram;
#[cfg(feature = "foma-tools")]
pub mod readiness_policy;
#[cfg(feature = "foma-tools")]
pub mod readiness_verdict;
#[cfg(feature = "foma-tools")]
mod recipe_optimize;
mod rich_trace;
mod serve;
mod stats_cmd;
mod stats_manifest;
mod stored_analysis_health;
mod surface;
mod trace_render;

/// Accepts experimental FST controls only in `developer-tools` builds, before positional parsing.
#[cfg(feature = "foma-tools")]
fn accept_developer_flag(arg: &str) -> Result<(), String> {
    debug_assert!(matches!(arg, "--allow-unproven"));
    #[cfg(feature = "developer-tools")]
    {
        let _ = arg;
        Ok(())
    }
    #[cfg(not(feature = "developer-tools"))]
    {
        Err(format!(
            "unknown option: {arg} (developer-tools feature required)"
        ))
    }
}

/// `command`'s own `surface::CommandSpec`, looked up by name, judges `arg`.
fn reject_unknown_option(command: &str, arg: &str) -> Result<(), String> {
    let spec = surface::find_command(command)
        .unwrap_or_else(|| panic!("`{command}` must have a surface::COMMANDS row"));
    spec.reject_unknown_option(arg)
}

/// The result status shared by batch and serve; timeout counts are wall-clock dependent.
fn outcome_status(outcome: &pg_parse::ParseOutcome) -> &'static str {
    if outcome.invalid_shape {
        "SKIPPED"
    } else if outcome.timed_out {
        "TIMEOUT"
    } else if outcome.capture_capped || outcome.word_mem_capped {
        "MEMCAP"
    } else if outcome.capped {
        "CAP"
    } else {
        "ok"
    }
}

#[derive(serde::Serialize)]
struct ParseAnalysisBatchRow {
    schema: &'static str,
    index: usize,
    word: String,
    #[serde(rename = "elapsedMs")]
    elapsed_ms: u128,
    capped: bool,
    #[serde(rename = "timedOut")]
    timed_out: bool,
    #[serde(rename = "invalidShape")]
    invalid_shape: bool,
    analyses: Vec<ParseAnalysis>,
    unavailable: Vec<String>,
}

fn projected_analyses(
    outcome: &pg_parse::ParseOutcome,
    grammar: &Grammar,
) -> (Vec<ParseAnalysis>, Vec<String>) {
    if outcome.invalid_shape {
        return (Vec::new(), Vec::new());
    }
    let mut analyses = Vec::with_capacity(outcome.structured.len());
    let mut unavailable = Vec::new();
    for analysis in &outcome.structured {
        match project_parse_analysis(analysis, grammar) {
            Ok(projected) => analyses.push(projected),
            Err(error) => unavailable.push(error.to_string()),
        }
    }
    (analyses, unavailable)
}

fn write_parse_analysis_row<W: Write>(
    w: &mut W,
    index: usize,
    word: &str,
    elapsed_ms: u128,
    outcome: &pg_parse::ParseOutcome,
    grammar: &Grammar,
) -> std::io::Result<()> {
    let (analyses, unavailable) = projected_analyses(outcome, grammar);
    let row = ParseAnalysisBatchRow {
        schema: PARSE_ANALYSIS_PROFILE,
        index,
        word: word.to_string(),
        elapsed_ms,
        capped: outcome.capped,
        timed_out: outcome.timed_out,
        invalid_shape: outcome.invalid_shape,
        analyses,
        unavailable,
    };
    serde_json::to_writer(&mut *w, &row)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    w.write_all(b"\n")
}

fn path_key(path: &std::path::Path) -> std::path::PathBuf {
    if path.exists() {
        return fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    }
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let parent = if parent.is_absolute() {
        parent.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(parent)
    };
    let parent = fs::canonicalize(&parent).unwrap_or(parent);
    parent.join(path.file_name().unwrap_or_default())
}

fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    if left == right {
        return true;
    }
    // A hard link gives one file two paths, so existing files are compared by identity, not spelling.
    if let (Some(left_id), Some(right_id)) = (
        pg_worker_containment::file_identity(left),
        pg_worker_containment::file_identity(right),
    ) {
        return left_id == right_id;
    }
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn reject_analysis_path_collisions(
    grammar_path: &str,
    words_path: &str,
    out_path: &str,
    analyses_path: &str,
    cache_path: Option<&str>,
) -> Result<(), String> {
    let analyses_key = path_key(std::path::Path::new(analyses_path));
    for (label, path) in [
        ("grammar", grammar_path),
        ("word list", words_path),
        ("TSV output", out_path),
    ] {
        let other_key = path_key(std::path::Path::new(path));
        if same_path(&analyses_key, &other_key) {
            return Err(format!(
                "--analyses path cannot overwrite {label} path: {analyses_path}"
            ));
        }
    }
    if let Some(cache_path) = cache_path {
        let cache_key = path_key(std::path::Path::new(cache_path));
        if same_path(&analyses_key, &cache_key) {
            return Err(format!(
                "--analyses path cannot overwrite stats cache path: {analyses_path}"
            ));
        }
    }
    Ok(())
}

fn reject_frozen_stats_path_collisions(
    grammar_path: &str,
    words_path: &str,
    out_path: &str,
    analyses_path: Option<&str>,
    cache_path: &str,
    manifest_path: &str,
) -> Result<(), String> {
    let mut paths = vec![
        ("grammar", grammar_path),
        ("word list", words_path),
        ("TSV output", out_path),
        ("stats cache", cache_path),
        ("stats manifest", manifest_path),
    ];
    if let Some(analyses_path) = analyses_path {
        paths.push(("analyses output", analyses_path));
    }
    for (index, (label, path)) in paths.iter().enumerate() {
        let key = path_key(std::path::Path::new(path));
        for (other_label, other_path) in paths.iter().skip(index + 1) {
            if same_path(&key, &path_key(std::path::Path::new(other_path))) {
                return Err(format!(
                    "stats manifest path collision: {label} and {other_label} resolve to the same path"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(feature = "developer-tools")]
const REPORT_DEVELOPER_HELP: &str = " [--allow-unproven]";
#[cfg(not(feature = "developer-tools"))]
const REPORT_DEVELOPER_HELP: &str = "";

/// A runaway guard, never a tuning knob: 5,000,000,000 work steps, the cap a 50,000,000-attempt limit implied at 100 work steps per attempt.
const DEFAULT_WORK_CAP: WorkCap =
    WorkCap::Finite(std::num::NonZeroU64::new(5_000_000_000).unwrap());

/// The limit on one `generate` run, in work steps and seed choices alike.
const DEFAULT_GENERATION_WORK_CAP: usize = 50_000_000;

fn parse_work_cap(value: &str) -> Result<WorkCap, String> {
    value.parse::<WorkCap>()
}

/// `--capture-cap-mib N|unbounded`: MiB of matcher capture state one word may hold at once, as the byte ceiling `Morpher::with_capture_cap` takes.
fn parse_capture_cap(value: &str) -> Result<Option<usize>, String> {
    if value == "unbounded" {
        return Ok(None);
    }
    value
        .parse::<usize>()
        .ok()
        .and_then(|mib| mib.checked_mul(1024 * 1024))
        .map(Some)
        .ok_or_else(|| {
            format!("invalid --capture-cap-mib: {value} (expected MiB or \"unbounded\")")
        })
}

fn parse_word_mem_cap(value: &str) -> Result<Option<usize>, String> {
    if value == "unbounded" {
        return Ok(None);
    }
    let mib = value
        .parse::<usize>()
        .map_err(|_| format!("invalid --word-mem-cap-mib: {value} (expected MiB or 0)"))?;
    if mib == 0 {
        return Ok(None);
    }
    mib.checked_mul(1024 * 1024)
        .map(Some)
        .ok_or_else(|| format!("invalid --word-mem-cap-mib: {value} (expected MiB or 0)"))
}

/// The value of a `--flag=true|false` override of a project parser setting.
fn parse_bool_flag(flag: &str, value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("invalid {flag}={value} (expected true or false)")),
    }
}

/// Allocator-level ground truth for `docs/research/word-memory-trace.md`; off unless built with `--features alloc-trace`.
#[cfg(feature = "alloc-trace")]
#[global_allocator]
static GLOBAL_ALLOC: alloc_trace::CountingAlloc = alloc_trace::CountingAlloc;

/// Inert until `--cost-column` or `cost-calibrate` turns counting on.
#[cfg(not(feature = "alloc-trace"))]
#[global_allocator]
static GLOBAL_ALLOC: pg_cost::CountingAllocator = pg_cost::CountingAllocator;

fn main() -> ExitCode {
    // The analysis cascade recurses to the depth of a word's unapplication chain, which on heavy corpus words exceeds the default 8 MiB main-thread stack, so the whole batch runs on a worker thread with a generous stack.
    std::thread::Builder::new()
        .stack_size(1 << 30) // 1 GiB
        .spawn(run)
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

fn run() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--version") | Some("-V") => {
            println!("pangloss {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        // `--describe` is a flag alias handled here; `pangloss describe` uses the table below.
        Some("--describe") => surface::run_describe(&args[2..]),
        Some(name) => match surface::find_command(name) {
            Some(spec) => (spec.handler)(&args[2..]),
            None => print_usage_and_fail(),
        },
        None => print_usage_and_fail(),
    }
}

fn print_usage_and_fail() -> ExitCode {
    let help = format!(
        "pangloss {} — HermitCrab Rust engine CLI\n\
         usage: pangloss batch <grammar> <words.txt> <out.tsv> [--work-cap N|unbounded] [--word-mem-cap-mib N] [--capture-cap-mib N|unbounded] [--word-timeout-ms N] [--threads N] [--start N] [--analyses <path>] [--guess[=true|false]] [--work-columns] [--cost-column] [--no-flush] [--phase-marks] [--stats] [--cache <path>] [--stats-manifest <path>] [--always-enforce-final-templates[=true|false]]\n\
         usage: pangloss analyze-text --profile <qaa.ldml|profile.json> --pack <language>=<grammar> [--pack ...] [--work-cap N|unbounded] [--word-mem-cap-mib N] [--word-timeout-ms N] [--max-checked-occurrences N] [--max-alternatives N] [--all-readings] <text-file>...\n\
         usage: pangloss cost-compare <before.tsv> <after.tsv> [--json]\n\
         usage: pangloss generate <grammar> <root-morpheme-id> [other-morpheme-id ...] | pangloss generate <snapshot|project> --request <request.json>\n\
         usage: pangloss parse <grammar> <word> [--trace[=<file>]] [--trace-format=text|json] [--trace-details] [--gloss] [--natural-gloss=eng] [--realize-map=<path>] [--guess[=true|false]] [--work-cap N|unbounded] [--word-mem-cap-mib N] [--word-timeout-ms N]\n\
         usage: pangloss serve --project <project.fwdata|snapshot.json> [--work-cap N|unbounded] [--word-mem-cap-mib N] [--word-timeout-ms N] [--guess[=true|false]] [--timing] (line-delimited JSON on stdin/stdout; see docs/formats/serve-protocol.md)\n\
         usage: pangloss import <project.fwdata/.fwbackup> <out.json>\n\
         usage: pangloss facts <snapshot.json> --out <facts.sqlite> --context <context.json> [--stats <cache.sqlite> --stats-manifest <manifest.json>] [--light] [--json]\n\
         usage: pangloss fst-health <grammar> [<out.json>] [--fst-strategy surface-enum|factored|auto]\n\
         usage: pangloss grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids] [--stored-analyses bounded|all|off] [--fst] [--fst-strategy surface-enum|factored|auto] [--snapshot-out <snapshot.json>] [--facts-out <facts.sqlite> --facts-context <context.json> [--facts-light]]\n\
         usage: pangloss coverage [--json] [--grammar=<path>] [<out.json>]\n\
         usage: pangloss plan-diagram <grammar> [--json] [--full] [--threshold=N] [<out>]\n\
         usage: pangloss make-report <grammar> <out.md> [--pack=<path>] [--policy=<path>]{}\n\
         usage: pangloss recipe-optimize <grammar> <words.txt> <out-dir> [--seed N] [--candidates N] [--evaluations N] [--elapsed-ns N] [--build-ns N] [--memory-bytes N] [--confirmation-work N] [--reserve-ns N]\n\
         usage: pangloss stats <project-or-grammar> [options] (run `pangloss stats` with no arguments to print the full current option list)\n\
         usage: pangloss --describe | pangloss describe (machine-readable JSON of every subcommand and flag, from the same table run() dispatches on)\n\
         \n\
         <grammar> is one of: a HermitCrab XML export (.xml, the legacy path), a\n\
         pg-snapshot JSON file (.json, from `pangloss import` or any other producer), or a\n\
         FieldWorks project file (.fwdata/.fwbackup, imported in-memory and compiled on the fly).\n\
         In a generate request, the morphemes array is generation order, not surface order.\n\
         See docs/generation.md.\n\
         \n\
         --guess (`batch`/`parse`/`serve`): a word whose normal analysis is empty is retried\n\
         via the lexical-pattern guesser (docs/p11-guesser-api-design.md). A FieldWorks project\n\
         sets it with <HC><GuessRoots> (default true, as in FieldWorks); a HermitCrab XML grammar\n\
         never guesses by default, as the C# hc tool does not. --guess or --guess=true forces it\n\
         on, --guess=false forces it off. A guessed analysis is never presented as confirmed:\n\
         with --guess, `parse` prints a `guessed:` line and `batch` adds a `guessed` TSV column\n\
         after `signature`; without the flag the stdout layout is unchanged, `parse` notes a\n\
         guess on stderr, and `batch --analyses` reports each guessed morph's guessedString.\n\
         --work-columns (`batch`) appends `work_steps` to every result row\n\
         (docs/formats/batch-tsv.md); off by default, the row layout is unchanged.\n\
         --no-flush (`batch --threads 1`) keeps rows in the write buffer instead of flushing after each
         word; the file is identical at exit, but a killed run loses the unflushed rows and the live
         STARTED sentinel. For instruction counting, where per-word writes add kernel work.
         --phase-marks (`batch --threads 1`) starts and joins one spinning thread before the first word and one after the\n\
         last, so a PMC trace can count the parse apart from the grammar load; for instruction-compare.\n\
         --cost-column (`batch`) appends `work_steps`, then each word's fixed-weight parse cost, as the last fields; off by\n\
         default. Compare two runs of one word list with `cost-compare`; it totals only words that are\n\
         ok in both, reports the rest, and says inconclusive when a status changed. Use the total;\n\
         per-word costs repeat exactly only with --threads 1 (docs/design/parse-cost.md).\n\
         --prefilter on (`batch`, needs the foma-tools build feature) is a verification and measurement mode: it skips the\n\
         warm-up, calibration, budget and minimum batch length, runs every stage that is valid for the grammar on every\n\
         word (a stage that is not valid is off with its reason, as under auto), pays the whole construction whatever the\n\
         batch length, and prints the same PREFILTER and PREFILTER-TOTAL lines.\n\
         --prefilter auto (`batch`, needs the foma-tools build feature) lets a load-time calibration decide whether a\n\
         rule-admission filter and a tiled whole-word test narrow each parse; the analyses equal --prefilter off (the\n\
         default) on every word whose parse completes in both modes; a word that is incomplete in either mode (work cap,\n\
         capture cap) can have a different partial list. Batches under 512 words stay plain. The guess-only path for grammars\n\
         with lexical patterns is covered by synthetic tests only. It prints PREFILTER and PREFILTER-TOTAL lines on\n\
         stderr and is refused with --stats and --word-timeout-ms. Auto and on turn the cost meter on, with or without\n\
         --cost-column, so the gate prices every parse in full parser work and the decision does not depend on the\n\
         column; with --work-columns or --cost-column the columns are the work the selected pipeline spent on the word\n\
         (docs/development/fst-construction-strategies.md).\n\
         --always-enforce-final-templates is result-changing: enforce template order even\n\
         when partial-rule rescue would otherwise allow an interleaving. A FieldWorks project\n\
         sets it with <HC><MakePartialsFinal>; an explicit flag overrides the project, and\n\
         without the flag the project's value (default false) applies.",
        env!("CARGO_PKG_VERSION"),
        REPORT_DEVELOPER_HELP
    );
    for line in help.lines() {
        if let Some(command) = line
            .strip_prefix("usage: pangloss ")
            .and_then(|usage| usage.split_whitespace().next())
        {
            if command != "--describe" && surface::find_command(command).is_none() {
                continue;
            }
        }
        eprintln!("{line}");
    }
    ExitCode::FAILURE
}

/// `import <project.fwdata/.fwbackup> <out.json>`: runs `pg-fwdata` over a FieldWorks project file and writes the snapshot only when its import provenance has no fatal issue; tolerated warnings and snapshot-validation warnings remain visible under separate headings.
fn run_import(args: &[String]) -> Result<(), String> {
    let [fwdata_path, out_path] = args else {
        return Err("usage: import <project.fwdata/.fwbackup> <out.json>".into());
    };

    let (snapshot, report) = pg_fwdata::import_file(std::path::Path::new(fwdata_path))
        .map_err(|e| format!("import {fwdata_path}: {e}"))?;

    if compile_failure::CompileFailure::import_has_fatal_issues(&snapshot.conversion_provenance) {
        let failure = compile_failure::CompileFailure::from_import_provenance(
            fwdata_path,
            &snapshot.conversion_provenance,
        );
        return Err(serde_json::to_string(&failure)
            .map_err(|error| format!("serialize import refusal: {error}"))?);
    }

    eprintln!("import warnings ({}):", report.warnings.len());
    for w in &report.warnings {
        eprintln!("  {w}");
    }

    let validate_warnings = snapshot.validate();
    eprintln!("validate warnings ({}):", validate_warnings.len());
    for w in &validate_warnings {
        eprintln!("  {w}");
    }

    fs::write(out_path, snapshot.to_json()).map_err(|e| format!("write {out_path}: {e}"))?;
    eprintln!(
        "import complete: {} lex entries, {} phonemes -> {out_path}",
        snapshot.lexicon.entries.len(),
        snapshot.phonology.phonemes.len()
    );
    Ok(())
}

struct LoadedGrammar {
    grammar: Grammar,
    warnings: Vec<pg_snapshot::Warning>,
    stored_analyses: Vec<pg_fwdata::StoredAnalysis>,
    metadata: Option<rich_trace::TraceMetadata>,
    stats_source_identity: Option<stats_manifest::SourceIdentity>,
    substrate: pg_grammar::compile::issues::SubstrateReport,
}

/// What a FieldWorks import produced, kept for `grammar-health --snapshot-out`/`--facts-out` so the
/// one import also yields the files `import` and `facts` would. Filled right after the import and
/// before the compile, so a compile refusal still leaves it behind, as `import` alone would.
#[derive(Default)]
pub(crate) struct ImportCapture {
    /// The snapshot JSON; `None` when the import has a fatal issue.
    pub snapshot_json: Option<String>,
    pub import_refused: bool,
    /// Set by the caller that will write facts: keep the compile's results for it, when reusing them is safe.
    pub keep_compile: bool,
    /// The successful compile's results, kept only when the snapshot JSON parses back to an equal Snapshot.
    pub compiled: Option<pg_facts::CompiledParts>,
    reuse_compile: bool,
}

fn load_grammar_impl(
    path: &str,
    capture_metadata: bool,
    capture_stats_identity: bool,
) -> Result<LoadedGrammar, compile_failure::GrammarLoadError> {
    load_grammar_capturing(path, capture_metadata, capture_stats_identity, None)
}

pub(crate) fn load_grammar_capturing(
    path: &str,
    capture_metadata: bool,
    capture_stats_identity: bool,
    mut capture: Option<&mut ImportCapture>,
) -> Result<LoadedGrammar, compile_failure::GrammarLoadError> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let t_phase = Instant::now();
    match ext {
        "json" => {
            let json = if capture_metadata {
                let bytes = fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
                String::from_utf8(bytes).map_err(|e| format!("read {path}: {e}"))?
            } else {
                fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?
            };
            let snapshot = pg_snapshot::Snapshot::from_json(&json)
                .map_err(|e| format!("parse snapshot {path}: {e}"))?;
            serve::record_ms(&serve::IMPORT_MS, t_phase);
            let stats_source_identity = capture_stats_identity
                .then(|| stats_manifest::SourceIdentity::from_snapshot_source(&json, &snapshot))
                .transpose()?;
            drop(json);
            let t_compile = Instant::now();
            let output = pg_grammar::compile_project_grammar_only_with_import_warnings(
                &snapshot,
                Default::default(),
                std::iter::empty(),
            )
            .map_err(|error| {
                compile_failure::GrammarLoadError::Compile(Box::new(
                    compile_failure::CompileFailure::new(path, error, Some(&snapshot)),
                ))
            })?;
            serve::record_ms(&serve::COMPILE_MS, t_compile);
            let metadata =
                capture_metadata.then(|| rich_trace::metadata_from_snapshot(snapshot, "snapshot"));
            Ok(LoadedGrammar {
                grammar: output.grammar,
                warnings: output.warnings,
                stored_analyses: Vec::new(),
                metadata,
                stats_source_identity,
                substrate: output.substrate,
            })
        }
        _ if ext.eq_ignore_ascii_case("fwdata") || ext.eq_ignore_ascii_case("fwbackup") => {
            let source_bytes = if capture_stats_identity {
                Some(fs::read(path).map_err(|e| format!("read {path}: {e}"))?)
            } else {
                None
            };
            // The facts export (a capture) and the stats manifest fingerprint the full snapshot, provenance included, so only a plain load goes lean.
            let imported = if capture.is_some() || capture_stats_identity {
                pg_fwdata::import_file(std::path::Path::new(path))
            } else {
                pg_fwdata::import_file_lean(std::path::Path::new(path))
            };
            let (snapshot, report) = imported.map_err(|e| format!("import {path}: {e}"))?;
            serve::record_ms(&serve::IMPORT_MS, t_phase);
            if let Some(capture) = capture.as_deref_mut() {
                capture.import_refused = compile_failure::CompileFailure::import_has_fatal_issues(
                    &snapshot.conversion_provenance,
                );
                if !capture.import_refused {
                    let json = snapshot.to_json();
                    capture.reuse_compile = capture.keep_compile
                        && pg_snapshot::Snapshot::from_json(&json)
                            .is_ok_and(|parsed| parsed == snapshot);
                    capture.snapshot_json = Some(json);
                }
            }
            let stored_analyses = report.stored_analyses;
            if let Some(source_bytes) = source_bytes.as_deref() {
                let bytes_after_import =
                    fs::read(path).map_err(|e| format!("read {path} after import: {e}"))?;
                if bytes_after_import != source_bytes {
                    return Err(format!(
                        "source changed while importing {path}; refusing to identify a different input"
                    )
                    .into());
                }
            }
            let stats_source_identity = source_bytes
                .as_deref()
                .map(|bytes| {
                    stats_manifest::SourceIdentity::from_imported_snapshot(bytes, &snapshot)
                })
                .transpose()?;
            let mut import_warnings = report.warnings;
            import_warnings.extend(snapshot.validate());
            let t_compile = Instant::now();
            let compiled = if capture
                .as_ref()
                .is_some_and(|capture| capture.reuse_compile)
            {
                pg_grammar::compile_project_with_options_and_import_warnings(
                    &snapshot,
                    Default::default(),
                    import_warnings,
                )
            } else {
                pg_grammar::compile_project_grammar_only_with_import_warnings(
                    &snapshot,
                    Default::default(),
                    import_warnings,
                )
            };
            let output = compiled.map_err(|error| {
                compile_failure::GrammarLoadError::Compile(Box::new(
                    compile_failure::CompileFailure::new(path, error, Some(&snapshot)),
                ))
            })?;
            serve::record_ms(&serve::COMPILE_MS, t_compile);
            if let Some(capture) = capture.filter(|capture| capture.reuse_compile) {
                capture.compiled = Some(pg_facts::CompiledParts {
                    inventory: output.inventory,
                    decisions: output.load_decisions,
                    compile_issues: output.issues,
                    environment_resolutions: output.environment_resolutions,
                    compiled_outputs: output.compiled_outputs,
                    compiled_mappings: output.compiled_mappings,
                    compiled_allomorph_order: output.compiled_allomorph_order,
                    allomorph_gates: output.allomorph_gates,
                    allomorph_output_ids: output.allomorph_output_ids,
                });
            }
            let metadata =
                capture_metadata.then(|| rich_trace::metadata_from_snapshot(snapshot, "fwdata"));
            Ok(LoadedGrammar {
                grammar: output.grammar,
                warnings: output.warnings,
                stored_analyses,
                metadata,
                stats_source_identity,
                substrate: output.substrate,
            })
        }
        _ => {
            let (xml, hash) = if capture_metadata {
                let bytes = fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
                let hash = stats_manifest::sha256_hex(&bytes);
                let xml = String::from_utf8(bytes).map_err(|e| format!("read {path}: {e}"))?;
                (xml, Some(hash))
            } else {
                (
                    fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?,
                    None,
                )
            };
            let stats_source_identity = if capture_stats_identity {
                Some(stats_manifest::SourceIdentity::from_hc_xml(&xml)?)
            } else {
                None
            };
            let grammar = pg_grammar::load(&xml).map_err(|error| {
                compile_failure::GrammarLoadError::Compile(Box::new(
                    compile_failure::CompileFailure::new(path, error, None),
                ))
            })?;
            let metadata = hash.map(|hash| rich_trace::metadata_from_xml(&grammar, hash));
            Ok(LoadedGrammar {
                grammar,
                warnings: Vec::new(),
                stored_analyses: Vec::new(),
                metadata,
                stats_source_identity,
                substrate: Default::default(),
            })
        }
    }
}

pub(crate) fn load_grammar(path: &str) -> Result<(Grammar, Vec<pg_snapshot::Warning>), String> {
    let loaded = load_grammar_impl(path, false, false).map_err(|error| error.to_string())?;
    print_substrate_report(&loaded.substrate);
    Ok((loaded.grammar, loaded.warnings))
}

fn load_grammar_with_stats_identity(
    path: &str,
) -> Result<
    (
        Grammar,
        Vec<pg_snapshot::Warning>,
        stats_manifest::SourceIdentity,
    ),
    String,
> {
    let mut loaded = load_grammar_impl(path, true, true).map_err(|error| error.to_string())?;
    let source_identity = loaded
        .stats_source_identity
        .take()
        .ok_or_else(|| "stats source identity was not captured".to_string())?;
    print_substrate_report(&loaded.substrate);
    Ok((loaded.grammar, loaded.warnings, source_identity))
}

pub(crate) fn load_grammar_with_trace_metadata(
    path: &str,
) -> Result<
    (
        Grammar,
        Vec<pg_snapshot::Warning>,
        rich_trace::TraceMetadata,
    ),
    String,
> {
    let loaded = load_grammar_impl(path, true, false).map_err(|error| error.to_string())?;
    let metadata = loaded
        .metadata
        .ok_or_else(|| "rich trace metadata was not captured".to_string())?;
    print_substrate_report(&loaded.substrate);
    Ok((loaded.grammar, loaded.warnings, metadata))
}
fn print_substrate_report(report: &pg_grammar::compile::issues::SubstrateReport) {
    for inferred in &report.inferred_segments {
        eprintln!(
            "substrate: inferred segment {:?} ({:?})",
            inferred.representation, inferred.evidence
        );
    }
    for inferred in &report.inferred_boundaries {
        eprintln!(
            "substrate: inferred boundary {:?} ({:?})",
            inferred.representation, inferred.evidence
        );
    }
    for source in &report.unresolved_uses {
        eprintln!("substrate: unresolved use {source:?}");
    }
    for source in &report.ambiguous_uses {
        eprintln!("substrate: ambiguous use {source:?}");
    }
}

/// Print grammar warnings to stderr so parser stdout remains machine-readable.
pub(crate) fn print_grammar_warnings(warnings: &[pg_snapshot::Warning]) {
    for w in warnings {
        eprintln!("warning: {w}");
    }
}

/// `parse <grammar> <word> [flags...]`: traces, glosses, and/or realizes exactly one word's analyses. Flag semantics are detailed in the top-level usage banner; `--gloss`/`--natural-gloss` never touch the `word\tsignature` parity line, and a missing default-resolved realize-map sidecar degrades to empty while an explicitly named one failing is a hard error.
fn run_parse(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut trace_dest: Option<Option<String>> = None; // None = --trace not given; Some(None) = stdout; Some(Some(path)) = file
    let mut trace_format = "text".to_string();
    let mut trace_details = false;
    let mut gloss = false;
    let mut natural_gloss: Option<String> = None;
    let mut realize_map_arg: Option<String> = None;
    let mut guess_arg: Option<bool> = None;
    let mut work_cap = DEFAULT_WORK_CAP;
    let mut word_mem_cap = Some(pg_parse::morpher::DEFAULT_WORD_MEM_CAP_BYTES);
    let mut word_timeout = None;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--trace" => trace_dest = Some(None),
            s if s.starts_with("--trace=") => {
                trace_dest = Some(Some(s["--trace=".len()..].to_string()))
            }
            "--trace-format" => {
                let v = it.next().ok_or("--trace-format requires a value")?;
                trace_format = v.clone();
            }
            s if s.starts_with("--trace-format=") => {
                trace_format = s["--trace-format=".len()..].to_string();
            }
            "--trace-details" => trace_details = true,
            "--gloss" => gloss = true,
            "--natural-gloss" => {
                let v = it.next().ok_or("--natural-gloss requires a value")?;
                natural_gloss = Some(v.clone());
            }
            s if s.starts_with("--natural-gloss=") => {
                natural_gloss = Some(s["--natural-gloss=".len()..].to_string());
            }
            "--realize-map" => {
                let v = it.next().ok_or("--realize-map requires a value")?;
                realize_map_arg = Some(v.clone());
            }
            s if s.starts_with("--realize-map=") => {
                realize_map_arg = Some(s["--realize-map=".len()..].to_string());
            }
            s if s == "--guess" || s.starts_with("--guess=") => {
                guess_arg = Some(parse_guess_flag(s)?)
            }
            "--work-cap" => {
                let value = it.next().ok_or("--work-cap requires a value")?;
                work_cap = parse_work_cap(value)?;
            }
            value if value.starts_with("--work-cap=") => {
                work_cap = parse_work_cap(&value["--work-cap=".len()..])?;
            }
            "--word-mem-cap-mib" => {
                word_mem_cap =
                    parse_word_mem_cap(it.next().ok_or("--word-mem-cap-mib requires a value")?)?;
            }
            value if value.starts_with("--word-mem-cap-mib=") => {
                word_mem_cap = parse_word_mem_cap(&value["--word-mem-cap-mib=".len()..])?;
            }
            "--word-timeout-ms" => {
                let value = it.next().ok_or("--word-timeout-ms requires a value")?;
                word_timeout = Some(Duration::from_millis(
                    value
                        .parse::<u64>()
                        .map_err(|_| "--word-timeout-ms must be an integer")?,
                ));
            }
            value if value.starts_with("--word-timeout-ms=") => {
                word_timeout = Some(Duration::from_millis(
                    value["--word-timeout-ms=".len()..]
                        .parse::<u64>()
                        .map_err(|_| "--word-timeout-ms must be an integer")?,
                ));
            }
            s => {
                reject_unknown_option("parse", s)?;
                positional.push(s);
            }
        }
    }
    if trace_format != "text" && trace_format != "json" {
        return Err(format!(
            "invalid --trace-format: {trace_format} (expected text|json)"
        ));
    }
    rich_trace::validate_details(
        trace_dest.is_some(),
        &trace_format,
        trace_details,
        gloss || natural_gloss.is_some(),
    )?;
    if let Some(v) = &natural_gloss {
        if v != "eng" {
            return Err(format!(
                "unsupported --natural-gloss value: {v} (supported: eng)"
            ));
        }
    }
    let [grammar_path, word] = positional[..] else {
        return Err("usage: parse <grammar> <word> [--trace[=<file>]] [--trace-format=text|json] [--trace-details] [--gloss] [--natural-gloss=eng] [--realize-map=<path>] [--guess[=true|false]] [--work-cap N|unbounded] [--word-mem-cap-mib N] [--word-timeout-ms N]".into());
    };

    let (grammar, warnings, trace_metadata) = if trace_details {
        load_grammar_with_trace_metadata(grammar_path)?
    } else {
        let (grammar, warnings) = load_grammar(grammar_path)?;
        (grammar, warnings, rich_trace::TraceMetadata::default())
    };
    print_grammar_warnings(&warnings);
    // --natural-gloss=eng setup built once up front, since neither the embedded table nor the sidecar map depends on the word being parsed.
    let natural: Option<(pg_realize::TableRealizer, pg_realize::RealizeMap)> = match &natural_gloss
    {
        None => None,
        Some(_) => {
            let realizer = pg_realize::TableRealizer::new()
                .map_err(|e| format!("load embedded natural-gloss assets: {e}"))?;
            let map = load_realize_map(grammar_path, realize_map_arg.as_deref())?;
            Some((realizer, map))
        }
    };

    let morpher = with_guess_override(
        Morpher::new(&grammar, work_cap.as_morpher_cap())
            .with_word_timeout(word_timeout)
            .with_word_mem_cap(word_mem_cap),
        guess_arg,
    );
    let opts = morpher.project_parse_options();
    let guess = guess_arg == Some(true);

    if let Some(dest) = trace_dest {
        let sink = if trace_details {
            pg_rules::trace::TreeTraceSink::with_failure_context()
        } else {
            pg_rules::trace::TreeTraceSink::new()
        };
        if trace_details {
            let started = Instant::now();
            let (outcome, rows) = morpher.parse_word_traced_with_stats(word, &opts, &sink);
            let rendered = rich_trace::render(
                &grammar,
                &sink,
                sink.root(),
                word,
                &outcome,
                &rows,
                started.elapsed(),
                &trace_metadata,
            )?;
            match dest {
                None => print!("{rendered}"),
                Some(path) => {
                    fs::write(&path, rendered).map_err(|e| format!("write {path}: {e}"))?
                }
            }
            return Ok(());
        }
        let outcome = morpher.parse_word_traced(word, &opts, &sink);
        println!("{}\t{}", word, outcome.signature());
        print_guessed_line(guess, outcome.guessed);
        print_realize_lines(&grammar, &outcome.structured, word, gloss, natural.as_ref());

        let rendered = match sink.root() {
            Some(root) if trace_format == "json" => {
                trace_render::render_json(&grammar, &sink, root)
            }
            Some(root) => trace_render::render_text(&grammar, &sink, root),
            None => String::new(), // no strata / invalid shape: nothing was ever traced
        };
        match dest {
            None => print!("{rendered}"),
            Some(path) => fs::write(&path, rendered).map_err(|e| format!("write {path}: {e}"))?,
        }
        require_complete_parse(&outcome)?;
    } else {
        // No --trace: behave like a minimal, single-word `batch` (the parse result only).
        let outcome = morpher.parse_word_opts(word, &opts);
        println!("{}\t{}", word, outcome.signature());
        print_guessed_line(guess, outcome.guessed);
        print_realize_lines(&grammar, &outcome.structured, word, gloss, natural.as_ref());
        require_complete_parse(&outcome)?;
    }
    Ok(())
}

fn require_complete_parse(outcome: &pg_parse::ParseOutcome) -> Result<(), String> {
    let flags: Vec<_> = [
        (outcome.capped, "capped"),
        (outcome.timed_out, "timed_out"),
        (outcome.invalid_shape, "invalid_shape"),
    ]
    .into_iter()
    .filter_map(|(present, flag)| present.then_some(flag))
    .collect();
    if flags.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "parse incomplete: {}; stdout is diagnostic, not a confirmed result",
            flags.join(", ")
        ))
    }
}

/// Marks a guessed result so it is never indistinguishable from a confirmed one: a stdout `guessed:` line right after the parity line when `--guess` was passed, otherwise a stderr note, so stdout keeps the layout of a run without `--guess`.
fn print_guessed_line(guess_requested: bool, guessed: bool) {
    if guess_requested {
        println!("guessed:\t{guessed}");
    } else if guessed {
        eprintln!("{GUESSED_NOTE}");
    }
}

/// The stderr marker for analyses guessed because the project, not the command line, enabled guessing.
pub(crate) const GUESSED_NOTE: &str = "note: these analyses were guessed from a lexical pattern, not found in the lexicon; the project's GuessRoots setting enabled guessing, and --guess=false turns it off";

/// `--guess` or `--guess=true` forces root guessing on, `--guess=false` forces it off.
pub(crate) fn parse_guess_flag(arg: &str) -> Result<bool, String> {
    match arg.strip_prefix("--guess=") {
        None => Ok(true),
        Some(value) => parse_bool_flag("--guess", value),
    }
}

/// Applies an explicit `--guess`/`--guess=false`; without one the morpher keeps the project's `GuessRoots`.
pub(crate) fn with_guess_override<'g>(
    morpher: Morpher<'g>,
    guess_arg: Option<bool>,
) -> Morpher<'g> {
    match guess_arg {
        Some(guess) => morpher.with_guess_roots(guess),
        None => morpher,
    }
}

/// For each analysis, optionally prints a `gloss:` line then an `eng:` line, interleaved per analysis (not two separate passes) so a reader can tell which `eng:` line goes with which `gloss:` line; reads `.structured`, not `.analyses`, since `gloss_bundle` needs numeric morpheme ordinals.
fn print_realize_lines(
    grammar: &Grammar,
    structured: &[WordAnalysis],
    word: &str,
    gloss: bool,
    natural: Option<&(pg_realize::TableRealizer, pg_realize::RealizeMap)>,
) {
    for analysis in structured {
        let bundle = pg_realize::gloss_bundle(grammar, analysis);
        if gloss {
            println!("gloss:\t{}", pg_realize::leipzig(&bundle, word));
        }
        if let Some((realizer, map)) = natural {
            let ir = pg_realize::to_ir(&bundle, map, word);
            let realization = pg_realize::Realizer::realize(realizer, &ir);
            if realization.residue.is_empty() {
                println!("eng:\t{}", realization.text);
            } else {
                println!(
                    "eng:\t{} ({})",
                    realization.text,
                    realization.residue.join("-")
                );
            }
        }
    }
}

/// Resolves and loads the `--natural-gloss=eng` sidecar map: an explicit `--realize-map` path wins when given, else the default is derived from the grammar path's stem; an explicit path or a parse failure is a hard error, but a missing default-resolved path degrades to `RealizeMap::empty()`.
fn load_realize_map(
    grammar_path: &str,
    explicit_arg: Option<&str>,
) -> Result<pg_realize::RealizeMap, String> {
    let (path, explicit) = match explicit_arg {
        Some(p) => (std::path::PathBuf::from(p), true),
        None => (default_realize_map_path(grammar_path), false),
    };
    if !path.exists() {
        if explicit {
            return Err(format!("--realize-map file not found: {}", path.display()));
        }
        return Ok(pg_realize::RealizeMap::empty());
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    pg_realize::RealizeMap::parse(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// `<grammar-dir>/<grammar-stem-with-"-hc"-suffix-stripped>-realize.toml`.
fn default_realize_map_path(grammar_path: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(grammar_path);
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let stem = stem.strip_suffix("-hc").unwrap_or(stem);
    dir.join(format!("{stem}-realize.toml"))
}

/// Version 5 removes the `steps` column from `--work-columns`, which now writes only `work_steps`; `--cost-column` writes `work_steps` then `cost`, so the two layouts differ by field count.
pub(crate) const BATCH_TSV_VERSION: u32 = 5;

/// The columns a `batch` result row carries after `signature`.
struct BatchRowTail {
    /// `Some(guessed)` only when `--guess` or `--guess=true` was passed, never for a project's own `GuessRoots`.
    guessed: Option<bool>,
    /// `Some(work_steps)` only when `--work-columns` was passed.
    work_steps: Option<usize>,
    /// `Some(cost)` only when `--cost-column` was passed.
    cost: Option<u64>,
}

impl BatchRowTail {
    fn new(flags: BatchRowFlags, outcome: &pg_parse::ParseOutcome) -> Self {
        Self {
            guessed: flags.guess.then_some(outcome.guessed),
            work_steps: (flags.work_columns || flags.cost_column).then_some(outcome.work_steps),
            cost: flags.cost_column.then(|| {
                outcome
                    .cost
                    .expect("--cost-column turned cost measurement on before parsing")
                    .cost()
            }),
        }
    }
}

/// The batch flags that add optional TSV columns.
#[derive(Clone, Copy)]
struct BatchRowFlags {
    guess: bool,
    work_columns: bool,
    cost_column: bool,
}

/// Writes one `idx word elapsed_ms status signature [guessed] [work_steps] [cost]` row; see `docs/formats/batch-tsv.md`.
fn write_batch_row<W: Write>(
    w: &mut W,
    idx: usize,
    word: &str,
    elapsed_ms: u128,
    status: &str,
    signature: &str,
    tail: BatchRowTail,
) -> std::io::Result<()> {
    write!(w, "{idx}\t{word}\t{elapsed_ms}\t{status}\t{signature}")?;
    if let Some(guessed) = tail.guessed {
        write!(w, "\t{guessed}")?;
    }
    if let Some(work_steps) = tail.work_steps {
        write!(w, "\t{work_steps}")?;
    }
    if let Some(cost) = tail.cost {
        write!(w, "\t{cost}")?;
    }
    writeln!(w)
}

/// The parallel batch dispatch for a run that guesses roots, since `hc_parse_batch` has no `ParseOptions` parameter and so cannot express "guess on"; deliberately simpler than that function's longest-surface-first scheduling, using a plain order-preserving `rayon` `par_iter`.
fn parse_batch_with_opts(
    morpher: &Morpher,
    words: &[String],
    max_threads: usize,
    opts: &pg_parse::ParseOptions,
    parse_counter: &BatchParseCounter,
) -> Vec<pg_parse::BatchWordOutcome> {
    use rayon::prelude::*;
    if words.is_empty() {
        return Vec::new();
    }
    let mut pool_builder = rayon::ThreadPoolBuilder::new().stack_size(1 << 30);
    if max_threads > 0 {
        pool_builder = pool_builder.num_threads(max_threads);
    }
    let pool = pool_builder
        .build()
        .expect("build rayon pool for --guess batch dispatch");
    pool.install(|| {
        words
            .par_iter()
            .map(|word| {
                let start = Instant::now();
                parse_counter.record();
                let outcome = morpher.parse_word_opts(word, opts);
                let elapsed = start.elapsed();
                pg_parse::BatchWordOutcome { outcome, elapsed }
            })
            .collect()
    })
}

struct BatchWordRun {
    outcome: pg_parse::BatchWordOutcome,
    stats: Option<stats_cmd::BatchStatsWord>,
}

#[allow(clippy::too_many_arguments)]
fn parse_batch_with_stats(
    morpher: &Morpher,
    grammar: &Grammar,
    identities: &pg_grammar::stats_identity::StatsIdentityCatalog,
    words: &[String],
    max_threads: usize,
    opts: &pg_parse::ParseOptions,
    start_idx: usize,
    existing_words: &HashSet<String>,
    parse_counter: &BatchParseCounter,
) -> Result<Vec<Option<BatchWordRun>>, String> {
    use rayon::prelude::*;
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let mut pool_builder = rayon::ThreadPoolBuilder::new().stack_size(1 << 30);
    if max_threads > 0 {
        pool_builder = pool_builder.num_threads(max_threads);
    }
    let pool = pool_builder
        .build()
        .expect("build rayon pool for --stats batch dispatch");
    pool.install(|| {
        words
            .par_iter()
            .enumerate()
            .map(|(index, word)| {
                if index < start_idx && existing_words.contains(word.as_str()) {
                    return Ok(None);
                }
                let start = Instant::now();
                parse_counter.record();
                let (outcome, rows, prune_rows) =
                    morpher.parse_word_with_stats_and_prunes(word, opts);
                let result = pg_parse::BatchWordOutcome {
                    outcome,
                    elapsed: start.elapsed(),
                };
                let stats = stats_cmd::batch_stats_word(
                    grammar,
                    identities,
                    word,
                    &result,
                    &rows,
                    &prune_rows,
                )?;
                Ok(Some(BatchWordRun {
                    outcome: result,
                    stats: Some(stats),
                }))
            })
            .collect()
    })
}

fn per_word_diagnostics_requested() -> bool {
    [
        "HC_STEP_STATS",
        "HC_FST_PROFILE",
        "HC_FRONTIER_STATS",
        "HC_WORD_STATS",
        "HC_ALT_YIELD",
        "HC_BRANCH_CENSUS",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some())
        || (cfg!(feature = "alloc-trace") && std::env::var_os("HC_ALLOC_STATS").is_some())
}

/// Reset per-word thread-local diagnostic counters before the next parse.
fn reset_per_word_diagnostics() {
    pg_fst::profile::reset();
    pg_fst::profile::enable();
    pg_rules::morph::dedup_profile::reset();
    pg_rules::stratum::frontier_profile::reset();
    pg_rules::word_stats::reset();
    pg_parse::alt_yield::reset();
    pg_rules::branch_census::reset();
    pg_parse::root_trie::take_identity_refusals();
    pg_rules::rewrite::take_class_identity_nodes();
    #[cfg(feature = "alloc-trace")]
    alloc_trace::reset_peak();
}

fn run_batch(args: &[String]) -> Result<(), String> {
    run_batch_with_counter(args, &BatchParseCounter::default())
}

#[cfg(test)]
pub(crate) fn run_batch_counted(args: &[String]) -> Result<usize, String> {
    let parse_counter = BatchParseCounter::default();
    run_batch_with_counter(args, &parse_counter)?;
    Ok(parse_counter.count())
}

/// Iterations of the marker thread's busy loop; its instruction total (about 60M) is how a PMC trace tells a marker thread from the loader and pool threads a process also starts.
const PHASE_MARK_SPIN: u64 = 20_000_000;

/// Starts and joins a thread that only spins: the thread start and the context switches around it are the edge of the parse phase in a PMC trace (rust/tools/instruction_compare_etl.py).
#[inline(never)]
fn phase_mark() {
    let _ = std::thread::spawn(|| {
        let mut x = 0u64;
        for _ in 0..PHASE_MARK_SPIN {
            x = std::hint::black_box(x.wrapping_add(1));
        }
        x
    })
    .join();
}

fn run_batch_with_counter(
    args: &[String],
    parse_counter: &BatchParseCounter,
) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut work_cap: WorkCap = DEFAULT_WORK_CAP;
    let mut word_mem_cap = Some(pg_parse::morpher::DEFAULT_WORD_MEM_CAP_BYTES);
    // --capture-cap-mib: ceiling on matcher capture state one word may hold at once; None = unbounded.
    let mut capture_cap: Option<usize> = Some(pg_fst::work::DEFAULT_RETAINED_CAP_BYTES);
    // --word-timeout-ms: an optional wall-clock deadline per word, independent of --work-cap; None (omitted) is a complete no-op.
    let mut word_timeout_ms: Option<u64> = None;
    // Default (unspecified --threads only) is logical CPUs capped at 8, since per-word memory on a pathological grammar multiplies by thread count and an uncapped default can exhaust machine memory.
    const DEFAULT_THREAD_CAP: usize = 8;
    let mut threads: usize = std::thread::available_parallelism()
        .map(|n| n.get().min(DEFAULT_THREAD_CAP))
        .unwrap_or(1);
    // 0-based resume index: skip the first N words (already-completed rows from a prior crashed/killed run) and append rather than truncate out.tsv, so a watchdog wrapper can kill+relaunch a stalled word and continue where it left off.
    let mut start_idx: usize = 0;
    let mut analyses_path_arg: Option<String> = None;
    // None defers to the project's own `<HC><GuessRoots>`; only an explicit force-on adds the `guessed` column.
    let mut guess_arg: Option<bool> = None;
    // Default-off so the 5/6-column contract external adapters require stays byte-identical.
    let mut work_columns = false;
    let mut cost_column = false;
    // Measurement only: skips the per-word flushes of the `--threads 1` path, whose write syscalls add kernel instructions to the run.
    let mut flush_each_word = true;
    // Measurement only: a thread starts and ends at each edge of the `--threads 1` parse loop, so a PMC trace can count the parse apart from the grammar load.
    let mut phase_marks = false;
    // --stats: additionally drives the `pg_stats` cache (`stats_cmd.rs`); never touches the TSV rows above.
    let mut stats_requested = false;
    // None defers to the project's own `<HC><MakePartialsFinal>` once the grammar is loaded.
    let mut always_enforce_final_templates_arg: Option<bool> = None;
    let mut cache_path_arg: Option<String> = None;
    let mut stats_manifest_path_arg: Option<String> = None;
    let mut prefilter_mode = batch_prefilter::PrefilterMode::Off;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--work-cap" => {
                let value = it.next().ok_or("--work-cap requires a value")?;
                work_cap = parse_work_cap(value)?;
            }
            s if s.starts_with("--work-cap=") => {
                work_cap = parse_work_cap(&s["--work-cap=".len()..])?;
            }
            "--word-mem-cap-mib" => {
                word_mem_cap =
                    parse_word_mem_cap(it.next().ok_or("--word-mem-cap-mib requires a value")?)?;
            }
            s if s.starts_with("--word-mem-cap-mib=") => {
                word_mem_cap = parse_word_mem_cap(&s["--word-mem-cap-mib=".len()..])?;
            }
            "--capture-cap-mib" => {
                let v = it.next().ok_or("--capture-cap-mib requires a value")?;
                capture_cap = parse_capture_cap(v)?;
            }
            s if s.starts_with("--capture-cap-mib=") => {
                capture_cap = parse_capture_cap(&s["--capture-cap-mib=".len()..])?;
            }
            "--word-timeout-ms" => {
                let v = it.next().ok_or("--word-timeout-ms requires a value")?;
                word_timeout_ms = Some(
                    v.parse()
                        .map_err(|_| format!("invalid --word-timeout-ms: {v}"))?,
                );
            }
            s if s.starts_with("--word-timeout-ms=") => {
                let v = &s["--word-timeout-ms=".len()..];
                word_timeout_ms = Some(
                    v.parse()
                        .map_err(|_| format!("invalid --word-timeout-ms: {v}"))?,
                );
            }
            "--threads" => {
                let v = it.next().ok_or("--threads requires a value")?;
                threads = v.parse().map_err(|_| format!("invalid --threads: {v}"))?;
            }
            s if s.starts_with("--threads=") => {
                let v = &s["--threads=".len()..];
                threads = v.parse().map_err(|_| format!("invalid --threads: {v}"))?;
            }
            "--start" => {
                let v = it.next().ok_or("--start requires a value")?;
                start_idx = v.parse().map_err(|_| format!("invalid --start: {v}"))?;
            }
            s if s.starts_with("--start=") => {
                let v = &s["--start=".len()..];
                start_idx = v.parse().map_err(|_| format!("invalid --start: {v}"))?;
            }
            "--analyses" => {
                let v = it.next().ok_or("--analyses requires a value")?;
                analyses_path_arg = Some(v.clone());
            }
            s if s.starts_with("--analyses=") => {
                analyses_path_arg = Some(s["--analyses=".len()..].to_string());
            }
            s if s == "--guess" || s.starts_with("--guess=") => {
                guess_arg = Some(parse_guess_flag(s)?)
            }
            "--work-columns" => work_columns = true,
            "--cost-column" => cost_column = true,
            "--no-flush" => flush_each_word = false,
            "--phase-marks" => phase_marks = true,
            "--stats" => stats_requested = true,
            "--prefilter" => {
                let v = it.next().ok_or("--prefilter requires a value")?;
                prefilter_mode = batch_prefilter::PrefilterMode::parse(v)?;
            }
            s if s.starts_with("--prefilter=") => {
                prefilter_mode = batch_prefilter::PrefilterMode::parse(&s["--prefilter=".len()..])?;
            }
            "--always-enforce-final-templates" => always_enforce_final_templates_arg = Some(true),
            s if s.starts_with("--always-enforce-final-templates=") => {
                always_enforce_final_templates_arg = Some(parse_bool_flag(
                    "--always-enforce-final-templates",
                    &s["--always-enforce-final-templates=".len()..],
                )?);
            }
            "--cache" => {
                let v = it.next().ok_or("--cache requires a value")?;
                cache_path_arg = Some(v.clone());
            }
            s if s.starts_with("--cache=") => {
                cache_path_arg = Some(s["--cache=".len()..].to_string());
            }
            "--stats-manifest" => {
                let v = it.next().ok_or("--stats-manifest requires a value")?;
                stats_manifest_path_arg = Some(v.clone());
            }
            s if s.starts_with("--stats-manifest=") => {
                stats_manifest_path_arg = Some(s["--stats-manifest=".len()..].to_string());
            }
            s => {
                reject_unknown_option("batch", s)?;
                positional.push(s);
            }
        }
    }
    if threads == 0 {
        return Err("--threads must be >= 1".into());
    }
    let per_word_diagnostics = per_word_diagnostics_requested();
    if per_word_diagnostics && threads != 1 {
        return Err(format!(
            "per-word profiling diagnostics require --threads 1 (received --threads {threads})"
        ));
    }
    let [grammar_path, words_path, out_path] = positional.as_slice() else {
        return Err(
            "usage: batch <grammar> <words.txt> <out.tsv> [--work-cap N|unbounded] [--word-mem-cap-mib N] [--capture-cap-mib N|unbounded] [--word-timeout-ms N] [--threads N] [--start N] [--analyses <path>] [--guess[=true|false]] [--work-columns] [--cost-column] [--no-flush] [--phase-marks] [--stats] [--cache <path>] [--stats-manifest <path>] [--always-enforce-final-templates[=true|false]] [--prefilter auto|on|off]"
                .into(),
        );
    };
    batch_prefilter::check(
        prefilter_mode,
        &batch_prefilter::SearchDependent {
            stats: stats_requested,
            word_timeout: word_timeout_ms.is_some(),
            per_word_diagnostics,
        },
    )?;
    if stats_manifest_path_arg.is_some() && !stats_requested {
        return Err("--stats-manifest requires --stats".into());
    }
    if stats_manifest_path_arg.is_some() && cache_path_arg.is_none() {
        return Err("--stats-manifest requires an explicit --cache path".into());
    }
    if stats_manifest_path_arg.is_some() && start_idx != 0 {
        return Err("--stats-manifest cannot be combined with --start".into());
    }
    if analyses_path_arg.is_some() && start_idx > 0 {
        return Err("--start cannot be combined with --analyses".into());
    }
    if cost_column && stats_requested {
        return Err(
            "--cost-column cannot be combined with --stats, whose collector would be counted as parse cost"
                .into(),
        );
    }
    // The gate prices parses in full parser work, so `--prefilter auto` and `on` meter the batch with or without the column.
    if cost_column || prefilter_mode != batch_prefilter::PrefilterMode::Off {
        pg_cost::enable().map_err(|e| match prefilter_mode {
            batch_prefilter::PrefilterMode::Off => e.to_string(),
            mode if !cost_column => format!("--prefilter {} cannot run: {e}", mode.name()),
            _ => e.to_string(),
        })?;
    }

    // LOADTIME always prints unconditionally, since one line per invocation costs nothing.
    let t_load = Instant::now();
    let (grammar, warnings, stats_source_identity) = if stats_manifest_path_arg.is_some() {
        let (grammar, warnings, identity) = load_grammar_with_stats_identity(grammar_path)?;
        (grammar, warnings, Some(identity))
    } else {
        let (grammar, warnings) = load_grammar(grammar_path)?;
        (grammar, warnings, None)
    };
    let stats_identities =
        stats_requested.then(|| pg_grammar::stats_identity::StatsIdentityCatalog::new(&grammar));
    print_grammar_warnings(&warnings);
    let always_enforce_final_templates =
        always_enforce_final_templates_arg.unwrap_or(grammar.always_enforce_final_templates);
    let guess = guess_arg.unwrap_or(grammar.guess_roots);
    let grammar_load_ms = t_load.elapsed().as_secs_f64() * 1e3;
    let stats_cache_path = if stats_requested {
        Some(
            stats_cmd::resolve_cache_path(grammar_path, cache_path_arg.as_deref())?
                .to_string_lossy()
                .into_owned(),
        )
    } else {
        None
    };
    if let Some(analyses_path) = analyses_path_arg.as_deref() {
        reject_analysis_path_collisions(
            grammar_path,
            words_path,
            out_path,
            analyses_path,
            stats_cache_path.as_deref(),
        )?;
    }
    let stats_manifest_path = stats_manifest_path_arg.as_deref();
    if let Some(manifest_path) = stats_manifest_path {
        let cache_path = stats_cache_path
            .as_deref()
            .expect("--stats-manifest requires --stats and --cache");
        reject_frozen_stats_path_collisions(
            grammar_path,
            words_path,
            out_path,
            analyses_path_arg.as_deref(),
            cache_path,
            manifest_path,
        )?;
        if std::path::Path::new(manifest_path).exists() {
            return Err(format!(
                "output_exists: stats manifest already exists at {manifest_path}"
            ));
        }
    }
    let words: Vec<String> = fs::read_to_string(words_path)
        .map_err(|e| format!("read {words_path}: {e}"))?
        .lines()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect();

    // A cache refusal must fire before the TSV below is truncated.
    let stats_cache = if let Some(manifest_path) = stats_manifest_path {
        let cache_path = stats_cache_path
            .as_deref()
            .expect("--stats-manifest requires --stats and --cache");
        let source = stats_source_identity
            .as_ref()
            .expect("--stats-manifest captures a source identity with the grammar");
        Some((
            stats_cmd::prepare_frozen_batch_stats_hc(
                grammar_path,
                &words,
                work_cap,
                word_mem_cap,
                word_timeout_ms,
                guess,
                always_enforce_final_templates,
                cache_path,
                &source.grammar_hash,
            )?,
            Some(manifest_path),
        ))
    } else if stats_requested {
        Some((
            stats_cmd::prepare_batch_stats_hc(
                grammar_path,
                &words,
                work_cap,
                word_mem_cap,
                word_timeout_ms,
                guess,
                always_enforce_final_templates,
                cache_path_arg.as_deref(),
            )?,
            None,
        ))
    } else {
        None
    };

    // start_idx=0 is a fresh run (truncate); >0 is a resume (append to the prior partial TSV).
    let file = if start_idx == 0 {
        fs::File::create(out_path).map_err(|e| format!("create {out_path}: {e}"))?
    } else {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(out_path)
            .map_err(|e| format!("open {out_path} for append: {e}"))?
    };
    let mut w = BufWriter::new(file);
    let mut analyses_w = analyses_path_arg
        .as_deref()
        .map(|path| {
            fs::File::create(path)
                .map(BufWriter::new)
                .map_err(|e| format!("create {path}: {e}"))
        })
        .transpose()?;

    let mut parsed = 0u64;
    let mut skipped = 0u64;
    let mut capped_words = 0u64;
    let mut timed_out_words = 0u64;

    let t_morpher = Instant::now();
    let morpher = Morpher::new(&grammar, work_cap.as_morpher_cap())
        .with_capture_cap(capture_cap)
        .with_word_mem_cap(word_mem_cap)
        .with_word_timeout(word_timeout_ms.map(Duration::from_millis))
        .with_always_enforce_final_templates(always_enforce_final_templates)
        .with_guess_roots(guess);
    let morpher_build_ms = t_morpher.elapsed().as_secs_f64() * 1e3;
    eprintln!(
        "LOADTIME\tengine=default\tgrammar_load_ms={grammar_load_ms:.3}\tmorpher_build_ms={morpher_build_ms:.3}\ttotal_ms={:.3}",
        grammar_load_ms + morpher_build_ms
    );
    let opts = morpher.project_parse_options();
    let row_flags = BatchRowFlags {
        guess: guess_arg == Some(true),
        work_columns,
        cost_column,
    };
    let mut stats_words = Vec::new();

    // Printed unconditionally (not just under --stats) so `--stats`'s own overhead is measurable: without this, disabling --stats leaves no elapsed figure to compare against.
    let t_parse = Instant::now();
    let mut prefilter = match prefilter_mode {
        batch_prefilter::PrefilterMode::Off => None,
        mode => Some(batch_prefilter::BatchPrefilter::prepare(
            mode,
            &grammar,
            &morpher,
            &opts,
            &words,
            start_idx.min(words.len()),
        )?),
    };
    // Under `auto` the meter priced the calibration parses only; the batch itself needs it just for the cost column.
    if prefilter_mode == batch_prefilter::PrefilterMode::Auto && !cost_column {
        pg_cost::disable();
    }
    if phase_marks && threads != 1 {
        return Err("--phase-marks needs --threads 1".to_string());
    }
    if threads == 1 {
        // Legacy sequential path: STARTED sentinel + per-line flush, crash-resumable.
        if phase_marks {
            phase_mark();
        }
        for (i, word) in words.iter().enumerate() {
            if i < start_idx {
                if stats_cache
                    .as_ref()
                    .is_some_and(|(cache, _)| !cache.contains(word))
                {
                    if per_word_diagnostics {
                        reset_per_word_diagnostics();
                    }
                    let start = Instant::now();
                    parse_counter.record();
                    let (outcome, rows, prune_rows) =
                        morpher.parse_word_with_stats_and_prunes(word, &opts);
                    let result = pg_parse::BatchWordOutcome {
                        outcome,
                        elapsed: start.elapsed(),
                    };
                    stats_words.push(stats_cmd::batch_stats_word(
                        &grammar,
                        stats_identities
                            .as_ref()
                            .expect("--stats creates a typed identity catalog"),
                        word,
                        &result,
                        &rows,
                        &prune_rows,
                    )?);
                }
                continue;
            }
            writeln!(w, "{i}\t{word}\tSTARTED").map_err(|e| e.to_string())?;
            // Flush the STARTED sentinel immediately, before starting this word's parse, or it would only reach disk alongside the result line, defeating its purpose as a live in-flight signal for an external watchdog.
            if flush_each_word {
                w.flush().map_err(|e| e.to_string())?;
            }
            if per_word_diagnostics {
                reset_per_word_diagnostics();
            }
            let start = Instant::now();
            let (result, stats) = if stats_requested {
                parse_counter.record();
                let (outcome, rows, prune_rows) =
                    morpher.parse_word_with_stats_and_prunes(word, &opts);
                let result = pg_parse::BatchWordOutcome {
                    outcome,
                    elapsed: start.elapsed(),
                };
                let stats = stats_cmd::batch_stats_word(
                    &grammar,
                    stats_identities
                        .as_ref()
                        .expect("--stats creates a typed identity catalog"),
                    word,
                    &result,
                    &rows,
                    &prune_rows,
                )?;
                (result, Some(stats))
            } else {
                parse_counter.record();
                let outcome = match prefilter.as_mut() {
                    Some(prefilter) => prefilter.parse(i, &morpher, word, &opts),
                    None => morpher.parse_word_opts(word, &opts),
                };
                (
                    pg_parse::BatchWordOutcome {
                        outcome,
                        elapsed: start.elapsed(),
                    },
                    None,
                )
            };
            if let Some(stats) = stats {
                stats_words.push(stats);
            }
            let outcome = result.outcome;
            let elapsed_ms = result.elapsed.as_millis();
            let status = outcome_status(&outcome);
            let signature = match status {
                "SKIPPED" => {
                    skipped += 1;
                    "-".to_string()
                }
                "TIMEOUT" => {
                    timed_out_words += 1;
                    eprintln!("TIMEOUT\t{i}\t{word}");
                    "-".to_string()
                }
                "CAP" | "MEMCAP" => {
                    parsed += 1;
                    capped_words += 1;
                    eprintln!("{status}\t{i}\t{word}");
                    outcome.signature()
                }
                _ => {
                    parsed += 1;
                    outcome.signature()
                }
            };
            // Diagnostic only: raw work total for this word, regardless of whether the cap fired.
            if std::env::var("HC_STEP_STATS").is_ok() {
                eprintln!("WORK_STEPS\t{i}\t{word}\t{}", outcome.work_steps);
                let shadow = pg_rules::required_slots::take_shadow();
                eprintln!(
                    "RSL_SHADOW\t{i}\t{word}\t{}\t{}\t{}",
                    shadow.fired, shadow.violated, shadow.completed_under_fired
                );
                eprintln!(
                    "IDENTITY_REFUSALS\t{i}\t{word}\t{}",
                    pg_parse::root_trie::take_identity_refusals()
                );
                eprintln!(
                    "CLASS_IDENTITY_NODES\t{i}\t{word}\t{}",
                    pg_rules::rewrite::take_class_identity_nodes()
                );
            }
            // Permanent profiling diagnostic dumping pg_fst::traverse and pg_rules::morph timing/size stats accumulated over this word's whole parse.
            // See docs/o2-profile-findings.md for what this found.
            if std::env::var("HC_FST_PROFILE").is_ok() {
                let (
                    run_calls,
                    run_ns,
                    run_max_ns,
                    nd_calls,
                    nd_ns,
                    nd_max_traversed,
                    nd_total_traversed,
                    det_calls,
                    det_ns,
                    distinct_calls,
                    distinct_ns,
                    distinct_max_input_len,
                    distinct_total_input_len,
                ) = pg_fst::profile::snapshot();
                eprintln!(
                    "FSTPROF\t{i}\t{word}\trun_calls={run_calls}\trun_ms={:.3}\trun_max_ms={:.3}\t\
                     nondet_calls={nd_calls}\tnondet_ms={:.3}\tnondet_max_traversed={nd_max_traversed}\t\
                     nondet_total_traversed={nd_total_traversed}\tdet_calls={det_calls}\tdet_ms={:.3}\t\
                     distinct_calls={distinct_calls}\tdistinct_ms={:.3}\tdistinct_max_input_len={distinct_max_input_len}\t\
                     distinct_total_input_len={distinct_total_input_len}",
                    run_ns as f64 / 1e6,
                    run_max_ns as f64 / 1e6,
                    nd_ns as f64 / 1e6,
                    det_ns as f64 / 1e6,
                    distinct_ns as f64 / 1e6,
                );
                let (dedup_calls, dedup_ns, dedup_max_out_len, dedup_total_out_len) =
                    pg_rules::morph::dedup_profile::snapshot();
                eprintln!(
                    "DEDUPPROF\t{i}\t{word}\tcalls={dedup_calls}\tms={:.3}\tmax_out_len={dedup_max_out_len}\ttotal_out_len={dedup_total_out_len}",
                    dedup_ns as f64 / 1e6,
                );
            }
            // Peak live-search-frontier counters (`docs/research/live-frontier-memory-bound.md`).
            if std::env::var("HC_FRONTIER_STATS").is_ok() {
                let f = pg_rules::stratum::frontier_profile::snapshot();
                eprintln!(
                    "FRONTIERPROF\t{i}\t{word}\t\
                     max_depth={}\t\
                     max_local_len={}\tmax_local_bytes={}\t\
                     max_dedup_len={}\tmax_dedup_bytes={}\t\
                     max_raw_cascade_len={}\tmax_raw_cascade_bytes={}\t\
                     max_template_len={}\tmax_template_bytes={}\t\
                     max_apply_mrules_len={}\tmax_apply_mrules_bytes={}\t\
                     max_apply_templates_len={}\tmax_apply_templates_bytes={}\t\
                     max_live_words={}\tmax_live_bytes={}",
                    f.max_depth,
                    f.max_local_len,
                    f.max_local_bytes,
                    f.max_dedup_len,
                    f.max_dedup_bytes,
                    f.max_raw_cascade_len,
                    f.max_raw_cascade_bytes,
                    f.max_template_len,
                    f.max_template_bytes,
                    f.max_apply_mrules_len,
                    f.max_apply_mrules_bytes,
                    f.max_apply_templates_len,
                    f.max_apply_templates_bytes,
                    f.max_live_words,
                    f.max_live_bytes,
                );
            }
            // Field-level byte attribution over the live frontier (docs/research/word-memory-trace.md).
            if std::env::var("HC_WORD_STATS").is_ok() {
                let s = pg_rules::word_stats::snapshot();
                let b = &s.live_peak_breakdown;
                eprintln!(
                    "WORDSTATS\t{i}\t{word}\t\
                     live_peak_count={}\tlive_peak_total={}\t\
                     live_base={}\tlive_shape={}\tlive_syn_fs={}\tlive_real_fs={}\tlive_morphs={}\t\
                     live_mrule_apps={}\tlive_obligatory={}\tlive_unapplied_rule_counts={}\t\
                     live_root_runtime_id={}\tlive_non_heads={}\tlive_alternatives={}\t\
                     max_single_word_bytes={}\t\
                     alt_len_p50={}\talt_len_p90={}\talt_len_max={}\t\
                     non_head_len_p50={}	non_head_len_p90={}	non_head_len_max={}",
                    s.live_peak_count,
                    s.live_peak_total,
                    b.base,
                    b.shape,
                    b.syn_fs,
                    b.real_fs,
                    b.morphs,
                    b.mrule_apps,
                    b.obligatory,
                    b.unapplied_rule_counts,
                    b.root_runtime_id,
                    b.non_heads,
                    b.alternatives,
                    s.max_single_word_bytes,
                    s.alt_len_p50,
                    s.alt_len_p90,
                    s.alt_len_max,
                    s.non_head_len_p50,
                    s.non_head_len_p90,
                    s.non_head_len_max,
                );
            }
            // What `Word::alternatives` actually yields once expanded (docs/research/alt-yield.md).
            if std::env::var("HC_ALT_YIELD").is_ok() {
                let s = pg_parse::alt_yield::snapshot();
                let dropped = s.expanded_total.saturating_sub(s.distinct_identities);
                eprintln!(
                    "ALTYIELD\t{i}\t{word}\t\
                     canonical_alt_total={}\tcanonical_alt_max={}\t\
                     expanded_total={}\tdistinct_identities={}\tdropped_as_duplicate={}",
                    s.canonical_alt_total,
                    s.canonical_alt_max,
                    s.expanded_total,
                    s.distinct_identities,
                    dropped,
                );
            }
            // Per-word dead-branch census, printed beside ALTYIELD.
            if pg_rules::branch_census::enabled() {
                let s = pg_rules::branch_census::snapshot();
                eprintln!("BRANCHCENSUS\t{i}\t{word}\t{}", s.to_tsv());
            }
            // Ground-truth allocator peak/live bytes for this word (docs/research/word-memory-trace.md).
            #[cfg(feature = "alloc-trace")]
            if std::env::var("HC_ALLOC_STATS").is_ok() {
                eprintln!(
                    "ALLOC\t{i}\t{word}\tpeak_bytes={}\tlive_at_end={}",
                    alloc_trace::peak_bytes(),
                    alloc_trace::live_bytes(),
                );
                alloc_trace::reset_peak();
            }
            write_batch_row(
                &mut w,
                i,
                word,
                elapsed_ms,
                status,
                &signature,
                BatchRowTail::new(row_flags, &outcome),
            )
            .map_err(|e| e.to_string())?;
            if let Some(analyses_w) = analyses_w.as_mut() {
                write_parse_analysis_row(analyses_w, i, word, elapsed_ms, &outcome, &grammar)
                    .map_err(|e| e.to_string())?;
            }
            if flush_each_word {
                w.flush().map_err(|e| e.to_string())?; // per-line flush (AutoFlush), crash/monitor resumable
                if let Some(analyses_w) = analyses_w.as_mut() {
                    analyses_w.flush().map_err(|e| e.to_string())?;
                }
            }
        }
        if phase_marks {
            phase_mark();
        }
    } else {
        // Parallel path: hc_parse_batch parallelizes internally and returns results already reindexed to original word order; buffered and written once, no STARTED lines, so --start only skips work with no per-word crash-resume in this mode.
        let remaining = &words[start_idx..];
        let (results, result_start) = if stats_requested {
            let cache = stats_cache
                .as_ref()
                .expect("--stats initializes the batch stats cache before parsing");
            (
                parse_batch_with_stats(
                    &morpher,
                    &grammar,
                    stats_identities
                        .as_ref()
                        .expect("--stats creates a typed identity catalog"),
                    &words,
                    threads,
                    &opts,
                    start_idx,
                    cache.0.existing_words(),
                    parse_counter,
                )?,
                0,
            )
        } else {
            // Without a lexical pattern the guess branch never runs, so `hc_parse_batch` returns the same rows.
            let results = if let Some(prefilter) = prefilter.as_mut() {
                prefilter.parse_batch(&morpher, remaining, threads, &opts, parse_counter)
            } else if opts.guess_root && !morpher.lexical_patterns().is_empty() {
                parse_batch_with_opts(&morpher, remaining, threads, &opts, parse_counter)
            } else {
                hc_parse_batch(&morpher, remaining, threads)
            };
            (
                results
                    .into_iter()
                    .map(|outcome| BatchWordRun {
                        outcome,
                        stats: None,
                    })
                    .map(Some)
                    .collect(),
                start_idx,
            )
        };
        for (j, result) in results.into_iter().enumerate() {
            let i = result_start + j;
            let Some(mut result) = result else {
                continue;
            };
            if let Some(stats) = result.stats.take() {
                stats_words.push(stats);
            }
            if i < start_idx {
                continue;
            }
            let word = &words[i];
            let r = result.outcome;
            let elapsed_ms = r.elapsed.as_millis();
            let status = outcome_status(&r.outcome);
            let signature = match status {
                "SKIPPED" => {
                    skipped += 1;
                    "-".to_string()
                }
                "TIMEOUT" => {
                    timed_out_words += 1;
                    eprintln!("TIMEOUT\t{i}\t{word}");
                    "-".to_string()
                }
                "CAP" | "MEMCAP" => {
                    parsed += 1;
                    capped_words += 1;
                    eprintln!("{status}\t{i}\t{word}");
                    r.outcome.signature()
                }
                _ => {
                    parsed += 1;
                    r.outcome.signature()
                }
            };
            write_batch_row(
                &mut w,
                i,
                word,
                elapsed_ms,
                status,
                &signature,
                BatchRowTail::new(row_flags, &r.outcome),
            )
            .map_err(|e| e.to_string())?;
            if let Some(analyses_w) = analyses_w.as_mut() {
                write_parse_analysis_row(analyses_w, i, word, elapsed_ms, &r.outcome, &grammar)
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    let parse_elapsed_ms = t_parse.elapsed().as_secs_f64() * 1e3;
    w.flush().map_err(|e| e.to_string())?;
    if let Some(analyses_w) = analyses_w.as_mut() {
        analyses_w.flush().map_err(|e| e.to_string())?;
    }

    eprintln!("PARSEELAPSED\tengine=default\telapsed_ms={parse_elapsed_ms:.3}");
    if let Some(prefilter) = &prefilter {
        eprintln!("PREFILTER-TOTAL\t{}", prefilter.total_line());
    }
    eprintln!(
        "batch complete: {} words parsed ({} skipped), {} hit a search cap, {} timed out [threads={}]",
        parsed,
        skipped,
        capped_words,
        timed_out_words,
        threads,
    );
    if let Some((stats_cache, manifest_path)) = stats_cache {
        if let Some(manifest_path) = manifest_path {
            let source = stats_source_identity
                .expect("--stats-manifest captures a source identity with the grammar");
            let batch_options = stats_manifest::BatchOptions {
                engine: "hc",
                threads,
                work_cap,
                word_mem_cap,
                search_budget_semantics: stats_cmd::SEARCH_BUDGET_SEMANTICS,
                word_timeout_ms,
                guess,
                always_enforce_final_templates,
                start: start_idx,
                analyses_requested: analyses_path_arg.is_some(),
            };
            let _ = stats_cmd::finish_frozen_batch_stats_hc(
                stats_cache,
                stats_words,
                std::path::Path::new(manifest_path),
                source,
                batch_options,
            )?;
        } else {
            let _ = stats_cmd::finish_batch_stats_hc(stats_cache, stats_words)?;
        }
    }
    Ok(())
}

/// `generate <grammar> <root-morpheme-id> [other-morpheme-id ...]` keeps the positional HC-XML `<MorphemeId>` mode; `generate <grammar> --request <request.json>` resolves exact FieldWorks entry/MSA identities from an imported snapshot.
fn run_generate(args: &[String]) -> Result<(), String> {
    let Some(grammar_path) = args.first() else {
        return Err("usage: generate <grammar> <root-morpheme-id> [other-morpheme-id ...] | generate <grammar> --request <request.json>".into());
    };

    if args.get(1).is_some_and(|arg| arg == "--request") {
        if args.len() == 2 {
            return Err(generate_request::GenerateRequestError::RequestPathMissing.to_string());
        }
        if args.len() > 3 {
            return Err(generate_request::GenerateRequestError::MixedSelectors.to_string());
        }
        let request =
            generate_request::read_request_file(&args[2]).map_err(|error| error.to_string())?;
        let (grammar, warnings) = load_grammar(grammar_path)?;
        print_grammar_warnings(&warnings);
        let words = generate_request::generate_request_words(&grammar, &request)
            .map_err(|error| error.to_string())?;
        for word in words {
            println!("{word}");
        }
        return Ok(());
    }
    if args.iter().skip(1).any(|arg| arg == "--request") {
        return Err(generate_request::GenerateRequestError::MixedSelectors.to_string());
    }

    let [_, root_id, other_ids @ ..] = args else {
        return Err("usage: generate <grammar> <root-morpheme-id> [other-morpheme-id ...] | generate <grammar> --request <request.json>".into());
    };

    let (grammar, warnings) = load_grammar(grammar_path)?;
    print_grammar_warnings(&warnings);
    // The shared Morpher generation allowance covers seed selection and synthesis.
    let morpher = Morpher::new(&grammar, DEFAULT_GENERATION_WORK_CAP)
        .with_generation_cap(DEFAULT_GENERATION_WORK_CAP);

    let root = lex_entry_by_morpheme_id(&grammar, root_id)
        .ok_or_else(|| format!("no LexicalEntry with <MorphemeId>{root_id}</MorphemeId>"))?;
    let mut others = Vec::with_capacity(other_ids.len());
    for id in other_ids {
        others.push(
            gen_morpheme_by_morpheme_id(&grammar, id)
                .ok_or_else(|| format!("no morpheme with <MorphemeId>{id}</MorphemeId>"))?,
        );
    }

    let outcome =
        morpher.generate_words_bounded(root, &others, pg_featstruct::FeatureStruct::EMPTY);
    if let Some(reason) = outcome.stop_reason {
        return Err(format!("GenerateResourceLimit: {reason}"));
    }
    for w in &outcome.surfaces {
        println!("{w}");
    }
    Ok(())
}

/// The `LexEntryId` of the `LexicalEntry` whose `<MorphemeId>` text is `id`.
fn lex_entry_by_morpheme_id(g: &Grammar, id: &str) -> Option<LexEntryId> {
    g.entries
        .iter()
        .position(|e| g.morphemes[e.morpheme.0 as usize].morph_id.as_deref() == Some(id))
        .map(|idx| LexEntryId(idx as u32))
}

/// A `GenMorpheme` for the "other morpheme" whose `<MorphemeId>` text is `id`: either a `NonHead` lexical entry or a `Rule` (an affix-process/realizational rule; a `CompoundingRule` never has a `<MorphemeId>` of its own).
fn gen_morpheme_by_morpheme_id(g: &Grammar, id: &str) -> Option<GenMorpheme> {
    if let Some(le) = lex_entry_by_morpheme_id(g, id) {
        return Some(GenMorpheme::NonHead(le));
    }
    g.mrules.iter().enumerate().find_map(|(idx, r)| {
        let m = match r {
            MorphRuleDef::AffixProcess(d) => Some(d.morpheme),
            MorphRuleDef::Realizational(d) => Some(d.morpheme),
            MorphRuleDef::Compounding(_) => None,
        };
        m.filter(|&mid| g.morphemes[mid.0 as usize].morph_id.as_deref() == Some(id))
            .map(|_| GenMorpheme::Rule(MRuleId(idx as u32)))
    })
}

#[cfg(test)]
mod tests;
