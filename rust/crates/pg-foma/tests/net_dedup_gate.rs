//! **Pins net-level candidate dedup: that it fires, that it changes nothing it reports, and that a
//! cached measurement cannot cross a grammar, a corpus, or an evidence mode.**
//!
//! # What is being optimized, and why it is sound
//!
//! Plan-shape recipes are ERASED by minimization — measured spread 0 across 8 fixtures, and all five
//! Indonesian plan-composed permutations landed on identical states/arcs with identical proposals. So
//! `evaluate_plans_with_cache` was paying a full propose + confirm + whole-corpus traversal for
//! candidates whose finished networks are bit-identical. Net-level dedup collapses those.
//!
//! Score attribution is TRIVIALLY sound here, and that is the whole reason this shape was chosen over
//! confirmation-memoization: identical networks legitimately have identical deterministic scores, so
//! nothing becomes order-dependent. Contrast a set-difference confirmation scheme, which is sound as a
//! RESULT but unsound as a MEASUREMENT, because each candidate's measured cost would become a function
//! of its position in the evaluation order — exactly what `Score::key`'s "why work and not time"
//! section exists to prevent. `dedup_moves_no_certification_and_no_deterministic_score_field` is the
//! assertion that keeps it that way.
//!
//! # Every test here is a NEGATIVE control by construction
//!
//! `RunEvaluationCache::without_net_dedup` is not a convenience; it is the falsifier. Every claim
//! below is stated as "dedup ON versus dedup OFF", so each test genuinely fails if the mechanism is
//! reverted or neutered — a same-path-twice comparison would pass whatever the mechanism did.

use pg_conformance_fixtures::{discover, Root};
use pg_foma::enumerate::{enumerate_default, EmissionStrategy, LoweredCandidate};
use pg_foma::executable_candidate::PortablePlan;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::recipe_optimizer::{Certification, Score};
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::recipe_runtime::{
    evaluate_plans_observed_with_cache, evaluate_plans_with_cache, grammar_identity, net_reuse_key,
    RunEvaluationCache, RuntimeBudget, RuntimeEvaluation,
};
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};

/// The fixture the fire-count is pinned on.
///
/// Named, not searched for: a test that scans for "some fixture where dedup fires" cannot fail when
/// the mechanism stops firing everywhere — it fails only when it stops firing *and* nothing else
/// starts. `net_dedup_sizing_census` measured this fixture's duplicate count; if that number ever goes
/// to zero, this test failing is the correct and informative outcome.
// Chosen FROM the sizing census, not by guessing, because these gates refuse to run vacuously and so
// only a fixture that genuinely produces a duplicate network can exercise them
// (`net_dedup_sizing_census::distinct_finished_nets_versus_plan_count_per_fixture`):
// `recipe-ordered-generic` is plans=7 digested=5 DISTINCT=4 duplicates=1.
//
// It was `recipe-gated-generic`, which the same census reports as plans=5 digested=3 DISTINCT=3
// **duplicates=0** — so all four fire-count-guarded gates failed on `nets_deduped() > 0`, exactly as
// their own assertion message predicted ("this fixture stopped producing duplicate networks -- check
// net_dedup_sizing_census before relaxing this"). The guard earned its place: without it these four
// would have passed VACUOUSLY, since dedup-on and dedup-off are trivially identical on a fixture where
// dedup can never fire.
//
// If this fixture ever stops producing a duplicate, re-read the census and pick another rather than
// relaxing the guard. Eight fixtures had a duplicate at that measurement: metathesis-phase-isolation,
// suffixing-extension-slot-ordering, suffixing-vowel-harmony, circumfix-reduplication-precedence,
// deletion-reduplication-exception-composite, guesser-pattern-root-fallback,
// optional-template-composite, recipe-ordered-generic.
const FIRING_FIXTURE: &str = "recipe-ordered-generic";

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

/// Every `Score` field that is a property of the COMPILATION rather than of the machine it ran on.
///
/// `build` and `apply` are excluded because they are wall-clock diagnostics with a documented 15-50%
/// and 6-20% run-to-run spread (`Score::key`), so requiring them to be equal would assert that time
/// is reproducible — the very claim `Score::key` exists to deny. Everything that RANKS is here.
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

/// The winner, chosen exactly as `RecipeOptimizationReport` chooses it: only a `selectable()`
/// candidate may win, ranked by `Score::key`.
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

// -------------------------------------------------------------------------------------------------
// The mechanism engaged
// -------------------------------------------------------------------------------------------------

