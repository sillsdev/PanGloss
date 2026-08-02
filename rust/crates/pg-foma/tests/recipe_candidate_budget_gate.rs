//! Pins the PER-CANDIDATE proposal budget: `RuntimeBudget::candidate_proposals`.
//!
//! # What this budget is for
//! Measured on Sena's 250-word probe, the plan-composed candidates generated **14,826,003
//! proposals** with 1.35e12 ns of apply time, against **16,815 proposals / 1.04 s** for the
//! whole-grammar compilers -- roughly 880x and 1300x. Scaled to the 4,030-row eligible corpus that
//! is about six hours PER CANDIDATE, and full-corpus runs banked ZERO candidates at both 2.5 and 3
//! hours. No pre-existing knob prunes it: `--confirmation-work` bounds full-HC confirmation CALLS,
//! which happen only AFTER the proposals exist, so the cost is already spent before that budget can
//! trip (verified -- `--confirmation-work 60000` over the full eligible corpus still banked zero
//! candidates in three hours); `--build-ns` bounds the build; `--elapsed-ns` kills the whole run
//! rather than abandoning one candidate.
//!
//! # The fixture, and why this one
//! `recipe-strata-generic` is the only staged fixture that produces all three outcomes these tests
//! need to separate, from ONE grammar under ONE shared budget. Measured unbounded (deterministic --
//! `Score`'s proposal counts have zero run-to-run spread, which is exactly why the budget is
//! denominated in proposals rather than nanoseconds):
//!
//! | candidate | realized strategy | proposals | verdict |
//! |---|---|---|---|
//! | 0-3 | `PlanComposed` | 28 | confirmed |
//! | 4 | `TunedSurfaceProbed` | 384 | confirmed |
//! | 5 | `TemplatedUnderlyingTokens` | 27 | identity mismatch on `buuubuuu` |
//!
//! So a budget of 100 proposals splits them three ways: candidate 4 is EXPENSIVE BUT CORRECT,
//! candidate 5 is INEXPENSIVE BUT WRONG, and candidates 0-3 are unaffected. That is the separation
//! the whole design turns on -- see [`expensive_and_wrong_candidates_are_distinguishable_in_one_report`].

use pg_foma::recipe_optimizer::{pareto_frontier, select_confirmed, Certification, Score};
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::recipe_runtime::{
    evaluate_plans, evaluate_plans_with_cache, RunEvaluationCache, RuntimeBudget, RuntimeEvaluation,
};
use pg_foma::replace::SegAlphabet;
use pg_foma::{enumerate::enumerate_default, junctions::PhonologyProbe};

/// The shared budget every test below uses: above the 27/28-proposal candidates, below the
/// 384-proposal one. See this module's doc for the measured table it is derived from.
const SPLITTING_BUDGET: u64 = 100;

/// The fixture whose PlanComposed permutations require subtrees `build_controllable` cannot build,
/// so their completed verdict is `Unsupported`. Used only by the relabelling tests. Measured
/// unbounded: candidates 1-4 are `PlanComposed`/16 proposals/`Unsupported`, candidates 0 and 5 are
/// `TunedSurfaceProbed`/366/confirmed, candidate 6 is `TemplatedUnderlyingTokens`/60/mismatch.
const MARKER_FIXTURE: &str = "recipe-ordered-generic";

/// Below the 16 proposals the `Unsupported` permutations spend, so they are abandoned mid-corpus.
const MARKER_FIXTURE_TIGHT_BUDGET: u64 = 8;

/// Above those 16, so they run to their real `Unsupported` verdict, while the 366-proposal
/// candidates are abandoned.
const MARKER_FIXTURE_LOOSE_BUDGET: u64 = 100;

fn fixture() -> (pg_grammar::model::Grammar, Vec<String>) {
    named_fixture("recipe-strata-generic")
}

