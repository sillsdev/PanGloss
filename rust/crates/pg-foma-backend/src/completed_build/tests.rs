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

/// Pins that no production artifact can come from this strategy; fails the day an arm is added.
#[test]
fn plan_composed_has_no_production_compile_arm() {
    let grammar = pg_grammar::load(MINIMAL_XML).expect("fixture must load");
    let attempt = CompileAttempt::try_new().expect("attempt id must construct");
    let result = compile_completed_backend(&grammar, EmissionStrategy::PlanComposed, &attempt);
    assert!(
        matches!(
            result,
            Err(CompletedBuildError::UnsupportedStrategy(
                EmissionStrategy::PlanComposed
            ))
        ),
        "expected UnsupportedStrategy(PlanComposed); got {result:?}"
    );
}

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

/// A `Role::Process` allomorph is `CannotRepresent` for Templated by architecture (`strrep-identity`'s plain-Prefix no longer fits this pin -- it compiles now).
#[test]
fn templated_route_refuses_before_emitting_when_the_envelope_refuses() {
    let fixture = pg_conformance_fixtures::discover()
        .into_iter()
        .find(|f| f.label() == "machine:edge-cases/process-morphology-in-place-mutation")
        .expect("fixture must be discoverable");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");
    let attempt = CompileAttempt::try_new().expect("attempt id must construct");
    let result = compile_completed_backend(
        &grammar,
        EmissionStrategy::TemplatedUnderlyingTokens,
        &attempt,
    );
    assert!(
        matches!(result, Err(CompletedBuildError::CapabilityRefused(_))),
        "expected CapabilityRefused; got {result:?}"
    );
}
