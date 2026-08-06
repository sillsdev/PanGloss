//! Ports `LexEntryTests` (`SIL.Machine.Morphology.HermitCrab.Tests`). Grammar/lexicon shared via `csharp_port_common` (entries `disj`/`free`/`54` transcribed verbatim from `HermitCrabTestBase.cs`); every test drives `pg_parse::Morpher::parse_word`, matching each C# test's own `ParseWord` calls, with expected values transcribed from `AssertMorphsEqual`/`Is.Empty` literals.

mod csharp_port_common;
use csharp_port_common::{assert_empty, assert_morphs_eq, build_grammar};
use pg_parse::Morpher;

/// Ports `LexEntryTests.DisjunctiveAllomorphs`'s positive half: the "disj" root's 4 environment-disjunctive allomorphs round-trip correctly for the un-suffixed and "baz"-suffixed forms.
#[test]
fn disjunctive_allomorphs() {
    let g = build_grammar("", "", ED_UD_SUFFIX_MRULE, "mrEd", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("bazɯd"), &["disj PAST"]);
    assert_morphs_eq(&m.parse_word("bas"), &["disj"]);
}

const ED_UD_SUFFIX_MRULE: &str = r#"
  <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
    <MorphologicalSubrules>
      <MorphologicalSubrule id="subEd">
        <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
        <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+ɯd</PhoneticShape></InsertSegments></MorphologicalOutput>
      </MorphologicalSubrule>
    </MorphologicalSubrules>
  </MorphologicalRule>
"#;

/// Ports `LexEntryTests.DisjunctiveAllomorphs`'s negative half: a later-indexed disjunctive allomorph is rejected whenever an earlier-indexed alternative's environment is also satisfied at the same position (first-listed-wins), so only `bazɯd` succeeds among `batɯd`/`badɯd`/`basɯd`/`bazɯd`.
#[test]
fn disjunctive_allomorphs_environment_across_boundary_diverges() {
    let g = build_grammar("", "", ED_UD_SUFFIX_MRULE, "mrEd", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_empty(&m.parse_word("batɯd"));
    assert_empty(&m.parse_word("badɯd"));
    assert_empty(&m.parse_word("basɯd"));
}

/// Ports `LexEntryTests.FreeFluctuation`: the "free" root's two unconstrained allomorphs and `ed_suffix`'s two unconstrained allomorphs surface in every combination, all glossed "free PAST".
#[test]
fn free_fluctuation() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd1">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+t</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subEd2">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><InsertSimpleContext><SimpleContext naturalClass="ncDLike" /></InsertSimpleContext></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrEd", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("tazd"), &["free PAST"]);
    assert_morphs_eq(&m.parse_word("tast"), &["free PAST"]);
    assert_morphs_eq(&m.parse_word("tazt"), &["free PAST"]);
    assert_morphs_eq(&m.parse_word("tasd"), &["free PAST"]);
}

/// Ports `LexEntryTests.PartialEntry`: entry "54" carries an empty syntactic FS (`IsPartial=true`), so a V-requiring rule still applies (empty unifies with anything), but a partial root morpheme blocks a template from ever being applicable at all.
#[test]
fn partial_entry() {
    let mrules_nominalizer = r#"
      <MorphologicalRule id="mrNom" requiredPartsOfSpeech="posV" outputPartOfSpeech="posN"><Name>nominalizer</Name><MorphemeId>NOM</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subNom">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>v</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g1 = build_grammar("", "", mrules_nominalizer, "mrNom", "");
    let m1 = Morpher::new(&g1, usize::MAX);
    assert_morphs_eq(&m1.parse_word("pi"), &["54"]);
    assert_morphs_eq(&m1.parse_word("piv"), &["54 NOM"]);

    let mrules_template = r#"
      <MorphologicalRule id="mrS" requiredPartsOfSpeech="posV"><Name>s_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subS">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let templates = r#"
      <AffixTemplate requiredPartsOfSpeech="posV"><Name>verb</Name><Slot morphologicalRules="mrS" optional="true"><Name>Sl1</Name></Slot></AffixTemplate>
    "#;
    let g2 = build_grammar("", "", mrules_template, "", templates);
    let m2 = Morpher::new(&g2, usize::MAX);
    assert_morphs_eq(&m2.parse_word("pi"), &["54"]);
    assert_empty(&m2.parse_word("pis"));
}

