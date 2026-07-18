//! Conformance replay for phase-2 audit C finding N1 (`PhonologicalFeatureSystem@isActive`).
//!
//! Fixture: `rust/conformance/loader/n1-isactive/{grammar.xml,words.txt,expected.tsv}`. The
//! grammar declares two `<PhonologicalFeatureSystem>` blocks: the first (default `isActive`,
//! DTD default "yes") declares feature `voi`, which `<NaturalClasses><FeatureNaturalClass
//! id="voiced">` resolves; the second is `isActive="no"` and declares an unrelated `junk`
//! feature only. C# `XmlLanguageLoader.LoadLanguage` selects
//! `Elements("PhonologicalFeatureSystem").SingleOrDefault(IsActive)` (`XmlLanguageLoader.cs:239`)
//! — i.e. the first (only active) block — so `voi` resolves and the grammar loads fine.
//!
//! Pre-fix, `hc-grammar`'s `load_char_def_table_from_xml` had no `isActive` check at all and let
//! the *last* `<PhonologicalFeatureSystem>` block in the file win regardless of its `isActive`
//! value; with this fixture's block order (active first, inactive-and-different second) that bug
//! selects the "junk"-only block, so `voiced`'s `<FeatureValue feature="voi" .../>` fails to
//! resolve at load time (`GrammarError::Semantic("unknown phonological feature 'voi'")`) and the
//! *whole grammar* fails to load — this is the red-on-revert signal for this fixture: comment out
//! the `is_active`/`phon_feat_sys_selected` gate in `lib.rs::load_char_def_table_from_xml` and
//! this test starts panicking on `hc_grammar::load` itself, before ever reaching the signature
//! assertions below.
//!
//! `expected.tsv` was oracle-generated:
//! `DOTNET_gcServer=0 hc.exe -i grammar.xml -s script.txt` against
//! `.worktrees/parse-opt/src/.../hc.dll` (`parse-optimization` HEAD `ccf750e6`), which loads this
//! grammar successfully (proving C# picks the active block) and produces the two signatures below.

use std::path::{Path, PathBuf};

use hc_grammar::load;
use hc_parse::Morpher;

fn fixture_path(name: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR = .../rust/crates/hc-parse ; fixtures live at repo_root/rust/conformance.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/loader/n1-isactive")
        .join(name)
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn n1_isactive_grammar_loads_and_matches_oracle_signatures() {
    if !fixture_path("grammar.xml").exists() {
        eprintln!("skipping: rust/conformance/loader/n1-isactive not present on disk");
        return;
    }
    let grammar_path = fixture_path("grammar.xml");
    let xml = std::fs::read_to_string(&grammar_path).expect("read n1-isactive/grammar.xml");
    let grammar = load(&xml).unwrap_or_else(|e| {
        panic!(
            "n1-isactive grammar failed to load (isActive not honored — last-block-wins bug?): {e}"
        )
    });
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    // (word, expected BatchCommand-protocol signature) — from the oracle-generated
    // `expected.tsv` (rows 2 and 4, column 5).
    let cases = [("kat", "|kat"), ("sod", "|sod")];
    for (word, expected) in cases {
        let got = morpher.parse_word(word).signature();
        assert_eq!(
            got, expected,
            "word {word:?}: signature mismatch vs C# oracle"
        );
    }
}
