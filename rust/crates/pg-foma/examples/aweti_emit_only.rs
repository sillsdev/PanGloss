//! Throwaway diagnostic (2026-07-18): how big is Aweti's emitted lexc, and how much of
//! `FomaAnalyzer::new`'s ~13min wall time is `emit()` (Rust-side composite generation) vs the
//! foma lexc compile itself? Calls `pg_foma::emit::emit` directly, WITHOUT compiling it -- much
//! cheaper than a full `FomaProposer::new`/`FomaAnalyzer::new` run, to separate the two phases.
use std::path::PathBuf;

fn main() {
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

    eprintln!("loading grammar from {}", path.display());
    let json = std::fs::read_to_string(&path).expect("read grammar json");
    let snapshot = pg_snapshot::Snapshot::from_json(&json).expect("parse snapshot");
    let (g, _warnings) = pg_grammar::compile_project(&snapshot).expect("compile project");

    eprintln!("calling emit()...");
    let t0 = std::time::Instant::now();
    let result = pg_foma::emit::emit(&g);
    let elapsed = t0.elapsed();
    eprintln!("emit() done in {:.1}s", elapsed.as_secs_f64());
    eprintln!("lexc_source bytes: {}", result.lexc_source.len());
    eprintln!("report: {:?}", result.report);
}
