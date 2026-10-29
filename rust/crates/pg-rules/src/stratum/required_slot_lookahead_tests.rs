use super::*;
use crate::required_slots;

const XML: &str = r#"
<HermitCrabInput>
  <Language>
    <Name>RequiredSlotLookahead</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cY"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cZ"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name>
        <Segment segment="cM" /><Segment segment="cI" /><Segment segment="cY" /><Segment segment="cZ" />
      </SegmentNaturalClass>
      <SegmentNaturalClass id="ncY"><Name>Y</Name><Segment segment="cY" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrY"><Name>y suffix</Name><MorphemeId>Y</MorphemeId>
            <MorphologicalSubrules><MorphologicalSubrule id="subY">
              <MorphologicalInput><PhoneticSequence id="s"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="s" /><InsertSegments><PhoneticShape>y</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrY2"><Name>second y suffix</Name><MorphemeId>Y2</MorphemeId>
            <MorphologicalSubrules><MorphologicalSubrule id="subY2">
              <MorphologicalInput><PhoneticSequence id="s"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="s" /><InsertSegments><PhoneticShape>y</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrZ"><Name>z suffix</Name><MorphemeId>Z</MorphemeId>
            <MorphologicalSubrules><MorphologicalSubrule id="subZ">
              <MorphologicalInput><PhoneticSequence id="s"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="s" /><InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrW"><Name>y deleter</Name><MorphemeId>W</MorphemeId>
            <MorphologicalSubrules><MorphologicalSubrule id="subW">
              <MorphologicalInput>
                <PhoneticSequence id="s"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                <PhoneticSequence id="gone"><SimpleContext naturalClass="ncY" /></PhoneticSequence>
              </MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="s" /></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrN"><Name>null affix</Name><MorphemeId>N</MorphemeId>
            <MorphologicalSubrules><MorphologicalSubrule id="subN">
              <MorphologicalInput><PhoneticSequence id="s"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="s" /></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate><Name>y then two optional z</Name>
            <Slot morphologicalRules="mrY"><Name>y</Name></Slot>
            <Slot morphologicalRules="mrZ" optional="true"><Name>z1</Name></Slot>
            <Slot morphologicalRules="mrZ" optional="true"><Name>z2</Name></Slot>
          </AffixTemplate>
          <AffixTemplate><Name>y then impure</Name>
            <Slot morphologicalRules="mrY"><Name>y</Name></Slot>
            <Slot morphologicalRules="mrW" optional="true"><Name>w</Name></Slot>
          </AffixTemplate>
          <AffixTemplate><Name>all optional</Name>
            <Slot morphologicalRules="mrY" optional="true"><Name>y</Name></Slot>
            <Slot morphologicalRules="mrZ" optional="true"><Name>z</Name></Slot>
          </AffixTemplate>
          <AffixTemplate><Name>null affix required</Name>
            <Slot morphologicalRules="mrN"><Name>n</Name></Slot>
            <Slot morphologicalRules="mrZ" optional="true"><Name>z</Name></Slot>
          </AffixTemplate>
          <AffixTemplate><Name>y then optional y</Name>
            <Slot morphologicalRules="mrY" ><Name>y</Name></Slot>
            <Slot morphologicalRules="mrY2" optional="true"><Name>y2</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="eV" partOfSpeech="posV"><MorphemeId>V</MorphemeId>
            <Allomorphs><Allomorph id="aV"><PhoneticShape>mi</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

const Y_THEN_Z: TemplateId = TemplateId(0);
const Y_THEN_IMPURE: TemplateId = TemplateId(1);

fn grammar() -> Grammar {
    pg_grammar::load(XML).expect("required-slot grammar loads")
}

pub(super) fn config(mode: RequiredSlotLookahead) -> AnalyzerConfig {
    AnalyzerConfig {
        required_slot_lookahead: mode,
        ..AnalyzerConfig::default()
    }
}

/// One template's analysis outputs and the work it cost, in `mode`.
pub(super) fn run(
    g: &Grammar,
    mode: RequiredSlotLookahead,
    tid: TemplateId,
    text: &str,
) -> (Vec<WordKey>, usize) {
    let shape = pg_grammar_model::segment::segment(&g.char_tables[0], text).expect("word segments");
    let input = Word::new(shape, StratumId(0));
    let budget = StepBudget::new(usize::MAX);
    let _scope = budget.enter();
    let a = StratumAnalyzer::new(
        g,
        StratumId(0),
        config(mode),
        None,
        None,
        None,
        &budget,
        FinalTemplateAnalysisPolicy::default(),
        None,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    );
    let keys = a
        .analyze_template(tid, &input)
        .iter()
        .map(Word::dedup_key)
        .collect();
    (keys, budget.work_steps())
}

