//! Conformance spec for the root-allomorph `PhoneticShape` pattern-language fallback (N3): a pattern-derived trie node has `char_def == NO_CHAR_DEF`, so matching an actual input word against it needs `edge_matches` to accept a concrete query `char_def` by `pg_shape::CdSet` membership plus lane unifiability, not literal `char_def` equality. `expected.tsv` shows the oracle resolving `bat`/`bet` via this mechanism (`bit` is skipped, "i" is not in `Vowel`).

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/loader/n3-pattern-shapes")
        .join(name)
}

/// The loader-only half: the grammar loads and the pattern-shaped allomorph is present, re-run here against the committed fixture file rather than an inline XML literal.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn n3_pattern_shapes_grammar_loads_with_the_allomorph_present() {
    if !fixture_path("grammar.xml").exists() {
        eprintln!("skipping: rust/conformance/loader/n3-pattern-shapes not present on disk");
        return;
    }
    let xml = std::fs::read_to_string(fixture_path("grammar.xml")).expect("read grammar.xml");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("grammar failed to load: {e}"));
    assert_eq!(grammar.entries.len(), 1, "the lexical entry must survive");
    assert_eq!(
        grammar.entries[0].allomorphs.len(),
        1,
        "the pattern-shaped allomorph must survive"
    );
}

/// End-to-end replay against the oracle TSV; red-on-revert: reverting `edge_matches`'s `NO_CHAR_DEF`-edge membership branch makes `bat`/`bet` return `-` again.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn n3_pattern_shapes_matches_oracle_end_to_end() {
    if !fixture_path("grammar.xml").exists() {
        eprintln!("skipping: rust/conformance/loader/n3-pattern-shapes not present on disk");
        return;
    }
    let xml = std::fs::read_to_string(fixture_path("grammar.xml")).expect("read grammar.xml");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("grammar failed to load: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    let cases = [("bat", "|b[ae]t"), ("bet", "|b[ae]t")];
    for (word, expected) in cases {
        let got = morpher.parse_word(word).signature();
        assert_eq!(
            got, expected,
            "word {word:?}: signature mismatch vs C# oracle"
        );
    }
}
