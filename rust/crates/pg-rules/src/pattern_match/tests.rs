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
      <FeatureNaturalClass id="ncVowelF"><Name>vowelish</Name><FeatureValue feature="featCons" symbolValues="cNeg" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncConsPlus"><Name>consonantal</Name><FeatureValue feature="featCons" symbolValues="cPos" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncAnyF"><Name>anything</Name></FeatureNaturalClass>
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
      <PhonologicalRule id="rBundle" multipleApplicationOrder="simultaneous"><Name>voice p between vowel-like segments</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="cP" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="cB" /></PhoneticSequence></PhoneticOutput>
          <Environment>
            <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncVowelF" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
            <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncVowelF" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
          </Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="rAlphaTarget" multipleApplicationOrder="simultaneous"><Name>a consonant becomes h before a segment of its own voice</Name>
        <VariableFeatures><VariableFeature id="varVoice" name="alpha" phonologicalFeature="featVoice" /></VariableFeatures>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncConsPlus"><AlphaVariables><AlphaVariable variableFeature="varVoice" /></AlphaVariables></SimpleContext></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="cH" /></PhoneticSequence></PhoneticOutput>
          <Environment>
            <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncAnyF"><AlphaVariables><AlphaVariable variableFeature="varVoice" /></AlphaVariables></SimpleContext></PhoneticSequence></PhoneticTemplate></RightEnvironment>
          </Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="rAlphaSides" multipleApplicationOrder="simultaneous"><Name>voice p between segments of one voice</Name>
        <VariableFeatures><VariableFeature id="varVoice" name="alpha" phonologicalFeature="featVoice" /></VariableFeatures>
        <PhoneticInput><PhoneticSequence><Segment segment="cP" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="cB" /></PhoneticSequence></PhoneticOutput>
          <Environment>
            <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncAnyF"><AlphaVariables><AlphaVariable variableFeature="varVoice" /></AlphaVariables></SimpleContext></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
            <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncAnyF"><AlphaVariables><AlphaVariable variableFeature="varVoice" /></AlphaVariables></SimpleContext></PhoneticSequence></PhoneticTemplate></RightEnvironment>
          </Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="rDisagree" multipleApplicationOrder="simultaneous"><Name>voice t between segments of opposite voice</Name>
        <VariableFeatures><VariableFeature id="varVoice" name="alpha" phonologicalFeature="featVoice" /></VariableFeatures>
        <PhoneticInput><PhoneticSequence><Segment segment="cT" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="cD" /></PhoneticSequence></PhoneticOutput>
          <Environment>
            <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncAnyF"><AlphaVariables><AlphaVariable variableFeature="varVoice" /></AlphaVariables></SimpleContext></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
            <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncAnyF"><AlphaVariables><AlphaVariable variableFeature="varVoice" polarity="minus" /></AlphaVariables></SimpleContext></PhoneticSequence></PhoneticTemplate></RightEnvironment>
          </Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="tbl" phonologicalRules="rVoice rDevoice rInitial rOptionalRight rKleene rPair rPlus rInsert rBundle rAlphaTarget rAlphaSides rDisagree">
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

