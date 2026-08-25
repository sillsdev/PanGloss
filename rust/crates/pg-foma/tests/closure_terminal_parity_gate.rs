use pg_foma::characterization::{
    characterize_tuned_surface_closure, characterize_tuned_surface_closure_for_test,
    CharacterizationResult, ClosureStopReason, ClosureTerminal, ClosureTestLimits,
};
use pg_foma::emit::{
    emit_tuned_surface_for_envelope, emit_tuned_surface_with_closure_limits_for_test, EmitResult,
    FomaTier,
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

fn observe(work_cap: usize, depth_cap: usize) -> CharacterizationResult {
    let grammar = pg_grammar::load(FINITE_CHAIN_XML).expect("finite closure fixture must load");
    let envelope = ResourceEnvelope::for_id(ResourceEnvelopeId::ManagedV1);
    characterize_tuned_surface_closure_for_test(
        &grammar,
        &envelope,
        ClosureTestLimits {
            work_cap,
            depth_cap,
        },
    )
}

fn construct(work_cap: usize, depth_cap: usize) -> EmitResult {
    let grammar = pg_grammar::load(FINITE_CHAIN_XML).expect("finite closure fixture must load");
    let envelope = ResourceEnvelope::for_id(ResourceEnvelopeId::ManagedV1);
    emit_tuned_surface_with_closure_limits_for_test(
        &grammar,
        &envelope,
        ClosureTestLimits {
            work_cap,
            depth_cap,
        },
    )
}

#[test]
fn work_boundary_is_total_and_characterization_matches_production() {
    let envelope = ResourceEnvelope::for_id(ResourceEnvelopeId::ManagedV1);
    let generous = observe(10_000, 64);
    assert_eq!(generous.terminal, ClosureTerminal::Complete);
    assert!(generous.evidence.worklist_empty);
    assert_eq!(generous.evidence.pending_successor_count, 0);
    assert_eq!(generous.evidence.envelope_digest, envelope.digest());
    let required = generous.evidence.rule_pairs_visited;
    assert!(required > 0);

    let below = observe(required - 1, 64);
    assert_eq!(
        below.terminal,
        ClosureTerminal::Incomplete(ClosureStopReason::WorkBudgetReached)
    );
    assert!(!below.evidence.worklist_empty);
    assert!(below.evidence.pending_successor_count > 0);
    let refused = construct(required - 1, 64);
    assert!(refused.lexc_source.is_empty());
    assert!(matches!(refused.report.tier, FomaTier::Unsupported { .. }));
    assert_eq!(refused.report.closure_evidence.as_ref(), Some(&below));

    for work_cap in [required, required + 1] {
        let observed = observe(work_cap, 64);
        let produced = construct(work_cap, 64);
        assert_eq!(observed.terminal, ClosureTerminal::Complete);
        assert_eq!(produced.report.closure_evidence.as_ref(), Some(&observed));
        assert!(!produced.lexc_source.is_empty());
        assert!(observed.evidence.worklist_empty);
        assert_eq!(observed.evidence.pending_successor_count, 0);
    }
}

#[test]
fn live_successor_at_depth_boundary_is_reported_not_silently_dropped() {
    let result = observe(10_000, 4);
    assert_eq!(
        result.terminal,
        ClosureTerminal::Incomplete(ClosureStopReason::DepthBudgetReached)
    );
    assert!(!result.evidence.worklist_empty);
    assert!(result.evidence.pending_successor_count > 0);
    assert_eq!(result.evidence.pending_rule_ordinals, vec![0]);
    assert_eq!(result.evidence.maximum_depth, 4);
    assert_eq!(result.evidence.per_depth_counts.len(), 5);

    let refused = construct(10_000, 4);
    assert!(refused.lexc_source.is_empty());
    assert!(matches!(refused.report.tier, FomaTier::Unsupported { .. }));
    assert_eq!(refused.report.closure_evidence.as_ref(), Some(&result));
}

#[test]
fn normal_product_entrypoints_use_the_selected_envelope_and_same_production_trace() {
    let grammar = pg_grammar::load(FINITE_CHAIN_XML).expect("finite closure fixture must load");
    let envelope = ResourceEnvelope::for_id(ResourceEnvelopeId::ManagedV1);

    let observed = characterize_tuned_surface_closure(&grammar, &envelope);
    let produced = emit_tuned_surface_for_envelope(&grammar, &envelope);

    assert_eq!(observed.terminal, ClosureTerminal::Complete);
    assert_eq!(observed.evidence.envelope_digest, envelope.digest());
    assert!(observed.evidence.worklist_empty);
    assert_eq!(observed.evidence.pending_successor_count, 0);
    assert_eq!(produced.report.closure_evidence.as_ref(), Some(&observed));
    assert!(!produced.lexc_source.is_empty());
}

#[test]
fn unsupported_construction_is_a_typed_refusal_not_false_completion() {
    let mut grammar = pg_grammar::load(FINITE_CHAIN_XML).expect("finite closure fixture must load");
    for stratum in &mut grammar.strata {
        stratum.entries.clear();
    }
    let envelope = ResourceEnvelope::for_id(ResourceEnvelopeId::ManagedV1);

    let produced = emit_tuned_surface_for_envelope(&grammar, &envelope);
    assert!(produced.lexc_source.is_empty());
    let evidence = produced
        .report
        .closure_evidence
        .expect("unsupported named-envelope construction must retain terminal evidence");
    assert_eq!(
        evidence.terminal,
        ClosureTerminal::Refused(ClosureStopReason::UnsupportedTransition)
    );
    assert_eq!(
        characterize_tuned_surface_closure(&grammar, &envelope),
        evidence
    );
}

#[test]
fn unsupported_empty_roots_remain_a_typed_refusal() {
    let mut grammar = pg_grammar::load(FINITE_CHAIN_XML).expect("fixture must load");
    for stratum in &mut grammar.strata {
        stratum.entries.clear();
    }
    let envelope = ResourceEnvelope::for_id(ResourceEnvelopeId::ManagedV1);
    let result = emit_tuned_surface_for_envelope(&grammar, &envelope);
    let evidence = result
        .report
        .closure_evidence
        .as_ref()
        .expect("unsupported construction retains terminal evidence");
    assert_eq!(
        evidence.terminal,
        ClosureTerminal::Refused(ClosureStopReason::UnsupportedTransition)
    );
    assert!(result.lexc_source.is_empty());
}
