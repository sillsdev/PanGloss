//! Ports `MorpherTests`' `GenerateWords_*`/`AnalyzeWord_CanAnalyze_ReturnsCorrectAnalysis` tests plus `CompoundingRuleTests.MorphosyntacticRules`'s bare-`LexEntry`-as-non-head case, covering the `WordAnalysis`/`GenerateWords` direct API this crate's other C# ports leave out of scope.

mod csharp_port_common;
use csharp_port_common::{build_grammar, lex_entry_id, morpheme_ordinal, mrule_id};
use pg_featstruct::FeatureStruct;
use pg_parse::{AnalysisProvenance, GenMorpheme, Morpher, WordAnalysis};
use std::collections::BTreeSet;

/// `si+` prefix (3SG) + `+ɯd` suffix (PAST), the two `AffixProcessRule`s `GenerateWords_CanGenerate_ReturnsCorrectWord` builds inline in C#, ported here as XML.
const SI_ED_MRULES: &str = r#"
  <MorphologicalRule id="mrSi" requiredPartsOfSpeech="posV"><Name>si_prefix</Name><MorphemeId>3SG</MorphemeId>
    <MorphologicalSubrules><MorphologicalSubrule id="subSi">
      <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
      <MorphologicalOutput><InsertSegments><PhoneticShape>si+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
    </MorphologicalSubrule></MorphologicalSubrules>
  </MorphologicalRule>
  <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
    <MorphologicalSubrules><MorphologicalSubrule id="subEd">
      <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+ɯd</PhoneticShape></InsertSegments></MorphologicalOutput>
    </MorphologicalSubrule></MorphologicalSubrules>
  </MorphologicalRule>
"#;

/// PORT-CORRESPONDENCE: ports `GenerateWords_CanGenerate_ReturnsCorrectWord` -- root "33" with `si_prefix`/`ed_suffix` must generate exactly `"sisasɯd"` via `Morpher::generate_words_from_analysis`.
#[test]
fn generate_words_can_generate_returns_correct_word() {
    let g = build_grammar("", "", SI_ED_MRULES, "mrSi mrEd", "");
    let m = Morpher::new(&g, usize::MAX);

    let wa = WordAnalysis {
        morpheme_ids: vec![
            morpheme_ordinal(&g, "3SG"),
            morpheme_ordinal(&g, "33"),
            morpheme_ordinal(&g, "PAST"),
        ],
        root_morpheme_index: 1,
        pos_id: None,
        syn_fs: pg_featstruct::FeatureStruct::EMPTY,
        mpr: pg_grammar::model::MprSet::EMPTY,
        guessed: false,
        provenance: AnalysisProvenance::Grammar,
        supplied_root: None,
        morpheme_roots: vec![None; 3],
    };
    let words: BTreeSet<String> = m.generate_words_from_analysis(&wa).into_iter().collect();
    assert_eq!(words, BTreeSet::from(["sisasɯd".to_string()]));
}

/// PORT-CORRESPONDENCE: ports `GenerateWords_CannotGenerate_ReturnsEmptyEnumerable` -- a `PL`-suffix requiring `posN` cannot generate from a `posV` root, a POS mismatch on the required-syn-fs gate.
#[test]
fn generate_words_cannot_generate_returns_empty_enumerable() {
    let mrules = r#"
      <MorphologicalRule id="mrPl" requiredPartsOfSpeech="posN"><Name>ed_suffix</Name><MorphemeId>PL</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subPl">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+ɯd</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrPl", "");
    let m = Morpher::new(&g, usize::MAX);

    let wa = WordAnalysis {
        morpheme_ids: vec![morpheme_ordinal(&g, "32"), morpheme_ordinal(&g, "PL")],
        root_morpheme_index: 0,
        pos_id: None,
        syn_fs: pg_featstruct::FeatureStruct::EMPTY,
        mpr: pg_grammar::model::MprSet::EMPTY,
        guessed: false,
        provenance: AnalysisProvenance::Grammar,
        supplied_root: None,
        morpheme_roots: vec![None; 1],
    };
    assert!(m.generate_words_from_analysis(&wa).is_empty());
}

/// Direct-API sanity check, no C# citation: the same `ed_suffix` rule via `Morpher::generate_words` directly must reproduce "sas" + "+ɯd" (boundary stripped) = "sasɯd".
#[test]
fn direct_api_single_allomorph_suffix() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subEd">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+ɯd</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrEd", "");
    let m = Morpher::new(&g, usize::MAX);

    let root = lex_entry_id(&g, "33");
    let rule = mrule_id(&g, "PAST");
    let words = m.generate_words(root, &[GenMorpheme::Rule(rule)], FeatureStruct::EMPTY);
    assert_eq!(words, vec!["sasɯd".to_string()]);
}

