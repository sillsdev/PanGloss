//! Diagnostic: hard-codes the templated backend (`compile_templated_morphotactics`) for one grammar and one word list, printing compile-stage wall times and per-word analyze timings, so the templated-vs-tuned routing decision can be made from measured numbers on a grammar the eager composite pipeline compiles slowly.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use pg_foma::composite::FomaAnalyzer;
use pg_foma::templated_compile::compile_templated_morphotactics;
use pg_grammar::model::Grammar;

fn main() {
    // Matches `pg-cli`'s 1 GiB dedicated stack: compile recursion overflows the default stack on large grammars.
    std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(run)
        .expect("spawn worker thread")
        .join()
        .expect("worker thread panicked");
}

/// Dispatches on extension like `pg-cli::load_grammar`: `.fwdata` imports in-memory, `.json` parses a snapshot, anything else loads legacy HC XML.
fn load_grammar(path: &Path) -> Grammar {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("fwdata") {
        let (snapshot, _report) = pg_fwdata::import_file(path)
            .unwrap_or_else(|e| panic!("import {}: {e}", path.display()));
        let (g, warnings) = pg_grammar::compile_project(&snapshot)
            .unwrap_or_else(|e| panic!("compile_project {}: {e}", path.display()));
        eprintln!("({} compile_project warnings)", warnings.len());
        g
    } else if ext.eq_ignore_ascii_case("json") {
        let json = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let snapshot = pg_snapshot::Snapshot::from_json(&json)
            .unwrap_or_else(|e| panic!("parse snapshot {}: {e}", path.display()));
        let (g, _warnings) = pg_grammar::compile_project(&snapshot)
            .unwrap_or_else(|e| panic!("compile_project {}: {e}", path.display()));
        g
    } else {
        let xml = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        pg_grammar::load(&xml).unwrap_or_else(|e| panic!("load {}: {e}", path.display()))
    }
}

fn run() {
    let grammar_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: templated_backend_probe <grammar> <words.txt> [out.tsv]");
    let words_path = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .expect("usage: templated_backend_probe <grammar> <words.txt> [out.tsv]");
    let out_path = std::env::args().nth(3).map(PathBuf::from);

    let t_load = Instant::now();
    let g = load_grammar(&grammar_path);
    eprintln!("grammar loaded in {:.1}s", t_load.elapsed().as_secs_f64());

    let t_compile = Instant::now();
    let output = match compile_templated_morphotactics(&g) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("TEMPLATED-COMPILE-FAILED\t{e}");
            std::process::exit(3);
        }
    };
    let compile_s = t_compile.elapsed().as_secs_f64();
    let p = &output.profile;
    eprintln!(
        "TEMPLATED-COMPILE-OK\ttotal_s={compile_s:.1}\temit_s={:.1}\tlexc_s={:.1}\trules_s={:.1}\tcleanup_s={:.1}\tcompose_min_s={:.1}\tapply_prep_s={:.1}\tskipped_rules={}",
        p.templated_emit_elapsed.as_secs_f64(),
        p.lexc_compile_elapsed.as_secs_f64(),
        p.rule_compile_compose_elapsed.as_secs_f64(),
        p.cleanup_compile_elapsed.as_secs_f64(),
        p.final_compose_minimize_elapsed.as_secs_f64(),
        p.apply_prepare_elapsed.as_secs_f64(),
        p.skipped_rules.len(),
    );
    let (states, arcs) = output.proposer.network_counts();
    eprintln!(
        "network: states={states} arcs={arcs} (lexc pre-compose: states={} arcs={})",
        p.lexc_state_count, p.lexc_arc_count
    );
    for r in &p.skipped_rules {
        eprintln!("  skipped rule: {r}");
    }

    let mut analyzer = FomaAnalyzer::from_precompiled_proposer(&g, output.proposer);

    let words_text = std::fs::read_to_string(&words_path).expect("read words");
    let mut out: Box<dyn Write> = match &out_path {
        Some(p) => Box::new(std::fs::File::create(p).expect("create out.tsv")),
        None => Box::new(std::io::stdout()),
    };
    let mut n = 0u32;
    let mut with_analyses = 0u32;
    let t_all = Instant::now();
    for (i, word) in words_text.lines().enumerate() {
        let word = word.trim();
        if word.is_empty() {
            continue;
        }
        let t = Instant::now();
        let outcome = analyzer.analyze_word(word);
        let ms = t.elapsed().as_millis();
        n += 1;
        if !outcome.analyses.is_empty() {
            with_analyses += 1;
        }
        writeln!(
            out,
            "{i}\t{word}\t{ms}\tok\t{}",
            pg_parse::result_signature(&outcome.analyses)
        )
        .expect("write row");
    }
    eprintln!(
        "TEMPLATED-ANALYZE-DONE\twords={n}\twith_analyses={with_analyses}\ttotal_s={:.1}",
        t_all.elapsed().as_secs_f64()
    );
}