fn named_fixture(name: &str) -> (pg_grammar::model::Grammar, Vec<String>) {
    let fixture = pg_conformance_fixtures::discover()
        .into_iter()
        .find(|fixture| {
            fixture.root == pg_conformance_fixtures::Root::Staging && fixture.name == name
        })
        .unwrap_or_else(|| panic!("staged {name} fixture"));
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture grammar");
    let words = fixture
        .load_words_yaml()
        .words
        .into_iter()
        .map(|word| word.word)
        .collect();
    (grammar, words)
}

fn plans(grammar: &pg_grammar::model::Grammar) -> Vec<pg_foma::enumerate::CandidatePlan> {
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let prules = grammar
        .strata
        .iter()
        .flat_map(|stratum| &stratum.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phonology = PhonologyProbe::new(grammar);
    let baseline = enumerate_default(grammar, &alphabet, &prules, phonology.as_ref());
    Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar,
            baseline: &baseline,
        })
        .expect("fixture plans")
        .into_iter()
        .map(|(_, plan)| plan)
        .collect()
}

fn unbounded() -> Vec<RuntimeEvaluation> {
    let (grammar, words) = fixture();
    evaluate_plans(&grammar, &plans(&grammar), &words, RuntimeBudget::default())
        .expect("the oracle liveness net / memory ceiling must not trip on this fixture")
}

fn bounded(limit: u64) -> Vec<RuntimeEvaluation> {
    let (grammar, words) = fixture();
    evaluate_plans(
        &grammar,
        &plans(&grammar),
        &words,
        RuntimeBudget {
            candidate_proposals: Some(limit),
            ..RuntimeBudget::default()
        },
    )
    .expect("the oracle liveness net / memory ceiling must not trip on this fixture")
}

fn evaluate_named(name: &str, limit: Option<u64>) -> Vec<RuntimeEvaluation> {
    let (grammar, words) = named_fixture(name);
    evaluate_plans(
        &grammar,
        &plans(&grammar),
        &words,
        RuntimeBudget {
            candidate_proposals: limit,
            ..RuntimeBudget::default()
        },
    )
    .expect("the oracle liveness net / memory ceiling must not trip on this fixture")
}

fn budget_verdicts(evaluations: &[RuntimeEvaluation]) -> Vec<&Certification> {
    evaluations
        .iter()
        .map(|evaluation| &evaluation.certification)
        .filter(|certification| matches!(certification, Certification::BudgetExceeded { .. }))
        .collect()
}

fn ranked(evaluations: &[RuntimeEvaluation]) -> Vec<(String, Certification, Score)> {
    evaluations
        .iter()
        .enumerate()
        .map(|(index, evaluation)| {
            (
                format!("candidate-{index}"),
                evaluation.certification.clone(),
                evaluation.score,
            )
        })
        .collect()
}

/// The fixture assumption every other test rests on. If a grammar or compiler change moves these
/// numbers, this fails FIRST and names what moved, rather than leaving the tests below to fail
/// for a reason that looks like a bug in the budget.
#[test]
fn fixture_still_offers_an_expensive_correct_and_a_cheap_wrong_candidate() {
    let unbounded = unbounded();
    let expensive_and_correct = unbounded
        .iter()
        .filter(|evaluation| evaluation.certification.selectable())
        .map(|evaluation| evaluation.score.proposals)
        .max()
        .expect("the fixture must confirm at least one candidate");
    let cheap_and_wrong = unbounded
        .iter()
        .filter(|evaluation| {
            matches!(
                evaluation.certification,
                Certification::IdentityMismatch { .. }
            )
        })
        .map(|evaluation| evaluation.score.proposals)
        .max()
        .expect("the fixture must produce at least one identity mismatch");
    assert!(
        cheap_and_wrong < SPLITTING_BUDGET && SPLITTING_BUDGET < expensive_and_correct,
        "the shared budget must separate the two: wrong={cheap_and_wrong}, \
         budget={SPLITTING_BUDGET}, correct={expensive_and_correct}"
    );
    assert!(
        budget_verdicts(&unbounded).is_empty(),
        "an unbounded run must never produce a budget verdict"
    );
}

