//! Conformance replay for W6 (co-occurrence rules): the three
//! `rust/conformance/cooccurrence/*` fixtures. Each `expected.tsv` is C#-oracle-generated
//! (parse-opt @ `ccf750e6`); see each fixture's README for the grammar design and row-by-row
//! rationale.
//!
//! Red-on-revert: reverting the co-occurrence evaluation in `hc-rules/src/validity.rs` (the
//! `allomorph_co_occurrence_ok`/`morpheme_co_occurrence_ok` calls in `allomorphs_valid_impl`) makes
//! every excluded/required row wrongly parse (or fail to parse) again -- e.g. `and-semantics-pin`'s
//! `sagka` starts parsing (the exact `90dcee64` regression shape), and `allomorph-basic`'s
//! `koyzka` starts parsing despite its excluded suffix. Reverting the loader change (restoring the
//! two `Unsupported` lints at `hc-grammar/src/load.rs`) makes all three fixtures fail to load at
//! all.

use std::path::{Path, PathBuf};

use hc_grammar::load;
use hc_parse::Morpher;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/cooccurrence")
        .join(name)
}

/// Replay one fixture: load its grammar, parse every word in `expected.tsv`, and assert the
/// signature matches the oracle-recorded one exactly. Returns the number of words checked so
/// callers can assert against the fixture's known row count (catching a truncated/mis-copied
/// `expected.tsv`).
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

/// Self-skip guard: `rust/conformance/` isn't a submodule yet (module doc), so `--include-ignored`
/// runs (CI's release sweep included) must not panic on the missing directory.
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
