//! Ports selected `MorpherTests` cases from the C# HermitCrab oracle; the 3 thread/memo tests substitute Rust's `Morpher::with_memo(bool)` comparison for C#'s cut intra-word parallelism, since both compare two execution strategies over the same rule-cascade machinery.

mod csharp_port_common;
use csharp_port_common::{
    assert_empty, assert_morphs_eq, build_grammar, build_grammar_cooccurrence, build_grammar_linear,
};
use pg_parse::Morpher;
use std::collections::BTreeSet;

/// Ports `MorpherTests.AnalyzeWord_CannotAnalyze_ReturnsEmptyEnumerable`: a well-formed grammar and a word that simply doesn't parse.
#[test]
fn analyze_word_cannot_analyze_returns_empty_enumerable() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrEd", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("sagd"), &["32 PAST"]); // sanity: the grammar does parse *something*
    assert_empty(&m.parse_word("sagt")); // C#'s actual negative case: "sagt" has no valid analysis
}

/// Ports `MorpherTests.AnalyzeWord_CanAnalyzeLinear_ReturnsCorrectAnalysis`: `Linear` order plus a t->d neutralization rule creates a dead-end sibling candidate, but the live PAST analysis must still be recovered.
#[test]
fn analyze_word_can_analyze_linear_returns_correct_analysis() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrT" requiredPartsOfSpeech="posN"><Name>t_suffix</Name><MorphemeId>PLURAL</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subT">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+t</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let prules = r#"
      <PhonologicalRule id="pr1"><Name>rule1</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncTSeg" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule><PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncDSeg" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    "#;
    let g = build_grammar_linear(prules, "pr1", mrules, "mrEd mrT");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("sagd"), &["32 PAST"]);
}

/// Ports `MorpherTests.AnalyzeWord_ConcurrentRepeatedParsing_IsDeterministic`: memo-on vs memo-off must agree, checked once per word since the split is a data-flow difference, not a race.
#[test]
fn analyze_word_concurrent_repeated_parsing_is_deterministic() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrEd", "");
    let memo_on = Morpher::new(&g, usize::MAX).with_memo(true);
    let memo_off = Morpher::new(&g, usize::MAX).with_memo(false);
    for word in ["sagd", "sag", "tag", "tagd", "gag", "xyzzy"] {
        let a: BTreeSet<String> = memo_on
            .parse_word(word)
            .analyses
            .into_iter()
            .map(|(m, s)| format!("{m}|{s}"))
            .collect();
        let b: BTreeSet<String> = memo_off
            .parse_word(word)
            .analyses
            .into_iter()
            .map(|(m, s)| format!("{m}|{s}"))
            .collect();
        assert_eq!(a, b, "memo-on vs memo-off disagree for {word:?}");
    }
}

/// Ports `MorpherTests.ParseWord_SingleThreaded_MatchesParallel_WithCompounding`: a compounding rule commutes with a PAST-tense prefix, forcing the memoized cascade to revisit an equal state via different arrival orders.
#[test]
fn parse_word_single_threaded_matches_parallel_with_compounding() {
    let mrules = r#"
      <CompoundingRule id="mrCompound">
        <Name>rule1</Name>
        <CompoundingSubrules>
          <CompoundingSubrule>
            <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
            <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
          </CompoundingSubrule>
        </CompoundingSubrules>
      </CompoundingRule>
      <MorphologicalRule id="mrPrefix" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>prefix</Name><MorphemeId>PAST</MorphemeId>
        <OutputHeadFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></OutputHeadFeatures>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subPrefix">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>di+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrCompound mrPrefix", "");
    let memo_on = Morpher::new(&g, usize::MAX).with_memo(true);
    let memo_off = Morpher::new(&g, usize::MAX).with_memo(false);
    for word in ["pʰutdidat", "pʰutdat"] {
        let a: BTreeSet<String> = memo_on
            .parse_word(word)
            .analyses
            .into_iter()
            .map(|(m, s)| format!("{m}|{s}"))
            .collect();
        let b: BTreeSet<String> = memo_off
            .parse_word(word)
            .analyses
            .into_iter()
            .map(|(m, s)| format!("{m}|{s}"))
            .collect();
        assert_eq!(a, b, "memo-on vs memo-off disagree for {word:?}");
    }
}

/// Ports `MorpherTests.ParseWord_SingleThreaded_MatchesParallel_WithAffixTemplate`: two commuting prefixes plus an optional-slot template suffix reach the same state via different trail orders, exercising the template-battery memo.
#[test]
fn parse_word_single_threaded_matches_parallel_with_affix_template() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>template_ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrDi" requiredPartsOfSpeech="posV"><Name>template_di_prefix</Name><MorphemeId>DI</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subDi">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>di+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrGu" requiredPartsOfSpeech="posV"><Name>template_ku_prefix</Name><MorphemeId>KU</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subGu">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>gu+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let templates = r#"
      <AffixTemplate requiredPartsOfSpeech="posV"><Name>verb_template</Name><Slot morphologicalRules="mrEd" optional="true"><Name>Sl1</Name></Slot></AffixTemplate>
    "#;
    let g = build_grammar("", "", mrules, "mrDi mrGu", templates);
    let memo_on = Morpher::new(&g, usize::MAX).with_memo(true);
    let memo_off = Morpher::new(&g, usize::MAX).with_memo(false);
    for word in ["digusagd", "disagd", "gusagd", "sagd", "sag"] {
        let a: BTreeSet<String> = memo_on
            .parse_word(word)
            .analyses
            .into_iter()
            .map(|(m, s)| format!("{m}|{s}"))
            .collect();
        let b: BTreeSet<String> = memo_off
            .parse_word(word)
            .analyses
            .into_iter()
            .map(|(m, s)| format!("{m}|{s}"))
            .collect();
        assert_eq!(a, b, "memo-on vs memo-off disagree for {word:?}");
    }
}

