use pg_conformance_fixtures::{assert_matches_oracle, discover, Root};
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::replace::SegAlphabet;

#[test]
fn promoted_recipe_fixtures_replay_and_offer_distinct_plans_or_elimination_evidence() {
    let expected = [
        "recipe-gated-generic",
        "recipe-template-generic",
        "recipe-ordered-generic",
        "recipe-strata-generic",
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
            "recipe-gated-generic" => {
                assert!(!grammar.mpr_features.is_empty());
                assert!(grammar.prules.len() >= 2);
            }
            "recipe-template-generic" => assert!(!grammar.templates.is_empty()),
            "recipe-ordered-generic" => {
                assert!(grammar
                    .prules
                    .iter()
                    .any(|r| matches!(r, pg_grammar::model::PhonRuleDef::Metathesis(_))));
            }
            "recipe-strata-generic" => assert!(grammar.strata.len() > 1),
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
            .unwrap_or_else(|e| panic!("{name} recipe materialization failed: {e}"));
        if name == "recipe-template-generic" {
            assert_eq!(candidates.len(), 1, "the checked-in elimination report is only valid while no distinct template Plan exists");
            let report = std::fs::read_to_string(fixture.dir.join("RECIPE_ELIMINATION.md"))
                .expect("single-candidate fixture must carry an elimination report");
            assert!(report.contains("content-address-duplicate"));
            assert!(report.contains("no `Union` node"));
        } else {
            assert!(
                candidates.len() >= 2,
                "{name} must retain the default plus a content-distinct executable alternative; got {}",
                candidates.len()
            );
        }
        for (_, candidate) in candidates {
            pg_foma::recipe_runtime::build_candidate(
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
