use pg_conformance_fixtures::{assert_matches_oracle, discover, Root};
use pg_foma::backend_registry::{MaterializerContext, Registry};
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;

#[test]
fn promoted_backend_fixtures_replay_and_offer_distinct_plans_or_elimination_evidence() {
    let expected = [
        "backend-gated-generic",
        "backend-template-generic",
        "backend-ordered-generic",
        "backend-strata-generic",
    ];
    let fixtures = discover();
    for name in expected {
        let fixture = fixtures
            .iter()
            .find(|f| f.root == Root::Staging && f.name == name)
            .unwrap_or_else(|| panic!("missing promoted fixture {name}"));
        let grammar = pg_grammar::load(&fixture.load_grammar_xml())
            .unwrap_or_else(|e| panic!("{name} failed to load: {e}"));
        let words = fixture.load_words_yaml();
        assert_matches_oracle(
            &fixture.label(),
            &words,
            &pg_parse::Morpher::new(&grammar, usize::MAX),
        );

        match name {
            "backend-gated-generic" => {
                assert!(!grammar.mpr_features.is_empty());
                assert!(grammar.prules.len() >= 2);
            }
            "backend-template-generic" => assert!(!grammar.templates.is_empty()),
            "backend-ordered-generic" => {
                assert!(grammar
                    .prules
                    .iter()
                    .any(|r| matches!(r, pg_grammar::model::PhonRuleDef::Metathesis(_))));
            }
            "backend-strata-generic" => assert!(grammar.strata.len() > 1),
            _ => unreachable!(),
        }

        let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
        let prules = grammar
            .strata
            .iter()
            .flat_map(|s| &s.prules)
            .map(|id| &grammar.prules[id.0 as usize])
            .collect::<Vec<_>>();
        let phonology = PhonologyProbe::new(&grammar);
        let baseline = enumerate_default(&grammar, &alphabet, &prules, phonology.as_ref());
        let candidates = Registry::seeded()
            .materialize_distinct(&MaterializerContext {
                grammar: &grammar,
                baseline: &baseline,
            })
            .unwrap_or_else(|e| panic!("{name} backend materialization failed: {e}"));
        // Counted over plan-composed candidates only: a whole-grammar strategy is a different compiler carrying the same plan, so counting it here would answer a question neither branch below asks.
        let plan_candidates = candidates
            .iter()
            .filter(|(_, c)| c.adapter.interprets_plan())
            .count();
        if name == "backend-template-generic" {
            assert_eq!(plan_candidates, 1, "the checked-in elimination report is only valid while no distinct template Plan exists");
            let report = std::fs::read_to_string(fixture.dir.join("BACKEND_ELIMINATION.md"))
                .expect("single-candidate fixture must carry an elimination report");
            assert!(report.contains("content-address-duplicate"));
            assert!(report.contains("no `Union` node"));
        } else {
            assert!(
                plan_candidates >= 2,
                "{name} must retain the default plus a content-distinct executable alternative; got {plan_candidates} plan-composed of {} total",
                candidates.len()
            );
        }
        // Plan-composed candidates only: `build_candidate` errors on a candidate naming a different compiler; a whole-grammar strategy's buildability is covered in `backend_emission_strategy_gate.rs`.
        for (_, candidate) in candidates
            .into_iter()
            .filter(|(_, c)| c.adapter.interprets_plan())
        {
            pg_foma::backend_runtime::build_candidate(
                &candidate,
                &foma::options::FomaOptions::default(),
                &grammar,
                &alphabet,
                &prules,
                &pg_foma::compose_budget::ComposeBudget::with_caps(
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                    None,
                ),
            )
            .unwrap_or_else(|e| panic!("{name} candidate did not build: {e}"));
        }
    }
}
