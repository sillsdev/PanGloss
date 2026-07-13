//! Conformance replay for the phase-2 W4 metathesis fixtures
//! (`rust/conformance/metathesis/{simple_rule,complex_rule,not_unapplied}/`): load each fixture's
//! `grammar.xml` exactly as authored (no `csharp_port_common` scaffolding — these are standalone,
//! oracle-verified fixtures), parse every word in `words.txt`, and check the resulting
//! `Morpher::parse_word(...).signature()` against the literal signature transcribed from that
//! fixture's oracle-generated `expected.tsv` (same convention as
//! `crates/hc-parse/tests/loader_n2_default_symbol_gate.rs`). Each fixture's README documents the
//! oracle-generating command and the derivation of every expected value.

use std::path::{Path, PathBuf};

use hc_grammar::load;
use hc_parse::Morpher;

fn fixture_path(name: &str, file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/metathesis")
        .join(name)
        .join(file)
}

fn load_fixture(name: &str) -> hc_grammar::model::Grammar {
    let xml = std::fs::read_to_string(fixture_path(name, "grammar.xml")).expect("read grammar.xml");
    load(&xml).unwrap_or_else(|e| panic!("{name}: grammar failed to load: {e}"))
}

/// `rust/conformance/metathesis/simple_rule/expected.tsv`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn simple_rule_matches_oracle() {
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
