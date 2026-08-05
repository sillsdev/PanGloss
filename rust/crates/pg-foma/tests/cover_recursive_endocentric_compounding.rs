//! `conformance-staging/edge-cases/recursive-endocentric-compounding`'s own regression gate
//! (docs/conformance/representative-typology-basis.md S1.2.1): pins the CURRENT, honest behavior
//! for a self-feeding `CompoundingRuleDef` (`headPartsOfSpeech`/`nonHeadPartsOfSpeech`/
//! `outputPartOfSpeech` all naming the same PoS, `multipleApplication="9"`) --
//!
//! 1. the capability gate's structural, word-independent verdict
//!    (`compounding.non-recursive`/`compounding.recursive`), and
//! 2. the oracle's (`pg_parse::Morpher`) own correct behavior on real words, INCLUDING an
//!    independently-discovered resource ceiling (`AnalyzerConfig::max_stem_count`, hardcoded to 2
//!    inside `Morpher::parse_word_opts`) that separately blocks genuine 3-stem self-feeding
//!    compounding, unrelated to the FST capability gate.
//!
//! `crate::emit`'s bounded compound loop unrolls enough extra non-head root levels to realize
//! this rule's own computed `max_depth` bound (`build_compound_chain`, consuming
//! `crate::capability::compounding_max_depth`'s precomputed number directly), so
//! `CompoundingRecursionSafePredicate` reaches `ConfirmOnly` unconditionally for this self-feeding
//! shape (`capability.rs`'s own doc, "the recursive split is now closed too"). The test below is
//! named for that verdict rather than deleted when the verdict changed, per this crate's own
//! "re-author, do not delete a superseded regression pin" convention -- see
//! `cover_compounding_recursive_depth_bound.rs`'s own containment/bound/budget tests for the full
//! construction-side proof. The ORACLE layer is separate and unaffected: `Morpher`'s own
//! `max_stem_count` default (2) permits at most one non-head split during analysis, so
//! `genuinely_recursive_three_stem_compound_currently_confirms_zero_analyses` (below) still pins
//! the real, current oracle-default behavior.

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

/// The capability gate's own verdict for the self-feeding shape this
/// fixture's `cr1` declares (`multipleApplication="9"`, PoS re-entering its own input set) is
/// `ConfirmOnly`, not `Refuse` -- `crate::emit`'s depth-budgeted compound loop closes the
/// construction gap that would otherwise force `Refuse`.
#[test]
fn capability_gate_is_confirm_only_for_recursive_compounding_shape() {
    let g = load();
    assert_eq!(
        evaluate_capability(&g),
        CompileDecision::ConfirmOnly,
        "a self-feeding CompoundingRule (multipleApplication > 1) must now evaluate to ConfirmOnly"
    );
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
/// self-feeding shape a `multipleApplication` compounding rule is meant to allow -- currently
/// produces ZERO analyses from the standard oracle. This is NOT the FST capability gate's own
/// doing (that verdict is a structural, always-on fact about the rule definition, proven
/// separately above): it is
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
