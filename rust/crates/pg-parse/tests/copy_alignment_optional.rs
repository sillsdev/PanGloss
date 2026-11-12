//! Divergence 049: copies holding optional nodes are aligned the way C# `CanAlign` does, with the optional nodes next to each copy's boundaries free to belong to either neighbour. The grammar is invented: a zero-or-more variable copied twice (`redup`), an obligatory syncope `u -> 0 / C _ C#`, and a root `salum`.

use pg_grammar::load;
use pg_grammar_model::model::{Grammar, StratumId};
use pg_parse::Morpher;
use pg_rules::stratum::{AnalyzerConfig, WordBudget};
use pg_rules::Word;

fn grammar() -> Grammar {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>SyncopeRedup</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<PhonologicalFeatureSystem><SymbolicFeature id="fCons"><Name>cons</Name><Symbols><Symbol id="consP">+</Symbol><Symbol id="consM">-</Symbol></Symbols></SymbolicFeature><SymbolicFeature id="fPlace"><Name>place</Name><Symbols>@PL@</Symbols></SymbolicFeature></PhonologicalFeatureSystem>
<CharacterDefinitionTable id="tbl"><Name>Main</Name><SegmentDefinitions>@SEGS@</SegmentDefinitions></CharacterDefinitionTable>
<NaturalClasses>
<FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
<FeatureNaturalClass id="ncU"><Name>U</Name><FeatureValue feature="fPlace" symbolValues="pl_u"/></FeatureNaturalClass>
<FeatureNaturalClass id="ncC"><Name>C</Name><FeatureValue feature="fCons" symbolValues="consP"/></FeatureNaturalClass>
</NaturalClasses>
<PhonologicalRuleDefinitions><PhonologicalRule id="prSync"><Name>syncope</Name><PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncU"/></PhoneticSequence></PhoneticInput><PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence/></PhoneticOutput><Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncC"/></PhoneticSequence></PhoneticTemplate></LeftEnvironment><RightEnvironment><PhoneticTemplate finalBoundaryCondition="true"><PhoneticSequence><SimpleContext naturalClass="ncC"/></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment></PhonologicalSubrule></PhonologicalSubrules></PhonologicalRule></PhonologicalRuleDefinitions>
<Strata><Stratum characterDefinitionTable="tbl" morphologicalRuleOrder="unordered" morphologicalRules="mrRedup" phonologicalRules="prSync"><Name>Main</Name>
<MorphologicalRuleDefinitions><MorphologicalRule id="mrRedup" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>redup</Name><MorphologicalSubrules><MorphologicalSubrule id="mrRedups"><MorphologicalInput><PhoneticSequence id="rp"><OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncAny"/></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="rp"/><CopyFromInput index="rp"/></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>REDUP</MorphemeId></MorphologicalRule></MorphologicalRuleDefinitions>
<LexicalEntries><LexicalEntry id="e1" partOfSpeech="posV"><Allomorphs><Allomorph id="a1"><PhoneticShape>salum</PhoneticShape></Allomorph></Allomorphs><MorphemeId>SALUM</MorphemeId></LexicalEntry></LexicalEntries>
</Stratum></Strata></Language></HermitCrabInput>"#
        .replace("@SEGS@", r#"<SegmentDefinition id="c_p"><Representations><Representation>p</Representation></Representations><FeatureValue feature="fCons" symbolValues="consP"/><FeatureValue feature="fPlace" symbolValues="pl_p"/></SegmentDefinition><SegmentDefinition id="c_t"><Representations><Representation>t</Representation></Representations><FeatureValue feature="fCons" symbolValues="consP"/><FeatureValue feature="fPlace" symbolValues="pl_t"/></SegmentDefinition><SegmentDefinition id="c_k"><Representations><Representation>k</Representation></Representations><FeatureValue feature="fCons" symbolValues="consP"/><FeatureValue feature="fPlace" symbolValues="pl_k"/></SegmentDefinition><SegmentDefinition id="c_s"><Representations><Representation>s</Representation></Representations><FeatureValue feature="fCons" symbolValues="consP"/><FeatureValue feature="fPlace" symbolValues="pl_s"/></SegmentDefinition><SegmentDefinition id="c_l"><Representations><Representation>l</Representation></Representations><FeatureValue feature="fCons" symbolValues="consP"/><FeatureValue feature="fPlace" symbolValues="pl_l"/></SegmentDefinition><SegmentDefinition id="c_m"><Representations><Representation>m</Representation></Representations><FeatureValue feature="fCons" symbolValues="consP"/><FeatureValue feature="fPlace" symbolValues="pl_m"/></SegmentDefinition><SegmentDefinition id="c_n"><Representations><Representation>n</Representation></Representations><FeatureValue feature="fCons" symbolValues="consP"/><FeatureValue feature="fPlace" symbolValues="pl_n"/></SegmentDefinition><SegmentDefinition id="c_a"><Representations><Representation>a</Representation></Representations><FeatureValue feature="fCons" symbolValues="consM"/><FeatureValue feature="fPlace" symbolValues="pl_a"/></SegmentDefinition><SegmentDefinition id="c_i"><Representations><Representation>i</Representation></Representations><FeatureValue feature="fCons" symbolValues="consM"/><FeatureValue feature="fPlace" symbolValues="pl_i"/></SegmentDefinition><SegmentDefinition id="c_u"><Representations><Representation>u</Representation></Representations><FeatureValue feature="fCons" symbolValues="consM"/><FeatureValue feature="fPlace" symbolValues="pl_u"/></SegmentDefinition>"#)
        .replace("@PL@", r#"<Symbol id="pl_p">p</Symbol><Symbol id="pl_t">t</Symbol><Symbol id="pl_k">k</Symbol><Symbol id="pl_s">s</Symbol><Symbol id="pl_l">l</Symbol><Symbol id="pl_m">m</Symbol><Symbol id="pl_n">n</Symbol><Symbol id="pl_a">a</Symbol><Symbol id="pl_i">i</Symbol><Symbol id="pl_u">u</Symbol>"#);
    load(&xml).unwrap_or_else(|e| panic!("grammar failed to load: {e}"))
}

fn redup_outputs(g: &Grammar, word: &str, prune: bool) -> usize {
    let shape = pg_rules::shape_feat::segment_with_features(g, &g.char_tables[0], word)
        .unwrap_or_else(|_| panic!("{word:?} should segment"));
    let config = AnalyzerConfig {
        prune_disagreeing_copies: prune,
        ..AnalyzerConfig::default()
    };
    let stratum = StratumId(0);
    pg_rules::stratum::analyze_stratum(
        g,
        stratum,
        Word::new(shape, stratum),
        &config,
        &WordBudget::new(usize::MAX),
    )
    .words
    .into_iter()
    .filter(|w: &Word| !w.trail.is_empty())
    .count()
}

/// The reduplicated and syncopated word `salumsalm`: with the restored `u?` in the second copy only the true split `salum|salum` aligns; every other split is refused, so the number of candidates drops.
#[test]
fn syncope_reduplication_keeps_only_the_true_split() {
    let g = grammar();
    let off = redup_outputs(&g, "salumsalm", false);
    let on = redup_outputs(&g, "salumsalm", true);
    assert!(off >= 5, "unpruned proposes many splits: {off}");
    assert_eq!(on, 1, "only the true split survives (unpruned {off})");
}

/// The true word still parses to its single reading, pruned or not.
#[test]
fn syncope_reduplication_reading_is_unchanged() {
    let g = grammar();
    for word in ["salumsalm", "salumsalum", "salum", "salsal"] {
        let on = Morpher::new(&g, usize::MAX).with_prune_disagreeing_copies(true);
        let off = Morpher::new(&g, usize::MAX).with_prune_disagreeing_copies(false);
        assert_eq!(
            on.parse_word(word).signature(),
            off.parse_word(word).signature(),
            "{word}"
        );
    }
    let m = Morpher::new(&g, usize::MAX);
    assert!(!m.parse_word("salumsalm").signature().is_empty());
}
