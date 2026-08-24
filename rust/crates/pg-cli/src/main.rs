//! `pangloss` — the standalone CLI mirroring C# `hc batch`'s TSV protocol so parity diffs against
//! managed golden runs are line-for-line comparable.
//!
//! `batch <grammar.xml> <words.txt> <out.tsv> [--step-cap N] [--word-timeout-ms N] [--threads N]`
//! loads the grammar once and parses every word, writing the `BatchCommand`-compatible TSV.
//! `--step-cap N` bounds the unmemoized analysis cascade (memoization removes the need).
//!
//! ## `--word-timeout-ms`
//! A second, independent bound: `--step-cap` bounds the *number* of analysis steps, but per-step
//! cost is not uniform — some pathological words legitimately spend far longer per step than
//! others (heavier narrowing/expansion analysis), so a step-count cap alone cannot bound wall-clock
//! time per word. `--word-timeout-ms N` arms a wall-clock deadline on the same shared
//! `pg_rules::stratum::StepBudget` the step cap already uses; whichever bound fires first wins,
//! and each is reported distinctly — a step-cap-exhausted word still writes its (partial) `ok` row
//! with `CAP` on stderr (unchanged), while a timed-out word writes a `TIMEOUT` row with signature
//! `-` (see the TSV row format below). Omitted (the default) is a complete no-op: no clock is ever
//! read, and every existing invocation's output is unchanged.
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
//! ## `import` and `.json`/`.fwdata` grammar dispatch
//! `import <project.fwdata> <out.json>` runs `pg_fwdata::import_file` and writes the resulting
//! `pg_snapshot::Snapshot::to_json()` to `<out.json>`. `ImportReport` warnings (dangling refs,
//! unsupported constructs, log-and-skip decisions) and `Snapshot::validate()` warnings (dangling
//! GUID cross-references *within* the snapshot) are printed to stderr, clearly labeled and kept
//! separate since they come from different stages of the pipeline; exit is non-zero only on a
//! hard `pg_fwdata::ImportError` (I/O failure / not-a-`.fwdata`-file), never on either warning list.
//!
//! ## `diagnose` (see `diagnostics.rs`'s own doc for the full contract)
//! `diagnose <grammar> <words.txt> <out-dir>` writes `<out-dir>/build.json` and
//! `<out-dir>/assessment.json`: a build-side report (grammar identity/counts, an always-empty
//! `pg_foma::health::HealthReport` until a real evaluator lands) and a word-run-side report whose
//! entries reuse `pg_realize::word_gloss_signature` for gloss signatures and record each word's
//! in-process apply-path containment outcome
//! (`pg_foma::analyzer::FomaProposer::propose_budgeted`) — never a watchdog, which is compile-only.
//!
//! ## `fst-health` (see `fst_health.rs`'s own doc for the full contract)
//! `fst-health <grammar> [<words.txt>] [<out.json>]` runs `pg_foma::characterization::characterization_findings`
//! (a cheap, pre-compile pass) plus `pg_foma::health_evaluator::evaluate_health` (a standalone
//! profiled compile), and — only when `<words.txt>` is given — a caller-supplied word set's
//! proposal/confirmation counts, rejection share, and pre-dedup duplicate-analysis evidence, into
//! one canonical `pg_foma::health::HealthReport`. `<out.json>` omitted prints the JSON to stdout.
//!
//! Every other subcommand that takes a grammar path (`parse`, `batch`, `generate`, `diagnose`)
//! now dispatches on the path's extension via `load_grammar`: `.xml` (or anything else) is the
//! legacy HC-XML path (`pg_grammar::load`, unchanged, no warnings); `.json` loads a `pg-snapshot`
//! `Snapshot` (`Snapshot::from_json`) and compiles it (`pg_grammar::compile_project`); `.fwdata`
//! imports the FieldWorks project file directly, in-memory, then compiles it -- no intermediate
//! JSON file is written (run the `import` subcommand first if you want to keep the snapshot
//! around, e.g. to inspect it or reuse it without re-importing every time). Compile/import
//! warnings from `.json`/`.fwdata` dispatch are always printed to stderr, never stdout --
//! `batch`'s TSV rows are parity-sensitive against C# goldens, so warnings must never be
//! interleaved into that output stream.
#![forbid(unsafe_code)]

#[cfg(test)]
mod test_support;

use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use pg_foma::composite::FomaAnalyzer;
use pg_foma::resource_envelope::CompileSizeMode;
use pg_grammar::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef};
use pg_parse::{hc_parse_batch, GenMorpheme, Morpher, WordAnalysis};

mod assess;
mod coverage;
mod diagnostics;
mod fst_health;
mod make_report;
mod pack;
mod plan_diagram;
mod recipe_optimize;
mod stats_cmd;
mod trace_render;

/// Accepts experimental FST controls only in `developer-tools` builds, before positional parsing.
fn accept_developer_flag(arg: &str) -> Result<(), String> {
    debug_assert!(matches!(
        arg,
        "--allow-unproven" | "--remove-size-limits" | "--no-enforce-capability"
    ));
    #[cfg(feature = "developer-tools")]
    {
        let _ = arg;
        Ok(())
    }
    #[cfg(not(feature = "developer-tools"))]
    {
        Err(format!("unknown option: {arg} (developer-tools feature required)"))
    }
}

fn reject_unknown_option(arg: &str) -> Result<(), String> {
    if arg.starts_with("--") {
        Err(format!("unknown option: {arg}"))
    } else {
        Ok(())
    }
}

#[cfg(feature = "developer-tools")]
const PARSE_DEVELOPER_HELP: &str =
    " [--enforce-capability|--no-enforce-capability] [--allow-unproven] [--remove-size-limits]";
#[cfg(not(feature = "developer-tools"))]
const PARSE_DEVELOPER_HELP: &str = "";

#[cfg(feature = "developer-tools")]
const BATCH_DEVELOPER_HELP: &str =
    " [--enforce-capability|--no-enforce-capability] [--allow-unproven] [--remove-size-limits]";
#[cfg(not(feature = "developer-tools"))]
const BATCH_DEVELOPER_HELP: &str = "";

#[cfg(feature = "developer-tools")]
const PACK_DEVELOPER_HELP: &str =
    " [--allow-unproven] [--remove-size-limits]";
#[cfg(not(feature = "developer-tools"))]
const PACK_DEVELOPER_HELP: &str = "";

#[cfg(feature = "developer-tools")]
const REPORT_DEVELOPER_HELP: &str =
    " [--allow-unproven] [--remove-size-limits]";
#[cfg(not(feature = "developer-tools"))]
const REPORT_DEVELOPER_HELP: &str = "";

#[cfg(feature = "developer-tools")]
const DEVELOPER_HELP: &str =
    "Developer-only FST controls: --allow-unproven overrides correctness refusal for an unproven\
     development run; --remove-size-limits requests the planned stress mode; --no-enforce-\
     capability is retained only as a legacy parse/batch development switch.\n                 ";
#[cfg(not(feature = "developer-tools"))]
const DEVELOPER_HELP: &str = "";

