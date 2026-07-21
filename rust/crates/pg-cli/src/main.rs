//! `pangloss` — the standalone CLI mirroring C# `hc batch`'s TSV protocol so parity diffs against
//! managed golden runs are line-for-line comparable (plan §8 layer 3).
//!
//! `batch <grammar.xml> <words.txt> <out.tsv> [--step-cap N] [--word-timeout-ms N] [--threads N]`
//! loads the grammar once and parses every word, writing the BatchCommand-compatible TSV
//! (`BatchCommand.cs`). `--step-cap N` bounds the unmemoized analysis cascade (M6 memoization
//! removes the need).
//!
//! ## `--word-timeout-ms` (`docs/budget-model.md`'s addendum)
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
//! ## `--threads` and the two TSV-writing modes (plan §7, M7)
//! C#'s own `BatchCommand` has two mutually exclusive dispatch modes with genuinely different TSV
//! behavior, confirmed by reading `BatchCommand.cs`:
//! - `RunSequential` (no `--parallel` flag): one word at a time, each line written and flushed
//!   immediately, preceded by a `{idx}\t{word}\tSTARTED` sentinel — crash-resumable (the recipe
//!   plan §8 layer 3 calls for on the nightly full-Sena run, which historically crashed a host).
//! - `RunParallel` (`--parallel[:N]`, `Parallel.ForEach`): results are buffered into an
//!   index-ordered array (`rows[i] = ...`) and the whole file is written **once, sequentially, in
//!   original order** only after every word has finished. No `STARTED` line is ever written in
//!   this mode — there is nothing to resume mid-run.
//!
//! `--threads N` here maps onto that split by **value**, not by flag presence: `--threads 1`
//! (the default-shaped case) keeps the exact legacy per-line/`STARTED`/flush loop — preserving
//! crash-resumability for the one workload (nightly full Sena) that plan §8 documents as still
//! needing it, pending the M10 budgets port. `--threads N` for `N > 1` routes through
//! `pg_parse::hc_parse_batch` and writes the buffered result in original order with no `STARTED`
//! lines, mirroring `RunParallel` exactly. This is a deliberate Rust-side choice (C# keys the
//! split on flag presence; we key it on thread count) — see the M7 commit/report for the full
//! rationale.
//!
//! ## `import` and `.json`/`.fwdata` grammar dispatch (`docs/fwdata-import-plan.md` T4)
//! `import <project.fwdata> <out.json>` runs `pg_fwdata::import_file` and writes the resulting
//! `pg_snapshot::Snapshot::to_json()` to `<out.json>`. `ImportReport` warnings (dangling refs,
//! unsupported constructs, log-and-skip decisions) and `Snapshot::validate()` warnings (dangling
//! GUID cross-references *within* the snapshot) are printed to stderr, clearly labeled and kept
//! separate since they come from different stages of the pipeline; exit is non-zero only on a
//! hard `pg_fwdata::ImportError` (I/O failure / not-a-`.fwdata`-file), never on either warning list.
//!
//! Every other subcommand that takes a grammar path (`parse`, `batch`, `generate`)
//! now dispatches on the path's extension via [`load_grammar`]: `.xml` (or anything else) is the
//! legacy HC-XML path (`pg_grammar::load`, unchanged, no warnings); `.json` loads a `pg-snapshot`
//! `Snapshot` (`Snapshot::from_json`) and compiles it (`pg_grammar::compile_project`); `.fwdata`
//! imports the FieldWorks project file directly, in-memory, then compiles it -- no intermediate
//! JSON file is written (run the `import` subcommand first if you want to keep the snapshot
//! around, e.g. to inspect it or reuse it without re-importing every time). Compile/import
//! warnings from `.json`/`.fwdata` dispatch are always printed to stderr, never stdout --
//! `batch`'s TSV rows are parity-sensitive against C# goldens, so warnings must never be
//! interleaved into that output stream.
#![forbid(unsafe_code)]

use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use pg_foma::composite::FomaAnalyzer;
use pg_grammar::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef};
use pg_parse::{hc_parse_batch, GenMorpheme, Morpher, WordAnalysis};

