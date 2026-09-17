//! Pins Machine issue #506 (docs/divergences/036-zero-width-morpheme-identity.md).

use pg_conformance_fixtures::require_fixture;
use pg_parse::{result_multiset, Morpher};

/// Asserts `word`'s exact analysis multiset (sorted, not deduped) against `expected`.
#[track_caller]
fn assert_multiset(m: &Morpher, word: &str, expected: &[&str]) {
    let outcome = m.parse_word(word);
    assert!(!outcome.invalid_shape, "{word}: must segment");
    let got = result_multiset(&outcome.analyses);
    let mut want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(got, want, "{word}: got {got:?}, want {want:?}");
}

#[test]
fn sagui_pins_the_forward_synthesized_multiset_with_third_outermost() {
    let fixture = require_fixture("edge-cases", "zero-width-morpheme-identity-stability");
    let xml = fixture.load_grammar_xml();
    let g = pg_grammar::load(&xml)
        .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", fixture.label()));
    let m = Morpher::new(&g, usize::MAX).with_memo(true);

    // Controls, per fixture STAGING.md.
    assert_multiset(&m, "sag", &["ROOT|sag", "ROOT+THIRD|sag"]);
    assert_multiset(&m, "sagu", &["ROOT+A2B|sagu"]);
    assert_multiset(&m, "sagi", &["ROOT+THIRD+B2A|sagi"]);

    // The pin: both analyses keep A2B+B2A; THIRD renders outermost, never mid-string, never dropped.
    assert_multiset(
        &m,
        "sagui",
        &["ROOT+A2B+B2A|sagui", "ROOT+A2B+B2A+THIRD|sagui"],
    );
}

/// Companion to `assert_multiset`'s controls: repeats "sagui" over many fresh `Morpher`s in one process.
#[test]
fn sagui_is_deterministic_across_fresh_morphers_in_process() {
    let fixture = require_fixture("edge-cases", "zero-width-morpheme-identity-stability");
    let xml = fixture.load_grammar_xml();
    let g = pg_grammar::load(&xml)
        .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", fixture.label()));

    let mut sigs = Vec::new();
    for _ in 0..50 {
        let m = Morpher::new(&g, usize::MAX).with_memo(true);
        sigs.push(m.parse_word("sagui").signature());
    }
    let first = &sigs[0];
    assert!(
        sigs.iter().all(|s| s == first),
        "nondeterministic: {sigs:?}"
    );
    assert_eq!(*first, "ROOT+A2B+B2A+THIRD|sagui;ROOT+A2B+B2A|sagui");
}
