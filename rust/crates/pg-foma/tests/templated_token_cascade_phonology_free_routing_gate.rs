//! Closes the routing gap named in `openspec/changes/cleanup-and-recipe-parity/specs/
//! recipe-strategy-routing/spec.md`: a template-bearing grammar with NO phonological rules (the
//! measured Sena shape) must still be offered `EmissionStrategy::TemplatedUnderlyingTokens`, the
//! only candidate whose lexicon carries template-aware morphotactic structure (slot ordering and
//! bounded slot occupancy) rather than the plan-composed baseline's deliberately-minimal
//! self-looping `uflexc` emitter (`uflexc`'s own module doc: it does not generalize to templated
//! grammars).
//!
//! Before this change, `token-cascade-morphology` (the family that requests this strategy,
//! `recipe_registry::SEEDS`) was gated on `Applicability::HasPhonology`
//! (`!grammar.prules.is_empty()`), a structural fact this fixture does not have: it declares
//! `<AffixTemplate>` slots but zero `<PhonologicalRule>`/`<MetathesisRule>` elements anywhere
//! (`words.yaml`'s own header comment). So the only underlying model ever offered for a grammar
//! shaped this way was `uflexc`, and the template-aware candidate was never even materialized to
//! compare against it -- the exact defect this test pins shut.
//!
//! `Applicability::HasPhonologyOrTemplates` widens the gate to `HasPhonology OR HasTemplates`,
//! evaluated structurally over the same two `Grammar` fields those two narrower variants already
//! read. This file is deliberately separate from `recipe_emission_strategy_gate.rs` (which pins the
//! phonology-bearing case, `recipe-gated-generic`) rather than folded into it, so the phonology-free
//! routing gap has its own named, independently-failing pin.

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
    candidates: &[(RecipeInstance, pg_foma::enumerate::CandidatePlan)],
) -> &pg_foma::enumerate::CandidatePlan {
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

/// Scenario 1 from the spec: the templated, phonology-free fixture gets the token-cascade
/// candidate in addition to the plan-composed baseline.
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
        candidate.strategy,
        EmissionStrategy::TemplatedUnderlyingTokens,
        "{FAMILY} must request the token-cascade compiler"
    );

    // The requirement's second half: the candidate set must not be uflexc-only. At least one
    // materialized candidate carries template-aware structure (a whole-grammar strategy), not just
    // permutations/refinements of the plan-composed baseline.
    assert!(
        candidates
            .iter()
            .any(|(_, c)| c.strategy.is_whole_grammar()),
        "candidate set for a template-bearing grammar was plan-composed (uflexc) only: {:?}",
        candidates
            .iter()
            .map(|(i, c)| (i.family_id.as_str(), c.strategy))
            .collect::<Vec<_>>()
    );
}

/// Scenario 2 from the spec: a phonology-bearing grammar's offering is unchanged by the widened
/// predicate. `recipe-gated-generic` has non-empty `prules`, so `HasPhonology` was already true and
/// `HasPhonologyOrTemplates` must stay true for the same reason -- this is a regression pin, not new
/// behavior.
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
        candidate.strategy,
        EmissionStrategy::TemplatedUnderlyingTokens
    );
}

/// Correctness, not just reachability: the widened routing is only useful if the compiler it points
/// at actually builds for a phonology-free grammar. Before this change,
/// `compile_templated_morphotactics` unconditionally turned "zero declared phonological rules" into
/// `TemplatedCompileError::NoCompiledRules` (an empty `prules_in_order` composes to `Ok(None)`,
/// which was then `.ok_or(NoCompiledRules)`-ed into an error) -- a guaranteed build failure that the
/// old `HasPhonology` gate happened to mask by never offering the family in the first place. Now
/// that the family IS offered here, this asserts the candidate reaches a real, non-`BuildFailed`
/// verdict with non-zero proposals: an honest confirm/mismatch is a real result, but a build failure
/// would mean the routing fix handed the optimizer a candidate that can never do anything.
#[test]
fn templated_candidate_builds_and_proposes_on_the_phonology_free_fixture() {
    let fixtures = discover();
    let fixture = fixtures
        .iter()
        .find(|f| f.root == Root::Staging && f.name == "recipe-template-generic")
        .expect("missing staged fixture recipe-template-generic");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");

    // Only the cheapest word (the C(12,0)=1 boundary case, `words.yaml`'s first entry): this test
    // is about the compiler reaching a real verdict, not about replaying the fixture's full
    // 2^12-analysis pathology (that budget-gated replay belongs to the promoted-fixture/CLI gates).
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

    let evaluations = evaluate_plans(&grammar, &plans, &words, RuntimeBudget::default());
    let (_, evaluation) = plans
        .iter()
        .zip(&evaluations)
        .find(|(plan, _)| plan.strategy == EmissionStrategy::TemplatedUnderlyingTokens)
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
    // The single-word boundary case (no optional slot fires) has exactly one valid analysis and
    // nothing else in the fixture's grammar to trip on, so this compiler should reach real
    // confirmation on it, not merely "did not crash".
    assert!(
        evaluation.certification.selectable(),
        "expected FullHcConfirmed on the trivial zero-slot word, got {:?}",
        evaluation.certification
    );
}