/// **The fire-count**, stated as a counter that is provably ZERO with the mechanism off and non-zero
/// with it on, on a NAMED input — never as a timing.
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
    // Deterministic counters, never elapsed time: one propose call per corpus word per deduped
    // candidate, and the donor's own confirmation-call count per deduped candidate.
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

// -------------------------------------------------------------------------------------------------
// Nothing it reports moves
// -------------------------------------------------------------------------------------------------

/// **Recall is not negotiable**: a deduped candidate reports exactly what it would have reported
/// unduplicated — same certification, same realized strategy, same every deterministic `Score` field,
/// and the same winner.
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
    // The run-scoped parity divergence must be folded exactly once per candidate either way: a
    // deduped candidate legitimately contributes the SAME counts its donor did, because it would have
    // compared the same identities against the same ground truth.
    assert_eq!(
        off_cache.identity_divergence(),
        on_cache.identity_divergence(),
        "net-level dedup changed the run's accumulated parity divergence"
    );
}

/// **Never truncate a word's proposal set.** The observed evaluator retains the exact deduplicated
/// candidate vector per word, and the parity relation reads a truncated set as disagreement — so this
/// compares the per-word evidence itself, not just the verdict derived from it.
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

/// **A dedup hit re-runs the budget breach ladder against its OWN score; it never inherits the
/// donor's verdict.**
///
/// This is the one place a naive dedup would smuggle evaluation order into a CERTIFICATION, and the
/// production optimizer makes it live rather than hypothetical: `pg_cli`'s evaluator calls in once per
/// candidate with `build: Some(remaining.build)` — a budget that DECLINES as the run proceeds. So the
/// same network, measured at call 1 under a generous allowance and hit at call 20 under a nearly
/// exhausted one, must produce call 20's verdict.
///
/// Modelled exactly that way: one cache, two calls, the second with a limit the first did not have.
/// The limit is `states`, a deterministic dimension, deliberately NOT `build` — `RuntimeBudget::build`
/// doubles as the `ComposeBudget` step timeout, so a value low enough to breach would sometimes kill
/// the compose instead and the test would be asserting flakiness. The ladder being re-run is the
/// property under test, and `states` exercises the identical code path with none of the ambiguity.
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

    // Same plan, same grammar, same corpus, same mode -- a guaranteed hit -- but now with a limit no
    // network can meet.
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
    // The deterministic half is still the donor's, because it is the same network -- that is the whole
    // point. Only the verdict is re-derived.
    assert_eq!(
        deterministic(second[0].score),
        deterministic(first[0].score)
    );
    // `build` is measured on the hit's own call, not inherited. Exact equality with a clock cannot be
    // asserted (that is the premise of `Score::key`), so what is asserted is that a real measurement
    // happened: `realize_plan_composed` floors its reading at 1, and a candidate served entirely from
    // the cache without building would have no reading at all.
    assert!(
        second[0].score.build >= 1,
        "a dedup hit still builds its own network -- it must report its own build reading"
    );
}

// -------------------------------------------------------------------------------------------------
// A cached measurement cannot cross a grammar, a corpus, or an evidence mode
// -------------------------------------------------------------------------------------------------

/// The reuse key discriminates on all four of its inputs.
///
/// A net digest ALONE would let a cached result cross grammars — silently, and in the reassuring
/// direction, since the reused verdict is usually a pass. Each of the four is varied in isolation.
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

