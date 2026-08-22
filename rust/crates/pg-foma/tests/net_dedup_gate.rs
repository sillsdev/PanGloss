//! Pins net-level candidate dedup: that it fires, that it changes nothing it reports, and that a cached measurement cannot cross a grammar, a corpus, or an evidence mode. Every test is a NEGATIVE control by construction: `RunEvaluationCache::without_net_dedup` is the falsifier, so each test fails if the mechanism is reverted or neutered.
//! See `docs/research/pg-foma-net-dedup-sizing-census.md` for why this optimization is sound and why score attribution cannot become order-dependent.

use pg_conformance_fixtures::{discover, Root};
use pg_foma::backend_optimizer::{Certification, Score};
use pg_foma::backend_registry::{MaterializerContext, Registry};
use pg_foma::backend_runtime::{
    evaluate_plans_observed_with_cache, evaluate_plans_with_cache, grammar_identity, net_reuse_key,
    RunEvaluationCache, RuntimeBudget, RuntimeEvaluation,
};
use pg_foma::enumerate::{enumerate_default, EmissionStrategy, LoweredCandidate};
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};

/// The fixture the fire-count is pinned on. Named, not searched for, so a test cannot pass vacuously by scanning for "some fixture where dedup fires" -- see `docs/research/pg-foma-net-dedup-sizing-census.md` for why this one was chosen and what happened when the wrong one was tried first.
const FIRING_FIXTURE: &str = "guesser-pattern-root-fallback";

fn surface_table(grammar: &Grammar) -> &pg_grammar::chardef::CharDefTable {
    let surface_stratum = grammar
        .strata
        .last()
        .expect("a loaded grammar always has at least one stratum");
    &grammar.char_tables[surface_stratum.table.0 as usize]
}

fn load(name: &str) -> (Grammar, Vec<String>) {
    let fixture = discover()
        .into_iter()
        .find(|fixture| fixture.root == Root::Staging && fixture.name == name)
        .unwrap_or_else(|| panic!("staged fixture {name}"));
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture grammar");
    let words = fixture
        .load_words_yaml()
        .words
        .into_iter()
        .map(|word| word.word)
        .collect();
    (grammar, words)
}

