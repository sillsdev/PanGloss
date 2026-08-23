//! Production RED gate for the exact selected worker payload.
//!
//! The frozen Indonesian denominator is the first shipping vertical: the named tuned retry must
//! compile in the contained worker, return one trusted completed payload, and reconstruct the
//! analyzer from those exact bytes. No parent rebuild, lower-ranked fallback, placeholder, or
//! partial comparison can satisfy this gate.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pg_foma::backend_runtime::grammar_identity;
use pg_foma::backend_selection::select_backends_for_grammar_with_tuned_closure_work_limit;
use pg_foma::completed_build::CompletionProofKind;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::resource_envelope::{CompileEnvelopeRequest, ResourceEnvelope, ResourceEnvelopeId};
use pg_foma::worker::{run_selected_compile_worker, GrammarFormat};
use pg_grammar::model::Grammar;
use pg_parse::identity::AnalysisIdentity;
use pg_parse::{Morpher, ParseOptions, WordAnalysis};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn child_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_worker_test_child"))
}

fn lock_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/three-language-case-sets.json")
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn identities(grammar: &Grammar, analyses: &[WordAnalysis]) -> BTreeSet<AnalysisIdentity> {
    analyses
        .iter()
        .map(|analysis| AnalysisIdentity::project(analysis, grammar).expect("stable identity"))
        .collect()
}

fn required_corpus_path(name: &str) -> PathBuf {
    pg_conformance_fixtures::corpus::require(name)
}

fn read_required(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
#[ignore = "needs local private Indonesian grammar/corpus; run through pg.ps1 corpus-test"]
fn indonesian_120_uses_exact_preferred_worker_payload_with_complete_analysis_sets() {
    let lock_bytes = read_required(&lock_path());
    let lock: Value = serde_json::from_slice(&lock_bytes).expect("parse privacy-safe case lock");
    let language = &lock["languages"][0];
    assert_eq!(language["language"], "indonesian");
    assert_eq!(language["caseSetId"], "indonesian-valid-120-v1");
    assert_eq!(language["declaredCount"], 120);
    assert_eq!(language["selectedBackend"], "tuned-surface-probed");

    let grammar_path = required_corpus_path(
        language["grammarSource"]
            .as_str()
            .expect("grammarSource string"),
    );
    let source_path = required_corpus_path(language["source"].as_str().expect("source string"));
    let grammar_bytes = read_required(&grammar_path);
    let source_bytes = read_required(&source_path);
    assert_eq!(
        sha256(&grammar_bytes),
        language["grammarSha256"].as_str().expect("grammar hash"),
        "the private grammar must be the exact source pinned by the frozen denominator"
    );
    assert_eq!(
        sha256(&source_bytes),
        language["sourceSha256"].as_str().expect("source hash"),
        "the private word source must be the exact file pinned by the frozen denominator"
    );

    let grammar_text = std::str::from_utf8(&grammar_bytes).expect("grammar is UTF-8 XML");
    let grammar = pg_grammar::load(grammar_text).expect("load pinned Indonesian grammar");
    let envelope = ResourceEnvelope::for_id(ResourceEnvelopeId::TunedSurfaceWork10kV1);
    let selection = select_backends_for_grammar_with_tuned_closure_work_limit(
        &grammar,
        envelope.backend().tuned_surface_closure_work_cap,
    );
    assert_eq!(
        selection.preferred(),
        Some(EmissionStrategy::TunedSurfaceProbed),
        "the named 10k retry must make the locked shipping backend preferred before construction"
    );
    let request = CompileEnvelopeRequest::try_new(ResourceEnvelopeId::TunedSurfaceWork10kV1)
        .expect("named tuned retry request");

    let selected = run_selected_compile_worker(
        &child_exe(),
        &[],
        grammar_path.to_string_lossy().as_ref(),
        GrammarFormat::Xml,
        &grammar,
        &selection,
        &request,
    )
    .expect("the contained worker must return the exact selected completed payload");
    assert_eq!(selected.strategy(), EmissionStrategy::TunedSurfaceProbed);
    assert_eq!(
        selected.evidence().requested_strategy(),
        EmissionStrategy::TunedSurfaceProbed
    );
    assert_eq!(
        selected.evidence().realized_strategy(),
        EmissionStrategy::TunedSurfaceProbed
    );
    assert_eq!(
        selected.evidence().completion_proof_kind(),
        CompletionProofKind::TunedClosure
    );
    assert_eq!(
        selected.evidence().grammar_identity(),
        grammar_identity(&grammar)
    );
    assert_eq!(selected.evidence().envelope_id(), request.envelope_id());
    assert!(selected.evidence().is_trusted_complete());
    assert!(!selected.payload_bytes().is_empty());

    let mut analyzer = selected
        .into_analyzer(&grammar)
        .expect("runtime must reconstruct from the exact worker-returned payload");
    let morpher = Morpher::new(&grammar, usize::MAX);
    let options = ParseOptions::default();
    let source = std::str::from_utf8(&source_bytes).expect("word source is UTF-8");
    let source_lines: Vec<&str> = source.lines().collect();
    let cases = language["cases"].as_array().expect("locked cases array");
    assert_eq!(cases.len(), 120);

    for case in cases {
        let case_id = case["caseId"].as_str().expect("case ID");
        let source_line = case["sourceLine"].as_u64().expect("source line") as usize;
        let word = source_lines
            .get(source_line - 1)
            .unwrap_or_else(|| panic!("{case_id}: source line {source_line} is absent"))
            .trim();
        let oracle = morpher.parse_word_opts(word, &options);
        assert!(
            !oracle.timed_out,
            "{case_id}: the exact oracle result is incomplete because it timed out"
        );
        let actual = analyzer.analyze_word(word);
        assert_eq!(
            identities(&grammar, &actual.structured),
            identities(&grammar, &oracle.structured),
            "{case_id}: selected worker payload changed the complete canonical analysis set for \
             frozen source line {source_line}"
        );
    }
    pg_conformance_fixtures::corpus::record_cases(
        "indonesian_120_uses_exact_preferred_worker_payload_with_complete_analysis_sets",
        cases.len(),
    );
}
