use pg_conformance_fixtures::{assert_matches_oracle, discover};
use pg_foma::backend_registry::{MaterializerContext, Registry};
use pg_foma::backend_runtime::{evaluate_plans, RuntimeBudget};
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;

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
fn every_distinct_plan_fully_confirms_or_refuses_markers_explicitly() {
    let f = fixture();
    let g = pg_grammar::load(&f.load_grammar_xml()).expect("fixture grammar must load");
    let yaml = f.load_words_yaml();
    let rules = g
        .strata
        .iter()
        .flat_map(|s| s.prules.iter())
        .map(|id| &g.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phon = PhonologyProbe::new(&g);
    let baseline = enumerate_default(&g, &rules, phon.as_ref());
    // The real per-marker computation, not the structural presence check: see crate::build::unbuildable_marker_material's own doc.
    let genuinely_unbuildable = pg_foma::build::unbuildable_marker_material(&baseline, &g);
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
    // PlanComposed must refuse marker-bearing plans rather than emit an incomplete network.
    let considered = candidates.len();
    plans.extend(
        candidates
            .into_iter()
            .map(|(_, p)| p)
            .filter(|p| !p.strategy().is_whole_grammar()),
    );
    assert!(
        plans.len() > 1,
        "the registry offered {considered} distinct candidate(s) but none of them was \
         plan-composed, so this test has no plan rewrite to exercise"
    );
    let words = yaml
        .words
        .iter()
        .map(|w| w.word.clone())
        .collect::<Vec<_>>();
    let mut marker_refusals = 0;
    for result in evaluate_plans(&g, &plans, &words, RuntimeBudget::default())
        .expect("the oracle liveness net / memory ceiling must not trip on this fixture")
    {
        match result.certification {
            pg_foma::backend_optimizer::Certification::FullHcConfirmed { .. } => {}
            pg_foma::backend_optimizer::Certification::Unsupported { ref reason } => {
                assert!(
                    reason.contains("CompositeEmissionMarker"),
                    "the only honest refusal expected here is the plan-composed marker boundary: {reason}"
                );
                marker_refusals += 1;
            }
            ref other => panic!("non-certifying evidence must remain explicit: {other:?}"),
        }
    }
    if genuinely_unbuildable.is_empty() {
        assert_eq!(
            marker_refusals, 0,
            "this fixture's marker material is buildable ({genuinely_unbuildable:?} empty), so \
             every plan-composed candidate is expected to confirm, not refuse on the marker boundary"
        );
    } else {
        assert!(
            marker_refusals > 0,
            "the composite-emission marker refusal must engage on this fixture ({genuinely_unbuildable:?})"
        );
    }
}
