//! Pins task 1.2's run-scoped evaluator caches.

use pg_conformance_fixtures::{discover, Root};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::recipe_runtime::{
    evaluate_plans, evaluate_plans_with_cache, RunEvaluationCache, RuntimeBudget,
};
use pg_foma::replace::SegAlphabet;
use pg_foma::{enumerate::enumerate_default, junctions::PhonologyProbe};

fn fixture() -> (pg_grammar::model::Grammar, Vec<String>) {
    let fixture = discover()
        .into_iter()
        .find(|fixture| fixture.root == Root::Staging && fixture.name == "recipe-gated-generic")
        .expect("staged recipe-gated-generic fixture");
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

fn deterministic_score(
    score: pg_foma::recipe_optimizer::Score,
) -> (u64, u64, u64, u64, u64, u64, u64) {
    (
        score.states,
        score.arcs,
        score.proposals,
        score.confirmation,
        score.confirmation_steps,
        score.raw_paths,
        score.key("test").0,
    )
}

#[test]
fn cached_and_uncached_scores_and_winner_are_invariant() {
    let (grammar, words) = fixture();
    let plans = plans(&grammar);
    let uncached = evaluate_plans(&grammar, &plans, &words, RuntimeBudget::default());
    let mut cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default());
    let cached = evaluate_plans_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget::default(),
        &mut cache,
    );

    assert_eq!(
        uncached
            .iter()
            .map(|evaluation| (
                &evaluation.certification,
                deterministic_score(evaluation.score)
            ))
            .collect::<Vec<_>>(),
        cached
            .iter()
            .map(|evaluation| (
                &evaluation.certification,
                deterministic_score(evaluation.score)
            ))
            .collect::<Vec<_>>()
    );
    let winner = |evaluations: &[pg_foma::recipe_runtime::RuntimeEvaluation]| {
        evaluations
            .iter()
            .enumerate()
            .filter(|(_, evaluation)| evaluation.certification.selectable())
            .min_by_key(|(index, evaluation)| evaluation.score.key(&index.to_string()))
            .map(|(index, _)| index)
    };
    assert_eq!(winner(&uncached), winner(&cached));
}

#[test]
fn prepared_oracle_is_shared_and_emission_report_is_strategy_lazy() {
    let (grammar, words) = fixture();
    let all_plans = plans(&grammar);
    let (whole, composed): (Vec<_>, Vec<_>) = all_plans
        .into_iter()
        .partition(|plan| plan.strategy.is_whole_grammar());
    let composed = composed
        .into_iter()
        .filter(|plan| plan.strategy == EmissionStrategy::PlanComposed)
        .take(2)
        .collect::<Vec<_>>();
    assert!(
        !whole.is_empty(),
        "fixture must have a whole-grammar candidate"
    );
    assert!(
        composed.len() >= 2,
        "fixture must have two composed candidates"
    );

    let mut whole_cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default());
    evaluate_plans_with_cache(
        &grammar,
        &whole,
        &words,
        RuntimeBudget::default(),
        &mut whole_cache,
    );
    assert_eq!(whole_cache.oracle_calls(), words.len());
    assert_eq!(whole_cache.emission_report_calls(), 0);

    let mut composed_cache =
        RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default());
    evaluate_plans_with_cache(
        &grammar,
        &composed,
        &words,
        RuntimeBudget::default(),
        &mut composed_cache,
    );
    assert_eq!(composed_cache.oracle_calls(), words.len());
    assert_eq!(composed_cache.emission_report_calls(), 1);
}
