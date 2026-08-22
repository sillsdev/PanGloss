//! Diagnostic: calls `pg_foma::emit::emit` directly without compiling, to separate emission cost from the foma lexc compile itself.
use std::path::PathBuf;

fn main() {
    let worker = std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(run)
        .expect("spawn worker thread");
    if let Some(seconds) = std::env::var("PANGLOSS_EMIT_DIAGNOSTIC_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        let started = std::time::Instant::now();
        while !worker.is_finished() {
            if started.elapsed() >= std::time::Duration::from_secs(seconds) {
                eprintln!("emit-only diagnostic stopped after {seconds}s");
                std::process::exit(124);
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    worker.join().expect("worker thread panicked");
}

fn run() {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../samples/data/aweti.json"));

    eprintln!("loading grammar from {}", path.display());
    let source = std::fs::read_to_string(&path).expect("read grammar");
    let g = if path.extension().is_some_and(|extension| extension == "xml") {
        pg_grammar::load(&source).expect("parse HC XML")
    } else {
        let snapshot = pg_snapshot::Snapshot::from_json(&source).expect("parse snapshot");
        pg_grammar::compile_project(&snapshot)
            .expect("compile project")
            .0
    };

    eprintln!("calling emit()...");
    let t0 = std::time::Instant::now();
    let result = pg_foma::emit::emit(&g);
    let elapsed = t0.elapsed();
    eprintln!("emit() done in {:.1}s", elapsed.as_secs_f64());
    eprintln!("lexc_source bytes: {}", result.lexc_source.len());
    eprintln!("report: {:?}", result.report);
}
