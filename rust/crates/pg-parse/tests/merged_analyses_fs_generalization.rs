//! Ports upstream `MorpherTests.ParseWord_MergedEquivalentAnalyses_CanonicalFsCoversEveryAlternative` and `ParseWord_CategoryChangeChain_FullChainStillFound`, exercising `AnalysisStratumRule`'s merge/generalization and its companion `PriorityUnion` change together.

#[path = "csharp_port_common/mod.rs"]
mod csharp_port_common;
use csharp_port_common::assert_morphs_eq;
use pg_grammar_model::model::Grammar;
use pg_parse::Morpher;

/// Lower (Morphophonemic-like) stratum: lexicon + `r3`; upper (Allophonic-like): `r0`/`r1`/`r2`, whose declared order `upper_mrule_ids` lets the caller vary independently.
fn build_category_change_grammar(upper_mrule_ids: &str) -> Grammar {
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MergeFsGeneralization</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cZ"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cU"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name>
        <Segment segment="cZ" /><Segment segment="cU" /><Segment segment="cD" />
        <Segment segment="cI" /><Segment segment="cT" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <!-- Lower (Morphophonemic-like): the root, plus r3 (V -> V, "+z"). -->
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="r3">
        <Name>Morphophonemic</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="r3" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>r3</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub3">
                <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>r3</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eSmRoot" partOfSpeech="posV"><MorphemeId>smRoot</MorphemeId>
            <Allomorphs><Allomorph id="aSmRoot"><PhoneticShape>zud</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
      <!-- Upper (Allophonic-like): r0 (N->V,"+i"), r1 (V->N,"+t"), r2 (V->N,"+it"). -->
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="{upper_mrule_ids}">
        <Name>Allophonic</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="r0" requiredPartsOfSpeech="posN" outputPartOfSpeech="posV">
            <Name>r0</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub0">
                <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>i</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>r0</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="r1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posN">
            <Name>r1</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub1">
                <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>t</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>r1</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="r2" requiredPartsOfSpeech="posV" outputPartOfSpeech="posN">
            <Name>r2</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub2">
                <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>it</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>r2</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
    );
    pg_grammar::load(&xml).unwrap_or_else(|e| {
        panic!("merged_analyses_fs_generalization grammar failed to load: {e}\n---\n{xml}")
    })
}

/// Ports `MorpherTests.ParseWord_MergedEquivalentAnalyses_CanonicalFsCoversEveryAlternative`: `r1`-then-`r0` and `r2`-alone both un-apply to stem "zudz" with distinct rule multisets (no state-key fold) and distinct narrowed FS (N vs V), so only the V-FS `r2` candidate survives `r3` below.
#[test]
fn parse_word_merged_equivalent_analyses_canonical_fs_covers_every_alternative_r2_last() {
    let g = build_category_change_grammar("r0 r1 r2");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("zudzit"), &["smRoot r3 r2"]);
}

#[test]
fn parse_word_merged_equivalent_analyses_canonical_fs_covers_every_alternative_r2_first() {
    let g = build_category_change_grammar("r2 r0 r1");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("zudzit"), &["smRoot r3 r2"]);
}

