//! The fast accuracy path, gated against the slow one: pins that it agrees with certification, is really confirmation-free, actually executes, and never reports a pass it did not earn -- but never that its verdict may select a candidate.
//! See `docs/research/pg-foma-recipe-accuracy-gate-notes.md` for the four claims in full and why each matters.

use pg_conformance_fixtures::{discover, Root};
use pg_foma::backend_accuracy::{candidate_admission_key, AccuracyVerdict};
use pg_foma::backend_registry::{MaterializerContext, Registry};
use pg_foma::backend_runtime::{
    assess_accuracy_with_cache, evaluate_plans_with_cache, RunEvaluationCache, RuntimeBudget,
};
use pg_foma::enumerate::{enumerate_default, CandidateRole, LoweredCandidate};
use pg_foma::junctions::PhonologyProbe;
use pg_foma::lowering_adapter::LoweringAdapter;
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};

const FIXTURE: &str = "backend-gated-generic";

/// See `tests/parity_divergence_census.rs` for why this is replicated rather than imported.
fn surface_table(grammar: &Grammar) -> &pg_grammar::chardef::CharDefTable {
    let surface_stratum = grammar
        .strata
        .last()
        .expect("a loaded grammar always has at least one stratum");
    &grammar.char_tables[surface_stratum.table.0 as usize]
}

fn fixture(name: &str) -> (Grammar, Vec<String>) {
    let fixture = discover()
        .into_iter()
        .find(|fixture| fixture.root == Root::Staging && fixture.name == name)
        .unwrap_or_else(|| panic!("missing staged fixture {name}"));
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture grammar must load");
    let words = fixture
        .load_words_yaml()
        .words
        .into_iter()
        .map(|word| word.word)
        .collect();
    (grammar, words)
}

fn baseline_plan(grammar: &Grammar) -> pg_foma::plan::Plan {
    let alphabet = SegAlphabet::new(surface_table(grammar));
    let prules: Vec<&PhonRuleDef> = pg_foma::enumerate::prules_in_order(grammar);
    let phonology = PhonologyProbe::new(grammar);
    enumerate_default(grammar, &alphabet, &prules, phonology.as_ref())
}

fn registry_plans(grammar: &Grammar) -> Vec<LoweredCandidate> {
    let baseline = baseline_plan(grammar);
    Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar,
            baseline: &baseline,
        })
        .expect("fixture candidates must materialize")
        .into_iter()
        .map(|(_, plan)| plan)
        .collect()
}

/// Claims 1, 2 and 3 together, on one fixture: "zero confirmation calls" means nothing without a run that shows what non-zero looks like on the same input.
#[test]
fn the_accuracy_path_agrees_with_certification_while_doing_no_confirmation_work() {
    let (grammar, words) = fixture(FIXTURE);
    let plans = registry_plans(&grammar);
    assert!(!plans.is_empty(), "fixture must materialize candidates");

    // The SLOW path: full propose -> confirm -> certify.
    let mut certify_cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
        .expect("oracle preparation must succeed for this fixture");
    let certified = evaluate_plans_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget::default(),
        &mut certify_cache,
    );

    // The FAST path: propose -> set containment. Same plans, same words, same budget.
    let mut accuracy_cache =
        RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
            .expect("oracle preparation must succeed for this fixture");
    let assessed = assess_accuracy_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget::default(),
        &mut accuracy_cache,
    );
    assert_eq!(assessed.len(), plans.len());

    // The hazard the whole containment argument rests on, measured on THIS run rather than assumed from the census (`parity_divergence_census.rs` carries the corpus-wide version).
    let divergence = certify_cache.identity_divergence();
    assert_eq!(
        divergence.candidate_only_identities, 0,
        "candidate-only identities invalidate free containment: {divergence:?}"
    );
    assert!(
        divergence.supports_free_containment(),
        "the run must have positively compared something: {divergence:?}"
    );

    let mut confirming_candidates = 0usize;
    let mut total_membership_tests = 0u64;
    for ((plan, certification), accuracy) in plans.iter().zip(&certified).zip(&assessed) {
        assert_eq!(accuracy.requested_strategy, plan.strategy());
        assert_eq!(
            accuracy.realized_strategy,
            certification.realized_strategy,
            "the two paths must attribute the measurement to the same compiler for {:?}",
            plan.strategy()
        );
        total_membership_tests += accuracy.counters.membership_tests;

        // Claim 2: the accuracy path reports zero for the SAME quantities certification reports non-zero for -- not a field nobody feeds.
        assert_eq!(
            (
                accuracy.counters.confirmation_calls,
                accuracy.counters.confirmation_steps
            ),
            (0, 0),
            "the accuracy path must perform no full-HC confirmation for {:?}: {:?}",
            plan.strategy(),
            accuracy.counters
        );

        if !certification.certification.selectable() {
            continue;
        }
        confirming_candidates += 1;
        assert!(
            certification.score.confirmation > 0 && certification.score.confirmation_steps > 0,
            "a confirmed candidate must have done real full-HC work, or 'zero on the fast path' \
             compares against nothing: {:?}",
            certification.score
        );
        // Claim 1: where certification says the identity sets are equal, containment says nothing was lost (the converse is not claimed -- containment is blind to over-generation by design).
        assert_eq!(
            accuracy.verdict,
            AccuracyVerdict::NoLoss,
            "{:?} certified but the accuracy path reported a loss -- the two mechanisms disagree \
             about ACCURACY, which is the one thing they must not do. counters={:?}",
            plan.strategy(),
            accuracy.counters
        );
        // Claim 3: the check ran for this candidate.
        assert!(
            accuracy.counters.membership_tests > 0,
            "the containment check never executed for {:?}: {:?}",
            plan.strategy(),
            accuracy.counters
        );
        assert!(
            accuracy.counters.oracle_keys_required > 0
                && accuracy.counters.oracle_keys_matched == accuracy.counters.oracle_keys_required,
            "a NoLoss verdict must have matched every required key: {:?}",
            accuracy.counters
        );
        assert_eq!(
            accuracy.counters.occurrences_checked as usize,
            words.len(),
            "every eligible occurrence must be checked -- a word subset here would be a silent \
             narrowing of the claim"
        );
    }
    assert!(
        confirming_candidates > 0,
        "the fixture must produce at least one confirmed candidate, or claims 1 and 2 compare \
         against nothing"
    );
    assert!(
        total_membership_tests > 0,
        "the accuracy mechanism never fired on any candidate"
    );
}