#[cfg(feature = "developer-tools")]
const CAPABILITY_REFUSAL_REMEDIATION: &str =
    "pass --allow-unproven to force-compile anyway, see ADR 0005";
#[cfg(not(feature = "developer-tools"))]
const CAPABILITY_REFUSAL_REMEDIATION: &str =
    "consult the saved capability/readiness report or use a developer-tools build for an explicitly authorized override workflow";

/// Which proposer/verifier path a `batch`/`parse` invocation drives: `Default` is the full-search `pg_parse::Morpher` engine, `Foma` proposes via the compiled foma network and confirms via the same `Morpher` machinery; output shape is identical between engines by construction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Engine {
    Default,
    Foma,
}

impl Engine {
    fn parse(v: &str) -> Result<Self, String> {
        match v {
            "default" | "full" | "hc" => Ok(Engine::Default),
            "foma" => Ok(Engine::Foma),
            other => Err(format!("invalid --engine: {other} (expected default|foma)")),
        }
    }
}

/// Mirrors `Morpher::parse_word_core_selected`'s own shape-validity early return, since `FomaAnalyzer` has no equivalent check of its own, so the `--engine=foma` batch/parse paths call this directly to keep the ok-vs-SKIPPED status column identical to the default engine's.
fn foma_invalid_shape(g: &Grammar, word: &str) -> bool {
    let Some(last) = g.strata.last() else {
        return false;
    };
    let surface_table = &g.char_tables[last.table.0 as usize];
    pg_grammar::segment::segment(surface_table, word).is_err()
}

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
        Some("batch") => match run_batch(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pangloss batch: {e}");
                ExitCode::FAILURE
            }
        },
        Some("generate") => match run_generate(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pangloss generate: {e}");
                ExitCode::FAILURE
            }
        },
        Some("parse") => match run_parse(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pangloss parse: {e}");
                ExitCode::FAILURE
            }
        },
        Some("import") => match run_import(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pangloss import: {e}");
                ExitCode::FAILURE
            }
        },
        Some("assess") => assess::exit(assess::run_assess(&args[2..]), "assess"),
        Some("compare") => assess::exit(assess::run_compare(&args[2..]), "compare"),
        Some("golden-diff") => assess::exit(assess::run_golden_diff(&args[2..]), "golden-diff"),
        Some("investigate") => assess::exit(assess::run_investigate(&args[2..]), "investigate"),
        Some("diagnose") => match diagnostics::run_diagnose(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pangloss diagnose: {e}");
                ExitCode::FAILURE
            }
        },
        Some("pack") => match pack::run_pack(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pangloss pack: {e}");
                ExitCode::FAILURE
            }
        },
        Some("fst-health") => match fst_health::run_fst_health(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pangloss fst-health: {e}");
                ExitCode::FAILURE
            }
        },
        Some("coverage") => match coverage::run_coverage(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pangloss coverage: {e}");
                ExitCode::FAILURE
            }
        },
        Some("plan-diagram") => match plan_diagram::run_plan_diagram(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pangloss plan-diagram: {e}");
                ExitCode::FAILURE
            }
        },
        Some("make-report") => match make_report::run_make_report(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pangloss make-report: {e}");
                ExitCode::FAILURE
            }
        },
        Some("stats") => match stats_cmd::run_stats(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pangloss stats: {e}");
                ExitCode::FAILURE
            }
        },
        Some("recipe-optimize") => match recipe_optimize::run_recipe_optimize(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pangloss recipe-optimize: {e}");
                ExitCode::FAILURE
            }
        },
        Some("__recipe-optimize-child") => match recipe_optimize::run_recipe_optimize(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pangloss __recipe-optimize-child: {e}");
                ExitCode::FAILURE
            }
        },
        // Hidden compile-worker child entry point, spawned only by `pangloss pack --watchdog` re-execing this same binary — deliberately absent from the usage banner below.
        Some("__compile-worker-child") => {
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            match pg_foma::worker::run_worker_child(stdin.lock(), stdout.lock()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("pangloss __compile-worker-child: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!(
                "pangloss {} — HermitCrab Rust engine CLI\n\
                 usage: pangloss batch <grammar> <words.txt> <out.tsv> [--step-cap N] [--word-timeout-ms N] [--memo=on|off] [--threads N] [--start N] [--engine=default|foma]{} [--guess] [--stats] [--cache <path>]\n\
                 usage: pangloss generate <grammar> <root-morpheme-id> [other-morpheme-id ...]\n\
                 usage: pangloss parse <grammar> <word> [--trace[=<file>]] [--trace-format=text|json] [--gloss] [--natural-gloss=eng] [--realize-map=<path>] [--engine=default|foma]{} [--guess]\n\
                 usage: pangloss import <project.fwdata> <out.json>\n\
                 usage: pangloss diagnose <grammar> <words.txt> <out-dir>\n\
                 usage: pangloss assess <grammar> (--suite <suite.json> | --words <words.txt>) [--pipeline foma-confirm|hermitcrab] [--budget-paths N] [--budget-candidates N] [--report <path>]\n\
                 usage: pangloss compare <baseline.json> <candidate.json> [--report <path>]\n\
                 usage: pangloss golden-diff <report.json> --suite <suite.json> [--report <path>]\n\
                 usage: pangloss investigate <report.json> --case <caseId> [--grammar <path>] [--report <path>]\n\
                 usage: pangloss pack <grammar> <out.pgpack>{} [--authorized-by=<name>] [--reason=<text>] [--watchdog]\n\
                 usage: pangloss fst-health <grammar> [<words.txt>] [<out.json>]\n\
                 usage: pangloss coverage [--json] [--grammar=<path>] [<out.json>]\n\
                 usage: pangloss plan-diagram <grammar> [--json] [--full] [--threshold=N] [<out>]\n\
                 usage: pangloss make-report <grammar> <out.md> [--pack=<path>] [--words=<path>] [--corpus=<path> --attestor=<name> --attested-on=<date>] [--policy=<path>]{} [--authorized-by=<name>] [--reason=<text>] [--repeats=N]\n\
                 usage: pangloss recipe-optimize <grammar> <words.txt> <out-dir> [--seed N] [--candidates N] [--evaluations N] [--elapsed-ns N] [--build-ns N] [--memory-bytes N] [--confirmation-work N] [--reserve-ns N]\n\
                 usage: pangloss stats <project-or-grammar> [options] (run `pangloss stats` with no arguments to print the full current option list)\n\
                 \n\
                 <grammar> is one of: a HermitCrab XML export (.xml, the legacy path), a\n\
                 pg-snapshot JSON file (.json, from `pangloss import` or any other producer), or a\n\
                 FieldWorks project file (.fwdata, imported in-memory and compiled on the fly).\n\
                 \n\
                 Capability gate (ADR 0001/0005): --engine=foma is DEFAULT-ENFORCING -- a Refuse\n\
                 verdict fails hard under the production policy. --engine=default (the\n\
                 HC-oracle path) is never enforced, regardless of any flag. `pack`'s own\n\
                 pack is always capability-checked, and its trust record is PERSISTENT, written\n\
                 into the .pgpack manifest itself -- not a session-only analysis marker (see\n\
                 `pack.rs`'s module doc).\n\
                 \n\
                 {}--guess (`batch`/`parse`, --engine=default only; HC-rust port gap G3,\n\
                 docs/hermitcrab-rust-port-audit.md sec 2/3 item 1): OFF by default, byte-identical\n\
                 to the pre-existing behavior. When passed, an out-of-lexicon word whose normal\n\
                 analysis is empty is retried via the lexical-pattern guesser (P11,\n\
                 docs/p11-guesser-api-design.md); a resulting analysis is always clearly marked\n\
                 guessed, never presented as confirmed -- `parse` prints an extra `guessed:` line,\n\
                 `batch` appends a 6th `guessed` TSV column, both only when --guess is passed.",
                env!("CARGO_PKG_VERSION"),
                BATCH_DEVELOPER_HELP,
                PARSE_DEVELOPER_HELP,
                PACK_DEVELOPER_HELP,
                REPORT_DEVELOPER_HELP,
                DEVELOPER_HELP
            );
            ExitCode::FAILURE
        }
    }
}

