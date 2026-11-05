use super::*;
use crate::construction::{ConstructionStrategy, FstCompileConfig, StrategyRefusal, StrategySelection};

fn grammar() -> Grammar {
    pg_grammar::load(r#"<HermitCrabInput><Language><Name>MinimalCompletedBuildFixture</Name>
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
    </Language></HermitCrabInput>"#).unwrap()
}

#[test]
fn explicit_factored_is_a_typed_refusal_and_never_a_surface_fallback() {
    let (result, profile) = compile_proposer_with_config(&grammar(), &FstCompileConfig::for_strategy(ConstructionStrategy::Factored));
    assert!(matches!(result, Err(FomaError::StrategyNotImplemented(ConstructionStrategy::Factored))));
    assert_eq!(profile.construction.selected, None);
    assert_eq!(profile.construction.attempts.len(), 1);
    assert_eq!(profile.construction.attempts[0].refusal, Some(StrategyRefusal::NotImplemented));
    assert_eq!(profile.final_state_count, None);
}

#[test]
fn auto_respects_order_records_refusals_and_stops_at_first_complete() {
    let mut config = FstCompileConfig { strategy: StrategySelection::Auto, auto_order: vec![ConstructionStrategy::Factored, ConstructionStrategy::SurfaceEnum], ..Default::default() };
    let (result, profile) = compile_proposer_with_config(&grammar(), &config);
    assert!(result.is_ok());
    assert_eq!(profile.construction.attempts.len(), 2);
    assert_eq!(profile.construction.attempts[0].refusal, Some(StrategyRefusal::NotImplemented));
    assert_eq!(profile.construction.selected, Some(ConstructionStrategy::SurfaceEnum));
    config.auto_order.reverse();
    let (result, profile) = compile_proposer_with_config(&grammar(), &config);
    assert!(result.is_ok());
    assert_eq!(profile.construction.attempts.len(), 1);
}

#[test]
fn auto_exhaustion_and_bad_order_are_explicit_and_keep_no_artifact() {
    let mut config = FstCompileConfig { strategy: StrategySelection::Auto, auto_order: vec![ConstructionStrategy::Factored], ..Default::default() };
    let (result, profile) = compile_proposer_with_config(&grammar(), &config);
    assert!(result.is_err());
    assert_eq!(profile.construction.attempts.len(), 1);
    config.auto_order.clear();
    let (result, profile) = compile_proposer_with_config(&grammar(), &config);
    assert!(result.is_err());
    assert!(profile.construction.configuration_error.is_some());
    config.auto_order = vec![ConstructionStrategy::SurfaceEnum; 2];
    assert!(config.strategies().is_err());
}

#[test]
fn configured_depth_refuses_instead_of_truncating() {
    let mut config = FstCompileConfig::default();
    config.bounds.derivation_depth = Some(1);
    let (result, profile) = compile_proposer_with_config(&grammar(), &config);
    assert!(matches!(result, Err(FomaError::Unsupported(_))));
    assert_eq!(profile.construction.attempts[0].refusal, Some(StrategyRefusal::LimitExceeded));
    let bound = profile.construction.attempts[0].bound_refusal.as_ref().unwrap();
    assert_eq!((bound.value, bound.limit), (2, 1));
    config.bounds.derivation_depth = Some(2);
    assert!(compile_proposer_with_config(&grammar(), &config).0.is_ok());
}

#[test]
fn shared_cli_parser_rejects_unknown_strategy_and_malformed_limits() {
    for args in [["--fst-strategy", "unknown"], ["--fst-closure-work", "many"], ["--fst-auto-order", "factored,unknown"]] {
        assert!(FstCompileConfig::default().parse_option(&args.map(String::from), &mut 0).is_err());
    }
}

#[test]
fn zero_closure_work_has_an_effect_and_never_yields_a_partial_proposer() {
    let fixture = pg_conformance_fixtures::require_fixture("edge-cases", "circumfix-cross-product-and-infix-drop");
    let grammar = fixture.load_grammar();
    let mut config = FstCompileConfig::default();
    config.bounds.closure_work = Some(0);
    let (result, profile) = compile_proposer_with_config(&grammar, &config);
    assert!(result.is_err());
    let closure = profile.construction.attempts[0].closure.as_ref().expect("closure must actually run");
    assert_eq!(closure.evidence.rule_pairs_visited, 0);
    assert!(closure.evidence.pending_successor_count > 0);
    assert_eq!(profile.construction.attempts[0].refusal, Some(StrategyRefusal::LimitExceeded));
}

#[test]
fn configured_compound_depth_is_enforced_by_the_emitter() {
    let fixture = pg_conformance_fixtures::require_fixture("edge-cases", "recursive-endocentric-compounding");
    let grammar = fixture.load_grammar();
    let mut config = FstCompileConfig::default();
    assert!(compile_proposer_with_config(&grammar, &config).0.is_ok());
    config.bounds.compound_depth = 0;
    let (result, profile) = compile_proposer_with_config(&grammar, &config);
    assert!(result.is_err());
    let attempt = &profile.construction.attempts[0];
    assert_eq!(attempt.refusal, Some(StrategyRefusal::LimitExceeded));
    let bound = attempt.bound_refusal.as_ref().expect("the emitter must name the compound bound");
    assert_eq!(bound.limit, 0);
    assert!(bound.value > bound.limit);
}

#[test]
fn shared_cli_parser_sets_strategy_order_and_every_construction_bound() {
    let args = ["--fst-strategy=auto", "--fst-auto-order", "factored,surface-enum", "--fst-closure-work=17", "--fst-structural-work=23", "--fst-structural-bytes", "29", "--fst-closure-depth", "3", "--fst-compound-depth=4", "--fst-derivation-depth", "5"].map(String::from);
    let mut config = FstCompileConfig::default();
    let mut index = 0;
    while index < args.len() {
        assert!(config.parse_option(&args, &mut index).unwrap());
    }
    assert_eq!((config.bounds.structural_work, config.bounds.structural_bytes), (Some(23), Some(29)));
    assert_eq!(config.strategy, StrategySelection::Auto);
    assert_eq!(config.strategies().unwrap(), vec![ConstructionStrategy::Factored, ConstructionStrategy::SurfaceEnum]);
    assert_eq!((config.bounds.closure_work, config.bounds.closure_depth, config.bounds.compound_depth, config.bounds.derivation_depth), (Some(17), 3, 4, Some(5)));
}

#[test]
fn structural_config_refusals_are_typed_and_record_effective_bounds() {
    let fixture = pg_conformance_fixtures::require_fixture("edge-cases", "circumfix-cross-product-and-infix-drop");
    let grammar = fixture.load_grammar();
    for (work, bytes, measure) in [(0, 16 * 1024 * 1024, "HC_STRUCTURAL_CLOSURE_WORK_LIMIT"), (100_000, 0, "HC_STRUCTURAL_CLOSURE_BYTES_LIMIT")] {
        let mut config = FstCompileConfig::default();
        config.bounds.structural_work = Some(work);
        config.bounds.structural_bytes = Some(bytes);
        let (result, profile) = compile_proposer_with_config(&grammar, &config);
        assert!(matches!(result, Err(FomaError::Unsupported(_))));
        assert_eq!(profile.final_state_count, None);
        assert_eq!(profile.construction.requested_config, Some(config));
        let effective = &profile.construction.config.as_ref().unwrap().bounds;
        assert_eq!((effective.structural_work, effective.structural_bytes), (Some(work), Some(bytes)));
        let attempt = &profile.construction.attempts[0];
        assert_eq!(attempt.refusal, Some(StrategyRefusal::LimitExceeded));
        let refusal = attempt.bound_refusal.as_ref().unwrap();
        assert_eq!(refusal.measure, measure);
        assert_eq!(refusal.limit, 0);
        assert!(refusal.value > 0);
    }
}
