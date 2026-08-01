//! Cross-compiler equivalence gates for the independent recipe construction pipelines.
//!
//! The gate observes only the final candidate vector after `propose UNION peel`, never raw Foma
//! paths or the peeler's internal residual queries.  This keeps the evidence tied to the same
//! candidate identities that the confirmation stage actually receives.

use std::collections::BTreeMap;

use pg_conformance_fixtures::{discover, Root};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::recipe_registry::{
    MaterializerContext, Registry, FAMILY_COMPLETE_TEMPLATE, FAMILY_SURFACE_PROBE_MORPHOLOGY,
    FAMILY_TOKEN_CASCADE_MORPHOLOGY,
};
use pg_foma::recipe_runtime::{
    certify_corpus, check_proposal_ratio, evaluate_plans_marked_observed_with_cache,
    evaluate_plans_with_cache, RunEvaluationCache, RuntimeBudget, WordEvidence,
};
use pg_foma::replace::SegAlphabet;
use pg_foma::{enumerate::enumerate_default, junctions::PhonologyProbe};

const FIXTURE: &str = "template-category-sharing";
const MAX_PROPOSAL_RATIO: u64 = 2;
const REQUIRED_STRATEGIES: [EmissionStrategy; 3] = [
    EmissionStrategy::PlanComposed,
    EmissionStrategy::TunedSurfaceProbed,
    EmissionStrategy::TemplatedUnderlyingTokens,
];

fn fixture() -> (pg_grammar::model::Grammar, Vec<String>) {
    let fixture = discover()
        .into_iter()
        .find(|fixture| fixture.root == Root::Staging && fixture.name == FIXTURE)
        .unwrap_or_else(|| panic!("missing pinned synthetic fixture {FIXTURE}"));
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");
    assert!(
        !grammar.templates.is_empty(),
        "the equivalence gate must use a genuinely template-bearing fixture"
    );
    let pinned_words = fixture
        .load_words_yaml()
        .words
        .into_iter()
        .map(|word| word.word)
        .collect::<Vec<_>>();
    let words = ["kolo", "pakolosa", "takolola", "pakolola", "mbili", "mbili"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert!(
        words.iter().all(|word| pinned_words.contains(word)),
        "the gate words must come from the pinned fixture: {pinned_words:?}"
    );
    assert!(
        pinned_words.iter().any(|word| word == "pakolosa")
            && pinned_words.iter().any(|word| word == "takolola"),
        "the fixture must retain both template-derived positive words"
    );
    assert!(
        pinned_words.iter().any(|word| word == "pakolola"),
        "the fixture must retain the invalid cross-template negative"
    );
    assert_eq!(
        words.iter().filter(|word| word.as_str() == "mbili").count(),
        2,
        "the gate must retain duplicate corpus occurrences"
    );
    (grammar, words)
}

fn selected_plans(grammar: &pg_grammar::model::Grammar) -> Vec<pg_foma::enumerate::CandidatePlan> {
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let prules = grammar
        .strata
        .iter()
        .flat_map(|stratum| {
            stratum
                .prules
                .iter()
                .map(|id| &grammar.prules[id.0 as usize])
        })
        .collect::<Vec<_>>();
    let baseline = enumerate_default(
        grammar,
        &alphabet,
        &prules,
        PhonologyProbe::new(grammar).as_ref(),
    );
    let candidates = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar,
            baseline: &baseline,
        })
        .expect("pinned candidates must materialize");
    let mut selected = [false; REQUIRED_STRATEGIES.len()];
    let plans = candidates
        .into_iter()
        .map(|(_, plan)| plan)
        .filter(|plan| {
            REQUIRED_STRATEGIES
                .iter()
                .position(|strategy| *strategy == plan.strategy)
                .is_some_and(|index| {
                    if selected[index] {
                        false
                    } else {
                        selected[index] = true;
                        true
                    }
                })
        })
        .collect::<Vec<_>>();
    for strategy in REQUIRED_STRATEGIES {
        assert!(
            plans.iter().any(|plan| plan.strategy == strategy),
            "requested strategy was not materialized: {strategy:?}"
        );
    }
    assert_eq!(
        plans.len(),
        REQUIRED_STRATEGIES.len(),
        "the gate must exercise exactly one candidate per pipeline"
    );
    for (family, strategy) in [
        (FAMILY_COMPLETE_TEMPLATE, EmissionStrategy::PlanComposed),
        (
            FAMILY_SURFACE_PROBE_MORPHOLOGY,
            EmissionStrategy::TunedSurfaceProbed,
        ),
        (
            FAMILY_TOKEN_CASCADE_MORPHOLOGY,
            EmissionStrategy::TemplatedUnderlyingTokens,
        ),
    ] {
        assert!(
            plans
                .iter()
                .any(|plan| plan.label == family && plan.strategy == strategy),
            "the gate must pin family {family:?} to strategy {strategy:?}"
        );
    }
    plans
}

fn candidate_key(candidate: &pg_foma::tags::Candidate) -> (Vec<u32>, i32) {
    (
        candidate
            .morphemes
            .iter()
            .map(|morpheme| morpheme.0)
            .collect(),
        candidate.root_index,
    )
}

