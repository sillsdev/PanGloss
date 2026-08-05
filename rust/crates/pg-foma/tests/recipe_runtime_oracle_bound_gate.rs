//! Pins the fix for the deep-truncation-chain-stress-grammar pilot hang: `recipe_runtime::
//! evaluate_plans` used to compute its ground-truth oracle with
//! `pg_parse::Morpher::new(grammar, usize::MAX)` and no
//! `.with_word_timeout(..)` -- both axes that could stop a pathological word were disabled, so one
//! bad corpus word hung the whole evaluator call forever, before any FST build/propose/confirm work
//! ever ran.
//!
//! The fix threads a finite default step cap + wall-clock deadline into that `Morpher` and, more
//! importantly, makes a step-capped oracle result an explicit non-certifying
//! `Certification::Truncated{stage: "oracle-capped"}` rather than letting a KNOWN
//! PARTIAL ground truth reach `certify_corpus`. That second half is the actual correctness property
//! this gate pins: a partial `expected` compared against a real, untruncated FST result can
//! manufacture a bogus `IdentityMismatch`/`MultiplicityMismatch` (a phantom "grammar/FST bug" that is
//! really an oracle-truncation artifact), or -- worse -- let a genuinely incomplete candidate look
//! right against an equally truncated oracle and wrongly certify.
//!
//! `oracle_step_cap: Some(0)` is used to force the truncation deterministically, independent of any
//! particular grammar or word: `pg_rules::stratum::StepBudget::over_budget()` reports `capped` on its
//! very FIRST check (`0 >= 0`), so this reproduces the hazard without needing a genuinely
//! pathological (and therefore slow-to-run-in-CI) fixture. It is now also the ONLY route: the
//! wall clock has been demoted to a liveness net whose trip aborts preparation as a typed
//! `OraclePreparationFault` and can no longer produce an exclusion at all -- see
//! `deterministic_eligibility_gate.rs`.

use pg_conformance_fixtures::{discover, Root};
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::recipe_optimizer::{pareto_frontier, select_confirmed, Certification};
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::recipe_runtime::{evaluate_plans, RuntimeBudget};
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
        .find(|f| f.root == Root::Staging && f.name == "recipe-gated-generic")
        .expect("missing staged fixture recipe-gated-generic");
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

    // Sanity check, so a failure of the real assertion below can never be misread as "this fixture
    // just doesn't confirm anything": with the DEFAULT (generous, finite) oracle bounds, at least one
    // candidate reaches `FullHcConfirmed` -- same fixture/property
    // `recipe_runtime_net_is_queryable_gate.rs::the_evaluator_confirms_a_wholly_in_scope_grammar`
    // already pins.
    let unbounded_enough = evaluate_plans(&grammar, &plans, &words, RuntimeBudget::default())
        .expect("the oracle liveness net / memory ceiling must not trip on this fixture");
    assert!(
        unbounded_enough
            .iter()
            .any(|e| e.certification.selectable()),
        "sanity check failed -- recipe-gated-generic should confirm under the default oracle \
         bounds, otherwise this test can't tell a real regression from a fixture that never \
         confirmed in the first place: {:?}",
        unbounded_enough
            .iter()
            .map(|e| &e.certification)
            .collect::<Vec<_>>()
    );

    // THE PIN. `oracle_step_cap: Some(0)` forces every word's ground-truth `Morpher::parse_word` call
    // to report `capped: true` (see module doc for why this is deterministic). The guard in
    // `evaluate_plans` must detect that BEFORE `certify_corpus` ever runs and report every
    // candidate's certification as the same explicit, non-certifying truncation.
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
                 not {other:?} -- an IdentityMismatch/MultiplicityMismatch here would mean the \
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
        .find(|f| f.root == Root::Staging && f.name == "recipe-gated-generic")
        .expect("missing staged fixture recipe-gated-generic");
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
