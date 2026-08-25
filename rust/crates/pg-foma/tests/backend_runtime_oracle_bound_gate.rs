//! Pins that a step-capped ground-truth oracle reports an explicit non-certifying `Certification::Truncated`, never reaching `certify_corpus` as a known-partial `expected` — otherwise a partial oracle can manufacture a bogus mismatch against a real, untruncated FST result, or wrongly certify an equally-truncated incomplete candidate. `oracle_step_cap: Some(0)` forces the truncation deterministically (`StepBudget::over_budget()` reports `capped` on its first check), reproducing the hazard without a genuinely pathological fixture.

use pg_conformance_fixtures::{discover, Root};
use pg_foma::backend_optimizer::{pareto_frontier, select_confirmed, Certification};
use pg_foma::backend_registry::{MaterializerContext, Registry};
use pg_foma::backend_runtime::{evaluate_plans, RuntimeBudget};
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;
use std::time::Duration;

fn materialize_plans(
    grammar: &pg_grammar::model::Grammar,
) -> Vec<pg_foma::enumerate::LoweredCandidate> {
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let prules = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phonology = PhonologyProbe::new(grammar);
    let baseline = enumerate_default(grammar, &alphabet, &prules, phonology.as_ref());
    let candidates = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar,
            baseline: &baseline,
        })
        .expect("materialization must succeed");
    candidates.into_iter().map(|(_, p)| p).collect()
}

#[test]
fn a_capped_oracle_yields_an_explicit_truncation_never_a_word_mismatch_or_a_confirmation() {
    let fixtures = discover();
    let fixture = fixtures
        .iter()
        .find(|f| f.root == Root::Staging && f.name == "backend-gated-generic")
        .expect("missing staged fixture backend-gated-generic");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("staged fixture must load");
    let words: Vec<String> = fixture
        .load_words_yaml()
        .words
        .iter()
        .map(|w| w.word.clone())
        .collect();
    assert!(!words.is_empty(), "fixture must carry corpus words");

    let plans = materialize_plans(&grammar);
    assert!(!plans.is_empty(), "must materialize at least one candidate");

    // Sanity check so the real assertion below can't be misread as "this fixture doesn't confirm anything": under default oracle bounds, at least one candidate must reach `FullHcConfirmed`.
    let unbounded_enough = evaluate_plans(&grammar, &plans, &words, RuntimeBudget::default())
        .expect("the oracle liveness net / memory ceiling must not trip on this fixture");
    assert!(
        unbounded_enough
            .iter()
            .any(|e| e.certification.selectable()),
        "sanity check failed -- backend-gated-generic should confirm under the default oracle \
         bounds, otherwise this test can't tell a real regression from a fixture that never \
         confirmed in the first place: {:?}",
        unbounded_enough
            .iter()
            .map(|e| &e.certification)
            .collect::<Vec<_>>()
    );

    // The pin: `evaluate_plans` must detect every word's capped oracle before `certify_corpus` runs and report every candidate's certification as the same explicit, non-certifying truncation.
    let capped = evaluate_plans(
        &grammar,
        &plans,
        &words,
        RuntimeBudget {
            oracle_step_cap: Some(0),
            ..RuntimeBudget::default()
        },
    )
    .expect("the oracle liveness net / memory ceiling must not trip on this fixture");
    assert!(!capped.is_empty());
    for evaluation in &capped {
        assert!(
            !evaluation.certification.selectable(),
            "a capped oracle's partial `expected` must never be allowed to certify a candidate, \
             got {:?}",
            evaluation.certification
        );
        match &evaluation.certification {
            Certification::Truncated { stage, .. } => {
                assert_eq!(
                    stage, "oracle-capped",
                    "capped oracle certification named the wrong stage: {stage:?}"
                );
            }
            other => panic!(
                "a capped oracle must report Certification::Truncated{{stage: \"oracle-capped\"}}, \
                 not {other:?} -- an IdentityMismatch here would mean the \
                 truncated oracle's partial analyses leaked into certify_corpus as if they were a \
                 complete ground truth"
            ),
        }
    }
}
#[test]
fn a_mixed_complete_and_capped_oracle_cannot_certify_the_complete_subset() {
    let fixture = discover()
        .into_iter()
        .find(|f| f.root == Root::Staging && f.name == "backend-gated-generic")
        .expect("missing staged fixture backend-gated-generic");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("staged fixture must load");
    let words = vec!["tulik".to_string(), "menulik".to_string()];
    let cap = 5;

    let morpher =
        pg_parse::Morpher::new(&grammar, cap).with_word_timeout(Some(Duration::from_secs(2)));
    let complete = morpher.parse_word(&words[0]);
    let capped = morpher.parse_word(&words[1]);
    assert!(
        !complete.capped && !complete.timed_out,
        "complete: capped={}, timed_out={}, steps={}",
        complete.capped,
        complete.timed_out,
        complete.steps
    );
    assert!(
        capped.capped && !capped.timed_out,
        "capped: capped={}, timed_out={}, steps={}",
        capped.capped,
        capped.timed_out,
        capped.steps
    );

    let plans = materialize_plans(&grammar);
    let evaluations = evaluate_plans(
        &grammar,
        &plans,
        &words,
        RuntimeBudget {
            oracle_step_cap: Some(cap),
            ..RuntimeBudget::default()
        },
    )
    .expect("the oracle liveness net / memory ceiling must not trip on this fixture");
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
        let Certification::Truncated { ref stage, .. } = evaluation.certification else {
            panic!(
                "mixed complete/capped oracle must not certify its comparable subset: {evaluation:?}"
            );
        };
        assert_eq!(stage, "oracle-capped");
        let value = serde_json::to_value(&evaluation.certification)
            .expect("truncation evidence must be serializable");
        let corpus = value
            .get("corpus")
            .expect("truncation must preserve corpus completeness evidence");
        assert_eq!(corpus["requested"], 2);
        assert_eq!(corpus["included"], 1);
        assert_eq!(corpus["excluded"], 1);
        assert_ne!(corpus["requested_hash"], corpus["included_hash"]);
        assert_ne!(corpus["requested_hash"], corpus["excluded_hash"]);
        assert_eq!(corpus["exclusions"][0]["requested_ordinal"], 1);
        assert_eq!(corpus["exclusions"][0]["word"], "menulik");
        assert_eq!(corpus["exclusions"][0]["reason"], "oracle-capped");
    }
}
