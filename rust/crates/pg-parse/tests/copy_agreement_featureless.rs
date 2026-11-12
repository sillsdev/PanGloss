//! A grammar with no phonological features still prunes disagreeing copies, because C# keeps the segment spelling (`StrRep`) on every segment node when the feature system is empty and so `CanAlign` compares characters there (`AnalysisMorphologicalTransform.CanAlign`, `FeatureStruct.IsUnifiable`). All words and morphemes below are invented.

use pg_grammar::load;
use pg_grammar_model::model::{Grammar, StratumId};
use pg_parse::Morpher;
use pg_rules::stratum::{AnalyzerConfig, WordBudget};
use pg_rules::Word;
use pg_shape::{CdBits, CdSet, NodeFlags, NodeKind, Shape, ShapeBuilder};

/// Segments `p t k s a i u` where `u` also spells `ú`; a CV copy rule `redCV` (copy C, V, C, V, then the rest); roots `tasi`, `tuka`, `pasa`.
fn featureless_grammar() -> Grammar {
    copy_grammar(false)
}

/// The same grammar with one feature, `cons`, that is `+` on every consonant and `-` on every vowel, so copies that differ in spelling still unify.
fn one_feature_grammar() -> Grammar {
    copy_grammar(true)
}

fn copy_grammar(with_feature: bool) -> Grammar {
    copy_grammar_with(with_feature, RED_CV_RULE)
}

/// `redCV`: copy C, V, C, V, then the rest.
const RED_CV_RULE: &str = r#"<MorphologicalRule id="mrRed"><Name>redCV</Name><MorphemeId>RED</MorphemeId>
    <MorphologicalSubrules><MorphologicalSubrule id="sub1">
      <MorphologicalInput>
        <PhoneticSequence id="c"><SimpleContext naturalClass="ncC"/></PhoneticSequence>
        <PhoneticSequence id="v"><SimpleContext naturalClass="ncV"/></PhoneticSequence>
        <PhoneticSequence id="rest"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny"/></OptionalSegmentSequence></PhoneticSequence>
      </MorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="c"/><CopyFromInput index="v"/><CopyFromInput index="c"/><CopyFromInput index="v"/><CopyFromInput index="rest"/></MorphologicalOutput>
    </MorphologicalSubrule></MorphologicalSubrules>
  </MorphologicalRule>"#;

/// `dbl`: a zero-or-more variable copied twice and nothing else (a FieldWorks "stem, X X" process).
const DOUBLE_RULE: &str = r#"<MorphologicalRule id="mrRed"><Name>dbl</Name><MorphemeId>RED</MorphemeId>
    <MorphologicalSubrules><MorphologicalSubrule id="sub1">
      <MorphologicalInput>
        <PhoneticSequence id="x"><OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncAny"/></OptionalSegmentSequence></PhoneticSequence>
      </MorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="x"/><CopyFromInput index="x"/></MorphologicalOutput>
    </MorphologicalSubrule></MorphologicalSubrules>
  </MorphologicalRule>"#;

fn copy_grammar_with(with_feature: bool, rule: &str) -> Grammar {
    let seg = |id: &str, reps: &[&str], cons: bool| {
        let reps: String = reps
            .iter()
            .map(|r| format!("<Representation>{r}</Representation>"))
            .collect();
        let value = match (with_feature, cons) {
            (false, _) => "",
            (true, true) => r#"<FeatureValue feature="fCons" symbolValues="sPlus"/>"#,
            (true, false) => r#"<FeatureValue feature="fCons" symbolValues="sMinus"/>"#,
        };
        format!(
            r#"<SegmentDefinition id="{id}"><Representations>{reps}</Representations>{value}</SegmentDefinition>"#
        )
    };
    let segments = [
        seg("cp", &["p"], true),
        seg("ct", &["t"], true),
        seg("ck", &["k"], true),
        seg("cs", &["s"], true),
        seg("ca", &["a"], false),
        seg("ci", &["i"], false),
        seg("cu", &["u", "ú"], false),
    ]
    .concat();
    let feature_system = if with_feature {
        r#"<PhonologicalFeatureSystem><SymbolicFeature id="fCons"><Name>cons</Name><Symbols><Symbol id="sPlus">+</Symbol><Symbol id="sMinus">-</Symbol></Symbols></SymbolicFeature></PhonologicalFeatureSystem>"#
    } else {
        ""
    };
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>FeaturelessCopy</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
{feature_system}<CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>{segments}</SegmentDefinitions></CharacterDefinitionTable>
<NaturalClasses>
  <SegmentNaturalClass id="ncC"><Name>C</Name><Segment segment="cp"/><Segment segment="ct"/><Segment segment="ck"/><Segment segment="cs"/></SegmentNaturalClass>
  <SegmentNaturalClass id="ncV"><Name>V</Name><Segment segment="ca"/><Segment segment="ci"/><Segment segment="cu"/></SegmentNaturalClass>
  <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
</NaturalClasses>
<Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrRed"><Name>S</Name>
<MorphologicalRuleDefinitions>
  {rule}
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
        &WordBudget::new(usize::MAX),
    )
    .words
    .into_iter()
    .filter(|word| !word.trail.is_empty())
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

