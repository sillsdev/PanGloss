use pg_conformance_fixtures::FixtureRef;
use pg_foma::compose_budget::ApplyBudget;
use pg_foma::composite::{FomaAnalyzer, ProfiledFomaApplyOutcome};
use pg_foma_backend::templated_compile::compile_templated_morphotactics;

pub(super) fn verify_templated_fixture(fixture: &FixtureRef) -> (usize, usize) {
    let name = fixture.label();
    let budget = ApplyBudget::with_caps(Some(128), Some(32));
    let mut checked = 0;
    let mut positives = 0;
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    let built =
        compile_templated_morphotactics(&grammar).unwrap_or_else(|error| panic!("{name}: {error}"));
    let mut analyzer = FomaAnalyzer::from_precompiled_proposer(&grammar, built.proposer);
    for word in fixture.load_words_yaml().words {
        assert!(word.adapter_visible() && !word.expect_skip);
        let ProfiledFomaApplyOutcome::Complete(profiled) =
            analyzer.analyze_word_with_diagnostics_budgeted(&word.word, &budget)
        else {
            panic!("{name}: {} exceeded the contained apply budget", word.word);
        };
        let outcome = profiled.outcome;
        if outcome.candidates_generated > 0 {
            assert!(
                profiled.diagnostics.confirmation_calls > 0,
                "{name}: {} bypassed HC confirmation",
                word.word
            );
        }
        assert!(outcome.peel_chain_depth_error.is_none());
        assert_eq!(
            pg_parse::result_multiset(&outcome.analyses),
            word.expected_multiset(),
            "{name}: {}",
            word.word
        );
        checked += 1;
        positives += usize::from(!outcome.analyses.is_empty());
    }
    (checked, positives)
}
