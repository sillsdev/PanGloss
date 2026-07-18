//! Conformance replay for W5 (the "realizational cluster": `StemName`, `LexFamily` blocking,
//! `RealizationalAffixProcessRule`): the three `rust/conformance/realizational/*` fixtures. Each
//! `expected.tsv` is C#-oracle-generated (parse-opt, frozen); see each fixture's README for the
//! grammar design and row-by-row rationale.
//!
//! Red-on-revert: reverting `pg-rules/src/validity.rs`'s `stem_name_gates_ok` call (the two W5
//! call sites in `allomorphs_valid_impl`'s `AllomorphOwner::Root` arm) makes every excluded row in
//! `stem-name` wrongly parse (e.g. `saned`/`sant`/`sans`/`sads`/`sapt` all start parsing).
//! Reverting the `apply_blocking` wiring in `pg-rules/src/morph.rs`'s `synthesize`/
//! `synthesize_cached` makes `family-blocking`'s `katid` wrongly parse (as `KAT+PAST|katid`).
//! Reverting `MorphRuleDef::Realizational`'s dispatch arms (or the loader's `try_load_
//! realizational_rule`) makes `realizational-rule` fail to load at all with
//! `Unsupported("RealizationalRule")`.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/realizational")
        .join(name)
}

/// Replay one fixture: load its grammar, parse every word in `expected.tsv`, and assert the
/// signature matches the oracle-recorded one exactly. Returns the number of words checked so
/// callers can assert against the fixture's known row count (catching a truncated/mis-copied
/// `expected.tsv`).
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

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn stem_name_matches_oracle() {
    assert_eq!(
        replay("stem-name"),
        12,
        "expected.tsv should pin all 12 fixture words"
    );
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn family_blocking_matches_oracle() {
    assert_eq!(
        replay("family-blocking"),
        4,
        "expected.tsv should pin all 4 fixture words"
    );
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn realizational_rule_matches_oracle() {
    assert_eq!(
        replay("realizational-rule"),
        4,
        "expected.tsv should pin all 4 fixture words"
    );
}