/// The shared `mrEd` PAST-suffix rule both co-occurrence tests below attach their rules to.
const ED_SUFFIX_MRULE: &str = r#"
  <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
    <MorphologicalSubrules>
      <MorphologicalSubrule id="subEd">
        <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
        <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
      </MorphologicalSubrule>
    </MorphologicalSubrules>
  </MorphologicalRule>
"#;

/// A decoy root that exists solely to be a legal, always-absent `otherAllomorphs`/`otherMorphemes` reference that never actually co-occurs in the tested word.
const D_ENCLITIC_ENTRY: &str = r#"
  <LexicalEntry id="eDEnclitic" partOfSpeech="posV">
    <Allomorphs><Allomorph id="aDEnclitic"><PhoneticShape>d</PhoneticShape></Allomorph></Allomorphs>
    <MorphemeId>dEnclitic</MorphemeId>
  </LexicalEntry>
"#;

/// Ports `MorpherTests.AnalyzeWord_CannotAnalyzeDueToAllomorphCooccurenceFailure_ReturnsEmptyEnumerable`: an `AllomorphCoOccurrenceRule` excluding the PAST suffix's allomorph blocks "sagd" from analyzing at all.
#[test]
fn analyze_word_cannot_analyze_due_to_allomorph_cooccurence_failure_returns_empty_enumerable() {
    // The single exclusion rule alone already rejects "sagd".
    let coo1 = r#"
      <AllomorphCoOccurrenceRules>
        <AllomorphCoOccurrenceRule type="exclude" primaryAllomorph="a32" otherAllomorphs="subEd" adjacency="anywhere" />
      </AllomorphCoOccurrenceRules>
    "#;
    let g1 = build_grammar_cooccurrence(ED_SUFFIX_MRULE, "mrEd", "", coo1);
    assert_empty(&Morpher::new(&g1, usize::MAX).parse_word("sagd"));

    // A second, trivially-satisfied exclusion rule (never actually co-occurring in this grammar) must not rescue "sagd": every attached rule must pass, not just one.
    let coo2 = r#"
      <AllomorphCoOccurrenceRules>
        <AllomorphCoOccurrenceRule type="exclude" primaryAllomorph="a32" otherAllomorphs="subEd" adjacency="anywhere" />
        <AllomorphCoOccurrenceRule type="exclude" primaryAllomorph="a32" otherAllomorphs="aDEnclitic" adjacency="anywhere" />
      </AllomorphCoOccurrenceRules>
    "#;
    let g2 = build_grammar_cooccurrence(ED_SUFFIX_MRULE, "mrEd", D_ENCLITIC_ENTRY, coo2);
    assert_empty(&Morpher::new(&g2, usize::MAX).parse_word("sagd"));
}

/// Ports `MorpherTests.AnalyzeWord_CannotAnalyzeDueToMorphemeCooccurenceFailure_ReturnsEmptyEnumerable`: identical to the allomorph-level test above, but the exclusion is at morpheme granularity.
#[test]
fn analyze_word_cannot_analyze_due_to_morpheme_cooccurence_failure_returns_empty_enumerable() {
    let coo1 = r#"
      <MorphemeCoOccurrenceRules>
        <MorphemeCoOccurrenceRule type="exclude" primaryMorpheme="e32" otherMorphemes="mrEd" adjacency="anywhere" />
      </MorphemeCoOccurrenceRules>
    "#;
    let g1 = build_grammar_cooccurrence(ED_SUFFIX_MRULE, "mrEd", "", coo1);
    assert_empty(&Morpher::new(&g1, usize::MAX).parse_word("sagd"));

    // Same AND-semantics re-check as the allomorph-level test, at morpheme granularity.
    let coo2 = r#"
      <MorphemeCoOccurrenceRules>
        <MorphemeCoOccurrenceRule type="exclude" primaryMorpheme="e32" otherMorphemes="mrEd" adjacency="anywhere" />
        <MorphemeCoOccurrenceRule type="exclude" primaryMorpheme="e32" otherMorphemes="eDEnclitic" adjacency="anywhere" />
      </MorphemeCoOccurrenceRules>
    "#;
    let g2 = build_grammar_cooccurrence(ED_SUFFIX_MRULE, "mrEd", D_ENCLITIC_ENTRY, coo2);
    assert_empty(&Morpher::new(&g2, usize::MAX).parse_word("sagd"));
}
