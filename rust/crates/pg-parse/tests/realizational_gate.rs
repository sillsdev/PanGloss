//! Conformance replay for the realizational cluster (`StemName`, `LexFamily` blocking, `RealizationalAffixProcessRule`): three fixtures, each `expected.tsv` C#-oracle-generated; reverting `stem_name_gates_ok`, the `apply_blocking` wiring, or `Realizational`'s dispatch arms each makes a specific fixture wrongly parse or fail to load.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/realizational")
        .join(name)
}

/// Replays one fixture: loads its grammar, parses every word in `expected.tsv`, asserts the signature matches, and returns the count checked, so callers can catch a truncated/mis-copied `expected.tsv`.
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

/// Self-skip guard, so `--include-ignored` runs must not panic when the fixture directory is missing.
fn have_fixture(name: &str) -> bool {
    fixture_dir(name).join("grammar.xml").exists()
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn stem_name_matches_oracle() {
    if !have_fixture("stem-name") {
        eprintln!("skipping: rust/conformance/realizational/stem-name not present on disk");
        return;
    }
    assert_eq!(
        replay("stem-name"),
        12,
        "expected.tsv should pin all 12 fixture words"
    );
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn family_blocking_matches_oracle() {
    if !have_fixture("family-blocking") {
        eprintln!("skipping: rust/conformance/realizational/family-blocking not present on disk");
        return;
    }
    assert_eq!(
        replay("family-blocking"),
        4,
        "expected.tsv should pin all 4 fixture words"
    );
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn realizational_rule_matches_oracle() {
    if !have_fixture("realizational-rule") {
        eprintln!(
            "skipping: rust/conformance/realizational/realizational-rule not present on disk"
        );
        return;
    }
    assert_eq!(
        replay("realizational-rule"),
        4,
        "expected.tsv should pin all 4 fixture words"
    );
}
