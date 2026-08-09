//! Strategy-aware capability accounting: a compiler that cannot represent a construct must not be offered as selectable for a grammar that uses it.
//! See docs/research/pg-foma-strategy-aware-capability-gate-notes.md for the defect this pins and why.

use foma::options::FomaOptions;

use pg_foma::capability::{
    compose_envelope, compose_envelope_for_strategy, default_registry, CharacteristicKind,
    CompileDecision,
};
use pg_foma::compose_budget::ComposeBudget;
use pg_foma::enumerate::{
    enumerate_default, prules_in_order, CandidateRole, EmissionStrategy, LoweredCandidate,
};
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::lowering_adapter::LoweringAdapter;
use pg_foma::plan::Plan;
use pg_foma::replace::SegAlphabet;
use pg_foma::selection::select_plan;
use pg_foma::strategy_coverage::{representation_of, StrategyRepresentation};
use pg_grammar::model::{Grammar, MorphRuleDef};

/// A minimal grammar whose only morphological rule is a `RealizationalRule`, reused verbatim from `capability.rs`'s own fixture so this file is not litigating a second, differently-shaped grammar.
const REALIZATIONAL_XML: &str = r#"<HermitCrabInput><Language><Name>RealizAlone</Name>
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

/// The negative control: nothing here is strategy-conditional, so every strategy must reach the identical verdict and the strategy-aware filter must be a no-op.
const NO_REALIZATIONAL_XML: &str = r#"<HermitCrabInput><Language><Name>PlainAlone</Name>
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

/// `ComposeBudget::unbounded()` is `#[cfg(test)]`-only inside the crate, so an integration test builds the equivalent never-trips budget through the public constructor.
fn unbounded_budget() -> ComposeBudget {
    ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    )
}

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
}

fn enumerated_plan(g: &Grammar) -> Plan {
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(g);
    let phon = PhonologyProbe::new(g);
    enumerate_default(g, &alphabet, &ro, phon.as_ref())
}

/// Two candidates with the identical plan differing only in `EmissionStrategy`; `PlanComposed` first so a filter that silently did nothing would leave it chosen (the roots tie, so no tie-break can rescue the assertion).
fn two_strategy_candidates(plan: &Plan) -> Vec<LoweredCandidate> {
    vec![
        LoweredCandidate {
            label: "plan-composed",
            plan: plan.clone(),
            adapter: LoweringAdapter::ControllablePlanCompose,
            role: CandidateRole::Baseline,
        },
        LoweredCandidate {
            label: "tuned-surface-probed",
            plan: plan.clone(),
            adapter: LoweringAdapter::TunedSurfaceEmit,
            // A whole-grammar adapter never reads a plan, so it is a different compiler, never "the baseline plan's compilation".
            role: CandidateRole::Alternative,
        },
    ]
}

/// A grammar whose only rule is one `PlanComposed`'s proposer emits nothing for must not offer a `PlanComposed` candidate as selectable, and must still offer the compiler that can represent it.
#[test]
fn a_strategy_that_cannot_represent_a_construct_is_not_selectable_for_a_grammar_using_it() {
    let g = load(REALIZATIONAL_XML);
    assert!(
        matches!(g.mrules[0], MorphRuleDef::Realizational(_)),
        "fixture must actually declare a RealizationalRule"
    );

    let plan = enumerated_plan(&g);
    let candidates = two_strategy_candidates(&plan);
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let outcome = select_plan(
        &candidates,
        &g,
        &default_registry(),
        &FomaOptions::default(),
        &alphabet,
        &ro,
        &unbounded_budget(),
    );

    let plan_composed = &outcome.considered[0];
    let tuned = &outcome.considered[1];

    assert!(
        !plan_composed.is_admissible(),
        "PlanComposed's proposer (uflexc) emits no lexc line for a RealizationalRule, so it cannot \
         propose the construct at all -- it must not be selectable. Decision was {:?}",
        plan_composed.decision
    );
    assert!(
        tuned.is_admissible(),
        "TunedSurfaceProbed handles RealizationalRule through emit.rs's shared rule accessors and \
         must stay selectable -- the account is per-strategy, not a blanket refusal. Decision was \
         {:?}",
        tuned.decision
    );
    assert_eq!(
        outcome.chosen,
        Some(1),
        "the only representable strategy must be the chosen one"
    );

    // The refusal has to be legible, not just a bool: it names the strategy, the construct, and the account that produced it.
    let CompileDecision::Refuse(diagnostics) = &plan_composed.decision else {
        panic!("expected a Refuse carrying diagnostics");
    };
    let hit = diagnostics
        .iter()
        .find(|d| d.predicate == "strategy-coverage.construct-not-representable")
        .expect("the refusal must come from the strategy-coverage account");
    assert_eq!(hit.construct, "RealizationalMorphology");
    assert!(
        hit.witness.contains("PlanComposed"),
        "the diagnostic must name the strategy that cannot represent the construct: {}",
        hit.witness
    );
}

