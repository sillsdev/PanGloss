use super::*;
use std::path::Path;

const STACK_BYTES: usize = 512 * 1024 * 1024;

fn load_aweti() -> Option<Grammar> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../samples/data/aweti.json");
    let json = std::fs::read_to_string(&path).ok()?;
    let snapshot = pg_snapshot::Snapshot::from_json(&json)
        .unwrap_or_else(|error| panic!("parse snapshot {}: {error}", path.display()));
    let (grammar, _warnings) = pg_grammar::compile_project(&snapshot)
        .unwrap_or_else(|error| panic!("compile_project {}: {error}", path.display()));
    Some(grammar)
}

/// This compiler must actually build for a template-bearing grammar that declares no phonological rules, not just for the phonology-bearing shape its existing callers exercised.
#[test]
fn phonology_free_templated_grammar_compiles_through_this_path() {
    let fixture =
        pg_conformance_fixtures::require_fixture("edge-cases", "backend-template-generic");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");
    assert!(
        grammar.prules.is_empty(),
        "fixture is used here BECAUSE it declares no phonological rules"
    );
    assert!(
        !grammar.templates.is_empty(),
        "fixture is used here BECAUSE it declares affix templates"
    );

    let compiled = compile_templated_morphotactics(&grammar)
        .expect("a phonology-free templated grammar must compile, not fail with NoCompiledRules");
    assert_eq!(
        compiled.profile.phonological_rule_count, 0,
        "this fixture declares no phonological rules; the profile should say so honestly"
    );
    let (states, arcs) = compiled.proposer.network_counts();
    assert!(
        states > 0 && arcs > 0,
        "must yield a real network, not an empty one that would analyze nothing: {states} \
         states / {arcs} arcs"
    );

    // The zero-slot boundary word: a plain propose against the compiled network must find at least one candidate.
    let mut proposer = compiled.proposer;
    let candidates = proposer.propose("k");
    assert!(
        !candidates.is_empty(),
        "the zero-slot word must propose at least one candidate on the compiled network"
    );
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/aweti.json); run with --include-ignored"]
fn large_templated_network_is_prepared_for_binary_search_apply() {
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(|| {
            let Some(grammar) = load_aweti() else {
                eprintln!("skipping: aweti.json not present on disk");
                return;
            };
            let compiled = compile_templated_morphotactics(&grammar)
                .expect("Aweti templated compile pipeline must succeed");
            assert!(
                compiled.network.arcs_sorted_out,
                "large templated network must use foma's binary-search apply path"
            );
        })
        .expect("spawn large-stack Aweti compile worker");
    handle
        .join()
        .expect("large-stack Aweti compile worker panicked");
}
