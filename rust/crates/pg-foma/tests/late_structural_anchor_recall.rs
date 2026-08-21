//! Mbugwe-derived containment regression for a structural anchor reached after four ordinary rules.

use std::path::PathBuf;

use pg_foma::emit;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

fn load_fixture() -> Grammar {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("conformance-staging/edge-cases/late-structural-anchor-five-rule-chain/grammar.xml");
    let xml = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|error| panic!("fixture failed to load: {error}"))
}

#[test]
fn five_rule_chain_with_late_structural_anchor_refuses_incomplete_fst() {
    let grammar = load_fixture();
    let emitted = emit::emit(&grammar);
    let reason = match &emitted.report.tier {
        emit::FomaTier::Unsupported { reason } => reason,
        tier => panic!("a live successor beyond the fixed closure depth must refuse, got {tier:?}"),
    };
    assert!(
        reason.contains("live successor") && reason.contains("incomplete"),
        "refusal must identify incomplete closure: {reason}"
    );
    assert!(
        emitted.report.enum_budget_exceeded.is_none(),
        "fixture must reach the closure refusal before any resource limit"
    );
    assert!(
        emitted.lexc_source.is_empty(),
        "incomplete closure must not return a usable lexc artifact"
    );

    let morpher = Morpher::new(&grammar, 20_000);
    let oracle = morpher.parse_word_opts("fedcbag", &ParseOptions::default());
    assert!(
        !oracle.structured.is_empty(),
        "full-engine oracle must analyze the load-bearing surface"
    );
}