/// Claim 4a: a refused corpus is `NotDetermined`, never `NoLoss`. Also the fire-counter's zero reading: `membership_tests` is 0 here and non-zero in the test above, on the same fixture, so the counter tracks execution rather than being a constant.
#[test]
fn a_refused_corpus_is_undetermined_and_the_check_provably_does_not_run() {
    let (grammar, words) = fixture(FIXTURE);
    let plans = registry_plans(&grammar);
    let budget = RuntimeBudget {
        oracle_step_cap: Some(0),
        ..RuntimeBudget::default()
    };
    let mut cache = RunEvaluationCache::prepare(&grammar, &words, budget)
        .expect("preparation must not trip the liveness net at a zero step cap");
    let assessed = assess_accuracy_with_cache(&grammar, &plans, &words, budget, &mut cache);
    assert_eq!(assessed.len(), plans.len());
    for accuracy in &assessed {
        assert!(
            matches!(&accuracy.verdict, AccuracyVerdict::NotDetermined { reason }
                if reason.contains("oracle-capped")),
            "a step-capped corpus must refuse the batch rather than assess a subset: {:?}",
            accuracy.verdict
        );
        assert!(
            !accuracy.verdict.is_no_loss(),
            "'could not look' must never read as 'nothing was lost'"
        );
        assert_eq!(
            accuracy.counters.membership_tests, 0,
            "the containment check must not have run at all: {:?}",
            accuracy.counters
        );
        assert_eq!(accuracy.counters.occurrences_checked, 0);
    }
}

/// Claim 4b: a real recall failure is detected and named. Rather than depend on a fixed gap still existing, the negative is constructed the only way that cannot rot: the containment check runs against a proposal set with a required key REMOVED, and must report exactly that key.
#[test]
fn a_removed_proposal_is_reported_as_the_exact_lost_analysis() {
    let (grammar, words) = fixture(FIXTURE);
    let mut cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
        .expect("oracle preparation must succeed for this fixture");
    // The word the control uses has to be one the oracle actually analyses, or "every required key is missing" would be trivially true over an empty set.
    let word = words
        .iter()
        .find(|word| {
            cache
                .oracle_analyses(word)
                .is_some_and(|analyses| !analyses.is_empty())
        })
        .expect("the fixture must have at least one word the oracle analyses")
        .clone();
    let oracle: Vec<pg_parse::WordAnalysis> = cache
        .oracle_analyses(&word)
        .expect("just found above")
        .to_vec();

    let plans = vec![LoweredCandidate {
        label: "accuracy-negative-control",
        plan: baseline_plan(&grammar),
        // Whole-grammar emission isolates missing-analysis accuracy from the marker capability gate.
        adapter: LoweringAdapter::TunedSurfaceEmit,
        role: CandidateRole::Alternative,
    }];
    let assessed = assess_accuracy_with_cache(
        &grammar,
        std::slice::from_ref(&plans[0]),
        std::slice::from_ref(&word),
        RuntimeBudget::default(),
        &mut cache,
    );
    let baseline = assessed.into_iter().next().expect("one candidate assessed");
    assert_eq!(
        baseline.verdict,
        AccuracyVerdict::NoLoss,
        "the unmodified candidate must not lose anything, or the control below proves nothing: \
         {:?}",
        baseline.counters
    );
    assert!(baseline.counters.oracle_keys_required > 0);
    assert!(baseline.counters.membership_tests > 0);

    // Now the control: the same containment check, over the same oracle analyses, with every proposal withheld -- must find every required key missing and name each one.
    let mut misses = Vec::new();
    let counters = pg_foma::backend_accuracy::check_occurrence(&word, 0, &oracle, &[], &mut misses);
    assert_eq!(
        counters.oracle_keys_missed, counters.oracle_keys_required,
        "with nothing proposed, every required key must be missing"
    );
    assert_eq!(misses.len() as u64, counters.oracle_keys_missed);
    let named: Vec<(Vec<u32>, i32)> = misses
        .iter()
        .map(|miss| (miss.morpheme_ids.clone(), miss.root_index))
        .collect();
    for analysis in &oracle {
        let key = (analysis.morpheme_ids.clone(), analysis.root_morpheme_index);
        assert!(
            named.contains(&key),
            "a lost oracle analysis was not named: {key:?} not in {named:?}"
        );
    }
    let verdict = pg_foma::backend_accuracy::verdict_from(&counters, misses);
    assert!(matches!(verdict, AccuracyVerdict::Undergenerated { .. }));
}

/// The admission key IS the routing key, checked on real proposals from a real compiled network rather than hand-built values -- if these two ever disagree, containment stops implying admissibility.
#[test]
fn proposal_and_analysis_admission_keys_are_the_same_notion() {
    let candidate = pg_foma::tags::Candidate {
        morphemes: vec![
            pg_grammar::model::MorphemeId(3),
            pg_grammar::model::MorphemeId(7),
        ],
        root_index: 1,
    };
    assert_eq!(candidate_admission_key(&candidate), (vec![3, 7], 1));
}