/// One stratum, one `AffixTemplate` with three mandatory N<->V-flipping slots, analyzed in sequence.
fn build_category_change_chain_grammar() -> Grammar {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CategoryChangeChain</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cZ"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cU"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name>
        <Segment segment="cZ" /><Segment segment="cI" /><Segment segment="cM" />
        <Segment segment="cU" /><Segment segment="cA" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="n2v" requiredPartsOfSpeech="posN" outputPartOfSpeech="posV">
            <Name>n2v</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subN2v">
                <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>u</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>n2v</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="v2n" requiredPartsOfSpeech="posV" outputPartOfSpeech="posN">
            <Name>v2n</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subV2n">
                <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>i</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>v2n</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="third" requiredPartsOfSpeech="posN" outputPartOfSpeech="posV">
            <Name>third</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subThird">
                <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>third</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate requiredPartsOfSpeech="posN">
            <Name>tmpl</Name>
            <Slot morphologicalRules="n2v"><Name>Sl1</Name></Slot>
            <Slot morphologicalRules="v2n"><Name>Sl2</Name></Slot>
            <Slot morphologicalRules="third"><Name>Sl3</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="eFlipRoot" partOfSpeech="posN"><MorphemeId>flipRoot</MorphemeId>
            <Allomorphs><Allomorph id="aFlipRoot"><PhoneticShape>zim</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    pg_grammar::load(xml)
        .unwrap_or_else(|e| panic!("category-change-chain grammar failed to load: {e}\n---\n{xml}"))
}

/// Ports `MorpherTests.ParseWord_CategoryChangeChain_FullChainStillFound`: three N<->V-flipping slots un-apply in sequence rather than getting stuck at the first POS mismatch.
#[test]
fn parse_word_category_change_chain_full_chain_still_found() {
    let g = build_category_change_chain_grammar();
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("zimuia"), &["flipRoot n2v v2n third"]);
}

/// One unordered stratum where prefix `pre` and suffix `suf` un-apply from "pasagd" in either order, reaching the same shape and rule counts: the merge folds the second path into the first's alternatives.
fn build_commuting_affixes_grammar() -> Grammar {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CommutingAffixes</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name>
        <Segment segment="cP" /><Segment segment="cA" /><Segment segment="cS" /><Segment segment="cG" /><Segment segment="cD" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="pre suf">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="pre" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>pre</Name>
            <MorphologicalSubrules><MorphologicalSubrule id="subPre">
              <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><InsertSegments><PhoneticShape>pa</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
            <MorphemeId>pre</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="suf" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>suf</Name>
            <MorphologicalSubrules><MorphologicalSubrule id="subSuf">
              <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
            <MorphemeId>suf</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eSag" partOfSpeech="posV"><MorphemeId>sag</MorphemeId>
            <Allomorphs><Allomorph id="aSag"><PhoneticShape>sag</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    pg_grammar::load(xml)
        .unwrap_or_else(|e| panic!("commuting-affixes grammar failed to load: {e}\n---\n{xml}"))
}

/// `Grammar::merge_equivalent_analyses` (FieldWorks `<HC><MergeAnalyses>`) reaches the parse with no builder call: switching merging off keeps the identical analysis multiset but carries the folded path forward on its own, which costs measurably more search work.
#[test]
fn merge_analyses_off_from_the_grammar_keeps_the_analyses_and_stops_folding() {
    let merged_grammar = build_commuting_affixes_grammar();
    assert!(
        merged_grammar.merge_equivalent_analyses,
        "HC XML carries C#'s default"
    );
    let merged = Morpher::new(&merged_grammar, usize::MAX).parse_word("pasagd");

    let mut unmerged_grammar = build_commuting_affixes_grammar();
    unmerged_grammar.merge_equivalent_analyses = false;
    let unmerged = Morpher::new(&unmerged_grammar, usize::MAX).parse_word("pasagd");

    assert!(!merged.analyses.is_empty(), "the word must parse");
    assert_eq!(
        pg_parse::result_multiset(&merged.analyses),
        pg_parse::result_multiset(&unmerged.analyses),
        "merging must never change the analyses"
    );
    assert!(
        unmerged.work_steps > merged.work_steps,
        "an unmerged parse looks up the folded path separately ({} work units merged, {} unmerged)",
        merged.work_steps,
        unmerged.work_steps
    );

    let overridden = Morpher::new(&unmerged_grammar, usize::MAX)
        .with_merge_equivalent_analyses(true)
        .parse_word("pasagd");
    assert_eq!(
        overridden.work_steps, merged.work_steps,
        "the builder overrides the grammar"
    );
}