/// The "before" half of the evidence: the strategy-blind envelope reaches `ConfirmOnly` on this same grammar and plan, so the old accounting genuinely could not see the hole.
#[test]
fn the_strategy_blind_envelope_cannot_see_the_hole() {
    let g = load(REALIZATIONAL_XML);
    let plan = enumerated_plan(&g);

    assert_eq!(
        compose_envelope(&g, &plan, &default_registry()),
        CompileDecision::ConfirmOnly,
        "the strategy-blind envelope rests at ConfirmOnly -- checking the ConfirmOnly precondition \
         against the union of every compiler's abilities is precisely the defect"
    );
}

/// The memo trap: `GrammarSemantics` memoizes `characteristics()` per grammar, so a second strategy reading the same owner could silently inherit the first one's answer. One shared owner must still give two different answers.
#[test]
fn two_strategies_get_their_own_answers_from_one_shared_semantics() {
    let g = load(REALIZATIONAL_XML);
    let plan = enumerated_plan(&g);
    let registry = default_registry();

    // One shared owner, deliberately warmed first; a strategy-keyed leak would show up here as two equal verdicts.
    let semantics = GrammarSemantics::derive(&g);
    let _ = semantics.characteristics();

    let plan_composed =
        compose_envelope_for_strategy(&semantics, &plan, EmissionStrategy::PlanComposed, &registry);
    let tuned = compose_envelope_for_strategy(
        &semantics,
        &plan,
        EmissionStrategy::TunedSurfaceProbed,
        &registry,
    );

    assert!(
        matches!(plan_composed, CompileDecision::Refuse(_)),
        "PlanComposed must get its OWN answer: {plan_composed:?}"
    );
    assert_eq!(
        tuned,
        CompileDecision::ConfirmOnly,
        "TunedSurfaceProbed must get its OWN answer, not PlanComposed's"
    );
    assert_ne!(
        plan_composed, tuned,
        "two strategies asking one shared GrammarSemantics must not receive the same verdict"
    );

    // Order-independence: a memo poisoned by whoever asked first would fail this while passing the pair above.
    let semantics_reversed = GrammarSemantics::derive(&g);
    let tuned_first = compose_envelope_for_strategy(
        &semantics_reversed,
        &plan,
        EmissionStrategy::TunedSurfaceProbed,
        &registry,
    );
    let plan_composed_second = compose_envelope_for_strategy(
        &semantics_reversed,
        &plan,
        EmissionStrategy::PlanComposed,
        &registry,
    );
    assert_eq!(tuned_first, tuned);
    assert_eq!(plan_composed_second, plan_composed);
}

/// The negative control: with no strategy-conditional construct, the filter must be a no-op -- otherwise the test above would be satisfied by a filter that simply refused `PlanComposed` always.
#[test]
fn a_grammar_using_no_strategy_conditional_construct_is_unaffected() {
    let g = load(NO_REALIZATIONAL_XML);
    assert!(
        g.mrules.is_empty(),
        "control fixture must declare no morphological rules"
    );

    let plan = enumerated_plan(&g);
    let registry = default_registry();
    let blind = compose_envelope(&g, &plan, &registry);
    let semantics = GrammarSemantics::derive(&g);

    for strategy in [
        EmissionStrategy::PlanComposed,
        EmissionStrategy::TunedSurfaceProbed,
        EmissionStrategy::TemplatedUnderlyingTokens,
    ] {
        assert_eq!(
            compose_envelope_for_strategy(&semantics, &plan, strategy, &registry),
            blind,
            "{strategy:?} must reach the strategy-blind verdict on a grammar with no \
             strategy-conditional construct"
        );
    }

    let candidates = two_strategy_candidates(&plan);
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let outcome = select_plan(
        &candidates,
        &g,
        &registry,
        &FomaOptions::default(),
        &alphabet,
        &ro,
        &unbounded_budget(),
    );
    assert!(outcome.considered.iter().all(|c| c.is_admissible()));
    assert_eq!(
        outcome.chosen,
        Some(0),
        "with nothing to filter, selection must be exactly what it was before"
    );
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
    for xml in [REALIZATIONAL_XML, NO_REALIZATIONAL_XML] {
        let g = load(xml);
        let plan = enumerated_plan(&g);
        let semantics = GrammarSemantics::derive(&g);
        let blind = compose_envelope(&g, &plan, &registry);
        for strategy in [
            EmissionStrategy::PlanComposed,
            EmissionStrategy::TunedSurfaceProbed,
            EmissionStrategy::TemplatedUnderlyingTokens,
        ] {
            let aware = compose_envelope_for_strategy(&semantics, &plan, strategy, &registry);
            assert!(
                rank(&aware) >= rank(&blind),
                "{strategy:?} raised the decision from {blind:?} to {aware:?}"
            );
        }
    }
}

/// The hole is per-strategy: a table answering `CannotRepresent` for every strategy would pass the selection test above while being just as blind as what it replaced.
#[test]
fn the_account_is_per_strategy_not_a_blanket_refusal() {
    assert_eq!(
        representation_of(
            EmissionStrategy::PlanComposed,
            CharacteristicKind::RealizationalMorphology
        )
        .representation,
        StrategyRepresentation::CannotRepresent
    );
    for strategy in [
        EmissionStrategy::TunedSurfaceProbed,
        EmissionStrategy::TemplatedUnderlyingTokens,
    ] {
        assert_eq!(
            representation_of(strategy, CharacteristicKind::RealizationalMorphology).representation,
            StrategyRepresentation::Represents,
            "{strategy:?}"
        );
    }
}
