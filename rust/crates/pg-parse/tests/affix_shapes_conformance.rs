//! Conformance replay for the phase-2 W9.1 affix-shape probes
//! (`rust/conformance/affix-shapes/{infix,circumfix,noncontiguous,truncate}/`): load each fixture's
//! `grammar.xml` exactly as authored, parse every word in `words.txt`, and check
//! `Morpher::parse_word(...).signature()` against the literal signature transcribed from that
//! fixture's oracle-generated `expected.tsv` (same convention as
//! `crates/pg-parse/tests/discontinuous_env_gate.rs`). Each fixture's README documents the
//! oracle-generating command and verdict.
//!
//! `truncate` was the one DIVERGING fixture at freeze time (wave-3): a pure-truncation affix
//! (`Rhs` = `CopyFromInput` only, no `InsertSegments`) lost its rule-application marker from the
//! signature. Fixed in wave-4 (`pg_rules::morph::attribute_morphs`'s tail-ordered fallback record,
//! mirroring C#'s `outputNewMorph == null` branch). Red-on-revert: reverting that fallback drops
//! `ag`/`as`/`sa` back to a `+`-count one lower than the oracle's, and `gas`'s two-analysis set
//! collapses to two identical `+|gas` strings instead of `++|gas;+|gas`.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/affix-shapes")
        .join(name)
}

/// Replay one fixture's `words.txt` against its `expected.tsv`, returning the number of words
/// checked (used to assert every fixture word was actually exercised, not silently skipped).
fn replay(name: &str) -> usize {
    let dir = fixture_dir(name);
    let xml = std::fs::read_to_string(dir.join("grammar.xml")).expect("read grammar.xml");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("{name}: grammar failed to load: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    let text = std::fs::read_to_string(dir.join("expected.tsv")).expect("read expected.tsv");
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

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn infix_matches_oracle() {
    assert_eq!(replay("infix"), 6);
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn circumfix_matches_oracle() {
    assert_eq!(replay("circumfix"), 5);
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn noncontiguous_matches_oracle() {
    assert_eq!(replay("noncontiguous"), 4);
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn truncate_matches_oracle() {
    assert_eq!(replay("truncate"), 9);
}
