//! **The two fixtures that used to kill the test process now return a typed verdict.**
//!
//! # What actually happened, and what the filed symptom got wrong
//!
//! Two fixtures — `machine:edge-cases/deep-optional-affix-nesting` and
//! `staging:edge-cases/recipe-template-generic` — aborted the whole test PROCESS instead of failing a
//! test: `memory allocation of 52 bytes failed`, then exit `0xc0000409`. They are **one grammar**:
//! `diff` reports the two `grammar.xml` files differ only in their `<Name>` element (and the staged
//! copy's trailing newline), and the two `words.yaml` files only in their `language:` line. So there
//! was never a question of two independent bugs here.
//!
//! `0xc0000409` is `STATUS_STACK_BUFFER_OVERRUN`, which is what Rust's stack-overflow handler
//! produces — and it is ALSO what MSVC's `abort()` produces, via `__fastfail`. The message decides
//! which, and the message was the ALLOCATOR's (`memory allocation of N bytes failed`), not the stack
//! handler's (`thread '...' has overflowed its stack`). This was heap exhaustion against procgov's
//! 19GB job-object committed-memory cap, **not** unbounded recursion. Three measurements pin that
//! (all now folded into the tests below or into
//! `pg_foma::compose_budget::DEFAULT_EVALUATION_APPLY_PATH_BUDGET`'s own doc):
//!
//! - the three corpus words parse UNCAPPED (`Morpher::new(g, usize::MAX)`) in **0.185s**, so no
//!   recursion in the engine is unbounded on this grammar;
//! - the plan-composed net BUILDS in **0.027s**, so the compiler is not where the memory goes;
//! - the tuned whole-grammar compiler proposes AND confirms all three words in **0.597s**.
//!
//! Only the plan-composed PROPOSE dies, and it dies enumerating `apply_up` paths: measured
//! **2,985,984 = 12^6** raw paths for `xxxxxxk` (against 924 real analyses), which implies
//! `12^12 = 8,916,100,448,256` for `xxxxxxxxxxxxk` — the word the process never got past. The
//! recursion is DEPTH-bounded (by each rule's `multipleApplication`, the DTD default 1, and by the
//! template's descending slot index); the search's OUTPUT is what is unbounded in magnitude.
//!
//! # What is asserted here
//!
//! That the aborting shape is now a `Certification::ResourceBreach`. A breach is not selectable, so
//! this cannot certify anything wrongly; and it is a REFUSAL, not a truncation — the refused word is
//! never confirmed and its partial proposal set never reaches the oracle comparison, so it cannot
//! manufacture the recall failure a truncated proposal set would.

use pg_conformance_fixtures::{discover, FixtureRef, Root};
use pg_foma::compose_budget::{
    DEFAULT_EVALUATION_APPLY_CANDIDATE_BUDGET, DEFAULT_EVALUATION_APPLY_PATH_BUDGET,
};
use pg_foma::enumerate::{enumerate_default, CandidateRole, LoweredCandidate};
use pg_foma::executable_candidate::LoweringAdapter;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::recipe_optimizer::Certification;
use pg_foma::recipe_runtime::{
    evaluate_plans_with_cache, RunEvaluationCache, RuntimeBudget, RuntimeEvaluation,
};
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};

/// The two fixtures the process used to die on. Byte-identical grammars bar `<Name>` — verified by
/// `the_two_aborting_fixtures_are_one_grammar` below rather than asserted in prose.
const REFUSING_FIXTURES: [&str; 2] = ["deep-optional-affix-nesting", "recipe-template-generic"];

/// A fixture that must NOT be refused at the default envelope — the negative control without which
/// a gate that refused everything would read as a pass.
const CONTROL_FIXTURE: &str = "compounding-non-recursive";

fn surface_table(grammar: &Grammar) -> &pg_grammar::chardef::CharDefTable {
    let stratum = grammar
        .strata
        .last()
        .expect("a loaded grammar always has at least one stratum");
    &grammar.char_tables[stratum.table.0 as usize]
}

fn baseline_only(grammar: &Grammar) -> Vec<LoweredCandidate> {
    let alphabet = SegAlphabet::new(surface_table(grammar));
    let prules: Vec<&PhonRuleDef> = pg_foma::enumerate::prules_in_order(grammar);
    let phonology = PhonologyProbe::new(grammar);
    vec![LoweredCandidate {
        label: "apply-path-refusal-gate-baseline",
        plan: enumerate_default(grammar, &alphabet, &prules, phonology.as_ref()),
        adapter: LoweringAdapter::ControllablePlanCompose,
        role: CandidateRole::Baseline,
    }]
}

fn fixture(name: &str) -> FixtureRef {
    discover()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| {
            panic!("fixture {name} is not discoverable -- machine submodule initialized?")
        })
}

