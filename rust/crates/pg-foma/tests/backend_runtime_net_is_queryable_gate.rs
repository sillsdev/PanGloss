//! Pins that `evaluate_plans` scores each candidate on a net with boundary cleanup applied, not a pre-finish one.
//! Why corpus-gated: `docs/research/pg-foma-recipe-runtime-queryable-gate-notes.md`.

use pg_conformance_fixtures::{corpus, discover, Root};
use pg_foma::backend_optimizer::Certification;
use pg_foma::backend_registry::{MaterializerContext, Registry};
use pg_foma::backend_runtime::{evaluate_plans, RuntimeBudget};
use pg_foma::build::unbuildable_markers;
use pg_foma::enumerate::enumerate_default;
use pg_foma::enumerate::CandidateRole;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;

/// Returns each candidate's evaluation paired with the strategy that produced it and its declared `CandidateRole`; the baseline is identified by `LoweredCandidate::role`, never by position.
fn materialize_and_evaluate(
    grammar: &pg_grammar::model::Grammar,
    words: &[String],
) -> Vec<(
    pg_foma::enumerate::EmissionStrategy,
    CandidateRole,
    pg_foma::backend_runtime::RuntimeEvaluation,
)> {
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
    let plans: Vec<_> = candidates.into_iter().map(|(_, p)| p).collect();
    assert!(!plans.is_empty(), "must materialize at least one candidate");
    let declared: Vec<_> = plans.iter().map(|p| (p.strategy(), p.role)).collect();
    let evaluations = evaluate_plans(grammar, &plans, words, RuntimeBudget::default())
        .expect("the oracle liveness net / memory ceiling must not trip on this fixture");
    declared
        .into_iter()
        .zip(evaluations)
        .map(|((strategy, role), evaluation)| (strategy, role, evaluation))
        .collect()
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

    let evaluations: Vec<_> = materialize_and_evaluate(&grammar, &words)
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
    let evaluations: Vec<_> = materialize_and_evaluate(&grammar, &words)
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

/// A candidate that full HC refused, whose plan needed subtrees `build_controllable` cannot build, must be reported as that limitation, not as a word-level mismatch that sends a reader hunting a phantom grammar bug.
#[test]
fn out_of_scope_marker_subtrees_are_attributed_not_blamed_on_the_grammar() {
    let fixtures = discover();
    let mut exercised = Vec::new();
    for fixture in fixtures.iter().filter(|f| f.root == Root::Staging) {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
        let prules = grammar
            .strata
            .iter()
            .flat_map(|s| &s.prules)
            .map(|id| &grammar.prules[id.0 as usize])
            .collect::<Vec<_>>();
        let phonology = PhonologyProbe::new(&grammar);
        let plan = enumerate_default(&grammar, &alphabet, &prules, phonology.as_ref());
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
        for (strategy, role, e) in materialize_and_evaluate(&grammar, &words) {
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
            if role.is_baseline() {
                // The baseline is routed to the tuned emit path, which can build those subtrees, so any failure here is a real result about a genuine network -- it must not be relabelled `Unsupported`.
                assert!(
                    !matches!(e.certification, Certification::Unsupported { .. }),
                    "{}: the baseline took the tuned emit path, so its verdict must be the real \
                     measurement rather than an `Unsupported` limitation notice, got {:?}",
                    fixture.label(),
                    e.certification
                );
                continue;
            }
            // Confirming is legitimate for a permutation too: the controllable builder does honour gate/union permutations.
            if e.certification.selectable() {
                continue;
            }
            // A permutation that failed here, whose plan needs subtrees the builder cannot construct, must be attributed rather than reported as a word-level grammar fault.
            assert!(
                matches!(e.certification, Certification::Unsupported { .. }),
                "{}: a non-baseline candidate that failed and whose plan required {markers:?} must be \
                 attributed as unhonourable, got {:?}",
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
