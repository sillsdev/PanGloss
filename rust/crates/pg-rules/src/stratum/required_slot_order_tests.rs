use super::required_slot_lookahead_tests::{run, words_up_to};
use super::*;
use crate::required_slots;

fn rule(id: &str, input: &str, output: &str) -> String {
    format!(
        r#"<MorphologicalRule id="{id}"><Name>{id}</Name><MorphemeId>{id}</MorphemeId>
            <MorphologicalSubrules><MorphologicalSubrule id="sub{id}">
              <MorphologicalInput>{input}</MorphologicalInput>
              <MorphologicalOutput>{output}</MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
          </MorphologicalRule>"#
    )
}

fn part(id: &str) -> String {
    format!(
        r#"<PhoneticSequence id="{id}"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>"#
    )
}

fn run_part(id: &str, min: i32, max: i32) -> String {
    format!(
        r#"<PhoneticSequence id="{id}"><OptionalSegmentSequence min="{min}" max="{max}"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>"#
    )
}

fn grammar() -> Grammar {
    let rules = [
        rule("mrP", &part("s"), r#"<InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments><CopyFromInput index="s" />"#),
        rule("mrY", &part("s"), r#"<CopyFromInput index="s" /><InsertSegments><PhoneticShape>y</PhoneticShape></InsertSegments>"#),
        rule("mrU", &format!("{}{}", part("s"), run_part("gone", 0, -1)), r#"<CopyFromInput index="s" />"#),
        rule("mrB", &format!("{}{}", part("s"), run_part("gone", 0, 2)), r#"<CopyFromInput index="s" />"#),
        rule(
            "mrR",
            &format!("{}{}", part("a"), part("b")),
            r#"<CopyFromInput index="b" /><CopyFromInput index="a" />"#,
        ),
    ]
    .concat();
    let xml = format!(
        r#"<HermitCrabInput><Language><Name>Order</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>
      <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cY"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cZ"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions></CharacterDefinitionTable>
    <NaturalClasses><SegmentNaturalClass id="ncAny"><Name>Any</Name>
      <Segment segment="cM" /><Segment segment="cI" /><Segment segment="cY" /><Segment segment="cZ" />
    </SegmentNaturalClass></NaturalClasses>
    <Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered"><Name>S</Name>
      <MorphologicalRuleDefinitions>{rules}</MorphologicalRuleDefinitions>
      <AffixTemplates>
        <AffixTemplate><Name>z prefix, y suffix</Name>
          <Slot morphologicalRules="mrP"><Name>p</Name></Slot>
          <Slot morphologicalRules="mrY"><Name>y</Name></Slot></AffixTemplate>
        <AffixTemplate><Name>z prefix, reorderer</Name>
          <Slot morphologicalRules="mrP"><Name>p</Name></Slot>
          <Slot morphologicalRules="mrR"><Name>r</Name></Slot></AffixTemplate>
        <AffixTemplate><Name>two z prefixes</Name>
          <Slot morphologicalRules="mrP"><Name>p1</Name></Slot>
          <Slot morphologicalRules="mrP"><Name>p2</Name></Slot></AffixTemplate>
        <AffixTemplate><Name>y suffix, z prefix</Name>
          <Slot morphologicalRules="mrY"><Name>y</Name></Slot>
          <Slot morphologicalRules="mrP"><Name>p</Name></Slot></AffixTemplate>
        <AffixTemplate><Name>y suffix, unbounded deleter</Name>
          <Slot morphologicalRules="mrY"><Name>y</Name></Slot>
          <Slot morphologicalRules="mrU" optional="true"><Name>u</Name></Slot></AffixTemplate>
        <AffixTemplate><Name>y suffix, bounded deleter</Name>
          <Slot morphologicalRules="mrY"><Name>y</Name></Slot>
          <Slot morphologicalRules="mrB" optional="true"><Name>b</Name></Slot></AffixTemplate>
      </AffixTemplates>
      <LexicalEntries><LexicalEntry id="eV" partOfSpeech="posV"><MorphemeId>V</MorphemeId>
        <Allomorphs><Allomorph id="aV"><PhoneticShape>mi</PhoneticShape></Allomorph></Allomorphs>
      </LexicalEntry></LexicalEntries>
    </Stratum></Strata></Language></HermitCrabInput>"#
    );
    pg_grammar::load(&xml).expect("order grammar loads")
}

/// How many slot tests fired on `text` for one template, in shadow mode, with the outputs of the shadow run.
fn fired(g: &Grammar, tid: u32, text: &str) -> u64 {
    required_slots::take_shadow();
    run(g, RequiredSlotLookahead::Shadow, TemplateId(tid), text);
    let counts = required_slots::take_shadow();
    assert_eq!((counts.violated, counts.completed_under_fired), (0, 0));
    counts.fired
}

#[test]
fn a_prefix_that_is_not_at_the_left_edge_cannot_unapply() {
    // The suffix slot removes at most one node on the right, so the `z` the prefix needs must open the word: "mzy" has it second.
    let g = grammar();
    assert!(fired(&g, 0, "mzy") > 0);
    assert_eq!(fired(&g, 0, "zmy"), 0);
}

#[test]
fn a_reach_from_the_other_edge_is_not_assumed() {
    // The outer slot is the prefix this time and the suffix slot is tested after it, so only the right edge is constrained.
    let g = grammar();
    assert_eq!(fired(&g, 3, "zmy"), 0);
    assert!(fired(&g, 3, "mymz") > 0);
}

#[test]
fn a_slot_that_reorders_what_it_copies_leaves_only_presence_testable() {
    let g = grammar();
    assert_eq!(
        fired(&g, 1, "mz"),
        0,
        "the reorderer may move the z to the front"
    );
    assert!(fired(&g, 1, "mim") > 0, "no z at all");
}

/// The verdicts at the root of the walk of template `tid` on `text`, slot by slot from the outermost until an impure slot ends the look-ahead.
fn root_verdicts(g: &Grammar, tid: u32, text: &str) -> Vec<bool> {
    let shape = pg_grammar_model::segment::segment(&g.char_tables[0], text).expect("word segments");
    let segs = crate::morph::segs_of(g, pg_grammar_model::model::TableId(0), &shape, false).0;
    let tid = TemplateId(tid);
    let material = required_slots::RequiredSlotMaterial::build(g, tid, None);
    let budget = StepBudget::new(usize::MAX);
    let _scope = budget.enter();
    let mut walk = required_slots::SlotWalk::start();
    let mut verdicts = Vec::new();
    for slot in (0..g.templates[tid.0 as usize].slots.len()).rev() {
        verdicts.push(material.cannot_unapply(slot, &segs, &mut walk));
        material.advance(slot, &mut walk);
        if !material.is_pure(slot) {
            break;
        }
    }
    verdicts
}

#[test]
fn a_node_serves_only_one_of_two_slots_that_insert_the_same_segment() {
    // The outer prefix takes the `z`, which the inner one then cannot reuse.
    let g = grammar();
    assert_eq!(root_verdicts(&g, 2, "zm"), [false, true]);
    assert_eq!(root_verdicts(&g, 2, "zz"), [false, false]);
}

#[test]
fn a_slot_deleting_an_unbounded_run_restores_nothing_so_the_look_ahead_goes_on() {
    let g = grammar();
    assert_eq!(
        root_verdicts(&g, 4, "mi"),
        [false, true],
        "no y behind the unbounded deleter"
    );
    assert_eq!(
        root_verdicts(&g, 5, "mi"),
        [false],
        "a bounded run restores nodes and stays impure"
    );
}

#[test]
fn the_ordered_look_ahead_never_changes_an_output() {
    let g = grammar();
    let mut nonempty = 0;
    for tid in (0..g.templates.len() as u32).map(TemplateId) {
        for text in words_up_to(5) {
            let (off, _) = run(&g, RequiredSlotLookahead::Off, tid, &text);
            let (on, _) = run(&g, RequiredSlotLookahead::On, tid, &text);
            nonempty += usize::from(!off.is_empty());
            assert_eq!(on, off, "template {tid:?}, word {text}");
            fired(&g, tid.0, &text);
        }
    }
    assert!(
        nonempty > 50,
        "the sweep must exercise real outputs, saw {nonempty}"
    );
}
