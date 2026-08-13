//! Contract for the single planner boundary in the FST -> CandidateFilter -> HC pipeline.

use pg_foma::cost_envelope::CostEnvelope;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::lowering_adapter::LoweringAdapter;
use pg_foma::strategy_planner::{
    plan_for_grammar, CandidateFilterProfile, CatalogContext, CatalogRevision, HcAuthority,
    MachineObligationId, ObligationLedger, SelectionPolicy, StrategyDisposition,
};

const REALIZATIONAL_XML: &str = r#"<HermitCrabInput><Language><Name>PlannerContract</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1">
      <Name>S</Name>
      <MorphologicalRuleDefinitions>
        <RealizationalRule id="rr1">
          <Name>Realiz</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="sub1">
              <MorphologicalInput><PhoneticSequence id="s0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="s0" /></MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
        </RealizationalRule>
      </MorphologicalRuleDefinitions>
      <LexicalEntries>
        <LexicalEntry id="e1">
          <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

const PLAIN_XML: &str = r#"<HermitCrabInput><Language><Name>PlannerPlain</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1">
      <Name>S</Name>
      <LexicalEntries>
        <LexicalEntry id="e1">
          <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

fn catalog() -> CatalogContext {
    fn id(value: &str) -> MachineObligationId {
        MachineObligationId::new(value).expect("nonblank opaque Machine obligation ID")
    }

    CatalogContext::new(
        CatalogRevision::new("machine:test-catalog-revision")
            .expect("nonblank pinned catalog revision"),
        ObligationLedger::new(
            vec![id("test/atomic/realizational-rule")],
            vec![id("test/configuration/realizational-copy-once")],
            vec![id("test/interaction/realizational-selects-allomorph")],
            vec![id("test/schedule/morph-before-confirm")],
        ),
    )
}

fn high_cost() -> CostEnvelope {
    CostEnvelope::high("test-cost-model/v1", 10_000, 100)
        .expect("valid high projected-cost evidence")
}

#[test]
fn automatic_planning_preserves_catalog_filter_and_hc_authority_without_cost_refusal() {
    let grammar = pg_grammar::load(REALIZATIONAL_XML).expect("valid synthetic grammar");
    let semantics = GrammarSemantics::derive(&grammar);

    let outcome = plan_for_grammar(
        &semantics,
        SelectionPolicy::Automatic,
        catalog(),
        CandidateFilterProfile::StructuralV1,
        high_cost(),
    );

    assert_eq!(outcome.requested_policy(), SelectionPolicy::Automatic);
    assert_eq!(
        outcome.choice().disposition(),
        &StrategyDisposition::ConfirmOnly,
        "high or unknown cost is operational evidence, never a semantic refusal"
    );
    assert_eq!(
        outcome.choice().physical_adapter(),
        Some(LoweringAdapter::TunedSurfaceEmit),
        "automatic policy uses the existing viable-backend preference"
    );
    assert_eq!(
        outcome.evaluated_adapters(),
        &[
            LoweringAdapter::TunedSurfaceEmit,
            LoweringAdapter::TemplatedUnderlyingEmit,
            LoweringAdapter::ControllablePlanCompose,
        ],
        "automatic policy evaluates the complete existing preference space once"
    );
    assert_eq!(
        outcome.candidate_filter_profile(),
        CandidateFilterProfile::StructuralV1
    );
    assert_eq!(outcome.hc_authority(), HcAuthority::HermitCrabConfirm);
    assert!(outcome.projected_cost().is_high());
    assert_eq!(outcome.projected_cost().revision(), "test-cost-model/v1");

    let choice = outcome.choice();
    assert_eq!(choice.catalog_revision(), catalog().revision());
    assert_eq!(choice.obligations().atomic().len(), 1);
    assert_eq!(choice.obligations().within_rule_configuration().len(), 1);
    assert_eq!(choice.obligations().cross_rule_interaction().len(), 1);
    assert_eq!(choice.obligations().schedule().len(), 1);
}

#[test]
fn refused_explicit_adapter_has_no_realization_and_does_not_fall_back() {
    let grammar = pg_grammar::load(REALIZATIONAL_XML).expect("valid synthetic grammar");
    let semantics = GrammarSemantics::derive(&grammar);
    let requested = SelectionPolicy::Explicit(LoweringAdapter::ControllablePlanCompose);

    let outcome = plan_for_grammar(
        &semantics,
        requested,
        catalog(),
        CandidateFilterProfile::StructuralV1,
        high_cost(),
    );

    assert_eq!(outcome.requested_policy(), requested);
    let StrategyDisposition::Refuse(diagnostics) = outcome.choice().disposition() else {
        panic!(
            "PlanComposed cannot represent this RealizationalRule and must refuse, not fall back: {:?}",
            outcome.choice().disposition()
        );
    };
    assert!(!diagnostics.is_empty(), "refusal must remain attributable");
    assert_eq!(outcome.choice().physical_adapter(), None);
    assert_eq!(outcome.choice().plan_id(), None);
    assert_eq!(
        outcome.evaluated_adapters(),
        &[LoweringAdapter::ControllablePlanCompose],
        "an explicit request evaluates only the requested adapter and cannot hide a fallback"
    );
    assert_eq!(outcome.hc_authority(), HcAuthority::HermitCrabConfirm);
    assert!(outcome.projected_cost().is_high());
}

#[test]
fn admitted_plan_composed_choice_carries_the_content_addressed_plan() {
    let grammar = pg_grammar::load(PLAIN_XML).expect("valid synthetic grammar");
    let semantics = GrammarSemantics::derive(&grammar);
    let requested = SelectionPolicy::Explicit(LoweringAdapter::ControllablePlanCompose);
    let cost = CostEnvelope::bounded("test-cost-model/v1", 10)
        .expect("valid bounded projected-cost evidence");

    let outcome = plan_for_grammar(
        &semantics,
        requested,
        catalog(),
        CandidateFilterProfile::Off,
        cost,
    );

    assert_eq!(outcome.requested_policy(), requested);
    assert!(matches!(
        outcome.choice().disposition(),
        StrategyDisposition::Admit | StrategyDisposition::ConfirmOnly
    ));
    assert_eq!(
        outcome.choice().physical_adapter(),
        Some(LoweringAdapter::ControllablePlanCompose)
    );
    assert_eq!(
        outcome.evaluated_adapters(),
        &[LoweringAdapter::ControllablePlanCompose]
    );
    assert!(
        outcome.choice().plan_id().is_some(),
        "an admitted plan-interpreting adapter must name the content-addressed plan it will build"
    );
    assert_eq!(
        outcome.candidate_filter_profile(),
        CandidateFilterProfile::Off
    );
    assert!(!outcome.projected_cost().is_high());
}

#[test]
fn high_cost_classification_requires_the_estimate_to_cross_its_threshold() {
    assert!(CostEnvelope::high("test-cost-model/v1", 100, 100).is_err());
    assert!(CostEnvelope::high("test-cost-model/v1", 1, 100).is_err());
    assert!(CostEnvelope::high("test-cost-model/v1", 101, 100).is_ok());
}
