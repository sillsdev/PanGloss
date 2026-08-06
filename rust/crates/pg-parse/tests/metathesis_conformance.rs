//! Loads each metathesis fixture's `grammar.xml` exactly as authored, parses every word in `words.txt`, and checks the resulting signature against the literal value transcribed from that fixture's oracle-generated `expected.tsv`.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_path(name: &str, file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/metathesis")
        .join(name)
        .join(file)
}

fn load_fixture(name: &str) -> pg_grammar::model::Grammar {
    let xml = std::fs::read_to_string(fixture_path(name, "grammar.xml")).expect("read grammar.xml");
    load(&xml).unwrap_or_else(|e| panic!("{name}: grammar failed to load: {e}"))
}

/// Self-skip guard, so `--include-ignored` runs must not panic when the fixture directory is missing.
fn have_fixture(name: &str) -> bool {
    fixture_path(name, "grammar.xml").exists()
}

/// `rust/conformance/metathesis/simple_rule/expected.tsv`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn simple_rule_matches_oracle() {
    if !have_fixture("simple_rule") {
        eprintln!("skipping: rust/conformance/metathesis/simple_rule not present on disk");
        return;
    }
    let g = load_fixture("simple_rule");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [("mui", "51|mui"), ("miu", "-")];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "simple_rule word {word:?}"
        );
    }
}

/// `rust/conformance/metathesis/complex_rule/expected.tsv`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn complex_rule_matches_oracle() {
    if !have_fixture("complex_rule") {
        eprintln!("skipping: rust/conformance/metathesis/complex_rule not present on disk");
        return;
    }
    let g = load_fixture("complex_rule");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [
        ("mu+i", "53+|mu+?i"),
        ("mui", "53+|mu+?i"),
        ("mi", "53|mi"),
        ("mi+u", "-"),
    ];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "complex_rule word {word:?}"
        );
    }
}

/// `rust/conformance/metathesis/not_unapplied/expected.tsv`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn not_unapplied_matches_oracle() {
    if !have_fixture("not_unapplied") {
        eprintln!("skipping: rust/conformance/metathesis/not_unapplied not present on disk");
        return;
    }
    let g = load_fixture("not_unapplied");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [("pui", "52+|pui"), ("piu", "-")];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "not_unapplied word {word:?}"
        );
    }
}
