//! Pins deterministic corpus eligibility: only the step cap classifies; a clock or memory fault must abort the run rather than produce a silent per-word exclusion.

use pg_conformance_fixtures::{discover, Root};
use pg_foma::backend_optimizer::Certification;
use pg_foma::backend_runtime::{
    OraclePreparationFault, RunEvaluationCache, RuntimeBudget, DEFAULT_ORACLE_LIVENESS_NET,
    DEFAULT_ORACLE_MEMORY_CEILING_BYTES, DEFAULT_ORACLE_STEP_CAP,
};
use std::time::Duration;

fn fixture() -> (pg_grammar::model::Grammar, Vec<String>) {
    let fixture = discover()
        .into_iter()
        .find(|f| f.root == Root::Staging && f.name == "backend-gated-generic")
        .expect("missing staged fixture backend-gated-generic");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("staged fixture must load");
    (grammar, vec!["tulik".to_string(), "menulik".to_string()])
}

fn plans(grammar: &pg_grammar::model::Grammar) -> Vec<pg_foma::enumerate::LoweredCandidate> {
    let prules = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phonology = pg_foma::junctions::PhonologyProbe::new(grammar);
    let baseline = pg_foma::enumerate::enumerate_default(grammar, &prules, phonology.as_ref());
    pg_foma::backend_registry::Registry::seeded()
        .materialize_distinct(&pg_foma::backend_registry::MaterializerContext {
            grammar,
            baseline: &baseline,
        })
        .expect("materialization must succeed")
        .into_iter()
        .map(|(_, plan)| plan)
        .collect()
}

/// A zero-length liveness net forces a deterministic trip: the deadline is already past on the first check, not a timing race.
#[test]
fn a_liveness_net_trip_aborts_the_run_and_can_never_be_an_exclusion() {
    let (grammar, words) = fixture();
    let fault = RunEvaluationCache::prepare(
        &grammar,
        &words,
        RuntimeBudget {
            oracle_liveness_net: Some(Duration::ZERO),
            ..RuntimeBudget::default()
        },
    )
    .expect_err(
        "a tripped liveness net must abort preparation -- if this returned Ok, the clock is still \
         producing eligibility outcomes and the eligible set is still load-sensitive",
    );
    match fault {
        OraclePreparationFault::LivenessNetTripped {
            ref word,
            requested_ordinal,
            net,
        } => {
            assert_eq!(word, "tulik", "the fault must name the word it tripped on");
            assert_eq!(requested_ordinal, 0);
            assert_eq!(net, Duration::ZERO);
        }
        other => panic!("expected a liveness-net fault, got {other:?}"),
    }
    assert!(
        fault
            .to_string()
            .contains("eligibility could not be determined"),
        "the fault must say eligibility is UNDETERMINED, not that a word was excluded: {fault}"
    );
}

/// The clock is armed too, but only the step cap may classify a word; the clock may only abort.
#[test]
fn a_step_capped_word_is_classified_by_the_step_cap_not_the_clock() {
    let (grammar, words) = fixture();
    let cache = RunEvaluationCache::prepare(
        &grammar,
        &words,
        RuntimeBudget {
            oracle_step_cap: Some(5),
            ..RuntimeBudget::default()
        },
    )
    .expect("the default liveness net must not trip on a two-word fixture corpus");
    let evidence = cache.corpus_evidence(&words);
    assert_eq!(evidence.requested, 2);
    assert_eq!(evidence.included, 1);
    assert_eq!(evidence.excluded, 1);
    assert!(
        evidence.reconciles(),
        "requested must equal included + excluded: {evidence:?}"
    );
    assert_eq!(evidence.exclusions[0].reason, "oracle-capped");
    assert!(
        !evidence
            .exclusions
            .iter()
            .any(|exclusion| exclusion.reason.contains("timeout")),
        "no exclusion reason may mention a timeout: {:?}",
        evidence.exclusions
    );
}

/// A one-byte memory ceiling deterministically trips, since any live process exceeds it.
#[test]
fn a_declared_memory_ceiling_aborts_the_run_with_a_typed_fault() {
    let (grammar, words) = fixture();
    let fault = RunEvaluationCache::prepare(
        &grammar,
        &words,
        RuntimeBudget {
            oracle_memory_ceiling: Some(1),
            ..RuntimeBudget::default()
        },
    )
    .expect_err("a one-byte memory ceiling must abort preparation");
    match fault {
        OraclePreparationFault::MemoryCeilingExceeded {
            ref word,
            ceiling_bytes,
            observed_bytes,
            ..
        } => {
            assert_eq!(word, "tulik");
            assert_eq!(ceiling_bytes, 1);
            assert!(observed_bytes > 1, "the fault must report what it observed");
        }
        // A build with no RSS sampler must refuse rather than silently skip the declared ceiling.
        OraclePreparationFault::MemoryCeilingUnobservable { ceiling_bytes } => {
            assert_eq!(ceiling_bytes, 1)
        }
        other => panic!("expected a memory-ceiling fault, got {other:?}"),
    }
    assert!(fault
        .to_string()
        .contains("eligibility could not be determined"));
}

