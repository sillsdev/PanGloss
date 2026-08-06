//! `hc_parse_batch` must produce byte-identical per-word signatures regardless of `max_threads`; runs uncapped (`usize::MAX`) so a step-cap-truncation flake can never be mistaken for scheduling nondeterminism. Self-skips (like `root_trie_gate.rs`) when the untracked Indonesian sample is absent, and stays `#[ignore]`d unconditionally.

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

/// Output order must match input order, independent of the longest-first dispatch order used internally for scheduling.
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
