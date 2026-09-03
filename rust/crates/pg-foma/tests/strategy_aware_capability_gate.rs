//! Strategy-aware capability accounting: a compiler that cannot represent a construct must not be offered as selectable for a grammar that uses it.
//! See docs/research/pg-foma-strategy-aware-capability-gate-notes.md for the defect this pins and why.

use foma::options::FomaOptions;

use pg_conformance_fixtures::{discover_scoped, ConformanceScope, FixtureRef, Root};
use pg_foma::backend_selection::select_backends_for_grammar;
use pg_foma::capability::{
    compose_envelope, compose_envelope_for_strategy, default_registry, CharacteristicKind,
    CompileDecision,
};
use pg_foma::enumerate::{
    enumerate_default, prules_in_order, CandidateRole, EmissionStrategy, LoweredCandidate,
};
use pg_foma::faithfulness_coverage::{
    observe_fixture_containment, ContainmentOutcome, NotAttemptedReason,
};
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::lowering_adapter::LoweringAdapter;
use pg_foma::plan::Plan;
use pg_foma::replace::SegAlphabet;
use pg_foma::selection::select_plan;
use pg_foma::strategy_coverage::{
    representation_of, unrepresentable_kinds, StrategyRepresentation,
};
use pg_grammar::model::{Grammar, MorphRuleDef};

const TEMPLATED_UNSUPPORTED_SHAPE_PREDICATE: &str = "strategy-coverage.templated-unsupported-shape";

const CROSS_TABLE_UNTRANSLATABLE_XML: &str = r#"<HermitCrabInput><Language><Name>cross-table-untranslatable</Name>
  <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="source"><Name>Source</Name><SegmentDefinitions>
    <SegmentDefinition id="sa"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="sb"><Representations><Representation>!</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions></CharacterDefinitionTable>
  <CharacterDefinitionTable id="active"><Name>Active</Name><SegmentDefinitions>
    <SegmentDefinition id="aa"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions></CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncSource"><Name>Source A</Name><Segment segment="sa"/></SegmentNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="source" morphologicalRules="mr"><Name>source</Name>
      <MorphologicalRuleDefinitions><MorphologicalRule id="mr" requiredPartsOfSpeech="p" outputPartOfSpeech="p"><Name>bad output</Name>
        <MorphologicalSubrules><MorphologicalSubrule id="sub"><MorphologicalInput><PhoneticSequence id="stem"><SimpleContext naturalClass="ncSource"/></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><InsertSegments><PhoneticShape>!</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>BAD</MorphemeId>
      </MorphologicalRule></MorphologicalRuleDefinitions>
      <LexicalEntries><LexicalEntry id="e" partOfSpeech="p"><Allomorphs><Allomorph id="ea"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries>
    </Stratum>
    <Stratum characterDefinitionTable="active"><Name>active</Name></Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

fn conformance_fixture(root: Root, category: &str, name: &str) -> FixtureRef {
    discover_scoped(ConformanceScope::All)
        .into_iter()
        .find(|fixture| {
            fixture.root == root && fixture.category == category && fixture.name == name
        })
        .unwrap_or_else(|| panic!("missing conformance fixture {root:?}:{category}/{name}"))
}

fn load_conformance_fixture(root: Root, category: &str, name: &str) -> (FixtureRef, Grammar) {
    let fixture = conformance_fixture(root, category, name);
    let label = fixture.label();
    let grammar = load(&fixture.load_grammar_xml());
    assert!(!label.is_empty());
    (fixture, grammar)
}

