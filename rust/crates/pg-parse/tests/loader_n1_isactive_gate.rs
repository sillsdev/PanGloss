//! With two `<PhonologicalFeatureSystem>` blocks where only the first is `isActive`, the loader must select the active block for feature resolution, matching C#'s `SingleOrDefault(IsActive)`; disabling that gate makes the inactive block's unrelated feature set win and the whole grammar fail to load, which is this fixture's red-on-revert signal.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_path(name: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR = .../rust/crates/pg-parse ; fixtures live at repo_root/rust/conformance.
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

    // (word, expected signature), from the oracle-generated expected.tsv.
    let cases = [("kat", "|kat"), ("sod", "|sod")];
    for (word, expected) in cases {
        let got = morpher.parse_word(word).signature();
        assert_eq!(
            got, expected,
            "word {word:?}: signature mismatch vs C# oracle"
        );
    }
}
