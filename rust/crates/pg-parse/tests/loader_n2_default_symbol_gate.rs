//! When a feature declares `defaultSymbol` and a segment leaves that feature unset, C#'s rewrite matchers substitute and check the feature's default value instead of treating "unset" as vacuously compatible; `pattern_defaults_ok` in `pg-rules/src/rewrite.rs::syn_feature` replays that, and disabling it makes an unconstrained lane wrongly overlap a pin it shouldn't.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/loader/n2-default-symbol")
        .join(name)
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn n2_default_symbol_matches_oracle() {
    if !fixture_path("grammar.xml").exists() {
        eprintln!("skipping: rust/conformance/loader/n2-default-symbol not present on disk");
        return;
    }
    let xml = std::fs::read_to_string(fixture_path("grammar.xml")).expect("read grammar.xml");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("grammar failed to load: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    // (word, expected BatchCommand-protocol signature) — from the oracle-generated `expected.tsv`.
    let cases = [("bat", "|bat"), ("bdt", "-")];
    for (word, expected) in cases {
        let got = morpher.parse_word(word).signature();
        assert_eq!(
            got, expected,
            "word {word:?}: signature mismatch vs C# oracle"
        );
    }
}
