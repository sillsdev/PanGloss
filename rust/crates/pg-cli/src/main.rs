//! `pangloss` — the standalone CLI mirroring C# `hc batch`'s TSV protocol so parity diffs against
//! managed golden runs are line-for-line comparable.
//!
//! `batch <grammar.xml> <words.txt> <out.tsv> [--step-cap N|unbounded] [--word-timeout-ms N] [--threads N] [--analyses <path>] [--always-enforce-final-templates]`
//! loads the grammar once and parses every word, writing the `BatchCommand`-compatible TSV.
//! `--analyses` additionally writes one FieldWorks `ParseAnalysis` JSONL row per input case while
//! preserving partial projections when a cap or timeout fires.
//! `--step-cap N` bounds the unmemoized analysis cascade (memoization removes the need); omitted,
//! it defaults to `DEFAULT_STEP_CAP` (50,000,000) so every batch terminates deterministically --
//! `--step-cap unbounded` opts back into no bound at all. See
//! `docs/research/step-cap-default-measurements.md` for the measurements behind that number.
//!
//! ## `--word-timeout-ms`
//! A second, independent bound: `--step-cap` bounds the *number* of analysis steps, but per-step
//! cost is not uniform — some pathological words legitimately spend far longer per step than
//! others (heavier narrowing/expansion analysis), so a step-count cap alone cannot bound wall-clock
//! time per word. `--word-timeout-ms N` arms a wall-clock deadline on the same shared
//! `pg_rules::stratum::StepBudget` the step cap already uses; whichever bound fires first wins,
//! and each is reported distinctly — a step-cap-exhausted word writes a `CAP` status row (its
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
//! separate since they come from different stages of the pipeline; exit is non-zero only on a
//! hard `pg_fwdata::ImportError` (I/O failure / not-a-`.fwdata`-or-`.fwbackup`-file), never on either warning list.
//!
//! ## `fst-health` (see `fst_health.rs`'s own doc for the full contract)
//! `fst-health <grammar> [<out.json>]` runs the cheap, grammar-only
//! `pg_foma::characterization::characterization_findings` pass and writes one canonical
//! `pg_foma::health::HealthReport`. It never compiles a backend or evaluates a corpus; corpus
//! measurements belong to a separate post-build operation over an explicitly completed artifact.
//! `<out.json>` omitted prints the JSON to stdout.
//!
//! ## `grammar-health` (see `grammar_health.rs`'s own doc for the full contract)
//! `grammar-health <grammar> [<out.json>]` runs the ported `hc-*` HermitCrab grammar-authoring
//! checks (`pg_grammar::grammar_health::check_grammar_health`) and prints/writes the findings as a
//! JSON array. A separate report from `fst-health`: this one asks whether the grammar is
//! well-formed for its author, not whether a compiled FST is production-ready. Always exits 0.
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
// `forbid` relaxes under `alloc-trace` (one `unsafe impl GlobalAlloc`) and on Windows (`winmem.rs`'s one `K32GetProcessMemoryInfo` call).
#![cfg_attr(not(any(feature = "alloc-trace", windows)), forbid(unsafe_code))]

#[cfg(feature = "alloc-trace")]
mod alloc_trace;
#[cfg(test)]
mod test_support;
#[cfg(windows)]
mod winmem;

use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use pg_grammar::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef};
use pg_parse::{
    hc_parse_batch,
    parse_morph::{project_parse_analysis, ParseAnalysis, PARSE_ANALYSIS_PROFILE},
    GenMorpheme, Morpher, WordAnalysis,
};
use pg_stats::StepCap;

mod assess;
// `pub` changes nothing for a binary crate; it marks these moved library modules' long docs as interface for comment-hygiene.
pub mod backend_report;
mod coverage;
mod fst_health;
// `pub`: see `backend_report`'s note above -- marks this bin module's long docs as interface for comment-hygiene.
pub mod grammar_health;
mod make_report;
mod pack;
mod plan_diagram;
pub mod readiness_policy;
pub mod readiness_verdict;
mod recipe_optimize;
mod stats_cmd;
mod surface;
mod trace_render;

/// Accepts experimental FST controls only in `developer-tools` builds, before positional parsing.
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

/// The `(status, signature)` pair both batch writers emit for a word that neither timed out nor was skipped.
fn row_status(outcome: &pg_parse::ParseOutcome) -> (&'static str, String) {
    if outcome.capped {
        ("CAP", outcome.signature())
    } else {
        ("ok", outcome.signature())
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

#[cfg(feature = "developer-tools")]
const REPORT_DEVELOPER_HELP: &str = " [--allow-unproven]";
#[cfg(not(feature = "developer-tools"))]
const REPORT_DEVELOPER_HELP: &str = "";

/// Ten times the highest step count any measured legitimate word reached across every corpus sampled; a runaway guard, never a performance tuning knob.
const DEFAULT_STEP_CAP: StepCap = StepCap::Finite(std::num::NonZeroU64::new(50_000_000).unwrap());

/// Allocator-level ground truth for `docs/research/word-memory-trace.md`; off unless built with `--features alloc-trace`.
#[cfg(feature = "alloc-trace")]
#[global_allocator]
static GLOBAL_ALLOC: alloc_trace::CountingAlloc = alloc_trace::CountingAlloc;

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
    eprintln!(
        "pangloss {} — HermitCrab Rust engine CLI\n\
         usage: pangloss batch <grammar> <words.txt> <out.tsv> [--step-cap N|unbounded] [--word-timeout-ms N] [--memo=on|off] [--threads N] [--start N] [--analyses <path>] [--guess] [--stats] [--cache <path>] [--always-enforce-final-templates]\n\
         usage: pangloss generate <grammar> <root-morpheme-id> [other-morpheme-id ...]\n\
         usage: pangloss parse <grammar> <word> [--trace[=<file>]] [--trace-format=text|json] [--gloss] [--natural-gloss=eng] [--realize-map=<path>] [--guess]\n\
         usage: pangloss import <project.fwdata/.fwbackup> <out.json>\n\
         usage: pangloss compare <baseline.json> <candidate.json> [--report <path>]\n\
         usage: pangloss golden-diff <report.json> --suite <suite.json> [--report <path>]\n\
         usage: pangloss investigate <report.json> --case <caseId> [--report <path>]\n\
         usage: pangloss fst-health <grammar> [<out.json>]\n\
         usage: pangloss grammar-health <grammar> [<out.json>]\n\
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
         \n\
         --guess (`batch`/`parse`, HC-rust port gap G3,\n\
         docs/hermitcrab-rust-port-audit.md sec 2/3 item 1): OFF by default, byte-identical\n\
         to the pre-existing behavior. When passed, an out-of-lexicon word whose normal\n\
         analysis is empty is retried via the lexical-pattern guesser (P11,\n\
         docs/p11-guesser-api-design.md); a resulting analysis is always clearly marked\n\
         guessed, never presented as confirmed -- `parse` prints an extra `guessed:` line,\n\
         `batch` appends a 6th `guessed` TSV column, both only when --guess is passed.\n\
         --always-enforce-final-templates is result-changing: enforce template order even\n\
         when partial-rule rescue would otherwise allow an interleaving.",
        env!("CARGO_PKG_VERSION"),
        REPORT_DEVELOPER_HELP
    );
    ExitCode::FAILURE
}

/// `import <project.fwdata/.fwbackup> <out.json>`: runs `pg-fwdata` over a FieldWorks project file and writes the resulting snapshot to `<out.json>`, printing import and validate warnings under separate headings; only a hard `ImportError` fails the command, since this pipeline must tolerate stale/dangling real-world project data.
fn run_import(args: &[String]) -> Result<(), String> {
    let [fwdata_path, out_path] = args else {
        return Err("usage: import <project.fwdata/.fwbackup> <out.json>".into());
    };

    let (snapshot, report) = pg_fwdata::import_file(std::path::Path::new(fwdata_path))
        .map_err(|e| format!("import {fwdata_path}: {e}"))?;

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

pub(crate) fn load_grammar(path: &str) -> Result<(Grammar, Vec<String>), String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "json" => {
            let json = fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            let snapshot = pg_snapshot::Snapshot::from_json(&json)
                .map_err(|e| format!("parse snapshot {path}: {e}"))?;
            let (grammar, warnings) = pg_grammar::compile_project(&snapshot)
                .map_err(|e| format!("compile {path}: {e:?}"))?;
            Ok((grammar, warnings))
        }
        _ if ext.eq_ignore_ascii_case("fwdata") || ext.eq_ignore_ascii_case("fwbackup") => {
            let (snapshot, report) = pg_fwdata::import_file(std::path::Path::new(path))
                .map_err(|e| format!("import {path}: {e}"))?;
            // report.warnings/snapshot.validate() are typed pg_snapshot::Warning; compile_project's are plain String, so flatten to prose here, the one place the two meet.
            let mut warnings: Vec<String> =
                report.warnings.into_iter().map(|w| w.to_string()).collect();
            warnings.extend(snapshot.validate().into_iter().map(|w| w.to_string()));
            let (grammar, compile_warnings) = pg_grammar::compile_project(&snapshot)
                .map_err(|e| format!("compile {path}: {e:?}"))?;
            warnings.extend(compile_warnings);
            Ok((grammar, warnings))
        }
        _ => {
            let xml = fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            let grammar = pg_grammar::load(&xml).map_err(|e| format!("load {path}: {e:?}"))?;
            Ok((grammar, Vec::new()))
        }
    }
}

