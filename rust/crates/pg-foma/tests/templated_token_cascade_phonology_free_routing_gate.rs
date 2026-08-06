//! Closes a routing gap: a template-bearing grammar with no phonological rules must still be offered `EmissionStrategy::TemplatedUnderlyingTokens` via the widened `Applicability::HasPhonologyOrTemplates` gate, not only the plan-composed baseline's `uflexc` emitter.
//! See `docs/research/pg-foma-templated-phonology-free-routing-notes.md` for why the old `HasPhonology`-only gate masked this and how the fix is verified beyond mere reachability.

use pg_conformance_fixtures::{discover, Root};
use pg_foma::enumerate::{enumerate_default, EmissionStrategy};
use pg_foma::junctions::PhonologyProbe;
use pg_foma::recipe_optimizer::Certification;
use pg_foma::recipe_registry::{MaterializerContext, RecipeInstance, Registry};
use pg_foma::recipe_runtime::{evaluate_plans, RuntimeBudget};
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::Grammar;

const FAMILY: &str = "token-cascade-morphology";

fn load(name: &str) -> Grammar {
    let fixtures = discover();
    let fixture = fixtures
        .iter()
        .find(|f| f.root == Root::Staging && f.name == name)
        .unwrap_or_else(|| panic!("missing staged fixture {name}"));
    pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load")
}

fn baseline_plan(grammar: &Grammar) -> pg_foma::plan::Plan {
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let prules = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phonology = PhonologyProbe::new(grammar);
    enumerate_default(grammar, &alphabet, &prules, phonology.as_ref())
}

fn token_cascade_candidate(
    candidates: &[(RecipeInstance, pg_foma::enumerate::LoweredCandidate)],
) -> &pg_foma::enumerate::LoweredCandidate {
    &candidates
        .iter()
        .find(|(instance, _)| instance.family_id == FAMILY)
        .unwrap_or_else(|| {
            panic!(
                "{FAMILY} offered no surviving candidate; owners: {:?}",
                candidates
                    .iter()
                    .map(|(i, _)| i.family_id.as_str())
                    .collect::<Vec<_>>()
            )
        })
        .1
}

/// The templated, phonology-free fixture gets the token-cascade candidate in addition to the plan-composed baseline.
#[test]
fn templated_phonology_free_fixture_offers_the_token_cascade_candidate() {
    let grammar = load("recipe-template-generic");
    assert!(
        grammar.prules.is_empty(),
        "recipe-template-generic is used here BECAUSE it declares no phonological rules; if it \
         gains one this test stops covering the phonology-free half of the routing gap"
    );
    assert!(
        !grammar.templates.is_empty(),
        "recipe-template-generic is used here BECAUSE it declares affix templates"
    );

    let baseline = baseline_plan(&grammar);
    let candidates = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar: &grammar,
            baseline: &baseline,
        })
        .expect("materialization must succeed");

    let candidate = token_cascade_candidate(&candidates);
    assert_eq!(
        candidate.strategy(),
        EmissionStrategy::TemplatedUnderlyingTokens,
        "{FAMILY} must request the token-cascade compiler"
    );

    // The candidate set must not be uflexc-only: at least one materialized candidate carries template-aware whole-grammar structure, not just refinements of the plan-composed baseline.
    assert!(
        candidates
            .iter()
            .any(|(_, c)| c.strategy().is_whole_grammar()),
        "candidate set for a template-bearing grammar was plan-composed (uflexc) only: {:?}",
        candidates
            .iter()
            .map(|(i, c)| (i.family_id.as_str(), c.strategy()))
            .collect::<Vec<_>>()
    );
}

/// A phonology-bearing grammar's offering is unchanged by the widened predicate: `HasPhonology` was already true, so `HasPhonologyOrTemplates` stays true for the same reason -- a regression pin, not new behavior.
#[test]
fn phonology_bearing_fixture_offering_is_unchanged() {
    let grammar = load("recipe-gated-generic");
    assert!(
        !grammar.prules.is_empty(),
        "recipe-gated-generic is used here BECAUSE it declares phonological rules"
    );

    let baseline = baseline_plan(&grammar);
    let candidates = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar: &grammar,
            baseline: &baseline,
        })
        .expect("materialization must succeed");

    let candidate = token_cascade_candidate(&candidates);
    assert_eq!(
        candidate.strategy(),
        EmissionStrategy::TemplatedUnderlyingTokens
    );
}

/// Correctness, not just reachability: asserts the candidate reaches a real, non-`BuildFailed` verdict with non-zero proposals on a phonology-free grammar (see `docs/research/pg-foma-templated-phonology-free-routing-notes.md`).
#[test]
fn templated_candidate_builds_and_proposes_on_the_phonology_free_fixture() {
    let fixtures = discover();
    let fixture = fixtures
        .iter()
        .find(|f| f.root == Root::Staging && f.name == "recipe-template-generic")
        .expect("missing staged fixture recipe-template-generic");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");

    // Only the cheapest word (the boundary case, `words.yaml`'s first entry): this test is about the compiler reaching a real verdict, not replaying the fixture's full analysis pathology.
    let words: Vec<String> = fixture
        .load_words_yaml()
        .words
        .first()
        .map(|w| vec![w.word.clone()])
        .expect("fixture must declare at least one word");

    let baseline = baseline_plan(&grammar);
    let candidates = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar: &grammar,
            baseline: &baseline,
        })
        .expect("materialization must succeed");
    let plans: Vec<_> = candidates.into_iter().map(|(_, p)| p).collect();

    let evaluations = evaluate_plans(&grammar, &plans, &words, RuntimeBudget::default())
        .expect("the oracle liveness net / memory ceiling must not trip on this fixture");
    let (_, evaluation) = plans
        .iter()
        .zip(&evaluations)
        .find(|(plan, _)| plan.strategy() == EmissionStrategy::TemplatedUnderlyingTokens)
        .expect("token-cascade-morphology's candidate must be among the evaluated plans");

    assert!(
        !matches!(evaluation.certification, Certification::BuildFailed { .. }),
        "the token-cascade compiler failed to build on a phonology-free grammar: {:?}",
        evaluation.certification
    );
    assert!(
        evaluation.score.proposals > 0,
        "a non-build-failed verdict with zero proposals is a vacuous pass: {:?}",
        evaluation.score
    );
    // The single-word boundary case has exactly one valid analysis, so this compiler should reach real confirmation on it, not merely "did not crash".
    assert!(
        evaluation.certification.selectable(),
        "expected FullHcConfirmed on the trivial zero-slot word, got {:?}",
        evaluation.certification
    );
}
