//! Ratchets `pg_foma::scoreboard`'s per-(fixture, backend) measurement in both directions.
//! See docs/research/backend-scoreboard-extraction-reconciliation.md for how `EXPECTED` was derived.
use pg_conformance_fixtures::discover;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::scoreboard::{self, CellOutcome, ScoredFixture, MAX_WORDS_PER_FIXTURE};
use pg_foma::strategy_coverage::ALL_STRATEGIES;

/// One discovered fixture: measured, or a named `expect_crash` exclusion.
enum Loaded {
    ExcludedByExpectCrash { label: String },
    Scored(ScoredFixture),
}

fn load_and_measure(fixture: &pg_conformance_fixtures::FixtureRef) -> Loaded {
    let label = fixture.label();
    let words_yaml = fixture.load_words_yaml();
    if words_yaml.expect_crash {
        return Loaded::ExcludedByExpectCrash { label };
    }
    let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
        return Loaded::Scored(scoreboard::unmeasurable(&label, "grammar failed to load"));
    };
    let all_words: Vec<String> = words_yaml.words.iter().map(|w| w.word.clone()).collect();
    let words: Vec<String> = if all_words.len() > MAX_WORDS_PER_FIXTURE {
        all_words[..MAX_WORDS_PER_FIXTURE].to_vec()
    } else {
        all_words
    };
    Loaded::Scored(scoreboard::measure(&label, &grammar, &words))
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Bucket {
    oracle_exact: usize,
    compiles_but_misses: usize,
    refused: usize,
    unmeasurable: usize,
}

impl Bucket {
    fn total(&self) -> usize {
        self.oracle_exact + self.compiles_but_misses + self.refused + self.unmeasurable
    }
}

/// A ratchet, not a target: the latest move is one TSP and one TUT "miss" becoming exact when an all-`expect_fail` fixture (metathesis-comparison-crash) stopped certifying `Truncated` for a proposer that ran and was pruned clean; earlier moves were a boundary-character fix and the `circumfix-conditioned-halves` fixture (60 -> 61 per strategy, a larger denominator) -- see this module's own doc for how each figure was reproduced.
const EXPECTED: &[(EmissionStrategy, Bucket)] = &[
    (
        EmissionStrategy::TunedSurfaceProbed,
        Bucket {
            oracle_exact: 54,
            compiles_but_misses: 1,
            refused: 6,
            unmeasurable: 0,
        },
    ),
    (
        EmissionStrategy::TemplatedUnderlyingTokens,
        Bucket {
            oracle_exact: 35,
            compiles_but_misses: 5,
            refused: 21,
            unmeasurable: 0,
        },
    ),
    (
        EmissionStrategy::PlanComposed,
        Bucket {
            oracle_exact: 20,
            compiles_but_misses: 2,
            refused: 36,
            unmeasurable: 3,
        },
    ),
];

/// A ratchet on the NAMED set, not just the count -- a fixture gaining or losing `expect_crash` is reviewable, not just countable.
const EXPECTED_EXCLUDED: &[&str] = &["machine:edge-cases/simultaneous-epenthesis-cascade"];

/// See docs/research/backend-scoreboard-extraction-reconciliation.md's own section on this fixture for why 0/3 `Refused` here does not contradict `non_first_allomorph_circumfix_recall_parity`'s PRE-confirm containment proof.
const CIRCUMFIX_NON_FIRST_ALLOMORPH_FIXTURE: &str =
    "staging:edge-cases/circumfix-non-first-allomorph-selection";

fn outcome_label(outcome: &CellOutcome) -> &'static str {
    match outcome {
        CellOutcome::OracleExact => "oracle_exact",
        CellOutcome::CompilesButMisses { .. } => "compiles_but_misses",
        CellOutcome::Refused { .. } => "refused",
        CellOutcome::Unmeasurable { .. } => "unmeasurable",
    }
}

