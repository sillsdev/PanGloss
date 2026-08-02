//! Pins task 1.2's run-scoped evaluator caches.

use pg_conformance_fixtures::{discover, Root};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::recipe_optimizer::{pareto_frontier, select_confirmed, Certification};
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
    let uncached = evaluate_plans(&grammar, &plans, &words, RuntimeBudget::default())
        .expect("the oracle liveness net / memory ceiling must not trip on this fixture");
    let mut cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
        .expect("oracle preparation must succeed for this fixture");
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

    let mut whole_cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
        .expect("oracle preparation must succeed for this fixture");
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
        RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
            .expect("oracle preparation must succeed for this fixture");
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

#[test]
fn cache_prepared_for_fewer_different_occurrences_fails_closed_without_selection() {
    let (grammar, _) = fixture();
    let plans = plans(&grammar);
    let prepared = vec!["tulik".to_string()];
    let requested = vec!["tulik".to_string(), "menulik".to_string()];
    let mut cache = RunEvaluationCache::prepare(&grammar, &prepared, RuntimeBudget::default())
        .expect("oracle preparation must succeed for this fixture");

    let evaluations = evaluate_plans_with_cache(
        &grammar,
        &plans,
        &requested,
        RuntimeBudget::default(),
        &mut cache,
    );
    assert!(!evaluations.is_empty());

    let ranked = evaluations
        .iter()
        .enumerate()
        .map(|(index, evaluation)| {
            (
                format!("candidate-{index}"),
                evaluation.certification.clone(),
                evaluation.score,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(select_confirmed(&ranked), None);
    assert!(pareto_frontier(&ranked).is_empty());

    for evaluation in evaluations {
        let Certification::Truncated {
            ref stage,
            ref corpus,
        } = evaluation.certification
        else {
            panic!("missing requested occurrence must truncate: {evaluation:?}");
        };
        assert_eq!(stage, "corpus-incomplete");
        let corpus = corpus
            .as_ref()
            .expect("truncation must carry occurrence completeness evidence");
        assert_eq!(
            (corpus.requested, corpus.included, corpus.excluded),
            (2, 1, 1)
        );
        assert_eq!(corpus.exclusions.len(), 1);
        let exclusion = serde_json::to_value(&corpus.exclusions[0])
            .expect("exclusion evidence must be serializable");
        assert_eq!(exclusion["requested_ordinal"], 1);
        assert_eq!(exclusion["word"], "menulik");
        assert_eq!(exclusion["reason"], "corpus-row-not-prepared");
    }
}

#[test]
fn cache_excess_duplicate_occurrence_is_truncated_and_keeps_occurrences_distinct() {
    let (grammar, _) = fixture();
    let plans = plans(&grammar);
    let prepared = vec!["tulik".to_string()];
    let requested = vec!["tulik".to_string(), "tulik".to_string()];
    let mut cache = RunEvaluationCache::prepare(&grammar, &prepared, RuntimeBudget::default())
        .expect("oracle preparation must succeed for this fixture");

    let evaluations = evaluate_plans_with_cache(
        &grammar,
        &plans,
        &requested,
        RuntimeBudget::default(),
        &mut cache,
    );
    assert!(!evaluations.is_empty());

    let mut repeat_cache =
        RunEvaluationCache::prepare(&grammar, &prepared, RuntimeBudget::default())
            .expect("oracle preparation must succeed for this fixture");
    let repeated_evaluations = evaluate_plans_with_cache(
        &grammar,
        &plans,
        &requested,
        RuntimeBudget::default(),
        &mut repeat_cache,
    );
    assert_eq!(
        evaluations
            .iter()
            .map(|evaluation| &evaluation.certification)
            .collect::<Vec<_>>(),
        repeated_evaluations
            .iter()
            .map(|evaluation| &evaluation.certification)
            .collect::<Vec<_>>()
    );

    let ranked = evaluations
        .iter()
        .enumerate()
        .map(|(index, evaluation)| {
            (
                format!("candidate-{index}"),
                evaluation.certification.clone(),
                evaluation.score,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(select_confirmed(&ranked), None);
    assert!(pareto_frontier(&ranked).is_empty());

    for evaluation in evaluations {
        let Certification::Truncated { ref corpus, .. } = evaluation.certification else {
            panic!("an excess duplicate occurrence must truncate: {evaluation:?}");
        };
        let corpus = corpus
            .as_ref()
            .expect("truncation must carry duplicate occurrence evidence");
        assert_eq!(
            (corpus.requested, corpus.included, corpus.excluded),
            (2, 1, 1)
        );
        assert_ne!(corpus.requested_hash, corpus.included_hash);
        assert_ne!(corpus.excluded_hash, corpus.included_hash);
        assert_ne!(corpus.excluded_hash, corpus.requested_hash);
        assert_eq!(corpus.exclusions.len(), 1);
        let exclusion = serde_json::to_value(&corpus.exclusions[0])
            .expect("duplicate exclusion evidence must be serializable");
        assert_eq!(exclusion["requested_ordinal"], 1);
        assert_eq!(exclusion["word"], "tulik");
        assert_eq!(exclusion["reason"], "corpus-row-not-prepared");
    }
}

#[test]
fn unrelated_excluded_prepared_row_does_not_poison_requested_pilot_subset() {
    let (grammar, _) = fixture();
    let plans = plans(&grammar);
    let prepared = vec!["tulik".to_string(), "menulik".to_string()];
    let requested = vec!["tulik".to_string()];
    let mut cache = RunEvaluationCache::prepare(
        &grammar,
        &prepared,
        RuntimeBudget {
            oracle_step_cap: Some(5),
            ..RuntimeBudget::default()
        },
    )
    .expect("oracle preparation must succeed for this fixture");

    let evaluations = evaluate_plans_with_cache(
        &grammar,
        &plans,
        &requested,
        RuntimeBudget {
            oracle_step_cap: Some(5),
            ..RuntimeBudget::default()
        },
        &mut cache,
    );
    assert!(
        evaluations
            .iter()
            .any(|evaluation| evaluation.certification.selectable()),
        "a complete requested pilot occurrence must remain certifiable even when an unrelated prepared row is excluded: {evaluations:?}"
    );
    assert!(evaluations.iter().all(|evaluation| {
        !matches!(
            evaluation.certification,
            Certification::Truncated {
                corpus: Some(_),
                ..
            }
        )
    }));
}
