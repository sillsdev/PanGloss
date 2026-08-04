//! M7 determinism property test (plan §7, §8 layer 3): `hc_parse_batch` must produce
//! byte-identical per-word signatures regardless of `max_threads`. Per-word computation shares
//! nothing mutable (`Morpher::parse_word` takes `&self`; its only `RefCell` is a fresh per-call
//! memo scope — see `pg-parse/src/morpher.rs` module docs), so this is a correctness gate, not
//! an aspiration: a failure here means something *is* shared across threads that shouldn't be.
//!
//! Uses the small Indonesian sample grammar/corpus (`root_trie_gate.rs` pattern: self-skip when
//! the untracked sample files are absent).
//!
//! Deliberately runs with `Morpher::new(&grammar, usize::MAX)` (uncapped): this test's job is to
//! prove the batch *scheduling* layer adds no nondeterminism, not to re-litigate the pre-existing,
//! step-cap-truncation-dependent nondeterminism tracked in `rust-parity-facts.md` (reproduces at
//! `--threads 1` too, on unmodified `pg-rules`/`pg-parse` engine code — unrelated to this module).
//! If this test is ever pointed at a corpus/step-cap combination where words hit the cap, expect
//! it to go flaky for reasons that have nothing to do with `hc_parse_batch`.
//!
//! Test-timing policy: the default local `cargo test --workspace --release`
//! run must stay under ~60s and must not depend on these gitignored fixtures at all, so both tests
//! here are unconditionally `#[ignore = "..."]`d; run with `--include-ignored` locally.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::{hc_parse_batch, Morpher};

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml, indonesian-words.txt); run with --include-ignored"]
fn indonesian_batch_is_thread_count_invariant() {
    let Some(grammar_path) = sample_path("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let Some(words_path) = sample_path("indonesian-words.txt") else {
        eprintln!("skipping: indonesian-words.txt not present on disk");
        return;
    };
    let xml = std::fs::read_to_string(&grammar_path).expect("read grammar");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}"));
    let words: Vec<String> = std::fs::read_to_string(&words_path)
        .expect("read words")
        .lines()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect();
    assert!(
        words.len() >= 50,
        "expected the full Indonesian corpus, got {}",
        words.len()
    );

    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    let thread_counts = [1usize, 2, 4, 8];
    let mut runs: Vec<Vec<String>> = Vec::new();
    for &threads in &thread_counts {
        let results = hc_parse_batch(&morpher, &words, threads);
        assert_eq!(
            results.len(),
            words.len(),
            "threads={threads}: result count must match word count"
        );
        let sigs: Vec<String> = results.iter().map(|r| r.outcome.signature()).collect();
        runs.push(sigs);
    }

    // Every run's signature vector must be byte-identical to the 1-thread baseline, word for word.
    let baseline = &runs[0];
    for (idx, &threads) in thread_counts.iter().enumerate().skip(1) {
        for (i, word) in words.iter().enumerate() {
            assert_eq!(
                runs[idx][i], baseline[i],
                "word {i} ({word:?}) signature differs between threads=1 and threads={threads}: \
                 {:?} vs {:?}",
                baseline[i], runs[idx][i],
            );
        }
    }
    eprintln!(
        "determinism OK: {} words identical across thread counts {:?}",
        words.len(),
        thread_counts
    );
}

/// Output order must be the original word order, independent of the longest-first dispatch
/// order used internally for scheduling.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml, indonesian-words.txt); run with --include-ignored"]
fn indonesian_batch_output_order_matches_input_order() {
    let Some(grammar_path) = sample_path("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let Some(words_path) = sample_path("indonesian-words.txt") else {
        eprintln!("skipping: indonesian-words.txt not present on disk");
        return;
    };
    let xml = std::fs::read_to_string(&grammar_path).expect("read grammar");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}"));
    let words: Vec<String> = std::fs::read_to_string(&words_path)
        .expect("read words")
        .lines()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect();

    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);
    let results = hc_parse_batch(&morpher, &words, 4);
    assert_eq!(results.len(), words.len());
    // Cross-check every entry against a direct sequential parse at the same index.
    for (i, word) in words.iter().enumerate() {
        let direct = morpher.parse_word(word).signature();
        assert_eq!(
            results[i].outcome.signature(),
            direct,
            "batch result at index {i} ({word:?}) does not match a direct parse_word call \
             at the same index — output order is not original-index-order",
        );
    }
}