/// Every template's outputs from the shared battery, and the work it cost, in `mode`.
fn run_shared(g: &Grammar, mode: RequiredSlotLookahead, text: &str) -> (Vec<Vec<WordKey>>, usize) {
    let shape = pg_grammar_model::segment::segment(&g.char_tables[0], text).expect("word segments");
    let input = Word::new(shape, StratumId(0));
    let budget = StepBudget::new(usize::MAX);
    let _scope = budget.enter();
    let a = StratumAnalyzer::new(
        g,
        StratumId(0),
        config(mode),
        None,
        None,
        None,
        &budget,
        FinalTemplateAnalysisPolicy::default(),
        None,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    );
    let keys = a
        .shared_slot_battery(&input)
        .iter()
        .map(|words| words.iter().map(Word::dedup_key).collect())
        .collect();
    (keys, budget.work_steps())
}

pub(super) fn words_up_to(len: usize) -> Vec<String> {
    let mut all = Vec::new();
    let mut frontier = vec![String::new()];
    for _ in 0..len {
        frontier = frontier
            .iter()
            .flat_map(|w| ["m", "i", "y", "z"].map(|c| format!("{w}{c}")))
            .collect();
        all.extend(frontier.iter().cloned());
    }
    all
}

#[test]
fn prunes_a_template_whose_required_slot_has_no_material() {
    let g = grammar();
    let (off_out, off_work) = run(&g, RequiredSlotLookahead::Off, Y_THEN_Z, "mizz");
    let (on_out, on_work) = run(&g, RequiredSlotLookahead::On, Y_THEN_Z, "mizz");
    assert_eq!(on_out, off_out);
    assert!(off_out.is_empty(), "no y to unapply, so no output");
    assert!(
        on_work < off_work,
        "pruning at the root skips the optional slots: on {on_work}, off {off_work}"
    );
}

#[test]
fn keeps_a_template_whose_required_material_is_present() {
    let g = grammar();
    let (off_out, off_work) = run(&g, RequiredSlotLookahead::Off, Y_THEN_Z, "miyz");
    let (on_out, on_work) = run(&g, RequiredSlotLookahead::On, Y_THEN_Z, "miyz");
    assert!(!off_out.is_empty());
    assert_eq!(on_out, off_out);
    assert!(
        on_work >= off_work,
        "a check that prunes nothing only adds work"
    );
}

#[test]
fn an_impure_slot_between_stops_the_look_ahead() {
    // `w` unapplies by restoring the `y` it deleted, so "mi" completes although it has no `y` to begin with.
    let g = grammar();
    let (off_out, _) = run(&g, RequiredSlotLookahead::Off, Y_THEN_IMPURE, "mi");
    let (on_out, _) = run(&g, RequiredSlotLookahead::On, Y_THEN_IMPURE, "mi");
    assert!(
        !off_out.is_empty(),
        "the walk through the impure slot completes"
    );
    assert_eq!(on_out, off_out);
}

#[test]
fn a_null_affix_slot_and_optional_slots_are_never_pruned() {
    let g = grammar();
    for tid in [TemplateId(2), TemplateId(3)] {
        for text in words_up_to(3) {
            let (off_out, _) = run(&g, RequiredSlotLookahead::Off, tid, &text);
            let (on_out, _) = run(&g, RequiredSlotLookahead::On, tid, &text);
            assert_eq!(on_out, off_out, "template {tid:?}, word {text}");
        }
    }
    let (out, _) = run(&g, RequiredSlotLookahead::On, TemplateId(3), "mi");
    assert!(!out.is_empty(), "the null affix unapplies on any word");
}

#[test]
fn every_template_gives_the_same_outputs_with_the_look_ahead_on_and_off() {
    let g = grammar();
    let mut nonempty = 0;
    for tid in (0..g.templates.len() as u32).map(TemplateId) {
        for text in words_up_to(4) {
            let (off_out, _) = run(&g, RequiredSlotLookahead::Off, tid, &text);
            let (on_out, _) = run(&g, RequiredSlotLookahead::On, tid, &text);
            nonempty += usize::from(!off_out.is_empty());
            assert_eq!(on_out, off_out, "template {tid:?}, word {text}");
        }
    }
    assert!(
        nonempty > 100,
        "the sweep must exercise real outputs, saw {nonempty}"
    );
}

