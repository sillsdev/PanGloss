//! Pins that `evaluate_plans` scores each candidate on a net with boundary cleanup applied, not a pre-finish one.
//! Why corpus-gated: `docs/research/pg-foma-recipe-runtime-queryable-gate-notes.md`.

use pg_conformance_fixtures::{corpus, discover, Root};
use pg_foma::backend_optimizer::Certification;
use pg_foma::backend_registry::{MaterializerContext, Registry};
use pg_foma::backend_runtime::{evaluate_plans, RuntimeBudget};
use pg_foma::build::unbuildable_markers;
use pg_foma::capability::{
    compose_envelope_across_strategies, default_grammar_wide_checks, default_registry,
    CapabilityContributions,
};
use pg_foma::emit;
use pg_foma::enumerate::enumerate_default;
use pg_foma::enumerate::CandidateRole;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::junctions::PhonologyProbe;
use std::time::Instant;

/// Returns each candidate's evaluation paired with the strategy that produced it and its declared `CandidateRole`; the baseline is identified by `LoweredCandidate::role`, never by position.
fn materialize_and_evaluate(
    grammar: &pg_grammar::model::Grammar,
    words: &[String],
    budget: RuntimeBudget,
) -> Vec<(
    pg_foma::enumerate::EmissionStrategy,
    CandidateRole,
    pg_foma::backend_runtime::RuntimeEvaluation,
)> {
    let started = Instant::now();
    let prules = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phonology = PhonologyProbe::new(grammar);
    let baseline = enumerate_default(grammar, &prules, phonology.as_ref());
    eprintln!(
        "runtime-net phase: enumerated baseline in {:?}",
        started.elapsed()
    );
    let materialize_started = Instant::now();
    let candidates = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar,
            baseline: &baseline,
        })
        .expect("materialization must succeed");
    eprintln!(
        "runtime-net phase: materialized {} candidate(s) in {:?}",
        candidates.len(),
        materialize_started.elapsed()
    );
    let plans: Vec<_> = candidates.into_iter().map(|(_, p)| p).collect();
    assert!(!plans.is_empty(), "must materialize at least one candidate");
    let declared: Vec<_> = plans.iter().map(|p| (p.strategy(), p.role)).collect();
    eprintln!(
        "runtime-net phase: evaluating {declared:?} over {} word(s)",
        words.len()
    );
    let evaluate_started = Instant::now();
    let evaluations = evaluate_plans(grammar, &plans, words, budget)
        .expect("the oracle liveness net / memory ceiling must not trip on this fixture");
    eprintln!(
        "runtime-net phase: evaluated candidates in {:?}",
        evaluate_started.elapsed()
    );
    declared
        .into_iter()
        .zip(evaluations)
        .map(|((strategy, role), evaluation)| (strategy, role, evaluation))
        .collect()
}

/// Distinguishes candidate construction from per-word work on a diagnostic-sized corpus slice.
#[test]
#[ignore = "needs the private corpus at samples/data/indonesian-hc.xml; run with --include-ignored"]
fn corpus_indonesian_first_word_runtime_phases_complete() {
    let grammar_path = corpus::require("indonesian-hc.xml");
    let words_path = corpus::require("indonesian-words.txt");
    let grammar = pg_grammar::load(&std::fs::read_to_string(&grammar_path).expect("read grammar"))
        .expect("indonesian grammar must load");
    let words = std::fs::read_to_string(words_path)
        .expect("read words")
        .lines()
        .map(str::trim)
        .find(|word| !word.is_empty())
        .map(|word| vec![word.to_owned()])
        .expect("Indonesian corpus has a word");

    let evaluations = materialize_and_evaluate(
        &grammar,
        &words,
        RuntimeBudget {
            build: Some(10_000_000_000),
            ..RuntimeBudget::default()
        },
    );
    assert!(!evaluations.is_empty());
    corpus::record_cases("corpus_indonesian_first_word_runtime_phases_complete", 1);
}