/// Print `load_grammar`'s warnings to stderr, one per line, prefixed so they're easy to grep out
/// of a noisy log -- never to stdout (batch's TSV rows and parse's parity line are both
/// parity-sensitive; see the module doc).
pub(crate) fn print_grammar_warnings(warnings: &[String]) {
    for w in warnings {
        eprintln!("warning: {w}");
    }
}

/// `parse <grammar> <word> [flags...]`: traces, glosses, and/or realizes exactly one word's analyses. Flag semantics are detailed in the top-level usage banner; `--gloss`/`--natural-gloss` never touch the `word\tsignature` parity line, and a missing default-resolved realize-map sidecar degrades to empty while an explicitly named one failing is a hard error.
fn run_parse(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut trace_dest: Option<Option<String>> = None; // None = --trace not given; Some(None) = stdout; Some(Some(path)) = file
    let mut trace_format = "text".to_string();
    let mut gloss = false;
    let mut natural_gloss: Option<String> = None;
    let mut realize_map_arg: Option<String> = None;
    let mut guess = false;

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
            "--guess" => guess = true,
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
    if let Some(v) = &natural_gloss {
        if v != "eng" {
            return Err(format!(
                "unsupported --natural-gloss value: {v} (supported: eng)"
            ));
        }
    }
    let [grammar_path, word] = positional[..] else {
        return Err("usage: parse <grammar> <word> [--trace[=<file>]] [--trace-format=text|json] [--gloss] [--natural-gloss=eng] [--realize-map=<path>] [--guess]".into());
    };

    let (grammar, warnings) = load_grammar(grammar_path)?;
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

    // `parse` has no `--step-cap` flag of its own; the same finite default as `batch` guards a single interactive word against a runaway grammar.
    let morpher = Morpher::new(&grammar, DEFAULT_STEP_CAP.as_morpher_cap());
    // --guess omitted is exactly ParseOptions::default(), so every call below is byte-identical to the unconditional-default-options behavior.
    let opts = pg_parse::ParseOptions::default().with_guess_root(guess);

    if let Some(dest) = trace_dest {
        let sink = pg_rules::trace::TreeTraceSink::new();
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
    } else {
        // No --trace: behave like a minimal, single-word `batch` (the parse result only).
        let outcome = morpher.parse_word_opts(word, &opts);
        println!("{}\t{}", word, outcome.signature());
        print_guessed_line(guess, outcome.guessed);
        print_realize_lines(&grammar, &outcome.structured, word, gloss, natural.as_ref());
    }
    Ok(())
}

/// `--guess`'s own output marker: printed only when `--guess` was passed, right after the parity line and before any `--gloss`/`--natural-gloss` lines, so a guessed result is never indistinguishable from a confirmed one.
fn print_guessed_line(guess_requested: bool, guessed: bool) {
    if guess_requested {
        println!("guessed:\t{guessed}");
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

/// Writes one `batch` TSV result row: the plain 5-column row when `guess_requested` is `false`, with a 6th `guessed` column appended only when `--guess` was passed, so a guessed row is never indistinguishable from a confirmed one.
fn write_batch_row<W: Write>(
    w: &mut W,
    idx: usize,
    word: &str,
    elapsed_ms: u128,
    status: &str,
    signature: &str,
    // (guess_requested, guessed) -- bundled into one tuple to stay at clippy's 7-argument limit.
    guess: (bool, bool),
) -> std::io::Result<()> {
    let (guess_requested, guessed) = guess;
    if guess_requested {
        writeln!(
            w,
            "{idx}\t{word}\t{elapsed_ms}\t{status}\t{signature}\t{guessed}"
        )
    } else {
        writeln!(w, "{idx}\t{word}\t{elapsed_ms}\t{status}\t{signature}")
    }
}

/// `--guess`'s own parallel batch dispatch, called only when `--guess` was passed, since `hc_parse_batch` has no `ParseOptions` parameter and so cannot express "guess on"; deliberately simpler than that function's longest-surface-first scheduling, using a plain order-preserving `rayon` `par_iter`.
fn parse_batch_with_opts(
    morpher: &Morpher,
    words: &[String],
    max_threads: usize,
    opts: &pg_parse::ParseOptions,
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
                let outcome = morpher.parse_word_opts(word, opts);
                let elapsed = start.elapsed();
                pg_parse::BatchWordOutcome { outcome, elapsed }
            })
            .collect()
    })
}

