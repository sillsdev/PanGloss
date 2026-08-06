//! Ports `AffixTemplateTests` (`SIL.Machine.Morphology.HermitCrab.Tests`). Grammar/lexicon shared via `csharp_port_common`; every test drives `pg_parse::Morpher::parse_word` end-to-end over an XML-loaded grammar, matching each C# test's own `ParseWord` calls, with expected values transcribed verbatim from the C# source's `AssertMorphsEqual` literals.

mod csharp_port_common;
use csharp_port_common::{assert_empty, assert_morphs_eq};
use pg_parse::Morpher;

/// Ports `AffixTemplateTests.NonFinalTemplate`: two configurations of the same grammar toggling `verbTemplate.IsFinal`, with `ed_suffix` picking its allomorph by phonological context.
#[test]
fn non_final_template() {
    let mrules_final = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd1">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
              <PhoneticSequence id="2"><SimpleContext naturalClass="ncAlvStop2" /></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput>
              <CopyFromInput index="1" /><CopyFromInput index="2" /><InsertSegments><PhoneticShape>+ɯd</PhoneticShape></InsertSegments>
            </MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subEd2">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence><SimpleContext naturalClass="ncVlCons" /></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput>
              <CopyFromInput index="1" /><InsertSegments><PhoneticShape>+t</PhoneticShape></InsertSegments>
            </MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subEd3">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput>
              <CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments>
            </MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrNom" requiredPartsOfSpeech="posV" outputPartOfSpeech="posN"><Name>nominalizer</Name><MorphemeId>NOM</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subNom">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>v</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrS" requiredPartsOfSpeech="posN"><Name>s_suffix</Name><MorphemeId>PL</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subS">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <CompoundingRule id="mrCompound" headPartsOfSpeech="posV" nonHeadPartsOfSpeech="posN" outputPartOfSpeech="posN">
        <Name>rule1</Name>
        <CompoundingSubrules>
          <CompoundingSubrule>
            <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
            <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
            <MorphologicalOutput>
              <CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" />
            </MorphologicalOutput>
          </CompoundingSubrule>
        </CompoundingSubrules>
      </CompoundingRule>
    "#;

    // `ed_suffix`/`s_suffix` are template-slot-only in C#; only `nominalizer`/`rule1` are ordinary cascade rules, hence `mrNom mrCompound` here, not `mrEd`/`mrS`.
    let templates_final = r#"
      <AffixTemplate requiredPartsOfSpeech="posV"><Name>verb</Name><Slot morphologicalRules="mrEd"><Name>Sl1</Name></Slot></AffixTemplate>
      <AffixTemplate requiredPartsOfSpeech="posN"><Name>noun</Name><Slot morphologicalRules="mrS" optional="true"><Name>Sl2</Name></Slot></AffixTemplate>
    "#;
    let g = csharp_port_common::build_grammar(
        "",
        "",
        mrules_final,
        "mrNom mrCompound",
        templates_final,
    );
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("sagd"), &["32 PAST"]);
    assert_empty(&m.parse_word("sagdv"));
    assert_empty(&m.parse_word("sagdvs"));
    assert_empty(&m.parse_word("sagdmi"));
    assert_empty(&m.parse_word("sagdmis"));

    let templates_nonfinal = r#"
      <AffixTemplate requiredPartsOfSpeech="posV" final="false"><Name>verb</Name><Slot morphologicalRules="mrEd"><Name>Sl1</Name></Slot></AffixTemplate>
      <AffixTemplate requiredPartsOfSpeech="posN"><Name>noun</Name><Slot morphologicalRules="mrS" optional="true"><Name>Sl2</Name></Slot></AffixTemplate>
    "#;
    let g2 = csharp_port_common::build_grammar(
        "",
        "",
        mrules_final,
        "mrNom mrCompound",
        templates_nonfinal,
    );
    let m2 = Morpher::new(&g2, usize::MAX);
    assert_empty(&m2.parse_word("sagd"));
    assert_morphs_eq(&m2.parse_word("sagdv"), &["32 PAST NOM"]);
    assert_morphs_eq(&m2.parse_word("sagdvs"), &["32 PAST NOM PL"]);
    assert_morphs_eq(&m2.parse_word("sagdmi"), &["32 PAST 53"]);
    assert_morphs_eq(&m2.parse_word("sagdmis"), &["32 PAST 53 PL"]);
}

