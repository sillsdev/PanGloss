//! Two part-of-speech-gated subrules of one rule must both stay reachable when a backend cannot partition roots by part of speech.

use pg_foma::enumerate::EmissionStrategy;
use pg_foma_backend::scoreboard::{self, CellOutcome};

#[test]
fn both_fst_backends_propose_the_verb_analysis_under_a_pos_gated_narrowing_rule() {
    let fixture =
        pg_conformance_fixtures::require_fixture("edge-cases", "merge-subrules-pos-rtl-template");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture grammar loads");
    let words: Vec<String> = ["F", "Gs", "FF"].map(String::from).to_vec();
    let scored = scoreboard::measure(&fixture.label(), &grammar, &words);
    for strategy in [
        EmissionStrategy::TunedSurfaceProbed,
        EmissionStrategy::TemplatedUnderlyingTokens,
    ] {
        let cell = scored
            .cells
            .iter()
            .find(|c| c.strategy == strategy)
            .expect("every strategy is measured");
        assert_eq!(
            cell.outcome,
            CellOutcome::OracleExact,
            "{strategy:?} must find TA_V+S for Gs"
        );
    }
}
