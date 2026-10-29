use super::*;
use crate::stats::{PruneCounters, StatsCollector};
use pg_featstruct::FeatureStruct;

const XML: &str = r#"
<HermitCrabInput>
  <Language>
    <Name>TemplateSlotSharing</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cE"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name>
        <Segment segment="cM" /><Segment segment="cI" /><Segment segment="cD" /><Segment segment="cE" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrD">
            <Name>d suffix</Name><MorphemeId>D</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subD">
                <MorphologicalInput>
                  <PhoneticSequence id="stemD"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemD" /><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrE">
            <Name>e suffix</Name><MorphemeId>E</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subE">
                <MorphologicalInput>
                  <PhoneticSequence id="stemE"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemE" /><InsertSegments><PhoneticShape>e</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrDD">
            <Name>dd suffix</Name><MorphemeId>DD</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subDD">
                <MorphologicalInput>
                  <PhoneticSequence id="stemDD"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemDD" /><InsertSegments><PhoneticShape>dd</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate>
            <Name>e then d</Name>
            <Slot morphologicalRules="mrE" optional="true"><Name>e</Name></Slot>
            <Slot morphologicalRules="mrD mrDD"><Name>d</Name></Slot>
          </AffixTemplate>
          <AffixTemplate>
            <Name>d alone</Name>
            <Slot morphologicalRules="mrD mrDD"><Name>d</Name></Slot>
          </AffixTemplate>
          <AffixTemplate>
            <Name>optional d</Name>
            <Slot morphologicalRules="mrE" optional="true"><Name>e</Name></Slot>
            <Slot morphologicalRules="mrD mrDD" optional="true"><Name>d</Name></Slot>
          </AffixTemplate>
          <AffixTemplate>
            <Name>d e d</Name>
            <Slot morphologicalRules="mrD" optional="true"><Name>inner d</Name></Slot>
            <Slot morphologicalRules="mrE" optional="true"><Name>e</Name></Slot>
            <Slot morphologicalRules="mrD mrDD"><Name>d</Name></Slot>
          </AffixTemplate>
          <AffixTemplate requiredPartsOfSpeech="posN">
            <Name>N only, same tail</Name>
            <Slot morphologicalRules="mrE"><Name>e</Name></Slot>
            <Slot morphologicalRules="mrD mrDD"><Name>d</Name></Slot>
          </AffixTemplate>
          <AffixTemplate>
            <Name>no slots</Name>
          </AffixTemplate>
          <AffixTemplate>
            <Name>e then d, again</Name>
            <Slot morphologicalRules="mrE" optional="true"><Name>e</Name></Slot>
            <Slot morphologicalRules="mrD mrDD"><Name>d</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="eN" partOfSpeech="posN"><MorphemeId>N</MorphemeId>
            <Allomorphs><Allomorph id="aN"><PhoneticShape>mi</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
          <LexicalEntry id="eV" partOfSpeech="posV"><MorphemeId>V</MorphemeId>
            <Allomorphs><Allomorph id="aV"><PhoneticShape>mi</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn grammar() -> Grammar {
    pg_grammar::load(XML).expect("slot-sharing grammar loads")
}

fn word(g: &Grammar, text: &str) -> Word {
    let shape = pg_grammar_model::segment::segment(&g.char_tables[0], text).expect("word segments");
    Word::new(shape, StratumId(0))
}

fn analyzer<'a>(
    g: &'a Grammar,
    budget: &'a StepBudget,
    stats: Option<&'a StatsCollector>,
) -> StratumAnalyzer<'a, 'a, 'a, 'a, 'a, 'a> {
    analyzer_with(g, budget, stats, RequiredSlotLookahead::default())
}

fn analyzer_with<'a>(
    g: &'a Grammar,
    budget: &'a StepBudget,
    stats: Option<&'a StatsCollector>,
    look_ahead: RequiredSlotLookahead,
) -> StratumAnalyzer<'a, 'a, 'a, 'a, 'a, 'a> {
    StratumAnalyzer::new(
        g,
        StratumId(0),
        AnalyzerConfig {
            required_slot_lookahead: look_ahead,
            ..AnalyzerConfig::default()
        },
        None,
        None,
        None,
        budget,
        FinalTemplateAnalysisPolicy::default(),
        stats,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    )
}

type Projection = Vec<Vec<(WordKey, FeatureStruct, Vec<Option<MRuleId>>)>>;

fn project(per_template: Vec<Vec<Word>>) -> Projection {
    per_template
        .into_iter()
        .map(|words| {
            words
                .into_iter()
                .map(|w| (w.dedup_key(), w.syn_fs.clone(), w.mrule_apps.clone()))
                .collect()
        })
        .collect()
}

fn words_up_to(len: usize) -> Vec<String> {
    let mut all = vec![String::new()];
    let mut frontier = vec![String::new()];
    for _ in 0..len {
        let next: Vec<String> = frontier
            .iter()
            .flat_map(|w| ["m", "i", "d", "e"].map(|c| format!("{w}{c}")))
            .collect();
        all.extend(next.iter().cloned());
        frontier = next;
    }
    all.retain(|w| !w.is_empty());
    all
}

fn battery_counters(stats: &StatsCollector) -> PruneCounters {
    let mut total = PruneCounters::default();
    for row in stats.prune_rows() {
        total.accumulate(&row.counters);
    }
    total
}

#[test]
fn shared_walk_matches_every_templates_own_walk_in_order() {
    let g = grammar();
    let pos_n = g.fs_interner.get(g.entries[0].syn_fs).clone();
    let mut compared = 0;
    for text in words_up_to(5) {
        for syn_fs in [FeatureStruct::EMPTY, pos_n.clone()] {
            let mut input = word(&g, &text);
            input.syn_fs = syn_fs;
            let budget = StepBudget::new(usize::MAX);
            let a = analyzer(&g, &budget, None);
            let own: Vec<Vec<Word>> = a
                .stratum
                .templates
                .iter()
                .map(|&tid| a.analyze_template(tid, &input))
                .collect();
            let shared = a.shared_slot_battery(&input);
            compared += own.iter().map(Vec::len).sum::<usize>();
            assert_eq!(project(shared), project(own), "word {text}");
        }
    }
    assert!(
        compared > 1000,
        "the comparison must exercise real outputs, saw {compared}"
    );
}

#[test]
fn untraced_battery_runs_a_shared_slot_batch_once() {
    let g = grammar();
    let input = word(&g, "miedd");

    let own_stats = StatsCollector::new(&g);
    let budget = StepBudget::new(usize::MAX);
    let a = analyzer_with(&g, &budget, Some(&own_stats), RequiredSlotLookahead::Off);
    for &tid in &a.stratum.templates {
        a.analyze_template(tid, &input);
    }
    let own = battery_counters(&own_stats);

    let shared_stats = StatsCollector::new(&g);
    let budget = StepBudget::new(usize::MAX);
    analyzer_with(&g, &budget, Some(&shared_stats), RequiredSlotLookahead::Off)
        .run_template_batch(&input);
    let shared = battery_counters(&shared_stats);

    assert!(shared.shared_slot_batches > 0, "{shared:?}");
    assert_eq!(
        shared.slot_batches + shared.slot_batches_avoided,
        own.slot_batches,
        "every avoided batch is one the per-template walk ran: shared {shared:?}, own {own:?}"
    );
    assert!(shared.slot_rule_attempts < own.slot_rule_attempts);
}
