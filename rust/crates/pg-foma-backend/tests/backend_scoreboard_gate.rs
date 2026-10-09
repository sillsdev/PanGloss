//! Ratchets `pg_foma::scoreboard`'s per-(fixture, backend) measurement in both directions.
//! See docs/research/backend-scoreboard-extraction-reconciliation.md for how `EXPECTED` was derived.
use pg_conformance_fixtures::discover;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::strategy_coverage::ALL_STRATEGIES;
use pg_foma_backend::scoreboard::{self, CellOutcome, ScoredFixture, MAX_WORDS_PER_FIXTURE};

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

/// Pins measured coverage, keeping missing proposals distinct from typed refusals.
const EXPECTED: &[(EmissionStrategy, Bucket)] = &[
    (
        EmissionStrategy::TunedSurfaceProbed,
        Bucket {
            oracle_exact: 69,
            compiles_but_misses: 3,
            refused: 2,
            unmeasurable: 0,
        },
    ),
    (
        EmissionStrategy::TemplatedUnderlyingTokens,
        Bucket {
            oracle_exact: 47,
            compiles_but_misses: 3,
            refused: 24,
            unmeasurable: 0,
        },
    ),
    (
        EmissionStrategy::PlanComposed,
        Bucket {
            oracle_exact: 34,
            compiles_but_misses: 2,
            refused: 35,
            unmeasurable: 3,
        },
    ),
];

/// A ratchet on the NAMED set, not just the count -- a fixture gaining or losing `expect_crash` is reviewable, not just countable.
const EXPECTED_EXCLUDED: &[&str] = &["machine:edge-cases/simultaneous-epenthesis-cascade"];

/// See docs/research/backend-scoreboard-extraction-reconciliation.md's own section on this fixture for why TSP now measures `OracleExact` here without contradicting `non_first_allomorph_circumfix_recall_parity`'s PRE-confirm containment proof.
const CIRCUMFIX_NON_FIRST_ALLOMORPH_FIXTURE: &str =
    "staging:edge-cases/circumfix-non-first-allomorph-selection";

/// See docs/research/backend-scoreboard-extraction-reconciliation.md's own section on this fixture for the hc.dll reading behind TSP's `refused` -> `oracle_exact` move.
const REALIZATIONAL_UNBOUNDED_FIXTURE: &str = "machine:languages/suffixing-extension-slot-ordering";

/// Expected `outcome_label` per `(fixture, strategy)`, checked as a table so each pin states what changed rather than a uniform `refused`.
fn expected_pinned_outcome(fixture: &str, strategy: EmissionStrategy) -> &'static str {
    use EmissionStrategy::{PlanComposed, TemplatedUnderlyingTokens, TunedSurfaceProbed};
    match (fixture, strategy) {
        (f, TunedSurfaceProbed | TemplatedUnderlyingTokens)
            if f == CIRCUMFIX_NON_FIRST_ALLOMORPH_FIXTURE =>
        {
            "oracle_exact"
        }
        (f, PlanComposed) if f == CIRCUMFIX_NON_FIRST_ALLOMORPH_FIXTURE => "oracle_exact",
        (f, TunedSurfaceProbed) if f == REALIZATIONAL_UNBOUNDED_FIXTURE => "oracle_exact",
        (f, TemplatedUnderlyingTokens | PlanComposed) if f == REALIZATIONAL_UNBOUNDED_FIXTURE => {
            "refused"
        }
        (f, s) => panic!("no pinned expectation for ({f}, {s:?})"),
    }
}

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

    let mut buckets: Vec<(EmissionStrategy, Bucket)> = ALL_STRATEGIES
        .iter()
        .map(|&s| (s, Bucket::default()))
        .collect();
    let mut excluded: Vec<String> = Vec::new();
    let mut scored_fixtures = 0usize;
    let mut soundness_violations: Vec<String> = Vec::new();
    let mut pinned_outcomes: Vec<(&'static str, EmissionStrategy, &'static str)> = Vec::new();
    // Named, not only counted, so a moved ratchet says WHICH fixture moved.
    let mut not_exact: Vec<String> = Vec::new();

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
                    let label = outcome_label(&cell.outcome);
                    eprintln!(
                        "SCOREBOARD_CELL\t{}\t{:?}\t{}",
                        row.label, cell.strategy, label
                    );
                    if label != "oracle_exact" {
                        not_exact.push(format!("{} [{:?}]: {label}", row.label, cell.strategy));
                    }
                    match label {
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
                    if row.label == CIRCUMFIX_NON_FIRST_ALLOMORPH_FIXTURE
                        || row.label == REALIZATIONAL_UNBOUNDED_FIXTURE
                    {
                        let f = if row.label == CIRCUMFIX_NON_FIRST_ALLOMORPH_FIXTURE {
                            CIRCUMFIX_NON_FIRST_ALLOMORPH_FIXTURE
                        } else {
                            REALIZATIONAL_UNBOUNDED_FIXTURE
                        };
                        pinned_outcomes.push((f, cell.strategy, outcome_label(&cell.outcome)));
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

    not_exact.sort();
    eprintln!("backend_scoreboard_gate: cells that are not oracle_exact:");
    for line in &not_exact {
        eprintln!("  {line}");
    }

    for (strategy, expected) in EXPECTED {
        let (_, measured) = buckets
            .iter()
            .find(|(s, _)| s == strategy)
            .expect("every EmissionStrategy has a bucket entry");
        assert_eq!(
            measured, expected,
            "{strategy:?}: measured {measured:?} but the ratchet says {expected:?} -- a WORSENED \
             count is a regression, an IMPROVED one means this constant is stale and must be \
             updated deliberately (see this module's own reconciliation note); the non-exact \
             cells are listed on stderr above"
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
        pinned_outcomes.len(),
        2 * ALL_STRATEGIES.len(),
        "one of the two pinned fixtures was not measured on every strategy -- has it been renamed, \
         removed, or excluded?"
    );
    for (fixture, strategy, label) in &pinned_outcomes {
        let expected = expected_pinned_outcome(fixture, *strategy);
        assert_eq!(
            *label, expected,
            "{fixture} [{strategy:?}]: expected `{expected}` (see this module's own doc) but got \
             {label} instead -- either a backend's capability changed or the refusal mechanism did; \
             update this test, `expected_pinned_outcome`, and the fixture's own STAGING.md together"
        );
    }
}
