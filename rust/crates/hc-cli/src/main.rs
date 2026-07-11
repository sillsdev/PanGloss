//! `hc-rs` — the standalone CLI mirroring C# `hc batch`'s TSV protocol so parity diffs against
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
//! `hc_rules::stratum::StepBudget` the step cap already uses; whichever bound fires first wins,
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
//! `hc_parse::hc_parse_batch` and writes the buffered result in original order with no `STARTED`
//! lines, mirroring `RunParallel` exactly. This is a deliberate Rust-side choice (C# keys the
//! split on flag presence; we key it on thread count) — see the M7 commit/report for the full
//! rationale.
#![forbid(unsafe_code)]

use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use hc_grammar::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef};
use hc_parse::{hc_parse_batch, GenMorpheme, Morpher};

mod trace_render;

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
            println!("hc-rs {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("batch") => match run_batch(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("hc-rs batch: {e}");
                ExitCode::FAILURE
            }
        },
        Some("generate") => match run_generate(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("hc-rs generate: {e}");
                ExitCode::FAILURE
            }
        },
        Some("parse") => match run_parse(&args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("hc-rs parse: {e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!(
                "hc-rs {} — HermitCrab Rust engine CLI\n\
                 usage: hc-rs batch <grammar.xml> <words.txt> <out.tsv> [--step-cap N] [--word-timeout-ms N] [--memo=on|off] [--threads N] [--start N]\n\
                 usage: hc-rs generate <grammar.xml> <root-morpheme-id> [other-morpheme-id ...]\n\
                 usage: hc-rs parse <grammar.xml> <word> [--trace[=<file>]] [--trace-format=text|json]",
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::FAILURE
        }
    }
}

