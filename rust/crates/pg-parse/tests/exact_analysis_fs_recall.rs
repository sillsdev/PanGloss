//! End-to-end pin for `pg-rules::morph::ana_syn_fs`'s "Exact" mode, porting `AnalysisSyntacticFeatureMergeTests.OverrideLoss_TenseFlipFlop_AddAndPriorityUnionLoseTheParse_ExactFindsIt` (research C#): `outermost`'s `Required=tense:past` can only see what `outer`'s `Out=tense:past` wrote if `inner`'s later un-application first strips it via `remove_paths`; old code left it in place and lost the parse. Deliberate, documented divergence from hc.dll master (still `Add`, PR #494/"Exact" unmerged); see `docs/research/pg-rules-analysis-syn-fs-gate-notes.md`.

#[path = "csharp_port_common/mod.rs"]
mod csharp_port_common;
use csharp_port_common::assert_morphs_eq;
use pg_parse::Morpher;

/// Root (POS=V, tense unspecified) + a 3-slot template: `inner` (Out=pres,"a"), `outer` (Out=past,"u"), `outermost` (Required=past, no Out, "i"); un-applied outermost, outer, inner in that order.
fn build_grammar() -> pg_grammar_model::model::Grammar {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ExactAnalysisFsRecall</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <HeadFeatures>
      <SymbolicFeature id="featTense">
        <Name>tense</Name>
        <Symbols>
          <Symbol id="symPres">pres</Symbol>
          <Symbol id="symPast">past</Symbol>
        </Symbols>
      </SymbolicFeature>
    </HeadFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cZ"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cU2"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name>
        <Segment segment="cZ" /><Segment segment="cU2" /><Segment segment="cD" /><Segment segment="cA" /><Segment segment="cI" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrInner"><Name>inner</Name><MorphemeId>inner</MorphemeId>
            <OutputHeadFeatures><FeatureValue feature="featTense" symbolValues="symPres" /></OutputHeadFeatures>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subInner">
                <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrOuter"><Name>outer</Name><MorphemeId>outer</MorphemeId>
            <OutputHeadFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></OutputHeadFeatures>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subOuter">
                <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>u</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrOutermost"><Name>outermost</Name><MorphemeId>outermost</MorphemeId>
            <RequiredHeadFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></RequiredHeadFeatures>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subOutermost">
                <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>i</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate requiredPartsOfSpeech="posV">
            <Name>tmpl</Name>
            <Slot morphologicalRules="mrInner"><Name>Sl1</Name></Slot>
            <Slot morphologicalRules="mrOuter"><Name>Sl2</Name></Slot>
            <Slot morphologicalRules="mrOutermost"><Name>Sl3</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="eRoot" partOfSpeech="posV"><MorphemeId>root</MorphemeId>
            <Allomorphs><Allomorph id="aRoot"><PhoneticShape>zud</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("grammar failed to load: {e}\n---\n{xml}"))
}

#[test]
fn root_plus_inner_plus_outer_plus_outermost_parses_under_exact() {
    let g = build_grammar();
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("zudaui"), &["root inner outer outermost"]);
}
