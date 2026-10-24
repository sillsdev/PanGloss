//! Analysis-side template state across a non-blocking stratum, which the generic replay cannot see: it prunes, and synthesis refuses the same words anyway.
//! Why this file names a fixture: docs/design/fixture-pins.md

use pg_conformance_fixtures::require_fixture;
use pg_grammar::model::StratumId;
use pg_parse::{Morpher, ParseOptions};

fn final_templates_skipped_in(morpher: &Morpher, word: &str, stratum: StratumId) -> u64 {
    let (_, _, prunes) = morpher.parse_word_with_stats_and_prunes(word, &ParseOptions::default());
    prunes
        .iter()
        .filter(|row| row.stratum == stratum)
        .map(|row| row.counters.final_templates_skipped)
        .sum()
}

/// `mrNotClitic` (in `Clitics`, `isCliticRule="false"`) marks `sagdt` `NonTemplate`; that state must reach `Main`, which does not block, and stop its final verb template there.
#[test]
fn non_clitic_state_crosses_a_non_blocking_stratum_and_prunes_its_final_template() {
    let fixture = require_fixture("edge-cases", "clitic-rule-designation");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml())
        .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", fixture.label()));
    let main = StratumId(0);
    assert!(!grammar.is_blocking_stratum(main, |_| true));
    assert!(grammar.is_blocking_stratum(StratumId(1), |_| true));
    let morpher = Morpher::new(&grammar, usize::MAX);

    assert!(
        final_templates_skipped_in(&morpher, "sagdt", main) > 0,
        "sagdt's NonTemplate state from Clitics must prune Main's final template"
    );
    assert_eq!(
        final_templates_skipped_in(&morpher, "sagds", main),
        0,
        "a clitic rule (mrEnc) leaves the state alone, so nothing is pruned for sagds"
    );
    assert!(morpher.parse_word("sagdt").analyses.is_empty());
}
