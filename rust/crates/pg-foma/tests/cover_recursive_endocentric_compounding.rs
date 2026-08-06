//! Pins the current, honest behavior for a self-feeding `CompoundingRuleDef` (`multipleApplication="9"`, PoS re-entering its own input set): (1) the capability gate's structural verdict, and (2) the oracle's own behavior on real words, including an independently-discovered resource ceiling (`AnalyzerConfig::max_stem_count`, hardcoded to 2) that separately blocks genuine 3-stem self-feeding compounding, unrelated to the FST capability gate.

use std::fs;
use std::path::Path;

use pg_foma::capability::CompileDecision;
use pg_foma::capability_entry::evaluate_capability;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../conformance-staging/edge-cases/recursive-endocentric-compounding/grammar.xml",
    )
}

fn load() -> Grammar {
    let path = fixture_path();
    let xml = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// This fixture's self-feeding `cr1` (`multipleApplication="9"`) must evaluate to `ConfirmOnly`, not `Refuse` — `crate::emit`'s depth-budgeted compound loop closes the construction gap that would otherwise force a refusal.
#[test]
fn capability_gate_is_confirm_only_for_recursive_compounding_shape() {
    let g = load();
    assert_eq!(
        evaluate_capability(&g),
        CompileDecision::ConfirmOnly,
        "a self-feeding CompoundingRule (multipleApplication > 1) must now evaluate to ConfirmOnly"
    );
}

/// Bare roots and depth-1 compounds parse normally: the self-feeding-capable PoS shape does not itself break the ordinary non-recursive case.
#[test]
fn bare_roots_and_depth_one_compounds_parse_normally() {
    let g = load();
    let morpher = Morpher::new(&g, usize::MAX);

    for (word, expected) in [
        ("tevi", "ROOT1|tevi"),
        ("mafl", "ROOT2|mafl"),
        ("isra", "ROOT3|isra"),
        ("tevimafl", "ROOT1+ROOT2|tevimafl"),
        ("maflisra", "ROOT2+ROOT3|maflisra"),
    ] {
        let outcome = morpher.parse_word_opts(word, &ParseOptions::default());
        assert!(!outcome.invalid_shape, "{word:?} must not be SKIPPED");
        assert_eq!(outcome.signature(), expected, "{word:?} signature mismatch");
    }
}

/// The load-bearing witness: (ROOT1+ROOT2)+ROOT3, the exact self-feeding shape `multipleApplication` is meant to allow, currently produces zero analyses — not the FST capability gate's doing, but `AnalyzerConfig::max_stem_count`'s hardcoded default of 2, which permits at most one non-head split during analysis.
#[test]
fn genuinely_recursive_three_stem_compound_currently_confirms_zero_analyses() {
    let g = load();
    let morpher = Morpher::new(&g, usize::MAX);

    let outcome = morpher.parse_word_opts("tevimaflisra", &ParseOptions::default());
    assert!(
        !outcome.invalid_shape,
        "tevimaflisra must segment fine (every character is in the grammar's table) -- the zero \
         result is a derivation-search ceiling, not an invalid-shape issue"
    );
    assert_eq!(
        outcome.analyses.len(),
        0,
        "tevimaflisra (a genuine 3-stem self-feeding compound) must currently confirm ZERO \
         analyses -- if this now finds an analysis, either the recursion-depth budget changed or \
         Morpher's own max_stem_count ceiling was lifted; review before updating this assertion"
    );
}