fn analysis_key(analysis: &pg_parse::WordAnalysis) -> (Vec<u32>, i32) {
    (analysis.morpheme_ids.clone(), analysis.root_morpheme_index)
}

fn assert_proposal_containment(strategy: EmissionStrategy, evidence: &[WordEvidence]) {
    for word in evidence {
        let mut proposed = BTreeMap::<(Vec<u32>, i32), usize>::new();
        for candidate in &word.proposals {
            *proposed.entry(candidate_key(candidate)).or_default() += 1;
        }
        assert_eq!(
            proposed.len(),
            word.proposals.len(),
            "{strategy:?} evidence for {:?} contains duplicate final candidate identities",
            word.word
        );

        let mut oracle = BTreeMap::<(Vec<u32>, i32), usize>::new();
        for analysis in &word.expected {
            *oracle.entry(analysis_key(analysis)).or_default() += 1;
        }
        for (identity, required) in oracle {
            let offered = proposed.get(&identity).copied().unwrap_or_default();
            assert!(
                offered >= required,
                "{strategy:?} failed proposal containment for {:?}: required multiplicity {required}, offered {offered}; proposals={:?}",
                identity,
                word.proposals
            );
        }
    }
}

fn deterministic_score(score: pg_foma::recipe_optimizer::Score) -> (u64, u64, u64, u64, u64, u64) {
    (
        score.states,
        score.arcs,
        score.proposals,
        score.confirmation,
        score.confirmation_steps,
        score.raw_paths,
    )
}

fn expected_multiset(evidence: &[WordEvidence]) -> Vec<(String, Vec<pg_parse::WordAnalysis>)> {
    evidence
        .iter()
        .map(|word| (word.word.clone(), word.expected.clone()))
        .collect()
}

fn actual_multiset(evidence: &[WordEvidence]) -> Vec<(String, Vec<pg_parse::WordAnalysis>)> {
    evidence
        .iter()
        .map(|word| (word.word.clone(), word.actual.clone()))
        .collect()
}

#[test]
fn pinned_three_pipeline_equivalence_observes_final_candidates_and_preserves_cache_semantics() {
    let (grammar, words) = fixture();
    let plans = selected_plans(&grammar);
    let is_baseline = plans
        .iter()
        .map(|plan| plan.strategy == EmissionStrategy::PlanComposed)
        .collect::<Vec<_>>();

    let mut ordinary_cache =
        RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default());
    let ordinary = evaluate_plans_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget::default(),
        &mut ordinary_cache,
    );
    let mut observed_cache =
        RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default());
    let observed = evaluate_plans_marked_observed_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget::default(),
        &is_baseline,
        &mut observed_cache,
    );

    assert_eq!(ordinary.len(), plans.len());
    assert_eq!(observed.len(), plans.len());
    assert_eq!(
        ordinary_cache.oracle_calls(),
        words.len(),
        "oracle must run once per corpus occurrence"
    );
    assert_eq!(
        observed_cache.oracle_calls(),
        words.len(),
        "observed evaluation must reuse one oracle result per corpus occurrence"
    );
    assert_eq!(ordinary_cache.oracle_calls(), observed_cache.oracle_calls());
    assert_eq!(
        ordinary_cache.emission_report_calls(),
        observed_cache.emission_report_calls()
    );
    assert_eq!(ordinary_cache.emission_report_calls(), 1);

    let mut oracle: Option<Vec<(String, Vec<pg_parse::WordAnalysis>)>> = None;
    let mut actual_by_strategy: Vec<(
        EmissionStrategy,
        Vec<(String, Vec<pg_parse::WordAnalysis>)>,
    )> = Vec::new();
    for ((plan, ordinary), observation) in plans.iter().zip(&ordinary).zip(&observed) {
        assert_eq!(observation.requested_strategy, plan.strategy);
        assert_eq!(observation.evaluation.certification, ordinary.certification);
        assert_eq!(
            deterministic_score(observation.evaluation.score),
            deterministic_score(ordinary.score),
            "observed evidence must not alter deterministic score fields for {:?}",
            plan.strategy
        );
        assert_eq!(
            observation.evaluation.realized_strategy, plan.strategy,
            "the observation must identify the strategy that actually ran"
        );
        assert!(
            observation.evaluation.certification.selectable(),
            "{strategy:?} did not reach full-HC confirmation: {:?}",
            observation.evaluation.certification,
            strategy = plan.strategy
        );

        let evidence = observation
            .words
            .as_ref()
            .expect("successful observation evidence");
        assert_eq!(
            evidence
                .iter()
                .map(|word| word.word.as_str())
                .collect::<Vec<_>>(),
            words.iter().map(String::as_str).collect::<Vec<_>>(),
            "duplicate corpus occurrences and order must be preserved"
        );
        assert_proposal_containment(plan.strategy, evidence);
        assert_eq!(
            evidence
                .iter()
                .map(|word| word.proposals.len() as u64)
                .sum::<u64>(),
            observation.evaluation.score.proposals,
            "summed final-evidence candidates must equal score.proposals"
        );
        assert!(
            evidence.iter().any(|word| !word.proposals.is_empty()),
            "{strategy:?} retained no final candidate evidence",
            strategy = plan.strategy
        );
        assert!(
            evidence.iter().map(|word| word.actual.len()).sum::<usize>() > 0,
            "{strategy:?} confirmed no analyses",
            strategy = plan.strategy
        );
        let invalid = evidence
            .iter()
            .find(|word| word.word == "pakolola")
            .expect("fixture must retain the cross-template negative");
        assert!(
            invalid.expected.is_empty() && invalid.actual.is_empty(),
            "{strategy:?} changed the invalid cross-template negative: {invalid:?}",
            strategy = plan.strategy
        );

        let oracle_confirmed = evidence
            .iter()
            .map(|word| word.expected.len() as u64)
            .sum::<u64>();
        assert!(
            check_proposal_ratio(
                observation.evaluation.realized_strategy,
                observation.evaluation.score.proposals,
                oracle_confirmed,
                MAX_PROPOSAL_RATIO,
            )
            .is_ok(),
            "{strategy:?} exceeded proposal ratio tripwire",
            strategy = plan.strategy
        );
        oracle.get_or_insert_with(|| expected_multiset(evidence));
        actual_by_strategy.push((plan.strategy, actual_multiset(evidence)));
    }

    let oracle = oracle.expect("the gate must observe one oracle evidence vector");
    for (strategy, actual) in actual_by_strategy {
        assert!(
            certify_corpus(&oracle, &actual).selectable(),
            "{strategy:?} confirmed multiset differs from the oracle"
        );
    }
}