mod trace_render;

/// P3 (docs/fst-plan/foma-fst-plan.md, gate F3): which proposer/verifier path a `batch`/`parse`
/// invocation drives. `Default` is the pre-existing `pg_parse::Morpher` full-search engine
/// (unchanged behavior, still the default when `--engine` is omitted). `Foma` routes through
/// `pg_foma::composite::FomaAnalyzer` (propose via the compiled foma network, confirm via the
/// same `Morpher` machinery pinned to each candidate) — see that crate's module docs for the
/// propose→confirm contract. Output SHAPE (the 5-column batch TSV; the `parse` subcommand's
/// `word\tsignature` line plus `--gloss`/`--natural-gloss` lines) is identical between engines by
/// construction: both paths end in the same `pg_parse::result_signature`/`pg_realize` calls.
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

/// Mirrors `Morpher::parse_word_core_selected`'s own shape-validity early return
/// (`pg-parse/src/morpher.rs`: `segment_with_features` on the surface stratum's char-def table) —
/// `pg_foma::composite::FomaAnalyzer` has no equivalent check of its own (a word the proposer can't
/// segment at all just yields zero candidates), so the `--engine=foma` batch/parse paths call this
/// directly to keep the `ok`-vs-`SKIPPED` status column identical to the default engine's, per the
/// conformance protocol's own rule ("a `status` mismatch is always a failure",
/// `machine/conformance/PROTOCOL.md` section 4). An empty-grammar (`g.strata.is_empty()`) is never
/// `invalid_shape` in the default engine either (`morpher.rs`'s `n == 0` early return), so this
/// mirrors that too.
fn foma_invalid_shape(g: &Grammar, word: &str) -> bool {
    let Some(last) = g.strata.last() else {
        return false;
    };
    let surface_table = &g.char_tables[last.table.0 as usize];
    pg_grammar::segment::segment(surface_table, word).is_err()
}

fn main() -> ExitCode {
    // The analysis cascade recurses to the depth of a word's unapplication chain, which on the heavy
    // corpus words exceeds the default 8 MiB main-thread stack (the managed engine runs on a large
    // stack too). Run the whole batch on a worker thread with a generous stack — a runtime detail, no
    // semantic effect on results.
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
        _ => {
            eprintln!(
                "pangloss {} — HermitCrab Rust engine CLI\n\
                 usage: pangloss batch <grammar> <words.txt> <out.tsv> [--step-cap N] [--word-timeout-ms N] [--memo=on|off] [--threads N] [--start N] [--engine=default|foma]\n\
                 usage: pangloss generate <grammar> <root-morpheme-id> [other-morpheme-id ...]\n\
                 usage: pangloss parse <grammar> <word> [--trace[=<file>]] [--trace-format=text|json] [--gloss] [--natural-gloss=eng] [--realize-map=<path>] [--engine=default|foma]\n\
                 usage: pangloss import <project.fwdata> <out.json>\n\
                 \n\
                 <grammar> is one of: a HermitCrab XML export (.xml, the legacy path), a\n\
                 pg-snapshot JSON file (.json, from `pangloss import` or any other producer), or a\n\
                 FieldWorks project file (.fwdata, imported in-memory and compiled on the fly).",
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::FAILURE
        }
    }
}

