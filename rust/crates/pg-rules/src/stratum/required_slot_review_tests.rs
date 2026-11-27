//! Machine #575 cases added after the port, mirrored; work stands for the C# rule attempts.

use super::required_slot_lookahead_tests::config;
use super::required_slot_order_tests::fired;
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

fn run_of(id: &str, min: i32, max: i32) -> String {
    format!(
        r#"<PhoneticSequence id="{id}"><OptionalSegmentSequence min="{min}" max="{max}"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>"#
    )
}

fn one(id: &str) -> String {
    format!(
        r#"<PhoneticSequence id="{id}"><SimpleContext naturalClass="ncAny" /></PhoneticSequence>"#
    )
}

fn copy(id: &str) -> String {
    format!(r#"<CopyFromInput index="{id}" />"#)
}

fn insert(text: &str) -> String {
    format!(r#"<InsertSegments><PhoneticShape>{text}</PhoneticShape></InsertSegments>"#)
}

fn template(name: &str, slots: &[(&str, bool)]) -> String {
    let slots: String = slots
        .iter()
        .map(|(rule, optional)| {
            format!(r#"<Slot morphologicalRules="{rule}" optional="{optional}"><Name>{rule}</Name></Slot>"#)
        })
        .collect();
    format!("<AffixTemplate><Name>{name}</Name>{slots}</AffixTemplate>")
}

// Templates, outermost slot last.
const INNER_INFIX_OUTER_SUFFIX: u32 = 0;
const BOUNDARY_INFIX_OUTER: u32 = 1;
const FUSION_OUTER: u32 = 2;
const RUN_DELETED_OUTER: u32 = 3;
const RUN_COPIED_OUTER: u32 = 4;
const PREFIX_ONLY: u32 = 5;

fn grammar() -> Grammar {
    let rules = [
        rule("mrKA", &run_of("s", 1, -1), &(copy("s") + &insert("ka"))),
        rule("mrTI", &run_of("s", 1, -1), &(copy("s") + &insert("ti"))),
        rule("mrI", &run_of("s", 1, -1), &(copy("s") + &insert("i"))),
        rule("mrPKA", &run_of("s", 1, -1), &(insert("ka") + &copy("s"))),
        rule(
            "mrAK",
            &(one("x") + &run_of("y", 1, -1)),
            &(copy("x") + &insert("ak") + &copy("y")),
        ),
        rule(
            "mrPlus",
            &(one("x") + &run_of("y", 1, -1)),
            &(copy("x") + &insert("+") + &copy("y")),
        ),
        rule(
            "mrFuse",
            &(one("p") + &run_of("r", 0, -1)),
            &(insert("k") + &copy("p") + &insert("a") + &copy("p") + &copy("r")),
        ),
        rule(
            "mrRunDel",
            &(run_of("p", 1, -1) + &run_of("r", 0, -1)),
            &(copy("p") + &insert("z")),
        ),
        rule(
            "mrRunCopy",
            &(run_of("p", 1, -1) + &run_of("r", 0, -1)),
            &(copy("p") + &copy("r") + &insert("z")),
        ),
    ]
    .concat();
    let templates = [
        template(
            "inner infix, outer suffix",
            &[("mrAK", false), ("mrKA", false)],
        ),
        template(
            "suffix ti, boundary-only infix",
            &[("mrTI", false), ("mrPlus", true)],
        ),
        template(
            "suffix i, fusion prefix",
            &[("mrI", false), ("mrFuse", true)],
        ),
        template(
            "suffix ti, deleted run",
            &[("mrTI", false), ("mrRunDel", true)],
        ),
        template(
            "suffix ti, copied run",
            &[("mrTI", false), ("mrRunCopy", true)],
        ),
        template("prefix ka", &[("mrPKA", false)]),
    ]
    .concat();
    let seg = |id: &str, c: &str| {
        format!(
            r#"<SegmentDefinition id="{id}"><Representations><Representation>{c}</Representation></Representations></SegmentDefinition>"#
        )
    };
    let segs: String = ["a", "g", "i", "k", "s", "t", "z"]
        .iter()
        .map(|c| seg(&format!("c{c}"), c))
        .collect();
    let members: String = ["a", "g", "i", "k", "s", "t", "z"]
        .iter()
        .map(|c| format!(r#"<Segment segment="c{c}" />"#))
        .collect();
    let xml = format!(
        r#"<HermitCrabInput><Language><Name>Review</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1"><Name>Main</Name>
      <SegmentDefinitions>{segs}</SegmentDefinitions>
      <BoundaryDefinitions><BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition></BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses><SegmentNaturalClass id="ncAny"><Name>Any</Name>{members}</SegmentNaturalClass></NaturalClasses>
    <Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered"><Name>S</Name>
      <MorphologicalRuleDefinitions>{rules}</MorphologicalRuleDefinitions>
      <AffixTemplates>{templates}</AffixTemplates>
      <LexicalEntries><LexicalEntry id="eV" partOfSpeech="posV"><MorphemeId>V</MorphemeId>
        <Allomorphs><Allomorph id="aV"><PhoneticShape>sag</PhoneticShape></Allomorph></Allomorphs>
      </LexicalEntry></LexicalEntries>
    </Stratum></Strata></Language></HermitCrabInput>"#
    );
    pg_grammar::load(&xml).expect("review grammar loads")
}

/// One template's outputs and work over `shape` in `mode`.
fn run_shape(
    g: &Grammar,
    mode: RequiredSlotLookahead,
    tid: u32,
    shape: Shape,
) -> (Vec<WordKey>, usize) {
    let input = Word::new(shape, StratumId(0));
    let budget = WordBudget::new(usize::MAX);
    let _scope = budget.enter();
    let values = crate::shared_values::SharedValues::default();
    let a = StratumAnalyzer::new(
        g,
        StratumId(0),
        config(mode),
        None,
        None,
        None,
        &budget,
        &values,
        FinalTemplateAnalysisPolicy::default(),
        None,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    );
    let keys = a
        .analyze_template(TemplateId(tid), &input)
        .into_vec()
        .iter()
        .map(Word::dedup_key)
        .collect();
    (keys, budget.work_steps())
}

fn run(g: &Grammar, mode: RequiredSlotLookahead, tid: u32, text: &str) -> (Vec<WordKey>, usize) {
    let shape = pg_grammar_model::segment::segment(&g.char_tables[0], text).expect("word segments");
    run_shape(g, mode, tid, shape)
}

/// The look-ahead leaves the outputs alone; returns (off work, on work, shadow fired count, outputs).
fn walk_both(g: &Grammar, tid: u32, text: &str) -> (usize, usize, u64, Vec<WordKey>) {
    let (off_out, off_work) = run(g, RequiredSlotLookahead::Off, tid, text);
    let (on_out, on_work) = run(g, RequiredSlotLookahead::On, tid, text);
    assert_eq!(
        on_out, off_out,
        "the look-ahead must not change the outputs of {text}"
    );
    (off_work, on_work, fired(g, tid, text), on_out)
}

fn assert_pruned_at_entry(g: &Grammar, tid: u32, text: &str) {
    let (off_work, on_work, fired, out) = walk_both(g, tid, text);
    assert!(
        out.is_empty(),
        "{text}: the required slot cannot unapply, so no output"
    );
    assert!(fired > 0, "{text}: the test must fire");
    assert!(on_work < off_work, "{text}: on {on_work}, off {off_work}");
}

// Machine `Apply_InnerMaterialOnlyInsideTheOuterSuffix_PrunesAtEntry`.
#[test]
fn inner_material_only_inside_the_outer_suffix_prunes_at_entry() {
    let g = grammar();
    assert_pruned_at_entry(&g, INNER_INFIX_OUTER_SUFFIX, "gaka");
}

// Machine `Apply_BoundaryOnlyInfix_DoesNotMakeReachUnbounded`.
#[test]
fn a_boundary_only_infix_does_not_make_the_reach_unbounded() {
    let g = grammar();
    assert_pruned_at_entry(&g, BOUNDARY_INFIX_OUTER, "sagtig");
}

// Machine `Apply_EmptyUnboundedRun_RestoresNoSegmentForTheRequiredSlot`.
#[test]
fn an_empty_unbounded_run_restores_no_segment_for_the_required_slot() {
    let g = grammar();
    assert_pruned_at_entry(&g, FUSION_OUTER, "ktat");
}

// Machine `Apply_UnboundedRunWithOrderedCopiesOrDeletion_RemainsPure(copyRun)`, both cases.
#[test]
fn an_unbounded_run_with_ordered_copies_or_deletion_remains_pure() {
    let g = grammar();
    for tid in [RUN_DELETED_OUTER, RUN_COPIED_OUTER] {
        assert_pruned_at_entry(&g, tid, "sagz");
    }
}

// Machine `Apply_DeletedOptionalSegmentBeforePrefix_DoesNotCountAgainstReach`.
#[test]
fn an_optional_segment_before_the_prefix_does_not_count_against_reach() {
    let g = grammar();
    let table = &g.char_tables[0];
    let shape_with_optional_head = || {
        let plain = pg_grammar_model::segment::segment(table, "kasag").expect("word segments");
        let mut builder = pg_shape::ShapeBuilder::with_interior_capacity(6);
        builder.push_segment(table.lookup_nfd("s").expect("s defined").0);
        builder.set_last_flags(pg_shape::NodeFlags::OPTIONAL);
        for (_, kind, char_def, _) in plain.interior() {
            assert_eq!(kind, pg_shape::NodeKind::Segment);
            builder.push_segment(char_def);
        }
        builder.finish()
    };
    let (off, _) = run_shape(
        &g,
        RequiredSlotLookahead::Off,
        PREFIX_ONLY,
        shape_with_optional_head(),
    );
    let (on, _) = run_shape(
        &g,
        RequiredSlotLookahead::On,
        PREFIX_ONLY,
        shape_with_optional_head(),
    );
    required_slots::take_shadow();
    let (shadow, _) = run_shape(
        &g,
        RequiredSlotLookahead::Shadow,
        PREFIX_ONLY,
        shape_with_optional_head(),
    );
    let counts = required_slots::take_shadow();
    assert!(
        !off.is_empty(),
        "the prefix unapplies behind the optional node"
    );
    assert_eq!(on, off);
    assert_eq!(shadow, off);
    assert_eq!((counts.violated, counts.completed_under_fired), (0, 0));
}

// The sweep behind all of the above: no word's outputs change, and the shadow never reports a violation.
#[test]
fn the_review_grammar_gives_the_same_outputs_on_and_off_and_never_violates_the_shadow() {
    let g = grammar();
    let letters = ["a", "g", "i", "k", "t", "z"];
    let mut words = Vec::new();
    let mut frontier = vec![String::new()];
    for _ in 0..4 {
        frontier = frontier
            .iter()
            .flat_map(|w| letters.map(|c| format!("{w}{c}")))
            .collect();
        words.extend(frontier.iter().cloned());
    }
    let mut nonempty = 0;
    for tid in 0..g.templates.len() as u32 {
        for text in &words {
            let (off, _) = run(&g, RequiredSlotLookahead::Off, tid, text);
            let (on, _) = run(&g, RequiredSlotLookahead::On, tid, text);
            nonempty += usize::from(!off.is_empty());
            assert_eq!(on, off, "template {tid}, word {text}");
            fired(&g, tid, text);
        }
    }
    assert!(
        nonempty > 50,
        "the sweep must exercise real outputs, saw {nonempty}"
    );
}
