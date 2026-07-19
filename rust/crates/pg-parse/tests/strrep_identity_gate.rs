//! Conformance replay for P10 (the Sena null-allomorph headline): the `StrRep` identity dimension
//! on a zero-phonological-feature grammar, `rust/conformance/allomorphy/strrep-identity/`.
//! `expected.tsv` is C#-oracle-generated (parse-opt @ `ccf750e6`); see the fixture README for the
//! grammar design and the row-by-row rationale.
//!
//! Red-on-revert, three independent ways:
//! - turn off `PatternBridge::id_lane` in `morph.rs::compile_parts` and `pat`/`mupat` lose their
//!   null/`mu+` parses while `mwpat` starts parsing (the disjunctive-break arm);
//! - swap `validity.rs`/`cache.rs` back to plain `compile_env` and `ndpat` goes to `-` (the W3.2
//!   environment-recheck arm);
//! - drop the `GetSkippedOptionalNodes` fold from `morph.rs::copy_part` and `ndpat`/`imat` lose
//!   their medial-zero (`nd+?[(^0)∅]?+?pat`) rows.

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
