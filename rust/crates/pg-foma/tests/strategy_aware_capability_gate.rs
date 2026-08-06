//! Strategy-AWARE capability accounting: a compiler that cannot represent a construct must not be
//! offered as selectable for a grammar that uses it.
//!
//! # The defect this pins
//! `capability::Disposition::ConfirmOnly` is defined as *"Recall-preserving only if the proposer
//! proposes the superset."* That precondition is a claim about a PROPOSER, and the capability layer
//! had no proposer in hand: `characterize(g: &Grammar)` takes no strategy, and
//! `enumerate::EmissionStrategy` appeared nowhere in `capability.rs`, `coverage_ledger.rs`,
//! `conformance_coverage.rs` or `gate.rs`. So a `ConfirmOnly` disposition was being checked against
//! the UNION of every compiler's abilities.
//!
//! The consequence was measured, not hypothesized: `Compounding` rested at a non-refusing
//! disposition while `crate::uflexc` -- the only lexicon emitter `EmissionStrategy::PlanComposed`
//! has -- emitted a structurally single-root continuation graph that could not propose ANY compound.
//! One compiler's coverage was silently inherited by all three, and the ledger's cited evidence for
//! it (`tests/cover_compounding.rs`) exercised only `FomaAnalyzer::new`, i.e.
//! `EmissionStrategy::TunedSurfaceProbed`.
//!
//! That specific hole is now fixed (uflexc grew a bounded compound loop). These tests pin the
//! ACCOUNTING, against a hole of the identical shape that is still live today:
//! `MorphRuleDef::Realizational`. `uflexc`'s mrule loop reports every such rule in `skipped` as
//! `kind=realizational-rule` and `continue`s past it -- no lexc line is written for the rule at all,
//! so `PlanComposed`'s proposer returns zero candidates for any word requiring it, while both
//! whole-grammar compilers handle it through `emit.rs`'s shared rule accessors.
//!
//! Synthetic, delanguaged fixtures only (this repo's standing rule for conformance-shaped grammars).

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

/// A minimal grammar whose ONLY morphological rule is a `RealizationalRule` -- reused verbatim from
/// `capability.rs`'s own `compose_envelope_confirm_only_for_realizational_rule_alone` fixture, so
/// this file is not litigating a second, differently-shaped grammar's characterization.
const REALIZATIONAL_XML: &str = r#"<HermitCrabInput><Language><Name>RealizAlone</Name>
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

/// The same grammar with the realizational rule removed -- the negative control. Nothing about this
/// grammar is strategy-conditional, so every strategy must reach the identical verdict and the
/// strategy-aware filter must be a no-op on it.
const NO_REALIZATIONAL_XML: &str = r#"<HermitCrabInput><Language><Name>PlainAlone</Name>
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

/// `ComposeBudget::unbounded()` is `#[cfg(test)]`-only inside the crate, so an integration test
/// builds the equivalent never-trips budget through the public constructor (the same shape
/// `tests/grammar_semantics_owner_gate.rs` uses).
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

/// Two candidates carrying the IDENTICAL plan and differing ONLY in `EmissionStrategy` -- which is
/// exactly the axis a strategy-blind envelope cannot see. `PlanComposed` first so a filter that
/// silently did nothing would leave it chosen (it is the minimum-`NodeId` candidate: the roots are
/// equal, so the tie-break cannot rescue the assertion).
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
            // A whole-grammar adapter never reads a plan, so it is not "the baseline plan's
            // compilation" under any reading -- it is a different compiler.
            role: CandidateRole::Alternative,
        },
    ]
}

/// THE TEST THE ORIGINAL DEFECT NEEDED. A grammar whose only morphological rule is one
/// `PlanComposed`'s proposer emits nothing for must not offer a `PlanComposed` candidate as
/// selectable -- and must still offer the compiler that CAN represent it.
///
/// Falsified before the fix by construction: `select_plan` called `compose_envelope_with_semantics`,
/// which takes no strategy at all, so both candidates reached the same `ConfirmOnly` decision, both
/// were admissible, and index 0 (`PlanComposed`) was chosen.
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

    // The refusal has to be legible, not just a bool: it names the strategy, the construct, and the
    // account that produced it.
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

/// The strategy-BLIND envelope reaches `ConfirmOnly` on exactly the same grammar and plan -- i.e.
/// the old accounting genuinely could not see the hole, and the assertion above is not passing for
/// some incidental reason. This is the "before" half of the before/after evidence, expressed as a
/// standing test rather than a one-off measurement.
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

/// THE MEMO TRAP, pinned. `GrammarSemantics` memoizes `characteristics()` as a function of the
/// GRAMMAR alone. That memo is deliberately kept (see `strategy_coverage`'s module doc for the
/// reasoning), so the risk is that a second strategy reading through the same owner silently gets
/// the first one's answer -- reintroducing the inheritance bug in a harder-to-see place. One shared
/// `GrammarSemantics`, two strategies, two different answers.
#[test]
fn two_strategies_get_their_own_answers_from_one_shared_semantics() {
    let g = load(REALIZATIONAL_XML);
    let plan = enumerated_plan(&g);
    let registry = default_registry();

    // ONE owner, shared -- and deliberately warmed first, so the memo is already populated by the
    // time either strategy asks. A strategy-keyed answer that leaked out of the grammar-only memo
    // would show up here as two equal verdicts.
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

    // Order-independence: asking in the reverse order gives the same two answers. A memo poisoned
    // by whoever asked first would fail this and pass the pair above.
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

/// The negative control: on a grammar that uses no strategy-conditional construct, the
/// strategy-aware filter changes nothing at all -- same decision for every strategy, same decision
/// as the strategy-blind envelope, and the first candidate still chosen. Without this, the test
/// above would be satisfied by a filter that simply refused `PlanComposed` always.
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

/// The account can only LOWER a decision, never raise it. `compose_envelope_for_strategy` starts
/// from the strategy-blind answer and `meet`s the per-strategy rows in, and `meet` is a greatest
/// lower bound -- so no grammar/strategy pair can be made MORE admissible by this change. Checked
/// over both fixtures and every strategy rather than argued from the code.
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

/// The table itself, at the point that matters: the hole is per-strategy, and the two whole-grammar
/// compilers do not share it. A table that answered `CannotRepresent` for every strategy would pass
/// the selection test above while being just as blind as what it replaced.
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
