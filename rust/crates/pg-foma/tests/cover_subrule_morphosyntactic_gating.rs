//! `machine/conformance/edge-cases/subrule-morphosyntactic-gating`'s own regression gate
//! (docs/conformance/representative-typology-basis.md S1.2.7): pins the CURRENT, honest behavior
//! for a `PhonologicalSubrule` gated by `requiredPartsOfSpeech` on a POS a `MorphologicalRule` sets
//! within the same derivation --
//!
//! 1. the capability gate's own `Admit` verdict (`SubruleGating` is `Disposition::Proven`, no
//!    compiler gap -- `gate.rs`'s existing partition mechanism already handles this faithfully), and
//! 2. the oracle's (`pg_parse::Morpher`) own correct disambiguation of the identical phonological
//!    environment ("p" before "a") reached via two different morphosyntactic derivation states.
//!
//! Unlike the other three fixtures this task adds, `SubruleGating` is not a refused construct --
//! this test's value is pure conformance-coverage (pinning that the ALREADY-correct behavior stays
//! correct), not a refusal pin.

use std::fs;
use std::path::Path;

use pg_foma::capability::CompileDecision;
use pg_foma::capability_entry::evaluate_capability;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../machine/conformance/edge-cases/subrule-morphosyntactic-gating/grammar.xml")
}

fn load() -> Grammar {
    let path = fixture_path();
    let xml = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// The capability gate's own verdict: `Admit` -- `SubruleGating` costs nothing extra.
#[test]
fn capability_gate_admits_subrule_gated_grammar() {
    let g = load();
    assert_eq!(
        evaluate_capability(&g),
        CompileDecision::Admit,
        "a grammar whose only ConfigPredicate-relevant characteristic is SubruleGating (Proven) \
         must Admit, never Refuse or ConfirmOnly"
    );
}

/// The oracle correctly disambiguates the SAME phonological environment ("p" before "a") reached
/// via two different morphosyntactic derivation states: `pat` (no derivation, gate blocked) vs.
/// `bat` (derived via `mrDerive`, gate licensed).
#[test]
fn oracle_correctly_gates_the_alternation_by_derivation_state() {
    let g = load();
    let morpher = Morpher::new(&g, usize::MAX);

    let bare = morpher.parse_word_opts("pat", &ParseOptions::default());
    assert!(!bare.invalid_shape);
    assert_eq!(
        bare.signature(),
        "ROOT1|pat",
        "the underived word must surface unchanged -- the gate must not fire without mrDerive"
    );

    let derived = morpher.parse_word_opts("bat", &ParseOptions::default());
    assert!(!derived.invalid_shape);
    assert_eq!(
        derived.signature(),
        "ROOT1+DERIVE|bat",
        "the derived word must show the alternation -- the gate must fire once mrDerive sets \
         posDerived"
    );
}
