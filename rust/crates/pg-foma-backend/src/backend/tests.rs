use super::*;

/// `backend_for` must realize every `ALL_STRATEGIES` member as itself, and `ALL_BACKENDS` must cover each exactly once -- the closed-table half of the adapter/strategy correspondence.
#[test]
fn backend_for_and_all_backends_cover_every_strategy_exactly_once() {
    for &strategy in crate::strategy_coverage::ALL_STRATEGIES {
        assert_eq!(backend_for(strategy).strategy(), strategy);
    }
    let mut covered: Vec<EmissionStrategy> = ALL_BACKENDS
        .iter()
        .map(|backend| backend.strategy())
        .collect();
    covered.sort_by_key(|strategy| strategy.label());
    let mut expected: Vec<EmissionStrategy> = crate::strategy_coverage::ALL_STRATEGIES.to_vec();
    expected.sort_by_key(|strategy| strategy.label());
    assert_eq!(
        covered, expected,
        "ALL_BACKENDS must cover every strategy exactly once"
    );
}

/// Exactly one backend interprets a plan.
#[test]
fn exactly_one_backend_interprets_a_plan() {
    assert_eq!(
        ALL_BACKENDS
            .iter()
            .filter(|backend| backend.interprets_plan())
            .count(),
        1
    );
}

/// `PlanComposed` has no production compile arm, pinned here at the trait seam too (mirrors `completed_build::tests::plan_composed_has_no_production_compile_arm`).
#[test]
fn plan_composed_backend_refuses_measurement_unconditionally() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>BackendTraitFixture</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <Strata>
        <Stratum characterDefinitionTable="t1">
          <Name>S</Name>
          <LexicalEntries>
            <LexicalEntry id="e1"><Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>
          </LexicalEntries>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;
    let grammar = pg_grammar::load(XML).expect("fixture must load");
    let selection = crate::backend_selection::select_backends_for_grammar(&grammar);
    let request = CompileAttempt::try_new().expect("attempt id must construct");
    let result = backend_for(EmissionStrategy::PlanComposed)
        .compile_for_measurement(&grammar, &selection, &request);
    assert!(matches!(
        result,
        Err(CompletedBuildError::UnsupportedStrategy(
            EmissionStrategy::PlanComposed
        ))
    ));
}
