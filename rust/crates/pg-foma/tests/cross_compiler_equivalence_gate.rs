//! Cross-compiler equivalence gates: observes only the final candidate vector after `propose UNION peel`, never raw Foma paths, keeping evidence tied to what confirmation actually receives.

use std::collections::{BTreeMap, BTreeSet};

use pg_conformance_fixtures::{discover, Root};
use pg_foma::backend_registry::{
    MaterializerContext, Registry, FAMILY_COMPLETE_TEMPLATE, FAMILY_SURFACE_PROBE_MORPHOLOGY,
    FAMILY_TOKEN_CASCADE_MORPHOLOGY,
};
use pg_foma::backend_runtime::{
    certify_corpus, check_proposal_ratio, evaluate_plans_observed_with_cache,
    evaluate_plans_with_cache, word_proposal_containment, RunEvaluationCache, RuntimeBudget,
    WordEvidence,
};
use pg_foma::enumerate::{CandidateRole, EmissionStrategy, LoweredCandidate};
use pg_foma::lowering_adapter::LoweringAdapter;
use pg_foma::{enumerate::enumerate_default, junctions::PhonologyProbe};

const FIXTURE: &str = "template-category-sharing";
const MAX_PROPOSAL_RATIO: u64 = 2;
const REQUIRED_STRATEGIES: [EmissionStrategy; 3] = [
    EmissionStrategy::PlanComposed,
    EmissionStrategy::TunedSurfaceProbed,
    EmissionStrategy::TemplatedUnderlyingTokens,
];

/// All three candidates share one baseline `Plan`; only the plan-composing strategy reads it, so it alone counts as `Baseline`.
fn baseline_role(strategy: EmissionStrategy) -> CandidateRole {
    if strategy == EmissionStrategy::PlanComposed {
        CandidateRole::Baseline
    } else {
        CandidateRole::Alternative
    }
}

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

fn selected_plans(
    grammar: &pg_grammar::model::Grammar,
) -> Vec<pg_foma::enumerate::LoweredCandidate> {
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
                .position(|strategy| *strategy == plan.strategy())
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
            plans.iter().any(|plan| plan.strategy() == strategy),
            "requested strategy was not materialized: {strategy:?}"
        );
    }
    assert_eq!(
        plans.len(),
        REQUIRED_STRATEGIES.len(),
        "the gate must exercise exactly one candidate per pipeline"
    );
    assert_eq!(
        plans.iter().map(|plan| plan.strategy()).collect::<Vec<_>>(),
        REQUIRED_STRATEGIES,
        "the gate must pin plan order before deriving baseline flags"
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
                .any(|plan| plan.label == family && plan.strategy() == strategy),
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

/// Identity comparison delegates to `word_proposal_containment`; the duplicate-identity check below is this evaluator's own dedup invariant, checked here only.
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
        if let Err(gap) = word_proposal_containment(word) {
            panic!(
                "{strategy:?} failed proposal containment: {gap}; proposals={:?}",
                word.proposals
            );
        }
    }
}

