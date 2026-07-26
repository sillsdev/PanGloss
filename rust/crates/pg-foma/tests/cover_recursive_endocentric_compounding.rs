//! `conformance-staging/edge-cases/recursive-endocentric-compounding`'s own regression gate
//! (docs/conformance/representative-typology-basis.md S1.2.1): pins the CURRENT, honest behavior
//! for a self-feeding `CompoundingRuleDef` (`headPartsOfSpeech`/`nonHeadPartsOfSpeech`/
//! `outputPartOfSpeech` all naming the same PoS, `multipleApplication="9"`) --
//!
//! 1. the capability gate's structural, word-independent `Refuse` verdict
//!    (`compounding.non-recursive` naming `compounding.recursive`), and
//! 2. the oracle's (`pg_parse::Morpher`) own correct behavior on real words, INCLUDING an
//!    independently-discovered resource ceiling (`AnalyzerConfig::max_stem_count`, hardcoded to 2
//!    inside `Morpher::parse_word_opts`) that separately blocks genuine 3-stem self-feeding
//!    compounding, unrelated to the FST capability gate.
//!
//! This test is the one that should FAIL -- prompting deliberate review, not silent staleness --
//! the day either layer is promoted to actually support recursive/self-feeding compounding: if
//! `CompoundingRecursionSafePredicate` ever admits this shape, or `Morpher`'s own `max_stem_count`
//! ceiling is lifted/raised, `tevimaflisra`'s own expectation below needs to flip from "zero
//! analyses" to a real signature, and the capability assertion needs to move from `Refuse` to
//! whatever `compose_envelope` newly reports.

use std::fs;
use std::path::Path;

use pg_foma::capability::CompileDecision;
use pg_foma::capability_entry::evaluate_capability;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../conformance-staging/edge-cases/recursive-endocentric-compounding/grammar.xml")
}

fn load() -> Grammar {
    let path = fixture_path();
    let xml = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// The capability gate's own verdict: `Refuse`, naming `Compounding`, for the self-feeding shape
/// this fixture's `cr1` declares (`multipleApplication="9"`, PoS re-entering its own input set).
#[test]
fn capability_gate_refuses_recursive_compounding_shape() {
    let g = load();
    match evaluate_capability(&g) {
        CompileDecision::Refuse(diags) => {
            assert!(
                diags.iter().any(|d| d.construct.contains("Compounding")),
                "expected a diagnostic naming Compounding: {diags:?}"
            );
            assert!(
                diags.iter().any(|d| d.predicate == "compounding.non-recursive"),
                "expected the compounding.non-recursive predicate to be the one refusing: {diags:?}"
            );
        }
        other => panic!(
            "expected Refuse for a self-feeding CompoundingRule (multipleApplication > 1), got {other:?}"
        ),
    }
}

/// Bare roots and depth-1 (single-application) compounds parse normally through the oracle --
/// the self-feeding-CAPABLE PoS shape does not itself break the ordinary, already-representative
/// non-recursive case.
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

/// **The load-bearing witness.** `tevimafl` (ROOT1+ROOT2, cr1's first application) re-entering
/// cr1's own head/non-head search a SECOND time with ROOT3 -- (ROOT1+ROOT2)+ROOT3, the exact
/// self-feeding shape design.md D2 item 3 describes -- currently produces ZERO analyses from the
/// standard oracle. This is NOT the FST capability gate's own doing (that verdict is a structural,
/// always-on fact about the rule definition, proven separately above): it is
/// `pg_rules::stratum::AnalyzerConfig::max_stem_count`'s own hardcoded default of 2 (mirroring C#'s
/// `Morpher.MaxStemCount`), which permits at most ONE non-head ever being split off during
/// analysis -- an independently-discovered, separate resource ceiling.
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
