//! A grammar with no phonological features still prunes disagreeing copies, because C# keeps the segment spelling (`StrRep`) on every segment node when the feature system is empty and so `CanAlign` compares characters there (`AnalysisMorphologicalTransform.CanAlign`, `FeatureStruct.IsUnifiable`). All words and morphemes below are invented.

use pg_grammar::load;
use pg_grammar_model::model::{Grammar, StratumId};
use pg_parse::Morpher;
use pg_rules::stratum::{AnalyzerConfig, StepBudget};
use pg_rules::Word;
use pg_shape::{CdBits, CdSet, NodeFlags, NodeKind, Shape, ShapeBuilder};

/// Segments `p t k s a i u` where `u` also spells `ú`; a CV copy rule `redCV` (copy C, V, C, V, then the rest); roots `tasi`, `tuka`, `pasa`.
fn featureless_grammar() -> Grammar {
    let seg = |id: &str, reps: &[&str]| {
        let reps: String = reps
            .iter()
            .map(|r| format!("<Representation>{r}</Representation>"))
            .collect();
        format!(
            r#"<SegmentDefinition id="{id}"><Representations>{reps}</Representations></SegmentDefinition>"#
        )
    };
    let segments = [
        seg("cp", &["p"]),
        seg("ct", &["t"]),
        seg("ck", &["k"]),
        seg("cs", &["s"]),
        seg("ca", &["a"]),
        seg("ci", &["i"]),
        seg("cu", &["u", "ú"]),
    ]
    .concat();
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>FeaturelessCopy</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>{segments}</SegmentDefinitions></CharacterDefinitionTable>
<NaturalClasses>
  <SegmentNaturalClass id="ncC"><Name>C</Name><Segment segment="cp"/><Segment segment="ct"/><Segment segment="ck"/><Segment segment="cs"/></SegmentNaturalClass>
  <SegmentNaturalClass id="ncV"><Name>V</Name><Segment segment="ca"/><Segment segment="ci"/><Segment segment="cu"/></SegmentNaturalClass>
  <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
</NaturalClasses>
<Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrRed"><Name>S</Name>
<MorphologicalRuleDefinitions>
  <MorphologicalRule id="mrRed"><Name>redCV</Name><MorphemeId>RED</MorphemeId>
    <MorphologicalSubrules><MorphologicalSubrule id="sub1">
      <MorphologicalInput>
        <PhoneticSequence id="c"><SimpleContext naturalClass="ncC"/></PhoneticSequence>
        <PhoneticSequence id="v"><SimpleContext naturalClass="ncV"/></PhoneticSequence>
        <PhoneticSequence id="rest"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny"/></OptionalSegmentSequence></PhoneticSequence>
      </MorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="c"/><CopyFromInput index="v"/><CopyFromInput index="c"/><CopyFromInput index="v"/><CopyFromInput index="rest"/></MorphologicalOutput>
    </MorphologicalSubrule></MorphologicalSubrules>
  </MorphologicalRule>
</MorphologicalRuleDefinitions>
<LexicalEntries>
  <LexicalEntry id="e1" partOfSpeech="posV"><MorphemeId>tasi</MorphemeId><Allomorphs><Allomorph id="a1"><PhoneticShape>tasi</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>
  <LexicalEntry id="e2" partOfSpeech="posV"><MorphemeId>tuka</MorphemeId><Allomorphs><Allomorph id="a2"><PhoneticShape>tuka</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>
  <LexicalEntry id="e3" partOfSpeech="posV"><MorphemeId>pasa</MorphemeId><Allomorphs><Allomorph id="a3"><PhoneticShape>pasa</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>
</LexicalEntries>
</Stratum></Strata></Language></HermitCrabInput>"#
    );
    load(&xml).unwrap_or_else(|e| panic!("featureless copy grammar failed to load: {e}"))
}

fn shape_of(g: &Grammar, text: &str) -> Shape {
    pg_rules::shape_feat::segment_with_features(g, &g.char_tables[0], text)
        .unwrap_or_else(|_| panic!("{text:?} should segment"))
}

fn analysis_outputs(g: &Grammar, shape: Shape, prune: bool) -> Vec<Word> {
    let stratum = StratumId(0);
    let config = AnalyzerConfig {
        prune_disagreeing_copies: prune,
        ..AnalyzerConfig::default()
    };
    pg_rules::stratum::analyze_stratum(
        g,
        stratum,
        Word::new(shape, stratum),
        &config,
        &StepBudget::new(usize::MAX),
    )
    .words
    .into_iter()
    .filter(|word| !word.mrule_apps.is_empty())
    .collect()
}

#[test]
fn the_grammar_really_has_no_phonological_features() {
    let g = featureless_grammar();
    assert!(
        g.char_tables[0].unif_closure_rows().is_none(),
        "this grammar must be in the characters-only (StrRep) regime"
    );
}

/// Two different characters in the copied consonant slot disagree, so the split is dropped.
#[test]
fn disagreeing_characters_are_pruned_without_features() {
    let g = featureless_grammar();
    let off = analysis_outputs(&g, shape_of(&g, "pasapa"), false);
    let on = analysis_outputs(&g, shape_of(&g, "pasapa"), true);
    assert_eq!(off.len(), 1, "unpruned analysis proposes the split");
    assert_eq!(on.len(), 0, "pruned analysis must refuse p/s");
}

