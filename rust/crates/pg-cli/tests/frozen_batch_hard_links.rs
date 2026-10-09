//! A frozen batch run refuses an output that is a hard link to one of its inputs, before writing.

#![cfg(unix)]

use std::path::Path;
use std::process::{Command, Output};

/// A grammar the compiler accepts, copied into each scratch directory so the link is ours to make.
fn write_grammar(path: &Path) {
    let source = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../conformance-staging/edge-cases/standalone-combining-mark/grammar.xml"
    );
    std::fs::copy(source, path).unwrap();
}

fn frozen_batch(grammar: &Path, words: &Path, out: &Path, cache: &Path, manifest: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .arg("batch")
        .arg(grammar)
        .arg(words)
        .arg(out)
        .args(["--stats", "--cache"])
        .arg(cache)
        .arg("--stats-manifest")
        .arg(manifest)
        .output()
        .expect("pangloss must start")
}

#[test]
fn frozen_batch_refuses_an_output_hard_linked_to_the_grammar() {
    let scratch = tempfile::tempdir().unwrap();
    let grammar = scratch.path().join("grammar.xml");
    let words = scratch.path().join("words.txt");
    let out = scratch.path().join("results.tsv");
    write_grammar(&grammar);
    std::fs::write(&words, "a\n").unwrap();
    std::fs::hard_link(&grammar, &out).unwrap();
    let grammar_bytes = std::fs::read(&grammar).unwrap();

    let output = frozen_batch(
        &grammar,
        &words,
        &out,
        &scratch.path().join("cache.sqlite"),
        &scratch.path().join("manifest.json"),
    );

    assert!(
        !output.status.success(),
        "the hard-linked output must be refused"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("stats manifest path collision"),
        "unexpected refusal: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(&grammar).unwrap(), grammar_bytes);
}

#[test]
fn frozen_batch_refuses_an_output_hard_linked_to_the_word_list() {
    let scratch = tempfile::tempdir().unwrap();
    let grammar = scratch.path().join("grammar.xml");
    let words = scratch.path().join("words.txt");
    let out = scratch.path().join("results.tsv");
    write_grammar(&grammar);
    std::fs::write(&words, "a\n").unwrap();
    std::fs::hard_link(&words, &out).unwrap();
    let words_bytes = std::fs::read(&words).unwrap();

    let output = frozen_batch(
        &grammar,
        &words,
        &out,
        &scratch.path().join("cache.sqlite"),
        &scratch.path().join("manifest.json"),
    );

    assert!(
        !output.status.success(),
        "the hard-linked output must be refused"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("stats manifest path collision"),
        "unexpected refusal: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(&words).unwrap(), words_bytes);
}
