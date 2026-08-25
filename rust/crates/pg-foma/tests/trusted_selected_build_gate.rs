//! Verifies that selection derives a trusted completed build and runtime consumes its exact finalized payload.

use std::collections::BTreeSet;

use pg_foma::backend_runtime::grammar_identity;
use pg_foma::backend_selection::{
    select_backends_for_grammar, BackendReport, BackendSelection,
};
use pg_foma::capability::CompileDecision;
use pg_foma::completed_build::{
    compile_completed_backend, select_completed_build, CompletionProofKind,
};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::health::{
    FindingCode, HealthFinding, Metric, MetricValue, Phase, Severity, ValueProvenance,
};
use pg_foma::resource_envelope::{CompileEnvelopeRequest, ResourceEnvelopeId};
use pg_grammar::model::Grammar;
use pg_parse::identity::AnalysisIdentity;
use pg_parse::{Morpher, ParseOptions, WordAnalysis};

const TUNED_FIXTURE: &str = r#"
<HermitCrabInput><Language><Name>SelectedTunedFixture</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
  <Strata><Stratum characterDefinitionTable="t1">
    <Name>S</Name>
    <MorphologicalRuleDefinitions>
      <RealizationalRule id="rr1"><Name>Realiz</Name>
        <MorphologicalSubrules><MorphologicalSubrule id="sub1">
          <MorphologicalInput><PhoneticSequence id="s0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="s0" /></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </RealizationalRule>
    </MorphologicalRuleDefinitions>
    <LexicalEntries><LexicalEntry id="e1"><Allomorphs>
      <Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph>
    </Allomorphs></LexicalEntry></LexicalEntries>
  </Stratum></Strata>
</Language></HermitCrabInput>
"#;

const TEMPLATED_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../conformance-staging/edge-cases/template-category-sharing/grammar.xml"
));

fn identities(grammar: &Grammar, analyses: &[WordAnalysis]) -> BTreeSet<AnalysisIdentity> {
    analyses
        .iter()
        .map(|analysis| AnalysisIdentity::project(analysis, grammar).expect("stable identity"))
        .collect()
}

fn synthetic_warning() -> HealthFinding {
    HealthFinding {
        code: FindingCode::PayloadSizeBand,
        severity: Severity::LargeMultiplier,
        phase: Phase::Compile,
        affected: vec!["selected-payload-test".to_string()],
        metric: Metric::EmittedLineCount,
        value: MetricValue::Count(1),
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation: "synthetic ranking-only warning".to_string(),
        remedies: Vec::new(),
        override_record: None,
    }
}

fn templated_preferred_selection() -> BackendSelection {
    BackendSelection::from_reports(vec![
        BackendReport::accepted(
            EmissionStrategy::TunedSurfaceProbed,
            CompileDecision::Admit,
            vec![synthetic_warning()],
        )
        .expect("synthetic tuned report must be accepted"),
        BackendReport::accepted(
            EmissionStrategy::TemplatedUnderlyingTokens,
            CompileDecision::Admit,
            Vec::new(),
        )
        .expect("synthetic templated report must be accepted"),
    ])
}

/// Each case supplies its own `selection`, so the payload seam stays the subject either way.
fn assert_selected_payload_route(
    grammar: &Grammar,
    selection: &BackendSelection,
    expected_strategy: EmissionStrategy,
    expected_proof: CompletionProofKind,
    word: &str,
) {
    assert_eq!(
        selection.preferred(),
        Some(expected_strategy),
        "this case's ranked reports must put {expected_strategy:?} first"
    );

    let request = CompileEnvelopeRequest::try_new(ResourceEnvelopeId::ManagedV1)
        .expect("managed envelope request");
    let grammar_id = grammar_identity(grammar);
    let build = compile_completed_backend(grammar, expected_strategy, &request)
        .expect("the selected route must return a finalized completed build");

    // The trust boundary: the route comes from the ranked reports, never from the build list.
    let selected = select_completed_build(selection, vec![build], &request, &grammar_id)
        .expect("a matching completed build must be selectable");
    assert_eq!(selected.strategy(), expected_strategy);
    assert_eq!(selected.evidence().requested_strategy(), expected_strategy);
    assert_eq!(selected.evidence().realized_strategy(), expected_strategy);
    assert_eq!(selected.evidence().grammar_identity(), grammar_id);
    assert_eq!(selected.evidence().envelope_id(), request.envelope_id());
    assert_eq!(selected.evidence().completion_proof_kind(), expected_proof);
    assert!(selected.evidence().is_trusted_complete());
    assert!(!selected.payload_bytes().is_empty());

    // Runtime deserializes the exact selected bytes before running propose -> peel -> confirm.
    let mut analyzer = selected
        .into_analyzer(grammar)
        .expect("exact selected payload must reconstruct the analyzer");
    let oracle = Morpher::new(grammar, usize::MAX).parse_word_opts(word, &ParseOptions::default());
    let outcome = analyzer.analyze_word(word);
    assert!(
        !oracle.structured.is_empty(),
        "fixture must have at least one canonical oracle analysis for {word:?}"
    );
    assert!(
        !outcome.structured.is_empty(),
        "selected {expected_strategy:?} payload must retain a queryable analysis for {word:?}"
    );
    assert_eq!(
        identities(grammar, &outcome.structured),
        identities(grammar, &oracle.structured),
        "selected {:?} payload changed the canonical assessment identity set for {word:?}",
        expected_strategy
    );
}

