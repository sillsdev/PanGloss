//! Shared-template parse identities from the Machine conformance grammar.

use pg_conformance_fixtures::{assert_matches_oracle, discover_scoped, ConformanceScope};
use pg_parse::Morpher;

#[test]
fn shared_template_unconstrained_suffix_preserves_every_identity_in_both_orders() {
    let fixtures = discover_scoped(ConformanceScope::All);
    let matching: Vec<_> = fixtures
        .iter()
        .filter(|f| f.category == "edge-cases" && f.name == "shared-template-unconstrained-suffix")
        .collect();
    assert_eq!(matching.len(), 1, "exactly one authoritative fixture must exist");
    let fixture = matching[0];
    let words = fixture.load_words_yaml();
    assert_eq!(words.words.len(), 10, "the oracle corpus must not shrink");
    assert!(words.skip_in_generic_replay().is_none());

    for reversed in [false, true] {
        let mut grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("Machine grammar loads");
        assert_eq!(grammar.strata.len(), 1);
        assert_eq!(grammar.strata[0].templates.len(), 2);
        if reversed {
            grammar.strata[0].templates.reverse();
        }
        for memo in [false, true] {
            let morpher = Morpher::new(&grammar, usize::MAX).with_memo(memo);
            let label = format!("{} reversed={reversed} memo={memo}", fixture.label());
            assert_eq!(assert_matches_oracle(&label, &words, &morpher), 10);
        }
    }
}
