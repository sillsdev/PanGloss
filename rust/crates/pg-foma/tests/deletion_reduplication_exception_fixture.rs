use pg_conformance_fixtures::{assert_matches_oracle, discover};
use pg_foma::backend_registry::{MaterializerContext, Registry};
use pg_foma::backend_runtime::{evaluate_plans, RuntimeBudget};
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;

fn fixture() -> pg_conformance_fixtures::FixtureRef {
    discover()
        .into_iter()
        .find(|f| {
            f.root == pg_conformance_fixtures::Root::Staging
                && f.name == "deletion-reduplication-exception-composite"
        })
        .expect("new staged fixture must be discoverable")
}

#[test]
fn fixture_loads_replays_and_exercises_required_grammar_facts() {
    let f = fixture();
    let xml = f.load_grammar_xml();
    let g = pg_grammar::load(&xml).expect("fixture grammar must load");
    let yaml = f.load_words_yaml();
    assert_matches_oracle(&f.label(), &yaml, &pg_parse::Morpher::new(&g, usize::MAX));
    assert!(
        g.mrules.iter().any(
            |r| r.affix_allomorphs().is_some_and(|as_| as_.iter().any(|a| a
                .rhs
                .iter()
                .filter(|x| matches!(x, pg_grammar::model::OutputAction::Copy(_)))
                .count()
                > a.lhs.len()))
        ),
        "reduplication fact missing"
    );
    assert!(g.prules.iter().any(|r| matches!(r, pg_grammar::model::PhonRuleDef::Rewrite(w) if w.subrules.iter().any(|s| s.rhs.nodes.is_empty()))), "deletion fact missing");
    assert!(!g.mpr_features.is_empty(), "gated/exception fact missing");
    assert!(
        g.entries.iter().any(|e| !e.mpr.is_empty()),
        "lexical exception fact missing"
    );
}

#[test]
fn every_applicable_distinct_backend_builds_and_full_hc_matches_each_word() {
    let f = fixture();
    let g = pg_grammar::load(&f.load_grammar_xml()).expect("fixture grammar must load");
    let yaml = f.load_words_yaml();
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let rules = g
        .strata
        .iter()
        .flat_map(|s| s.prules.iter())
        .map(|id| &g.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phon = PhonologyProbe::new(&g);
    let baseline = enumerate_default(&g, &alphabet, &rules, phon.as_ref());
    let registry = Registry::seeded();
    let candidates = registry
        .materialize_distinct(&MaterializerContext {
            grammar: &g,
            baseline: &baseline,
        })
        .expect("baseline and every applicable registry Plan must materialize");
    assert!(!candidates.is_empty(), "registry must retain baseline");
    let mut plans = Vec::with_capacity(candidates.len());
    plans.push(pg_foma::enumerate::LoweredCandidate {
        label: "baseline",
        plan: baseline,
        adapter: pg_foma::lowering_adapter::LoweringAdapter::ControllablePlanCompose,
        // This candidate carries the grammar's own default plan.
        role: pg_foma::enumerate::CandidateRole::Baseline,
    });
    // Scoped to `PlanComposed`: this asserts `build_controllable` honours rewritten assembly trees, not that every compiler this crate owns is equivalent (a different, false claim — `EmissionStrategy::TemplatedUnderlyingTokens`'s own coverage lives in `backend_emission_strategy_gate.rs`).
    let considered = candidates.len();
    plans.extend(
        candidates
            .into_iter()
            .map(|(_, p)| p)
            .filter(|p| !p.strategy().is_whole_grammar()),
    );
    assert!(
        plans.len() > 1,
        "the registry offered {considered} distinct candidate(s) but none of them was plan-composed, \
         so this test would assert nothing about plan rewrites at all"
    );
    let words = yaml
        .words
        .iter()
        .map(|w| w.word.clone())
        .collect::<Vec<_>>();
    for result in evaluate_plans(&g, &plans, &words, RuntimeBudget::default())
        .expect("the oracle liveness net / memory ceiling must not trip on this fixture")
    {
        assert!(
            matches!(
                result.certification,
                pg_foma::backend_optimizer::Certification::FullHcConfirmed { .. }
            ),
            "non-certifying evidence must remain explicit: {:?}",
            result.certification
        );
    }
}