/// Pure-ablaut `MorphologicalRule` (`Role::Process`); replaces this file's old `RealizationalRule` fixture now that `uflexc` represents `RealizationalMorphology` too.
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
    let g = load(ABLAUT_XML);
    assert!(
        matches!(g.mrules[0], MorphRuleDef::AffixProcess(_)),
        "fixture must actually declare an AffixProcess rule"
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
    );

    let plan_composed = &outcome.considered[0];
    let tuned = &outcome.considered[1];

    assert!(
        !plan_composed.is_admissible(),
        "PlanComposed's proposer (uflexc) emits no lexc line for a Role::Process (ablaut) \
         allomorph, so it cannot propose the construct at all -- it must not be selectable. \
         Decision was {:?}",
        plan_composed.decision
    );
    assert!(
        tuned.is_admissible(),
        "TunedSurfaceProbed handles Role::Process through build_structural_composites and must \
         stay selectable -- the account is per-strategy, not a blanket refusal. Decision was {:?}",
        tuned.decision
    );
    // The refusal has to be legible, not just a bool: it names the strategy, the construct, and the account that produced it.
    let CompileDecision::Refuse(diagnostics) = &plan_composed.decision else {
        panic!("expected a Refuse carrying diagnostics");
    };
    let hit = diagnostics
        .iter()
        .find(|d| d.predicate == "strategy-coverage.construct-not-representable")
        .expect("the refusal must come from the strategy-coverage account");
    assert_eq!(hit.construct, "ProcessMorphology");
    assert!(
        hit.witness.contains("PlanComposed"),
        "the diagnostic must name the strategy that cannot represent the construct: {}",
        hit.witness
    );
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

/// The memo trap: `GrammarSemantics` memoizes `characteristics()` per grammar, so a second strategy reading the same owner could silently inherit the first one's answer. One shared owner must still give two different answers.
#[test]
fn two_strategies_get_their_own_answers_from_one_shared_semantics() {
    let g = load(ABLAUT_XML);
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
    let g = load(NO_ABLAUT_XML);
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
    );
    assert!(outcome.considered.iter().all(|c| c.is_admissible()));
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

/// The hole is per-strategy: `PlanComposed` and `TemplatedUnderlyingTokens` both cannot represent `ProcessMorphology`, while `TunedSurfaceProbed` can.
#[test]
fn the_account_is_per_strategy_not_a_blanket_refusal() {
    for strategy in [
        EmissionStrategy::PlanComposed,
        EmissionStrategy::TemplatedUnderlyingTokens,
    ] {
        assert_eq!(
            representation_of(strategy, CharacteristicKind::ProcessMorphology).representation,
            StrategyRepresentation::CannotRepresent,
            "{strategy:?}"
        );
    }
    assert_eq!(
        representation_of(
            EmissionStrategy::TunedSurfaceProbed,
            CharacteristicKind::ProcessMorphology
        )
        .representation,
        StrategyRepresentation::Represents,
        "TunedSurfaceProbed must still represent what the other two cannot -- the account is \
         per-strategy, not a blanket refusal"
    );
}

/// Refuses each unsupported allomorph shape without attributing that capability gap to Tuned.
#[test]
fn templated_selector_refuses_each_known_unsupported_shape_with_per_allomorph_diagnostics() {
    let fixtures = [
        (
            Root::Machine,
            "languages",
            "fusional-realizational-morphology",
            "vinc",
        ),
        (
            Root::Machine,
            "languages",
            "metathesis-phase-isolation",
            "sumulat",
        ),
        (
            Root::Staging,
            "edge-cases",
            "backend-ordered-generic",
            "sumulat",
        ),
        (
            Root::Staging,
            "edge-cases",
            "circumfix-cross-product-and-infix-drop",
            "bumat",
        ),
        (
            Root::Staging,
            "edge-cases",
            "circumfix-infix-interior-action-precedence",
            "kebzatan",
        ),
        (
            Root::Staging,
            "edge-cases",
            "circumfix-reduplication-precedence",
            "ketamtaman",
        ),
        (
            Root::Staging,
            "edge-cases",
            "infix-interdigitation",
            "kpfotab",
        ),
    ];

    for (root, category, name, surface) in fixtures {
        let (_, grammar) = load_conformance_fixture(root, category, name);
        let selection = select_backends_for_grammar(&grammar);
        let templated = selection
            .report_for(EmissionStrategy::TemplatedUnderlyingTokens)
            .expect("templated backend must be reported");
        let diagnostics = templated.declined_on();
        assert!(
            !diagnostics.is_empty(),
            "{root:?}:{category}/{name} ({surface}) refusal must retain per-shape diagnostics"
        );
        assert!(
            diagnostics.iter().all(|diagnostic| {
                diagnostic.predicate == TEMPLATED_UNSUPPORTED_SHAPE_PREDICATE
                    && diagnostic
                        .witness
                        .contains("no faithful templated emission path")
            }),
            "{root:?}:{category}/{name} ({surface}) diagnostics must use the stable predicate and faithful-path refusal: {diagnostics:?}"
        );
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.construct.contains("mrule")
                    && diagnostic.construct.contains("allomorph")
            }),
            "{root:?}:{category}/{name} ({surface}) must retain a precise mrule/allomorph refusal: {diagnostics:?}"
        );
        let tuned = selection
            .report_for(EmissionStrategy::TunedSurfaceProbed)
            .expect("tuned backend must be reported");
        assert!(
            !matches!(tuned.decision(), CompileDecision::Refuse(_)),
            "{root:?}:{category}/{name} ({surface}) must remain within Tuned's capability envelope: {tuned:?}"
        );
    }
}

