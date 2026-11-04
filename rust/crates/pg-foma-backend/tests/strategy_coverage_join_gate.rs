//! THE `strategy_coverage` measurement join gate: build-breaking only on the sound direction (see `pg_foma::strategy_coverage_join`'s own doc), scoped to the can-fire proof for a `CannotRepresent` claim; the table holds no such row today, so the per-row gate was deleted with the last one.

use std::collections::HashSet;

use pg_conformance_fixtures::{discover, FixtureRef, WordsYaml};
use pg_foma::capability::CharacteristicKind;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::strategy_coverage::{representation_of, StrategyRepresentation};
use pg_foma_backend::strategy_coverage_join::{
    classify_with_witnesses, kinds_exercised_by, measure_fixture_exact, FixtureExactness,
    JoinVerdict,
};

/// Every `constructs.txt` id any word/parse in `words_yaml` names.
fn exercised_ids(words_yaml: &WordsYaml) -> HashSet<&str> {
    let mut ids = HashSet::new();
    for w in &words_yaml.words {
        ids.extend(w.exercises.iter().map(String::as_str));
        for p in &w.parses {
            ids.extend(p.exercises.iter().map(String::as_str));
        }
    }
    ids
}

/// Every discovered fixture whose authored `exercises:` tags put it in `kind`'s reverse mapping.
fn exhibiting_fixtures(kind: CharacteristicKind) -> Vec<FixtureRef> {
    discover()
        .into_iter()
        .filter(|f| kinds_exercised_by(&exercised_ids(&f.load_words_yaml())).contains(&kind))
        .collect()
}

/// Measures every fixture in `fixtures` on `strategy`; an unmeasurable one is simply omitted.
fn measure_all(fixtures: &[FixtureRef], strategy: EmissionStrategy) -> Vec<FixtureExactness> {
    let mut out = Vec::new();
    for f in fixtures {
        let Ok(grammar) = pg_grammar::load(&f.load_grammar_xml()) else {
            continue;
        };
        if grammar.char_tables.is_empty() {
            continue;
        }
        let words: Vec<String> = f
            .load_words_yaml()
            .words
            .iter()
            .map(|w| w.word.clone())
            .collect();
        if words.is_empty() {
            continue;
        }
        out.push(FixtureExactness {
            label: f.label(),
            exact: measure_fixture_exact(&grammar, &words, strategy),
        });
    }
    out
}

/// Can-fire proof: a synthetic wrong `CannotRepresent` claim over a real exact fixture must be caught and named.
#[test]
fn a_synthetic_cannot_represent_claim_is_contradicted_by_a_real_exact_fixture() {
    // A claim the real table never makes today (TunedSurfaceProbed represents every kind), so this injects a known-wrong entry without touching strategy_coverage.rs.
    let kind = CharacteristicKind::RealizationalMorphology;
    let strategy = EmissionStrategy::TunedSurfaceProbed;
    assert_eq!(
        representation_of(strategy, kind).representation,
        StrategyRepresentation::Represents,
        "the real table must actually claim Represents here, or this synthetic-wrong-entry test \
         is no longer synthetic"
    );

    let fixtures = exhibiting_fixtures(kind);
    assert!(
        !fixtures.is_empty(),
        "no fixture exercises RealizationalMorphology at all -- this can-fire test needs at least \
         one to inject a real exact witness"
    );
    let measured = measure_all(&fixtures, strategy);
    assert!(
        measured.iter().any(|m| m.exact),
        "no fixture measured oracle-exact for {strategy:?} x {kind:?} -- this can-fire test needs \
         a real positive witness to prove contradiction detection actually fires: {measured:?}"
    );

    let (verdict, witnesses) =
        classify_with_witnesses(StrategyRepresentation::CannotRepresent, &measured);
    assert_eq!(
        verdict,
        JoinVerdict::Contradicted,
        "injecting CannotRepresent over a real exact witness must be caught, not waved through"
    );
    assert!(
        witnesses
            .iter()
            .any(|w| measured.iter().any(|m| m.exact && &m.label == w)),
        "the reported witness(es) {witnesses:?} must actually be among the exact fixtures \
         measured: {measured:?}"
    );
    eprintln!(
        "can-fire OK: synthetic CannotRepresent({strategy:?}, {kind:?}) named {witnesses:?} as \
         the contradicting fixture(s)"
    );
}