/// `import <project.fwdata> <out.json>`: runs `pg-fwdata` over a FieldWorks project file and writes the resulting snapshot to `<out.json>`, printing import and validate warnings under separate headings; only a hard `ImportError` fails the command, since this pipeline must tolerate stale/dangling real-world project data.
fn run_import(args: &[String]) -> Result<(), String> {
    let [fwdata_path, out_path] = args else {
        return Err("usage: import <project.fwdata> <out.json>".into());
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

/// Load a `Grammar` from any of the three supported grammar-path shapes, dispatching on the
/// path's extension (see the module doc's "`import` and `.json`/`.fwdata` grammar dispatch"
/// section): `.json` -> `pg_snapshot::Snapshot::from_json` + `pg_grammar::compile_project`;
/// `.fwdata` -> `pg_fwdata::import_file` + `pg_grammar::compile_project` (in-memory, no
/// intermediate file); anything else (including `.xml`) -> the legacy `pg_grammar::load`, which
/// never produces warnings of its own. Returns any compile/import warnings alongside the
/// `Grammar` -- callers are responsible for printing them to stderr (never stdout; see the
/// module doc).
/// `load_grammar`, but keeping each warning's stable code instead of flattening it to prose.
///
/// `load_grammar` returns `Vec<String>` because almost every caller only ever prints warnings, and
/// changing that signature would churn every one of them. But an assessment report is exactly the
/// caller that must NOT lose the code: `compare` diffs importer diagnostics **by code and count**
/// so that rewording a message is never reported as a change in the grammar's context.
/// Flattening first and re-tagging everything `importer.warning` would give a caller one bucket,
/// which cannot distinguish "the importer skipped 400 more constructs" from "one message was
/// reworded".
///
/// So the two commands that build assessment reports use this; everything else keeps the simpler
/// shape.
///
/// `pg_grammar::compile_project`'s own warnings are still plain `String` and are tagged
/// `compiler.warning` here — honestly one bucket, because that is genuinely all the granularity
/// that exists on that side today, rather than a code invented to look finer.
pub(crate) fn load_grammar_coded(
    path: &str,
) -> Result<(Grammar, Vec<pg_snapshot::Warning>), String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    const COMPILER: &str = "compiler.warning";
    match ext {
        "json" => {
            let json = fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            let snapshot = pg_snapshot::Snapshot::from_json(&json)
                .map_err(|e| format!("parse snapshot {path}: {e}"))?;
            let mut warnings: Vec<pg_snapshot::Warning> = snapshot.validate();
            let (grammar, compile_warnings) = pg_grammar::compile_project(&snapshot)
                .map_err(|e| format!("compile {path}: {e:?}"))?;
            warnings.extend(
                compile_warnings
                    .into_iter()
                    .map(|w| pg_snapshot::Warning::new(COMPILER, w)),
            );
            Ok((grammar, warnings))
        }
        "fwdata" => {
            let (snapshot, report) = pg_fwdata::import_file(std::path::Path::new(path))
                .map_err(|e| format!("import {path}: {e}"))?;
            let mut warnings = report.warnings;
            warnings.extend(snapshot.validate());
            let (grammar, compile_warnings) = pg_grammar::compile_project(&snapshot)
                .map_err(|e| format!("compile {path}: {e:?}"))?;
            warnings.extend(
                compile_warnings
                    .into_iter()
                    .map(|w| pg_snapshot::Warning::new(COMPILER, w)),
            );
            Ok((grammar, warnings))
        }
        _ => {
            let xml = fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            let grammar = pg_grammar::load(&xml).map_err(|e| format!("load {path}: {e:?}"))?;
            Ok((grammar, Vec::new()))
        }
    }
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
        "fwdata" => {
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

#[cfg(feature = "developer-tools")]
fn resolve_compile_size_mode(
    engine: Engine,
    remove_size_limits: bool,
) -> Result<CompileSizeMode, String> {
    if remove_size_limits && engine != Engine::Foma {
        return Err("--remove-size-limits requires --engine=foma".to_string());
    }
    Ok(if remove_size_limits {
        CompileSizeMode::DeveloperStress
    } else {
        CompileSizeMode::Managed
    })
}

fn build_foma_analyzer<'g>(
    grammar: &'g Grammar,
    capability_overridden: bool,
    size_mode: CompileSizeMode,
) -> Result<FomaAnalyzer<'g>, pg_foma::analyzer::FomaError> {
    #[cfg(feature = "developer-tools")]
    {
        let (proposer, _profile) = if capability_overridden {
            pg_foma::analyzer::FomaProposer::new_unproven_with_profile_for_mode(grammar, size_mode)
        } else {
            pg_foma::analyzer::FomaProposer::new_with_profile_for_mode(grammar, size_mode)
        };
        return proposer.map(|proposer| FomaAnalyzer::from_precompiled_proposer(grammar, proposer));
    }

    #[cfg(not(feature = "developer-tools"))]
    {
        let _ = capability_overridden;
        let (proposer, _profile) =
            pg_foma::analyzer::FomaProposer::new_with_profile_for_mode(grammar, size_mode);
        proposer.map(|proposer| FomaAnalyzer::from_precompiled_proposer(grammar, proposer))
    }
}

/// Decides what `run_batch`/`run_parse` should do about the gated backend's `CompileDecision` for `g` (`gated_backend_decision` over `pg_foma::backend_selection::select_backends_for_grammar`'s report), given the resolved `enforce`/`allow_unproven` booleans, and what to print to stderr about it.
/// See `docs/research/pg-cli-main-design-notes.md` for the full enforce/override contract and why the override marker is session-level, not a persistent stamp.
struct GateResult {
    /// `false` only for `enforce == true, Refuse, !allow_unproven`; every other combination proceeds.
    proceed: bool,
    /// Lines for the caller to `eprintln!`, in order — stderr-only by construction.
    stderr_lines: Vec<String>,
    /// `true` iff this call force-compiled a `Refuse` via `allow_unproven`, exposed as a plain bool so a test can key off the degraded-trust fact directly rather than string-matching stderr.
    #[allow(dead_code)]
    overridden: bool,
}

/// Resolves the effective `enforce` boolean `capability_gate` takes, from the parsed `--engine` and the user's explicit enforce-capability choice.
/// See `docs/research/pg-cli-main-design-notes.md` for the hard scoping rule: only `--engine=foma` is ever gated.
fn resolve_capability_enforcement(engine: Engine, enforce_flag: Option<bool>) -> bool {
    if engine == Engine::Default {
        if enforce_flag == Some(true) {
            eprintln!(
                "capability: --enforce-capability has no effect with --engine=default -- the \
                 HC-oracle path always builds the exact HermitCrab-faithful analyzer and never \
                 relies on the FST proposer, so it is never gated; enforcement is scoped to \
                 --engine=foma only, see ADR 0001/0005."
            );
        }
        return false;
    }
    enforce_flag.unwrap_or(true)
}

/// The backend a `--engine=foma` run actually compiles with, and therefore the only one whose
/// compatibility report licenses that run.
pub(crate) const GATED_BACKEND: pg_foma::enumerate::EmissionStrategy =
    pg_foma::analyzer::FomaProposer::EMISSION_STRATEGY;

/// `GATED_BACKEND`'s own verdict out of `selection`, fail-closed: a backend the selector never
/// reported on is a refusal, never a silent pass, since "I could not look" must not read as "the
/// gate is satisfied".
pub(crate) fn gated_backend_decision(
    selection: &pg_foma::backend_selection::BackendSelection,
) -> pg_foma::capability::CompileDecision {
    use pg_foma::capability::{CapabilityDiagnostic, CompileDecision};
    match selection.report_for(GATED_BACKEND) {
        Some(report) => report.decision().clone(),
        None => CompileDecision::Refuse(vec![CapabilityDiagnostic {
            predicate: "capability-gate.backend-not-reported",
            construct: GATED_BACKEND.label().to_string(),
            witness: "the selector composed no compatibility report for the backend this run \
                      would compile with, so nothing licenses the run"
                .to_string(),
        }]),
    }
}

/// The label naming which backend a gate line is about, so a refusal says WHICH compiler declined.
fn gated_backend_tag() -> String {
    format!("backend={}", GATED_BACKEND.label())
}

fn capability_gate(g: &Grammar, enforce: bool, allow_unproven: bool) -> GateResult {
    use pg_foma::capability::CompileDecision;
    let selection = pg_foma::backend_selection::select_backends_for_grammar(g);
    let decision = gated_backend_decision(&selection);
    let backend = gated_backend_tag();

    if !enforce {
        // Unchanged: the exact pre-existing advisory-only report, regardless of `allow_unproven`.
        let stderr_lines = match &decision {
            CompileDecision::Admit => vec![format!(
                "capability: Admit [{backend}; advisory/preview -- gate not yet enforced, see ADR \
                 0001]"
            )],
            CompileDecision::ConfirmOnly => vec![format!(
                "capability: ConfirmOnly [{backend}; advisory/preview -- gate not yet enforced, \
                 see ADR 0001]"
            )],
            CompileDecision::Refuse(diags) => {
                let mut lines = vec![format!(
                    "capability: Refuse ({} diagnostic(s)) [{backend} declined; \
                     advisory/preview -- gate not yet enforced; compilation/analysis proceeds \
                     unchanged, see ADR 0001]",
                    diags.len()
                )];
                for d in diags {
                    lines.push(format!(
                        "  capability-refuse: predicate={} construct={} witness={}",
                        d.predicate, d.construct, d.witness
                    ));
                }
                lines
            }
        };
        return GateResult {
            proceed: true,
            stderr_lines,
            overridden: false,
        };
    }

    match decision {
        CompileDecision::Admit => GateResult {
            proceed: true,
            stderr_lines: vec![format!(
                "capability: Admit [{backend}; enforcing: gate satisfied, proceeding]"
            )],
            overridden: false,
        },
        CompileDecision::ConfirmOnly => GateResult {
            proceed: true,
            stderr_lines: vec![format!(
                "capability: ConfirmOnly [{backend}; enforcing: proceeding -- ConfirmOnly is a \
                 valid non-failure verdict per ADR 0001, recall-preserving via confirm]"
            )],
            overridden: false,
        },
        CompileDecision::Refuse(diags) => {
            if !allow_unproven {
                let mut lines = vec![format!(
                    "capability: Refuse ({} diagnostic(s)) [{backend} declined; enforcing: \
                     REFUSING -- no analysis will be performed, see ADR 0001; {CAPABILITY_REFUSAL_REMEDIATION}]",
                    diags.len()
                )];
                for d in &diags {
                    lines.push(format!(
                        "  capability-refuse: predicate={} construct={} witness={}",
                        d.predicate, d.construct, d.witness
                    ));
                }
                return GateResult {
                    proceed: false,
                    stderr_lines: lines,
                    overridden: false,
                };
            }

            // The override: force-compile behind an unmissable degraded-trust marker, repeating the machine-readable trust=unproven token at both the top and bottom of the block so a long diagnostic list can never let it scroll out of view.
            let mut lines = vec![format!(
                "CAPABILITY-OVERRIDE trust=unproven: --allow-unproven force-compiled {} construct(s) \
                 {backend} declined (ADR 0005) -- THIS RUN'S OUTPUT IS RECALL-UNSAFE, NOT a clean \
                 result. This is a SESSION/REPORT-LEVEL marker only for this invocation -- \
                 `batch`/`parse` write no persistent artifact of their own, so there is nothing \
                 for a pack-manifest stamp to attach to here. For a real, PERSISTENT, indelible \
                 ADR 0005 stamp, use `pangloss pack <grammar> <out.pgpack> --allow-unproven` \
                 instead, which writes this same override record into an actual .pgpack manifest.",
                diags.len()
            )];
            for d in &diags {
                lines.push(format!(
                    "  capability-override: predicate={} construct={} witness={}",
                    d.predicate, d.construct, d.witness
                ));
            }
            lines.push(format!(
                "CAPABILITY-OVERRIDE trust=unproven: end of override record -- {} config(s) \
                 force-compiled, this run's results are NOT recall-proven",
                diags.len()
            ));
            GateResult {
                proceed: true,
                stderr_lines: lines,
                overridden: true,
            }
        }
    }
}

/// Runs `capability_gate` over `g`, prints its `stderr_lines`, and returns `Err` on a gate refusal; called before any output file is created or analysis line printed, so a hard refusal truly produces no analysis output.
fn run_capability_gate(
    g: &Grammar,
    enforce: bool,
    allow_unproven: bool,
) -> Result<GateResult, String> {
    let gate = capability_gate(g, enforce, allow_unproven);
    for line in &gate.stderr_lines {
        eprintln!("{line}");
    }
    if gate.proceed {
        Ok(gate)
    } else {
        Err(format!(
            "capability gate refused this grammar under capability enforcement (diagnostics \
             printed above; ADR 0001); no analysis was performed. --engine=foma enforces by \
             default -- {CAPABILITY_REFUSAL_REMEDIATION}."
        ))
    }
}

/// `parse <grammar> <word> [flags...]`: traces, glosses, and/or realizes exactly one word's analyses. Flag semantics are detailed in the top-level usage banner; `--gloss`/`--natural-gloss` never touch the `word\tsignature` parity line, `--guess`/`--trace` only apply to `--engine=default`, and a missing default-resolved realize-map sidecar degrades to empty while an explicitly named one failing is a hard error.
fn run_parse(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut trace_dest: Option<Option<String>> = None; // None = --trace not given; Some(None) = stdout; Some(Some(path)) = file
    let mut trace_format = "text".to_string();
    let mut gloss = false;
    let mut natural_gloss: Option<String> = None;
    let mut realize_map_arg: Option<String> = None;
    let mut engine = Engine::Default;
    // None unless the user explicitly passed --enforce-capability/--no-enforce-capability; resolved to a plain bool once engine is final (see resolve_capability_enforcement).
    let mut enforce_capability_flag: Option<bool> = None;
    let mut allow_unproven = false;
    #[cfg(feature = "developer-tools")]
    let mut remove_size_limits = false;
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
            "--engine" => {
                let v = it.next().ok_or("--engine requires a value")?;
                engine = Engine::parse(v)?;
            }
            s if s.starts_with("--engine=") => {
                engine = Engine::parse(&s["--engine=".len()..])?;
            }
            "--enforce-capability" => enforce_capability_flag = Some(true),
            "--no-enforce-capability" => {
                accept_developer_flag(a)?;
                enforce_capability_flag = Some(false);
            }
            "--allow-unproven" => {
                accept_developer_flag(a)?;
                allow_unproven = true;
            }
            "--remove-size-limits" => {
                accept_developer_flag(a)?;
                #[cfg(feature = "developer-tools")]
                {
                    remove_size_limits = true;
                }
            }
            "--guess" => guess = true,
            s => {
                reject_unknown_option(s)?;
                positional.push(s);
            }
        }
    }
    #[cfg(feature = "developer-tools")]
    let size_mode = resolve_compile_size_mode(engine, remove_size_limits)?;
    #[cfg(not(feature = "developer-tools"))]
    let size_mode = CompileSizeMode::Managed;
    let enforce_capability = resolve_capability_enforcement(engine, enforce_capability_flag);
    if trace_format != "text" && trace_format != "json" {
        return Err(format!(
            "invalid --trace-format: {trace_format} (expected text|json)"
        ));
    }
    if engine == Engine::Foma && trace_dest.is_some() {
        return Err(
            "--trace is not supported with --engine=foma (the foma path has no trace \
                     sink of its own; use the default engine for tracing)"
                .into(),
        );
    }
    if engine == Engine::Foma && guess {
        return Err(
            "--guess is not supported with --engine=foma (the foma path proposes only from the \
             real lexicon; use the default engine for the lexical-pattern guesser)"
                .into(),
        );
    }
    if let Some(v) = &natural_gloss {
        if v != "eng" {
            return Err(format!(
                "unsupported --natural-gloss value: {v} (supported: eng)"
            ));
        }
    }
    let [grammar_path, word] = positional[..] else {
        return Err(format!("usage: parse <grammar> <word> [--trace[=<file>]] [--trace-format=text|json] [--gloss] [--natural-gloss=eng] [--realize-map=<path>] [--engine=default|foma]{} [--guess]", PARSE_DEVELOPER_HELP));
    };

    let (grammar, warnings) = load_grammar(grammar_path)?;
    print_grammar_warnings(&warnings);
    let gate = run_capability_gate(&grammar, enforce_capability, allow_unproven)?;

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

    if engine == Engine::Foma {
        // --trace was already rejected above, so this routes through FomaAnalyzer::analyze_word instead of Morpher::parse_word, with output shape identical to the default engine's.
        let mut analyzer = build_foma_analyzer(&grammar, gate.overridden, size_mode)
            .map_err(|e| format!("foma compile failed for {grammar_path}: {e}"))?;
        let (analyses, structured) = if foma_invalid_shape(&grammar, word) {
            (Vec::new(), Vec::new())
        } else {
            let outcome = analyzer.analyze_word(word);
            (outcome.analyses, outcome.structured)
        };
        println!("{}\t{}", word, pg_parse::result_signature(&analyses));
        print_realize_lines(&grammar, &structured, word, gloss, natural.as_ref());
        return Ok(());
    }

    let morpher = Morpher::new(&grammar, usize::MAX);
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

