//! Ports `LexEntryTests` (parse-opt: `tests/SIL.Machine.Morphology.HermitCrab.Tests/LexEntryTests.cs`)
//! bucket-B rows per `rust/parity-out/audit/phase2/D-test-coverage-map.md` §3. Grammar/lexicon shared
//! via [`csharp_port_common`] (entries `disj`/`free`/`54` transcribed verbatim from
//! `HermitCrabTestBase.cs`); every test drives `hc_parse::Morpher::parse_word`, matching each C#
//! test's own `morpher.ParseWord(...)` calls. Expected values are transcribed from the C# source's
//! `AssertMorphsEqual`/`Is.Empty` literals.
//!
//! `StemNames` (D-batch-3) is now ported here — the W5 realizational-cluster port unlinted
//! `StemName` (see [`stem_names`]). `BoundRootAllomorph`/`AllomorphEnvironments` (already bucket A
//! via `hc-rules/tests/validity_gate.rs`) remain out of this file's scope.

mod csharp_port_common;
use csharp_port_common::{assert_empty, assert_morphs_eq, build_grammar};
use hc_parse::Morpher;

/// Ports `LexEntryTests.DisjunctiveAllomorphs` (LexEntryTests.cs:13-39), positive/live half. The
/// "disj" root's 4 allomorphs are environment-disjunctive; the un-suffixed and "baz"-suffixed forms
/// (which don't turn on the boundary-transparency question raised in
/// [`disjunctive_allomorphs_environment_across_boundary_diverges`]) round-trip correctly.
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

/// Ports `LexEntryTests.DisjunctiveAllomorphs`'s negative half (`LexEntryTests.cs:35-37`):
/// `batɯd`/`badɯd`/`basɯd` all fail; only `bazɯd` succeeds. Un-ignored by W3.2 (plan #5d,
/// history row `987be2fd`): the W11 port initially flagged this as a mystery ("a stricter
/// environment succeeding while a looser one fails is impossible if both are checked the same
/// way"), but the mechanism is not environment matching at all — it is the disjunctive-allomorph
/// final re-check (`Allomorph.IsWordValid`'s second loop, Allomorph.cs:127-152): the "disj"
/// root's allomorphs are ordered `baz`(0, unrounded-V env) / `bat`(1, V env) / `bad`(2, V env) /
/// `bas`(3, elsewhere), and a word using a later-indexed allomorph is REJECTED whenever an
/// earlier-indexed, non-free-fluctuating alternative's environment is also satisfied at the same
/// morph position — before `ɯd`, allomorph 0's "followed by an unrounded vowel" is satisfied, so
/// every later allomorph loses to it (first-listed-wins disjunctivity), while `bazɯd` itself has
/// no earlier rival. Now implemented in `hc-rules/src/validity.rs` (the `passed_over`/
/// `Range(0, Index)` candidate loop); the freestanding oracle fixture is
/// `rust/conformance/allomorphy/disjunctive-recheck/`.
#[test]
fn disjunctive_allomorphs_environment_across_boundary_diverges() {
    let g = build_grammar("", "", ED_UD_SUFFIX_MRULE, "mrEd", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_empty(&m.parse_word("batɯd"));
    assert_empty(&m.parse_word("badɯd"));
    assert_empty(&m.parse_word("basɯd"));
}

/// Ports `LexEntryTests.FreeFluctuation` (LexEntryTests.cs:41-83). The "free" root's `taz`/`tas`
/// allomorphs are both unconstrained (free fluctuation, no distinguishing environment), and
/// `ed_suffix` ALSO has two unconstrained allomorphs (`+t` / `+`+voiced-alveolar-stop) -- every
/// combination of root-allomorph x affix-allomorph surfaces, all glossed "free PAST".
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

/// Ports `LexEntryTests.PartialEntry` (LexEntryTests.cs:265-316). Entry "54" ("pi") carries an
/// EMPTY syntactic FS (`IsPartial=true`, matching a FLEx placeholder/underspecified entry): a
/// V-requiring `nominalizer` still applies to it (empty unifies with anything), so "pi"/"piv" both
/// parse. Once `nominalizer` is replaced by an optional-slot template (`s_suffix`), "pi" still parses
/// bare but "pis" does NOT (Tier-2 #13 gate 2: the *root morpheme* being partial blocks the template
/// from ever being applicable, so there is no path to attach "s").
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

/// Ports `LexEntryTests.StemNames` (LexEntryTests.cs:85-166, W5/D-batch-3 — formerly out of scope
/// per this file's module doc; the W5 realizational-cluster port unlints `StemName`). The
/// `stemname` root (`HermitCrabTestBase.cs:722-768`: FS `{V, head:{tense:pres}}`, allomorphs
/// `san`/`sad`/`sap`, with `sad` restricted to stem name `sn1` = regions `{pers:1}|{pers:2}` and
/// `sap` to `sn2` = `{pers:1}|{pers:3}`) is supplied via [`build_grammar_w5`]'s extra-lexicon
/// block; three suffix rules assign `pers` 1/2/3 via `OutputHeadFeatures` exactly as the C# test's
/// `ed`/`t`/`s` suffixes do. Every `AssertMorphsEqual` literal is transcribed verbatim; the
/// interesting rows are `sadɯd`/`sapɯd` (same `pers=1`, DIFFERENT allomorphs both valid, because
/// `sn1`/`sn2` share the `{pers:1}` region and `StemName.IsExcludedMatch` exempts shared regions)
/// and `sanɯd`/`sant`/`sans` (the unrestricted `san` is blocked wherever either named stem's
/// region claims the word's `pers` value).
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
    // ed_suffix/t_suffix/s_suffix (LexEntryTests.cs:90-148): RequiredSyntacticFeatureStruct V,
    // OutSyntacticFeatureStruct head pers=1/2/3, glosses "1"/"2"/"3".
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

    // LexEntryTests.cs:152-155: the unrestricted `san` allomorph is excluded wherever a named
    // stem's region claims the suffixed word's pers value; the bare form parses.
    assert_empty(&m.parse_word("sanɯd"));
    assert_empty(&m.parse_word("sant"));
    assert_empty(&m.parse_word("sans"));
    assert_morphs_eq(&m.parse_word("san"), &["stemname"]);

    // cs:157-160: `sad`/sn1 covers pers 1 and 2 only; the bare form fails sn1's IsRequiredMatch
    // (no pers on the word at all).
    assert_morphs_eq(&m.parse_word("sadɯd"), &["stemname 1"]);
    assert_morphs_eq(&m.parse_word("sadt"), &["stemname 2"]);
    assert_empty(&m.parse_word("sads"));
    assert_empty(&m.parse_word("sad"));

    // cs:162-165: `sap`/sn2 covers pers 1 and 3 only; `sapɯd` (pers=1, sn1's SHARED region) is
    // the exempted-shared-region row.
    assert_morphs_eq(&m.parse_word("sapɯd"), &["stemname 1"]);
    assert_empty(&m.parse_word("sapt"));
    assert_morphs_eq(&m.parse_word("saps"), &["stemname 3"]);
    assert_empty(&m.parse_word("sap"));
}
