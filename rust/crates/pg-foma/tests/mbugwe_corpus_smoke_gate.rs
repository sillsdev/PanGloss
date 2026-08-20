//! Corpus-presence and oracle-recall smoke gate for the Mbugwe fwdata grammar: the grammar compiles, and at least one word of a small deterministic sample gets a definite analysis from the full engine (never a panic). Corpus-blocked (needs gitignored `samples/data/mbugwe.*`), so this is `#[ignore]`d unconditionally; run with `--include-ignored`.

use std::time::Duration;

use pg_conformance_fixtures::corpus;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

fn load_grammar() -> Grammar {
    let path = corpus::require("mbugwe.fwdata");
    let (snapshot, _) =
        pg_fwdata::import_file(&path).unwrap_or_else(|e| panic!("import {}: {e}", path.display()));
    pg_grammar::compile_project(&snapshot)
        .map(|(g, _)| g)
        .unwrap_or_else(|e| panic!("compile mbugwe.fwdata: {e:?}"))
}

fn read_words(count: usize) -> Vec<String> {
    let path = corpus::require("mbugwe-words.txt");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .take(count)
        .map(str::to_owned)
        .collect()
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/mbugwe.fwdata); run with --include-ignored"]
fn mbugwe_grammar_compiles_and_oracle_parses_a_sample() {
    if corpus::path("mbugwe.fwdata").is_none() {
        eprintln!("skipping: mbugwe.fwdata not present");
        return;
    }
    let g = load_grammar();
    // A bare `Morpher` with no timeout can run long enough on a slow Mbugwe word to attempt a multi-gigabyte allocation and abort the whole process.
    let morpher = Morpher::new(&g, usize::MAX).with_word_timeout(Some(Duration::from_secs(5)));
    let words = read_words(20);
    assert!(!words.is_empty(), "mbugwe-words.txt has at least one word");

    let mut analyzed = 0usize;
    for word in &words {
        let outcome = morpher.parse_word_opts(word, &ParseOptions::default());
        if !outcome.structured.is_empty() {
            analyzed += 1;
        }
    }
    corpus::record_cases(
        "mbugwe_grammar_compiles_and_oracle_parses_a_sample",
        words.len(),
    );
    assert!(
        analyzed > 0,
        "expected at least one of {} sampled words to receive an oracle analysis, got 0",
        words.len()
    );
}