/// THE test the design exists for: with one budget in force, a candidate that is merely EXPENSIVE
/// and a candidate that is WRONG must be told apart by reading the report.
///
/// Both wrong answers are excluded here. Reporting the expensive candidate as a mismatch would
/// blame it for being wrong when it was only costly; reporting the wrong candidate as
/// budget-exceeded would let a genuine identity disagreement be absorbed by a cost gate.
#[test]
fn expensive_and_wrong_candidates_are_distinguishable_in_one_report() {
    let unbounded = unbounded();
    let bounded = bounded(SPLITTING_BUDGET);
    assert_eq!(unbounded.len(), bounded.len());

    let mut abandoned = 0usize;
    let mut mismatched = 0usize;
    for (index, (before, after)) in unbounded.iter().zip(&bounded).enumerate() {
        match &after.certification {
            Certification::BudgetExceeded {
                dimension,
                limit,
                observed,
                words_evaluated,
                words_requested,
            } => {
                abandoned += 1;
                assert_eq!(dimension, "proposals");
                assert_eq!(*limit, SPLITTING_BUDGET);
                assert!(
                    *observed > *limit,
                    "the observed total must be the one that actually tripped the budget"
                );
                assert!(
                    words_evaluated < words_requested,
                    "an abandoned candidate must have stopped BEFORE the corpus ended, \
                     otherwise the budget only reported a cost it had already paid \
                     ({words_evaluated}/{words_requested})"
                );
                // The saving, stated as a measurement rather than assumed: this candidate did
                // strictly less propose work than the same candidate does unbudgeted.
                assert!(
                    after.score.proposals < before.score.proposals,
                    "candidate {index} was abandoned but still did all {} proposals",
                    before.score.proposals
                );
                assert_eq!(
                    after.score.proposals, *observed,
                    "the verdict's observed value and the recorded score must be one measurement"
                );
                // ... and this candidate is one whose true verdict is CONFIRMED. Abandoning it
                // must not have been provoked by, or reported as, a disagreement.
                assert!(
                    before.certification.selectable(),
                    "this test's whole point is an EXPENSIVE BUT CORRECT candidate; candidate \
                     {index}'s true verdict is {:?}",
                    before.certification
                );
            }
            Certification::IdentityMismatch { word, .. } => {
                mismatched += 1;
                // Unchanged by the budget's presence: a wrong candidate cheap enough to finish is
                // still reported as wrong, naming the same word.
                assert_eq!(
                    &after.certification, &before.certification,
                    "a candidate inside the budget must keep its true verdict verbatim"
                );
                assert!(!word.is_empty());
            }
            other => {
                assert_eq!(
                    other, &before.certification,
                    "candidate {index} is inside the budget, so nothing about it may change"
                );
            }
        }
    }
    assert!(
        abandoned > 0,
        "the budget must actually abandon an expensive candidate"
    );
    assert!(
        mismatched > 0,
        "a wrong candidate must still be reported as wrong while the budget is in force"
    );
}

/// An abandoned candidate is a REPORTED outcome that can never be chosen -- not a silent absence
/// (which this repo treats as "I could not look" reading as a result) and not a selectable one.
#[test]
fn an_abandoned_candidate_is_reported_and_can_never_win() {
    let bounded = bounded(SPLITTING_BUDGET);
    let verdicts = budget_verdicts(&bounded);
    assert!(!verdicts.is_empty());
    for verdict in &verdicts {
        assert!(
            !verdict.selectable(),
            "a candidate abandoned for cost was never certified against the corpus: {verdict:?}"
        );
        assert!(
            verdict.shortest_disagreement().is_none(),
            "cost is not a disagreement, so it must not present as one: {verdict:?}"
        );
    }
    let ranked = ranked(&bounded);
    assert!(
        !pareto_frontier(&ranked)
            .iter()
            .any(|id| matches!(
                ranked.iter().find(|(candidate, _, _)| candidate == id),
                Some((_, Certification::BudgetExceeded { .. }, _))
            )),
        "an abandoned candidate must not enter the Pareto frontier"
    );
    // The fixture still confirms cheap candidates, so a winner is still selected -- the run
    // continues rather than being killed by one expensive candidate.
    let winner = select_confirmed(&ranked).expect("cheap confirmed candidates must still win");
    assert!(matches!(
        ranked
            .iter()
            .find(|(id, _, _)| *id == winner)
            .map(|(_, certification, _)| certification),
        Some(Certification::FullHcConfirmed { .. })
    ));
}

