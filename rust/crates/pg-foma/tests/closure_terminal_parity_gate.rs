use pg_foma::characterization::{
    characterize_tuned_surface_closure, characterize_tuned_surface_closure_for_test,
    CharacterizationResult, ClosureStopReason, ClosureTerminal, ClosureTestLimits,
};
use pg_foma::emit::{
    emit_tuned_surface_for_envelope, emit_tuned_surface_for_request,
    emit_tuned_surface_with_closure_limits_for_test, EmitResult, FomaTier,
};
use pg_foma::resource_envelope::{
    CompileEnvelopeRequest, ResourceEnvelope, ResourceEnvelopeId,
};

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

fn bounded_branching_xml(rule_count: usize) -> String {
    let rule_ids = (0..rule_count)
        .map(|index| format!("mr{index}"))
        .collect::<Vec<_>>()
        .join(" ");
    let rules = (0..rule_count)
        .map(|index| {
            format!(
                r#"<MorphologicalRule id="mr{index}" multipleApplication="1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>branch-{index}</Name>
        <MorphologicalSubrules><MorphologicalSubrule id="s{index}"><MorphologicalInput><PhoneticSequence id="stem{index}"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
        <MorphologicalOutput><InsertSegments><PhoneticShape>f</PhoneticShape></InsertSegments><CopyFromInput index="stem{index}" /><InsertSegments><PhoneticShape>g</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>R{index}</MorphemeId></MorphologicalRule>"#
            )
        })
        .collect::<String>();
    let mut xml = FINITE_CHAIN_XML.replace(
        "morphologicalRules=\"mr1\"",
        &format!("morphologicalRules=\"{rule_ids}\""),
    );
    let definitions_start = xml
        .find("<MorphologicalRuleDefinitions>")
        .expect("fixture has morphology definitions");
    let rules_start = definitions_start + "<MorphologicalRuleDefinitions>".len();
    let rules_end = xml
        .find("</MorphologicalRuleDefinitions>")
        .expect("fixture closes morphology definitions");
    xml.replace_range(rules_start..rules_end, &rules);
    xml
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
fn only_a_terminal_failure_can_authorize_a_linked_retry() {
    let ordinary = pg_grammar::load(FINITE_CHAIN_XML).expect("finite closure fixture must load");
    let complete_request = CompileEnvelopeRequest::try_new(ResourceEnvelopeId::ManagedV1)
        .expect("managed request must be constructible");
    let complete = emit_tuned_surface_for_request(&ordinary, &complete_request);
    assert_eq!(
        complete
            .report
            .closure_evidence
            .as_ref()
            .expect("named attempt retains closure evidence")
            .terminal,
        ClosureTerminal::Complete
    );
    assert!(complete.retry_authorization().is_none());

    let expensive_xml = bounded_branching_xml(10);
    let expensive = pg_grammar::load(&expensive_xml).expect("bounded retry fixture must load");
    let first_request = CompileEnvelopeRequest::try_new(ResourceEnvelopeId::ManagedV1)
        .expect("managed request must be constructible");
    let first = emit_tuned_surface_for_request(&expensive, &first_request);
    let first_terminal = first
        .report
        .closure_evidence
        .as_ref()
        .expect("failed named attempt retains closure evidence");
    assert!(matches!(
        first_terminal.terminal,
        ClosureTerminal::Incomplete(_) | ClosureTerminal::Refused(_)
    ));
    let authorization = first
        .retry_authorization()
        .expect("terminal failure alone authorizes retry");
    assert!(CompileEnvelopeRequest::retry_from(authorization, ResourceEnvelopeId::ManagedV1)
        .is_err());

    let retry = CompileEnvelopeRequest::retry_from(
        authorization,
        ResourceEnvelopeId::TunedSurfaceWork10kV1,
    )
    .expect("caller may explicitly retry under a different named envelope");
    assert_ne!(retry.attempt_id(), first_request.attempt_id());
    assert_eq!(retry.retry_of(), Some(first_request.attempt_id()));
    assert_eq!(retry.prior_closure(), Some(first_terminal));

    let second = emit_tuned_surface_for_request(&expensive, &retry);
    let second_terminal = second
        .report
        .closure_evidence
        .as_ref()
        .expect("retry retains its own fresh closure evidence");
    assert_ne!(second_terminal.evidence.envelope_digest, first_terminal.evidence.envelope_digest);
    assert_eq!(second_terminal.terminal, ClosureTerminal::Complete);
    assert!(second.retry_authorization().is_none());
}
