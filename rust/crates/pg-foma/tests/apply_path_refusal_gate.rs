//! Two fixtures used to abort the test process instead of failing; the crash diagnosis and the
//! path-growth derivation live in `docs/research/pg-foma-apply-path-refusal-design-notes.md`.

use pg_conformance_fixtures::{discover, FixtureRef, Root};
use pg_foma::compose_budget::{
    DEFAULT_EVALUATION_APPLY_CANDIDATE_BUDGET, DEFAULT_EVALUATION_APPLY_PATH_BUDGET,
};
use pg_foma::enumerate::{enumerate_default, CandidateRole, LoweredCandidate};
use pg_foma::junctions::PhonologyProbe;
use pg_foma::lowering_adapter::LoweringAdapter;
use pg_foma::recipe_optimizer::Certification;
use pg_foma::recipe_runtime::{
    evaluate_plans_with_cache, RunEvaluationCache, RuntimeBudget, RuntimeEvaluation,
};
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};

/// The two fixtures the process used to die on -- byte-identical grammars but for `<Name>`.
const REFUSING_FIXTURES: [&str; 2] = ["deep-optional-affix-nesting", "recipe-template-generic"];

/// A fixture that must not be refused at the default envelope -- the negative control against a gate that refuses everything.
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

/// Evaluates one fixture's baseline candidate at `budget`; `None` means oracle preparation faulted, not a verdict.
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

/// These two fixtures are the same grammar under different names; the shared diagnosis depends on it.
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
    // 12 slots, 12 rules, each at multipleApplication = 1 -- the shape the 12^k path-count claim depends on.
    assert_eq!(a.templates[0].slots.len(), 12);
    assert!(
        a.mrules.iter().all(|m| m.max_apps() == 1),
        "every rule must be at multipleApplication = 1 -- the composed net proposing the same rule \
         repeatedly is exactly the over-generation this gate bounds"
    );
}

/// Both fixtures now return a typed refusal naming the dimension, instead of aborting the process.
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
/// See `docs/research/pg-foma-apply-path-refusal-design-notes.md`, "`the_refused_magnitude_grows_with_the_word_and_not_with_the_grammar`: the over-generation magnitude".
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

/// Both directions matter: refusing nothing would hide an inert gate; refusing everything would hide a broken one.
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

/// `None` resolves to the calibrated default, not unbounded -- the same inversion `oracle_step_cap` uses.
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
