//! `conformance-staging/edge-cases/right-to-left-bounded-quantifier-rewrite`'s own regression gate
//! (docs/conformance/representative-typology-basis.md S1.2.2): pins the CURRENT, honest behavior
//! for a `Dir::RightToLeft` rewrite rule whose own environment contains a BOUNDED
//! `PatternNode::Quantifier` (`OptionalSegmentSequence min=0 max=2`).
//!
//! ## A correction to the research doc's own premise, discovered while authoring this fixture
//! S1.2.2 of the research doc assumed a `Quantifier` node anywhere in an RTL rule's own
//! LHS/RHS/environment is still an EXCLUDED shape (grouped with `Segments`/`Anchor`/disagreeing
//! alpha-variables). Empirically, that is true for an UNBOUNDED quantifier (the shape
//! `rust/crates/pg-foma/src/capability.rs`'s own
//! `right_to_left_predicate_refuses_quantifier_shaped_rule` unit test probes, `min=1 max=-1` in the
//! rule's own LHS) but NOT for a genuinely BOUNDED one in the rule's own ENVIRONMENT: this fixture's
//! `rtl_reversal_construction_attempted` characterizes `true` (`crate::replace::pattern_slots`
//! DOES accept a bounded `Quantifier`, per `compile-bounded-fst-quantifiers`'s own
//! `Slot::Repeat` support), and `RightToLeftRewriteFaithfulReversalPredicate` correctly returns
//! `ConfirmOnly`, not `Refuse`. This fixture is therefore NOT one of the three honestly-refused
//! shapes this task otherwise pins -- it demonstrates the ALREADY-CORRECT, already-`ConfirmOnly`
//! propose-and-confirm pipeline for a bounded-quantifier RTL environment, a genuine, useful
//! conformance-coverage addition in its own right (see STAGING.md's own "A correction" section for
//! the full account). This test is the one that should FAIL if this ever regresses back to
//! `Refuse`.

use std::fs;
use std::path::Path;

use pg_foma::capability::CompileDecision;
use pg_foma::capability_entry::evaluate_capability;
use pg_grammar::model::{Dir, Grammar, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions};

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../conformance-staging/edge-cases/right-to-left-bounded-quantifier-rewrite/grammar.xml",
    )
}

fn load() -> Grammar {
    let path = fixture_path();
    let xml = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// Sanity on the fixture's own shape: the rule is genuinely `Dir::RightToLeft`.
#[test]
fn fixture_rule_is_right_to_left() {
    let g = load();
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule at prules[0]");
    };
    assert_eq!(r.dir, Dir::RightToLeft);
}

/// The capability gate's own verdict: `ConfirmOnly` -- a BOUNDED `Quantifier` in an RTL rule's own
/// environment is within `crate::replace::pattern_slots`' supported shape (unlike an UNBOUNDED one,
/// which stays `Refuse`d -- see `rust/crates/pg-foma/src/capability.rs`'s own
/// `right_to_left_predicate_refuses_quantifier_shaped_rule` unit test). This is the module doc's
/// own "correction to the research doc's premise" pinned executably.
#[test]
fn capability_gate_confirms_only_for_bounded_quantifier_in_rtl_environment() {
    let g = load();
    assert_eq!(
        evaluate_capability(&g),
        CompileDecision::ConfirmOnly,
        "a BOUNDED Quantifier in an RTL rule's own environment must be ConfirmOnly (not Refuse) -- \
         if this now Refuses, `pattern_slots`'/`compile_rtl_branch_net`'s own bounded-quantifier \
         support for RTL regressed; if it now Admits, a no-false-positive proof was found -- either \
         way, review before updating this assertion"
    );
}

/// The oracle correctly applies the alternation up to (and not beyond) the bounded quantifier's
/// own `max="2"`, and never accepts a root's own raw, un-rewritten underlying shape as a valid
/// surface form (the rule is obligatory wherever its environment matches).
#[test]
fn oracle_applies_the_bound_correctly() {
    let g = load();
    let morpher = Morpher::new(&g, usize::MAX);

    for (word, expected) in [
        ("acet", "ROOT1|acet"),   // 0 intervening consonants -- within bound
        ("ecct", "ROOT2|ecct"),   // 2 intervening consonants -- exactly saturates the bound
        ("accct", "ROOT3|accct"), // 3 intervening consonants -- one past the bound, unchanged
    ] {
        let outcome = morpher.parse_word_opts(word, &ParseOptions::default());
        assert!(!outcome.invalid_shape, "{word:?} must not be SKIPPED");
        assert_eq!(outcome.signature(), expected, "{word:?} signature mismatch");
    }

    // Negative controls: the raw, un-rewritten underlying shapes are never valid surface forms.
    for word in ["acat", "acct"] {
        let outcome = morpher.parse_word_opts(word, &ParseOptions::default());
        assert!(!outcome.invalid_shape);
        assert_eq!(
            outcome.analyses.len(),
            0,
            "{word:?} (a root's own raw underlying shape) must confirm zero analyses -- the rule \
             is obligatory wherever its environment matches"
        );
    }
}