/// A different vowel in the second copy disagrees as well.
#[test]
fn disagreeing_vowels_are_pruned_without_features() {
    let g = featureless_grammar();
    assert_eq!(analysis_outputs(&g, shape_of(&g, "tatika"), false).len(), 1);
    assert_eq!(analysis_outputs(&g, shape_of(&g, "tatika"), true).len(), 0);
}

/// Identical characters agree, so the split survives pruning.
#[test]
fn agreeing_characters_survive_without_features() {
    let g = featureless_grammar();
    let on = analysis_outputs(&g, shape_of(&g, "tatasi"), true);
    assert_eq!(on.len(), 1);
    assert_eq!(
        on.len(),
        analysis_outputs(&g, shape_of(&g, "tatasi"), false).len()
    );
}

/// Two spellings of one segment are one character definition, so their copies agree.
#[test]
fn spellings_of_one_segment_agree() {
    let g = featureless_grammar();
    let shape = shape_of(&g, "tutúka");
    assert_eq!(
        shape.char_def(2),
        shape.char_def(4),
        "u and ú share a segment"
    );
    assert_eq!(analysis_outputs(&g, shape.clone(), false).len(), 1);
    assert_eq!(analysis_outputs(&g, shape, true).len(), 1);
}

/// Replaces the second consonant of `pasapa` (node 3) with a class-derived node that has no fixed character.
fn with_abstract_second_consonant(g: &Grammar, cd_set: CdSet) -> Shape {
    let shape = shape_of(g, "pasapa");
    let lanes = shape.node_lanes(3).to_vec();
    let mut builder = ShapeBuilder::from_shape(&shape);
    builder.delete(3);
    builder.insert_with_set(3, NodeFlags::EMPTY, &lanes, cd_set);
    builder.freeze()
}

/// A class-derived node with no member restriction could be any character: unsure, so it keeps the match.
#[test]
fn an_unrestricted_abstract_node_keeps_the_match() {
    let g = featureless_grammar();
    let shape = with_abstract_second_consonant(&g, CdSet::Unrestricted);
    assert_eq!(shape.char_def(3), pg_shape::NO_CHAR_DEF);
    assert_eq!(analysis_outputs(&g, shape.clone(), false).len(), 1);
    assert_eq!(analysis_outputs(&g, shape, true).len(), 1);
}

/// A class-derived node whose members include the other copy's character could agree, so it keeps the match.
#[test]
fn an_abstract_node_that_includes_the_character_keeps_the_match() {
    let g = featureless_grammar();
    let p = shape_of(&g, "pa").char_def(1);
    let s = shape_of(&g, "sa").char_def(1);
    let shape = with_abstract_second_consonant(&g, CdSet::Members(CdBits::from_ids([p, s])));
    assert_eq!(analysis_outputs(&g, shape, true).len(), 1);
}

/// A class-derived node whose members exclude the other copy's character cannot agree, so the match goes.
#[test]
fn an_abstract_node_that_excludes_the_character_is_pruned() {
    let g = featureless_grammar();
    let s = shape_of(&g, "sa").char_def(1);
    let k = shape_of(&g, "ka").char_def(1);
    let shape = with_abstract_second_consonant(&g, CdSet::Members(CdBits::from_ids([s, k])));
    assert_eq!(analysis_outputs(&g, shape.clone(), false).len(), 1);
    assert_eq!(analysis_outputs(&g, shape, true).len(), 0);
}

/// An optional node in a copy is an unapplied deletion: undecidable, so it keeps the match.
#[test]
fn an_optional_copy_node_keeps_the_match() {
    let g = featureless_grammar();
    let shape = shape_of(&g, "pasapa");
    let char_def = shape.char_def(3);
    let lanes = shape.node_lanes(3).to_vec();
    let mut builder = ShapeBuilder::from_shape(&shape);
    builder.delete(3);
    builder.insert(
        3,
        NodeKind::Segment,
        char_def,
        NodeFlags(NodeFlags::OPTIONAL),
        &lanes,
    );
    let shape = builder.freeze();
    assert_eq!(
        analysis_outputs(&g, shape.clone(), true).len(),
        analysis_outputs(&g, shape, false).len()
    );
}

/// Parses are the same with pruning on and off; only wasted analysis candidates go.
#[test]
fn parses_do_not_change() {
    let g = featureless_grammar();
    for word in [
        "tatasi", "pasa", "tasi", "tutúka", "tutuka", "pasapa", "tatika", "pasasa",
    ] {
        let on = Morpher::new(&g, usize::MAX).with_prune_disagreeing_copies(true);
        let off = Morpher::new(&g, usize::MAX).with_prune_disagreeing_copies(false);
        assert_eq!(
            on.parse_word(word).signature(),
            off.parse_word(word).signature(),
            "{word}"
        );
    }
    let m = Morpher::new(&g, usize::MAX);
    assert!(!m.parse_word("tatasi").signature().is_empty());
    assert!(!m.parse_word("tutúka").signature().is_empty());
}
