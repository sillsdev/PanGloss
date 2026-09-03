//! Differential gate for `emit.rs`'s token-space pattern-root route: Refused-to-OracleExact in both directions, plus the one fixture that must stay Refused.
use pg_conformance_fixtures::discover;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::scoreboard::{self, CellOutcome, MAX_WORDS_PER_FIXTURE};

fn measure_tut(label: &str) -> CellOutcome {
    let fixtures = discover();
    let fixture = fixtures
        .iter()
        .find(|f| f.label() == label)
        .unwrap_or_else(|| panic!("fixture {label} not discovered"));
    let words_yaml = fixture.load_words_yaml();
    assert!(
        !words_yaml.expect_crash,
        "{label} is expect_crash-excluded; pick a different fixture for this gate"
    );
    let grammar = pg_grammar::load(&fixture.load_grammar_xml())
        .unwrap_or_else(|error| panic!("{label} must load: {error}"));
    let all_words: Vec<String> = words_yaml.words.iter().map(|w| w.word.clone()).collect();
    let words: Vec<String> = if all_words.len() > MAX_WORDS_PER_FIXTURE {
        all_words[..MAX_WORDS_PER_FIXTURE].to_vec()
    } else {
        all_words
    };
    let scored = scoreboard::measure(label, &grammar, &words);
    let cell = scored
        .cells
        .iter()
        .find(|cell| cell.strategy == EmissionStrategy::TemplatedUnderlyingTokens)
        .expect("TemplatedUnderlyingTokens has a cell in every scored fixture");
    if let Some(divergence) = cell.divergence {
        assert_eq!(
            divergence.candidate_only_identities, 0,
            "{label} [TemplatedUnderlyingTokens]: surviving overgeneration -- a real soundness \
             defect, never acceptable regardless of what this gate ratchets"
        );
    }
    cell.outcome.clone()
}

#[test]
fn unbounded_star_pattern_roots_move_from_refused_to_oracle_exact() {
    for label in [
        "staging:edge-cases/backend-strata-generic",
        "machine:languages/polysynthetic-stratal-derivation-chain",
        "staging:edge-cases/guesser-pattern-root-fallback",
    ] {
        let outcome = measure_tut(label);
        assert!(
            matches!(outcome, CellOutcome::OracleExact),
            "{label} [TemplatedUnderlyingTokens]: expected OracleExact via the token-space pattern \
             route (an `[Any]*` root, no RequiredEnvironments, single character table), got {outcome:?}"
        );
    }
}

#[test]
fn bounded_class_reference_pattern_root_moves_from_refused_to_oracle_exact() {
    let outcome = measure_tut("machine:edge-cases/loader-pattern-shapes");
    assert!(
        matches!(outcome, CellOutcome::OracleExact),
        "machine:edge-cases/loader-pattern-shapes [TemplatedUnderlyingTokens]: expected \
         OracleExact via the token-space bounded pattern enumeration (`b[Vowel]t`/`b([Vowel])t`, \
         finite variant sets), got {outcome:?}"
    );
}

#[test]
fn required_environment_pattern_root_stays_refused() {
    let outcome = measure_tut("staging:edge-cases/pattern-root-required-environment");
    assert!(
        matches!(outcome, CellOutcome::Refused { .. }),
        "staging:edge-cases/pattern-root-required-environment [TemplatedUnderlyingTokens]: the \
         token route excludes RequiredEnvironments allomorphs the same way the surface route's own \
         regex route does (an unbounded shape has no literal surface to check an environment \
         against) -- this must stay Refused, not silently start compiling with dropped \
         environments; got {outcome:?}"
    );
}
