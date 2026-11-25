use super::*;
use crate::shape_graph::test_disable;

const XML: &str = r#"
<HermitCrabInput>
  <Language>
    <Name>ShapeGraphGate</Name>
    <PartsOfSpeech><PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name>
        <Segment segment="cM" /><Segment segment="cI" /><Segment segment="cD" /><Segment segment="cT" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrSufD mrSufD2 mrSufT mrPreM">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrSufD" outputPartOfSpeech="posN"><Name>d suffix</Name><MorphemeId>SD</MorphemeId>
            <MorphologicalSubrules><MorphologicalSubrule id="subSufD">
              <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrSufD2" outputPartOfSpeech="posN"><Name>d homophone suffix</Name><MorphemeId>SD2</MorphemeId>
            <MorphologicalSubrules><MorphologicalSubrule id="subSufD2">
              <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrSufT" outputPartOfSpeech="posN"><Name>t suffix</Name><MorphemeId>ST</MorphemeId>
            <MorphologicalSubrules><MorphologicalSubrule id="subSufT">
              <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>t</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrPreM" outputPartOfSpeech="posN"><Name>m prefix</Name><MorphemeId>PM</MorphemeId>
            <MorphologicalSubrules><MorphologicalSubrule id="subPreM">
              <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><InsertSegments><PhoneticShape>m</PhoneticShape></InsertSegments><CopyFromInput index="stem" /></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eN" partOfSpeech="posN"><MorphemeId>N</MorphemeId>
            <Allomorphs><Allomorph id="aN"><PhoneticShape>mimi</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

const REAL_XML: &str = r#"
<HermitCrabInput>
  <Language>
    <Name>ShapeGraphGate</Name>
    <HeadFeatures><SymbolicFeature id="tense"><Name>Tense</Name><Symbols><Symbol id="past">Past</Symbol><Symbol id="present">Present</Symbol><Symbol id="future">Future</Symbol></Symbols></SymbolicFeature></HeadFeatures><PartsOfSpeech><PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name>
        <Segment segment="cM" /><Segment segment="cI" /><Segment segment="cD" /><Segment segment="cT" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrR1 mrR2 mrR3 mrPreM">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <RealizationalRule id="mrR1"><Name>real 1</Name>
            <MorphologicalSubrules><MorphologicalSubrule id="subR1">
              <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
            <RealizationalFeatures><FeatureValue feature="tense" symbolValues="past" /></RealizationalFeatures></RealizationalRule>
          <RealizationalRule id="mrR2"><Name>real 2</Name>
            <MorphologicalSubrules><MorphologicalSubrule id="subR2">
              <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
            <RealizationalFeatures><FeatureValue feature="tense" symbolValues="present" /></RealizationalFeatures></RealizationalRule>
          <RealizationalRule id="mrR3"><Name>real 3</Name>
            <MorphologicalSubrules><MorphologicalSubrule id="subR3">
              <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>t</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
            <RealizationalFeatures><FeatureValue feature="tense" symbolValues="future" /></RealizationalFeatures></RealizationalRule>
          <MorphologicalRule id="mrPreM" outputPartOfSpeech="posN"><Name>m prefix</Name><MorphemeId>PM</MorphemeId>
            <MorphologicalSubrules><MorphologicalSubrule id="subPreM">
              <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><InsertSegments><PhoneticShape>m</PhoneticShape></InsertSegments><CopyFromInput index="stem" /></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eN" partOfSpeech="posN"><MorphemeId>N</MorphemeId>
            <Allomorphs><Allomorph id="aN"><PhoneticShape>mimi</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

struct Run {
    words: String,
    capped: bool,
    word_capped: bool,
    hits: usize,
    work: usize,
}

fn run(g: &Grammar, cache: &RuleCache, text: &str, graph_on: bool) -> Run {
    run_with(g, cache, text, graph_on, usize::MAX)
}

fn run_with(g: &Grammar, cache: &RuleCache, text: &str, graph_on: bool, work_cap: usize) -> Run {
    run_metered(g, cache, text, graph_on, work_cap, None)
}

fn run_metered(
    g: &Grammar,
    cache: &RuleCache,
    text: &str,
    graph_on: bool,
    work_cap: usize,
    word_cap: Option<usize>,
) -> Run {
    test_disable(!graph_on);
    let shape = pg_grammar_model::segment::segment(&g.char_tables[0], text).expect("word segments");
    let budget = WordBudget::new(work_cap).with_word_cap(word_cap);
    let out = analyze_stratum_filtered(
        g,
        StratumId(0),
        Word::new(shape, StratumId(0)),
        &AnalyzerConfig::default(),
        None,
        Some(cache),
        &budget,
    );
    test_disable(false);
    Run {
        words: format!("{:?}", out.words),
        capped: out.capped,
        word_capped: budget.word_capped(),
        hits: budget.shape_graph().hits(),
        work: budget.work_steps(),
    }
}