#[test]
fn observed_evidence_distinguishes_failed_evaluation_from_real_empty_observation() {
    let (grammar, words) = fixture();
    let plans = selected_plans(&grammar);
    let baseline = vec![true; 1];
    let plan = plans
        .into_iter()
        .find(|plan| plan.strategy == EmissionStrategy::PlanComposed)
        .expect("plan-composed fixture candidate");

    let capped_words = vec![words[0].clone()];
    let mut capped_cache = RunEvaluationCache::prepare(
        &grammar,
        &capped_words,
        RuntimeBudget {
            oracle_step_cap: Some(0),
            ..RuntimeBudget::default()
        },
    );
    let failed = evaluate_plans_marked_observed_with_cache(
        &grammar,
        std::slice::from_ref(&plan),
        &capped_words,
        RuntimeBudget {
            oracle_step_cap: Some(0),
            ..RuntimeBudget::default()
        },
        &baseline,
        &mut capped_cache,
    )
    .pop()
    .expect("one failed observation");
    assert!(
        failed.words.is_none(),
        "failure must not masquerade as empty evidence"
    );

    let mut empty_cache = RunEvaluationCache::prepare(&grammar, &[], RuntimeBudget::default());
    let empty = evaluate_plans_marked_observed_with_cache(
        &grammar,
        &[plan],
        &[],
        RuntimeBudget::default(),
        &baseline,
        &mut empty_cache,
    )
    .pop()
    .expect("one empty observation");
    assert_eq!(empty.words, Some(Vec::new()));
}

#[test]
fn fault_injected_template_flattened_uflexc_route_reports_typed_proposal_ratio_violation() {
    let (grammar, words) = fixture();
    let plans = selected_plans(&grammar);
    let plan = plans
        .into_iter()
        .find(|plan| plan.strategy == EmissionStrategy::PlanComposed)
        .expect("plan-composed/uflexc candidate");
    let mut cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default());
    let observation = evaluate_plans_marked_observed_with_cache(
        &grammar,
        std::slice::from_ref(&plan),
        &words,
        RuntimeBudget::default(),
        &[true],
        &mut cache,
    )
    .pop()
    .expect("uflexc observation");
    let evidence = observation
        .words
        .expect("uflexc route must produce evidence");
    let denominator = evidence
        .iter()
        .map(|word| word.expected.len() as u64)
        .sum::<u64>();
    assert!(
        denominator > 0,
        "fault injection must start from real oracle analyses"
    );

    // Fault-inject the known bad template-flattened route by replacing its final candidate count
    // with one just beyond K times the real oracle denominator. This proves the production ratio
    // checker emits a typed diagnostic rather than merely asserting that today's routes are below K.
    let numerator = denominator
        .saturating_mul(MAX_PROPOSAL_RATIO)
        .saturating_add(1);
    let violation = check_proposal_ratio(
        EmissionStrategy::PlanComposed,
        numerator,
        denominator,
        MAX_PROPOSAL_RATIO,
    )
    .expect_err("the flattened-route fault injection must violate the proposal ratio");
    assert_eq!(violation.strategy, EmissionStrategy::PlanComposed);
    assert_eq!(violation.numerator, numerator);
    assert_eq!(violation.denominator, denominator);
    assert_eq!(violation.threshold, MAX_PROPOSAL_RATIO);
    assert!(violation.to_string().contains("numerator="));
    assert!(violation.to_string().contains("denominator="));
    assert!(violation.to_string().contains("threshold="));
    assert!(violation.to_string().contains("strategy=PlanComposed"));
}
