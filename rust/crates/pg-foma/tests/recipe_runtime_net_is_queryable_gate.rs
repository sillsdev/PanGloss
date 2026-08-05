//! The gate whose absence let a whole optimizer land on an unqueryable network.
//!
//! `recipe_runtime::evaluate_plans` builds each candidate through `crate::build::build_controllable`
//! — a `Plan` interpreter — then queries it through the production propose→confirm pipeline. Two
//! independent defects made every real-grammar measurement meaningless, and NOTHING in the suite
//! noticed:
//!
//!  1. **The mandatory finish step was missing.** `gate::compile_gated_grammar_with_budget`'s own doc
//!     states every caller further composing its result does so "with a boundary-cleanup net" and
//!     then needs its own final minimize. That step existed only as an open-coded copy inside
//!     `tests/p6_gate_parity.rs`, so the one *production* caller omitted it and scored every
//!     candidate against a net still carrying `uflexc`'s inter-morph boundary tokens.
//!     Measured on the Indonesian corpus: **0 of 3 candidates confirmed → 3 of 3**, proposals
//!     51 → 131, once `build::finish_controllable_net` was applied.
//!
//!  2. **Nothing cross-checked the two build paths.** `build.rs` proves `build_controllable`
//!     equivalent to `gate.rs`'s direct compile *for the controllable subtree* — precisely the
//!     equivalence that cannot catch this, since both sides of it are pre-finish nets.
//!
//! # A measured limitation of this gate, stated rather than hidden
//! Defect (1) is **not reproducible on any checked-in synthetic fixture today.** That was verified,
//! not assumed: with the finish step bypassed and then restored, every staged fixture declaring
//! `Boundary` char-defs (`guesser-pattern-root-fallback`, `recipe-ordered-generic`,
//! `recipe-strata-generic`) produced byte-identical proposal counts, confirmation counts, and state
//! counts in both states. Their corpora are too shallow for an inter-morph boundary token to ever
//! block a query, so an assertion over them would pass with the fix reverted — a vacuous gate, which
//! is worse than none. The real pin therefore lives in `corpus_indonesian_confirms_after_the_finish_step`,
//! which needs the private corpus.
//!
//! **Follow-up owed:** a synthetic fixture reproducing the boundary-token pathology would move this
//! pin into CI. The blocker was not knowing what such a fixture must contain; that has now been
//! measured, so it is authorable. Emitting each grammar's `uflexc` lexc and counting lines carrying a
//! boundary token gives:
//!
//! | grammar | boundary tokens | lexc lines with one | continuation class |
//! |---|---:|---:|---|
//! | `indonesian` (DOES reproduce) | 3 | 7 | `PrefixOrRoot` |
//! | `recipe-ordered-generic` | 1 | 1 | `SuffixOrEnd` |
//! | `guesser-pattern-root-fallback` | 1 | 1 | `SuffixOrEnd` |
//! | `recipe-strata-generic` | 1 | 0 | never emitted |
//!
//! So the property is NOT "declares a `BoundaryDefinition`" -- every staged fixture above declares
//! one and none reproduces the defect. It is that a morph's own emitted UNDERLYING text carries a
//! boundary token in the **prefix** chain, so a multi-morph path contains a boundary the surface form
//! never does, and `apply_up` on a plain surface query cannot traverse it until the cleanup compose
//! removes it. `recipe-strata-generic` shows the declaration alone does nothing (0 emitted lines);
//! the two `SuffixOrEnd` fixtures show one boundary-bearing suffix line is not enough.
//!
//! A fixture therefore needs: a `BoundaryDefinition`, a PREFIX affix whose allomorph text includes
//! that boundary's representation, roots it attaches to, and words whose surface omits the boundary.
//! Note `crate::emit`'s `with_boundary_insertions` can mask this on paths that go through it (it
//! expands the query with boundary-inserted variants -- how `metathesis-phase-isolation`'s `mu+i`
//! works), and `crate::templated_compile` already applies its own cleanup; the gap was only in
//! `recipe_runtime`'s plan-driven path.

use pg_conformance_fixtures::{corpus, discover, Root};
use pg_foma::build::unbuildable_markers;
use pg_foma::enumerate::enumerate_default;
use pg_foma::enumerate::CandidateRole;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::recipe_optimizer::Certification;
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::recipe_runtime::{evaluate_plans, RuntimeBudget};
use pg_foma::replace::SegAlphabet;