#[test]
fn shadow_counts_fired_checks_and_never_prunes() {
    let g = grammar();
    required_slots::take_shadow();
    let (off_out, off_work) = run(&g, RequiredSlotLookahead::Off, Y_THEN_Z, "mizz");
    let (shadow_out, shadow_work) = run(&g, RequiredSlotLookahead::Shadow, Y_THEN_Z, "mizz");
    let shadow = required_slots::take_shadow();
    assert_eq!(shadow_out, off_out);
    assert!(shadow_work >= off_work, "shadow does the whole walk");
    assert!(shadow.fired > 0, "{shadow:?}");
    assert_eq!((shadow.violated, shadow.completed_under_fired), (0, 0));

    for tid in (0..g.templates.len() as u32).map(TemplateId) {
        for text in words_up_to(4) {
            run(&g, RequiredSlotLookahead::Shadow, tid, &text);
        }
    }
    let sweep = required_slots::take_shadow();
    assert!(sweep.fired > 0);
    assert_eq!(
        (sweep.violated, sweep.completed_under_fired),
        (0, 0),
        "a violation means the look-ahead pruned a partial that completes: {sweep:?}"
    );
}

#[test]
fn the_shared_walk_gives_each_template_its_own_walks_outputs_with_the_look_ahead() {
    let g = grammar();
    let mut nonempty = 0;
    for text in words_up_to(4) {
        let (on, _) = run_shared(&g, RequiredSlotLookahead::On, &text);
        let (off, _) = run_shared(&g, RequiredSlotLookahead::Off, &text);
        assert_eq!(on, off, "word {text}");
        for (i, keys) in on.iter().enumerate() {
            let (own, _) = run(&g, RequiredSlotLookahead::On, TemplateId(i as u32), &text);
            assert_eq!(*keys, own, "template {i}, word {text}");
            nonempty += usize::from(!own.is_empty());
        }
    }
    assert!(
        nonempty > 100,
        "the sweep must exercise real outputs, saw {nonempty}"
    );
}

#[test]
fn a_dead_template_stops_collecting_while_its_siblings_share_the_walk_on() {
    // No `y`: the first template is dead at the root, while the two that share its first slot go on.
    let g = grammar();
    let (off, off_work) = run_shared(&g, RequiredSlotLookahead::Off, "mizz");
    let (on, on_work) = run_shared(&g, RequiredSlotLookahead::On, "mizz");
    assert_eq!(on, off);
    assert!(on[0].is_empty());
    assert!(
        !on[2].is_empty() && !on[3].is_empty(),
        "siblings still complete"
    );
    assert!(on_work < off_work, "on {on_work}, off {off_work}");
}

#[test]
fn shadow_on_the_shared_walk_counts_fired_checks_and_no_violation() {
    let g = grammar();
    required_slots::take_shadow();
    let mut shadow_work = 0;
    let mut off_work = 0;
    for text in words_up_to(4) {
        let (off, w) = run_shared(&g, RequiredSlotLookahead::Off, &text);
        off_work += w;
        let (shadow, w) = run_shared(&g, RequiredSlotLookahead::Shadow, &text);
        shadow_work += w;
        assert_eq!(shadow, off, "shadow must not prune: word {text}");
    }
    let counts = required_slots::take_shadow();
    assert!(counts.fired > 0, "{counts:?}");
    assert_eq!(
        (counts.violated, counts.completed_under_fired),
        (0, 0),
        "{counts:?}"
    );
    assert!(shadow_work >= off_work);
}

#[test]
fn a_template_dead_in_one_branch_is_alive_in_the_branch_that_skips_the_slot() {
    // Unapplying the optional `y2` removes the only `y`, so that branch is dead; skipping it keeps the `y` the required slot needs.
    let g = grammar();
    let y_then_y2 = TemplateId(4);
    let (off, _) = run_shared(&g, RequiredSlotLookahead::Off, "miy");
    let (on, _) = run_shared(&g, RequiredSlotLookahead::On, "miy");
    assert!(!off[y_then_y2.0 as usize].is_empty());
    assert_eq!(on, off);
}
