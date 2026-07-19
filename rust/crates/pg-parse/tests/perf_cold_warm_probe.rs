//! TEMPORARY INVESTIGATION INSTRUMENTATION (see reports/03-parse-latency-profile.md). Not a
//! correctness gate. Tests hypothesis (a) directly: is there any lazy/deferred per-parse
//! compilation cost, by calling `Morpher::parse_word` on the SAME real pathological Sena word
//! several times in a row within one process (same `Morpher`, i.e. `RuleCache` built once at
//! `Morpher::new` per `pg-parse/src/morpher.rs`) and comparing call 1 (cold) vs calls 2-5 (warm).
//! Also compares `--memo=on` vs `--memo=off` on the same words to size the M6 memo's own effect.
//!
//! CI note: `morpher_memo_on` below is built with NO wall-clock or step cap
//! (`Morpher::new(&g, usize::MAX)`), unlike `morpher_memo_off`'s deliberate 20s watchdog — on these
//! genuinely pathological words this leg has been observed to run past 5 minutes on this
//! development machine without completing. `.github/workflows/rust-ci.yml`'s
//! `--include-ignored` step therefore `--skip`s this test by name; it is diagnostic
//! instrumentation, not a correctness gate, so CI does not need to run it at all.

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
fn sena_cold_vs_warm_and_memo_effect() {
    let Some(gpath) = sample_path("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let xml = std::fs::read_to_string(&gpath).expect("read grammar");

    let load_start = Instant::now();
    let g = pg_grammar::load(&xml).expect("load sena grammar");
    eprintln!("grammar load: {:.1}ms", load_start.elapsed().as_secs_f64() * 1000.0);

    // Real pathological words pulled live from this investigation's plain-engine batch run
    // (top of the observed-so-far distribution on a partial Sena run), not synthetic.
    let words = [
        "kakukhondani",
        "pisabulukira",
        "kumwenikira",
        "ungandigodamira",
        "anyakuidiwa",
    ];

    let morpher_memo_on = Morpher::new(&g, usize::MAX);
    // memo=off is the fair unmemoized baseline (M6 ablation) -- on a word where the memo is doing
    // real work, this can be MUCH slower (the whole reason the memo exists), so it is guarded with
    // a generous but finite wall-clock deadline rather than risking a many-minutes hang here.
    let morpher_memo_off = Morpher::new(&g, usize::MAX)
        .with_memo(false)
        .with_word_timeout(Some(std::time::Duration::from_secs(20)));

    for word in words {
        eprintln!("\n=== word {word:?} ===");
        eprintln!("  --memo=on, 5 repeated calls (same Morpher, same RuleCache):");
        for i in 0..5 {
            let start = Instant::now();
            let outcome = morpher_memo_on.parse_word(word);
            let elapsed = start.elapsed();
            eprintln!(
                "    call {i}: {:.2}ms (steps={}, capped={}, n_analyses={})",
                elapsed.as_secs_f64() * 1000.0,
                outcome.steps,
                outcome.capped,
                outcome.analyses.len()
            );
        }
        eprintln!("  --memo=off, 1 call, 20s watchdog (fair unmemoized baseline; one call only):");
        let start = Instant::now();
        let outcome = morpher_memo_off.parse_word(word);
        let elapsed = start.elapsed();
        eprintln!(
            "    memo=off: {:.2}ms (steps={}, capped={}, timed_out={}, n_analyses={})",
            elapsed.as_secs_f64() * 1000.0,
            outcome.steps,
            outcome.capped,
            outcome.timed_out,
            outcome.analyses.len()
        );
    }
}