/// `import <project.fwdata> <out.json>` (`docs/fwdata-import-plan.md` T4): run `pg-fwdata` over a
/// FieldWorks project file and write the resulting `pg_snapshot::Snapshot::to_json()` to
/// `<out.json>`. Prints `ImportReport` warnings and `Snapshot::validate()` warnings to stderr,
/// each under its own labeled heading (they come from different stages -- extraction vs.
/// cross-reference validation -- and conflating them would make root-causing a warning harder).
/// Non-zero exit only on a hard `pg_fwdata::ImportError`; neither warning list ever fails the
/// command (`docs/fwdata-import-plan.md` §1: this pipeline must tolerate stale/dangling real-world
/// project data, never crash on it).
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
fn load_grammar(path: &str) -> Result<(Grammar, Vec<String>), String> {
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
            let mut warnings = report.warnings;
            warnings.extend(snapshot.validate());
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
fn print_grammar_warnings(warnings: &[String]) {
    for w in warnings {
        eprintln!("warning: {w}");
    }
}

/// P12 chunk 7 (design doc §4.3): `pangloss parse <grammar.xml> <word> [--trace[=<file>]]
/// [--trace-format=text|json]` -- today's CLI only has `batch`/`generate`, neither the right shape
/// for "trace exactly one word" (see the design doc's own rationale). `--trace` with no value
/// writes the tree to stdout; `--trace=<file>` writes it there instead, leaving stdout for just the
/// parse result. Default format: indented text; `--trace-format=json` emits the structured form.
///
/// `--gloss` (`docs/natural-phrases-plan.md` N0): after the existing, UNCHANGED
/// `word\tsignature` line, print one additional `gloss:\t{leipzig}` line per surviving analysis
/// (same index order as `ParseOutcome.analyses`/`.structured`), via the new additive `pg-realize`
/// crate. Purely additive — orthogonal to `--trace`, works with or without it, and never touches
/// the parity line above it.
///
/// `--natural-gloss=eng` (`docs/natural-phrases-plan.md` N2): after the parity line (and after
/// the `gloss:` line for that same analysis, if `--gloss` was also given), print one additional
/// `eng:\t{text}` line per surviving analysis -- `eng:\t{text} ({residue})` when the realization
/// is only partial (`pg_realize::Realization::complete == false`). `eng` is the only supported
/// value today (a hard error lists what's supported, rather than silently no-op-ing on a typo);
/// the flag name leaves room for other target languages later without a breaking change.
/// `--natural-gloss` implies building the gloss bundle -> IR -> realization chain internally, but
/// deliberately does NOT imply `--gloss`'s own `gloss:` line -- the two flags are independent.
///
/// `--realize-map=<path>` overrides the sidecar `pg_realize::RealizeMap` used to build each
/// analysis's `GlossIr`; omitted, it defaults to `<grammar-dir>/<grammar-stem-with-"-hc"-suffix-
/// stripped>-realize.toml` (e.g. `samples/data/amharic-hc.xml` -> `samples/data/amharic-
/// realize.toml`), matching `samples/data/*-realize.toml`'s naming convention
/// (`docs/natural-phrases-plan.md` N1). A missing default-resolved file degrades to
/// `RealizeMap::empty()` (the documented no-sidecar path, e.g. Sena); a missing *explicitly*
/// named `--realize-map` file, or any sidecar that fails to parse (default-resolved or explicit),
/// is a hard error -- typos must not silently degrade.
fn run_parse(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut trace_dest: Option<Option<String>> = None; // None = --trace not given; Some(None) = stdout; Some(Some(path)) = file
    let mut trace_format = "text".to_string();
    let mut gloss = false;
    let mut natural_gloss: Option<String> = None;
    let mut realize_map_arg: Option<String> = None;
    let mut engine = Engine::Default;

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
            s => positional.push(s),
        }
    }
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
    if let Some(v) = &natural_gloss {
        if v != "eng" {
            return Err(format!(
                "unsupported --natural-gloss value: {v} (supported: eng)"
            ));
        }
    }
    let [grammar_path, word] = positional[..] else {
        return Err("usage: parse <grammar> <word> [--trace[=<file>]] [--trace-format=text|json] [--gloss] [--natural-gloss=eng] [--realize-map=<path>] [--engine=default|foma]".into());
    };

    let (grammar, warnings) = load_grammar(grammar_path)?;
    print_grammar_warnings(&warnings);

    // `--natural-gloss=eng` setup: the embedded English table + the per-grammar sidecar map, both
    // built once up front (not per-analysis) since neither depends on the word being parsed.
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
        // P3 (docs/fst-plan/foma-fst-plan.md): `--trace` was already rejected above, so this is
        // always the "minimal single-word batch" shape below, just routed through
        // `FomaAnalyzer::analyze_word` instead of `Morpher::parse_word`. Output shape (the
        // `word\tsignature` line plus `--gloss`/`--natural-gloss` lines) is identical to the
        // default engine's — both end in the same `pg_parse::result_signature`/`pg_realize` calls.
        let mut analyzer = FomaAnalyzer::new(&grammar)
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

    if let Some(dest) = trace_dest {
        let sink = pg_rules::trace::TreeTraceSink::new();
        let outcome = morpher.parse_word_traced(word, &pg_parse::ParseOptions::default(), &sink);
        println!("{}\t{}", word, outcome.signature());
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
        let outcome = morpher.parse_word(word);
        println!("{}\t{}", word, outcome.signature());
        print_realize_lines(&grammar, &outcome.structured, word, gloss, natural.as_ref());
    }
    Ok(())
}

