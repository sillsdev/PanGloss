//! THE `strategy_coverage` measurement join gate: build-breaking only on the sound direction (see `pg_foma::strategy_coverage_join`'s own doc), scoped to today's `CannotRepresent` rows only.

use std::collections::HashSet;

use pg_conformance_fixtures::{discover, FixtureRef, WordsYaml};
use pg_foma::capability::CharacteristicKind;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::strategy_coverage::{representation_of, StrategyRepresentation, ALL_STRATEGIES};
use pg_foma::strategy_coverage_join::{
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

/// Every `(strategy, kind)` row the table marks `CannotRepresent` today -- a view, not a copy.
fn cannot_represent_rows() -> Vec<(EmissionStrategy, CharacteristicKind)> {
    let mut rows = Vec::new();
    for &strategy in ALL_STRATEGIES {
        for &kind in CharacteristicKind::ALL {
            if representation_of(strategy, kind).representation
                == StrategyRepresentation::CannotRepresent
            {
                rows.push((strategy, kind));
            }
        }
    }
    rows
}

/// Today's ratcheted contradiction count (measured zero) -- see `docs/research/strategy-coverage-join-report.md`.
const CONTRADICTION_RATCHET: usize = 0;

#[test]
fn cannot_represent_rows_are_not_contradicted_by_measurement() {
    let rows = cannot_represent_rows();
    assert!(
        !rows.is_empty(),
        "the table has no CannotRepresent row at all -- this gate would be vacuous; if every \
         strategy now represents every kind, delete this gate rather than leaving it passing on \
         nothing"
    );

    let mut contradictions: Vec<String> = Vec::new();
    let mut agreed = 0usize;
    let mut no_evidence = 0usize;

    for (strategy, kind) in rows {
        let fixtures = exhibiting_fixtures(kind);
        let measured = measure_all(&fixtures, strategy);
        let (verdict, witnesses) =
            classify_with_witnesses(StrategyRepresentation::CannotRepresent, &measured);
        match verdict {
            JoinVerdict::Contradicted => contradictions.push(format!(
                "{strategy:?} x {kind:?}: table says CannotRepresent, but {witnesses:?} \
                 measured oracle-exact"
            )),
            JoinVerdict::Agreed => agreed += 1,
            JoinVerdict::NoEvidence => no_evidence += 1,
            JoinVerdict::Unsupported => unreachable!(
                "classify never returns Unsupported for a CannotRepresent representation"
            ),
        }
    }

    eprintln!(
        "=== strategy_coverage x measurement join (CannotRepresent rows only) ===\n\
         agreed={agreed} no_evidence={no_evidence} contradicted={}",
        contradictions.len()
    );
    for c in &contradictions {
        eprintln!("  CONTRADICTED: {c}");
    }

    assert!(
        contradictions.len() <= CONTRADICTION_RATCHET,
        "NEW CannotRepresent contradiction(s) beyond the {CONTRADICTION_RATCHET} ratcheted today: \
         {contradictions:#?}\n\
         Either the table is now wrong about a construct that measurably compiles (fix \
         `strategy_coverage.rs`'s row, citing the fixture below), or this ratchet needs raising \
         with the same citation. Never lower CONTRADICTION_RATCHET without also naming which \
         contradiction it now permits."
    );
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
