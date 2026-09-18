//! Diagnostic instrumentation, not a correctness gate: calls `Morpher::parse_word` on the same pathological word several times in one process to test for lazy/deferred per-parse compilation cost (cold vs warm). The memo-on/memo-off arm this probe also carried went away with the memo itself (`docs/divergences/045-memoization-removed.md`).

use std::path::PathBuf;
use std::time::Instant;

use pg_parse::Morpher;

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

#[test]
#[ignore = "diagnostic instrumentation, not a correctness gate; also needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn sena_cold_vs_warm() {
    let Some(gpath) = sample_path("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let xml = std::fs::read_to_string(&gpath).expect("read grammar");

    let load_start = Instant::now();
    let g = pg_grammar::load(&xml).expect("load sena grammar");
    eprintln!(
        "grammar load: {:.1}ms",
        load_start.elapsed().as_secs_f64() * 1000.0
    );

    // Real pathological words, not synthetic, pulled from a plain-engine batch run's observed latency distribution.
    let words = [
        "kakukhondani",
        "pisabulukira",
        "kumwenikira",
        "ungandigodamira",
        "anyakuidiwa",
    ];

    // A generous but finite deadline, so a pathological word cannot hang the probe.
    let morpher =
        Morpher::new(&g, usize::MAX).with_word_timeout(Some(std::time::Duration::from_secs(20)));

    for word in words {
        eprintln!("\n=== word {word:?} ===");
        eprintln!("  5 repeated calls (same Morpher, same RuleCache):");
        for i in 0..5 {
            let start = Instant::now();
            let outcome = morpher.parse_word(word);
            let elapsed = start.elapsed();
            eprintln!(
                "    call {i}: {:.2}ms (steps={}, capped={}, timed_out={}, n_analyses={})",
                elapsed.as_secs_f64() * 1000.0,
                outcome.steps,
                outcome.capped,
                outcome.timed_out,
                outcome.analyses.len()
            );
        }
    }
}