/// Classifier recognition does not make a backend selectable until emission, relation compilation, and strict result validation agree.
#[test]
fn templated_static_floor_keeps_process_morphology_unrepresentable_until_realization() {
    assert_eq!(
        unrepresentable_kinds(EmissionStrategy::TemplatedUnderlyingTokens),
        vec![CharacteristicKind::ProcessMorphology],
        "recognizing a recipe cannot select Templated until its real emission path exists"
    );
}

#[test]
fn templated_capability_translates_from_owner_to_final_active_table() {
    let grammar = load(CROSS_TABLE_UNTRANSLATABLE_XML);
    let selection = select_backends_for_grammar(&grammar);
    let templated = selection
        .report_for(EmissionStrategy::TemplatedUnderlyingTokens)
        .expect("templated backend must be reported");
    assert!(templated.declined_on().iter().any(|diagnostic| {
        diagnostic.predicate == TEMPLATED_UNSUPPORTED_SHAPE_PREDICATE
            && diagnostic.witness.contains("untranslatable-output-table")
    }));
}

/// Refuses a morphology-relation-classifier shape and self-opaquing epenthesis shapes for Templated.
#[test]
fn templated_selector_refuses_structural_and_self_opaquing_fixture_shapes() {
    let fixtures = [
        (
            Root::Machine,
            "edge-cases",
            "strrep-identity",
            "ndpat",
            "morphology relation",
        ),
        (
            Root::Machine,
            "languages",
            "suffixing-vowel-harmony",
            "semitide",
            "self-opaquing epenthesis",
        ),
        (
            Root::Machine,
            "languages",
            "templatic-root-modification",
            "katabit",
            "self-opaquing epenthesis",
        ),
    ];

    for (root, category, name, surface, shape) in fixtures {
        let (_, grammar) = load_conformance_fixture(root, category, name);
        let selection = select_backends_for_grammar(&grammar);
        let templated = selection
            .report_for(EmissionStrategy::TemplatedUnderlyingTokens)
            .expect("templated backend must be reported");
        let diagnostics = templated.declined_on();
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.predicate == TEMPLATED_UNSUPPORTED_SHAPE_PREDICATE
                    && diagnostic.witness.contains("no faithful templated emission path")
                    && diagnostic
                        .witness
                        .to_ascii_lowercase()
                        .contains(shape)
            }),
            "{root:?}:{category}/{name} ({surface}) must identify {shape} and the faithful-path refusal: {diagnostics:?}"
        );
    }
}

/// A selector refusal is containment `NotAttempted`, never a misleading `Failed` comparison.
#[test]
fn refused_templated_fixture_is_not_attempted_by_containment() {
    let (fixture, grammar) = load_conformance_fixture(
        Root::Staging,
        "edge-cases",
        "circumfix-cross-product-and-infix-drop",
    );
    let observation =
        observe_fixture_containment(&fixture.label(), &grammar, &["bumat".to_string()]);
    assert_eq!(
        observation.outcome_for(EmissionStrategy::TemplatedUnderlyingTokens),
        Some(&ContainmentOutcome::NotAttempted {
            reason: NotAttemptedReason::RefusedBySelector,
        })
    );
}
