//! Pins that the oracle's `(capped, steps, analyses)` triple and the eligibility ledger built from it are a pure function of `(grammar, word, cap)`, never of machine load -- measured under deliberate CPU load.

use pg_conformance_fixtures::{discover, Root};
use pg_foma::recipe_runtime::{RunEvaluationCache, RuntimeBudget};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Repetitions per configuration: enough that a load-sensitive classifier would be unlikely to agree every time.
const REPETITIONS: usize = 12;

/// Spins every spare core until dropped, so each repetition is measured on a contended machine.
struct ArtificialLoad {
    stop: Arc<AtomicBool>,
    handles: Vec<std::thread::JoinHandle<()>>,
}

impl ArtificialLoad {
    fn start() -> Self {
        let spare = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(1))
            .unwrap_or(1)
            .clamp(1, 8);
        let stop = Arc::new(AtomicBool::new(false));
        let handles = (0..spare)
            .map(|_| {
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || {
                    let mut acc: u64 = 0;
                    while !stop.load(Ordering::Relaxed) {
                        for i in 0..10_000u64 {
                            acc = acc.wrapping_mul(6364136223846793005).wrapping_add(i);
                        }
                    }
                    std::hint::black_box(acc);
                })
            })
            .collect();
        Self { stop, handles }
    }
}

impl Drop for ArtificialLoad {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

fn fixture() -> (pg_grammar::model::Grammar, Vec<String>) {
    let fixture = discover()
        .into_iter()
        .find(|f| f.root == Root::Staging && f.name == "recipe-gated-generic")
        .expect("missing staged fixture recipe-gated-generic");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("staged fixture must load");
    // "tulik" completes at cap 5; "menulik" exhausts it -- a mixed corpus exercises the classifier's boundary.
    let words = vec!["tulik".to_string(), "menulik".to_string()];
    (grammar, words)
}

#[test]
fn morpher_step_count_and_cap_verdict_are_deterministic_under_load() {
    let (grammar, words) = fixture();
    let _load = ArtificialLoad::start();

    for cap in [0usize, 5, 20_000] {
        let mut observations: Vec<Vec<(bool, usize, usize)>> = Vec::with_capacity(REPETITIONS);
        for _ in 0..REPETITIONS {
            // A fresh Morpher per repetition, so a persisting per-instance cache can't make later repetitions agree for the wrong reason.
            let morpher = pg_parse::Morpher::new(&grammar, cap);
            observations.push(
                words
                    .iter()
                    .map(|word| {
                        let outcome = morpher.parse_word(word);
                        assert!(
                            !outcome.timed_out,
                            "no wall-clock deadline was armed, so `timed_out` must be impossible"
                        );
                        (outcome.capped, outcome.steps, outcome.structured.len())
                    })
                    .collect(),
            );
        }
        let first = &observations[0];
        for (index, observation) in observations.iter().enumerate().skip(1) {
            assert_eq!(
                observation, first,
                "PREMISE VIOLATED: the oracle's (capped, steps, analyses) triple is NOT a function \
                 of (grammar, word, cap) alone -- repetition {index} at cap {cap} disagreed with \
                 repetition 0. Step-cap classification cannot be more reproducible than this, so \
                 deterministic eligibility as designed is not achievable and needs rethinking."
            );
        }
    }
}

#[test]
fn eligibility_ledger_is_byte_identical_across_repetitions_under_load() {
    let (grammar, words) = fixture();
    let _load = ArtificialLoad::start();

    for cap in [0usize, 5, 20_000] {
        let budget = RuntimeBudget {
            oracle_step_cap: Some(cap),
            ..RuntimeBudget::default()
        };
        let mut ledgers = Vec::with_capacity(REPETITIONS);
        for _ in 0..REPETITIONS {
            let cache = RunEvaluationCache::prepare(&grammar, &words, budget)
                .expect("the liveness net must not trip on a two-word fixture corpus");
            ledgers.push(cache.corpus_evidence(&words));
        }
        let first = &ledgers[0];
        for (index, ledger) in ledgers.iter().enumerate().skip(1) {
            assert_eq!(
                ledger, first,
                "PREMISE VIOLATED: the eligibility ledger is load-sensitive -- repetition {index} \
                 at cap {cap} produced a different ledger than repetition 0. A digest over a set \
                 that machine load can change is not a digest."
            );
        }
        // Non-vacuity: the two extreme caps must actually classify differently, or this test could pass while measuring nothing.
        assert_eq!(first.requested, 2);
        if cap == 0 {
            assert_eq!(first.excluded, 2, "cap 0 must exclude every occurrence");
        }
        if cap == 20_000 {
            assert_eq!(
                first.excluded, 0,
                "the default cap must exclude nothing here"
            );
        }
    }
}

#[test]
fn the_step_cap_is_what_decides_and_a_larger_cap_admits_strictly_more() {
    let (grammar, words) = fixture();
    let ledger_at = |cap: usize| {
        let cache = RunEvaluationCache::prepare(
            &grammar,
            &words,
            RuntimeBudget {
                oracle_step_cap: Some(cap),
                ..RuntimeBudget::default()
            },
        )
        .expect("the liveness net must not trip on a two-word fixture corpus");
        cache.corpus_evidence(&words)
    };
    let tight = ledger_at(5);
    let generous = ledger_at(20_000);
    assert!(
        generous.included > tight.included,
        "a larger step cap must admit strictly more occurrences on this fixture, otherwise the cap \
         is not the classifier: tight={tight:?} generous={generous:?}"
    );
    assert_eq!(tight.exclusions[0].reason, "oracle-capped");
    assert_eq!(tight.exclusions[0].word, "menulik");
}
