//! Conformance spec for phase-2 audit C finding N3 (root-allomorph `PhoneticShape` pattern-language
//! fallback). Fixture: `rust/conformance/loader/n3-pattern-shapes/`.
//!
//! **Scope note (read before "fixing" this test):** N3's audit/plan scope was explicitly
//! loader-only ("port `GetShapeNodes`'s `allowPattern=true` branch into a `segment_with_patterns`
//! used only by the root-allomorph loader") -- and that half is done: `pg_grammar::load` no longer
//! silently drops a root allomorph whose `<PhoneticShape>` needs the `[NatClass]`/`([NatClass])`/
//! `[NatClass]*` pattern language (see `pg-grammar/src/segment.rs::segment_with_patterns` and its
//! unit tests, plus `pg-grammar/src/load.rs`'s
//! `root_allomorph_shape_falls_back_to_pattern_language_natural_class_reference`, which is the real
//! red-on-revert regression guard for the loader half).
//!
//! What the loader-only scope did **not** cover, discovered while building this fixture: matching
//! an *actual input word* against such a root allomorph at analysis time. `pg-parse::root_trie`'s
//! `RootAllomorphTrie::add_path` inserted one edge per `Segment` node keyed by its literal
//! `char_def`; a pattern-derived node has `char_def == NO_CHAR_DEF`, so its edge was unreachable
//! by any real input text (the old `cd_ok` only treated a **query** segment's `NO_CHAR_DEF` as a
//! wildcard, never an **edge**'s).
//!
//! **FIXED (wave-4, N3 end-to-end):** `TrieEdge` now carries the stored node's `pg_shape::CdSet`
//! (the class's member set, from `Shape::node_cd_set`), and `edge_matches` accepts a concrete
//! query `char_def` against a `NO_CHAR_DEF` edge by **set membership** + lane unifiability — the
//! port's analog of C#'s arc-condition `FeatureStruct` unification against a class-only
//! (no-`StrRep`) condition (`RootAllomorphTrie.cs:39-40,61-63`, `UseUnification = true`). Edge
//! grouping for pattern edges keys on `cd_set` + lanes (the `ValueEquals` analog), so distinct
//! classes never merge. See `root_trie.rs`'s unit tests for the isolated-trie coverage; the test
//! below (formerly `#[ignore]`d on exactly this gap) is the end-to-end oracle replay.
//!
//! `expected.tsv` (oracle-generated, C# `parse-optimization` HEAD `ccf750e6`) shows C# resolving
//! `bat`/`bet` to signature `|b[ae]t` via this exact mechanism (`bit` is SKIPPED -- "i" is not in
//! the `Vowel` class).

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/loader/n3-pattern-shapes")
        .join(name)
}

/// The loader-only half: the grammar loads and the allomorph is present (mirrors
/// `pg-grammar`'s own `root_allomorph_shape_falls_back_to_pattern_language_natural_class_reference`
/// unit test, re-run here against the committed fixture file rather than an inline XML literal).
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

/// End-to-end replay against the oracle TSV — live since wave-4's `CdSet`-aware edge matching
/// landed in `root_trie.rs` (see module doc). Red-on-revert: reverting `edge_matches`'s
/// `NO_CHAR_DEF`-edge membership branch makes `bat`/`bet` return `-` again.
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
