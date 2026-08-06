//! Pins that a BOUNDED `Quantifier` in an RTL rule's own environment is `ConfirmOnly`, not `Refuse`,
//! correcting docs/conformance/representative-typology-basis.md S1.2.2's assumption that grouped it as excluded.

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

/// The capability gate returns `ConfirmOnly` for a bounded `Quantifier` in an RTL rule's own environment.
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

/// The oracle applies the alternation only within the quantifier's bound, and never accepts the raw, un-rewritten shape since the rule is obligatory.
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