/// Ports `LexEntryTests.StemNames`.
/// See `docs/research/csharp-port-stem-names.md` for the shared-region exemption the interesting rows (`sadɯd`/`sapɯd`) pin down.
#[test]
fn stem_names() {
    let stem_names = r#"
      <StemNames>
        <StemName id="sn1" partsOfSpeech="posV"><Name>sn1</Name>
          <Regions>
            <Region><AssignedHeadFeatures><FeatureValue feature="featPers" symbolValues="symP1" /></AssignedHeadFeatures></Region>
            <Region><AssignedHeadFeatures><FeatureValue feature="featPers" symbolValues="symP2" /></AssignedHeadFeatures></Region>
          </Regions>
        </StemName>
        <StemName id="sn2" partsOfSpeech="posV"><Name>sn2</Name>
          <Regions>
            <Region><AssignedHeadFeatures><FeatureValue feature="featPers" symbolValues="symP1" /></AssignedHeadFeatures></Region>
            <Region><AssignedHeadFeatures><FeatureValue feature="featPers" symbolValues="symP3" /></AssignedHeadFeatures></Region>
          </Regions>
        </StemName>
      </StemNames>
    "#;
    // ed_suffix/t_suffix/s_suffix: require syntactic FS V, output head pers=1/2/3, glosses "1"/"2"/"3".
    let mrules = r#"
      <MorphologicalRule id="mrSnEd" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>ed_suffix</Name>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subSnEd">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+ɯd</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
        <OutputHeadFeatures><FeatureValue feature="featPers" symbolValues="symP1" /></OutputHeadFeatures>
        <MorphemeId>1</MorphemeId>
      </MorphologicalRule>
      <MorphologicalRule id="mrSnT" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>t_suffix</Name>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subSnT">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+t</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
        <OutputHeadFeatures><FeatureValue feature="featPers" symbolValues="symP2" /></OutputHeadFeatures>
        <MorphemeId>2</MorphemeId>
      </MorphologicalRule>
      <MorphologicalRule id="mrSnS" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>s_suffix</Name>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subSnS">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+s</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
        <OutputHeadFeatures><FeatureValue feature="featPers" symbolValues="symP3" /></OutputHeadFeatures>
        <MorphemeId>3</MorphemeId>
      </MorphologicalRule>
    "#;
    let extra_lexicon = r#"
      <LexicalEntry id="eStemname" partOfSpeech="posV">
        <Allomorphs>
          <Allomorph id="aSan"><PhoneticShape>san</PhoneticShape></Allomorph>
          <Allomorph id="aSad" stemName="sn1"><PhoneticShape>sad</PhoneticShape></Allomorph>
          <Allomorph id="aSap" stemName="sn2"><PhoneticShape>sap</PhoneticShape></Allomorph>
        </Allomorphs>
        <AssignedHeadFeatures><FeatureValue feature="featTense" symbolValues="symPres" /></AssignedHeadFeatures>
        <MorphemeId>stemname</MorphemeId>
      </LexicalEntry>
    "#;
    let g = csharp_port_common::build_grammar_w5(
        stem_names,
        "",
        mrules,
        "mrSnEd mrSnT mrSnS",
        "",
        extra_lexicon,
    );
    let m = Morpher::new(&g, usize::MAX);

    // The unrestricted `san` allomorph is excluded wherever a named stem's region claims the suffixed word's pers value; the bare form parses.
    assert_empty(&m.parse_word("sanɯd"));
    assert_empty(&m.parse_word("sant"));
    assert_empty(&m.parse_word("sans"));
    assert_morphs_eq(&m.parse_word("san"), &["stemname"]);

    // `sad`/sn1 covers pers 1 and 2 only; the bare form fails sn1's IsRequiredMatch (no pers on the word at all).
    assert_morphs_eq(&m.parse_word("sadɯd"), &["stemname 1"]);
    assert_morphs_eq(&m.parse_word("sadt"), &["stemname 2"]);
    assert_empty(&m.parse_word("sads"));
    assert_empty(&m.parse_word("sad"));

    // `sap`/sn2 covers pers 1 and 3 only; `sapɯd` (pers=1, sn1's shared region) is the exempted-shared-region row.
    assert_morphs_eq(&m.parse_word("sapɯd"), &["stemname 1"]);
    assert_empty(&m.parse_word("sapt"));
    assert_morphs_eq(&m.parse_word("saps"), &["stemname 3"]);
    assert_empty(&m.parse_word("sap"));
}
