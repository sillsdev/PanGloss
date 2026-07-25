//! Throwaway diagnostic (2026-07-18): localize the 8.86GB allocation-failure crash observed
//! running `pangloss batch aweti.json aweti-words.txt out.tsv --engine=foma` after the
//! morphotactic-pruning fix landed. Zero-word batches complete cleanly in ~13-15min (compile-side
//! is fine); at least one word crashes identically. Builds `FomaAnalyzer` ONCE, then calls
//! `analyze_word` on every corpus word in sequence, printing+flushing before each call so the
//! last printed word is the culprit if the process aborts (Rust's alloc-error handler is not
//! catchable -- this is the only way to localize it without a rebuild per word).
use std::io::Write;
use std::path::PathBuf;

use pg_foma::composite::FomaAnalyzer;

fn main() {
    // Matches `pg-cli`'s own `main()` (1 GiB dedicated stack) -- the default thread stack
    // overflowed during the compile step alone (recursion in build_composites/emit), so this
    // must run on the same big-stack worker `pg-cli` and `pg_parse::batch::hc_parse_batch` use.
    std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(run)
        .expect("spawn worker thread")
        .join()
        .expect("worker thread panicked");
}

fn run() {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../samples/data/aweti.json"));
    let words_path = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../samples/data/aweti-words.txt"));

    eprintln!("loading grammar from {}", path.display());
    let json = std::fs::read_to_string(&path).expect("read grammar json");
    let snapshot = pg_snapshot::Snapshot::from_json(&json).expect("parse snapshot");
    let (g, _warnings) = pg_grammar::compile_project(&snapshot).expect("compile project");

    eprintln!("building FomaAnalyzer (compile step)...");
    let t0 = std::time::Instant::now();
    let mut analyzer = FomaAnalyzer::new(&g).expect("analyzer build");
    eprintln!("analyzer built in {:.1}s", t0.elapsed().as_secs_f64());

    let words_text = std::fs::read_to_string(&words_path).expect("read words");
    for (i, word) in words_text.lines().enumerate() {
        let word = word.trim();
        if word.is_empty() {
            continue;
        }
        eprint!("[{i}] trying {word:?} ... ");
        std::io::stderr().flush().ok();
        let t = std::time::Instant::now();
        let outcome = analyzer.analyze_word(word);
        eprintln!(
            "done in {:.3}s ({} analyses, {} candidates_generated)",
            t.elapsed().as_secs_f64(),
            outcome.analyses.len(),
            outcome.candidates_generated
        );
    }
    eprintln!("all words confirmed with no crash");
}