fn registry_plans(grammar: &Grammar) -> Vec<LoweredCandidate> {
    let alphabet = SegAlphabet::new(surface_table(grammar));
    let prules: Vec<&PhonRuleDef> = pg_foma::enumerate::prules_in_order(grammar);
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

/// Every `Score` field that is a property of the COMPILATION, not the machine it ran on. `build`/`apply` are excluded as wall-clock diagnostics with a documented run-to-run spread; everything that RANKS is here.
type DeterministicScore = (u64, u64, u64, u64, u64, u64, [u64; 6]);

fn deterministic(score: Score) -> DeterministicScore {
    (
        score.states,
        score.arcs,
        score.proposals,
        score.confirmation,
        score.confirmation_steps,
        score.raw_paths,
        score.pareto_vector(),
    )
}

fn verdicts(
    evaluations: &[RuntimeEvaluation],
) -> Vec<(Certification, EmissionStrategy, DeterministicScore)> {
    evaluations
        .iter()
        .map(|evaluation| {
            (
                evaluation.certification.clone(),
                evaluation.realized_strategy,
                deterministic(evaluation.score),
            )
        })
        .collect()
}

/// The winner, chosen exactly as `BackendOptimizationReport` chooses it: only a `selectable()` candidate may win, ranked by `Score::key`.
fn winner(evaluations: &[RuntimeEvaluation]) -> Option<(usize, (u64, u64, u64, u64, String))> {
    evaluations
        .iter()
        .enumerate()
        .filter(|(_, evaluation)| evaluation.certification.selectable())
        .min_by_key(|(index, evaluation)| evaluation.score.key(&index.to_string()))
        .map(|(index, evaluation)| (index, evaluation.score.key(&index.to_string())))
}

fn evaluate(
    grammar: &Grammar,
    plans: &[LoweredCandidate],
    words: &[String],
    budget: RuntimeBudget,
    dedup: bool,
) -> (Vec<RuntimeEvaluation>, RunEvaluationCache) {
    let cache = RunEvaluationCache::prepare(grammar, words, budget)
        .expect("oracle preparation must succeed on this fixture");
    let mut cache = if dedup {
        cache
    } else {
        cache.without_net_dedup()
    };
    let evaluations = evaluate_plans_with_cache(grammar, plans, words, budget, &mut cache);
    (evaluations, cache)
}

// The mechanism engaged.

/// The fire-count, stated as a counter provably ZERO with the mechanism off and non-zero with it on, on a NAMED input -- never as a timing.
#[test]
fn dedup_fires_and_avoids_propose_and_confirm_work() {
    let (grammar, words) = load(FIRING_FIXTURE);
    let plans = registry_plans(&grammar);
    let composed = plans
        .iter()
        .filter(|plan| plan.strategy() == EmissionStrategy::PlanComposed)
        .count();
    assert!(
        composed >= 2,
        "{FIRING_FIXTURE} must materialize at least two plan-composed candidates for dedup to have \
         anything to collapse; it materialized {composed}"
    );

    let (_, off) = evaluate(&grammar, &plans, &words, RuntimeBudget::default(), false);
    assert!(!off.net_dedup_enabled());
    assert_eq!(
        off.nets_deduped(),
        0,
        "with dedup disabled nothing may be served from another candidate"
    );
    assert_eq!(off.propose_calls_avoided(), 0);
    assert_eq!(off.confirmation_calls_avoided(), 0);

    let (_, on) = evaluate(&grammar, &plans, &words, RuntimeBudget::default(), true);
    assert!(on.net_dedup_enabled());
    assert!(
        on.nets_deduped() > 0,
        "net-level dedup never fired on {FIRING_FIXTURE}: {} plan-composed candidates produced {} \
         distinct finished networks. Either the mechanism is not engaged or this fixture stopped \
         producing duplicate networks -- check net_dedup_sizing_census before relaxing this",
        composed,
        on.distinct_nets()
    );
    assert!(
        on.distinct_nets() < composed,
        "distinct nets ({}) must be fewer than plan-composed candidates ({composed}) for a hit to be \
         possible at all",
        on.distinct_nets()
    );
    // Deterministic counters, never elapsed time: one propose call per corpus word per deduped candidate, and the donor's confirmation-call count per deduped candidate.
    assert!(
        on.propose_calls_avoided() > 0,
        "a deduped candidate must have skipped PROPOSE as well as confirm -- skipping only \
         confirmation would leave the corpus traversal in place, which is half the cost"
    );
    eprintln!(
        "{FIRING_FIXTURE}: plan_composed={composed} distinct_nets={} nets_deduped={} \
         propose_calls_avoided={} confirmation_calls_avoided={} confirmation_steps_avoided={}",
        on.distinct_nets(),
        on.nets_deduped(),
        on.propose_calls_avoided(),
        on.confirmation_calls_avoided(),
        on.confirmation_steps_avoided(),
    );
}

// Nothing it reports moves.

/// Recall is not negotiable: a deduped candidate reports exactly what it would have reported unduplicated -- same certification, same realized strategy, same every deterministic `Score` field, same winner.
#[test]
fn dedup_moves_no_certification_and_no_deterministic_score_field() {
    let (grammar, words) = load(FIRING_FIXTURE);
    let plans = registry_plans(&grammar);
    let (off, off_cache) = evaluate(&grammar, &plans, &words, RuntimeBudget::default(), false);
    let (on, on_cache) = evaluate(&grammar, &plans, &words, RuntimeBudget::default(), true);
    assert!(
        on_cache.nets_deduped() > 0,
        "this comparison is vacuous unless dedup actually fired"
    );
    assert_eq!(
        verdicts(&off),
        verdicts(&on),
        "net-level dedup changed a certification, a realized strategy, or a deterministic score field"
    );
    assert_eq!(
        winner(&off),
        winner(&on),
        "net-level dedup moved the winner"
    );
    // The run-scoped parity divergence must be folded exactly once per candidate either way: a deduped candidate legitimately contributes the SAME counts its donor did.
    assert_eq!(
        off_cache.identity_divergence(),
        on_cache.identity_divergence(),
        "net-level dedup changed the run's accumulated parity divergence"
    );
}

/// Never truncate a word's proposal set: the observed evaluator retains the exact deduplicated candidate vector per word, so this compares the per-word evidence itself, not just the verdict derived from it.
#[test]
fn a_deduped_candidate_keeps_every_words_full_proposal_set() {
    let (grammar, words) = load(FIRING_FIXTURE);
    let plans = registry_plans(&grammar);

    let observe = |dedup: bool| {
        let cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
            .expect("oracle preparation must succeed on this fixture");
        let mut cache = if dedup {
            cache
        } else {
            cache.without_net_dedup()
        };
        let observations = evaluate_plans_observed_with_cache(
            &grammar,
            &plans,
            &words,
            RuntimeBudget::default(),
            &mut cache,
        );
        (observations, cache)
    };

    let (off, _) = observe(false);
    let (on, on_cache) = observe(true);
    assert!(
        on_cache.nets_deduped() > 0,
        "this comparison is vacuous unless dedup actually fired in observed mode too"
    );
    assert_eq!(off.len(), on.len());
    for (off, on) in off.iter().zip(&on) {
        assert_eq!(
            off.requested_strategy, on.requested_strategy,
            "observed candidates must line up"
        );
        assert_eq!(
            off.words.as_ref().map(Vec::len),
            on.words.as_ref().map(Vec::len),
            "a deduped candidate lost or gained corpus evidence rows"
        );
        assert_eq!(
            off.words, on.words,
            "a deduped candidate's per-word evidence -- expected analyses, actual analyses, the full \
             proposal vector, and both identity sets -- must be identical to what it would have \
             produced unduplicated"
        );
    }
}

/// A dedup hit re-runs the budget breach ladder against its OWN score; it never inherits the donor's verdict -- modelled as one cache, two calls, the second with a `states` limit (deterministic, unlike `build` which doubles as a compose timeout) the first did not have.
/// See `docs/research/pg-foma-net-dedup-sizing-census.md` for why a naive dedup would otherwise smuggle evaluation order into a certification.
#[test]
fn a_dedup_hit_re_runs_the_budget_breach_ladder_on_its_own_score() {
    let (grammar, words) = load(FIRING_FIXTURE);
    let composed: Vec<LoweredCandidate> = registry_plans(&grammar)
        .into_iter()
        .filter(|plan| plan.strategy() == EmissionStrategy::PlanComposed)
        .take(1)
        .collect();
    assert_eq!(composed.len(), 1, "need one plan-composed candidate");
    let mut cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
        .expect("oracle preparation must succeed on this fixture");

    let first = evaluate_plans_with_cache(
        &grammar,
        &composed,
        &words,
        RuntimeBudget::default(),
        &mut cache,
    );
    assert_eq!(
        cache.nets_deduped(),
        0,
        "the first evaluation of a network cannot be a hit"
    );
    assert_eq!(cache.distinct_nets(), 1);
    assert!(
        first[0].certification.selectable(),
        "the donor must be a clean confirmation for this test to mean anything: {:?}",
        first[0].certification
    );

    // Same plan, same grammar, same corpus, same mode -- a guaranteed hit -- but now with a limit no network can meet.
    let second = evaluate_plans_with_cache(
        &grammar,
        &composed,
        &words,
        RuntimeBudget {
            states: Some(0),
            ..Default::default()
        },
        &mut cache,
    );
    assert_eq!(
        cache.nets_deduped(),
        1,
        "re-evaluating an identical network against the same cache must be a hit"
    );
    assert!(
        matches!(
            &second[0].certification,
            Certification::ResourceBreach { dimension, .. } if dimension == "states"
        ),
        "a deduped candidate inherited the donor's verdict ({:?}) instead of being judged against the \
         budget in force for its OWN call; got {:?}",
        first[0].certification,
        second[0].certification
    );
    // The deterministic half is still the donor's, because it is the same network -- only the verdict is re-derived.
    assert_eq!(
        deterministic(second[0].score),
        deterministic(first[0].score)
    );
    // `build` is measured on the hit's own call, not inherited: `realize_plan_composed` floors its reading at 1, and a candidate served entirely from cache would have no reading at all.
    assert!(
        second[0].score.build >= 1,
        "a dedup hit still builds its own network -- it must report its own build reading"
    );
}

// A cached measurement cannot cross a grammar, a corpus, or an evidence mode.

/// The reuse key discriminates on all four of its inputs: a net digest ALONE would let a cached result cross grammars silently, and in the reassuring direction, since the reused verdict is usually a pass.
#[test]
fn the_reuse_key_discriminates_grammar_corpus_mode_and_net() {
    let base = net_reuse_key("grammar-a", "corpus-a", false, "net-a");
    assert_eq!(base, net_reuse_key("grammar-a", "corpus-a", false, "net-a"));
    assert_ne!(
        base,
        net_reuse_key("grammar-b", "corpus-a", false, "net-a"),
        "a different grammar must not share a reuse key"
    );
    assert_ne!(
        base,
        net_reuse_key("grammar-a", "corpus-b", false, "net-a"),
        "a different corpus must not share a reuse key"
    );
    assert_ne!(
        base,
        net_reuse_key("grammar-a", "corpus-a", true, "net-a"),
        "the observed evaluator retains per-word proposal evidence the ordinary one does not, so the \
         two modes must not share a reuse key"
    );
    assert_ne!(
        base,
        net_reuse_key("grammar-a", "corpus-a", false, "net-b"),
        "a different network must not share a reuse key"
    );
    // Length-prefixed framing: without it, a boundary shift between adjacent parts would collide.
    assert_ne!(
        net_reuse_key("ab", "c", false, "net"),
        net_reuse_key("a", "bc", false, "net"),
        "the reuse key's parts must be length-prefixed"
    );
}

/// The grammar identity is stable across independent loads, differs between two different grammars, and moves for a change to a SINGLE allomorph field -- RED today because `grammar_identity` hashes a non-canonical `Debug` projection over hash-ordered collections, failing SAFE (costs reuse, not correctness).
/// See `docs/research/pg-foma-net-dedup-sizing-census.md` for the full diagnosis, what it breaks, and why the fix is not "sort the HashMaps".
#[test]
#[ignore = "RED: grammar_identity hashes a non-canonical derived Debug projection, so two loads of \
            the same grammar disagree (hash-ordered HashMap fields in chardef/featsys). Fails safe \
            -- costs reuse, never correctness -- and does not affect the run-scoped net dedup here, \
            but it blocks any persistent cross-run cache keyed on this identity."]
fn the_grammar_identity_is_stable_and_moves_for_a_single_allomorph_field() {
    let (grammar, _) = load(FIRING_FIXTURE);
    let (reloaded, _) = load(FIRING_FIXTURE);
    assert_eq!(
        grammar_identity(&grammar),
        grammar_identity(&reloaded),
        "two independent loads of the same grammar must have the same identity, or dedup would never \
         fire across calls"
    );

    let (other, _) = load("head-ambiguous-compounding");
    assert_ne!(
        grammar_identity(&grammar),
        grammar_identity(&other),
        "two different grammars must not share an identity"
    );

    let mut mutated = reloaded;
    let entry = mutated
        .entries
        .iter_mut()
        .find(|entry| !entry.allomorphs.is_empty())
        .expect("the fixture grammar must have a lexical entry with an allomorph");
    let allomorph = &mut entry.allomorphs[0];
    allomorph.is_bound = !allomorph.is_bound;
    assert_ne!(
        grammar_identity(&grammar),
        grammar_identity(&mutated),
        "flipping ONE field of ONE allomorph must move the grammar identity; if it does not, the \
         projection is narrower than the grammar and a cached measurement could cross grammars that \
         differ only in what the projection forgot"
    );
}