/// P12 chunk 7 (design doc §4.3): `hc-rs parse <grammar.xml> <word> [--trace[=<file>]]
/// [--trace-format=text|json]` -- today's CLI only has `batch`/`generate`, neither the right shape
/// for "trace exactly one word" (see the design doc's own rationale). `--trace` with no value
/// writes the tree to stdout; `--trace=<file>` writes it there instead, leaving stdout for just the
/// parse result. Default format: indented text; `--trace-format=json` emits the structured form.
fn run_parse(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut trace_dest: Option<Option<String>> = None; // None = --trace not given; Some(None) = stdout; Some(Some(path)) = file
    let mut trace_format = "text".to_string();

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--trace" => trace_dest = Some(None),
            s if s.starts_with("--trace=") => trace_dest = Some(Some(s["--trace=".len()..].to_string())),
            "--trace-format" => {
                let v = it.next().ok_or("--trace-format requires a value")?;
                trace_format = v.clone();
            }
            s if s.starts_with("--trace-format=") => {
                trace_format = s["--trace-format=".len()..].to_string();
            }
            s => positional.push(s),
        }
    }
    if trace_format != "text" && trace_format != "json" {
        return Err(format!("invalid --trace-format: {trace_format} (expected text|json)"));
    }
    let [grammar_path, word] = positional[..] else {
        return Err("usage: parse <grammar.xml> <word> [--trace[=<file>]] [--trace-format=text|json]".into());
    };

    let xml = fs::read_to_string(grammar_path).map_err(|e| format!("read {grammar_path}: {e}"))?;
    let grammar = hc_grammar::load(&xml).map_err(|e| format!("load {grammar_path}: {e:?}"))?;
    let morpher = Morpher::new(&grammar, usize::MAX);

    if let Some(dest) = trace_dest {
        let sink = hc_rules::trace::TreeTraceSink::new();
        let outcome = morpher.parse_word_traced(word, &hc_parse::ParseOptions::default(), &sink);
        println!("{}\t{}", word, outcome.signature());

        let rendered = match sink.root() {
            Some(root) if trace_format == "json" => trace_render::render_json(&grammar, &sink, root),
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
    }
    Ok(())
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
    let mut threads: usize = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    // 0-based resume index (C# `batch --start=N` equivalent, BatchCommand.cs): skip the first N
    // words (already-completed rows from a prior crashed/killed run) and append rather than
    // truncate `out.tsv`, so a watchdog wrapper can kill+relaunch a stalled word and continue
    // where it left off (plan §8 layer 3's nightly full-Sena recipe).
    let mut start_idx: usize = 0;
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
                word_timeout_ms = Some(v.parse().map_err(|_| format!("invalid --word-timeout-ms: {v}"))?);
            }
            s if s.starts_with("--word-timeout-ms=") => {
                let v = &s["--word-timeout-ms=".len()..];
                word_timeout_ms = Some(v.parse().map_err(|_| format!("invalid --word-timeout-ms: {v}"))?);
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
            s => positional.push(s),
        }
    }
    if threads == 0 {
        return Err("--threads must be >= 1".into());
    }
    let [grammar_path, words_path, out_path] = positional.as_slice() else {
        return Err(
            "usage: batch <grammar.xml> <words.txt> <out.tsv> [--step-cap N] [--word-timeout-ms N] [--memo=on|off] [--threads N] [--start N]"
                .into(),
        );
    };

    let xml = fs::read_to_string(grammar_path).map_err(|e| format!("read {grammar_path}: {e}"))?;
    let grammar = hc_grammar::load(&xml).map_err(|e| format!("load {grammar_path}: {e:?}"))?;
    let morpher = Morpher::new(&grammar, step_cap)
        .with_memo(memo)
        .with_word_timeout(word_timeout_ms.map(Duration::from_millis));

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
            // permanent diagnostic: dumps hc_fst::traverse's `Transduce::run`/`distinct()` and
            // hc_rules::morph's `push_remove_duplicates` timing/size stats accumulated over this
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
                ) = hc_fst::profile::snapshot();
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
                    hc_rules::morph::dedup_profile::snapshot();
                eprintln!(
                    "DEDUPPROF\t{i}\t{word}\tcalls={dedup_calls}\tms={:.3}\tmax_out_len={dedup_max_out_len}\ttotal_out_len={dedup_total_out_len}",
                    dedup_ns as f64 / 1e6,
                );
            }
            writeln!(w, "{i}\t{word}\t{elapsed_ms}\t{status}\t{signature}").map_err(|e| e.to_string())?;
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
            writeln!(w, "{i}\t{word}\t{elapsed_ms}\t{status}\t{signature}").map_err(|e| e.to_string())?;
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
        return Err("usage: generate <grammar.xml> <root-morpheme-id> [other-morpheme-id ...]".into());
    };

    let xml = fs::read_to_string(grammar_path).map_err(|e| format!("read {grammar_path}: {e}"))?;
    let grammar = hc_grammar::load(&xml).map_err(|e| format!("load {grammar_path}: {e:?}"))?;
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

    let words = morpher.generate_words(root, &others, hc_featstruct::FeatureStruct::EMPTY);
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
    //! `hc-parse/tests/word_timeout_gate.rs` already covers the engine-level behavior) so a bug in
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
        let dir = std::env::temp_dir().join(format!("hc-rs-cli-test-{tag}-{}-{n}", std::process::id()));
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
        fields[2].parse::<u128>().expect("ms column must be an integer");
        assert_eq!(fields[3], "TIMEOUT");
        assert_eq!(fields[4], "-");
    }

    /// Parallel path (`--threads 2`): same deadline, same row shape, but no `STARTED` line at all
    /// (the parallel writer never emits one — see the module doc's "two TSV-writing modes").
    #[test]
    fn word_timeout_ms_zero_writes_timeout_row_parallel() {
        let lines = run_batch_tsv("par-timeout", &["--word-timeout-ms", "0", "--threads", "2"]);
        assert_eq!(lines.len(), 1, "no STARTED line in the parallel writer: {lines:?}");
        let fields: Vec<&str> = lines[0].split('\t').collect();
        assert_eq!(fields.len(), 5);
        assert_eq!(fields[0], "0");
        assert_eq!(fields[1], "kat");
        fields[2].parse::<u128>().expect("ms column must be an integer");
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
            assert_ne!(fields[4], "-", "threads={threads}: \"kat\" should analyze to a real signature");
        }
    }
}
