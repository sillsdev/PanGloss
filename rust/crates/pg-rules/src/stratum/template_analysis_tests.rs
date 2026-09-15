use super::*;
use pg_featstruct::FeatureStruct;
use pg_grammar::model::TemplateId;

const XML: &str = r#"
<HermitCrabInput>
  <Language>
    <Name>TemplateAnalysisGate</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name>
        <Segment segment="cM" /><Segment segment="cI" /><Segment segment="cD" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrD" requiredPartsOfSpeech="posV" outputPartOfSpeech="posN">
            <Name>d suffix</Name><MorphemeId>D</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subD">
                <MorphologicalInput>
                  <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate requiredPartsOfSpeech="posV">
            <Name>optional V template</Name>
            <Slot morphologicalRules="mrD" optional="true"><Name>suffix</Name></Slot>
          </AffixTemplate>
          <AffixTemplate requiredPartsOfSpeech="posN">
            <Name>consuming N template</Name>
            <Slot morphologicalRules="mrD"><Name>suffix</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="eN" partOfSpeech="posN"><MorphemeId>N</MorphemeId>
            <Allomorphs><Allomorph id="aN"><PhoneticShape>mi</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
          <LexicalEntry id="eV" partOfSpeech="posV"><MorphemeId>V</MorphemeId>
            <Allomorphs><Allomorph id="aV"><PhoneticShape>mi</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn grammar() -> Grammar {
    pg_grammar::load(XML).expect("template-analysis grammar loads")
}

fn word(g: &Grammar, text: &str) -> Word {
    let shape = pg_grammar::segment::segment(&g.char_tables[0], text).expect("word segments");
    Word::new(shape, StratumId(0))
}

fn analyze_template(g: &Grammar, template: TemplateId, input: &Word) -> Vec<Word> {
    let budget = StepBudget::new(usize::MAX);
    StratumAnalyzer::new(
        g,
        StratumId(0),
        AnalyzerConfig::default(),
        None,
        None,
        None,
        None,
        &budget,
        FinalTemplateAnalysisPolicy::default(),
        None,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    )
    .analyze_template(template, input)
}

#[test]
fn optional_template_absent_suffix_does_not_add_template_requirement() {
    let g = grammar();
    let input = word(&g, "mi");
    let output = analyze_template(&g, TemplateId(0), &input);

    assert_eq!(output.len(), 1);
    assert_eq!(output[0].shape, input.shape);
    assert_eq!(output[0].syn_fs, FeatureStruct::EMPTY);
}

#[test]
fn disjoint_template_input_is_rejected() {
    let g = grammar();
    let mut input = word(&g, "mi");
    input.syn_fs = g.fs_interner.get(g.entries[0].syn_fs).clone();

    let output = analyze_template(&g, TemplateId(0), &input);

    assert!(output.is_empty());
}

#[test]
fn consumed_slot_preserves_rule_analysis_features() {
    let g = grammar();
    let mut input = word(&g, "mid");
    input.syn_fs = g.fs_interner.get(g.entries[0].syn_fs).clone();
    let direct = morph::analyze(&g, &input, &g.mrules[0]);
    let output = analyze_template(&g, TemplateId(1), &input);

    assert_eq!(direct.len(), 1);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].shape, direct[0].shape);
    assert_eq!(direct[0].syn_fs, g.fs_interner.get(g.entries[1].syn_fs).clone());
    assert_eq!(output[0].syn_fs, g.fs_interner.get(g.entries[1].syn_fs).clone());
}