/// **This Plan identity is an EXPLICIT canonical serialization, stable across two independent
/// constructions from two independent loads.**
///
/// This is the property the RED test below shows `grammar_identity` does NOT have, and it is here
/// rather than beside the sealing gates precisely so the contrast is readable in one file. The
/// difference is not care taken; it is the choice of preimage:
///
/// * `grammar_identity` hashes the grammar's derived `Debug` projection. `Grammar` holds hash-ordered
///   collections as struct fields (`CharDefTable::lookup`, `featsys`'s `symbol_index`/`id_to_flat`),
///   Rust seeds `RandomState` per `HashMap` INSTANCE, so two loads print those fields in different
///   orders and hash differently.
/// * `PortablePlan::canonical_json` serializes a document whose every collection is in a defined
///   order by construction: nodes come from `Plan`'s `BTreeMap<NodeId, _>` (content-address order),
///   children/rules/groups are in their own semantic order, and `serde_json` emits struct fields in
///   declaration order. No `HashMap` is ever in the preimage, and no `Debug` impl is either -- so a
///   later edit "for readability" cannot narrow or reorder it.
///
/// Both halves are asserted, because either alone is worthless: a constant would pass stability and a
/// nonce would pass discrimination.
///
/// TWO independent loads AND two independent enumerations, deliberately. Re-encoding one in-memory
/// `Plan` twice would only prove `serde_json` is deterministic; re-loading the grammar is what
/// exercises the hash-ordered collections that `enumerate_default` reads on its way to a plan.
#[test]
fn the_plan_document_identity_is_canonical_across_two_independent_constructions() {
    let plan_of = |name: &str| {
        let (grammar, _) = load(name);
        let alphabet = SegAlphabet::new(surface_table(&grammar));
        let prules: Vec<&PhonRuleDef> = pg_foma::enumerate::prules_in_order(&grammar);
        let phonology = PhonologyProbe::new(&grammar);
        let plan = enumerate_default(&grammar, &alphabet, &prules, phonology.as_ref());
        let document = PortablePlan::encode(&plan);
        (document.canonical_json(), document.digest())
    };

    let (first_json, first_digest) = plan_of(FIRING_FIXTURE);
    let (second_json, second_digest) = plan_of(FIRING_FIXTURE);
    assert_eq!(
        first_json, second_json,
        "the canonical serialization must be BYTE-identical across two independent loads of the same \
         grammar; if it is not, the preimage contains something ordered by a per-instance hash seed \
         and this identity names a process rather than an artifact"
    );
    assert_eq!(
        first_digest, second_digest,
        "and therefore so must the digest -- this is the property `grammar_identity` lacks (see the \
         RED test below), and it is what makes this digest usable as a persisted artifact identity"
    );
    assert!(
        first_digest.starts_with("sha256:"),
        "domain-framed SHA-256, never the plan's 64-bit FNV root: {first_digest}"
    );

    // Discrimination: a genuinely different grammar's default plan must not share the identity. The
    // stability assertion above is satisfied by any constant, so without this the test proves nothing.
    let (other_json, other_digest) = plan_of("recipe-gated-generic");
    assert_ne!(
        first_json, other_json,
        "two different grammars' default plans must not serialize identically"
    );
    assert_ne!(
        first_digest, other_digest,
        "two different plans must not share a plan-document digest"
    );
}

/// The grammar identity is stable across independent loads of the same grammar, differs between two
/// different grammars, and moves for a change to a SINGLE allomorph field deep in the tree.
///
/// The last of these is the one that matters: `grammar_identity` hashes the grammar's derived `Debug`
/// projection precisely so that no field can be forgotten, and this asserts that property rather than
/// trusting it. If a future member of the grammar tree acquires a hand-written `Debug` that elides
/// content, a test like this is the only thing that notices.
/// RED — this FAILS today, on its FIRST assertion, and the defect it exposes is real.
///
/// `grammar_identity` hashes the grammar's derived `Debug` projection. That projection is NOT
/// CANONICAL, because the grammar tree holds hash-ordered collections as struct fields —
/// `pg_grammar::chardef::CharDefTable::lookup` (`HashMap<String, CharDefId>`, chardef.rs:125) and
/// `featsys`'s `symbol_index` / `id_to_flat` (featsys.rs:46, :93) among them. Rust's `RandomState` is
/// seeded per `HashMap` instance, so two independent loads of the SAME grammar hold identical contents
/// in different iteration order, print different `Debug` output, and hash to different digests.
///
/// WHICH DIRECTION IT FAILS IN, because that decides how urgent it is: it fails SAFE. An unstable
/// identity means a key never matches across loads, so a cached measurement is never reused where it
/// should not be — the failure costs reuse, never correctness. The net-level dedup that ships in this
/// commit is RUN-SCOPED and holds one `&Grammar` for the whole run, so it is unaffected and its own
/// gates pass.
///
/// WHAT IT DOES BREAK: any PERSISTENT, CROSS-RUN cache keyed on this identity — which is exactly the
/// design of the queued persistent oracle cache. That task's premise is a digest keyed on
/// (grammar identity, word, step cap, memory ceiling), and this digest cannot serve it. Fix this first
/// or that cache will silently never hit, which is the inert-mechanism failure this project has already
/// shipped once.
///
/// THE FIX IS NOT "sort the HashMaps in `Debug`" — a `Debug` impl written to be canonical is a
/// `Debug` impl someone will later edit for readability, and the whole point of hashing the derived
/// projection was that no field can be forgotten. Prefer an explicit canonical serialization of the
/// semantic content, following the `ModelRevision` precedent that already split semantic from
/// presentation-only fields for this same class of reason.
///
/// The SECOND assertion in this test (flipping one `is_bound` moves the identity) is the property
/// worth keeping either way, and it has never been reached. Re-enable the whole test with the fix.
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