/// PORT-CORRESPONDENCE: ports `CompoundingRuleTests.MorphosyntacticRules`'s `GenerateWords` call with a bare `LexEntry` as a compounding non-head (owning `CompoundingRule` unspecified); must produce `"pʰutdat"`.
#[test]
fn direct_api_compounding_non_head() {
    let mrules = r#"
      <CompoundingRule id="mrC" nonHeadPartsOfSpeech="posV">
        <Name>rule1</Name>
        <CompoundingSubrules><CompoundingSubrule>
          <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
          <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
        </CompoundingSubrule></CompoundingSubrules>
      </CompoundingRule>
    "#;
    let g = build_grammar("", "", mrules, "mrC", "");
    let m = Morpher::new(&g, usize::MAX);

    let root = lex_entry_id(&g, "5");
    let non_head = lex_entry_id(&g, "9");
    let words = m.generate_words(
        root,
        &[GenMorpheme::NonHead(non_head)],
        FeatureStruct::EMPTY,
    );
    assert_eq!(words, vec!["pʰutdat".to_string()]);
}

/// No C# citation: two `GenMorpheme::NonHead` items in one call pin that a nested compound resolves each non-head slot in turn, not by re-reading the same slot twice.
#[test]
fn direct_api_compounding_two_non_heads_resolve_distinct_slots() {
    let mrules = r#"
      <CompoundingRule id="mrC">
        <Name>rule1</Name>
        <CompoundingSubrules><CompoundingSubrule>
          <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
          <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
        </CompoundingSubrule></CompoundingSubrules>
      </CompoundingRule>
    "#;
    // Unrestricted (no POS gates): the same rule must be free to re-confirm on its own output for the nested compounding step.
    let g = build_grammar("", "", mrules, "mrC", "");
    let m = Morpher::new(&g, usize::MAX);

    let root = lex_entry_id(&g, "5"); // "pʰut"
    let dat = lex_entry_id(&g, "8"); // "dat" -- pushed FIRST (non_heads[0]), confirmed SECOND
    let bupu = lex_entry_id(&g, "46"); // "bupu" -- pushed SECOND (non_heads[1]), confirmed FIRST
    let words = m.generate_words(
        root,
        &[GenMorpheme::NonHead(dat), GenMorpheme::NonHead(bupu)],
        FeatureStruct::EMPTY,
    );
    assert_eq!(
        words,
        vec!["pʰutbupudat".to_string()],
        "each non-head must be used exactly once (bupu innermost, dat outermost); got {words:?} \
         (\"pʰutbupubupu\" would mean the second confirmation re-read the wrong non-head)"
    );
}

/// PORT-CORRESPONDENCE: pins that generation reverses the left-prefix slice to match C#'s stack-based confirmation order; the grammar's POS chain makes only the C#-correct order synthesize anything.
#[test]
fn generate_words_from_analysis_two_prefixes_confirm_in_the_correct_relative_order() {
    let mrules = r#"
      <MorphologicalRule id="mrOuter" requiredPartsOfSpeech="posV" outputPartOfSpeech="posA"><Name>outer</Name><MorphemeId>OUTER</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subOuter">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><InsertSegments><PhoneticShape>o+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrInner" requiredPartsOfSpeech="posA" outputPartOfSpeech="posA"><Name>inner</Name><MorphemeId>INNER</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subInner">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><InsertSegments><PhoneticShape>i+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrOuter mrInner", "");
    let m = Morpher::new(&g, usize::MAX);

    // Word-position order: OUTER (leftmost) then INNER (adjacent to root) then the root "32" ("sag", V).
    let wa = WordAnalysis {
        morpheme_ids: vec![
            morpheme_ordinal(&g, "OUTER"),
            morpheme_ordinal(&g, "INNER"),
            morpheme_ordinal(&g, "32"),
        ],
        root_morpheme_index: 2,
        pos_id: None,
        syn_fs: pg_featstruct::FeatureStruct::EMPTY,
        mpr: pg_grammar::model::MprSet::EMPTY,
        guessed: false,
        provenance: AnalysisProvenance::Grammar,
        supplied_root: None,
        morpheme_roots: vec![None; 3],
    };
    let words: BTreeSet<String> = m.generate_words_from_analysis(&wa).into_iter().collect();
    assert_eq!(words, BTreeSet::from(["iosag".to_string()]));
}

/// PORT-CORRESPONDENCE: ports `AnalyzeWord_CanAnalyze_ReturnsCorrectAnalysis` -- the structured `WordAnalysis`-return path (`ParseOutcome::structured`), unlike the sibling port that checks only the signature string.
#[test]
fn analyze_word_can_analyze_returns_correct_analysis() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subEd">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrEd", "");
    let m = Morpher::new(&g, usize::MAX);

    let outcome = m.parse_word("sagd");
    let want_pos = g
        .syn_features
        .symbol_index(g.syn_features.pos, "posV")
        .expect("posV symbol");
    let want = WordAnalysis {
        morpheme_ids: vec![morpheme_ordinal(&g, "32"), morpheme_ordinal(&g, "PAST")],
        root_morpheme_index: 0,
        pos_id: Some(want_pos),
        syn_fs: outcome.structured[0].syn_fs.clone(),
        mpr: outcome.structured[0].mpr,
        guessed: false,
        provenance: AnalysisProvenance::Grammar,
        supplied_root: None,
        morpheme_roots: vec![None; 2],
    };
    assert_eq!(
        outcome.structured,
        vec![want],
        "structured WordAnalysis mismatch (got {:?})",
        outcome.structured
    );
}
