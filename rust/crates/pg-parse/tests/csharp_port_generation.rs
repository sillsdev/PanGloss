//! Ports `MorpherTests`' `GenerateWords_*`/`AnalyzeWord_CanAnalyze_ReturnsCorrectAnalysis` tests plus `CompoundingRuleTests.MorphosyntacticRules`'s bare-`LexEntry`-as-non-head case, covering the `WordAnalysis`/`GenerateWords` direct API this crate's other C# ports leave out of scope.

#[path = "csharp_port_common/mod.rs"]
mod csharp_port_common;
use csharp_port_common::{
    build_grammar, build_grammar_custom_lexicon, lex_entry_id, morpheme_ordinal, mrule_id,
};
use pg_featstruct::FeatureStruct;
use pg_grammar_model::model::AllomorphOwner;
use pg_parse::{AnalysisProvenance, GenMorpheme, GenerateWordsStopReason, Morpher, WordAnalysis};
use std::collections::BTreeSet;

const MULTI_ALLOMORPH_COMPOUND_RULE: &str = r#"
  <CompoundingRule id="mrC">
    <Name>compound</Name>
    <CompoundingSubrules><CompoundingSubrule>
      <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
      <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
    </CompoundingSubrule></CompoundingSubrules>
  </CompoundingRule>
"#;

const MULTI_ALLOMORPH_LEXICON: &str = r#"
  <LexicalEntry id="root" partOfSpeech="posN"><MorphemeId>root</MorphemeId>
    <Allomorphs>
      <Allomorph id="root1"><PhoneticShape>ka</PhoneticShape></Allomorph>
      <Allomorph id="root2"><PhoneticShape>ta</PhoneticShape></Allomorph>
    </Allomorphs>
  </LexicalEntry>
  <LexicalEntry id="nonhead" partOfSpeech="posN"><MorphemeId>nonhead</MorphemeId>
    <Allomorphs>
      <Allomorph id="nonhead1"><PhoneticShape>mi</PhoneticShape></Allomorph>
      <Allomorph id="nonhead2"><PhoneticShape>ni</PhoneticShape></Allomorph>
    </Allomorphs>
  </LexicalEntry>
  <LexicalEntry id="lateZero" partOfSpeech="posN"><MorphemeId>lateZero</MorphemeId>
    <Allomorphs><Allomorph id="lateZero1"><PhoneticShape>ya</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
"#;

fn multi_allomorph_compound_grammar() -> pg_grammar_model::model::Grammar {
    build_grammar_custom_lexicon(
        MULTI_ALLOMORPH_COMPOUND_RULE,
        "mrC",
        MULTI_ALLOMORPH_LEXICON,
    )
}

fn multi_allomorph_compound_words(
    morpher: &Morpher<'_>,
    grammar: &pg_grammar_model::model::Grammar,
) -> pg_parse::GenerateWordsOutcome {
    morpher.generate_words_bounded(
        lex_entry_id(grammar, "root"),
        &[GenMorpheme::NonHead(lex_entry_id(grammar, "nonhead"))],
        FeatureStruct::EMPTY,
    )
}

#[test]
fn bounded_generation_keeps_small_multi_allomorph_surface_set() {
    let grammar = multi_allomorph_compound_grammar();
    let morpher = Morpher::new(&grammar, 1_000);

    let outcome = multi_allomorph_compound_words(&morpher, &grammar);

    assert_eq!(outcome.stop_reason, None);
    assert_eq!(outcome.seed_choices, 4);
    assert_eq!(
        outcome.surfaces,
        vec![
            "kami".to_string(),
            "kani".to_string(),
            "tami".to_string(),
            "tani".to_string()
        ]
    );
}

