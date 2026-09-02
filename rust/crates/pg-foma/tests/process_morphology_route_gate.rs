//! A Role::Process standalone rule (pure `ModifyFromInput`) must compile oracle-exact under `TunedSurfaceProbed` once `build_structural_composites` genuinely routes it.

use pg_conformance_fixtures::discover;
use pg_foma::analyzer::FomaProposer;
use pg_foma::backend_selection::select_backends;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::scoreboard::{self, CellOutcome};
use pg_grammar::model::Grammar;

const FIXTURE: &str = "machine:edge-cases/process-morphology-in-place-mutation";

fn grammar() -> Grammar {
    let fixture = discover()
        .into_iter()
        .find(|f| f.label() == FIXTURE)
        .unwrap_or_else(|| panic!("{FIXTURE} not discovered"));
    pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture grammar must load")
}

#[test]
fn tsp_admits_and_certifies_the_pure_ablaut_rule() {
    let g = grammar();
    let semantics = GrammarSemantics::derive(&g);
    let selection = select_backends(&semantics);
    let report = selection.report_for(EmissionStrategy::TunedSurfaceProbed);
    assert!(
        report.is_some_and(|r| r.can_represent()),
        "TSP must admit a Modify-only Role::Process standalone rule once \
         build_structural_composites genuinely routes it -- got {report:?}"
    );

    let proposer = FomaProposer::new(&g);
    assert!(
        proposer.is_ok(),
        "TSP must compile {FIXTURE}, got {:?}",
        proposer.err()
    );

    let fixture = discover().into_iter().find(|f| f.label() == FIXTURE).unwrap();
    let words: Vec<String> = fixture
        .load_words_yaml()
        .words
        .iter()
        .map(|w| w.word.clone())
        .collect();
    let scored = scoreboard::measure(FIXTURE, &g, &words);
    let cell = scored
        .cells
        .iter()
        .find(|c| c.strategy == EmissionStrategy::TunedSurfaceProbed)
        .expect("TSP is one of ALL_STRATEGIES");
    assert_eq!(
        cell.outcome,
        CellOutcome::OracleExact,
        "expected TSP oracle-exact for {FIXTURE}, got {:?}",
        cell.outcome
    );
    if let Some(divergence) = &cell.divergence {
        assert_eq!(
            divergence.candidate_only_identities, 0,
            "soundness violation: an over-generated candidate survived confirm for {FIXTURE}"
        );
    }
}