/// For each analysis, optionally prints a `gloss:` line then an `eng:` line, interleaved per analysis (not two separate passes) so a reader can tell which `eng:` line goes with which `gloss:` line; reads `.structured`, not `.analyses`, since `gloss_bundle` needs numeric morpheme ordinals. Shared by both engines' `parse` output, so the lines are byte-for-byte identical in shape regardless of which engine produced them.
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
    let mut step_cap: usize = usize::MAX;
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
    // --engine=foma routes the whole batch through FomaAnalyzer instead of Morpher.
    let mut engine = Engine::Default;
    // None unless the user explicitly passed --enforce-capability/--no-enforce-capability; resolved to a plain bool once engine is final.
    let mut enforce_capability_flag: Option<bool> = None;
    let mut allow_unproven = false;
    #[cfg(feature = "developer-tools")]
    let mut remove_size_limits = false;
    // Same shared --guess contract as run_parse: default-off, guessed rows always marked, --engine=default only.
    let mut guess = false;
    // --stats: additionally drives the `pg_stats` cache (`stats_cmd.rs`); never touches the TSV rows above.
    let mut stats_requested = false;
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
                step_cap = v.parse().map_err(|_| format!("invalid --step-cap: {v}"))?;
            }
            s if s.starts_with("--step-cap=") => {
                let v = &s["--step-cap=".len()..];
                step_cap = v.parse().map_err(|_| format!("invalid --step-cap: {v}"))?;
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
            "--engine" => {
                let v = it.next().ok_or("--engine requires a value")?;
                engine = Engine::parse(v)?;
            }
            s if s.starts_with("--engine=") => {
                engine = Engine::parse(&s["--engine=".len()..])?;
            }
            "--enforce-capability" => enforce_capability_flag = Some(true),
            "--no-enforce-capability" => {
                accept_developer_flag(a)?;
                enforce_capability_flag = Some(false);
            }
            "--allow-unproven" => {
                accept_developer_flag(a)?;
                allow_unproven = true;
            }
            "--remove-size-limits" => {
                accept_developer_flag(a)?;
                #[cfg(feature = "developer-tools")]
                {
                    remove_size_limits = true;
                }
            }
            "--guess" => guess = true,
            "--stats" => stats_requested = true,
            "--cache" => {
                let v = it.next().ok_or("--cache requires a value")?;
                cache_path_arg = Some(v.clone());
            }
            s if s.starts_with("--cache=") => {
                cache_path_arg = Some(s["--cache=".len()..].to_string());
            }
            s => {
                reject_unknown_option(s)?;
                positional.push(s);
            }
        }
    }
    #[cfg(feature = "developer-tools")]
    let size_mode = resolve_compile_size_mode(engine, remove_size_limits)?;
    #[cfg(not(feature = "developer-tools"))]
    let size_mode = CompileSizeMode::Managed;
    let enforce_capability = resolve_capability_enforcement(engine, enforce_capability_flag);
    if threads == 0 {
        return Err("--threads must be >= 1".into());
    }
    if engine == Engine::Foma && guess {
        return Err(
            "--guess is not supported with --engine=foma (the foma path proposes only from the \
             real lexicon; use the default engine for the lexical-pattern guesser)"
                .into(),
        );
    }
    let [grammar_path, words_path, out_path] = positional.as_slice() else {
        return Err(
            format!("usage: batch <grammar> <words.txt> <out.tsv> [--step-cap N] [--word-timeout-ms N] [--memo=on|off] [--threads N] [--start N] [--engine=default|foma]{} [--guess] [--stats] [--cache <path>]", BATCH_DEVELOPER_HELP)
                .into(),
        );
    };

    // One-time cost instrumentation comparing grammar load against load+foma-compile; LOADTIME always prints unconditionally, since one line per invocation costs nothing.
    let t_load = Instant::now();
    let (grammar, warnings) = load_grammar(grammar_path)?;
    print_grammar_warnings(&warnings);
    let grammar_load_ms = t_load.elapsed().as_secs_f64() * 1e3;
    // Computed after grammar_load_ms so the gate's own cost never perturbs the LOADTIME diagnostic, and before out_path is created, so a foma-path refusal truly produces no analysis output.
    let gate = run_capability_gate(&grammar, enforce_capability, allow_unproven)?;

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

    let mut parsed = 0u64;
    let mut skipped = 0u64;
    let mut capped_words = 0u64;
    let mut timed_out_words = 0u64;

    if engine == Engine::Foma {
        // The foma path: one FomaAnalyzer built once and reused across every word. --step-cap/--memo are silently ignored (the internal verifier Morpher is always uncapped), --word-timeout-ms applies via with_word_timeout, and threads > 1 routes through analyze_words (confirm parallelized, propose sequential since the single ApplyHandle can't be split across threads) with results buffered and written once, no per-word crash-resume.
        let t_compile = Instant::now();
        let mut analyzer = build_foma_analyzer(&grammar, gate.overridden, size_mode)
            .map_err(|e| format!("foma compile failed for {grammar_path}: {e}"))?
            .with_word_timeout(word_timeout_ms.map(Duration::from_millis));
        let compile_ms = t_compile.elapsed().as_secs_f64() * 1e3;
        eprintln!(
            "LOADTIME\tengine=foma\tgrammar_load_ms={grammar_load_ms:.3}\tanalyzer_build_ms={compile_ms:.3}\ttotal_ms={:.3}",
            grammar_load_ms + compile_ms
        );
        // Candidate-count/confirm distributions, gated behind an env var since it is one line per word, too much for a default-on diagnostic on a large corpus.
        let stats_on = std::env::var("HC_FOMA_STATS").is_ok();

        // Printed unconditionally (not just under --stats) so `--stats`'s own overhead is measurable: without this, disabling --stats leaves no elapsed figure to compare against.
        let t_parse = Instant::now();
        if threads == 1 {
            for (i, word) in words.iter().enumerate() {
                if i < start_idx {
                    continue;
                }
                writeln!(w, "{i}\t{word}\tSTARTED").map_err(|e| e.to_string())?;
                w.flush().map_err(|e| e.to_string())?;
                let start = Instant::now();
                let (status, signature) = if foma_invalid_shape(&grammar, word) {
                    skipped += 1;
                    ("SKIPPED", "-".to_string())
                } else {
                    let outcome = analyzer.analyze_word(word);
                    parsed += 1;
                    if stats_on {
                        eprintln!(
                            "CANDSTATS\t{i}\t{word}\tcandidates_generated={}\tconfirmed={}",
                            outcome.candidates_generated, outcome.confirmed
                        );
                    }
                    ("ok", pg_parse::result_signature(&outcome.analyses))
                };
                let elapsed_ms = start.elapsed().as_millis();
                writeln!(w, "{i}\t{word}\t{elapsed_ms}\t{status}\t{signature}")
                    .map_err(|e| e.to_string())?;
                w.flush().map_err(|e| e.to_string())?;
            }
        } else {
            // Parallel path: skip foma_invalid_shape words up front, since analyze_words has no shape check of its own.
            let remaining = &words[start_idx..];
            let mut valid_idx: Vec<usize> = Vec::new();
            let mut valid_words: Vec<String> = Vec::new();
            for (j, word) in remaining.iter().enumerate() {
                if foma_invalid_shape(&grammar, word) {
                    skipped += 1;
                } else {
                    valid_idx.push(j);
                    valid_words.push(word.clone());
                }
            }
            let results = analyzer.analyze_words(&valid_words);
            let mut rows: Vec<Option<(u128, &'static str, String)>> =
                (0..remaining.len()).map(|_| None).collect();
            for (k, &j) in valid_idx.iter().enumerate() {
                let (outcome, elapsed) = &results[k];
                parsed += 1;
                if stats_on {
                    eprintln!(
                        "CANDSTATS\t{}\t{}\tcandidates_generated={}\tconfirmed={}",
                        start_idx + j,
                        remaining[j],
                        outcome.candidates_generated,
                        outcome.confirmed
                    );
                }
                rows[j] = Some((
                    elapsed.as_millis(),
                    "ok",
                    pg_parse::result_signature(&outcome.analyses),
                ));
            }
            for (j, word) in remaining.iter().enumerate() {
                let i = start_idx + j;
                let (elapsed_ms, status, signature) =
                    rows[j].take().unwrap_or((0, "SKIPPED", "-".to_string()));
                writeln!(w, "{i}\t{word}\t{elapsed_ms}\t{status}\t{signature}")
                    .map_err(|e| e.to_string())?;
            }
        }
        let parse_elapsed_ms = t_parse.elapsed().as_secs_f64() * 1e3;
        w.flush().map_err(|e| e.to_string())?;
        eprintln!("PARSEELAPSED\tengine=foma\telapsed_ms={parse_elapsed_ms:.3}");
        eprintln!(
            "batch complete: {} words parsed ({} skipped) [engine=foma, threads={}]",
            parsed, skipped, threads,
        );
        if stats_requested {
            stats_cmd::run_batch_stats_foma(
                &grammar,
                grammar_path,
                &mut analyzer,
                &words,
                word_timeout_ms,
                cache_path_arg.as_deref(),
            )?;
        }
        return Ok(());
    }

    let t_morpher = Instant::now();
    let morpher = Morpher::new(&grammar, step_cap)
        .with_memo(memo)
        .with_word_timeout(word_timeout_ms.map(Duration::from_millis));
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
                ("ok", outcome.signature())
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
            w.flush().map_err(|e| e.to_string())?; // per-line flush (AutoFlush), crash/monitor resumable
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
                ("ok", r.outcome.signature())
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
        }
    }
    let parse_elapsed_ms = t_parse.elapsed().as_secs_f64() * 1e3;
    w.flush().map_err(|e| e.to_string())?;

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
        stats_cmd::run_batch_stats_hc(
            &grammar,
            grammar_path,
            &morpher,
            &opts,
            &words,
            step_cap,
            word_timeout_ms,
            memo,
            guess,
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
    let morpher = Morpher::new(&grammar, usize::MAX);

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
    use super::run_batch;
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

        /// `--guess --engine=foma` must be a hard, explicit error, never a silent no-op, since the foma path has no guesser of its own.
        #[test]
        fn guess_with_foma_engine_is_a_hard_error() {
            let dir = super::scratch_dir("guess-foma-conflict");
            let grammar_path = dir.join("grammar.xml");
            let words_path = dir.join("words.txt");
            let out_path = dir.join("out.tsv");
            std::fs::write(&grammar_path, GUESS_GRAMMAR_XML).unwrap();
            std::fs::write(&words_path, "gag\n").unwrap();
            let args = vec![
                grammar_path.to_string_lossy().into_owned(),
                words_path.to_string_lossy().into_owned(),
                out_path.to_string_lossy().into_owned(),
                "--engine=foma".to_string(),
                "--guess".to_string(),
            ];
            let err = super::run_batch(&args).expect_err("--guess --engine=foma must error");
            assert!(err.contains("--guess"), "{err}");
        }
    }

    /// Covers `capability_gate`'s pure boolean contract directly, and `resolve_capability_enforcement`'s engine-scoping policy end-to-end through `run_batch`: `--engine=default` never enforces, `--engine=foma` enforces by default, `--no-enforce-capability` opts back out, and `--allow-unproven` still overrides a foma-path refusal.
    mod capability_gate_tests {
        use super::super::{capability_gate, run_batch};
        use std::fs;
        use std::sync::atomic::{AtomicU32, Ordering};

        /// A grammar the gated backend declines on a structural fact about the fixture rather than pending a proof; see `crate::test_support::BACKEND_REFUSED_GRAMMAR_XML` for the shape and why it still compiles.
        const PERMANENTLY_REFUSED_GRAMMAR_XML: &str =
            crate::test_support::BACKEND_REFUSED_GRAMMAR_XML;

        fn load(xml: &str) -> pg_grammar::model::Grammar {
            pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
        }

        // --- Unit-level: `capability_gate` itself, no process I/O ------------------------------

        /// No flags: behavior must be advisory-only for both an `Admit` grammar and a `Refuse` grammar, per `capability_gate`'s own bool contract.
        #[test]
        fn capability_gate_no_flags_never_blocks_either_grammar() {
            let clean = load(super::MINI_GRAMMAR_XML);
            let refused = load(PERMANENTLY_REFUSED_GRAMMAR_XML);

            let g1 = capability_gate(&clean, false, false);
            assert!(
                g1.proceed,
                "advisory-only must never block an Admit grammar"
            );
            assert!(!g1.overridden);

            let g2 = capability_gate(&refused, false, false);
            assert!(
                g2.proceed,
                "advisory-only must never block even a Refuse grammar (unchanged default \
                 behavior)"
            );
            assert!(!g2.overridden);
        }

        /// `--enforce-capability` alone (no override) on a clean grammar: proceeds cleanly.
        #[test]
        fn capability_gate_enforce_admits_clean_grammar() {
            let clean = load(super::MINI_GRAMMAR_XML);
            let g = capability_gate(&clean, true, false);
            assert!(
                g.proceed,
                "an Admit-verdict grammar must proceed under enforcement"
            );
            assert!(!g.overridden);
        }

        /// `--enforce-capability` alone on a `Refuse` grammar: must block, with typed diagnostics naming the construct, and must not be marked `overridden`.
        #[test]
        fn capability_gate_enforce_refuses_permanently_refused_without_override() {
            let refused = load(PERMANENTLY_REFUSED_GRAMMAR_XML);
            let g = capability_gate(&refused, true, false);
            assert!(
                !g.proceed,
                "a Refuse verdict must block under --enforce-capability"
            );
            assert!(!g.overridden, "no override was requested");
            assert!(
                g.stderr_lines
                    .iter()
                    .any(|l| l.contains("reduplication.peel-eligible-rule-kind")),
                "expected a diagnostic naming the construct the backend declined on: {:?}",
                g.stderr_lines
            );
            assert!(
                g.stderr_lines
                    .iter()
                    .any(|l| l.contains(crate::GATED_BACKEND.label())),
                "a refusal must say WHICH backend declined, not just that something did: {:?}",
                g.stderr_lines
            );
        }

        #[cfg(not(feature = "developer-tools"))]
        #[test]
        fn production_refusal_diagnostics_do_not_advertise_developer_flags() {
            let refused = load(PERMANENTLY_REFUSED_GRAMMAR_XML);
            let gate = capability_gate(&refused, true, false);
            assert!(!gate.proceed);
            let rendered = gate.stderr_lines.join("\n");
            for flag in [
                "--allow-unproven",
                "--no-enforce-capability",
                "--remove-size-limits",
            ] {
                assert!(!rendered.contains(flag), "production diagnostic leaked {flag}: {rendered}");
            }
            let error = super::super::run_capability_gate(&refused, true, false)
                .err()
                .expect("a refused grammar must fail the production gate");
            for flag in [
                "--allow-unproven",
                "--no-enforce-capability",
                "--remove-size-limits",
            ] {
                assert!(!error.contains(flag), "production error leaked {flag}: {error}");
            }
        }

        /// A grammar the gated backend cannot compile at all must be refused AT THE GATE, naming that backend and the construct -- never let through to die inside the compiler on an internal budget message.
        #[test]
        fn a_grammar_only_the_gated_backend_refuses_is_still_blocked_at_the_gate() {
            let xml = crate::test_support::unordered_over_budget_grammar_xml(101);
            let over_budget = load(&xml);

            let selection = pg_foma::backend_selection::select_backends_for_grammar(&over_budget);
            assert!(
                !selection.selected().is_empty(),
                "this fixture is the interesting case only while ANOTHER backend still accepts it \
                 -- otherwise a whole-grammar join would have caught it too and the gate's \
                 per-backend reading would be untested: {selection:?}"
            );
            assert!(
                !selection
                    .report_for(crate::GATED_BACKEND)
                    .expect("the gated backend must be reported")
                    .is_selected(),
                "the gated backend must decline this fixture: {selection:?}"
            );

            let g = capability_gate(&over_budget, true, false);
            assert!(
                !g.proceed,
                "the gate must block a grammar the backend it is gating cannot compile: {:?}",
                g.stderr_lines
            );
            assert!(
                g.stderr_lines
                    .iter()
                    .any(|l| l.contains(crate::GATED_BACKEND.label())),
                "the refusal must name which backend declined: {:?}",
                g.stderr_lines
            );
            assert!(
                g.stderr_lines.iter().any(|l| l.contains("Unordered")),
                "the refusal must name the construct declined on: {:?}",
                g.stderr_lines
            );
        }

        /// `--enforce-capability --allow-unproven` on the same `Refuse` grammar: must proceed (the override), flagged `overridden`, with an unmissable `trust=unproven` marker plus the overridden diagnostics by name.
        #[cfg(feature = "developer-tools")]
        #[test]
        fn capability_gate_override_force_compiles_and_marks_trust_unproven() {
            let refused = load(PERMANENTLY_REFUSED_GRAMMAR_XML);
            let g = capability_gate(&refused, true, true);
            assert!(
                g.proceed,
                "--allow-unproven must force-compile a Refuse verdict"
            );
            assert!(g.overridden, "must be flagged as an overridden run");
            assert!(
                g.stderr_lines.iter().any(|l| l.contains("trust=unproven")),
                "expected an unmissable trust=unproven marker: {:?}",
                g.stderr_lines
            );
            assert!(
                g.stderr_lines
                    .iter()
                    .any(|l| l.contains("CAPABILITY-OVERRIDE")),
                "expected a CAPABILITY-OVERRIDE-labeled line: {:?}",
                g.stderr_lines
            );
            assert!(
                g.stderr_lines
                    .iter()
                    .any(|l| l.contains("reduplication.peel-eligible-rule-kind")),
                "the override record must still name which construct was force-compiled: {:?}",
                g.stderr_lines
            );
        }

        // --- End-to-end: `run_batch` itself, file-level behavior --------------------------------

        fn scratch_dir(tag: &str) -> std::path::PathBuf {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "pangloss-cli-test-capgate-{tag}-{}-{n}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).expect("create scratch dir");
            dir
        }

        /// Runs `batch <grammar> <words> <out.tsv> <extra_args...>`, returning `run_batch`'s own `Result` plus the `out.tsv` path, deliberately not read-and-unwrapped here since a refuse-without-override run must never create the file at all.
        fn run_batch_raw(
            tag: &str,
            grammar_xml: &str,
            extra_args: &[&str],
        ) -> (Result<(), String>, std::path::PathBuf) {
            let dir = scratch_dir(tag);
            let grammar_path = dir.join("grammar.xml");
            let words_path = dir.join("words.txt");
            let out_path = dir.join("out.tsv");
            fs::write(&grammar_path, grammar_xml).expect("write grammar");
            fs::write(&words_path, "a\n").expect("write words");

            let mut args: Vec<String> = vec![
                grammar_path.to_string_lossy().into_owned(),
                words_path.to_string_lossy().into_owned(),
                out_path.to_string_lossy().into_owned(),
            ];
            args.extend(extra_args.iter().map(|s| s.to_string()));

            (run_batch(&args), out_path)
        }

        /// The core flip: a `Refuse`-verdict grammar on `--engine=foma` with no capability flags must fail hard by default — `run_batch` returns `Err` and `out.tsv` must never have been created, since the gate sits before `FomaAnalyzer::new` in the control flow.
        #[test]
        fn run_batch_foma_engine_default_enforces_refuses_permanently_refused_with_no_flags() {
            let (result, out_path) = run_batch_raw(
                "foma-default-refuse",
                PERMANENTLY_REFUSED_GRAMMAR_XML,
                &["--engine=foma"],
            );
            assert!(
                result.is_err(),
                "--engine=foma must refuse a Refuse-verdict grammar BY DEFAULT, with no flags: \
                 {result:?}"
            );
            assert!(
                !out_path.exists(),
                "no analysis output may be produced for a refused, non-overridden run"
            );
        }

        /// `--engine=foma --no-enforce-capability` is the escape hatch back to advisory-only: the same `Refuse`-verdict grammar must now proceed unenforced.
        #[cfg(feature = "developer-tools")]
        #[test]
        fn run_batch_foma_engine_no_enforce_capability_proceeds_for_permanently_refused() {
            let (result, out_path) = run_batch_raw(
                "foma-no-enforce",
                PERMANENTLY_REFUSED_GRAMMAR_XML,
                &["--engine=foma", "--no-enforce-capability"],
            );
            assert!(
                result.is_ok(),
                "--no-enforce-capability must drop back to advisory-only on --engine=foma: \
                 {result:?}"
            );
            assert!(
                out_path.exists(),
                "an unenforced run must still produce output"
            );
        }

        /// Same `Refuse`-verdict grammar on `--engine=foma`, now with `--allow-unproven` alone (enforcement is already the foma-path default): `run_batch` must succeed and actually write `out.tsv`.
        #[cfg(feature = "developer-tools")]
        #[test]
        fn run_batch_foma_engine_allow_unproven_overrides_default_enforcement() {
            let (result, out_path) = run_batch_raw(
                "foma-override",
                PERMANENTLY_REFUSED_GRAMMAR_XML,
                &["--engine=foma", "--allow-unproven"],
            );
            assert!(
                result.is_ok(),
                "run_batch must proceed under --allow-unproven: {result:?}"
            );
            assert!(
                out_path.exists(),
                "the overridden run must still produce output"
            );
            let tsv = fs::read_to_string(&out_path).expect("read out.tsv");
            assert!(
                !tsv.trim().is_empty(),
                "out.tsv must contain at least one row"
            );
        }

        /// A clean (`Admit`-verdict) grammar on `--engine=foma`, no flags: ordinary success under default enforcement, same 5-column `ok` row shape as every other path.
        #[test]
        fn run_batch_foma_engine_admits_clean_grammar_normally() {
            let (result, out_path) =
                run_batch_raw("foma-clean", super::MINI_GRAMMAR_XML, &["--engine=foma"]);
            assert!(
                result.is_ok(),
                "a clean grammar must pass default enforcement: {result:?}"
            );
            let tsv = fs::read_to_string(&out_path).expect("read out.tsv");
            let last = tsv.lines().last().expect("at least one row");
            let fields: Vec<&str> = last.split('\t').collect();
            assert_eq!(fields.len(), 5, "{fields:?}");
            assert_eq!(fields[3], "ok", "{fields:?}");
        }

        /// The other half of the scoping rule: `--engine=default` must never enforce, not even with an explicit `--enforce-capability`, since it never relies on the FST proposer a `Refuse` verdict is about — the same grammar that hard-fails under `--engine=foma` must still succeed and write output here.
        #[test]
        fn run_batch_default_engine_never_enforces_even_with_explicit_flag() {
            let (result, out_path) = run_batch_raw(
                "default-flag-noop",
                PERMANENTLY_REFUSED_GRAMMAR_XML,
                &["--enforce-capability"],
            );
            assert!(
                result.is_ok(),
                "--engine=default must never enforce, even with --enforce-capability: {result:?}"
            );
            assert!(
                out_path.exists(),
                "--engine=default must still produce output"
            );
        }

        /// No flags at all, no `--engine` (so `--engine=default`): `run_batch` must still succeed and still write output.
        #[test]
        fn run_batch_no_flags_still_proceeds_for_permanently_refused_grammar() {
            let (result, out_path) = run_batch_raw(
                "no-flags-permanently-refused",
                PERMANENTLY_REFUSED_GRAMMAR_XML,
                &[],
            );
            assert!(
                result.is_ok(),
                "default (no-flag) behavior must be unchanged -- never blocks: {result:?}"
            );
            assert!(
                out_path.exists(),
                "default behavior must still produce output"
            );
        }
    }
}
