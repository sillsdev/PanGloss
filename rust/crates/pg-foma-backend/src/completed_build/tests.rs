use super::*;
use crate::characterization::{CharacterizationResult, ClosureEvidence};

#[test]
fn incomplete_closure_error_preserves_frontier_evidence() {
    let closure = CharacterizationResult {
        terminal: ClosureTerminal::Incomplete(
            crate::characterization::ClosureStopReason::WorkBudgetReached,
        ),
        evidence: ClosureEvidence {
            rule_pairs_visited: 10_000,
            synthesized_successors: 10_001,
            maximum_depth: 42,
            per_depth_counts: vec![1, 9_999],
            pending_successor_count: 3,
            pending_rule_ordinals: vec![7, 19],
            worklist_empty: false,
        },
    };

    let error = validate_closure(&closure).expect_err("closure must be rejected");
    assert_eq!(
        error,
        CompletedBuildError::IncompleteClosure {
            terminal: closure.terminal,
            rule_pairs_visited: 10_000,
            pending_successor_count: 3,
            pending_rule_ordinals: vec![7, 19],
            worklist_empty: false,
        }
    );
    let rendered = error.to_string();
    assert!(rendered.contains("WorkBudgetReached"));
    assert!(rendered.contains("rule_pairs_visited=10000"));
    assert!(rendered.contains("pending_successor_count=3"));
    assert!(rendered.contains("pending_rule_ordinals=[7, 19]"));
    assert!(rendered.contains("worklist_empty=false"));
}

/// Minimal, delanguaged grammar: one char table, one entry, no rules.
const MINIMAL_XML: &str = r#"<HermitCrabInput><Language><Name>MinimalCompletedBuildFixture</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
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

/// The measurement half must COMPLETE on the very grammar admission refuses, or the refusal reads as a backend that cannot build it.
#[test]
fn a_partial_grammar_compiles_under_containment_and_is_then_refused_publication() {
    let mut grammar = pg_grammar::load(MINIMAL_XML).expect("fixture must load");
    grammar.entries[0].partial_reason = Some(
        pg_grammar::model::PartialMorphemeReason::StemWithoutCategory,
    );
    let strategy = EmissionStrategy::TunedSurfaceProbed;
    let attempt = CompileAttempt::try_new().expect("attempt id must construct");
    let selection = crate::backend_selection::select_backends_for_grammar(&grammar);

    let measured =
        compile_completed_backend_for_measurement(&grammar, &selection, strategy, &attempt)
            .expect("the contained measurement attempt must complete for a partial grammar");
    assert!(
        !measured.payload_bytes().is_empty(),
        "the measurement attempt must really produce a payload"
    );

    let error = compile_completed_backend(&grammar, strategy, &attempt)
        .expect_err("a partial grammar must not yield a production artifact");
    match &error {
        CompletedBuildError::NotProductionReady {
            strategy: refused,
            health,
        } => {
            assert_eq!(*refused, strategy);
            assert_eq!(
                health.admission(),
                crate::health::Severity::NotProductionReady
            );
            assert_eq!(
                health.admission_by_class().representability,
                crate::health::Severity::WithinLimits,
                "a readiness refusal must not claim the grammar is unrepresentable"
            );
        }
        other => panic!("expected NotProductionReady; got {other:?}"),
    }
    let rendered = error.to_string();
    assert!(
        !rendered.contains(measured.evidence().payload_fingerprint()),
        "the refusal must not leak a payload fingerprint: {rendered}"
    );
}

/// The same grammar without the partial marker keeps its ordinary production path.
#[test]
fn the_non_partial_control_still_returns_a_completed_build() {
    let grammar = pg_grammar::load(MINIMAL_XML).expect("fixture must load");
    let attempt = CompileAttempt::try_new().expect("attempt id must construct");
    let build = compile_completed_backend(&grammar, EmissionStrategy::TunedSurfaceProbed, &attempt)
        .expect("a non-partial grammar must still produce a completed build");
    assert!(!build.payload_bytes().is_empty());
}

#[test]
fn tuned_measurement_refuses_alpha_environment_even_with_an_unrelated_selection() {
    let plain = pg_grammar::load(MINIMAL_XML).expect("plain fixture loads");
    let selection = crate::backend_selection::select_backends_for_grammar(&plain);
    let xml = include_str!("../../../pg-parse/tests/data/alpha-optional-environment.xml");
    let grammar = pg_grammar::load(xml).expect("alpha fixture loads");
    let attempt = CompileAttempt::try_new().expect("attempt id constructs");
    let result = crate::backend::backend_for(EmissionStrategy::TunedSurfaceProbed)
        .compile_for_measurement(&grammar, &selection, &attempt);
    assert!(matches!(result, Err(CompletedBuildError::ConstructionRefused(ref report)) if report.selected.is_none() && report.attempts[0].refusal == Some(pg_foma::construction::StrategyRefusal::Incomplete)), "the real tuned compiler must refuse alpha environments");
}