fn deterministic_score(score: pg_foma::backend_optimizer::Score) -> (u64, u64, u64, u64, u64, u64) {
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

fn assert_proposal_ratio(strategy: EmissionStrategy, numerator: u64, denominator: u64) {
    check_proposal_ratio(strategy, numerator, denominator, MAX_PROPOSAL_RATIO)
        .unwrap_or_else(|violation| panic!("{violation}"));
}

#[test]
fn pinned_three_pipeline_equivalence_observes_final_candidates_and_preserves_cache_semantics() {
    let (grammar, words) = fixture();
    let plans = selected_plans(&grammar);
    let mut ordinary_cache =
        RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
            .expect("oracle preparation must succeed for this fixture");
    let ordinary = evaluate_plans_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget::default(),
        &mut ordinary_cache,
    );
    let mut observed_cache =
        RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
            .expect("oracle preparation must succeed for this fixture");
    let observed = evaluate_plans_observed_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget::default(),
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

    let mut oracle: Option<Vec<(String, Vec<pg_parse::WordAnalysis>)>> = None;
    let mut actual_by_strategy: Vec<(
        EmissionStrategy,
        Vec<(String, Vec<pg_parse::WordAnalysis>)>,
    )> = Vec::new();
    for ((plan, ordinary), observation) in plans.iter().zip(&ordinary).zip(&observed) {
        assert_eq!(observation.requested_strategy, plan.strategy());
        assert_eq!(observation.evaluation.certification, ordinary.certification);
        assert_eq!(
            deterministic_score(observation.evaluation.score),
            deterministic_score(ordinary.score),
            "observed evidence must not alter deterministic score fields for {:?}",
            plan.strategy()
        );
        assert_eq!(
            observation.evaluation.realized_strategy,
            plan.strategy(),
            "the observation must identify the strategy that actually ran"
        );
        assert!(
            observation.evaluation.certification.selectable(),
            "{strategy:?} did not reach full-HC confirmation: {:?}",
            observation.evaluation.certification,
            strategy = plan.strategy()
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
        assert_proposal_containment(plan.strategy(), evidence);
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
            strategy = plan.strategy()
        );
        for positive in ["pakolosa", "takolola"] {
            let positive = evidence
                .iter()
                .find(|word| word.word == positive)
                .unwrap_or_else(|| {
                    panic!("{:?} omitted positive word {positive}", plan.strategy())
                });
            assert!(
                !positive.expected.is_empty(),
                "{strategy:?} positive {word:?} has no oracle analyses",
                strategy = plan.strategy(),
                word = positive.word
            );
            assert!(
                !positive.actual.is_empty(),
                "{strategy:?} positive {word:?} has no confirmed analyses",
                strategy = plan.strategy(),
                word = positive.word
            );
        }
        assert!(
            evidence.iter().map(|word| word.actual.len()).sum::<usize>() > 0,
            "{strategy:?} confirmed no analyses",
            strategy = plan.strategy()
        );
        let invalid = evidence
            .iter()
            .find(|word| word.word == "pakolola")
            .expect("fixture must retain the cross-template negative");
        assert!(
            invalid.expected.is_empty() && invalid.actual.is_empty(),
            "{strategy:?} changed the invalid cross-template negative: {invalid:?}",
            strategy = plan.strategy()
        );

        let oracle_confirmed = evidence
            .iter()
            .map(|word| word.expected.len() as u64)
            .sum::<u64>();
        assert_proposal_ratio(
            observation.evaluation.realized_strategy,
            observation.evaluation.score.proposals,
            oracle_confirmed,
        );
        oracle.get_or_insert_with(|| expected_multiset(evidence));
        actual_by_strategy.push((plan.strategy(), actual_multiset(evidence)));
    }

    let oracle = oracle.expect("the gate must observe one oracle evidence vector");
    for (strategy, actual) in actual_by_strategy {
        assert!(
            certify_corpus(&grammar, &oracle, &actual).selectable(),
            "{strategy:?} confirmed identity set differs from the oracle"
        );
    }
}

#[test]
fn observed_evidence_distinguishes_failed_evaluation_from_real_empty_observation() {
    let (grammar, words) = fixture();
    let plans = selected_plans(&grammar);
    let plan = plans
        .into_iter()
        .find(|plan| plan.strategy() == EmissionStrategy::PlanComposed)
        .expect("plan-composed fixture candidate");

    let capped_words = vec![words[0].clone()];
    let mut capped_cache = RunEvaluationCache::prepare(
        &grammar,
        &capped_words,
        RuntimeBudget {
            oracle_step_cap: Some(0),
            ..RuntimeBudget::default()
        },
    )
    .expect("oracle preparation must succeed for this fixture");
    let failed = evaluate_plans_observed_with_cache(
        &grammar,
        std::slice::from_ref(&plan),
        &capped_words,
        RuntimeBudget {
            oracle_step_cap: Some(0),
            ..RuntimeBudget::default()
        },
        &mut capped_cache,
    )
    .pop()
    .expect("one failed observation");
    assert!(
        failed.words.is_none(),
        "failure must not masquerade as empty evidence"
    );
    assert_eq!(
        failed.evaluation.realized_strategy,
        EmissionStrategy::PlanComposed
    );
    assert!(matches!(
        failed.evaluation.certification,
        pg_foma::backend_optimizer::Certification::Truncated { ref stage, .. }
            if stage == "oracle-capped"
    ));
    assert_eq!(failed.evaluation.score.proposals, 0);

    let mut empty_cache = RunEvaluationCache::prepare(&grammar, &[], RuntimeBudget::default())
        .expect("oracle preparation must succeed for this fixture");
    let empty = evaluate_plans_observed_with_cache(
        &grammar,
        &[plan],
        &[],
        RuntimeBudget::default(),
        &mut empty_cache,
    )
    .pop()
    .expect("one empty observation");
    assert_eq!(empty.words, Some(Vec::new()));
}

