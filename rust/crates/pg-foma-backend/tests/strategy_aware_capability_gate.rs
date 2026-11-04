//! Strategy-aware capability accounting: a compiler that cannot represent a construct must not be offered as selectable for a grammar that uses it.
//! See docs/research/pg-foma-strategy-aware-capability-gate-notes.md for the defect this pins and why.

use pg_foma::capability::{
    compose_envelope, compose_envelope_for_strategy, default_registry, CharacteristicKind,
    CompileDecision,
};
use pg_foma::enumerate::{enumerate_default, prules_in_order, EmissionStrategy};
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::plan::Plan;
use pg_foma::strategy_coverage::{representation_of, StrategyRepresentation};
use pg_grammar::model::Grammar;

/// Pure-ablaut `MorphologicalRule` (`Role::Process`).
const ABLAUT_XML: &str = r#"<HermitCrabInput><Language><Name>AblautAlone</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRules="mrAblaut">
      <Name>S</Name>
      <MorphologicalRuleDefinitions>
        <MorphologicalRule id="mrAblaut">
          <Name>ablaut</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="subAblaut">
              <MorphologicalInput><PhoneticSequence id="pA"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><ModifyFromInput index="pA"><SimpleContext naturalClass="ncAll" /></ModifyFromInput></MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
        </MorphologicalRule>
      </MorphologicalRuleDefinitions>
      <LexicalEntries>
        <LexicalEntry id="e1">
          <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

/// The negative control: nothing here is strategy-conditional, so every strategy must reach the identical verdict and the strategy-aware filter must be a no-op.
const NO_ABLAUT_XML: &str = r#"<HermitCrabInput><Language><Name>PlainAlone</Name>
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

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
}

fn enumerated_plan(g: &Grammar) -> Plan {
    let ro = prules_in_order(g);
    let phon = PhonologyProbe::new(g);
    enumerate_default(g, &ro, phon.as_ref())
}

/// The "before" half of the evidence: the strategy-blind envelope reaches `ConfirmOnly` on this same grammar and plan, so the old accounting genuinely could not see the hole.
#[test]
fn the_strategy_blind_envelope_cannot_see_the_hole() {
    let g = load(ABLAUT_XML);
    let plan = enumerated_plan(&g);

    assert_eq!(
        compose_envelope(&g, &plan, &default_registry()),
        CompileDecision::ConfirmOnly,
        "the strategy-blind envelope rests at ConfirmOnly -- checking the ConfirmOnly precondition \
         against the union of every compiler's abilities is precisely the defect"
    );
}

/// The memo trap: `GrammarSemantics` memoizes `characteristics()` per grammar, so asking in either order through one shared owner must give the same verdict as a fresh owner.
#[test]
fn a_shared_semantics_gives_the_same_answer_in_either_order() {
    let g = load(ABLAUT_XML);
    let plan = enumerated_plan(&g);
    let registry = default_registry();

    let semantics = GrammarSemantics::derive(&g);
    let _ = semantics.characteristics();
    let warmed = compose_envelope_for_strategy(
        &semantics,
        &plan,
        EmissionStrategy::TunedSurfaceProbed,
        &registry,
    );
    let fresh = compose_envelope_for_strategy(
        &GrammarSemantics::derive(&g),
        &plan,
        EmissionStrategy::TunedSurfaceProbed,
        &registry,
    );
    assert_eq!(warmed, CompileDecision::ConfirmOnly);
    assert_eq!(warmed, fresh);
}

/// The negative control: with no strategy-conditional construct, the filter must be a no-op -- otherwise a filter that simply refused always would pass the tests above.
#[test]
fn a_grammar_using_no_strategy_conditional_construct_is_unaffected() {
    let g = load(NO_ABLAUT_XML);
    assert!(
        g.mrules.is_empty(),
        "control fixture must declare no morphological rules"
    );

    let plan = enumerated_plan(&g);
    let registry = default_registry();
    let blind = compose_envelope(&g, &plan, &registry);
    let semantics = GrammarSemantics::derive(&g);

    for &strategy in pg_foma::strategy_coverage::ALL_STRATEGIES {
        assert_eq!(
            compose_envelope_for_strategy(&semantics, &plan, strategy, &registry),
            blind,
            "{strategy:?} must reach the strategy-blind verdict on a grammar with no \
             strategy-conditional construct"
        );
    }
}

/// The account can only lower a decision, never raise it: `meet` is a greatest lower bound, so no grammar/strategy pair can become more admissible by this change.
#[test]
fn the_strategy_account_never_raises_a_decision() {
    fn rank(decision: &CompileDecision) -> u8 {
        match decision {
            CompileDecision::Admit => 0,
            CompileDecision::ConfirmOnly => 1,
            CompileDecision::Refuse(_) => 2,
        }
    }

    let registry = default_registry();
    for xml in [ABLAUT_XML, NO_ABLAUT_XML] {
        let g = load(xml);
        let plan = enumerated_plan(&g);
        let semantics = GrammarSemantics::derive(&g);
        let blind = compose_envelope(&g, &plan, &registry);
        for &strategy in pg_foma::strategy_coverage::ALL_STRATEGIES {
            let aware = compose_envelope_for_strategy(&semantics, &plan, strategy, &registry);
            assert!(
                rank(&aware) >= rank(&blind),
                "{strategy:?} raised the decision from {blind:?} to {aware:?}"
            );
        }
    }
}

/// `TunedSurfaceProbed` represents `ProcessMorphology`, so the strategy account does not lower the verdict for the ablaut grammar.
#[test]
fn tuned_surface_probed_represents_process_morphology() {
    assert_eq!(
        representation_of(
            EmissionStrategy::TunedSurfaceProbed,
            CharacteristicKind::ProcessMorphology
        )
        .representation,
        StrategyRepresentation::Represents
    );
}