/// Evaluate one fixture's baseline candidate at `budget`. `None` means oracle preparation faulted,
/// which is a "could not look" and never folded into an assertion as if it were a verdict.
fn evaluate(name: &str, budget: RuntimeBudget) -> Option<RuntimeEvaluation> {
    let f = fixture(name);
    let grammar = pg_grammar::load(&f.load_grammar_xml()).expect("fixture grammar must load");
    let words: Vec<String> = f
        .load_words_yaml()
        .words
        .into_iter()
        .map(|w| w.word)
        .collect();
    assert!(!words.is_empty(), "{name}: fixture declares no corpus word");
    let plans = baseline_only(&grammar);
    let mut cache = RunEvaluationCache::prepare(&grammar, &words, budget).ok()?;
    Some(
        evaluate_plans_with_cache(&grammar, &plans, &words, budget, &mut cache)
            .pop()
            .expect("one candidate in, one evaluation out"),
    )
}

/// The `dimension` string a per-word apply refusal reports, or `None` for any other certification.
fn apply_refusal_dimension(evaluation: &RuntimeEvaluation) -> Option<(String, u64, u64)> {
    match &evaluation.certification {
        Certification::ResourceBreach {
            dimension,
            value,
            limit,
        } if dimension.starts_with("per-word apply ") => Some((dimension.clone(), *value, *limit)),
        _ => None,
    }
}

/// The claim the whole exclusion list rested on: these are two names for one grammar.
///
/// Asserted rather than described because the ONLY thing that made them look like two independent
/// bugs was that they are checked in under different names in different roots. If a future edit
/// makes them genuinely different grammars, this fails and the shared diagnosis in this file's module
/// doc stops being licensed.
#[test]
fn the_two_aborting_fixtures_are_one_grammar() {
    let machine = fixture(REFUSING_FIXTURES[0]);
    let staging = fixture(REFUSING_FIXTURES[1]);
    assert_eq!(machine.root, Root::Machine, "expected the upstream copy");
    assert_eq!(staging.root, Root::Staging, "expected the staged copy");
    let a = pg_grammar::load(&machine.load_grammar_xml()).expect("machine copy must load");
    let b = pg_grammar::load(&staging.load_grammar_xml()).expect("staging copy must load");
    assert_eq!(
        a.mrules.len(),
        b.mrules.len(),
        "the two copies must declare the same morphological rules"
    );
    assert_eq!(
        a.templates.len(),
        b.templates.len(),
        "the two copies must declare the same affix templates"
    );
    assert_eq!(
        a.templates[0].slots.len(),
        b.templates[0].slots.len(),
        "the two copies must declare the same slot count"
    );
    // The shape the whole diagnosis turns on: 12 slots, 12 rules, every rule at the DTD default
    // `multipleApplication = 1`. `12^k` paths for a k-`x` word is a claim ABOUT this shape, so pin it.
    assert_eq!(a.templates[0].slots.len(), 12);
    assert!(
        a.mrules.iter().all(|m| m.max_apps() == 1),
        "every rule must be at multipleApplication = 1 -- the composed net proposing the same rule \
         repeatedly is exactly the over-generation this gate bounds"
    );
}

/// **The headline.** Both fixtures return, and both return a typed refusal naming the dimension.
///
/// Before this envelope existed, this test could not be written at all: the process died inside
/// `evaluate_plans` and no assertion downstream of it ever ran.
#[test]
fn both_formerly_aborting_fixtures_now_return_a_typed_apply_refusal() {
    for name in REFUSING_FIXTURES {
        let Some(evaluation) = evaluate(name, RuntimeBudget::default()) else {
            panic!("{name}: oracle preparation faulted -- cannot say anything about the candidate");
        };
        let Some((dimension, value, limit)) = apply_refusal_dimension(&evaluation) else {
            panic!(
                "{name}: expected a per-word apply ResourceBreach, got {:?}",
                evaluation.certification
            );
        };
        eprintln!("{name}: REFUSED -- {dimension} value={value} limit={limit}");
        assert_eq!(
            limit,
            DEFAULT_EVALUATION_APPLY_PATH_BUDGET.min(DEFAULT_EVALUATION_APPLY_CANDIDATE_BUDGET)
                as u64,
            "{name}: the refusal must report the calibrated default it was measured against"
        );
        assert_eq!(
            value,
            limit + 1,
            "{name}: the apply budget is checked one past the limit (ApplyOutcome::Incomplete's own \
             convention), so a trip always reports limit + 1"
        );
        assert!(
            !evaluation.certification.selectable(),
            "{name}: a resource refusal must never be selectable"
        );
    }
}