const WORDS: [&str; 41] = [
    "apa", "pap", "apap", "aapa", "pa", "ap", "papa", "ipai", "ab", "bab", "abb", "b", "tat",
    "tta", "ptat", "adai", "ad", "ada", "dai", "idd", "t", "ta", "ptai", "pta", "apta", "ptpta",
    "ptt", "pt", "tpt", "ddt", "bpd", "tpd", "dpt", "apt", "tda", "tdt", "bta", "pdp", "ppb",
    "iib", "tit",
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
            // Feature bundles and alpha variables, which must agree with the rule's own alpha agreement.
            (8, "/p/ / [-cons] _ [-cons]", &WORDS),
            (9, "[+cons αvoice] / _ [αvoice]", &WORDS),
            (10, "/p/ / [αvoice] _ [αvoice]", &WORDS),
            (11, "/t/ / [αvoice] _ [-αvoice]", &WORDS),
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

/// A grammar for the counter-cases: segments with no `voice` value (`h`, `x`), a three-valued feature (`ht`), a feature whose symbols run `-` then `+` (`cons`), with and without a `defaultSymbol` on voice.
const RG: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ReviewAlpha</Name>
    <PartsOfSpeech><PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featVoice"VOICEDEF><Name>voice</Name>
        <Symbols><Symbol id="vPos">+</Symbol><Symbol id="vNeg">-</Symbol></Symbols>
      </SymbolicFeature>
      <SymbolicFeature id="featCons"><Name>cons</Name>
        <Symbols><Symbol id="cNeg">-</Symbol><Symbol id="cPos">+</Symbol></Symbols>
      </SymbolicFeature>
      <SymbolicFeature id="featHt"><Name>ht</Name>
        <Symbols><Symbol id="hHi">hi</Symbol><Symbol id="hMid">mid</Symbol><Symbol id="hLo">lo</Symbol></Symbols>
      </SymbolicFeature>
      <SymbolicFeature id="featMk"><Name>mk</Name>
        <Symbols><Symbol id="mPos">+</Symbol><Symbol id="mNeg">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="tbl">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featVoice" symbolValues="vNeg" /><FeatureValue feature="featCons" symbolValues="cPos" /><FeatureValue feature="featMk" symbolValues="mNeg" /></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featVoice" symbolValues="vPos" /><FeatureValue feature="featCons" symbolValues="cPos" /><FeatureValue feature="featMk" symbolValues="mNeg" /></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations><FeatureValue feature="featVoice" symbolValues="vPos" /><FeatureValue feature="featCons" symbolValues="cNeg" /><FeatureValue feature="featHt" symbolValues="hLo" /><FeatureValue feature="featMk" symbolValues="mNeg" /></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations><FeatureValue feature="featVoice" symbolValues="vPos" /><FeatureValue feature="featCons" symbolValues="cNeg" /><FeatureValue feature="featHt" symbolValues="hHi" /><FeatureValue feature="featMk" symbolValues="mNeg" /></SegmentDefinition>
        <SegmentDefinition id="cE"><Representations><Representation>e</Representation></Representations><FeatureValue feature="featVoice" symbolValues="vPos" /><FeatureValue feature="featCons" symbolValues="cNeg" /><FeatureValue feature="featHt" symbolValues="hMid" /><FeatureValue feature="featMk" symbolValues="mNeg" /></SegmentDefinition>
        <SegmentDefinition id="cH"><Representations><Representation>h</Representation></Representations><FeatureValue feature="featCons" symbolValues="cNeg" /><FeatureValue feature="featMk" symbolValues="mNeg" /></SegmentDefinition>
        <SegmentDefinition id="cX"><Representations><Representation>x</Representation></Representations><FeatureValue feature="featCons" symbolValues="cPos" /><FeatureValue feature="featMk" symbolValues="mNeg" /></SegmentDefinition>
        <SegmentDefinition id="cZ"><Representations><Representation>z</Representation></Representations><FeatureValue feature="featVoice" symbolValues="vNeg" /><FeatureValue feature="featCons" symbolValues="cPos" /><FeatureValue feature="featHt" symbolValues="hLo" /><FeatureValue feature="featMk" symbolValues="mPos" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncVp"><Name>vp</Name><FeatureValue feature="featVoice" symbolValues="vPos" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncVm"><Name>vm</Name><FeatureValue feature="featVoice" symbolValues="vNeg" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncCp"><Name>cp</Name><FeatureValue feature="featCons" symbolValues="cPos" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncAny"><Name>any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
RULES
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="tbl" phonologicalRules="STRATUM">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="eRoot" partOfSpeech="posN">
            <Allomorphs><Allomorph id="aRoot"><PhoneticShape>pa</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;

fn rv_ctx(nc: &str, vars: &[(&str, bool)]) -> String {
    if vars.is_empty() {
        return format!(r#"<SimpleContext naturalClass="{nc}" />"#);
    }
    let v: String = vars
        .iter()
        .map(|(id, plus)| {
            if *plus {
                format!(r#"<AlphaVariable variableFeature="{id}" />"#)
            } else {
                format!(r#"<AlphaVariable variableFeature="{id}" polarity="minus" />"#)
            }
        })
        .collect();
    format!(
        r#"<SimpleContext naturalClass="{nc}"><AlphaVariables>{v}</AlphaVariables></SimpleContext>"#
    )
}

fn rv_rule(id: &str, input: &str, left: Option<String>, right: Option<String>) -> String {
    let side = |tag: &str, s: &Option<String>| {
        match s {
        None => String::new(),
        Some(c) => format!("<{tag}><PhoneticTemplate><PhoneticSequence>{c}</PhoneticSequence></PhoneticTemplate></{tag}>"),
    }
    };
    format!(
        r#"<PhonologicalRule id="{id}" multipleApplicationOrder="simultaneous"><Name>{id}</Name>
        <VariableFeatures><VariableFeature id="vV" name="alpha" phonologicalFeature="featVoice" /><VariableFeature id="vC" name="beta" phonologicalFeature="featCons" /><VariableFeature id="vH" name="gamma" phonologicalFeature="featHt" /></VariableFeatures>
        <PhoneticInput><PhoneticSequence>{input}</PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="cZ" /></PhoneticSequence></PhoneticOutput>
          <Environment>{}{}</Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>"#,
        side("LeftEnvironment", &left),
        side("RightEnvironment", &right)
    )
}

fn rv_run(voice_default: bool) {
    let p = r#"<Segment segment="cP" />"#.to_string();
    let cases: Vec<(String, &str)> = vec![
        (rv_rule("r0", &rv_ctx("ncVp", &[]), None, None), "[+voice]"),
        (rv_rule("r1", &rv_ctx("ncVm", &[]), None, None), "[-voice]"),
        (
            rv_rule(
                "r2",
                &p,
                Some(rv_ctx("ncAny", &[("vV", true)])),
                Some(rv_ctx("ncAny", &[("vV", true)])),
            ),
            "/p/ / [αvoice] _ [αvoice]",
        ),
        (
            rv_rule(
                "r3",
                &p,
                Some(rv_ctx("ncAny", &[("vV", true)])),
                Some(rv_ctx("ncAny", &[("vV", false)])),
            ),
            "/p/ / [αvoice] _ [-αvoice]",
        ),
        (
            rv_rule(
                "r4",
                &rv_ctx("ncCp", &[("vV", true)]),
                None,
                Some(rv_ctx("ncAny", &[("vV", true)])),
            ),
            "[+cons αvoice] / _ [αvoice]",
        ),
        (
            rv_rule(
                "r5",
                &p,
                Some(rv_ctx("ncAny", &[("vH", true)])),
                Some(rv_ctx("ncAny", &[("vH", false)])),
            ),
            "/p/ / [αht] _ [-αht]",
        ),
        (
            rv_rule(
                "r6",
                &p,
                Some(rv_ctx("ncAny", &[("vH", true)])),
                Some(rv_ctx("ncAny", &[("vH", true)])),
            ),
            "/p/ / [αht] _ [αht]",
        ),
        (
            rv_rule(
                "r7",
                &p,
                Some(rv_ctx("ncAny", &[("vV", true), ("vC", true)])),
                Some(rv_ctx("ncAny", &[("vV", true), ("vC", true)])),
            ),
            "/p/ / [αvoice βcons] _ [αvoice βcons]",
        ),
        (
            rv_rule(
                "r8",
                &rv_ctx("ncAny", &[("vV", true)]),
                Some(rv_ctx("ncAny", &[("vV", true)])),
                None,
            ),
            "[αvoice] / [αvoice] _",
        ),
        (
            rv_rule(
                "r9",
                &rv_ctx("ncCp", &[("vV", true)]),
                Some(rv_ctx("ncAny", &[("vV", true)])),
                Some(rv_ctx("ncAny", &[("vV", false)])),
            ),
            "[+cons αvoice] / [αvoice] _ [-αvoice]",
        ),
        (
            rv_rule(
                "r10",
                &rv_ctx("ncCp", &[("vV", false)]),
                None,
                Some(rv_ctx("ncAny", &[("vV", true)])),
            ),
            "[+cons -αvoice] / _ [αvoice]",
        ),
        (
            rv_rule(
                "r11",
                &rv_ctx("ncAny", &[("vH", true)]),
                None,
                Some(rv_ctx("ncAny", &[("vH", true)])),
            ),
            "[αht] / _ [αht]",
        ),
    ];
    let rules: String = cases
        .iter()
        .map(|(r, _)| r.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let stratum: String = (0..cases.len())
        .map(|i| format!("r{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let xml = RG
        .replace("RULES", &rules)
        .replace("STRATUM", &stratum)
        .replace(
            "VOICEDEF",
            if voice_default {
                r#" defaultSymbol="vNeg""#
            } else {
                ""
            },
        );
    let g = pg_grammar::load(&xml).unwrap();
    let alpha = ['p', 'b', 'a', 'i', 'e', 'h', 'x'];
    let mut words = Vec::new();
    for a in alpha {
        for b in alpha {
            words.push(format!("{a}{b}"));
            for c in alpha {
                words.push(format!("{a}{b}{c}"));
            }
        }
    }
    let mut bad = 0;
    for (i, (_, text)) in cases.iter().enumerate() {
        let mut n = 0;
        let mut any = false;
        for w in &words {
            let (applied, matched) = applied_and_matched(&g, i, text, w);
            any |= !applied.is_empty();
            let diff: Vec<usize> = applied
                .iter()
                .filter(|x| !matched.contains(x))
                .chain(matched.iter().filter(|x| !applied.contains(x)))
                .copied()
                .collect();
            let only_x = diff.iter().all(|&d| w.as_bytes()[d] == b'x') && text.starts_with("/p/");
            if applied != matched && !only_x {
                if n < 6 {
                    eprintln!("MISMATCH default={voice_default} r{i} `{text}` on {w}: rule {applied:?} match {matched:?}");
                }
                n += 1;
            }
        }
        assert!(any, "default={voice_default} rule {i} `{text}` applies somewhere, so the comparison is not vacuous");
        bad += n;
    }
    assert_eq!(
        bad, 0,
        "default={voice_default}: rule application and match disagree"
    );
}

#[test]
fn bundles_and_alphas_agree_with_rules_on_underspecified_and_multivalued_features() {
    rv_run(false);
}
#[test]
fn bundles_and_alphas_agree_with_rules_when_voice_has_a_default() {
    rv_run(true);
}

#[test]
fn one_greek_letter_on_two_features_is_unsupported() {
    let xml = RG
        .replace("RULES", "")
        .replace(r#" phonologicalRules="STRATUM""#, "")
        .replace("VOICEDEF", "");
    let g = pg_grammar::load(&xml).unwrap();
    let table = &g.char_tables[0];
    for text in [
        "/p/ / [αvoice] _ [αcons]",
        "/p/ / [αvoice] _ [αht]",
        "[αvoice αcons]",
    ] {
        match parse_pattern(&g, table, text, None) {
            Err(crate::pattern_text::PatternError::Unsupported { construct, .. }) => {
                assert_eq!(construct, "alphaAcrossFeatures", "{text}");
            }
            other => panic!("{text}: {:?}", other.err()),
        }
    }
    for text in [
        "/p/ / [αvoice] _ [αvoice]",
        "[αvoice βcons]",
        "[αvoice][αvoice]",
    ] {
        assert!(parse_pattern(&g, table, text, None).is_ok(), "{text}");
    }
}

#[test]
fn every_position_a_conformance_rule_rewrites_is_a_match_span() {
    use crate::pattern_text::EnvironmentPattern;
    use pg_grammar_model::chardef::CharDefKind;
    use pg_grammar_model::model::RewriteMode;
    // Feeds each rule's own `Pattern` objects to `compile_match_engine`, bypassing the text parser, over short words of the grammar's own segments. Soundness: every position a simultaneous rule changes is a position `match` reports. Extra positions are fine (the rule's output may equal its input). Iterative rules feed themselves, so only simultaneous rules are asserted.
    let (mut rules_checked, mut missed, mut extra, mut skipped, mut panicked) = (0, 0, 0, 0, 0);
    for fixture in
        pg_conformance_fixtures::discover_scoped(pg_conformance_fixtures::ConformanceScope::All)
    {
        let rel = fixture.label();
        let xml = fixture.load_grammar_xml();
        // Left out: `intermediate-alpha-target` runs on another rule's output.
        if fixture.name == "intermediate-alpha-target" {
            continue;
        }
        let Ok(mut g) = pg_grammar::load(&xml) else {
            continue;
        };
        // Normalise rule application to simultaneous, so a rule is not fed by its own output.
        for r in g.prules.iter_mut() {
            if let PhonRuleDef::Rewrite(r) = r {
                r.mode = RewriteMode::Simultaneous;
            }
        }
        if g.char_tables.is_empty() {
            continue;
        }
        let table = &g.char_tables[0];
        let reps: Vec<String> = table
            .iter()
            .filter(|(_, d)| d.kind() == CharDefKind::Segment)
            .filter_map(|(_, d)| d.representations().first().cloned())
            .filter(|r| !r.is_empty())
            .take(9)
            .collect();
        let mut words: Vec<String> = reps.clone();
        for a in &reps {
            for b in &reps {
                words.push(format!("{a}{b}"));
                for c in &reps {
                    words.push(format!("{a}{b}{c}"));
                }
            }
        }
        for (ri, rule) in g.prules.iter().enumerate() {
            let PhonRuleDef::Rewrite(rule) = rule else {
                continue;
            };
            if rule.lhs.nodes.is_empty()
                || rule
                    .subrules
                    .iter()
                    .any(|sr| sr.rhs.nodes.len() != rule.lhs.nodes.len())
            {
                skipped += 1;
                continue;
            }
            // `synthesize` runs every rule against the first table, so a rule of another stratum's table has no verdict here.
            if crate::cache::owning_table_for_prule(&g, pg_grammar_model::model::PRuleId(ri as u32))
                .is_some_and(|t| t != TableId(0))
            {
                skipped += 1;
                continue;
            }
            let iterative = rule.mode != RewriteMode::Simultaneous;
            if iterative {
                skipped += 1;
                continue;
            }
            let mut compiled_all = Vec::new();
            for sr in &rule.subrules {
                let parsed = ParsedPattern {
                    target: rule.lhs.clone(),
                    environment: Some(EnvironmentPattern {
                        left: sr.left_env.clone(),
                        right: sr.right_env.clone(),
                    }),
                    synthetic_classes: Vec::new(),
                };
                compiled_all.push(compile_match_engine(&g, TableId(0), &parsed).expect("compiles"));
            }
            let gated = rule.subrules.iter().any(|sr| sr.required_pos.is_some());
            rules_checked += 1;
            let (mut m1, mut m2, mut any) = (0, 0, false);
            for w in &words {
                let Ok(input) = segment_with_features(&g, table, w) else {
                    continue;
                };
                let segpos: Vec<usize> = (0..input.len())
                    .filter(|&i| input.kind(i) == NodeKind::Segment)
                    .collect();
                // The engine panics on a few fixtures (an out-of-range char-def lookup); such a word has no verdict.
                let Ok(mut outputs) =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        synthesize(&g, rule, &input)
                    }))
                else {
                    panicked += 1;
                    continue;
                };
                let applied: Vec<usize> = match outputs.pop() {
                    Some(out) if out.len() == input.len() => segpos
                        .iter()
                        .enumerate()
                        .filter(|&(_, &i)| out.node_lanes(i) != input.node_lanes(i))
                        .map(|(s, _)| s)
                        .collect(),
                    _ => Vec::new(),
                };
                let mut matched: Vec<usize> = compiled_all
                    .iter()
                    .flat_map(|c| c.matches(&g, &input))
                    .flat_map(|m| m.start..m.end)
                    .collect();
                matched.sort_unstable();
                matched.dedup();
                any |= !applied.is_empty();
                let miss: Vec<_> = applied.iter().filter(|x| !matched.contains(x)).collect();
                let ext: Vec<_> = matched.iter().filter(|x| !applied.contains(x)).collect();
                if !miss.is_empty() {
                    if m1 < 3 {
                        eprintln!(
                            "MISSED {rel} r{ri} {:?} on {w}: rule {applied:?} match {matched:?}",
                            rule.name
                        );
                    }
                    m1 += 1;
                }
                if !ext.is_empty() {
                    m2 += 1;
                }
            }
            let _ = (gated, any, m2);
            missed += m1;
            extra += m2;
        }
    }
    eprintln!("engine sweep: rules={rules_checked} skipped={skipped} engine_panics={panicked} missed_words={missed} extra_words={extra}");
    assert!(
        rules_checked > 0,
        "the sweep found no simultaneous rule to check"
    );
    assert_eq!(
        missed, 0,
        "match missed a position a conformance rule changes"
    );
}

/// Whether `pattern` matches `word` (morpheme boundaries written `+`) over a span that overlaps the form's segments `[from, to)` and is not wholly inside them.
fn straddles_for_real(
    g: &Grammar,
    compiled: &CompiledMatch,
    word: &str,
    from: usize,
    to: usize,
) -> bool {
    let shape = segment_with_features(g, &g.char_tables[0], word).unwrap();
    compiled
        .matches(g, &shape)
        .iter()
        .any(|m| m.start < m.end && m.start < to && m.end > from && (m.start < from || m.end > to))
}

/// The soundness of `StraddleProbes`: whenever a pattern really matches across an edge of a form between any neighbours (with the morpheme boundaries between them), `may_straddle` says so.
#[test]
fn a_straddle_probe_never_misses_a_match_across_an_edge() {
    let g = pg_grammar::load(GRAMMAR).unwrap();
    let table = &g.char_tables[0];
    let boundary = crate::pattern_text::boundary_char_def(table, "+");
    let patterns = [
        // A natural class and a literal.
        "[V] /p/",
        // An optional group in front.
        "([Stop]) [V]",
        // An unbounded group between two nodes, and at the end.
        "/p/ ([V])* /b/",
        "[V] ([Stop])*",
        // A single node never straddles.
        "[Stop]",
        // Three nodes, so a form can sit wholly inside the pattern.
        "[Stop] [V] [Stop]",
    ];
    let alphabet = ["p", "b", "a", "i", "h"];
    let mut neighbours = vec![String::new()];
    for a in alphabet {
        neighbours.push(a.to_string());
        for b in alphabet {
            neighbours.push(format!("{a}{b}"));
        }
    }
    let forms = ["p", "a", "ap", "pa", "bab", "hh"];
    let (mut straddling, mut clear) = (0usize, 0usize);
    for text in patterns {
        let parsed =
            parse_pattern(&g, table, text, boundary).unwrap_or_else(|e| panic!("{text}: {e:?}"));
        let compiled = compile_match(&g, TableId(0), &parsed).unwrap();
        let probes = compile_straddle(&g, TableId(0), &parsed.target)
            .unwrap_or_else(|e| panic!("{text}: {e:?}"));
        for form in forms {
            let alone = segment_with_features(&g, table, form).unwrap();
            let says = probes.may_straddle(&g, &alone);
            let n = form.chars().count();
            let mut real = false;
            for left in &neighbours {
                for right in &neighbours {
                    let word = format!("{left}+{form}+{right}");
                    let from = left.chars().count();
                    real |= straddles_for_real(&g, &compiled, &word, from, from + n);
                }
            }
            assert!(
                says || !real,
                "`{text}` really straddles `{form}` but the probes say no"
            );
            if says {
                straddling += 1;
            } else {
                clear += 1;
            }
        }
    }
    assert!(
        straddling > 5 && clear > 5,
        "{straddling} straddle, {clear} clear: the check is not vacuous"
    );
}

#[test]
fn a_pattern_the_probes_cannot_cut_soundly_is_refused() {
    let g = pg_grammar::load(GRAMMAR).unwrap();
    let table = &g.char_tables[0];
    for text in ["([V])*", "([Stop]) ([V])", "+", "#"] {
        let parsed = parse_pattern(
            &g,
            table,
            text,
            crate::pattern_text::boundary_char_def(table, "+"),
        );
        if let Ok(parsed) = parsed {
            assert!(
                compile_straddle(&g, TableId(0), &parsed.target).is_err(),
                "`{text}` matches no segment, so no probe can be built"
            );
        }
    }
}

#[test]
fn a_boundary_or_anchor_in_the_pattern_is_an_edge_marker_and_is_not_cut() {
    let g = pg_grammar::load(GRAMMAR).unwrap();
    let table = &g.char_tables[0];
    let boundary = crate::pattern_text::boundary_char_def(table, "+");
    for text in [
        "+ /t/",
        "/t/ +",
        "+ /b/ /a/",
        "[Stop] + [V]",
        "# /p/ [V]",
        "[V] /b/ #",
    ] {
        let parsed =
            parse_pattern(&g, table, text, boundary).unwrap_or_else(|e| panic!("{text}: {e:?}"));
        assert!(has_edge_marker(&g, TableId(0), &parsed.target), "`{text}`");
        assert!(
            compile_straddle(&g, TableId(0), &parsed.target).is_err(),
            "`{text}`"
        );
    }
    for text in ["[V] /p/", "([Stop]) [V]"] {
        let parsed = parse_pattern(&g, table, text, boundary).unwrap();
        assert!(!has_edge_marker(&g, TableId(0), &parsed.target), "`{text}`");
    }
}
