//! Cross-check: `match` with a rule's own environment marks exactly the positions where the rule applies.

use super::*;
use crate::pattern_text::parse_pattern;
use crate::rewrite::synthesize;
use crate::shape_feat::segment_with_features;
use pg_grammar_model::model::PhonRuleDef;

/// A synthetic grammar with one simultaneous feature-changing rule per case below; each segment has its own feature bundle, so a literal in a rule matches only itself.
const GRAMMAR: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>EnvCrossCheck</Name>
    <PartsOfSpeech><PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featVoice"><Name>voice</Name>
        <Symbols><Symbol id="vPos">+</Symbol><Symbol id="vNeg">-</Symbol></Symbols>
      </SymbolicFeature>
      <SymbolicFeature id="featCons"><Name>cons</Name>
        <Symbols><Symbol id="cPos">+</Symbol><Symbol id="cNeg">-</Symbol></Symbols>
      </SymbolicFeature>
      <SymbolicFeature id="featLab"><Name>lab</Name>
        <Symbols><Symbol id="lPos">+</Symbol><Symbol id="lNeg">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="tbl">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featVoice" symbolValues="vNeg" /><FeatureValue feature="featCons" symbolValues="cPos" /><FeatureValue feature="featLab" symbolValues="lPos" /></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featVoice" symbolValues="vPos" /><FeatureValue feature="featCons" symbolValues="cPos" /><FeatureValue feature="featLab" symbolValues="lPos" /></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations><FeatureValue feature="featVoice" symbolValues="vNeg" /><FeatureValue feature="featCons" symbolValues="cPos" /><FeatureValue feature="featLab" symbolValues="lNeg" /></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations><FeatureValue feature="featVoice" symbolValues="vPos" /><FeatureValue feature="featCons" symbolValues="cPos" /><FeatureValue feature="featLab" symbolValues="lNeg" /></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations><FeatureValue feature="featVoice" symbolValues="vPos" /><FeatureValue feature="featCons" symbolValues="cNeg" /><FeatureValue feature="featLab" symbolValues="lNeg" /></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations><FeatureValue feature="featVoice" symbolValues="vPos" /><FeatureValue feature="featCons" symbolValues="cNeg" /><FeatureValue feature="featLab" symbolValues="lPos" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncV"><Name>V</Name><Segment segment="cA" /><Segment segment="cI" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncStop"><Name>Stop</Name><Segment segment="cP" /><Segment segment="cB" /><Segment segment="cT" /><Segment segment="cD" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="rVoice" multipleApplicationOrder="simultaneous"><Name>voice p between vowels</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="cP" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="cB" /></PhoneticSequence></PhoneticOutput>
          <Environment>
            <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
            <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
          </Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="rDevoice" multipleApplicationOrder="simultaneous"><Name>devoice b at the end</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="cB" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="cP" /></PhoneticSequence></PhoneticOutput>
          <Environment>
            <RightEnvironment><PhoneticTemplate finalBoundaryCondition="true" /></RightEnvironment>
          </Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="rInitial" multipleApplicationOrder="simultaneous"><Name>voice t after at most one initial stop</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="cT" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="cD" /></PhoneticSequence></PhoneticOutput>
          <Environment>
            <LeftEnvironment><PhoneticTemplate initialBoundaryCondition="true"><PhoneticSequence><OptionalSegmentSequence min="0" max="1"><SimpleContext naturalClass="ncStop" /></OptionalSegmentSequence></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
          </Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="rOptionalRight" multipleApplicationOrder="simultaneous"><Name>devoice d before an optional vowel and the end</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="cD" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="cT" /></PhoneticSequence></PhoneticOutput>
          <Environment>
            <RightEnvironment><PhoneticTemplate finalBoundaryCondition="true"><PhoneticSequence><OptionalSegmentSequence min="0" max="1"><SimpleContext naturalClass="ncV" /></OptionalSegmentSequence></PhoneticSequence></PhoneticTemplate></RightEnvironment>
          </Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="tbl" phonologicalRules="rVoice rDevoice rInitial rOptionalRight">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="eRoot" partOfSpeech="posN">
            <Allomorphs><Allomorph id="aRoot"><PhoneticShape>apa</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>ROOT</MorphemeId><Gloss>root</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;

const WORDS: [&str; 20] = [
    "apa", "pap", "apap", "aapa", "pa", "ap", "papa", "ipai", "ab", "bab", "abb", "b", "tat",
    "tta", "ptat", "adai", "ad", "ada", "dai", "idd",
];

/// Segment positions where the rule changes the word, and where `text` (the rule's own environment) matches a one-segment target.
fn applied_and_matched(
    g: &Grammar,
    rule: usize,
    text: &str,
    word: &str,
) -> (Vec<usize>, Vec<usize>) {
    let PhonRuleDef::Rewrite(rule) = &g.prules[rule] else {
        panic!("rewrite rule")
    };
    let table = &g.char_tables[0];
    let input = segment_with_features(g, table, word).unwrap();
    let applied: Vec<usize> = match synthesize(g, rule, &input).pop() {
        None => Vec::new(),
        Some(out) => (0..input.len())
            .filter(|&i| input.kind(i) == NodeKind::Segment)
            .enumerate()
            .filter(|&(_, i)| out.node_lanes(i) != input.node_lanes(i))
            .map(|(segment, _)| segment)
            .collect(),
    };
    let parsed = parse_pattern(g, table, text, None).unwrap_or_else(|e| panic!("{text}: {e:?}"));
    let matched = match_pattern(g, TableId(0), &parsed, &input)
        .unwrap()
        .into_iter()
        .map(|m| {
            assert_eq!(m.end, m.start + 1, "{text} on {word}");
            m.start
        })
        .collect();
    (applied, matched)
}

#[test]
fn a_rules_own_environment_text_marks_exactly_where_the_rule_applies() {
    let g = pg_grammar::load(GRAMMAR).unwrap();
    // (rule index, the rule's target and environment written as pattern text)
    let cases = [
        (0, "/p/ / [V] _ [V]"),
        (1, "/b/ / _ #"),
        (2, "/t/ / # ([Stop]) _"),
        (3, "/d/ / _ ([V]) #"),
    ];
    let mut applied_anywhere = [false; 4];
    for (rule, text) in cases {
        for word in WORDS {
            let (applied, matched) = applied_and_matched(&g, rule, text, word);
            assert_eq!(applied, matched, "rule {rule} `{text}` on {word}");
            applied_anywhere[rule] |= !applied.is_empty();
        }
    }
    assert_eq!(
        applied_anywhere, [true; 4],
        "every rule applies somewhere in the word list, so the comparison is not vacuous"
    );
}

#[test]
fn the_cross_check_sees_a_wrong_environment() {
    let g = pg_grammar::load(GRAMMAR).unwrap();
    let (applied, matched) = applied_and_matched(&g, 0, "/p/ / [V] _", "apap");
    assert_ne!(
        applied, matched,
        "an environment missing its right side matches more"
    );
}