/// `--gloss`/`--natural-gloss=eng`'s per-analysis output (N0/N2): for each `outcome.structured[i]`
/// (same index order as `ParseOutcome.analyses`), optionally a `gloss:\t{leipzig}` line, then
/// optionally an `eng:\t{text}` (or `eng:\t{text} ({residue})` when residue is non-empty; an
/// incomplete realization with empty residue prints bare -- a guessed root's `*word*` notation
/// already signals incompleteness, so a trailing `()` would be noise) line -- interleaved
/// per analysis, not as two separate passes, so a reader can tell which `eng:` line goes with
/// which `gloss:` line when a word has more than one surviving analysis. Deliberately reading
/// `.structured`, not `.analyses`, since `pg_realize::gloss_bundle` needs the numeric morpheme
/// ordinals, not the display-string join `.analyses` carries (see `ParseOutcome`'s own doc on why
/// the two views share an index but not a shape).
/// Shared by both engines' `parse` output (P3, docs/fst-plan/foma-fst-plan.md): takes the same
/// `.structured` slice either `pg_parse::ParseOutcome` or `pg_foma::composite::FomaOutcome` exposes
/// (parallel by index to `.analyses` in both), so the `--gloss`/`--natural-gloss=eng` lines are
/// byte-for-byte identical in shape regardless of which engine produced them.
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

/// Resolve and load the `--natural-gloss=eng` sidecar [`pg_realize::RealizeMap`]: `explicit_arg`
/// (`--realize-map=<path>`) wins when given, else the default path is derived from
/// `grammar_path`'s directory and stem (`docs/natural-phrases-plan.md` N2: strip a trailing
/// `-hc` from the stem, append `-realize.toml`). An explicit path that doesn't exist, or any
/// resolved path that exists but fails to parse, is a hard error; a *default*-resolved path that
/// doesn't exist degrades to `RealizeMap::empty()` (the documented no-sidecar path).
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

/// `<grammar-dir>/<grammar-stem-with-"-hc"-suffix-stripped>-realize.toml`, e.g.
/// `samples/data/amharic-hc.xml` -> `samples/data/amharic-realize.toml`.
fn default_realize_map_path(grammar_path: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(grammar_path);
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let stem = stem.strip_suffix("-hc").unwrap_or(stem);
    dir.join(format!("{stem}-realize.toml"))
}

