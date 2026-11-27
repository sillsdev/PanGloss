//! Pattern-root emission remains available independently of rewrite spelling refusals.

use pg_conformance_fixtures::discover;
use pg_foma::emit::eager_route_drops_root_spellings;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma_backend::scoreboard::{self, CellOutcome};

const TARGET_FIXTURES: &[&str] = &[
    "machine:languages/polysynthetic-stratal-derivation-chain",
    "staging:edge-cases/backend-strata-generic",
];

#[test]
fn pattern_root_route_survives_when_wide_rewrite_targets_are_isolated() {
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
        let Err(pg_foma::analyzer::FomaError::CapabilityRefused(diagnostics)) =
            pg_foma::analyzer::compile_proposer(&grammar)
        else {
            panic!("{label}: the original wide rewrite target must refuse");
        };
        assert!(diagnostics.iter().any(|d| d.refusal
            == pg_foma::capability::CapabilityRefusal::ProposalRecall(
                pg_foma::capability::ProposalRecallGap::AmbiguousSegmentSpelling
            )));
        let xml = fixture.load_grammar_xml();
        let target = r#"<PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighVowel" /><SimpleContext naturalClass="ncHighVowel" /></PhoneticSequence></PhoneticInput>"#;
        assert_eq!(xml.matches(target).count(), 2);
        let xml = xml.replace(target, r#"<PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHfuVowel" /><SimpleContext naturalClass="ncHfuVowel" /></PhoneticSequence></PhoneticInput>"#);
        assert!(
            xml.contains("<CompoundingRule id=\""),
            "{label}: the control must keep a compounding rule"
        );
        let unstripped = pg_grammar::load(&xml).expect("scalar vowel control must load");
        let Err(pg_foma::analyzer::FomaError::CapabilityRefused(refusals)) =
            pg_foma::analyzer::compile_proposer(&unstripped)
        else {
            panic!("{label}: the unstripped compound control must refuse");
        };
        assert!(
            refusals.iter().any(|d| d.refusal
                == pg_foma::capability::CapabilityRefusal::ProposalRecall(
                    pg_foma::capability::ProposalRecallGap::CompoundJunctionPhonology
                )),
            "{label}: expected the junction phonology refusal, got {refusals:?}"
        );
        let mut grammar =
            pg_grammar::load(&pg_conformance_fixtures::grammar_xml_without_compounding_rules(&xml))
                .expect("scalar vowel control must load");
        pg_conformance_fixtures::drop_segment_deletions(&mut grammar);
        assert!(
            pg_foma::analyzer::compile_proposer(&grammar).is_ok(),
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