#[test]
fn backend_scoreboard_matches_the_ratchet_in_both_directions() {
    let fixtures = discover();
    // Non-vacuity: a short walk would make every bucket read as zero and this gate pass vacuously.
    assert!(
        fixtures.len() > 40,
        "only {} fixtures discovered; a short walk makes this gate vacuous",
        fixtures.len()
    );

    let mut buckets: Vec<(EmissionStrategy, Bucket)> =
        ALL_STRATEGIES.iter().map(|&s| (s, Bucket::default())).collect();
    let mut excluded: Vec<String> = Vec::new();
    let mut scored_fixtures = 0usize;
    let mut soundness_violations: Vec<String> = Vec::new();
    let mut circumfix_outcomes: Vec<(EmissionStrategy, &'static str)> = Vec::new();

    for fixture in &fixtures {
        match load_and_measure(fixture) {
            Loaded::ExcludedByExpectCrash { label } => excluded.push(label),
            Loaded::Scored(row) => {
                scored_fixtures += 1;
                for cell in &row.cells {
                    let (_, bucket) = buckets
                        .iter_mut()
                        .find(|(s, _)| *s == cell.strategy)
                        .expect("every EmissionStrategy has a bucket entry");
                    match outcome_label(&cell.outcome) {
                        "oracle_exact" => bucket.oracle_exact += 1,
                        "compiles_but_misses" => bucket.compiles_but_misses += 1,
                        "refused" => bucket.refused += 1,
                        "unmeasurable" => bucket.unmeasurable += 1,
                        other => panic!("unhandled CellOutcome label {other}"),
                    }
                    if let Some(divergence) = cell.divergence {
                        if divergence.candidate_only_identities > 0 {
                            soundness_violations.push(format!(
                                "{} [{:?}]: {} candidate-only identities (a surviving \
                                 over-generation)",
                                row.label, cell.strategy, divergence.candidate_only_identities
                            ));
                        }
                    }
                    if row.label == CIRCUMFIX_NON_FIRST_ALLOMORPH_FIXTURE {
                        circumfix_outcomes.push((cell.strategy, outcome_label(&cell.outcome)));
                    }
                }
            }
        }
    }

    // Hard invariant, never a ratchet: a surviving over-generation is a real soundness defect.
    assert!(
        soundness_violations.is_empty(),
        "soundness violation(s) found (ADR-0001's propose-and-confirm invariant is one-directional \
         -- overgeneration must never survive confirm): {soundness_violations:?}"
    );

    excluded.sort();
    let mut expected_excluded: Vec<&str> = EXPECTED_EXCLUDED.to_vec();
    expected_excluded.sort_unstable();
    assert_eq!(
        excluded, expected_excluded,
        "expect_crash exclusion set changed -- update EXPECTED_EXCLUDED deliberately (a fixture \
         gaining or losing expect_crash is a real, reviewable event, not just a count)"
    );

    for (strategy, expected) in EXPECTED {
        let (_, measured) = buckets
            .iter()
            .find(|(s, _)| s == strategy)
            .expect("every EmissionStrategy has a bucket entry");
        assert_eq!(
            measured, expected,
            "{strategy:?}: measured {measured:?} but the ratchet says {expected:?} -- a WORSENED \
             count is a regression, an IMPROVED one means this constant is stale and must be \
             updated deliberately (see this module's own reconciliation note)"
        );
    }

    for (strategy, expected) in EXPECTED {
        assert_eq!(
            expected.total(),
            scored_fixtures,
            "{strategy:?}'s own bucket total {} does not equal the {scored_fixtures} scored \
             fixture(s) measured this run",
            expected.total()
        );
    }

    assert_eq!(
        circumfix_outcomes.len(),
        ALL_STRATEGIES.len(),
        "{CIRCUMFIX_NON_FIRST_ALLOMORPH_FIXTURE} was not measured on every strategy -- has it been \
         renamed, removed, or excluded?"
    );
    for (strategy, label) in &circumfix_outcomes {
        assert_eq!(
            *label, "refused",
            "{CIRCUMFIX_NON_FIRST_ALLOMORPH_FIXTURE} [{strategy:?}]: expected a typed `Refused` \
             cell (see this module's own doc on why this fixture's PRE-confirm containment proof \
             and POST-confirm 0/3 measurement do not contradict); got {label} instead -- either a \
             backend gained the ability to build this shape (update this test AND the fixture's own \
             STAGING.md) or the refusal mechanism changed"
        );
    }
}