/// An explicitly unbounded ceiling is a recorded choice, not merely an omission, and costs no sampling.
#[test]
fn an_explicitly_unbounded_memory_ceiling_is_recorded_not_silent() {
    let (grammar, words) = fixture();
    let cache = RunEvaluationCache::prepare(
        &grammar,
        &words,
        RuntimeBudget {
            oracle_memory_ceiling: Some(u64::MAX),
            ..RuntimeBudget::default()
        },
    )
    .expect("an unbounded ceiling cannot trip");
    assert_eq!(
        cache.corpus_evidence(&words).oracle_memory_ceiling_bytes,
        u64::MAX
    );
}

/// Two runs over the same words at different caps must produce distinguishable evidence.
#[test]
fn the_evidence_binds_the_generating_configuration_and_distinguishes_two_caps() {
    let (grammar, words) = fixture();
    let evidence_at = |budget: RuntimeBudget| {
        RunEvaluationCache::prepare(&grammar, &words, budget)
            .expect("preparation must succeed")
            .corpus_evidence(&words)
    };

    let defaults = evidence_at(RuntimeBudget::default());
    assert_eq!(defaults.oracle_step_cap, DEFAULT_ORACLE_STEP_CAP as u64);
    assert_eq!(
        defaults.oracle_memory_ceiling_bytes,
        DEFAULT_ORACLE_MEMORY_CEILING_BYTES
    );
    assert_eq!(
        defaults.oracle_liveness_net_ns,
        DEFAULT_ORACLE_LIVENESS_NET.as_nanos() as u64
    );

    // Word hashes are equal (same strings); only the exclusion-ledger hash may carry the cap difference.
    let higher = evidence_at(RuntimeBudget {
        oracle_step_cap: Some(DEFAULT_ORACLE_STEP_CAP * 2),
        ..RuntimeBudget::default()
    });
    assert_eq!(higher.requested_hash, defaults.requested_hash);
    assert_eq!(higher.included_hash, defaults.included_hash);
    assert_eq!(higher.excluded, 0);
    assert_eq!(defaults.excluded, 0);
    assert_ne!(
        higher.excluded_hash, defaults.excluded_hash,
        "two runs at different oracle step caps must not produce the same exclusion-ledger hash, \
         or a certification cannot say which cap it was earned under"
    );
    assert_ne!(higher.oracle_step_cap, defaults.oracle_step_cap);
}

/// Zero exclusions still emits a ledger over the raw requested slice -- "no exclusions" and "nobody looked" must not be indistinguishable.
#[test]
fn a_zero_exclusion_run_still_states_the_corpus_it_derived_that_zero_from() {
    let (grammar, words) = fixture();
    let cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
        .expect("preparation must succeed");
    let evidence = cache.corpus_evidence(&words);
    assert_eq!(evidence.excluded, 0);
    assert_eq!(evidence.requested, words.len() as u64);
    assert_eq!(evidence.included, words.len() as u64);
    assert!(evidence.reconciles());
    assert_eq!(evidence.requested_hash, evidence.included_hash);
    assert!(evidence.exclusions.is_empty());
}

/// Exclusions are decided by the oracle alone, never by candidate outcome.
#[test]
fn exclusions_are_candidate_independent() {
    let (grammar, words) = fixture();
    let budget = RuntimeBudget {
        oracle_step_cap: Some(5),
        ..RuntimeBudget::default()
    };
    let mut cache =
        RunEvaluationCache::prepare(&grammar, &words, budget).expect("preparation must succeed");
    let before = cache.corpus_evidence(&words);
    let plans = plans(&grammar);
    assert!(!plans.is_empty(), "fixture must materialize candidates");
    let evaluations = pg_foma::backend_runtime::evaluate_plans_with_cache(
        &grammar, &plans, &words, budget, &mut cache,
    );
    let after = cache.corpus_evidence(&words);
    assert_eq!(
        before, after,
        "evaluating candidates changed the eligibility ledger -- exclusions must be a property of \
         the oracle alone"
    );
    // And every candidate saw the SAME ledger, rather than one derived per candidate.
    for evaluation in &evaluations {
        let Certification::Truncated {
            corpus: Some(corpus),
            ..
        } = &evaluation.certification
        else {
            panic!(
                "a step-capped corpus must refuse every candidate with its ledger attached: {:?}",
                evaluation.certification
            );
        };
        assert_eq!(corpus, &before);
    }
}