/// Eligibility is decided by the ORACLE before any candidate is materialized, so abandoning a
/// candidate must be invisible to the corpus ledger. Asserted against the published evidence
/// itself -- counts and all four digests -- both across a budgeted/unbudgeted pair and across the
/// before/after of a budgeted evaluation on one cache.
#[test]
fn abandoning_a_candidate_leaves_the_corpus_ledger_byte_identical() {
    let (grammar, words) = fixture();
    let plans = plans(&grammar);

    let mut cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
        .expect("oracle preparation must succeed for this fixture");
    let before = cache.corpus_evidence(&words);
    let bounded = evaluate_plans_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget {
            candidate_proposals: Some(SPLITTING_BUDGET),
            ..RuntimeBudget::default()
        },
        &mut cache,
    );
    assert!(
        !budget_verdicts(&bounded).is_empty(),
        "this assertion is vacuous unless the budget actually tripped"
    );
    let after = cache.corpus_evidence(&words);

    let mut unbudgeted_cache =
        RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
            .expect("oracle preparation must succeed for this fixture");
    let _ = evaluate_plans_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget::default(),
        &mut unbudgeted_cache,
    );
    let unbudgeted = unbudgeted_cache.corpus_evidence(&words);

    for (label, other) in [("after the budgeted run", &after), ("unbudgeted", &unbudgeted)] {
        assert_eq!(
            (before.requested, before.included, before.excluded),
            (other.requested, other.included, other.excluded),
            "requested/included/excluded moved {label}"
        );
        assert_eq!(before.exclusions, other.exclusions, "exclusions moved {label}");
        assert_eq!(
            serde_json::to_value(&before).expect("evidence must serialize"),
            serde_json::to_value(other).expect("evidence must serialize"),
            "the published ledger -- every digest included -- moved {label}"
        );
    }
    assert_eq!(
        before.excluded, 0,
        "this fixture must have a fully eligible corpus, or the assertion above could pass by \
         both sides being equally broken"
    );

    // And the verdict's own denominator agrees with the ledger's included count, so a reader can
    // tell how much of the ELIGIBLE corpus was skipped rather than guessing.
    for verdict in budget_verdicts(&bounded) {
        let Certification::BudgetExceeded {
            words_requested, ..
        } = verdict
        else {
            unreachable!("filtered above")
        };
        assert_eq!(*words_requested, before.included);
    }
}

/// Indices whose true (unbounded) verdict is `Unsupported`, plus their proposal counts.
fn unsupported_candidates(unbounded: &[RuntimeEvaluation]) -> Vec<(usize, u64)> {
    unbounded
        .iter()
        .enumerate()
        .filter(|(_, evaluation)| {
            matches!(evaluation.certification, Certification::Unsupported { .. })
        })
        .map(|(index, evaluation)| (index, evaluation.score.proposals))
        .collect()
}