fn fixture() -> (Grammar, RuleCache) {
    let g = pg_grammar::load(XML).expect("graph grammar loads");
    let cache = RuleCache::build(&g);
    (g, cache)
}

#[test]
fn graph_hits_leave_the_analyses_and_their_order_identical_and_cost_less_work() {
    let (g, cache) = fixture();
    let text = "mimimimimimimimimitd";
    let off = run(&g, &cache, text, false);
    let on = run(&g, &cache, text, true);
    assert!(!off.capped && !on.capped);
    assert_eq!(off.hits, 0);
    assert!(
        on.hits > 0,
        "the fixture must reach a real graph hit, or the comparison proves nothing"
    );
    assert!(
        off.words.matches("Word {").count() > 1,
        "the fixture must unapply something"
    );
    assert_eq!(on.words, off.words);
    assert!(on.work < off.work, "{} !< {}", on.work, off.work);
}

#[test]
fn the_graph_is_keyed_by_the_rule_not_only_the_shape() {
    let (g, cache) = fixture();
    for text in [
        "dmimimimimimimimid",
        "tmimimimimimimimidt",
        "dmimimimimimimimidd",
    ] {
        let off = run(&g, &cache, text, false);
        let on = run(&g, &cache, text, true);
        assert_eq!(on.words, off.words, "{text}");
    }
}

#[test]
fn the_graph_never_caps_a_word_the_base_completes_and_a_capped_run_repeats() {
    let (g, cache) = fixture();
    for text in ["mimimimimimimimimitd", "dmimimimimimimimidd"] {
        let full = run(&g, &cache, text, false);
        assert!(!full.capped);
        let step = (full.work / 80).max(1);
        for cap in (1..=full.work + step).step_by(step) {
            let off = run_with(&g, &cache, text, false, cap);
            let on = run_with(&g, &cache, text, true, cap);
            if !off.capped {
                assert!(!on.capped, "{text} at {cap}");
                assert_eq!(on.words, off.words, "{text} at {cap}");
            }
            let again = run_with(&g, &cache, text, true, cap);
            assert_eq!(
                (again.words, again.capped, again.work),
                (on.words, on.capped, on.work)
            );
        }
    }
}

#[test]
fn a_realizational_rule_reaches_a_graph_hit_with_identical_output() {
    let g = pg_grammar::load(REAL_XML).expect("realizational graph grammar loads");
    let cache = RuleCache::build(&g);
    let mut hits = 0;
    for text in [
        "mimimimimimimimimidtd",
        "mimimimimimimimimitdd",
        "mimimimimimimimimidd",
    ] {
        let off = run(&g, &cache, text, false);
        let on = run(&g, &cache, text, true);
        assert_eq!(on.words, off.words, "{text}");
        assert!(on.work <= off.work);
        hits += on.hits;
    }
    assert!(
        hits > 0,
        "no realizational graph hit; the fixture proves nothing"
    );
}

#[test]
fn the_graph_leaves_the_word_memory_cap_where_the_base_puts_it() {
    let (g, cache) = fixture();
    let text = "mimimimimimimimimitd";
    let metered = |on: bool, cap: usize| run_metered(&g, &cache, text, on, usize::MAX, Some(cap));
    // The smallest word cap the base completes under: its retained peak.
    let (mut low, mut high) = (1usize, 1usize << 30);
    assert!(
        metered(false, low).word_capped,
        "the fixture must reach the word cap"
    );
    assert!(!metered(false, high).word_capped);
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if metered(false, mid).word_capped {
            low = mid;
        } else {
            high = mid;
        }
    }
    let peak = high;
    assert!(
        metered(true, peak + peak / 16).hits > 0,
        "a graph hit just above the peak"
    );
    let mut hit_somewhere = false;
    for cap in (peak / 2..=peak * 2)
        .step_by((peak / 64).max(1))
        .chain([peak - 1, peak, peak + 1])
    {
        let off = metered(false, cap);
        let on = metered(true, cap);
        assert_eq!(
            on.word_capped, off.word_capped,
            "word cap {cap}, peak {peak}"
        );
        assert_eq!(on.capped, off.capped, "word cap {cap}");
        if !off.capped {
            assert_eq!(on.words, off.words, "word cap {cap}");
            hit_somewhere |= on.hits > 0;
        }
    }
    assert!(
        hit_somewhere,
        "the graph must serve a hit under some uncapped word cap"
    );
}