#[test]
fn construction_configuration_and_payload_match_the_analyzer_route() {
    use pg_foma::construction::{ConstructionStrategy, FstCompileConfig, StrategyRefusal, StrategySelection};
    let grammar = pg_grammar::load(MINIMAL_XML).unwrap();
    let config = FstCompileConfig {
        strategy: StrategySelection::Auto,
        auto_order: vec![ConstructionStrategy::Factored, ConstructionStrategy::SurfaceEnum],
        ..Default::default()
    };
    let attempt = CompileAttempt::try_new().unwrap().with_config(config.clone());
    let (proposer, profile) = crate::analyzer::compile_proposer_with_config(&grammar, &config);
    let build = compile_tuned_surface_for_measurement(&grammar, &attempt).unwrap();
    let payload = proposer.unwrap().foma_binary_payload().unwrap();
    let network = read_foma_binary_payload(&payload).unwrap();
    assert_eq!(finished_net_digest(&network), build.evidence().model_fingerprint());
    assert_eq!(build.evidence().construction().selected, profile.construction.selected);
    assert_eq!(build.evidence().construction().attempts.len(), 2);
    assert_eq!(build.to_wire().construction.attempts[0].refusal, Some(StrategyRefusal::NotImplemented));
}

#[test]
fn construction_refusal_matches_on_both_compile_routes() {
    use pg_foma::construction::{ConstructionStrategy, FstCompileConfig};
    let grammar = pg_grammar::load(MINIMAL_XML).unwrap();
    for config in [
        FstCompileConfig::for_strategy(ConstructionStrategy::Factored),
        FstCompileConfig { bounds: pg_foma::construction::ConstructionBounds { derivation_depth: Some(0), ..Default::default() }, ..Default::default() },
    ] {
        let (_, profile) = crate::analyzer::compile_proposer_with_config(&grammar, &config);
        let attempt = CompileAttempt::try_new().unwrap().with_config(config);
        let Err(CompletedBuildError::ConstructionRefused(report)) = compile_tuned_surface_for_measurement(&grammar, &attempt) else { panic!("expected typed construction refusal"); };
        assert_eq!(report.attempts[0].strategy, profile.construction.attempts[0].strategy);
        assert_eq!(report.attempts[0].refusal, profile.construction.attempts[0].refusal);
        assert_eq!(report.selected, None);
    }
}

#[test]
fn completed_selection_cannot_relabel_the_construction() {
    let grammar = pg_grammar::load(MINIMAL_XML).unwrap();
    let request = CompileAttempt::try_new().unwrap();
    let build = compile_tuned_surface_for_measurement(&grammar, &request).unwrap();
    let changed = request.with_config(pg_foma::construction::FstCompileConfig::for_strategy(pg_foma::construction::ConstructionStrategy::Factored));
    assert!(validate_selected_build(&build, &changed, &grammar_identity(&grammar)).is_err());
}

#[test]
fn a_covered_composite_uses_the_same_construction_on_both_routes() {
    let fixture = pg_conformance_fixtures::require_fixture("edge-cases", "circumfix-non-first-allomorph-selection");
    let grammar = fixture.load_grammar();
    let config = pg_foma::construction::FstCompileConfig::default();
    let request = CompileAttempt::try_new().unwrap().with_config(config.clone());
    let (result, profile) = crate::analyzer::compile_proposer_with_config(&grammar, &config);
    assert!(result.is_ok(), "the named exact surface-enum control must compile");
    let completed = compile_tuned_surface_for_measurement(&grammar, &request).unwrap();
    assert_eq!(profile.final_state_count, Some(i64::from(completed.evidence().state_count())));
    assert_eq!(profile.final_arc_count, Some(i64::from(completed.evidence().arc_count())));
}

#[test]
fn structural_bounds_and_refusals_match_on_both_compile_routes() {
    use pg_foma::construction::{FstCompileConfig, StrategyRefusal};
    let fixture = pg_conformance_fixtures::require_fixture("edge-cases", "circumfix-cross-product-and-infix-drop");
    let grammar = fixture.load_grammar();
    for (work, bytes) in [(0, 16 * 1024 * 1024), (100_000, 0)] {
        let mut config = FstCompileConfig::default();
        config.bounds.structural_work = Some(work);
        config.bounds.structural_bytes = Some(bytes);
        let (result, profile) = crate::analyzer::compile_proposer_with_config(&grammar, &config);
        assert!(result.is_err());
        let request = CompileAttempt::try_new().unwrap().with_config(config);
        let Err(CompletedBuildError::ConstructionRefused(report)) = compile_tuned_surface_for_measurement(&grammar, &request) else { panic!("expected typed structural refusal"); };
        assert_eq!(report.config, profile.construction.config);
        assert_eq!(report.attempts[0].refusal, Some(StrategyRefusal::LimitExceeded));
        assert_eq!(report.attempts[0].bound_refusal, profile.construction.attempts[0].bound_refusal);
        assert_eq!(report.selected, None);
    }
}