#[test]
fn bounded_generation_shares_tiny_allowance_across_seed_choices() {
    let grammar = multi_allomorph_compound_grammar();
    let morpher = Morpher::new(&grammar, 100);

    let outcome = multi_allomorph_compound_words(&morpher, &grammar);

    assert_eq!(
        outcome.stop_reason,
        Some(GenerateWordsStopReason::WorkLimitReached { limit: 100 })
    );
    assert_eq!(outcome.seed_choices, 3, "{outcome:?}");
    assert!(outcome.seed_choices < 4, "{outcome:?}");
    assert!(outcome.work_units <= 100, "{outcome:?}");
}

#[test]
fn bounded_generation_refuses_overflowing_seed_product_before_visiting_it() {
    let grammar = multi_allomorph_compound_grammar();
    let root = lex_entry_id(&grammar, "root");
    let nonhead = lex_entry_id(&grammar, "nonhead");
    let others = vec![GenMorpheme::NonHead(nonhead); 63];
    let morpher = Morpher::new(&grammar, 100);

    let outcome = morpher.generate_words_bounded(root, &others, FeatureStruct::EMPTY);

    assert_eq!(
        outcome.stop_reason,
        Some(GenerateWordsStopReason::SeedProductExceedsBudget { limit: 100 })
    );
    assert_eq!(outcome.seed_choices, 0);
    assert_eq!(outcome.work_units, 0);
    assert!(outcome.surfaces.is_empty());
}

#[test]
fn bounded_generation_keeps_late_zero_after_product_overflow() {
    let mut grammar = multi_allomorph_compound_grammar();
    let root = lex_entry_id(&grammar, "root");
    let nonhead = lex_entry_id(&grammar, "nonhead");
    let late_zero = lex_entry_id(&grammar, "lateZero");
    // The loader drops empty entries; clear this sentinel to exercise the exact product invariant.
    let removed_allos = grammar.entries[late_zero.0 as usize].allomorphs.len();
    let owner_boundary = grammar.allomorph_owners.len() - removed_allos;
    assert!(grammar.allomorph_owners[owner_boundary..]
        .iter()
        .all(|owner| matches!(owner, AllomorphOwner::Root(entry, _) if *entry == late_zero)));
    grammar.allomorph_owners.truncate(owner_boundary);
    grammar.allomorph_sources.truncate(owner_boundary);
    grammar.entries[late_zero.0 as usize].allomorphs.clear();
    let morpher = Morpher::new(&grammar, 100);
    let mut others = vec![GenMorpheme::NonHead(nonhead); usize::BITS as usize];
    others.push(GenMorpheme::NonHead(late_zero));

    let outcome = morpher.generate_words_bounded(root, &others, FeatureStruct::EMPTY);

    assert_eq!(outcome.stop_reason, None);
    assert_eq!(outcome.seed_choices, 0);
    assert_eq!(outcome.work_units, 0);
    assert!(outcome.surfaces.is_empty());
}

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
        morph_occurrences: vec![],
        root_morpheme_index: 1,
        pos_id: None,
        syn_fs: pg_featstruct::FeatureStruct::EMPTY,
        mpr: pg_grammar_model::model::MprSet::EMPTY,
        guessed: false,
        guessed_string: None,
        guessed_pattern: None,
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
        morph_occurrences: vec![],
        root_morpheme_index: 0,
        pos_id: None,
        syn_fs: pg_featstruct::FeatureStruct::EMPTY,
        mpr: pg_grammar_model::model::MprSet::EMPTY,
        guessed: false,
        guessed_string: None,
        guessed_pattern: None,
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
        morph_occurrences: vec![],
        root_morpheme_index: 2,
        pos_id: None,
        syn_fs: pg_featstruct::FeatureStruct::EMPTY,
        mpr: pg_grammar_model::model::MprSet::EMPTY,
        guessed: false,
        guessed_string: None,
        guessed_pattern: None,
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
        morph_occurrences: outcome.structured[0].morph_occurrences.clone(),
        root_morpheme_index: 0,
        pos_id: Some(want_pos),
        syn_fs: outcome.structured[0].syn_fs.clone(),
        mpr: outcome.structured[0].mpr,
        guessed: false,
        guessed_string: outcome.structured[0].guessed_string.clone(),
        guessed_pattern: None,
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