fn run_batch(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut step_cap: usize = usize::MAX;
    // `--word-timeout-ms` (docs/budget-model.md's addendum): an optional wall-clock deadline per
    // word, independent of `--step-cap`. `None` (the flag omitted) is the default and a complete
    // no-op — see `Morpher::with_word_timeout`.
    let mut word_timeout_ms: Option<u64> = None;
    let mut memo = true;
    // Default: number of logical CPUs (typical rayon default) — matches plan §7's "parallel by
    // default, override for the 1/2/4/8/16 benchmark sweep" M7 requirement.
    let mut threads: usize = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    // 0-based resume index (C# `batch --start=N` equivalent, BatchCommand.cs): skip the first N
    // words (already-completed rows from a prior crashed/killed run) and append rather than
    // truncate `out.tsv`, so a watchdog wrapper can kill+relaunch a stalled word and continue
    // where it left off (plan §8 layer 3's nightly full-Sena recipe).
    let mut start_idx: usize = 0;
    // P3 (docs/fst-plan/foma-fst-plan.md): `--engine=foma` routes the whole batch through
    // `pg_foma::composite::FomaAnalyzer` instead of `pg_parse::Morpher`. Default unchanged.
    let mut engine = Engine::Default;
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
            s => positional.push(s),
        }
    }
    if threads == 0 {
        return Err("--threads must be >= 1".into());
    }
    let [grammar_path, words_path, out_path] = positional.as_slice() else {
        return Err(
            "usage: batch <grammar> <words.txt> <out.tsv> [--step-cap N] [--word-timeout-ms N] [--memo=on|off] [--threads N] [--start N] [--engine=default|foma]"
                .into(),
        );
    };

    // P3 3c (docs/fst-plan/foma-fst-plan.md): one-time cost instrumentation -- "grammar load with
    // emit+foma-compile vs load today (native)". `LOADTIME` always prints (cheap, one line per
    // invocation, same convention as the existing `HC_STEP_STATS`/`HC_FST_PROFILE` diagnostics
    // below, just unconditional since a single line costs nothing to always emit).
    let t_load = Instant::now();
    let (grammar, warnings) = load_grammar(grammar_path)?;
    print_grammar_warnings(&warnings);
    let grammar_load_ms = t_load.elapsed().as_secs_f64() * 1e3;

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
        // P3 (docs/fst-plan/foma-fst-plan.md): the foma path — one `FomaAnalyzer` built once
        // (the expensive emit+foma-compile step), reused across every word, exactly like
        // `pg_parse::Morpher` is built once above. `--step-cap`/`--memo` do not apply to this
        // path (the verifier `Morpher` inside `FomaAnalyzer` is always uncapped, per plan §2) and
        // are silently ignored, matching how the default engine silently ignores flags it doesn't
        // ship yet. `--word-timeout-ms` DOES apply here (wired via `FomaAnalyzer::with_word_timeout`,
        // threading straight to the same internal `Morpher::with_word_timeout` the default engine
        // below uses) — omitted (the default) stays a complete no-op, matching this flag's
        // contract everywhere else.
        //
        // `--threads`: `threads == 1` keeps the legacy per-word sequential writer (STARTED
        // sentinel + per-line flush, crash-resumable), routed through `FomaAnalyzer::analyze_word`
        // exactly as before. `threads > 1` (perf pass, 2026-07-16) routes through
        // `FomaAnalyzer::analyze_words` instead — CONFIRM (the dominant cost, per a tracer) runs
        // across a dedicated rayon pool inside that call; PROPOSE stays sequential either way
        // (see that method's own doc for why — the single foma `ApplyHandle` this analyzer owns
        // can't be split across threads without a lock or a redundant per-thread network clone).
        // Buffered + written once in original order, no `STARTED` lines — the same
        // `RunParallel`-equivalent shape the default engine's `threads > 1` path already uses
        // below, and for the same reason (no per-word crash-resume is possible in this mode).
        let t_compile = Instant::now();
        let mut analyzer = FomaAnalyzer::new(&grammar)
            .map_err(|e| format!("foma compile failed for {grammar_path}: {e}"))?
            .with_word_timeout(word_timeout_ms.map(Duration::from_millis));
        let compile_ms = t_compile.elapsed().as_secs_f64() * 1e3;
        eprintln!(
            "LOADTIME\tengine=foma\tgrammar_load_ms={grammar_load_ms:.3}\tanalyzer_build_ms={compile_ms:.3}\ttotal_ms={:.3}",
            grammar_load_ms + compile_ms
        );
        // P3 3c: candidate-count/confirm distributions -- `FomaOutcome` exposes exactly
        // `candidates_generated`/`confirmed`; gated behind an env var (like `HC_STEP_STATS`)
        // since it is one line per word, too much for a default-on diagnostic on a 7k-word corpus.
        let stats_on = std::env::var("HC_FOMA_STATS").is_ok();

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
            // Parallel path: skip `foma_invalid_shape` words up front (never handed to
            // `analyze_words` at all — matches the sequential branch's own SKIPPED handling,
            // just resolved before the batch call since `analyze_words` has no shape check of
            // its own, same as `analyze_word`).
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
        w.flush().map_err(|e| e.to_string())?;
        eprintln!(
            "batch complete: {} words parsed ({} skipped) [engine=foma, threads={}]",
            parsed, skipped, threads,
        );
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

    if threads == 1 {
        // Legacy sequential path (C# `RunSequential` equivalent): STARTED sentinel + per-line
        // flush, crash-resumable. Preserved byte-for-byte from the pre-M7 implementation.
        for (i, word) in words.iter().enumerate() {
            if i < start_idx {
                continue;
            }
            writeln!(w, "{i}\t{word}\tSTARTED").map_err(|e| e.to_string())?;
            // Flush the STARTED sentinel immediately, before starting this word's (potentially
            // very long) parse — otherwise it only reaches disk alongside the result line once
            // the word finishes, defeating its purpose as a live "currently in flight" signal for
            // an external watchdog (it still works as a crash marker for the *previous* completed
            // word on resume either way).
            w.flush().map_err(|e| e.to_string())?;
            let start = Instant::now();
            let outcome = morpher.parse_word(word);
            let elapsed_ms = start.elapsed().as_millis();
            let (status, signature) = if outcome.invalid_shape {
                skipped += 1;
                ("SKIPPED", "-".to_string())
            } else if outcome.timed_out {
                // `--word-timeout-ms` fired (independent of `--step-cap`) — reported as its own
                // TSV status/signature shape, matching the synthetic TIMEOUT row
                // `tools/run-sena-rust.ps1`'s watchdog already writes for an externally-killed
                // stall (`idx\tword\tms\tTIMEOUT\t-`), so downstream tooling that already
                // understands that shape needs no changes.
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
            // Diagnostic only (W8 step-4 investigation): raw StepBudget tick count for this word,
            // regardless of whether the cap fired -- compares against C# --rule-stats attempt counts.
            if std::env::var("HC_STEP_STATS").is_ok() {
                eprintln!("STEPS\t{i}\t{word}\t{}", outcome.steps);
            }
            // O2 profiling instrumentation (rust-optimizations-phase2.md O2), HC_STEP_STATS-style
            // permanent diagnostic: dumps pg_fst::traverse's `Transduce::run`/`distinct()` and
            // pg_rules::morph's `push_remove_duplicates` timing/size stats accumulated over this
            // word's whole parse -- see docs/o2-profile-findings.md for what this found.
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
            writeln!(w, "{i}\t{word}\t{elapsed_ms}\t{status}\t{signature}")
                .map_err(|e| e.to_string())?;
            w.flush().map_err(|e| e.to_string())?; // per-line flush (AutoFlush), crash/monitor resumable
        }
    } else {
        // Parallel path (C# `RunParallel` equivalent): hc_parse_batch parallelizes internally
        // (rayon, longest-surface-first dispatch, large worker stacks); results come back already
        // reindexed to original word order (plan §7). Buffer + write once, in order, no STARTED
        // lines — matching BatchCommand.cs's own `rows[i] = ...` then single ordered write-out.
        // `--start` here only skips work (no per-word crash-resume is possible in this mode, same
        // as C#'s RunParallel — see module doc); indices are offset back to the original numbering.
        let remaining = &words[start_idx..];
        let results = hc_parse_batch(&morpher, remaining, threads);
        for (j, (word, r)) in remaining.iter().zip(results.iter()).enumerate() {
            let i = start_idx + j;
            let elapsed_ms = r.elapsed.as_millis();
            let (status, signature) = if r.outcome.invalid_shape {
                skipped += 1;
                ("SKIPPED", "-".to_string())
            } else if r.outcome.timed_out {
                // Same `--word-timeout-ms` outcome as the sequential path above — the parallel
                // path buffers rather than flushing per-line, but the row shape itself is
                // identical in both thread modes.
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
            writeln!(w, "{i}\t{word}\t{elapsed_ms}\t{status}\t{signature}")
                .map_err(|e| e.to_string())?;
        }
    }
    w.flush().map_err(|e| e.to_string())?;

    eprintln!(
        "batch complete: {} words parsed ({} skipped), {} hit the step cap, {} timed out [memo={}, threads={}]",
        parsed,
        skipped,
        capped_words,
        timed_out_words,
        if memo { "on" } else { "off" },
        threads,
    );
    Ok(())
}

/// `generate <grammar.xml> <root-morpheme-id> [other-morpheme-id ...]` (W7, manual-testing
/// helper): a thin CLI wrapper over `Morpher::generate_words` (the direct API, C# `Morpher.
/// GenerateWords(LexEntry, IEnumerable<Morpheme>, FeatureStruct)`, Morpher.cs:169-237) — always
/// with an empty realizational FS (no wire-friendly way to type one on a command line). Each
/// `<morpheme-id>` is the `<MorphemeId>` XML text a `LexicalEntry`/`MorphologicalRule` declares
/// (NOT the grammar-tier ordinal `hc_generate_words`'/`WordAnalysis`'s `morpheme_ids` use — this
/// is a human-typable identifier, resolved by lookup), applied in exactly the order given (no
/// interleaving search — that's `generate_words_from_analysis`'s job, not exposed here since it
/// needs a `WordAnalysis`, which isn't naturally hand-typable either).
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

/// The [`LexEntryId`] of the `LexicalEntry` whose `<MorphemeId>` text is `id`.
fn lex_entry_by_morpheme_id(g: &Grammar, id: &str) -> Option<LexEntryId> {
    g.entries
        .iter()
        .position(|e| g.morphemes[e.morpheme.0 as usize].morph_id.as_deref() == Some(id))
        .map(|idx| LexEntryId(idx as u32))
}

/// A [`GenMorpheme`] for the "other morpheme" whose `<MorphemeId>` text is `id` — either
/// [`GenMorpheme::NonHead`] (a `LexicalEntry`, a compounding non-head) or [`GenMorpheme::Rule`] (an
/// `AffixProcessRule`/`RealizationalRule`; a `CompoundingRule` never has a `<MorphemeId>` of its
/// own — C#'s `Morpher._morphemes` never gathers one either, Morpher.cs:50-52).
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

    /// A minimal, self-contained grammar (no phonological/morphological rules at all -- root-only
    /// lookup is enough to exercise the batch pipeline) ported from
    /// `conformance/loader/n1-isactive/grammar.xml`'s lexicon shape: one stratum, one table, one
    /// `LexicalEntry` whose surface form is "kat".
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

    /// A fresh, collision-free scratch directory per test (tests in one binary run concurrently by
    /// default) — cleaned up best-effort on drop-equivalent (next run overwrites; CI temp dirs are
    /// ephemeral anyway), avoiding any dependency on files elsewhere in the repo.
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

    /// Run `batch` with `extra_args` appended after the 3 positional args, returning the written
    /// TSV's lines.
    fn run_batch_tsv(tag: &str, extra_args: &[&str]) -> Vec<String> {
        let dir = scratch_dir(tag);
        let grammar_path = dir.join("grammar.xml");
        let words_path = dir.join("words.txt");
        let out_path = dir.join("out.tsv");
        fs::write(&grammar_path, MINI_GRAMMAR_XML).expect("write grammar");
        fs::write(&words_path, "kat\n").expect("write words");

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

    /// Sequential path (`--threads 1`, the default writer mode): a `--word-timeout-ms=0` deadline
    /// must produce a `STARTED` sentinel (unchanged) followed by a `TIMEOUT`/`-` result row, in the
    /// exact `idx\tword\tms\tTIMEOUT\t-` shape `tools/run-sena-rust.ps1`'s watchdog already
    /// synthesizes for an externally-killed stall (see that script's `Get-ResumeIndex`).
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

    /// Parallel path (`--threads 2`): same deadline, same row shape, but no `STARTED` line at all
    /// (the parallel writer never emits one — see the module doc's "two TSV-writing modes").
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

    /// Control: omitting `--word-timeout-ms` must keep producing the pre-existing `ok` row shape,
    /// in both thread modes — the flag's mere presence in `run_batch`'s parser must not perturb the
    /// no-flag path.
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
}
