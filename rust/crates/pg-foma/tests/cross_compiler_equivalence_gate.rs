//! Cross-compiler equivalence gates for the independent recipe construction pipelines.
//!
//! The gate observes only the final candidate vector after `propose UNION peel`, never raw Foma
//! paths or the peeler's internal residual queries.  This keeps the evidence tied to the same
//! candidate identities that the confirmation stage actually receives.

use std::collections::{BTreeMap, BTreeSet};

use pg_conformance_fixtures::{discover, Root};
use pg_foma::enumerate::{CandidateRole, EmissionStrategy, LoweredCandidate};
use pg_foma::executable_candidate::LoweringAdapter;
use pg_foma::recipe_registry::{
    MaterializerContext, Registry, FAMILY_COMPLETE_TEMPLATE, FAMILY_SURFACE_PROBE_MORPHOLOGY,
    FAMILY_TOKEN_CASCADE_MORPHOLOGY,
};
use pg_foma::recipe_runtime::{
    certify_corpus, check_proposal_ratio, evaluate_plans_observed_with_cache,
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

/// The role this gate's hand-built candidates carry: all three candidates share ONE baseline
/// `Plan`, so the plan-COMPOSING one is that plan's own compilation and the two whole-grammar
/// adapters -- which never read a plan at all -- are not.
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
        pg_foma::recipe_optimizer::Certification::Truncated { ref stage, .. }
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

/// **This test DELIBERATELY opts out of the per-word apply envelope, and that is the point of it.**
///
/// Its whole subject is the magnitude of the flattened uflexc route's over-generation on this
/// fixture: it observes the proposal count, divides by the oracle's own analysis count, and requires
/// the ratio to be a violation. The default envelope
/// (`pg_foma::compose_budget::DEFAULT_EVALUATION_APPLY_PATH_BUDGET`, 1,000,000) exists to stop exactly
/// that magnitude from being enumerated — measured 2,985,984 = 12^6 raw paths for `xxxxxxk` — so under
/// the default this candidate is refused with a `ResourceBreach` and there is no proposal count to
/// observe at all. A test that measures an over-generation and a budget that refuses it are not in
/// conflict; they are the same finding at two seams.
///
/// So the envelope is raised HERE, explicitly, to 3,000,000 — just above the measured figure, chosen
/// over `Some(usize::MAX)` so this test stays bounded rather than trading one unbounded enumeration
/// for another. The corpus is `["k", "xxxxxxk"]` only; the fixture's third word (`xxxxxxxxxxxxk`,
/// 12^12 raw paths) is deliberately absent and no envelope should ever admit it.
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
        .find(|fixture| fixture.root == Root::Staging && fixture.name == "recipe-template-generic")
        .expect("missing pinned synthetic fixture recipe-template-generic");
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
        &grammar,
        &alphabet,
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

/// RED-1: pins the compounding recall gap `uflexc.rs`'s module doc used to name --
/// `EmissionStrategy::PlanComposed` uses `uflexc::emit_underlying_filtered_with_budget` as its ONLY
/// lexicon emitter (`build.rs`), and that emitter's continuation graph WAS structurally single-root
/// (no arc from at-or-after `RootBare` back to `RootBare`/`PrefixOrRoot`), so it could never propose
/// a compound no matter what a `MorphRuleDef::Compounding` rule said. `uflexc`'s bounded compound
/// loop (that module's own "Bounded compound loop" section) closes it; this test was written BEFORE
/// that fix, deliberately un-shapeable by it, and is now un-`#[ignore]`d.
///
/// Runs the already-staged `conformance-staging/edge-cases/compounding-non-recursive` fixture's
/// two-stem positive witness `fasubel` (headA `fasu` + nonHeadOk `bel`, via the grammar's single
/// `CompoundingRuleDef` `cr1`) through all three `REQUIRED_STRATEGIES` and asserts each one's final,
/// oracle-certified candidate set is the SAME. Deliberately does not go through
/// `Registry::seeded()`/`recipe_registry::Applicability` at all: this fixture declares no
/// phonological rules and no templates, so `FAMILY_TOKEN_CASCADE_MORPHOLOGY`'s
/// `Applicability::HasPhonologyOrTemplates` gate would never offer `TemplatedUnderlyingTokens` for
/// it (that gate only controls which candidates the OPTIMIZER auto-proposes, not what a compiler can
/// legally be asked to build) -- so each `LoweredCandidate` here is built directly, all three sharing
/// the one baseline `Plan` (`recipe_runtime::evaluate_plans_with_cache_mode`'s own dispatch
/// proves this is safe: the two whole-grammar strategies ignore `plan` entirely and only
/// `PlanComposed` ever reads it).
///
/// **OBSERVED, not merely expected** (verified running with this test temporarily un-ignored):
/// fails for `PlanComposed` (its candidate set for `fasubel` differs from the oracle -- the
/// HEADA+NONHEADOK compound is never proposed), passes for `TunedSurfaceProbed` and
/// `TemplatedUnderlyingTokens` (both route through `emit.rs`'s `compound_license`/compound-chain
/// support, which `emit_underlying_templated` -- `TemplatedUnderlyingTokens`'s own emitter --
/// shares with the main surface-probed `emit()` path).
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
    let baseline_plan = enumerate_default(
        &grammar,
        &alphabet,
        &prules,
        PhonologyProbe::new(&grammar).as_ref(),
    );

    let plans: Vec<LoweredCandidate> = REQUIRED_STRATEGIES
        .iter()
        .map(|&strategy| LoweredCandidate {
            label: "red1-compounding-cross-compiler",
            plan: baseline_plan.clone(),
            adapter: LoweringAdapter::for_strategy(strategy),
            // Exactly what the deleted `is_baseline` slice said here: the plan-composing candidate
            // carries the grammar's own default plan and so is the baseline; the two whole-grammar
            // adapters never read a plan, so their role is never consulted.
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

/// RED-2: pins the compounding discrimination `pg_parse::identity::AnalysisIdentity` makes
/// load-bearing via `root_index` -- two analyses of the SAME word, with the SAME ordered morpheme
/// sequence and the SAME output category, differing ONLY in which morpheme is the compound's
/// head/root. This is NOT the same claim RED-1 pins (RED-1 is "a compound gets proposed at all");
/// RED-2 is "when TWO headedness readings of the SAME surface form both exist, does the strategy
/// retain BOTH, not just one."
///
/// This discrimination is invisible to the flat `morphs|surface` signature string every other
/// staged fixture (including RED-1's own `compounding-non-recursive`) diffs against:
/// `pg_parse::result_signature`/`BatchCommand.BuildSignature` joins bare `MorphemeId`s with no root
/// marker at all, so `conformance-staging/edge-cases/head-ambiguous-compounding`'s two `dakimo`
/// readings both render to the identical string `"DAK+IMO|dakimo"` (see that fixture's own
/// `words.yaml` header note). A words.yaml-only pin of this fixture is therefore an ANNOTATION, not
/// an assertion -- a human note claiming "these two identical-looking entries differ in headedness"
/// that nothing machine-checks. The assertion below is the first-class one: it compares
/// deduplicated [`pg_parse::identity::AnalysisIdentity`] SETS via [`certify_corpus`], exactly as
/// RED-1 already does -- `AnalysisIdentity` carries `root_index`, so `certify_corpus` was already
/// root-index-aware, just never exercised by a fixture where the flat signature string alone
/// couldn't tell the difference.
///
/// Runs the newly-staged `head-ambiguous-compounding` fixture's two-reading witness `dakimo`
/// (crLeftHead's dak-headed reading, root_index 0; crRightHead's imo-headed reading, root_index 1)
/// through all three `REQUIRED_STRATEGIES`, built directly as `LoweredCandidate`s exactly as RED-1
/// does: this fixture likewise declares no phonological rules and no templates, so
/// `Registry`/`Applicability::HasPhonologyOrTemplates` would never auto-offer
/// `TemplatedUnderlyingTokens` for it (that gate controls what the OPTIMIZER auto-proposes, not
/// what a compiler can legally be asked to build), and `recipe_runtime::
/// evaluate_plans_with_cache_mode`'s own dispatch proves sharing one baseline `Plan` across
/// all three is safe (the two whole-grammar strategies ignore `plan` entirely).
///
/// **OBSERVED** (verified by running this test): all three strategies -- `PlanComposed`,
/// `TunedSurfaceProbed`, and `TemplatedUnderlyingTokens` -- retain BOTH headedness readings for
/// `dakimo`, matching the oracle's own two-reading `AnalysisIdentity` set exactly.
/// `EmissionStrategy::PlanComposed`'s bounded compound loop (`uflexc.rs`) closes RED-1's
/// single-root recall gap generally enough that headedness ambiguity survives too, not merely
/// generic compound proposal -- so this fixture is committed un-`#[ignore]`d as a green regression
/// guard, not a pinned gap.
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
    let baseline_plan = enumerate_default(
        &grammar,
        &alphabet,
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

        // Sanity: the fixture's own oracle (full-HC parser) must retain both headedness readings
        // -- this is the fixture's own claim, checked before asking anything of the strategy under
        // test. A regression here would mean the FIXTURE stopped being ambiguous, not the compiler.
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
