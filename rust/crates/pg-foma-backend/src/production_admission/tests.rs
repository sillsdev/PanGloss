use super::*;
use crate::health::FindingClass;

/// Minimal, delanguaged grammar: one char table, one entry, no rules.
const NO_PARTIAL_XML: &str = r#"<HermitCrabInput><Language><Name>NoPartialFixture</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <Strata>
        <Stratum characterDefinitionTable="t1">
          <Name>S</Name>
          <LexicalEntries>
            <LexicalEntry id="entry-plain">
              <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
            </LexicalEntry>
          </LexicalEntries>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

#[test]
fn a_grammar_with_no_partials_is_production_admissible_for_every_strategy() {
    let grammar = pg_grammar::load(NO_PARTIAL_XML).expect("fixture must load");
    for &strategy in crate::strategy_coverage::ALL_STRATEGIES {
        let admission = assess_completed_fst(&grammar, strategy).expect("valid grammar facts");
        assert_eq!(admission.strategy(), strategy);
        assert_eq!(admission.health().admission(), Severity::WithinLimits);
        assert!(!admission.blocks_publication());
        assert!(admission.health().findings.is_empty());
    }
}

#[test]
fn a_partial_entry_blocks_publication_as_readiness_only() {
    let mut grammar = pg_grammar::load(NO_PARTIAL_XML).expect("fixture must load");
    grammar.entries[0].partial_reason = Some(
        pg_grammar::model::PartialMorphemeReason::StemWithoutCategory,
    );
    for &strategy in crate::strategy_coverage::ALL_STRATEGIES {
        let admission = assess_completed_fst(&grammar, strategy).expect("valid grammar facts");
        assert_eq!(admission.health().admission(), Severity::NotProductionReady);
        let by_class = admission.health().admission_by_class();
        assert_eq!(by_class.readiness, Severity::NotProductionReady);
        assert_eq!(by_class.representability, Severity::WithinLimits);
        assert_eq!(by_class.containment, Severity::WithinLimits);
        assert_eq!(by_class.process, Severity::WithinLimits);
        assert!(admission.blocks_publication());
        let finding = &admission.health().findings[0];
        assert_eq!(finding.class(), FindingClass::Readiness);
        assert_eq!(finding.metric, Metric::PartialMorphemeCount);
        assert_eq!(finding.value, MetricValue::Count(1));
        assert_eq!(finding.provenance, ValueProvenance::Observed);
        assert_eq!(finding.affected, vec!["entry-plain".to_string()]);
    }
}

/// The diagnostic must separate the two kinds; one count cannot stand in for the other.
#[test]
fn the_diagnostic_names_both_kinds_separately() {
    let mut grammar = pg_grammar::load(NO_PARTIAL_XML).expect("fixture must load");
    grammar.entries[0].partial_reason = Some(
        pg_grammar::model::PartialMorphemeReason::StemWithoutCategory,
    );
    let admission = assess_completed_fst(&grammar, EmissionStrategy::TunedSurfaceProbed)
        .expect("valid grammar facts");
    let explanation = &admission.health().findings[0].explanation;
    assert!(
        explanation.contains("1 lexical entry/entries")
            && explanation.contains("0 affix-process rule(s)"),
        "explanation must count both kinds: {explanation}"
    );
}
