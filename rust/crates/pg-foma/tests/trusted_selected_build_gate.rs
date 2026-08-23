//! RED gate for the trusted selected-build seam.
//!
//! This is deliberately written against the opaque selected-build API that Phase B must add:
//! selection computes the route from reports and completed artifacts, and runtime receives the
//! exact finalized payload.  The test must never provide `preferred`/`selected` values to a
//! validator and must never rebuild after selection.

use std::collections::BTreeSet;

use pg_foma::backend_runtime::grammar_identity;
use pg_foma::backend_selection::select_backends_for_grammar;
use pg_foma::completed_build::{
    compile_completed_backend, select_completed_build, CompletionProofKind,
};
use pg_foma::enumerate::EmissionStrategy;
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

fn assert_selected_payload_route(
    xml: &str,
    expected_strategy: EmissionStrategy,
    expected_proof: CompletionProofKind,
    word: &str,
) {
    let grammar = pg_grammar::load(xml).expect("synthetic fixture must load");
    let selection = select_backends_for_grammar(&grammar);

    // The route is discovered from the real reports.  The test may state the fixture's expected
    // route, but it cannot inject that route into selection or trusted-build validation.
    assert!(
        selection.selected().contains(&expected_strategy),
        "the real capability reports must admit the route exercised by this case"
    );

    let request = CompileEnvelopeRequest::try_new(ResourceEnvelopeId::ManagedV1)
        .expect("managed envelope request");
    let grammar_id = grammar_identity(&grammar);
    let build = compile_completed_backend(&grammar, expected_strategy, &request)
        .expect("the selected route must return a finalized completed build");

    // This is the trust boundary: no preferred/selected values are supplied by the caller.
    let selected = select_completed_build(&selection, vec![build], &request, &grammar_id)
        .expect("a matching completed build must be selectable");
    assert_eq!(selected.strategy(), expected_strategy);
    assert_eq!(selected.evidence().requested_strategy(), expected_strategy);
    assert_eq!(selected.evidence().realized_strategy(), expected_strategy);
    assert_eq!(selected.evidence().grammar_identity(), grammar_id);
    assert_eq!(selected.evidence().envelope_id(), request.envelope_id());
    assert_eq!(selected.evidence().completion_proof_kind(), expected_proof);
    assert!(selected.evidence().is_trusted_complete());
    assert!(!selected.payload_bytes().is_empty());

    // Runtime receives the exact selected payload.  This API is intentionally not a compiler API:
    // it must deserialize the finalized bytes and then run propose -> peel -> confirm.
    let mut analyzer = selected
        .into_analyzer(&grammar)
        .expect("exact selected payload must reconstruct the analyzer");
    let oracle = Morpher::new(&grammar, usize::MAX).parse_word_opts(word, &ParseOptions::default());
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
        identities(&grammar, &outcome.structured),
        identities(&grammar, &oracle.structured),
        "selected {:?} payload changed the canonical assessment identity set for {word:?}",
        expected_strategy
    );
}

#[test]
fn selected_tuned_surface_payload_reconstructs_exact_analysis_pipeline() {
    assert_selected_payload_route(
        TUNED_FIXTURE,
        EmissionStrategy::TunedSurfaceProbed,
        CompletionProofKind::TunedClosure,
        "a",
    );
}

#[test]
fn selected_templated_underlying_tokens_payload_reconstructs_exact_analysis_pipeline() {
    assert_selected_payload_route(
        TEMPLATED_FIXTURE,
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
