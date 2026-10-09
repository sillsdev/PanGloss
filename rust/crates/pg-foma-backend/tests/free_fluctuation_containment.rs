//! TunedSurfaceProbed containment for C#'s disjunctive allomorph re-check and its free-fluctuation escape.

use pg_conformance_fixtures::{assert_matches_oracle, discover};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma_backend::scoreboard::{self, CellOutcome};

const FIXTURE: &str = "machine:edge-cases/disjunctive-recheck";
const WORDS: &[&str] = &["gray", "grey", "wakta", "pakda"];

#[test]
fn disjunctive_recheck_proposes_and_confirms_free_fluctuating_allomorphs() {
    let fixture = discover()
        .into_iter()
        .find(|fixture| fixture.label() == FIXTURE)
        .unwrap_or_else(|| panic!("{FIXTURE} not discovered"));
    let grammar = pg_grammar::load(&fixture.load_grammar_xml())
        .unwrap_or_else(|error| panic!("{FIXTURE}: {error}"));
    let expected = fixture.load_words_yaml();

    assert_eq!(
        assert_matches_oracle(
            FIXTURE,
            &expected,
            &pg_parse::Morpher::new(&grammar, usize::MAX)
        ),
        expected.words.len(),
        "every recorded disjunctive-recheck word must match the founding oracle"
    );

    let words: Vec<String> = WORDS.iter().map(|word| (*word).to_string()).collect();
    for word in WORDS {
        assert!(
            expected.words.iter().any(|record| record.word == *word),
            "{word} is part of this fixture's committed C# record"
        );
    }

    let measured = scoreboard::measure(FIXTURE, &grammar, &words);
    let cell = measured
        .cells
        .iter()
        .find(|cell| cell.strategy == EmissionStrategy::TunedSurfaceProbed)
        .expect("TunedSurfaceProbed is measured for every grammar");
    assert_eq!(
        cell.outcome,
        CellOutcome::OracleExact,
        "TunedSurfaceProbed must confirm the exact recorded gray/grey and disjunctive-control \
         identities; got {:?}",
        cell.outcome
    );
    let divergence = cell
        .divergence
        .as_ref()
        .expect("an exact proposal-confirmation result has per-word evidence");
    assert_eq!(divergence.oracle_only_identities, 0);
    assert_eq!(divergence.candidate_only_identities, 0);
    assert_eq!(cell.words_measured, Some(WORDS.len()));
}
