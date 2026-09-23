//! Unit tests rather than integration ones: `is_word_valid_traced` never reads
//! `Grammar::strata`, so a hand-built `Word` against a zero-stratum grammar drives the gate
//! directly, where a natural repro would need a multi-stratum/template scenario.

use super::*;
use pg_rules::trace::{FailureReason, TraceType, TreeTraceSink};
use pg_shape::ShapeBuilder;

/// The smallest grammar `pg_grammar::load` accepts; sufficient since `is_word_valid_traced` doesn't read `Grammar::strata`.
fn minimal_grammar() -> pg_grammar_model::model::Grammar {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
          <HeadFeatures />
          <MorphologicalPhonologicalRuleFeatures>
            <MorphologicalPhonologicalRuleFeature id="mprA">Alpha</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeatureGroup features="mprA"><Name>G</Name></MorphologicalPhonologicalRuleFeatureGroup>
          </MorphologicalPhonologicalRuleFeatures>
          <CharacterDefinitionTable id="t1">
            <Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="cA" /></SegmentNaturalClass></NaturalClasses>
        </Language></HermitCrabInput>"#;
    pg_grammar::load(XML).unwrap_or_else(|e| panic!("minimal_grammar failed to load: {e}"))
}

fn w() -> Word {
    Word::new(ShapeBuilder::new().finish(), StratumId(0))
}

#[test]
fn partial_parse_is_reported_when_an_unapplied_rule_never_confirms() {
    let g = minimal_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let mut word = w();
    // A leftover unapplied rule never re-confirmed by synthesis (C#'s `mruleAppIndex != -1`).
    word.mrule_apps = vec![Some(pg_grammar_model::model::MRuleId(0))];
    word.mrule_app_index = 0;

    let sink = TreeTraceSink::new();
    let root = sink.analyze_word(&word);
    let ok = m.is_word_valid_traced(&word, &sink, root);
    assert!(!ok);

    let child = *sink
        .node(root)
        .children
        .first()
        .expect("Failed must be appended under root");
    assert_eq!(sink.node(child).type_, TraceType::Failed);
    assert_eq!(
        sink.node(child).failure_reason,
        Some(FailureReason::PartialParse)
    );
}

#[test]
fn valid_word_with_no_pending_rules_and_no_obligatory_features_passes() {
    let g = minimal_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let word = w(); // mrule_app_index == -1, obligatory empty, no morphs.

    let sink = TreeTraceSink::new();
    let root = sink.analyze_word(&word);
    let ok = m.is_word_valid_traced(&word, &sink, root);
    assert!(ok, "a fresh Word with nothing pending must be valid");
    assert!(
        sink.node(root).children.is_empty(),
        "no Failed event should fire for a valid word"
    );
}

#[test]
fn noop_sink_path_is_unaffected_by_is_word_valid_traced() {
    // Confirms the untraced `is_word_valid` wrapper (NoopSink + DUMMY handle) shares one implementation with the traced path.
    let g = minimal_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let mut word = w();
    word.mrule_apps = vec![Some(pg_grammar_model::model::MRuleId(0))];
    word.mrule_app_index = 0;
    assert!(!m.is_word_valid(&word));
}