/// Names Indonesian registry candidates without constructing their networks.
#[test]
#[ignore = "needs the private corpus at samples/data/indonesian-hc.xml; run with --include-ignored"]
fn corpus_indonesian_registry_candidates_are_named_before_build() {
    let grammar_path = corpus::require("indonesian-hc.xml");
    let grammar = pg_grammar::load(&std::fs::read_to_string(&grammar_path).expect("read grammar"))
        .expect("indonesian grammar must load");
    let prules = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phonology = PhonologyProbe::new(&grammar);
    let baseline = enumerate_default(&grammar, &prules, phonology.as_ref());
    let semantics = GrammarSemantics::derive(&grammar);
    let registry = default_registry();
    let grammar_wide = default_grammar_wide_checks();
    let contributions = CapabilityContributions::new(&registry, &grammar_wide);
    let envelope = compose_envelope_across_strategies(&semantics, &baseline, &contributions);
    for verdict in envelope.verdicts() {
        eprintln!(
            "capability strategy={:?} decision={:?}",
            verdict.strategy, verdict.decision
        );
    }
    let candidates = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar: &grammar,
            baseline: &baseline,
        })
        .expect("materialization must succeed");

    let (composite, composite_rules, roots) = emit::composite_scale_hint(&grammar);
    let structural_rules = emit::composite_candidate_rules(&grammar)
        .structural_candidates
        .len();
    eprintln!(
        "scale composite={composite} composite_rules={composite_rules} structural_rules={structural_rules} roots={roots}"
    );

    for (ordinal, (instance, candidate)) in candidates.iter().enumerate() {
        eprintln!(
            "candidate[{ordinal}] family={} parameters={:?} label={} adapter={:?} role={:?}",
            instance.family_id,
            instance.parameters,
            candidate.label,
            candidate.adapter,
            candidate.role
        );
    }
    assert!(!candidates.is_empty());
    corpus::record_cases(
        "corpus_indonesian_registry_candidates_are_named_before_build",
        candidates.len(),
    );
}

/// Exercises the one plan-composed route production retains for Indonesian.
#[test]
#[ignore = "needs the private corpus at samples/data/indonesian-hc.xml; run with --include-ignored"]
fn corpus_indonesian_plan_composed_baseline_completes() {
    let grammar_path = corpus::require("indonesian-hc.xml");
    let words_path = corpus::require("indonesian-words.txt");
    let grammar = pg_grammar::load(&std::fs::read_to_string(&grammar_path).expect("read grammar"))
        .expect("indonesian grammar must load");
    let word = std::fs::read_to_string(words_path)
        .expect("read words")
        .lines()
        .map(str::trim)
        .find(|word| !word.is_empty())
        .expect("Indonesian corpus has a word")
        .to_owned();
    let prules = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phonology = PhonologyProbe::new(&grammar);
    let baseline = enumerate_default(&grammar, &prules, phonology.as_ref());
    let candidates = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar: &grammar,
            baseline: &baseline,
        })
        .expect("materialization must succeed");
    let plan = candidates
        .into_iter()
        .map(|(_, candidate)| candidate)
        .find(|candidate| candidate.is_baseline())
        .expect("registry must retain the plan-composed baseline");
    let evaluations = evaluate_plans(
        &grammar,
        &[plan],
        &[word],
        RuntimeBudget {
            build: Some(10_000_000_000),
            ..RuntimeBudget::default()
        },
    )
    .expect("the oracle liveness net / memory ceiling must not trip");
    assert_eq!(evaluations.len(), 1);
    corpus::record_cases("corpus_indonesian_plan_composed_baseline_completes", 1);
}

/// The pin for the finish-step defect. Fail-closed on a missing corpus: silently returning success while testing nothing is exactly the false-success path this guards against.
#[test]
#[ignore = "needs the private corpus at samples/data/indonesian-hc.xml; run with --include-ignored"]
fn corpus_indonesian_confirms_after_the_finish_step() {
    // `corpus::require` (not a skip-if-absent guard) so a missing corpus fails rather than reporting a pass it did not earn.
    let grammar_path = corpus::require("indonesian-hc.xml");
    let words_path = corpus::require("indonesian-words.txt");

    let grammar = pg_grammar::load(&std::fs::read_to_string(&grammar_path).expect("read grammar"))
        .expect("indonesian grammar must load");
    let words: Vec<String> = std::fs::read_to_string(&words_path)
        .expect("read words")
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect();
    assert!(!words.is_empty());

    let evaluations: Vec<_> = materialize_and_evaluate(&grammar, &words, RuntimeBudget::default())
        .into_iter()
        .map(|(_, _, e)| e)
        .collect();
    let confirmed = evaluations
        .iter()
        .filter(|e| e.certification.selectable())
        .count();
    let proposals: u64 = evaluations.iter().map(|e| e.score.proposals).sum();

    assert!(
        confirmed > 0,
        "no candidate reached FullHcConfirmed on the Indonesian corpus (proposals={proposals}). \
         Pre-fix this read 0 of 3 confirmed with a `merasa` multiplicity mismatch, because the net \
         still carried uflexc's boundary tokens -- check that \
         `build::finish_controllable_net`'s cleanup+reminimize is still applied in \
         `backend_runtime::evaluate_plans`. Certifications: {:?}",
        evaluations
            .iter()
            .map(|e| &e.certification)
            .collect::<Vec<_>>()
    );
    assert!(
        proposals > 0,
        "confirmed with zero proposals is a vacuous pass"
    );
    // The managed front end rejects a successful cargo exit whose total executed-case count is zero.
    corpus::record_cases(
        "corpus_indonesian_confirms_after_the_finish_step",
        words.len(),
    );
}

