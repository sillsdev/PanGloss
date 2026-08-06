//! Conformance replay for the `StrRep` identity dimension on a zero-phonological-feature grammar; red-on-revert if `PatternBridge::id_lane`, the environment recheck in `validity.rs`/`cache.rs`, or the `GetSkippedOptionalNodes` fold in `morph.rs::copy_part` is disabled.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/allomorphy/strrep-identity")
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn strrep_identity_matches_oracle() {
    if !fixture_dir().join("grammar.xml").exists() {
        eprintln!("skipping: rust/conformance/allomorphy/strrep-identity not present on disk");
        return;
    }
    let dir = fixture_dir();
    let xml = std::fs::read_to_string(dir.join("grammar.xml")).expect("read grammar.xml");
    let grammar =
        load(&xml).unwrap_or_else(|e| panic!("strrep-identity grammar failed to load: {e}"));
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
            "strrep-identity: word {word:?} signature mismatch vs C# oracle"
        );
        checked += 1;
    }
    assert_eq!(checked, 12, "expected.tsv should pin all 12 fixture words");
}