/// The magnitude that makes the refusal necessary rather than merely tidy.
///
/// `12^6 = 2,985,984` raw paths for `xxxxxxk` against `C(12,6) = 924` real analyses is a 3,231x
/// over-generation, and `12^12` for the 12-`x` word is ~8.9 x 10^12 — which is why no larger buffer
/// fixes this. Measured here by raising the envelope to just above the k=6 figure so the k=6 word
/// completes and the 12-`x` word is the one that trips, proving the growth is in the word length and
/// not a fixed cost.
#[test]
fn the_refused_magnitude_grows_with_the_word_and_not_with_the_grammar() {
    let name = REFUSING_FIXTURES[0];
    // Just above 12^6 = 2,985,984: the k=6 word now fits, the k=12 word cannot.
    let budget = RuntimeBudget {
        apply_path_budget: Some(3_000_000),
        apply_candidate_budget: Some(3_000_000),
        ..RuntimeBudget::default()
    };
    let Some(evaluation) = evaluate(name, budget) else {
        panic!("{name}: oracle preparation faulted");
    };
    let Some((dimension, value, limit)) = apply_refusal_dimension(&evaluation) else {
        panic!(
            "{name}: at a 3,000,000 envelope the 12-x word must still be refused, got {:?}",
            evaluation.certification
        );
    };
    eprintln!("{name}: at a raised envelope -- {dimension} value={value} limit={limit}");
    assert_eq!(limit, 3_000_000);
    assert!(
        dimension.contains("xxxxxxxxxxxxk"),
        "the refusal must name the 12-x word, not the 6-x one: {dimension}"
    );
    assert!(
        evaluation.score.raw_paths >= 2_985_984,
        "the 6-x word alone yields 12^6 = 2,985,984 raw paths and must be accounted for in the \
         score before the 12-x word trips; got {}",
        evaluation.score.raw_paths
    );
}

/// Negative control, two ways: an ordinary fixture is NOT refused at the default envelope, and the
/// SAME fixture IS refused at an absurdly small one.
///
/// Without the first half this gate would pass if the envelope refused everything. Without the
/// second half it would pass if the mechanism were inert — the exact failure mode this repository has
/// already paid for once (a work budget "merged once, never fired, and reverted").
#[test]
fn an_ordinary_fixture_passes_the_default_envelope_and_trips_a_tiny_one() {
    let Some(at_default) = evaluate(CONTROL_FIXTURE, RuntimeBudget::default()) else {
        panic!("{CONTROL_FIXTURE}: oracle preparation faulted");
    };
    assert!(
        apply_refusal_dimension(&at_default).is_none(),
        "{CONTROL_FIXTURE} must not be refused at the calibrated default -- got {:?}",
        at_default.certification
    );

    let Some(at_one) = evaluate(
        CONTROL_FIXTURE,
        RuntimeBudget {
            apply_path_budget: Some(1),
            apply_candidate_budget: Some(1),
            ..RuntimeBudget::default()
        },
    ) else {
        panic!("{CONTROL_FIXTURE}: oracle preparation faulted");
    };
    let refusal = apply_refusal_dimension(&at_one);
    assert!(
        refusal.is_some(),
        "{CONTROL_FIXTURE} at a 1-path envelope must be REFUSED, not quietly measured on a \
         one-path proposal set -- got {:?}",
        at_one.certification
    );
}

/// The `None` / `Some(usize::MAX)` / `Some(n)` resolution, checked at the seam rather than trusted.
///
/// `None` meaning "the default" and not "unbounded" is the whole point of the field, and it is the
/// same inversion `oracle_step_cap` already documents. A silent regression to `None == unbounded`
/// would restore the process abort and nothing else in the suite would notice.
#[test]
fn none_resolves_to_the_calibrated_default_and_only_usize_max_opts_out() {
    let default = RuntimeBudget::default().resolved_apply_budget();
    assert_eq!(
        default.path_cap(),
        Some(DEFAULT_EVALUATION_APPLY_PATH_BUDGET),
        "an unset apply_path_budget must resolve to the calibrated default, never to unbounded"
    );
    assert_eq!(
        default.candidate_cap(),
        Some(DEFAULT_EVALUATION_APPLY_CANDIDATE_BUDGET)
    );

    let opted_out = RuntimeBudget {
        apply_path_budget: Some(usize::MAX),
        apply_candidate_budget: Some(usize::MAX),
        ..RuntimeBudget::default()
    }
    .resolved_apply_budget();
    assert_eq!(
        opted_out.path_cap(),
        None,
        "Some(usize::MAX) is the explicit, greppable opt-out and must resolve to unbounded"
    );
    assert_eq!(opted_out.candidate_cap(), None);

    let explicit = RuntimeBudget {
        apply_path_budget: Some(7),
        apply_candidate_budget: Some(9),
        ..RuntimeBudget::default()
    }
    .resolved_apply_budget();
    assert_eq!(explicit.path_cap(), Some(7));
    assert_eq!(explicit.candidate_cap(), Some(9));
}