/// Deliberately raises the apply-path budget above the default (which would otherwise refuse this fixture outright) so the resulting proposal-ratio violation can be observed directly, bounded rather than unbounded.
#[test]
fn template_flattened_uflexc_route_reports_typed_proposal_ratio_violation() {
    // Just above the measured 12^6 for this fixture's 6-x word. See this test's own doc.
    let budget = RuntimeBudget {
        apply_path_budget: Some(3_000_000),
        apply_candidate_budget: Some(3_000_000),
        ..RuntimeBudget::default()
    };
    let fixture = discover()
        .into_iter()
        .find(|fixture| fixture.root == Root::Staging && fixture.name == "backend-template-generic")
        .expect("missing pinned synthetic fixture backend-template-generic");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");
    assert!(!grammar.templates.is_empty());
    let pinned_words = fixture
        .load_words_yaml()
        .words
        .into_iter()
        .map(|word| word.word)
        .collect::<Vec<_>>();
    let words = ["k", "xxxxxxk"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert!(
        words.iter().all(|word| pinned_words.contains(word)),
        "the known-bad route words must come from the pinned fixture: {pinned_words:?}"
    );

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
        &grammar,
        &prules,
        PhonologyProbe::new(&grammar).as_ref(),
    );
    let plan = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar: &grammar,
            baseline: &baseline,
        })
        .expect("pinned candidates must materialize")
        .into_iter()
        .map(|(_, plan)| plan)
        .find(|plan| {
            plan.label == FAMILY_COMPLETE_TEMPLATE
                && plan.strategy() == EmissionStrategy::PlanComposed
        })
        .expect("plan-composed/uflexc candidate");

    let mut cache = RunEvaluationCache::prepare(&grammar, &words, budget)
        .expect("oracle preparation must succeed for this fixture");
    let observation = evaluate_plans_observed_with_cache(
        &grammar,
        std::slice::from_ref(&plan),
        &words,
        budget,
        &mut cache,
    )
    .pop()
    .expect("uflexc observation");
    let evidence = observation.words.unwrap_or_else(|| {
        panic!(
            "uflexc route must produce final-candidate evidence at the raised envelope this test \
             declares (see its own doc); got {:?}",
            observation.evaluation.certification
        )
    });
    assert_eq!(
        observation.evaluation.realized_strategy,
        EmissionStrategy::PlanComposed,
        "the known-bad case must exercise the flattened uflexc route, not a fallback compiler"
    );
    assert!(
        observation.evaluation.certification.selectable(),
        "the known-bad route must complete oracle confirmation before its proposal ratio is checked: {:?}",
        observation.evaluation.certification
    );

    let denominator = evidence
        .iter()
        .map(|word| word.expected.len() as u64)
        .sum::<u64>();
    assert!(
        denominator > 0,
        "known-bad route must start from real oracle analyses"
    );
    let numerator = evidence
        .iter()
        .map(|word| word.proposals.len() as u64)
        .sum::<u64>();
    assert_eq!(numerator, observation.evaluation.score.proposals);
    let violation = check_proposal_ratio(
        observation.evaluation.realized_strategy,
        numerator,
        denominator,
        MAX_PROPOSAL_RATIO,
    )
    .expect_err("the flattened-route evidence must violate the proposal ratio");
    assert_eq!(violation.strategy, observation.evaluation.realized_strategy);
    assert_eq!(violation.numerator, numerator);
    assert_eq!(violation.denominator, denominator);
    assert_eq!(violation.threshold, MAX_PROPOSAL_RATIO);
    assert!(violation.to_string().contains("numerator="));
    assert!(violation.to_string().contains("denominator="));
    assert!(violation.to_string().contains("threshold="));
    assert!(violation.to_string().contains("strategy=PlanComposed"));
}

#[test]
#[should_panic(expected = "strategy=PlanComposed numerator=7 denominator=2 threshold=2")]
fn three_pipeline_gate_reports_proposal_ratio_violation_details() {
    assert_proposal_ratio(EmissionStrategy::PlanComposed, 7, 2);
}

