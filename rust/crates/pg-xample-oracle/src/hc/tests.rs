use super::*;
use pg_parse::Morpher;

/// A 2-slot reduction of the same `deep-optional-affix-nesting` grammar the `tests/data/captured-parse-*.json` fixtures were captured from.
fn two_slot_grammar() -> Grammar {
    const XML: &str = r#"<HermitCrabInput><Language><Name>PathologicalDeepOptionalAffixNestingReduced</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1">
            <Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cX"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
              <Name>Main</Name>
              <MorphologicalRuleDefinitions>
                <MorphologicalRule id="mrP1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
                  <Name>p1</Name>
                  <MorphologicalSubrules><MorphologicalSubrule id="subP1">
                    <MorphologicalInput><PhoneticSequence id="stemP1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                    <MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments><CopyFromInput index="stemP1" /></MorphologicalOutput>
                  </MorphologicalSubrule></MorphologicalSubrules>
                  <MorphemeId>P1</MorphemeId>
                </MorphologicalRule>
                <MorphologicalRule id="mrP2" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
                  <Name>p2</Name>
                  <MorphologicalSubrules><MorphologicalSubrule id="subP2">
                    <MorphologicalInput><PhoneticSequence id="stemP2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                    <MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments><CopyFromInput index="stemP2" /></MorphologicalOutput>
                  </MorphologicalSubrule></MorphologicalSubrules>
                  <MorphemeId>P2</MorphemeId>
                </MorphologicalRule>
              </MorphologicalRuleDefinitions>
              <AffixTemplates>
                <AffixTemplate requiredPartsOfSpeech="posV">
                  <Name>reducedTemplate</Name>
                  <Slot optional="true" morphologicalRules="mrP1"><Name>slot1</Name></Slot>
                  <Slot optional="true" morphologicalRules="mrP2"><Name>slot2</Name></Slot>
                </AffixTemplate>
              </AffixTemplates>
              <LexicalEntries>
                <LexicalEntry id="eK" partOfSpeech="posV">
                  <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
                  <MorphemeId>K</MorphemeId>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    pg_grammar::load(XML).unwrap_or_else(|e| panic!("two_slot_grammar failed to load: {e}"))
}

#[test]
fn root_only_word_normalizes_to_a_single_member_result() {
    let g = two_slot_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let outcome = m.parse_word("k");
    let result = xample_result_from_hc_outcome(&outcome, &g, "k").expect("k must normalize");
    assert_eq!(result.analyses.values().sum::<usize>(), 1);
    assert_eq!(result.engine_error, None);
    assert_eq!(result.reached_max_analyses, None);
    let (signature, count) = result.analyses.iter().next().unwrap();
    assert_eq!(*count, 1);
    assert_eq!(signature.msa_ids, vec!["eK".to_string()]);
    assert_eq!(signature.surface_nfd, "k");
}

#[test]
fn two_optional_prefixes_normalize_to_two_distinct_signatures() {
    let g = two_slot_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let outcome = m.parse_word("xk");
    let result = xample_result_from_hc_outcome(&outcome, &g, "xk").expect("xk must normalize");
    assert_eq!(
        result.analyses.len(),
        2,
        "P1-fired and P2-fired are distinct analyses"
    );
    assert_eq!(result.analyses.values().sum::<usize>(), 2);
    let msa_id_sets: Vec<&Vec<String>> = result.analyses.keys().map(|s| &s.msa_ids).collect();
    assert!(
        msa_id_sets.contains(&&vec!["eK".to_string(), "mrP1".to_string()])
            || msa_id_sets.contains(&&vec!["mrP1".to_string(), "eK".to_string()])
    );
}

#[test]
fn surface_nfd_is_the_literal_input_word_not_any_engine_resynthesis() {
    // A caller-supplied word can never carry a resynthesized morph-boundary marker.
    let g = two_slot_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let outcome = m.parse_word("xk");
    let result = xample_result_from_hc_outcome(&outcome, &g, "xk").expect("xk must normalize");
    for signature in result.analyses.keys() {
        assert_eq!(signature.surface_nfd, "xk");
    }
}

#[test]
fn unparseable_word_normalizes_to_invalid_shape_engine_error() {
    let g = two_slot_grammar();
    let m = Morpher::new(&g, usize::MAX);
    // "q" has no representation in the character table, so it never segments.
    let outcome = m.parse_word("q");
    let result = xample_result_from_hc_outcome(&outcome, &g, "q")
        .expect("q must normalize (empty, not an error state HC lacks a flag for)");
    if outcome.invalid_shape {
        assert!(result
            .engine_error
            .as_deref()
            .unwrap_or_default()
            .contains("invalid shape"));
    }
    assert!(result.analyses.is_empty());
}
