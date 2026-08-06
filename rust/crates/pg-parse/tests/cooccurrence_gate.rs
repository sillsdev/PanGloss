//! Conformance replay for co-occurrence rules against C#-oracle-generated `rust/conformance/cooccurrence/*` fixtures; reverting the co-occurrence evaluation in `pg-rules/src/validity.rs` makes excluded/required rows wrongly parse (or fail to parse) again.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/cooccurrence")
        .join(name)
}

/// Replays one fixture, asserting each word's signature matches the oracle-recorded one; returns the count checked so callers can assert against the fixture's known row count.
fn replay(name: &str) -> usize {
    let dir = fixture_dir(name);
    let xml = std::fs::read_to_string(dir.join("grammar.xml"))
        .unwrap_or_else(|e| panic!("read {name}/grammar.xml: {e}"));
    let grammar = load(&xml).unwrap_or_else(|e| panic!("{name} grammar failed to load: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    let text = std::fs::read_to_string(dir.join("expected.tsv"))
        .unwrap_or_else(|e| panic!("read {name}/expected.tsv: {e}"));
    let mut checked = 0;
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 5 {
            continue; // interleaved STARTED sentinel rows
        }
        let (word, expected_sig) = (cols[1], cols[4]);
        let got = morpher.parse_word(word).signature();
        assert_eq!(
            got, expected_sig,
            "{name}: word {word:?} signature mismatch vs C# oracle"
        );
        checked += 1;
    }
    checked
}

/// Self-skip guard, so an `--include-ignored` run does not panic when the fixture directory is absent.
fn have_fixture(name: &str) -> bool {
    fixture_dir(name).join("grammar.xml").exists()
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn morpheme_adjacency_matches_oracle() {
    if !have_fixture("morpheme-adjacency") {
        eprintln!("skipping: rust/conformance/cooccurrence/morpheme-adjacency not present on disk");
        return;
    }
    assert_eq!(
        replay("morpheme-adjacency"),
        16,
        "expected.tsv should pin all 16 fixture words"
    );
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn allomorph_basic_matches_oracle() {
    if !have_fixture("allomorph-basic") {
        eprintln!("skipping: rust/conformance/cooccurrence/allomorph-basic not present on disk");
        return;
    }
    assert_eq!(
        replay("allomorph-basic"),
        4,
        "expected.tsv should pin all 4 fixture words"
    );
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn and_semantics_pin_matches_oracle() {
    if !have_fixture("and-semantics-pin") {
        eprintln!("skipping: rust/conformance/cooccurrence/and-semantics-pin not present on disk");
        return;
    }
    assert_eq!(
        replay("and-semantics-pin"),
        2,
        "expected.tsv should pin both fixture words"
    );
}
