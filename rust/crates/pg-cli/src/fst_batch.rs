//! Sequential FST batch output with the same resume and flush contract as HC.

use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::time::Instant;

use pg_stats::WorkCap;

use crate::engine::Engine;
use crate::{BatchRowFlags, BatchRowTail};

pub(crate) fn validate(
    args: &[String],
    work_cap: WorkCap,
    word_mem_cap: Option<usize>,
    capture_cap: Option<usize>,
    threads: usize,
) -> Result<(), String> {
    let mut forbidden = Vec::new();
    for arg in args {
        let name = arg.split('=').next().unwrap_or(arg);
        if matches!(
            name,
            "--stats"
                | "--stats-profile"
                | "--stats-compact"
                | "--cache-pack"
                | "--cache"
                | "--stats-manifest"
                | "--partials"
                | "--work-columns"
                | "--cost-column"
                | "--word-timeout-ms"
                | "--always-enforce-final-templates"
        ) {
            forbidden.push(name);
        }
    }
    if !forbidden.is_empty() {
        return Err(format!(
            "--engine=fst cannot honour {}",
            forbidden.join(", ")
        ));
    }
    if crate::per_word_diagnostics_requested() {
        return Err("--engine=fst cannot honour per-word HC profiling diagnostics".into());
    }
    if work_cap != WorkCap::Unbounded || word_mem_cap.is_some() || capture_cap.is_some() {
        return Err("--engine=fst cannot honour HC work/memory caps; explicitly pass --work-cap unbounded --word-mem-cap-mib 0 --capture-cap-mib unbounded; FST proposal limits still apply".into());
    }
    if threads != 1 {
        return Err("--engine=fst currently requires --threads 1".into());
    }
    Ok(())
}

pub(crate) struct Args<'a> {
    pub grammar_path: &'a str,
    pub words_path: &'a str,
    pub out_path: &'a str,
    pub analyses_path: Option<&'a str>,
    pub start_idx: usize,
    pub flush_each_word: bool,
    pub phase_marks: bool,
    pub guess_arg: Option<bool>,
}

pub(crate) fn run(args: Args<'_>) -> Result<(), String> {
    if args.start_idx > 0 && args.analyses_path.is_some() {
        return Err("--start cannot be combined with --analyses".into());
    }
    if let Some(path) = args.analyses_path {
        crate::reject_analysis_path_collisions(
            args.grammar_path,
            args.words_path,
            args.out_path,
            path,
            None,
        )?;
    }
    let started = Instant::now();
    let (grammar, warnings) = crate::load_grammar(args.grammar_path)?;
    crate::print_grammar_warnings(&warnings);
    if args.guess_arg.unwrap_or(grammar.guess_roots) {
        return Err(
            "--engine=fst cannot honour the project's GuessRoots setting; pass --guess=false"
                .into(),
        );
    }
    let grammar_load_ms = started.elapsed().as_secs_f64() * 1e3;
    let build_started = Instant::now();
    let mut session = crate::fst_engine::Session::compile(&grammar)?;
    let fst_build_ms = build_started.elapsed().as_secs_f64() * 1e3;
    eprintln!("LOADTIME\tengine=fst\tgrammar_load_ms={grammar_load_ms:.3}\tfst_build_ms={fst_build_ms:.3}\ttotal_ms={:.3}", grammar_load_ms + fst_build_ms);
    let words = fs::read_to_string(args.words_path)
        .map_err(|error| format!("read {}: {error}", args.words_path))?;
    let words: Vec<_> = words
        .lines()
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .collect();
    if args.start_idx > words.len() {
        return Err(format!(
            "--start {} exceeds word count {}",
            args.start_idx,
            words.len()
        ));
    }
    // Admission and argument errors leave an existing output intact.
    let file = if args.start_idx == 0 {
        fs::File::create(args.out_path)
    } else {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(args.out_path)
    }
    .map_err(|error| format!("open {}: {error}", args.out_path))?;
    let mut writer = BufWriter::new(file);
    let mut analyses_writer = args
        .analyses_path
        .map(|path| fs::File::create(path).map(BufWriter::new))
        .transpose()
        .map_err(|error| error.to_string())?;
    let flags = BatchRowFlags {
        engine: Some(Engine::Fst),
        partials: false,
        guess: false,
        work_columns: false,
        cost_column: false,
        trace_rejections: false,
    };
    let parse_started = Instant::now();
    if args.phase_marks {
        crate::phase_mark();
    }
    for (index, word) in words.iter().enumerate().skip(args.start_idx) {
        writeln!(writer, "{index}\t{word}\tSTARTED").map_err(|error| error.to_string())?;
        if args.flush_each_word {
            writer.flush().map_err(|error| error.to_string())?;
        }
        let started = Instant::now();
        let outcome = session.analyze(word);
        let elapsed_ms = started.elapsed().as_millis();
        crate::write_batch_row(
            &mut writer,
            index,
            word,
            elapsed_ms,
            crate::outcome_status(&outcome),
            &outcome.signature(),
            BatchRowTail::new(flags, &outcome),
        )
        .map_err(|error| error.to_string())?;
        if let Some(writer) = analyses_writer.as_mut() {
            crate::write_parse_analysis_row(
                writer,
                index,
                word,
                elapsed_ms,
                &outcome,
                &grammar,
                Some(Engine::Fst),
            )
            .map_err(|error| error.to_string())?;
            if args.flush_each_word {
                writer.flush().map_err(|error| error.to_string())?;
            }
        }
        if args.flush_each_word {
            writer.flush().map_err(|error| error.to_string())?;
        }
    }
    if args.phase_marks {
        crate::phase_mark();
    }
    writer.flush().map_err(|error| error.to_string())?;
    if let Some(writer) = analyses_writer.as_mut() {
        writer.flush().map_err(|error| error.to_string())?;
    }
    eprintln!(
        "PARSEELAPSED\tengine=fst\telapsed_ms={:.3}",
        parse_started.elapsed().as_secs_f64() * 1e3
    );
    Ok(())
}
