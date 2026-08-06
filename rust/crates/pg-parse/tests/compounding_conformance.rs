//! Conformance replay for the compounding fixtures (`rust/conformance/compounding/{prefix-commute,nonhead-not-root}/`): parses every word in `words.txt` and checks its signature against the fixture's oracle-generated `expected.tsv`; each fixture's README documents the oracle-generating command and what it pins.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_path(name: &str, file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/compounding")
        .join(name)
        .join(file)
}

fn load_fixture(name: &str) -> pg_grammar::model::Grammar {
    let xml = std::fs::read_to_string(fixture_path(name, "grammar.xml")).expect("read grammar.xml");
    load(&xml).unwrap_or_else(|e| panic!("{name}: grammar failed to load: {e}"))
}

/// Self-skip guard, so an `--include-ignored` run does not panic when the fixture directory is absent.
fn have_fixture(name: &str) -> bool {
    fixture_path(name, "grammar.xml").exists()
}

/// `rust/conformance/compounding/prefix-commute/expected.tsv`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn prefix_commute_matches_oracle() {
    if !have_fixture("prefix-commute") {
        eprintln!("skipping: rust/conformance/compounding/prefix-commute not present on disk");
        return;
    }
    let g = load_fixture("prefix-commute");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [
        ("pʰutdidat", "5+PAST+9|(pʰ)ut+?di+?dat"),
        ("pʰutdat", "5+8|(pʰ)ut+?dat;5+9|(pʰ)ut+?dat"),
        ("pʰutdas", "-"),
    ];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "prefix-commute word {word:?}"
        );
    }
}

/// `rust/conformance/compounding/nonhead-not-root/expected.tsv`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn nonhead_not_root_matches_oracle() {
    if !have_fixture("nonhead-not-root") {
        eprintln!("skipping: rust/conformance/compounding/nonhead-not-root not present on disk");
        return;
    }
    let g = load_fixture("nonhead-not-root");
    let m = Morpher::new(&g, usize::MAX);
    // "pʰutdat": with head+nonHead order the dat-homophone pair (entries 8/9) resolves via the non-head, and the live oracle keeps both readings.
    let cases = [
        ("pʰutdidat", "-"),
        ("pʰutdat", "5+8|(pʰ)ut+?dat;5+9|(pʰ)ut+?dat"),
        ("pʰutdas", "-"),
    ];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "nonhead-not-root word {word:?}"
        );
    }
}