#[test]
fn selected_tuned_surface_payload_reconstructs_exact_analysis_pipeline() {
    let grammar = pg_grammar::load(TUNED_FIXTURE).expect("synthetic fixture must load");
    assert_selected_payload_route(
        &grammar,
        &select_backends_for_grammar(&grammar),
        EmissionStrategy::TunedSurfaceProbed,
        CompletionProofKind::TunedClosure,
        "a",
    );
}

#[test]
fn selected_templated_underlying_tokens_payload_reconstructs_exact_analysis_pipeline() {
    let grammar = pg_grammar::load(TEMPLATED_FIXTURE).expect("synthetic fixture must load");
    // Constructed: production ranking prefers the tuned backend, so cover this route directly.
    assert_selected_payload_route(
        &grammar,
        &templated_preferred_selection(),
        EmissionStrategy::TemplatedUnderlyingTokens,
        CompletionProofKind::TemplatedFullEmission,
        "pakolosa",
    );
}

#[test]
fn stale_completed_build_evidence_is_rejected_before_runtime() {
    let grammar = pg_grammar::load(TUNED_FIXTURE).expect("synthetic fixture must load");
    let selection = select_backends_for_grammar(&grammar);
    let request = CompileEnvelopeRequest::try_new(ResourceEnvelopeId::ManagedV1)
        .expect("managed envelope request");
    let build = compile_completed_backend(
        &grammar,
        EmissionStrategy::TunedSurfaceProbed,
        &request,
    )
    .expect("fixture must produce a completed build");
    let grammar_id = grammar_identity(&grammar);

    let other_request = CompileEnvelopeRequest::try_new(ResourceEnvelopeId::TunedSurfaceWork10kV1)
        .expect("retry envelope request");
    assert!(
        select_completed_build(&selection, vec![build], &other_request, &grammar_id)
            .is_err(),
        "a build from another envelope must not become a selected artifact"
    );
    let build = compile_completed_backend(
        &grammar,
        EmissionStrategy::TunedSurfaceProbed,
        &request,
    )
    .expect("fixture must produce a second completed build");
    assert!(
        select_completed_build(
            &selection,
            vec![build],
            &request,
            "stale-grammar-identity",
        )
        .is_err(),
        "a build for another grammar identity must fail closed before analyzer construction"
    );
}

#[test]
fn missing_preferred_completed_build_must_not_silently_select_a_lower_ranked_route() {
    let grammar = pg_grammar::load(TUNED_FIXTURE).expect("synthetic fixture must load");
    let selection = select_backends_for_grammar(&grammar);
    let preferred = selection
        .preferred()
        .expect("fixture must have a preferred backend");
    assert_eq!(
        preferred,
        EmissionStrategy::TunedSurfaceProbed,
        "fixture's preferred route must be the shipping tuned backend"
    );
    let lower_ranked = selection
        .selected()
        .into_iter()
        .find(|strategy| *strategy != preferred && strategy.is_whole_grammar())
        .expect("fixture must expose a lower-ranked whole-grammar route");
    let request = CompileEnvelopeRequest::try_new(ResourceEnvelopeId::ManagedV1)
        .expect("managed envelope request");
    let lower_build = compile_completed_backend(&grammar, lower_ranked, &request)
        .expect("lower-ranked route must be independently buildable");
    let grammar_id = grammar_identity(&grammar);

    let error = select_completed_build(
        &selection,
        vec![lower_build],
        &request,
        &grammar_id,
    )
    .expect_err(
        "selection must fail closed when the preferred completed build is absent; silently using \
         a lower-ranked payload would misreport preferred == selected == realized",
    );
    assert!(
        error.to_string().contains("preferred"),
        "the typed failure must explain that the preferred build is missing, got: {error}"
    );
}