/// Returns each candidate's evaluation PAIRED WITH the strategy that produced it AND its declared
/// `CandidateRole`. Both pairings are necessary, not decorative:
///
/// * the marker-attribution rule below applies only to candidates measured on
///   `build_controllable`'s controllable-subtree network -- a candidate compiled by a whole-grammar
///   strategy has no marker gap to attribute anything to, so its verdict is a real measurement and
///   must be read as one; and
/// * the BASELINE rule applies to whichever candidate the runtime treats as the baseline,
///   and this gate used to identify that candidate by POSITION (`index == 0`) because
///   `evaluate_plans` itself did. It no longer does -- the fact is `LoweredCandidate::role` -- and
///   position was never right here anyway: `materialize_distinct` orders candidates by FAMILY ID,
///   and `ordered-morphophonology` (the only plan-composing `Identity` family, i.e. the one carrying
///   the baseline plan verbatim) sorts after four other families. On any grammar those apply to,
///   element zero was an ALTERNATIVE being held to the baseline's rule.
fn materialize_and_evaluate(
    grammar: &pg_grammar::model::Grammar,
    words: &[String],
) -> Vec<(
    pg_foma::enumerate::EmissionStrategy,
    CandidateRole,
    pg_foma::recipe_runtime::RuntimeEvaluation,
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

/// THE pin for defect (1). Corpus-gated because no synthetic fixture reproduces it (module doc).
///
/// Fail-closed on a missing corpus rather than the usual skip-with-a-message: this test only ever
/// runs when someone asks for it explicitly with `--include-ignored`, and at that point silently
/// returning success while testing nothing is the exact "second false-success path" a fail-closed
/// corpus gate exists to prevent.
#[test]
#[ignore = "needs the private corpus at samples/data/indonesian-hc.xml; run with --include-ignored"]
fn corpus_indonesian_confirms_after_the_finish_step() {
    // `corpus::require` (not a skip-if-absent guard) so a missing corpus fails rather than
    // reporting a pass it did not earn -- the manifest declares both of these under the
    // `indonesian` corpus.
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
         `recipe_runtime::evaluate_plans`. Certifications: {:?}",
        evaluations
            .iter()
            .map(|e| &e.certification)
            .collect::<Vec<_>>()
    );
    assert!(
        proposals > 0,
        "confirmed with zero proposals is a vacuous pass"
    );
    // The managed front end rejects a successful cargo exit whose total executed-case count is
    // zero: a suite that compiles, runs, and exercises nothing is a failure, not a pass.
    corpus::record_cases(
        "corpus_indonesian_confirms_after_the_finish_step",
        words.len(),
    );
}

/// Non-vacuous on staged fixtures: the production evaluator must reach `FullHcConfirmed` end to end
/// with no private corpus. This does NOT pin defect (1) (module doc).
///
/// Note this fixture's plan DOES carry an out-of-scope marker (checked: an earlier version of this
/// test asserted the opposite and failed), and it confirms anyway. That is the direct evidence that
/// marker presence must never be treated as disqualifying on its own — the reason
/// `recipe_runtime` records markers but only consults them after full HC has actually refused.
#[test]
fn the_evaluator_confirms_a_wholly_in_scope_grammar() {
    let fixtures = discover();
    let fixture = fixtures
        .iter()
        .find(|f| f.root == Root::Staging && f.name == "recipe-gated-generic")
        .expect("missing staged fixture recipe-gated-generic");
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

/// The attribution path: a candidate that full HC refused, whose plan needed subtrees
/// `build_controllable` cannot build, must be reported as that limitation — not as a word-level
/// analysis mismatch that sends a reader hunting a phantom grammar bug.
///
/// Marker presence alone must NOT condemn a candidate (`gate::compile_gated_grammar` is
/// controllable-only too and still reaches full recall on marker-carrying grammars), so this asserts
/// the conditional, never the blanket refusal.
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
            // Same rule as the baseline below, for the same reason, and it has to be checked BEFORE
            // the marker-attribution assertion: a whole-grammar strategy is compiled by its own
            // compiler, which builds the marker material rather than skipping it. There is therefore
            // no compiler limitation to attribute, and relabelling its verdict `Unsupported` would
            // hide a real measurement behind a limitation notice that does not apply to it.
            // Measured: `EmissionStrategy::TemplatedUnderlyingTokens` reports `multiplicity-mismatch`
            // with non-zero proposals on these fixtures — a genuine result about a genuine network.
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
                // The BASELINE of a marker-requiring grammar is routed to the tuned emit path, which
                // CAN build those subtrees. So it is measured on a network that genuinely represents
                // the grammar, and any failure here is a real result about that network -- it must NOT
                // be relabelled `Unsupported`. Confirming and failing are both legitimate; what must
                // not happen is a compiler limitation being reported in place of the measurement.
                assert!(
                    !matches!(e.certification, Certification::Unsupported { .. }),
                    "{}: the baseline took the tuned emit path, so its verdict must be the real \
                     measurement rather than an `Unsupported` limitation notice, got {:?}",
                    fixture.label(),
                    e.certification
                );
                continue;
            }
            // Confirming is legitimate for a permutation too: the controllable builder DOES honour
            // gate/union permutations, and `mpr-gated-exception` confirms all of its candidates that
            // way. Evidence first -- so nothing is refused before being tried.
            if e.certification.selectable() {
                continue;
            }
            // But a permutation that FAILED on the controllable network, and whose plan needs subtrees
            // that builder cannot construct, must be attributed rather than reported as a word-level
            // grammar fault: the tuned path that could build those subtrees derives topology from its
            // own plan, so it cannot stand in for this permutation. Note the failure is reported AFTER
            // measurement (proposals may well be non-zero) -- that measurement is the evidence the
            // attribution rests on.
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