/// Ports `AffixTemplateTests.AffixTemplateAppliedAfterMorphologicalRule`: an ordinary V->N `nominalizer` rule feeds a `noun` template's `s_suffix` slot.
#[test]
fn affix_template_applied_after_morphological_rule() {
    let mrules = r#"
      <MorphologicalRule id="mrNom" requiredPartsOfSpeech="posV" outputPartOfSpeech="posN"><Name>nominalizer</Name><MorphemeId>NOM</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subNom">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>v</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrS" requiredPartsOfSpeech="posN"><Name>s_suffix</Name><MorphemeId>PL</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subS">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let templates = r#"
      <AffixTemplate requiredPartsOfSpeech="posN"><Name>noun</Name><Slot morphologicalRules="mrS" optional="true"><Name>Sl1</Name></Slot></AffixTemplate>
    "#;
    let g = csharp_port_common::build_grammar("", "", mrules, "mrNom", templates);
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("sagv"), &["32 NOM"]);
    assert_morphs_eq(&m.parse_word("sagvs"), &["32 NOM PL"]);
}

/// Ports `AffixTemplateTests.SameRuleUsedInMultipleTemplates`: `ed_suffix` is referenced by both the TV and IV templates, and a nominalizer feeds the IV template's slot.
#[test]
fn same_rule_used_in_multiple_templates() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV posTv posIv"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrIverb" requiredPartsOfSpeech="posN" outputPartOfSpeech="posIv"><Name>intransitive verbalizer</Name><MorphemeId>IVERB</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subIverb">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>v</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let templates = r#"
      <AffixTemplate requiredPartsOfSpeech="posTv"><Name>Transitive Verb</Name><Slot morphologicalRules="mrEd"><Name>Sl1</Name></Slot></AffixTemplate>
      <AffixTemplate requiredPartsOfSpeech="posIv"><Name>Intransitive Verb</Name><Slot morphologicalRules="mrEd"><Name>Sl2</Name></Slot></AffixTemplate>
    "#;
    // posTv/posIv are declared locally (the shared fixture only declares N/V/A).
    let g = build_grammar_with_tv_iv(mrules, templates);
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("mivd"), &["53 IVERB PAST"]);
}

/// The shared common lexicon lacks entry "53" and the TV/IV parts of speech this test needs, and is small enough that a bespoke tiny grammar is clearer than growing the shared fixture for it.
fn build_grammar_with_tv_iv(mrules_xml: &str, templates_xml: &str) -> pg_grammar::model::Grammar {
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>SameRuleMultiTemplate</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
      <PartOfSpeech id="posTv"><Name>TV</Name></PartOfSpeech>
      <PartOfSpeech id="posIv"><Name>IV</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cV"><Representations><Representation>v</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name>
        <Segment segment="cM" /><Segment segment="cI" /><Segment segment="cV" /><Segment segment="cD" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrIverb">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>{mrules_xml}</MorphologicalRuleDefinitions>
        <AffixTemplates>{templates_xml}</AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="e53" partOfSpeech="posN"><MorphemeId>53</MorphemeId>
            <Allomorphs><Allomorph id="a53"><PhoneticShape>mi</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
    );
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("grammar failed to load: {e}\n---\n{xml}"))
}

// --- AffixTemplateTests.RealizationalRule ---

/// The three realizational rules of `AffixTemplateTests.RealizationalRule`; `evid_features` is a substitution point because the C# test flips `evidential`'s feature struct mid-test and rebuilds the `Morpher` (two grammars in this port). Natural-class stand-ins: `ncAlvStop2`=`alvStop`, `ncVlCons`=`voicelessCons`, `ncLabC`=`labiodental`, `ncVoiced`=`voiced`, `ncStrident`=`strident`.
fn realizational_rule_mrules(evid_features: &str) -> String {
    format!(
        r#"
      <RealizationalRule id="rrEd"><Name>ed_suffix</Name>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subREd1">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
              <PhoneticSequence id="2"><SimpleContext naturalClass="ncAlvStop2" /></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><CopyFromInput index="2" /><InsertSegments><PhoneticShape>ɯd</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subREd2">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence><SimpleContext naturalClass="ncVlCons" /></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>t</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subREd3">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
        <RealizationalFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></RealizationalFeatures>
        <MorphemeId>PAST</MorphemeId>
      </RealizationalRule>
      <RealizationalRule id="rrS"><Name>s_suffix</Name>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subRS1">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
              <PhoneticSequence id="2"><SimpleContext naturalClass="ncLabC" /></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput>
              <CopyFromInput index="1" /><ModifyFromInput index="2"><SimpleContext naturalClass="ncVoiced" /></ModifyFromInput><InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments>
            </MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subRS2">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence><SimpleContext naturalClass="ncStrident" /></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>ɯz</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subRS3">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
              <PhoneticSequence id="2"><SimpleContext naturalClass="ncVlCons" /></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><CopyFromInput index="2" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subRS4">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
        <RealizationalFeatures>
          <FeatureValue feature="featPers" symbolValues="symP3" />
          <FeatureValue feature="featTense" symbolValues="symPres" />
        </RealizationalFeatures>
        <MorphemeId>3SG</MorphemeId>
      </RealizationalRule>
      <RealizationalRule id="rrWit"><Name>evidential</Name>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subRWit">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>v</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
        <RealizationalFeatures>{evid_features}</RealizationalFeatures>
        <MorphemeId>WIT</MorphemeId>
      </RealizationalRule>
    "#
    )
}