/// "Too expensive" must never be reported as "this compiler cannot represent this grammar".
///
/// The composed path's tail concludes `Unsupported` for a permutation that failed on the
/// controllable-only network AND needs subtrees `build_controllable` cannot build. An abandoned
/// candidate satisfies only the second conjunct -- the corpus never finished, so whether the
/// network fails is UNKNOWN -- yet before the fix it fell straight through into that arm and came
/// out wearing the `Unsupported` label with its typed budget verdict buried inside a `{:?}` string.
///
/// This is exactly the failure that regresses invisibly: the label is correct where it is assigned
/// and wrong by the time it is read, so no assertion at the assignment site would catch it. A
/// reader would conclude a compiler lacks a capability it demonstrably has, and the remedy they
/// would reach for (change the compiler) is not the one that works (raise the budget).
#[test]
fn an_abandoned_candidate_is_never_relabelled_unsupported() {
    let unbounded = evaluate_named(MARKER_FIXTURE, None);
    let unsupported = unsupported_candidates(&unbounded);
    assert!(
        !unsupported.is_empty(),
        "{MARKER_FIXTURE} must still produce marker-requiring permutations, or this test proves \
         nothing"
    );
    for (index, proposals) in &unsupported {
        assert!(
            *proposals > MARKER_FIXTURE_TIGHT_BUDGET,
            "candidate {index} spends only {proposals} proposals, so the tight budget would never \
             trip and this test would pass vacuously"
        );
    }
    let tight = evaluate_named(MARKER_FIXTURE, Some(MARKER_FIXTURE_TIGHT_BUDGET));
    for (index, _) in &unsupported {
        let certification = &tight[*index].certification;
        assert!(
            matches!(certification, Certification::BudgetExceeded { .. }),
            "candidate {index} was abandoned for cost before its corpus finished, so nothing is \
             yet known about whether the controllable network can honour its plan; it must not be \
             reported as Unsupported. Got {certification:?}"
        );
    }
    // ... and the verdict must be the top-level tag, never text buried inside another verdict's
    // reason. A reader filtering the report by `status` has to find it.
    for evaluation in &tight {
        if let Certification::Unsupported { reason } = &evaluation.certification {
            assert!(
                !reason.contains("BudgetExceeded"),
                "a budget verdict was swallowed into an Unsupported reason string: {reason}"
            );
        }
    }
}

/// The conflation is bad in both directions: a plan that genuinely EARNS `Unsupported` must keep
/// that verdict verbatim while a budget is in force.
///
/// Structurally this cannot fail -- `BudgetExceeded` has exactly one producer, the in-loop
/// `proposals > limit` test, so a candidate that finishes its corpus cannot emit one. The test
/// exists because that is a property of where the code sits, which a later refactor could move
/// without noticing.
#[test]
fn a_genuinely_unsupported_plan_is_never_relabelled_budget_exceeded() {
    let unbounded = evaluate_named(MARKER_FIXTURE, None);
    let unsupported = unsupported_candidates(&unbounded);
    assert!(!unsupported.is_empty());
    for (index, proposals) in &unsupported {
        assert!(
            *proposals < MARKER_FIXTURE_LOOSE_BUDGET,
            "candidate {index} must finish inside the loose budget ({proposals} proposals)"
        );
    }
    let loose = evaluate_named(MARKER_FIXTURE, Some(MARKER_FIXTURE_LOOSE_BUDGET));
    for (index, _) in &unsupported {
        assert_eq!(
            loose[*index].certification, unbounded[*index].certification,
            "candidate {index} finished inside its budget, so its verdict must be untouched"
        );
    }
    assert!(
        !budget_verdicts(&loose).is_empty(),
        "some other candidate must have been abandoned under this same budget, or the assertions \
         above hold only because the budget was never in play"
    );
}

/// A budget nothing reaches must change nothing at all -- verdicts and every deterministic score
/// component identical to an unbudgeted run. A cost gate that perturbs the candidates it does not
/// prune is not a cost gate.
#[test]
fn a_budget_no_candidate_reaches_is_a_no_op() {
    let unbounded = unbounded();
    let bounded = bounded(u64::MAX - 1);
    let deterministic = |evaluations: &[RuntimeEvaluation]| {
        evaluations
            .iter()
            .map(|evaluation| {
                (
                    evaluation.certification.clone(),
                    evaluation.realized_strategy,
                    evaluation.score.states,
                    evaluation.score.arcs,
                    evaluation.score.proposals,
                    evaluation.score.confirmation,
                    evaluation.score.confirmation_steps,
                    evaluation.score.raw_paths,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(deterministic(&unbounded), deterministic(&bounded));
}