/// Builds each `LoweredCandidate` directly instead of through `Registry`: `Applicability::HasPhonologyOrTemplates` gates only what the optimizer auto-proposes, not what a compiler can legally build, and this template-less fixture would never trigger it.
#[test]
fn plan_composed_cannot_represent_compounding_construct_red1() {
    let fixture = discover()
        .into_iter()
        .find(|fixture| {
            fixture.root == Root::Staging && fixture.name == "compounding-non-recursive"
        })
        .expect("missing pinned synthetic fixture compounding-non-recursive");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");
    let pinned_words = fixture
        .load_words_yaml()
        .words
        .into_iter()
        .map(|word| word.word)
        .collect::<Vec<_>>();
    let word = "fasubel".to_owned();
    assert!(
        pinned_words.contains(&word),
        "the two-stem compound word must come from the pinned fixture: {pinned_words:?}"
    );
    let words = vec![word];

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
    let baseline_plan = enumerate_default(
        &grammar,
        &prules,
        PhonologyProbe::new(&grammar).as_ref(),
    );

    let plans: Vec<LoweredCandidate> = REQUIRED_STRATEGIES
        .iter()
        .map(|&strategy| LoweredCandidate {
            label: "red1-compounding-cross-compiler",
            plan: baseline_plan.clone(),
            adapter: LoweringAdapter::for_strategy(strategy),
            // The plan-composing candidate carries the grammar's default plan and so is baseline; the two whole-grammar adapters never read a plan, so their role is never consulted.
            role: baseline_role(strategy),
        })
        .collect();
    let mut cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
        .expect("oracle preparation must succeed for this fixture");
    let observed = evaluate_plans_observed_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget::default(),
        &mut cache,
    );
    assert_eq!(observed.len(), plans.len());

    let mut oracle: Option<Vec<(String, Vec<pg_parse::WordAnalysis>)>> = None;
    for (plan, observation) in plans.iter().zip(&observed) {
        assert_eq!(observation.requested_strategy, plan.strategy());
        let evidence = observation.words.as_ref().unwrap_or_else(|| {
            panic!(
                "{:?} evaluation failed outright: {:?}",
                plan.strategy(),
                observation.evaluation
            )
        });
        let oracle = oracle.get_or_insert_with(|| expected_multiset(evidence));
        assert!(
            oracle
                .iter()
                .any(|(word, analyses)| word == "fasubel" && !analyses.is_empty()),
            "the fixture's oracle must find at least one analysis for the two-stem compound fasubel"
        );
        let actual = actual_multiset(evidence);
        let certification = certify_corpus(&grammar, oracle.as_slice(), &actual);
        assert!(
            certification.selectable(),
            "{:?} produced a final candidate set for the compounding fixture's two-stem word that \
             differs from the oracle -- {:?}",
            plan.strategy(),
            certification
        );
    }
}

/// Distinguishes headedness readings that share an identical flat `morphs|surface` signature: compares deduplicated `AnalysisIdentity` sets (root-index aware) instead, since a signature-only diff cannot tell the two readings apart.
#[test]
fn plan_composed_distinguishes_headedness_ambiguity_red2() {
    let fixture = discover()
        .into_iter()
        .find(|fixture| {
            fixture.root == Root::Staging && fixture.name == "head-ambiguous-compounding"
        })
        .expect("missing pinned synthetic fixture head-ambiguous-compounding");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");
    let pinned_words = fixture
        .load_words_yaml()
        .words
        .into_iter()
        .map(|word| word.word)
        .collect::<Vec<_>>();
    let word = "dakimo".to_owned();
    assert!(
        pinned_words.contains(&word),
        "the headedness-ambiguity word must come from the pinned fixture: {pinned_words:?}"
    );
    let words = vec![word];

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
    let baseline_plan = enumerate_default(
        &grammar,
        &prules,
        PhonologyProbe::new(&grammar).as_ref(),
    );

    let plans: Vec<LoweredCandidate> = REQUIRED_STRATEGIES
        .iter()
        .map(|&strategy| LoweredCandidate {
            label: "red2-head-ambiguous-compounding",
            plan: baseline_plan.clone(),
            adapter: LoweringAdapter::for_strategy(strategy),
            role: baseline_role(strategy),
        })
        .collect();
    let mut cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
        .expect("oracle preparation must succeed for this fixture");
    let observed = evaluate_plans_observed_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget::default(),
        &mut cache,
    );
    assert_eq!(observed.len(), plans.len());

    let mut oracle: Option<Vec<(String, Vec<pg_parse::WordAnalysis>)>> = None;
    for (plan, observation) in plans.iter().zip(&observed) {
        assert_eq!(observation.requested_strategy, plan.strategy());
        let evidence = observation.words.as_ref().unwrap_or_else(|| {
            panic!(
                "{:?} evaluation failed outright: {:?}",
                plan.strategy(),
                observation.evaluation
            )
        });
        let oracle = oracle.get_or_insert_with(|| expected_multiset(evidence));

        // Sanity: confirms the fixture's own oracle still finds both headedness readings before testing the strategy -- a failure here means the fixture regressed, not the compiler.
        let oracle_roots: BTreeSet<i32> = oracle
            .iter()
            .find(|(w, _)| w == "dakimo")
            .map(|(_, analyses)| analyses.iter().map(|a| a.root_morpheme_index).collect())
            .unwrap_or_default();
        assert_eq!(
            oracle_roots,
            BTreeSet::from([0, 1]),
            "the fixture's own oracle must retain both headedness readings for dakimo \
             (root_index 0 and 1); got {oracle_roots:?}"
        );

        let actual = actual_multiset(evidence);
        let certification = certify_corpus(&grammar, oracle.as_slice(), &actual);
        assert!(
            certification.selectable(),
            "{:?} produced a final candidate set for the headedness-ambiguity word that differs \
             from the oracle -- must retain BOTH readings (dak-headed root_index=0 AND imo-headed \
             root_index=1), not just one -- {:?}",
            plan.strategy(),
            certification
        );
    }
}