fn run_batch(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut step_cap: StepCap = DEFAULT_STEP_CAP;
    // --word-timeout-ms: an optional wall-clock deadline per word, independent of --step-cap; None (omitted) is a complete no-op.
    let mut word_timeout_ms: Option<u64> = None;
    let mut memo = true;
    // Default (unspecified --threads only) is logical CPUs capped at 8, since per-word memory on a pathological grammar multiplies by thread count and an uncapped default can exhaust machine memory.
    const DEFAULT_THREAD_CAP: usize = 8;
    let mut threads: usize = std::thread::available_parallelism()
        .map(|n| n.get().min(DEFAULT_THREAD_CAP))
        .unwrap_or(1);
    // 0-based resume index: skip the first N words (already-completed rows from a prior crashed/killed run) and append rather than truncate out.tsv, so a watchdog wrapper can kill+relaunch a stalled word and continue where it left off.
    let mut start_idx: usize = 0;
    let mut analyses_path_arg: Option<String> = None;
    // --guess is default-off; guessed rows are always marked.
    let mut guess = false;
    // --stats: additionally drives the `pg_stats` cache (`stats_cmd.rs`); never touches the TSV rows above.
    let mut stats_requested = false;
    let mut always_enforce_final_templates = false;
    let mut cache_path_arg: Option<String> = None;
    let parse_memo = |v: &str| match v {
        "on" | "true" | "1" => Ok(true),
        "off" | "false" | "0" => Ok(false),
        other => Err(format!("invalid --memo: {other} (expected on|off)")),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--step-cap" => {
                let v = it.next().ok_or("--step-cap requires a value")?;
                step_cap = v
                    .parse::<StepCap>()
                    .map_err(|e| format!("invalid --step-cap: {e}"))?;
            }
            s if s.starts_with("--step-cap=") => {
                let v = &s["--step-cap=".len()..];
                step_cap = v
                    .parse::<StepCap>()
                    .map_err(|e| format!("invalid --step-cap: {e}"))?;
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
            "--memo" => {
                let v = it.next().ok_or("--memo requires a value")?;
                memo = parse_memo(v)?;
            }
            s if s.starts_with("--memo=") => {
                memo = parse_memo(&s["--memo=".len()..])?;
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
            "--guess" => guess = true,
            "--stats" => stats_requested = true,
            "--always-enforce-final-templates" => always_enforce_final_templates = true,
            "--cache" => {
                let v = it.next().ok_or("--cache requires a value")?;
                cache_path_arg = Some(v.clone());
            }
            s if s.starts_with("--cache=") => {
                cache_path_arg = Some(s["--cache=".len()..].to_string());
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
    let [grammar_path, words_path, out_path] = positional.as_slice() else {
        return Err(
            "usage: batch <grammar> <words.txt> <out.tsv> [--step-cap N|unbounded] [--word-timeout-ms N] [--memo=on|off] [--threads N] [--start N] [--analyses <path>] [--guess] [--stats] [--cache <path>] [--always-enforce-final-templates]"
                .into(),
        );
    };
    if analyses_path_arg.is_some() && start_idx > 0 {
        return Err("--start cannot be combined with --analyses".into());
    }

    // LOADTIME always prints unconditionally, since one line per invocation costs nothing.

    // Installed once, before any parsing, so every `HC_CLOCK_SAMPLE=1` reading can query it.
    #[cfg(feature = "alloc-trace")]
    pg_rules::clock_sample::set_allocator_query_hook(|| {
        (
            alloc_trace::peak_bytes() as u64,
            alloc_trace::live_bytes() as u64,
        )
    });

    let t_load = Instant::now();
    let (grammar, warnings) = load_grammar(grammar_path)?;
    print_grammar_warnings(&warnings);
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
    let words: Vec<String> = fs::read_to_string(words_path)
        .map_err(|e| format!("read {words_path}: {e}"))?
        .lines()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect();

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
    let mut morpher = Morpher::new(&grammar, step_cap.as_morpher_cap())
        .with_memo(memo)
        .with_word_timeout(word_timeout_ms.map(Duration::from_millis))
        .with_always_enforce_final_templates(always_enforce_final_templates);
    // Developer diagnostic mirroring HC_MEMO_STATS/HC_STEP_STATS: tune the memo byte budget without a rebuild ("none" disables it); unset leaves Morpher's own default.
    if let Some(v) = std::env::var("HC_MEMO_BYTES").ok() {
        let budget = if v.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(
                v.parse::<usize>()
                    .map_err(|e| format!("invalid HC_MEMO_BYTES: {v}: {e}"))?,
            )
        };
        morpher = morpher.with_memo_byte_budget(budget);
    }
    let morpher_build_ms = t_morpher.elapsed().as_secs_f64() * 1e3;
    eprintln!(
        "LOADTIME\tengine=default\tgrammar_load_ms={grammar_load_ms:.3}\tmorpher_build_ms={morpher_build_ms:.3}\ttotal_ms={:.3}",
        grammar_load_ms + morpher_build_ms
    );
    // --guess omitted is exactly ParseOptions::default(), so parse_word_opts below is byte-identical to parse_word(word).
    let opts = pg_parse::ParseOptions::default().with_guess_root(guess);

    // Printed unconditionally (not just under --stats) so `--stats`'s own overhead is measurable: without this, disabling --stats leaves no elapsed figure to compare against.
    let t_parse = Instant::now();
    if threads == 1 {
        // Legacy sequential path: STARTED sentinel + per-line flush, crash-resumable.
        for (i, word) in words.iter().enumerate() {
            if i < start_idx {
                continue;
            }
            writeln!(w, "{i}\t{word}\tSTARTED").map_err(|e| e.to_string())?;
            // Flush the STARTED sentinel immediately, before starting this word's parse, or it would only reach disk alongside the result line, defeating its purpose as a live in-flight signal for an external watchdog.
            w.flush().map_err(|e| e.to_string())?;
            let start = Instant::now();
            let outcome = morpher.parse_word_opts(word, &opts);
            let elapsed_ms = start.elapsed().as_millis();
            let (status, signature) = if outcome.invalid_shape {
                skipped += 1;
                ("SKIPPED", "-".to_string())
            } else if outcome.timed_out {
                // --word-timeout-ms fired, reported in the same TSV shape (idx\tword\tms\tTIMEOUT\t-) an external watchdog's synthetic row already uses for a killed stall, so downstream tooling needs no changes.
                timed_out_words += 1;
                eprintln!("TIMEOUT\t{i}\t{word}");
                ("TIMEOUT", "-".to_string())
            } else {
                parsed += 1;
                if outcome.capped {
                    capped_words += 1;
                    eprintln!("CAP\t{i}\t{word}");
                }
                row_status(&outcome)
            };
            // Diagnostic only: raw StepBudget tick count for this word, regardless of whether the cap fired.
            if std::env::var("HC_STEP_STATS").is_ok() {
                eprintln!("STEPS\t{i}\t{word}\t{}", outcome.steps);
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
            // `pg_memo::AnalysisScope`'s per-word lookup/hit/insert/fallthrough/size counters, opt-in since walking every insert's size is not free.
            if std::env::var("HC_MEMO_STATS").is_ok() {
                let s = pg_memo::profile::snapshot();
                let mean_results_len = if s.insert_samples > 0 {
                    s.insert_results_len_total as f64 / s.insert_samples as f64
                } else {
                    0.0
                };
                let mean_words = if s.insert_samples > 0 {
                    s.insert_words_total as f64 / s.insert_samples as f64
                } else {
                    0.0
                };
                eprintln!(
                    "MEMOPROF\t{i}\t{word}\t\
                     memo_lookups={}\tmemo_hits_pos={}\tmemo_hits_nogood={}\tmemo_inserts={}\tmemo_insert_refused={}\tmemo_refused_entries={}\tmemo_refused_words={}\tmemo_refused_bytes={}\tmemo_fallthrough={}\tmemo_max_in_progress={}\t\
                     tpl_lookups={}\ttpl_hits_pos={}\ttpl_hits_nogood={}\ttpl_inserts={}\ttpl_insert_refused={}\ttpl_refused_entries={}\ttpl_refused_words={}\ttpl_refused_bytes={}\ttpl_fallthrough={}\ttpl_max_in_progress={}\t\
                     insert_samples={}\tresults_len_mean={:.3}\tresults_len_max={}\twords_per_entry_mean={:.3}\twords_per_entry_max={}\t\
                     shape_seg_total={}\tsynfs_total={}\trealfs_total={}\tmorphs_total={}\treplay_clones={}\tmemo_evictions={}\t\
                     memo_heap_bytes={}\ttpl_heap_bytes={}",
                    s.memo_lookups,
                    s.memo_hits_positive,
                    s.memo_hits_nogood,
                    s.memo_inserts,
                    s.memo_insert_refused,
                    s.memo_insert_refused_entries,
                    s.memo_insert_refused_words,
                    s.memo_insert_refused_bytes,
                    s.memo_fallthrough,
                    s.memo_max_in_progress,
                    s.tpl_lookups,
                    s.tpl_hits_positive,
                    s.tpl_hits_nogood,
                    s.tpl_inserts,
                    s.tpl_insert_refused,
                    s.tpl_insert_refused_entries,
                    s.tpl_insert_refused_words,
                    s.tpl_insert_refused_bytes,
                    s.tpl_fallthrough,
                    s.tpl_max_in_progress,
                    s.insert_samples,
                    mean_results_len,
                    s.insert_results_len_max,
                    mean_words,
                    s.insert_words_max,
                    s.insert_shape_seg_total,
                    s.insert_synfs_total,
                    s.insert_realfs_total,
                    s.insert_morphs_total,
                    s.replay_clones,
                    s.memo_evictions,
                    s.memo_heap_bytes,
                    s.tpl_heap_bytes,
                );
            }
            // T5(b) (docs/research/memory-measurement-repair.md), extended by docs/research/memo-entry-work-value.md.
            if std::env::var("HC_MEMO_VALUE_STATS").is_ok() {
                let s = pg_rules::memo_value::snapshot();
                eprintln!(
                    "MEMOVALUE\t{i}\t{word}\tentries={}\ttotal_bytes={}\tzero_hit_entries={}\t\
                     zero_hit_bytes={}\tmean_hits={:.3}\tmean_results_len={:.3}\t\
                     mean_depth_at_insert={:.3}\tvalue_p50={:.6}\tvalue_p90={:.6}\t\
                     mean_subtree_work_inclusive={:.3}\tmean_subtree_work_exclusive={:.3}\t\
                     mean_descendant_count={:.3}\ttotal_work_saved_exclusive={}\ttotal_work={}",
                    s.entries,
                    s.total_bytes,
                    s.zero_hit_entries,
                    s.zero_hit_bytes,
                    s.mean_hits,
                    s.mean_results_len,
                    s.mean_depth_at_insert,
                    pg_rules::memo_value::value_per_byte_percentile(0.5),
                    pg_rules::memo_value::value_per_byte_percentile(0.9),
                    s.mean_subtree_work_inclusive,
                    s.mean_subtree_work_exclusive,
                    s.mean_descendant_count,
                    s.total_work_saved_exclusive,
                    outcome.steps,
                );
            }
            // Per-entry dump for docs/research/memo-entry-work-value.md; opt-in (one line per entry).
            if std::env::var("HC_MEMO_VALUE_DUMP").is_ok() {
                for e in pg_rules::memo_value::dump_entries() {
                    eprintln!(
                        "MEMOVALUEENTRY\t{i}\t{word}\tbytes={}\thits={}\twork_incl={}\twork_excl={}\t\
                         descendants={}\tresults_len={}\tdepth={}",
                        e.bytes,
                        e.hits,
                        e.subtree_work_inclusive,
                        e.subtree_work_exclusive,
                        e.descendant_count,
                        e.results_len,
                        e.depth_at_insert,
                    );
                }
            }
            // Peak live-search-frontier counters, independent of the memo caps above (`docs/research/live-frontier-memory-bound.md`).
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
            // Field-level byte attribution over the live frontier and the memo tables (docs/research/word-memory-trace.md).
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
                     non_head_len_p50={}\tnon_head_len_p90={}\tnon_head_len_max={}\t\
                     memo_key_bytes={}\tmemo_results_bytes={}\ttpl_key_bytes={}\ttpl_results_bytes={}",
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
                    s.memo_key_bytes,
                    s.memo_results_bytes,
                    s.tpl_key_bytes,
                    s.tpl_results_bytes,
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
            // T5(a): full vs delta cost per stored alternative (docs/research/memory-measurement-repair.md).
            if std::env::var("HC_ALT_DELTA_STATS").is_ok() {
                let s = pg_rules::alt_delta::snapshot();
                eprintln!(
                    "ALTDELTA\t{i}\t{word}\tsamples={}\tfull_bytes_total={}\tdelta_bytes_total={}\t\
                     mean_identical_field_frac={:.4}\tratio_p50={:.4}\tratio_p90={:.4}\tratio_max={:.4}",
                    s.samples,
                    s.full_bytes_total,
                    s.delta_bytes_total,
                    s.mean_identical_fields,
                    pg_rules::alt_delta::delta_full_ratio_percentile(0.5),
                    pg_rules::alt_delta::delta_full_ratio_percentile(0.9),
                    pg_rules::alt_delta::delta_full_ratio_percentile(1.0),
                );
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
            // Real OS-reported working set, its own column -- never summed with ALLOC or either model estimator.
            #[cfg(windows)]
            if std::env::var("HC_WS_STATS").is_ok() {
                match winmem::snapshot() {
                    Some(ws) => eprintln!(
                        "WS\t{i}\t{word}\tpeak_working_set_bytes={}\tcurrent_working_set_bytes={}",
                        ws.peak_working_set_bytes, ws.current_working_set_bytes,
                    ),
                    None => eprintln!("WS\t{i}\t{word}\tERROR=K32GetProcessMemoryInfo failed"),
                }
            }
            #[cfg(not(windows))]
            if std::env::var("HC_WS_STATS").is_ok() {
                eprintln!(
                    "WS\t{i}\t{word}\tERROR=working-set measurement is Windows-only (K32GetProcessMemoryInfo)"
                );
            }
            // Every pool sampled at the same instants (docs/research/memory-measurement-repair.md, T3).
            // `record` already no-ops unless HC_CLOCK_SAMPLE is set, so `take_samples` is empty then too.
            for s in pg_rules::clock_sample::take_samples() {
                let fmt_opt = |v: Option<u64>| v.map_or_else(|| "-".to_string(), |v| v.to_string());
                eprintln!(
                    "CLOCK\t{i}\t{word}\tseq={}\tpoint={}\talloc_peak_bytes={}\talloc_live_bytes={}\t\
                     live_frontier_bytes={}\tmemo_key_bytes={}\tmemo_results_bytes={}\t\
                     tpl_key_bytes={}\ttpl_results_bytes={}",
                    s.seq,
                    s.point,
                    fmt_opt(s.alloc_peak_bytes),
                    fmt_opt(s.alloc_live_bytes),
                    s.live_frontier_bytes,
                    s.memo_key_bytes,
                    s.memo_results_bytes,
                    s.tpl_key_bytes,
                    s.tpl_results_bytes,
                );
            }
            write_batch_row(
                &mut w,
                i,
                word,
                elapsed_ms,
                status,
                &signature,
                (guess, outcome.guessed),
            )
            .map_err(|e| e.to_string())?;
            if let Some(analyses_w) = analyses_w.as_mut() {
                write_parse_analysis_row(analyses_w, i, word, elapsed_ms, &outcome, &grammar)
                    .map_err(|e| e.to_string())?;
            }
            w.flush().map_err(|e| e.to_string())?; // per-line flush (AutoFlush), crash/monitor resumable
            if let Some(analyses_w) = analyses_w.as_mut() {
                analyses_w.flush().map_err(|e| e.to_string())?;
            }
        }
    } else {
        // Parallel path: hc_parse_batch parallelizes internally and returns results already reindexed to original word order; buffered and written once, no STARTED lines, so --start only skips work with no per-word crash-resume in this mode.
        let remaining = &words[start_idx..];
        // --guess omitted keeps calling hc_parse_batch unchanged; only the --guess path routes through the additive parse_batch_with_opts sibling.
        let results = if guess {
            parse_batch_with_opts(&morpher, remaining, threads, &opts)
        } else {
            hc_parse_batch(&morpher, remaining, threads)
        };
        for (j, (word, r)) in remaining.iter().zip(results.iter()).enumerate() {
            let i = start_idx + j;
            let elapsed_ms = r.elapsed.as_millis();
            let (status, signature) = if r.outcome.invalid_shape {
                skipped += 1;
                ("SKIPPED", "-".to_string())
            } else if r.outcome.timed_out {
                // Same --word-timeout-ms outcome as the sequential path above; the row shape is identical in both thread modes.
                timed_out_words += 1;
                eprintln!("TIMEOUT\t{i}\t{word}");
                ("TIMEOUT", "-".to_string())
            } else {
                parsed += 1;
                if r.outcome.capped {
                    capped_words += 1;
                    eprintln!("CAP\t{i}\t{word}");
                }
                row_status(&r.outcome)
            };
            write_batch_row(
                &mut w,
                i,
                word,
                elapsed_ms,
                status,
                &signature,
                (guess, r.outcome.guessed),
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
    eprintln!(
        "batch complete: {} words parsed ({} skipped), {} hit the step cap, {} timed out [memo={}, threads={}]",
        parsed,
        skipped,
        capped_words,
        timed_out_words,
        if memo { "on" } else { "off" },
        threads,
    );
    if stats_requested {
        let _ = stats_cmd::run_batch_stats_hc(
            &grammar,
            grammar_path,
            &morpher,
            &opts,
            &words,
            step_cap,
            word_timeout_ms,
            memo,
            guess,
            always_enforce_final_templates,
            cache_path_arg.as_deref(),
        )?;
    }
    Ok(())
}

/// `generate <grammar> <root-morpheme-id> [other-morpheme-id ...]`: a thin CLI wrapper over `Morpher::generate_words` with an empty realizational FS; each morpheme-id is the human-typable `<MorphemeId>` XML text, applied in exactly the order given, no interleaving search.
fn run_generate(args: &[String]) -> Result<(), String> {
    let [grammar_path, root_id, other_ids @ ..] = args else {
        return Err("usage: generate <grammar> <root-morpheme-id> [other-morpheme-id ...]".into());
    };

    let (grammar, warnings) = load_grammar(grammar_path)?;
    print_grammar_warnings(&warnings);
    // `generate` has no `--step-cap` flag either; `synthesis_pipeline` folds this same cap into its own `StepBudget`, so the finite default guards it too.
    let morpher = Morpher::new(&grammar, DEFAULT_STEP_CAP.as_morpher_cap());

    let root = lex_entry_by_morpheme_id(&grammar, root_id)
        .ok_or_else(|| format!("no LexicalEntry with <MorphemeId>{root_id}</MorphemeId>"))?;
    let mut others = Vec::with_capacity(other_ids.len());
    for id in other_ids {
        others.push(
            gen_morpheme_by_morpheme_id(&grammar, id)
                .ok_or_else(|| format!("no morpheme with <MorphemeId>{id}</MorphemeId>"))?,
        );
    }

    let words = morpher.generate_words(root, &others, pg_featstruct::FeatureStruct::EMPTY);
    for w in &words {
        println!("{w}");
    }
    eprintln!("generate complete: {} word(s)", words.len());
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
mod tests {
    //! `--word-timeout-ms` end-to-end plumbing: flag parsing, `Morpher` wiring, and the TSV row
    //! shape, exercised through `run_batch` itself (not just `Morpher::parse_word` --
    //! `pg-parse/tests/word_timeout_gate.rs` already covers the engine-level behavior) so a bug in
    //! this file's own flag parsing or row-writing can't hide behind a lower-level test passing.
    //! Covers both `--threads` writer paths per the task brief -- the sequential (`STARTED` +
    //! per-line flush) and rayon-parallel (buffered, no `STARTED`) modes have genuinely different
    //! code paths in `run_batch` and each needed its own bug fixed above.
    use super::{
        load_grammar, run_batch, write_parse_analysis_row, StepCap, DEFAULT_STEP_CAP,
    };
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A minimal, self-contained grammar (no phonological/morphological rules): one stratum, one table, one `LexicalEntry` whose surface form is "kat" — root-only lookup is enough to exercise the batch pipeline.
    const MINI_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MiniCliTest</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="seg6"><Name>SegId</Name>
        <Symbols><Symbol id="idA">a</Symbol><Symbol id="idT">t</Symbol><Symbol id="idK">k</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="seg1"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="seg6" symbolValues="idA" /></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations>
          <FeatureValue feature="seg6" symbolValues="idT" /></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations>
          <FeatureValue feature="seg6" symbolValues="idK" /></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="bdry1"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="le1" partOfSpeech="n">
            <Allomorphs><Allomorph id="le1-1"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kat</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    /// A fresh, collision-free scratch directory per test, since tests in one binary run concurrently by default.
    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "pangloss-cli-test-{tag}-{}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn fwbackup_extension_routes_through_fieldworks_importer() {
        for extension in ["fwbackup", "FWBACKUP"] {
            let missing = scratch_dir("missing-fwbackup").join(format!("missing.{extension}"));
            let err = load_grammar(&missing.to_string_lossy()).unwrap_err();
            assert!(
                err.starts_with("import "),
                "expected .{extension} to use pg-fwdata import, got: {err}"
            );
        }
    }

    /// Runs `batch` with `extra_args` appended after the 3 positional args, returning the written TSV's lines.
    fn run_batch_tsv(tag: &str, extra_args: &[&str]) -> Vec<String> {
        run_batch_tsv_custom(tag, MINI_GRAMMAR_XML, "kat\n", extra_args)
    }

    /// Like `run_batch_tsv`, but with a caller-supplied grammar/word-list body, used by the `--guess` tests below whose grammar needs a lexical pattern `MINI_GRAMMAR_XML` doesn't have.
    fn run_batch_tsv_custom(
        tag: &str,
        grammar_xml: &str,
        words_text: &str,
        extra_args: &[&str],
    ) -> Vec<String> {
        let dir = scratch_dir(tag);
        let grammar_path = dir.join("grammar.xml");
        let words_path = dir.join("words.txt");
        let out_path = dir.join("out.tsv");
        fs::write(&grammar_path, grammar_xml).expect("write grammar");
        fs::write(&words_path, words_text).expect("write words");

        let mut args: Vec<String> = vec![
            grammar_path.to_string_lossy().into_owned(),
            words_path.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
        ];
        args.extend(extra_args.iter().map(|s| s.to_string()));

        run_batch(&args).unwrap_or_else(|e| panic!("run_batch failed: {e}"));
        fs::read_to_string(&out_path)
            .expect("read out.tsv")
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn run_batch_sidecar_custom(
        tag: &str,
        grammar_xml: &str,
        words_text: &str,
        extra_args: &[&str],
    ) -> Vec<serde_json::Value> {
        let dir = scratch_dir(tag);
        let grammar_path = dir.join("grammar.xml");
        fs::write(&grammar_path, grammar_xml).expect("write grammar");
        run_batch_sidecar_path(tag, &grammar_path, words_text, extra_args)
    }

    fn run_batch_sidecar_path(
        tag: &str,
        grammar_path: &std::path::Path,
        words_text: &str,
        extra_args: &[&str],
    ) -> Vec<serde_json::Value> {
        let dir = scratch_dir(tag);
        let words_path = dir.join("words.txt");
        let out_path = dir.join("out.tsv");
        let analyses_path = dir.join("analyses.jsonl");
        fs::write(&words_path, words_text).expect("write words");

        let mut args: Vec<String> = vec![
            grammar_path.to_string_lossy().into_owned(),
            words_path.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
            "--analyses".to_string(),
            analyses_path.to_string_lossy().into_owned(),
        ];
        args.extend(extra_args.iter().map(|s| s.to_string()));

        run_batch(&args).unwrap_or_else(|e| panic!("run_batch failed: {e}"));
        fs::read_to_string(&analyses_path)
            .expect("read analyses.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).expect("valid sidecar JSON row"))
            .collect()
    }

    fn fwdata_fixture_with_k() -> std::path::PathBuf {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../pg-fwdata/tests/data/fixture.fwdata");
        let dir = scratch_dir("analyses-fwdata-source-guids");
        let target = dir.join("fixture-with-k.fwdata");
        let xml = fs::read_to_string(source).expect("read fwdata fixture");
        // Remove the unrelated dangling environment so this fixture isolates source-GUID projection.
        let dangling_env =
            "<objsur guid=\"00000000-0000-0000-0000-0000000000ff\" t=\"r\" />\r\n";
        assert!(
            xml.contains(dangling_env),
            "fixture dangling-environment shape changed"
        );
        let xml = xml.replacen(dangling_env, "", 1);
        let phoneme_ref = "<objsur guid=\"00000000-0000-0000-0000-00000000001b\" t=\"o\" />";
        let phoneme_ref_with_k = format!(
            "{phoneme_ref}\n<objsur guid=\"00000000-0000-0000-0000-000000000025\" t=\"o\" />"
        );
        assert!(
            xml.contains(phoneme_ref),
            "fixture phoneme set shape changed"
        );
        let xml = xml.replacen(phoneme_ref, &phoneme_ref_with_k, 1);
        let entry_marker = "<rt class=\"LexEntry\" guid=\"00000000-0000-0000-0000-000000000030\">";
        let k_records = r#"<rt class="PhPhoneme" guid="00000000-0000-0000-0000-000000000025" ownerguid="00000000-0000-0000-0000-00000000000f">
<Codes>
<objsur guid="00000000-0000-0000-0000-000000000026" t="o" />
</Codes>
<Name>
<AUni ws="fx">k</AUni>
</Name>
</rt>
<rt class="PhCode" guid="00000000-0000-0000-0000-000000000026" ownerguid="00000000-0000-0000-0000-000000000025">
<Representation>
<AUni ws="fx">k</AUni>
</Representation>
</rt>
"#;
        assert!(xml.contains(entry_marker), "fixture lexicon shape changed");
        let xml = xml.replacen(entry_marker, &format!("{k_records}{entry_marker}"), 1);
        fs::write(&target, xml).expect("write local fwdata fixture copy");
        target
    }

    #[test]
    fn analyses_sidecar_keeps_closed_rows_and_duplicate_word_indexes() {
        for (tag, threads) in [("analyses-seq", "1"), ("analyses-par", "2")] {
            let rows = run_batch_sidecar_custom(
                tag,
                MINI_GRAMMAR_XML,
                "kat\nkat\n",
                &["--threads", threads],
            );
            assert_eq!(rows.len(), 2, "threads={threads}: {rows:?}");
            for (expected_index, row) in rows.iter().enumerate() {
                assert_eq!(row["schema"], "fieldworks-parse-analysis/v1");
                assert_eq!(row["index"], expected_index);
                assert_eq!(row["word"], "kat");
                assert!(row["elapsedMs"].is_u64());
                assert_eq!(row["capped"], false);
                assert_eq!(row["timedOut"], false);
                assert_eq!(row["invalidShape"], false);
                assert!(row["analyses"].is_array());
                assert!(row["unavailable"].is_array());
                assert!(
                    row["analyses"].as_array().unwrap().len()
                        + row["unavailable"].as_array().unwrap().len()
                        > 0
                );
            }
        }
    }

    #[test]
    fn analyses_sidecar_projects_source_guids_from_fwdata() {
        let fixture = fwdata_fixture_with_k();
        let rows = run_batch_sidecar_path("analyses-fwdata-source-guids", &fixture, "kat\n", &[]);
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row["invalidShape"], false);
        assert_eq!(row["unavailable"], serde_json::json!([]));
        assert_eq!(row["analyses"].as_array().unwrap().len(), 1);
        let morph = row["analyses"][0]["morphs"][0]
            .as_object()
            .expect("the fwdata fixture analysis should project");
        assert_eq!(morph["form"], "00000000-0000-0000-0000-000000000031");
        assert_eq!(morph["msa"], "00000000-0000-0000-0000-000000000032");
        assert_eq!(morph["inflType"], serde_json::Value::Null);
        assert_eq!(morph["guessedString"], serde_json::Value::Null);
    }

    #[test]
    fn analyses_sidecar_preserves_cap_and_timeout_flags() {
        let grammar_xml = homophonous_suffix_grammar_xml(7);
        let capped_rows = run_batch_sidecar_custom(
            "analyses-cap-timeout",
            &grammar_xml,
            &format!("kad{}\n", "d".repeat(7)),
            &["--step-cap", "500", "--memo", "off"],
        );
        assert_eq!(capped_rows.len(), 1);
        assert_eq!(capped_rows[0]["capped"], true);
        assert_eq!(capped_rows[0]["timedOut"], false);
        assert!(capped_rows[0]["analyses"].is_array());
        assert!(capped_rows[0]["unavailable"].is_array());

        let timed_out_rows = run_batch_sidecar_custom(
            "analyses-timeout",
            MINI_GRAMMAR_XML,
            "kat\n",
            &["--word-timeout-ms", "0"],
        );
        assert_eq!(timed_out_rows.len(), 1);
        assert_eq!(timed_out_rows[0]["capped"], false);
        assert_eq!(timed_out_rows[0]["timedOut"], true);

        let grammar = pg_grammar::load(MINI_GRAMMAR_XML).expect("load mini grammar");
        let outcome = pg_parse::ParseOutcome {
            analyses: vec![("partial".into(), "kat".into())],
            structured: Vec::new(),
            capped: true,
            invalid_shape: false,
            steps: 1,
            timed_out: true,
            guessed: false,
            candidates_generated: 0,
        };
        let mut encoded = Vec::new();
        write_parse_analysis_row(&mut encoded, 0, "kat", 12, &outcome, &grammar)
            .expect("serialize mixed-bound outcome");
        let row: serde_json::Value = serde_json::from_slice(&encoded).expect("valid row");
        assert_eq!(row["capped"], true);
        assert_eq!(row["timedOut"], true);
    }

    #[test]
    fn analyses_sidecar_rejects_resume_and_path_collisions() {
        let dir = scratch_dir("analyses-paths");
        let grammar_path = dir.join("grammar.xml");
        let words_path = dir.join("words.txt");
        let out_path = dir.join("out.tsv");
        fs::write(&grammar_path, MINI_GRAMMAR_XML).expect("write grammar");
        fs::write(&words_path, "kat\n").expect("write words");

        let err = run_batch(&[
            grammar_path.to_string_lossy().into_owned(),
            words_path.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
            "--start".to_string(),
            "1".to_string(),
            "--analyses".to_string(),
            dir.join("analyses.jsonl").to_string_lossy().into_owned(),
        ])
        .expect_err("sidecar cannot resume from a nonzero start index");
        assert!(err.contains("--start"), "{err}");

        let err = run_batch(&[
            grammar_path.to_string_lossy().into_owned(),
            words_path.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
            "--analyses".to_string(),
            words_path.to_string_lossy().into_owned(),
        ])
        .expect_err("sidecar cannot overwrite the word list");
        assert!(err.contains("overwrite"), "{err}");

        let analyses_path = dir.join("analyses-cache-collision.jsonl");
        let err = run_batch(&[
            grammar_path.to_string_lossy().into_owned(),
            words_path.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
            "--analyses".to_string(),
            analyses_path.to_string_lossy().into_owned(),
            "--stats".to_string(),
            "--cache".to_string(),
            analyses_path.to_string_lossy().into_owned(),
        ])
        .expect_err("sidecar cannot overwrite the stats cache");
        assert!(err.contains("stats cache"), "{err}");

        let default_cache =
            pg_stats::default_cache_path(&grammar_path).expect("resolve default stats cache");
        let default_cache_text = default_cache.to_string_lossy().into_owned();
        let cache_before = fs::read(&default_cache).ok();
        let default_out = dir.join("default-cache-out.tsv");
        let err = run_batch(&[
            grammar_path.to_string_lossy().into_owned(),
            words_path.to_string_lossy().into_owned(),
            default_out.to_string_lossy().into_owned(),
            "--analyses".to_string(),
            default_cache_text,
            "--stats".to_string(),
        ])
        .expect_err("sidecar cannot overwrite the default stats cache");
        assert!(err.contains("stats cache"), "{err}");
        assert!(!default_out.exists(), "TSV output must not be created");
        match cache_before {
            Some(contents) => assert_eq!(fs::read(&default_cache).unwrap(), contents),
            None => assert!(!default_cache.exists(), "cache must not be created"),
        }
    }

    #[test]
    fn analyses_sidecar_invalid_shape_has_empty_projection_arrays() {
        let rows =
            run_batch_sidecar_custom("analyses-invalid-shape", MINI_GRAMMAR_XML, "zzz\n", &[]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["invalidShape"], true);
        assert_eq!(rows[0]["analyses"].as_array().unwrap().len(), 0);
        assert_eq!(rows[0]["unavailable"].as_array().unwrap().len(), 0);
    }

    /// Sequential path (`--threads 1`): a `--word-timeout-ms=0` deadline must produce a `STARTED` sentinel followed by a `TIMEOUT`/`-` result row, matching the shape an external watchdog synthesizes for a killed stall.
    #[test]
    fn word_timeout_ms_zero_writes_timeout_row_single_threaded() {
        let lines = run_batch_tsv("seq-timeout", &["--word-timeout-ms", "0", "--threads", "1"]);
        assert_eq!(lines.len(), 2, "STARTED sentinel + result row: {lines:?}");
        assert_eq!(lines[0], "0\tkat\tSTARTED");
        let fields: Vec<&str> = lines[1].split('\t').collect();
        assert_eq!(fields.len(), 5, "idx/word/ms/status/signature: {fields:?}");
        assert_eq!(fields[0], "0");
        assert_eq!(fields[1], "kat");
        fields[2]
            .parse::<u128>()
            .expect("ms column must be an integer");
        assert_eq!(fields[3], "TIMEOUT");
        assert_eq!(fields[4], "-");
    }

    /// Parallel path (`--threads 2`): same deadline, same row shape, but no `STARTED` line at all.
    #[test]
    fn word_timeout_ms_zero_writes_timeout_row_parallel() {
        let lines = run_batch_tsv("par-timeout", &["--word-timeout-ms", "0", "--threads", "2"]);
        assert_eq!(
            lines.len(),
            1,
            "no STARTED line in the parallel writer: {lines:?}"
        );
        let fields: Vec<&str> = lines[0].split('\t').collect();
        assert_eq!(fields.len(), 5);
        assert_eq!(fields[0], "0");
        assert_eq!(fields[1], "kat");
        fields[2]
            .parse::<u128>()
            .expect("ms column must be an integer");
        assert_eq!(fields[3], "TIMEOUT");
        assert_eq!(fields[4], "-");
    }

    /// Control: omitting `--word-timeout-ms` must keep producing the pre-existing `ok` row shape, in both thread modes.
    #[test]
    fn no_word_timeout_flag_keeps_ok_row_shape_both_thread_modes() {
        for (tag, threads) in [("seq-ok", "1"), ("par-ok", "2")] {
            let lines = run_batch_tsv(tag, &["--threads", threads]);
            let result_line = lines.last().expect("at least one line");
            let fields: Vec<&str> = result_line.split('\t').collect();
            assert_eq!(fields.len(), 5, "threads={threads}: {fields:?}");
            assert_eq!(fields[3], "ok", "threads={threads}: {fields:?}");
            assert_ne!(
                fields[4], "-",
                "threads={threads}: \"kat\" should analyze to a real signature"
            );
        }
    }

    /// `k` homophonous one-shot suffix rules unapplying "d", so an unmemoized unwind is genuinely combinatorial; `MINI_GRAMMAR_XML`'s root-only lookup takes zero steps and cannot exercise a real `--step-cap` firing.
    fn homophonous_suffix_grammar_xml(k: usize) -> String {
        let mut mrule_defs = String::new();
        let mut ids = Vec::with_capacity(k);
        for i in 0..k {
            mrule_defs.push_str(&format!(
                r#"<MorphologicalRule id="mrD{i}" requiredPartsOfSpeech="posV"><Name>d_suffix_{i}</Name><MorphemeId>D{i}</MorphemeId>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="subD{i}">
                  <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                  <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
            </MorphologicalRule>"#
            ));
            ids.push(format!("mrD{i}"));
        }
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>StepCapCliTest</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>Verb</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="{ids}">
        <Name>main</Name>
        <MorphologicalRuleDefinitions>{mrule_defs}</MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eroot" partOfSpeech="posV">
            <MorphemeId>ROOT</MorphemeId>
            <Allomorphs><Allomorph id="aroot"><PhoneticShape>kad</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#,
            ids = ids.join(" "),
        )
    }

    /// A small `--step-cap` fires on a genuinely combinatorial unmemoized unwind, in both thread modes; an incomplete outcome is typed `CAP`, never an `ok` row.
    #[test]
    fn step_cap_small_writes_cap_row_both_thread_modes() {
        let grammar_xml = homophonous_suffix_grammar_xml(7);
        let word = format!("kad{}\n", "d".repeat(7));
        for (tag, threads) in [("seq-cap", "1"), ("par-cap", "2")] {
            let lines = run_batch_tsv_custom(
                tag,
                &grammar_xml,
                &word,
                &["--step-cap", "500", "--memo", "off", "--threads", threads],
            );
            let result_line = lines.last().expect("at least one line");
            let fields: Vec<&str> = result_line.split('\t').collect();
            assert_eq!(fields.len(), 5, "threads={threads}: {fields:?}");
            assert_eq!(fields[3], "CAP", "threads={threads}: {fields:?}");
            assert!(
                !fields[4].is_empty(),
                "threads={threads}: partial signature column must not be empty: {fields:?}"
            );
        }
    }

    #[test]
    fn default_step_cap_is_fifty_million() {
        assert_eq!(
            DEFAULT_STEP_CAP,
            StepCap::Finite(std::num::NonZeroU64::new(50_000_000).unwrap())
        );
    }

    /// `--step-cap 0` is rejected before any file is touched, with the same message `StepCap::from_str` gives.
    #[test]
    fn step_cap_zero_is_rejected_with_a_specific_message() {
        let err = run_batch(&[
            "unused.xml".to_string(),
            "unused.txt".to_string(),
            "unused.tsv".to_string(),
            "--step-cap".to_string(),
            "0".to_string(),
        ])
        .expect_err("--step-cap 0 must be rejected");
        assert!(err.contains("fires before the first step"), "{err}");
    }

    /// `--step-cap unbounded` opts back into no bound at all; "kat" analyzes in far fewer than `DEFAULT_STEP_CAP` steps regardless, so this only proves the flag parses and the run still completes normally.
    #[test]
    fn step_cap_unbounded_parses_and_batch_completes_ok() {
        let lines = run_batch_tsv("unbounded-ok", &["--step-cap", "unbounded"]);
        let result_line = lines.last().expect("at least one line");
        let fields: Vec<&str> = result_line.split('\t').collect();
        assert_eq!(fields[3], "ok", "{fields:?}");
    }

    /// End-to-end `--guess` gate through `run_batch` itself, covering both `--threads` writer paths, using the same synthetic lexical-pattern grammar shape as the engine-level guesser conformance gate.
    mod guess_tests {
        use super::run_batch_tsv_custom;

        /// One lexical PATTERN entry ("[Any]*", matches every segment, so it is partitioned out of ordinary root lookup) plus one ordinary root ("kad") as the negative control.
        const GUESS_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>GuessCliTest</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>Verb</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRules="mrPast">
        <Name>Morphophonemic</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrPast" requiredPartsOfSpeech="posV">
            <Name>past_suffix</Name>
            <MorphemeId>PAST</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subPast">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="ePattern">
            <MorphemeId>PATTERN</MorphemeId>
            <Allomorphs><Allomorph id="aPattern"><PhoneticShape>[Any]*</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pattern</Gloss>
          </LexicalEntry>
          <LexicalEntry id="eKad" partOfSpeech="posV">
            <MorphemeId>KAD</MorphemeId>
            <Allomorphs><Allomorph id="aKad"><PhoneticShape>kad</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kad</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

        /// Omitting `--guess` must keep the plain 5-column row shape and never analyze the out-of-lexicon word "gag", in both thread modes.
        #[test]
        fn guess_omitted_keeps_five_column_rows_and_finds_nothing_for_pattern_only_word() {
            for (tag, threads) in [("guess-off-seq", "1"), ("guess-off-par", "2")] {
                let lines =
                    run_batch_tsv_custom(tag, GUESS_GRAMMAR_XML, "gag\n", &["--threads", threads]);
                assert_eq!(lines.len(), if threads == "1" { 2 } else { 1 }, "{lines:?}");
                let result_line = lines.last().unwrap();
                let fields: Vec<&str> = result_line.split('\t').collect();
                assert_eq!(
                    fields.len(),
                    5,
                    "threads={threads}: --guess omitted must keep the pre-existing 5-column row: {fields:?}"
                );
                assert_eq!(fields[3], "ok");
                assert_eq!(
                    fields[4], "-",
                    "threads={threads}: guessing off must find nothing for the pattern-only word"
                );
            }
        }

        /// `--guess` analyzes the out-of-lexicon word "gag" (matched only by the lexical pattern) and appends a 6th `guessed` column marked `true`, in both thread modes.
        #[test]
        fn guess_flag_analyzes_pattern_only_word_and_marks_the_row_guessed() {
            for (tag, threads) in [("guess-on-seq", "1"), ("guess-on-par", "2")] {
                let lines = run_batch_tsv_custom(
                    tag,
                    GUESS_GRAMMAR_XML,
                    "gag\n",
                    &["--threads", threads, "--guess"],
                );
                let result_line = lines.last().unwrap();
                let fields: Vec<&str> = result_line.split('\t').collect();
                assert_eq!(
                    fields.len(),
                    6,
                    "threads={threads}: --guess must append a 6th `guessed` column: {fields:?}"
                );
                assert_eq!(fields[3], "ok");
                assert_eq!(
                    fields[4], "gag|gag",
                    "threads={threads}: guessing on must analyze \"gag\" via the lexical pattern"
                );
                assert_eq!(
                    fields[5], "true",
                    "threads={threads}: a guessed analysis must be clearly marked, not silently \
                     indistinguishable from a confirmed one: {fields:?}"
                );
            }
        }

        /// Negative control: the ordinary lexical root "kad" is never marked guessed, since the guesser only fires on a genuine total miss.
        #[test]
        fn guess_flag_never_marks_the_ordinary_root_guessed() {
            for (tag, threads) in [("guess-control-seq", "1"), ("guess-control-par", "2")] {
                let lines = run_batch_tsv_custom(
                    tag,
                    GUESS_GRAMMAR_XML,
                    "kad\n",
                    &["--threads", threads, "--guess"],
                );
                let result_line = lines.last().unwrap();
                let fields: Vec<&str> = result_line.split('\t').collect();
                assert_eq!(fields.len(), 6, "{fields:?}");
                assert_eq!(fields[4], "KAD|kad");
                assert_eq!(
                    fields[5], "false",
                    "threads={threads}: an ordinary lexical hit must never be marked guessed: {fields:?}"
                );
            }
        }
    }
}
