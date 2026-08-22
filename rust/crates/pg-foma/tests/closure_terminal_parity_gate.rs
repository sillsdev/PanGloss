use pg_foma::characterization::{
    trace_tuned_surface_closure_for_test, ClosureStopReason, ClosureTerminal, ClosureTestLimits,
    ClosureWalkMode,
};
use pg_foma::resource_envelope::{ResourceEnvelope, ResourceEnvelopeId};

const FINITE_CHAIN_XML: &str = r#"<HermitCrabInput><Language><Name>TotalClosureContract</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>
    <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cf"><Representations><Representation>f</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cg"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions></CharacterDefinitionTable>
  <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
  <Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mr1"><Name>Main</Name>
    <MorphologicalRuleDefinitions>
      <MorphologicalRule id="mr1" multipleApplication="9" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>finite-chain</Name>
        <MorphologicalSubrules><MorphologicalSubrule id="s1">
          <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><InsertSegments><PhoneticShape>f</PhoneticShape></InsertSegments><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>g</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>R1</MorphemeId>
      </MorphologicalRule>
    </MorphologicalRuleDefinitions>
    <LexicalEntries><LexicalEntry id="e1" partOfSpeech="posV"><Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs><MorphemeId>ROOT</MorphemeId></LexicalEntry></LexicalEntries>
  </Stratum></Strata>
</Language></HermitCrabInput>"#;

fn trace(
    mode: ClosureWalkMode,
    work_cap: usize,
    depth_cap: usize,
) -> pg_foma::characterization::CharacterizationResult {
    let grammar = pg_grammar::load(FINITE_CHAIN_XML).expect("finite closure fixture must load");
    let envelope = ResourceEnvelope::for_id(ResourceEnvelopeId::ManagedV1);
    trace_tuned_surface_closure_for_test(
        &grammar,
        &envelope,
        ClosureTestLimits {
            work_cap,
            depth_cap,
        },
        mode,
    )
}

#[test]
fn work_boundary_is_total_and_characterization_matches_production() {
    let generous = trace(ClosureWalkMode::Characterization, 10_000, 64);
    assert_eq!(generous.terminal, ClosureTerminal::Complete);
    assert!(generous.evidence.worklist_empty);
    assert_eq!(generous.evidence.pending_successor_count, 0);
    let required = generous.evidence.rule_pairs_visited;
    assert!(required > 0);

    let below = trace(ClosureWalkMode::Characterization, required - 1, 64);
    assert_eq!(
        below.terminal,
        ClosureTerminal::Incomplete(ClosureStopReason::WorkBudgetReached)
    );
    assert!(!below.evidence.worklist_empty);
    assert!(below.evidence.pending_successor_count > 0);

    for work_cap in [required, required + 1] {
        let observed = trace(ClosureWalkMode::Characterization, work_cap, 64);
        let produced = trace(ClosureWalkMode::Production, work_cap, 64);
        assert_eq!(observed.terminal, ClosureTerminal::Complete);
        assert_eq!(produced.terminal, ClosureTerminal::Complete);
        assert_eq!(observed.evidence, produced.evidence);
        assert!(observed.evidence.worklist_empty);
        assert_eq!(observed.evidence.pending_successor_count, 0);
    }
}

#[test]
fn live_successor_at_depth_boundary_is_reported_not_silently_dropped() {
    for mode in [
        ClosureWalkMode::Characterization,
        ClosureWalkMode::Production,
    ] {
        let result = trace(mode, 10_000, 4);
        assert_eq!(
            result.terminal,
            ClosureTerminal::Incomplete(ClosureStopReason::DepthBudgetReached)
        );
        assert!(!result.evidence.worklist_empty);
        assert!(result.evidence.pending_successor_count > 0);
        assert_eq!(result.evidence.pending_rule_ordinals, vec![0]);
        assert_eq!(result.evidence.maximum_depth, 4);
        assert_eq!(result.evidence.per_depth_counts.len(), 5);
    }
}
