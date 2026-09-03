//! Pins G7 question (a): a pattern root's regex route now compiles and certifies these two fixtures oracle-exact under `TunedSurfaceProbed`, which previously refused both with zero accepted backends.

use pg_conformance_fixtures::discover;
use pg_foma::analyzer::FomaProposer;
use pg_foma::emit::eager_route_drops_root_spellings;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::scoreboard::{self, CellOutcome};

const TARGET_FIXTURES: &[&str] = &[
    "machine:languages/polysynthetic-stratal-derivation-chain",
    "staging:edge-cases/backend-strata-generic",
];

#[test]
fn pattern_root_shapes_no_longer_refuse_tuned_surface_probed() {
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

        assert!(
            !eager_route_drops_root_spellings(&grammar),
            "{label}: eager_route_drops_root_spellings still claims a drop -- the regex route \
             should have retired this shape's Unbounded verdict as a drop"
        );
        assert!(
            FomaProposer::new(&grammar).is_ok(),
            "{label}: TunedSurfaceProbed still refuses to compile"
        );

        let words_yaml = fixture.load_words_yaml();
        let words: Vec<String> = words_yaml.words.iter().map(|w| w.word.clone()).collect();
        let row = scoreboard::measure(&label, &grammar, &words);
        let cell = row
            .cells
            .iter()
            .find(|c| c.strategy == EmissionStrategy::TunedSurfaceProbed)
            .expect("TunedSurfaceProbed is always in ALL_STRATEGIES");

        assert!(
            matches!(cell.outcome, CellOutcome::OracleExact),
            "{label} [TunedSurfaceProbed]: expected OracleExact, got {:?} (certification: {})",
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
            "{label} [TunedSurfaceProbed]: {candidate_only} candidate-only identities -- a \
             surviving over-generation, which ADR-0001 forbids"
        );
    }
    assert_eq!(
        checked,
        TARGET_FIXTURES.len(),
        "not every target fixture was discovered -- has one been renamed, moved, or removed?"
    );
}