/// Non-vacuous on staged fixtures: this plan carries an out-of-scope marker and confirms anyway, evidence marker presence alone must never disqualify a candidate.
#[test]
fn the_evaluator_confirms_a_wholly_in_scope_grammar() {
    let fixtures = discover();
    let fixture = fixtures
        .iter()
        .find(|f| f.root == Root::Staging && f.name == "backend-gated-generic")
        .expect("missing staged fixture backend-gated-generic");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");

    let words: Vec<String> = fixture
        .load_words_yaml()
        .words
        .iter()
        .map(|w| w.word.clone())
        .collect();
    let evaluations: Vec<_> = materialize_and_evaluate(&grammar, &words, RuntimeBudget::default())
        .into_iter()
        .map(|(_, _, e)| e)
        .collect();
    let confirmed = evaluations
        .iter()
        .filter(|e| e.certification.selectable())
        .count();
    assert!(
        confirmed > 0,
        "no candidate confirmed on a wholly-in-scope grammar: {:?}",
        evaluations
            .iter()
            .map(|e| &e.certification)
            .collect::<Vec<_>>()
    );
    for e in evaluations.iter().filter(|e| e.certification.selectable()) {
        assert!(e.score.proposals > 0, "vacuous pass: {:?}", e.score);
        assert!(e.score.states > 0 && e.score.arcs > 0);
    }
}

/// A plan that advertises subtrees its compiler cannot build is refused before measurement.
#[test]
fn out_of_scope_marker_subtrees_are_attributed_not_blamed_on_the_grammar() {
    let fixtures = discover();
    let mut exercised = Vec::new();
    for fixture in fixtures.iter().filter(|f| f.root == Root::Staging) {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        let prules = grammar
            .strata
            .iter()
            .flat_map(|s| &s.prules)
            .map(|id| &grammar.prules[id.0 as usize])
            .collect::<Vec<_>>();
        let phonology = PhonologyProbe::new(&grammar);
        let plan = enumerate_default(&grammar, &prules, phonology.as_ref());
        let markers = unbuildable_markers(&plan);
        if markers.is_empty() {
            continue;
        }
        let words: Vec<String> = fixture
            .load_words_yaml()
            .words
            .iter()
            .map(|w| w.word.clone())
            .take(6)
            .collect();
        if words.is_empty() {
            continue;
        }
        for (strategy, _role, e) in
            materialize_and_evaluate(&grammar, &words, RuntimeBudget::default())
        {
            // Checked before the marker-attribution assertion: a whole-grammar strategy's own compiler builds the marker material rather than skipping it, so there is no compiler limitation to attribute here.
            if strategy.is_whole_grammar() {
                assert!(
                    !matches!(e.certification, Certification::Unsupported { .. }),
                    "{}: {strategy:?} builds the whole grammar, so its verdict must be the real \
                     measurement rather than an `Unsupported` limitation notice, got {:?}",
                    fixture.label(),
                    e.certification
                );
                continue;
            }
            assert!(
                matches!(e.certification, Certification::Unsupported { .. }),
                "{}: a PlanComposed candidate whose plan requires {markers:?} must be refused before \
                 build_controllable can silently omit those subtrees, got {:?}",
                fixture.label(),
                e.certification
            );
        }
        exercised.push(fixture.label());
    }
    assert!(
        !exercised.is_empty(),
        "no staged fixture exercised the marker-subtree path, so this gate proved nothing -- \
         repoint it at a fixture whose plan carries a composite/structural marker"
    );
}
