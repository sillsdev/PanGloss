//! Pins PC's `RealizationalRule` fix: these two fixtures move from `CompilesButMisses` to `OracleExact` now that `uflexc` emits a `RealizationalRule`'s allomorphs.

use pg_conformance_fixtures::discover;
use pg_foma::capability::CharacteristicKind;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::scoreboard::{self, CellOutcome};
use pg_foma::strategy_coverage::{representation_of, StrategyRepresentation};

const TARGET_FIXTURES: &[&str] = &[
    "machine:edge-cases/feature-gating-breadth",
    "machine:edge-cases/morphotactic-attribute-breadth",
];

#[test]
fn plan_composed_represents_realizational_morphology_now() {
    assert_eq!(
        representation_of(
            EmissionStrategy::PlanComposed,
            CharacteristicKind::RealizationalMorphology
        )
        .representation,
        StrategyRepresentation::Represents,
        "the published strategy-coverage fact must say PlanComposed represents \
         RealizationalMorphology, or the fixture-level assertions below rest on an over-claim"
    );
}

#[test]
fn realizational_fixtures_are_oracle_exact_under_plan_composed() {
    let fixtures = discover();
    let mut checked = 0usize;
    for fixture in &fixtures {
        let label = fixture.label();
        if !TARGET_FIXTURES.contains(&label.as_str()) {
            continue;
        }
        checked += 1;

        let grammar = pg_grammar::load(&fixture.load_grammar_xml())
            .unwrap_or_else(|e| panic!("{label}: grammar failed to load: {e}"));
        let words_yaml = fixture.load_words_yaml();
        let words: Vec<String> = words_yaml.words.iter().map(|w| w.word.clone()).collect();
        let row = scoreboard::measure(&label, &grammar, &words);
        let cell = row
            .cells
            .iter()
            .find(|c| c.strategy == EmissionStrategy::PlanComposed)
            .expect("PlanComposed is always in ALL_STRATEGIES");

        assert!(
            matches!(cell.outcome, CellOutcome::OracleExact),
            "{label} [PlanComposed]: expected OracleExact now that uflexc emits Realizational \
             allomorphs, got {:?} (certification: {})",
            cell.outcome,
            cell.certification_debug
        );
        let candidate_only = cell
            .divergence
            .as_ref()
            .map(|d| d.candidate_only_identities)
            .unwrap_or(0);
        assert_eq!(
            candidate_only, 0,
            "{label} [PlanComposed]: {candidate_only} candidate-only identities -- a surviving \
             over-generation, which ADR-0001 forbids"
        );
    }
    assert_eq!(
        checked,
        TARGET_FIXTURES.len(),
        "not every target fixture was discovered -- has one been renamed, moved, or removed?"
    );
}
