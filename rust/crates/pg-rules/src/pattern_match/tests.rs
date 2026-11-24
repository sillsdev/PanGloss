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
        <SegmentDefinition id="cH"><Representations><Representation>h</Representation></Representations><FeatureValue feature="featVoice" symbolValues="vNeg" /><FeatureValue feature="featCons" symbolValues="cNeg" /><FeatureValue feature="featLab" symbolValues="lNeg" /></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="bPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
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
      <PhonologicalRule id="rKleene" multipleApplicationOrder="simultaneous"><Name>voice t after initial stops and before vowels to the end</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="cT" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="cD" /></PhoneticSequence></PhoneticOutput>
          <Environment><LeftEnvironment><PhoneticTemplate initialBoundaryCondition="true"><PhoneticSequence><OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncStop" /></OptionalSegmentSequence></PhoneticSequence></PhoneticTemplate></LeftEnvironment><RightEnvironment><PhoneticTemplate finalBoundaryCondition="true"><PhoneticSequence><OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncV" /></OptionalSegmentSequence></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="rPair" multipleApplicationOrder="simultaneous"><Name>voice the pair pt before a vowel</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="cP" /><Segment segment="cT" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="cB" /><Segment segment="cD" /></PhoneticSequence></PhoneticOutput>
          <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="rPlus" multipleApplicationOrder="simultaneous"><Name>devoice b before a morpheme boundary</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="cB" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="cP" /></PhoneticSequence></PhoneticOutput>
          <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><BoundaryMarker boundary="bPlus" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="rInsert" multipleApplicationOrder="simultaneous"><Name>insert h between two stops</Name>
        <PhoneticInput><PhoneticSequence></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="cH" /></PhoneticSequence></PhoneticOutput>
          <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="tbl" phonologicalRules="rVoice rDevoice rInitial rOptionalRight rKleene rPair rPlus rInsert">
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

const WORDS: [&str; 30] = [
    "apa", "pap", "apap", "aapa", "pa", "ap", "papa", "ipai", "ab", "bab", "abb", "b", "tat",
    "tta", "ptat", "adai", "ad", "ada", "dai", "idd", "t", "ta", "ptai", "pta", "apta", "ptpta",
    "ptt", "pt", "tpt", "ddt",
];

/// Words for the rule whose right context is a morpheme boundary, which segment as boundary nodes.
const BOUNDARY_WORDS: [&str; 8] = ["b+a", "ab+", "ab+b", "b+b", "+b", "bb+a", "a+b+", "b"];

/// The sorted positions where the rule changes `word` and where `text` (its environment as pattern text) matches: segments changed and covered, or for an insertion the gaps inserted into and the starts of the zero-width matches.
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
    let segment_positions: Vec<usize> = (0..input.len())
        .filter(|&i| input.kind(i) == NodeKind::Segment)
        .collect();
    let (applied, insertion): (Vec<usize>, bool) = match synthesize(g, rule, &input).pop() {
        None => (Vec::new(), false),
        Some(out) if out.len() == input.len() => (
            segment_positions
                .iter()
                .enumerate()
                .filter(|&(_, &i)| out.node_lanes(i) != input.node_lanes(i))
                .map(|(segment, _)| segment)
                .collect(),
            false,
        ),
        Some(out) => {
            // Walk both shapes: an output node equal to the next input node is that node; any other is inserted before the segments consumed so far.
            let (mut next, mut consumed, mut gaps) = (0, 0, Vec::new());
            for j in 0..out.len() {
                if next < input.len() && out.node_lanes(j) == input.node_lanes(next) {
                    consumed += usize::from(input.kind(next) == NodeKind::Segment);
                    next += 1;
                } else {
                    gaps.push(consumed);
                }
            }
            (gaps, true)
        }
    };
    let parsed = parse_pattern(
        g,
        table,
        text,
        crate::pattern_text::boundary_char_def(table, "+"),
    )
    .unwrap_or_else(|e| panic!("{text}: {e:?}"));
    let mut matched: Vec<usize> = Vec::new();
    for m in match_pattern(g, TableId(0), &parsed, &input).unwrap() {
        if insertion {
            assert_eq!(m.start, m.end, "{text} on {word}");
            matched.push(m.start);
        } else {
            matched.extend(m.start..m.end);
        }
    }
    matched.sort_unstable();
    matched.dedup();
    (applied, matched)
}

/// Each case is a rule index, its own target and environment as pattern text, and the words to run it over.
fn check_cases(g: &Grammar, cases: &[(usize, &str, &[&str])]) {
    let mut applied_anywhere = vec![false; g.prules.len()];
    for &(rule, text, words) in cases {
        for word in words {
            let (applied, matched) = applied_and_matched(g, rule, text, word);
            assert_eq!(applied, matched, "rule {rule} `{text}` on {word}");
            applied_anywhere[rule] |= !applied.is_empty();
        }
    }
    for &(rule, text, _) in cases {
        assert!(
            applied_anywhere[rule],
            "rule {rule} `{text}` applies somewhere in its word list, so the comparison is not vacuous"
        );
    }
}

#[test]
fn a_rules_own_environment_text_marks_exactly_where_the_rule_applies() {
    let g = pg_grammar::load(GRAMMAR).unwrap();
    check_cases(
        &g,
        &[
            (0, "/p/ / [V] _ [V]", &WORDS),
            (1, "/b/ / _ #", &WORDS),
            (2, "/t/ / # ([Stop]) _", &WORDS),
            (3, "/d/ / _ ([V]) #", &WORDS),
            // A `( )*` on each side, anchored at both edges.
            (4, "/t/ / # ([Stop])* _ ([V])* #", &WORDS),
            // A two-segment target.
            (5, "/pt/ / _ [V]", &WORDS),
            // A boundary named in the context.
            (6, "/b/ / _ +", &BOUNDARY_WORDS),
            // An insertion: the target is empty, and the match is the gap.
            (7, "/ [Stop] _ [Stop]", &WORDS),
        ],
    );
}

#[test]
fn a_conformance_grammars_rules_agree_with_their_environment_text() {
    // `word-boundary-affix-rewrite` has two rules that rewrite `r` to `t` at a word edge, next to the class `Vowel`.
    let g = pg_conformance_fixtures::require_fixture("edge-cases", "word-boundary-affix-rewrite")
        .load_grammar();
    let words = [
        "ra", "ar", "rar", "rra", "arr", "tra", "ark", "r", "rk", "aar", "raa", "kra", "arar",
    ];
    check_cases(
        &g,
        &[
            (0, "/r/ / # _ [Vowel]", &words),
            (1, "/r/ / [Vowel] _ #", &words),
        ],
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