/// With features a segment carries no spelling, so copies that differ in spelling but unify in features keep the match.
#[test]
fn spelling_differences_are_not_refused_when_the_grammar_has_features() {
    let g = one_feature_grammar();
    assert!(
        g.char_tables[0].unif_closure_rows().is_some(),
        "this grammar must be in the feature regime"
    );
    for word in ["pasapa", "tatika"] {
        let off = analysis_outputs(&g, shape_of(&g, word), false);
        let on = analysis_outputs(&g, shape_of(&g, word), true);
        assert_eq!(off.len(), 1, "{word}: unpruned analysis proposes the split");
        assert_eq!(
            on.len(),
            1,
            "{word}: spelling alone must not refuse a match"
        );
    }
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

/// Replaces the consonants of `pasapa` (node 1 is the first copy's, node 3 the second copy's) with class-derived nodes that have no fixed character; `None` keeps the spelled consonant.
fn with_abstract_consonants(g: &Grammar, first: Option<CdSet>, second: Option<CdSet>) -> Shape {
    let mut shape = shape_of(g, "pasapa");
    for (index, cd_set) in [(1, first), (3, second)] {
        let Some(cd_set) = cd_set else { continue };
        let lanes = shape.node_lanes(index).to_vec();
        let mut builder = ShapeBuilder::from_shape(&shape);
        builder.delete(index);
        builder.insert_with_set(index, NodeFlags::EMPTY, &lanes, cd_set);
        shape = builder.freeze();
    }
    shape
}

fn members(g: &Grammar, spellings: &[&str]) -> CdSet {
    CdSet::Members(CdBits::from_ids(
        spellings
            .iter()
            .map(|s| shape_of(g, &format!("{s}a")).char_def(1)),
    ))
}

/// Pruning on versus off for a shape: (analyses kept when unpruned, analyses kept when pruned).
fn kept(g: &Grammar, shape: Shape) -> (usize, usize) {
    (
        analysis_outputs(g, shape.clone(), false).len(),
        analysis_outputs(g, shape, true).len(),
    )
}

/// A class-derived node with no member restriction could be any character: unsure, so it keeps the match, in either copy.
#[test]
fn an_unrestricted_abstract_node_keeps_the_match() {
    let g = featureless_grammar();
    let second = with_abstract_consonants(&g, None, Some(CdSet::Unrestricted));
    assert_eq!(second.char_def(3), pg_shape::NO_CHAR_DEF);
    assert_eq!(kept(&g, second), (1, 1));
    let first = with_abstract_consonants(&g, Some(CdSet::Unrestricted), None);
    assert_eq!(first.char_def(1), pg_shape::NO_CHAR_DEF);
    assert_eq!(kept(&g, first), (1, 1));
}

/// A class-derived node whose members include the other copy's character could agree, so it keeps the match, in either copy.
#[test]
fn an_abstract_node_that_includes_the_character_keeps_the_match() {
    let g = featureless_grammar();
    let second = with_abstract_consonants(&g, None, Some(members(&g, &["p", "s"])));
    assert_eq!(kept(&g, second), (1, 1));
    let first = with_abstract_consonants(&g, Some(members(&g, &["p", "s"])), None);
    assert_eq!(kept(&g, first), (1, 1));
}

/// A class-derived node whose members exclude the other copy's character cannot agree, so the match goes, in either copy.
#[test]
fn an_abstract_node_that_excludes_the_character_is_pruned() {
    let g = featureless_grammar();
    let second = with_abstract_consonants(&g, None, Some(members(&g, &["s", "k"])));
    assert_eq!(kept(&g, second), (1, 0));
    let first = with_abstract_consonants(&g, Some(members(&g, &["t", "k"])), None);
    assert_eq!(kept(&g, first), (1, 0));
}

/// Two class-derived copies agree when their member sets intersect and disagree when they are disjoint.
#[test]
fn two_abstract_nodes_agree_exactly_when_their_members_intersect() {
    let g = featureless_grammar();
    let overlapping = with_abstract_consonants(
        &g,
        Some(members(&g, &["p", "s"])),
        Some(members(&g, &["s", "k"])),
    );
    assert_eq!(kept(&g, overlapping), (1, 1));
    let disjoint = with_abstract_consonants(
        &g,
        Some(members(&g, &["p", "t"])),
        Some(members(&g, &["s", "k"])),
    );
    assert_eq!(kept(&g, disjoint), (1, 0));
}

/// Rebuilds `pasapa` with node 3 (the second copy's consonant) optional, spelled like node `like`.
fn with_optional_second_consonant(g: &Grammar, like: usize) -> Shape {
    let shape = shape_of(g, "pasapa");
    let char_def = shape.char_def(like);
    let lanes = shape.node_lanes(like).to_vec();
    let mut builder = ShapeBuilder::from_shape(&shape);
    builder.delete(3);
    builder.insert(
        3,
        NodeKind::Segment,
        char_def,
        NodeFlags(NodeFlags::OPTIONAL),
        &lanes,
    );
    builder.freeze()
}

/// A copy made only of an optional node can be an empty capture in C# (the matcher skips optional nodes at a group's edge), and C# keeps a match with an empty capture, so the split stays whatever the node's spelling (divergence 049).
#[test]
fn a_copy_of_only_an_optional_node_keeps_the_match() {
    let g = featureless_grammar();
    let shape = with_optional_second_consonant(&g, 3);
    assert_ne!(shape.char_def(3), shape.char_def(1));
    assert_eq!(kept(&g, shape), (1, 1));
    assert_eq!(kept(&g, with_optional_second_consonant(&g, 1)), (1, 1));
}

/// `tasitasi` with the second copy's `a` made an optional `u`: the copies `tasi` and `tu?si` cannot line up (`u?` may be passed over or stand for a node of the other copy, and no node of the other copy is a `u`), so only the true split is at stake and it is refused; the empty-copy splits are refused too.
#[test]
fn an_optional_node_that_cannot_stand_for_its_partner_refuses_the_split() {
    let g = copy_grammar_with(false, DOUBLE_RULE);
    let shape = shape_of(&g, "tasitasi");
    let char_def = shape_of(&g, "tusi").char_def(2);
    let lanes = shape.node_lanes(6).to_vec();
    let mut builder = ShapeBuilder::from_shape(&shape);
    builder.delete(6);
    builder.insert(
        6,
        NodeKind::Segment,
        char_def,
        NodeFlags(NodeFlags::OPTIONAL),
        &lanes,
    );
    let (off, on) = kept(&g, builder.freeze());
    assert!(off >= 1, "unpruned proposes the split: {off}");
    assert_eq!(on, 0, "the copies cannot line up");
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

/// Leak 2 (empty copy): the split "first copy = whole word, second copy empty" (and its mirror) used to be kept because an empty capture was undecidable. A non-empty copy cannot equal an empty one, so only the true halves survive.
#[test]
fn an_empty_second_copy_is_refused() {
    let g = copy_grammar_with(false, DOUBLE_RULE);
    let off = analysis_outputs(&g, shape_of(&g, "tasitasi"), false);
    let on = analysis_outputs(&g, shape_of(&g, "tasitasi"), true);
    assert!(
        off.len() >= 3,
        "unpruned proposes the empty splits too: {}",
        off.len()
    );
    assert_eq!(on.len(), 1, "only tasi|tasi survives");
    // A word with no reduplication keeps no candidate at all (this was the +9% floor on every plain word).
    assert_eq!(analysis_outputs(&g, shape_of(&g, "tasipa"), true).len(), 0);
}
