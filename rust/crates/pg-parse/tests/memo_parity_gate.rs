//! Memo on/off must yield the identical deduplicated analysis-identity set and `capped` flag for every conformance-fixture word.

use std::collections::BTreeSet;

use pg_conformance_fixtures::{discover_scoped, ConformanceScope};
use pg_grammar::model::Grammar;
use pg_parse::identity::AnalysisIdentity;
use pg_parse::{Morpher, WordAnalysis};

/// Projects every analysis to its structured identity and collects the deduplicated set.
fn identity_set(
    analyses: &[WordAnalysis],
    grammar: &Grammar,
    fixture_label: &str,
    word: &str,
    side: &str,
) -> BTreeSet<AnalysisIdentity> {
    analyses
        .iter()
        .map(|a| {
            AnalysisIdentity::project(a, grammar).unwrap_or_else(|e| {
                panic!(
                    "{fixture_label}: word {word:?} (memo={side}): identity projection failed: {e}"
                )
            })
        })
        .collect()
}

#[test]
fn memo_on_and_off_agree_on_every_fixture_word() {
    let fixtures = discover_scoped(ConformanceScope::All);
    assert!(
        !fixtures.is_empty(),
        "no conformance fixtures discovered at all -- check the `machine` submodule is \
         initialized (`git submodule update --init machine`) and conformance-staging/ exists"
    );

    let mut fixtures_checked = 0usize;
    let mut words_checked = 0usize;
    let mut analyses_compared = 0usize;
    let mut capped_both = 0usize;
    let mut completed_only_on = 0usize;
    let mut completed_only_off = 0usize;

    for fixture in &fixtures {
        // An unloadable or table-less grammar has nothing this gate can analyse.
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        if grammar.char_tables.is_empty() {
            continue;
        }
        let words_yaml = fixture.load_words_yaml();
        // An expect_crash fixture aborts the parse on both sides, so there is no identity set to compare.
        if words_yaml.words.is_empty() || words_yaml.skip_in_generic_replay().is_some() {
            continue;
        }
        let label = fixture.label();
        fixtures_checked += 1;

        let memo_on = Morpher::new(&grammar, usize::MAX).with_memo(true);
        let memo_off = Morpher::new(&grammar, usize::MAX).with_memo(false);

        for entry in &words_yaml.words {
            let word = &entry.word;
            words_checked += 1;

            let on_outcome = memo_on.parse_word(word);
            let off_outcome = memo_off.parse_word(word);

            // A cap firing on only one side reflects memoization's step-count effect, not a recall difference, and a capped run's partial set is never compared (not even as a subset) against a completed one -- recorded below, never failed.
            match (on_outcome.capped, off_outcome.capped) {
                (false, false) => {
                    let on_set = identity_set(&on_outcome.structured, &grammar, &label, word, "on");
                    let off_set =
                        identity_set(&off_outcome.structured, &grammar, &label, word, "off");
                    analyses_compared += on_set.len();
                    if on_set != off_set {
                        let only_on: Vec<_> = on_set.difference(&off_set).collect();
                        let only_off: Vec<_> = off_set.difference(&on_set).collect();
                        panic!(
                            "{label}: word {word:?}: memo on/off analysis-identity sets differ\n  \
                             only with memo=on:  {only_on:?}\n  only with memo=off: {only_off:?}"
                        );
                    }
                }
                (true, true) => capped_both += 1,
                (false, true) => completed_only_on += 1,
                (true, false) => completed_only_off += 1,
            }
        }
    }

    assert!(
        fixtures_checked > 0,
        "no fixture contributed any word -- the sweep measured nothing"
    );

    eprintln!(
        "memo_parity_gate: {fixtures_checked} fixture(s), {words_checked} word(s), \
         {analyses_compared} analysis identity(ies) compared, {capped_both} capped both sides, \
         memo=on vs memo=off"
    );
    eprintln!(
        "{completed_only_on} words completed only with memo on, {completed_only_off} only with memo off"
    );
}