/// The `SEE` family roots, absent from the shared lexicon: `bl1` = "si" (V), `bl2` = "sau" (V, tense=past), `bl3` = "sis" (V, tense=pres).
const SEE_FAMILY_LEXICON: &str = r#"
  <LexicalEntry id="eBl1" partOfSpeech="posV" family="famSee">
    <Allomorphs><Allomorph id="aBl1"><PhoneticShape>si</PhoneticShape></Allomorph></Allomorphs>
    <MorphemeId>bl1</MorphemeId>
  </LexicalEntry>
  <LexicalEntry id="eBl2" partOfSpeech="posV" family="famSee">
    <Allomorphs><Allomorph id="aBl2"><PhoneticShape>sau</PhoneticShape></Allomorph></Allomorphs>
    <AssignedHeadFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></AssignedHeadFeatures>
    <MorphemeId>bl2</MorphemeId>
  </LexicalEntry>
  <LexicalEntry id="eBl3" partOfSpeech="posV" family="famSee">
    <Allomorphs><Allomorph id="aBl3"><PhoneticShape>sis</PhoneticShape></Allomorph></Allomorphs>
    <AssignedHeadFeatures><FeatureValue feature="featTense" symbolValues="symPres" /></AssignedHeadFeatures>
    <MorphemeId>bl3</MorphemeId>
  </LexicalEntry>
"#;

/// Ports `AffixTemplateTests.RealizationalRule` in a two-optional-slot `verb` template with no ordinary cascade rules.
/// See `docs/research/csharp-port-realizational-rule.md` for what each assertion pins down, including the family-blocking and realizational-unify cases.
#[test]
fn realizational_rule() {
    let templates = r#"
      <AffixTemplate requiredPartsOfSpeech="posV"><Name>verb</Name>
        <Slot morphologicalRules="rrS rrEd" optional="true"><Name>Sl1</Name></Slot>
        <Slot morphologicalRules="rrWit" optional="true"><Name>Sl2</Name></Slot>
      </AffixTemplate>
    "#;

    let mrules1 =
        realizational_rule_mrules(r#"<FeatureValue feature="featEvid" symbolValues="symWit" />"#);
    let g1 = csharp_port_common::build_grammar_w5(
        "",
        r#"<Families><Family id="famSee">SEE</Family></Families>"#,
        &mrules1,
        "",
        templates,
        SEE_FAMILY_LEXICON,
    );
    let m1 = Morpher::new(&g1, usize::MAX);
    assert_morphs_eq(&m1.parse_word("sagd"), &["32 PAST"]);
    assert_morphs_eq(&m1.parse_word("sagdv"), &["32 PAST WIT"]);
    assert_empty(&m1.parse_word("sid"));
    assert_morphs_eq(&m1.parse_word("sau"), &["bl2"]);

    // cs:194-221: evidential's realizational FS gains tense=pres; rebuild and parse "sagzv".
    let mrules2 = realizational_rule_mrules(
        r#"<FeatureValue feature="featEvid" symbolValues="symWit" /><FeatureValue feature="featTense" symbolValues="symPres" />"#,
    );
    let g2 = csharp_port_common::build_grammar_w5(
        "",
        r#"<Families><Family id="famSee">SEE</Family></Families>"#,
        &mrules2,
        "",
        templates,
        SEE_FAMILY_LEXICON,
    );
    let m2 = Morpher::new(&g2, usize::MAX);
    assert_morphs_eq(&m2.parse_word("sagzv"), &["32 3SG WIT"]);
}
