//! Spelling certificates through admission, actual emission, and confirmed identities.

use pg_conformance_fixtures::discover;
use pg_foma::emit::{emit_tuned_surface, FomaTier};

const PREFIX: &str = "staging:edge-cases/spelling-coverage-copy-slots";

#[test]
fn compounds_without_complete_candidate_coverage_keep_the_spelling_refusal() {
    let fixture = discover()
        .into_iter()
        .find(|f| f.label() == "staging:edge-cases/spelling-coverage-compound-repeated-root")
        .unwrap();
    let g = fixture.load_grammar();
    let emitted = emit_tuned_surface(&g);
    assert!(
        emitted.lexc_source.is_empty(),
        "unproved compound emitted: {:?}",
        emitted.report
    );
    let Err(pg_foma::analyzer::FomaError::CapabilityRefused(refusals)) =
        pg_foma::analyzer::compile_proposer(&g)
    else {
        panic!("compound must remain refused")
    };
    assert!(refusals.iter().any(|d| d.refusal
        == pg_health::capability::CapabilityRefusal::ProposalRecall(
            pg_health::capability::ProposalRecallGap::AmbiguousSegmentSpelling
        )));
}

#[test]
fn witnessed_spelling_paths_are_created_consumed_and_recover_every_identity() {
    let fixtures: Vec<_> = discover()
        .into_iter()
        .filter(|f| f.label().starts_with(PREFIX))
        .collect();
    assert_eq!(fixtures.len(), 6);
    for fixture in fixtures {
        let g = fixture.load_grammar();
        let emitted = emit_tuned_surface(&g);
        assert!(
            matches!(emitted.report.tier, FomaTier::Full),
            "{}: {:?}",
            fixture.label(),
            emitted.report
        );
        assert!(!emitted.lexc_source.is_empty());
        assert!(emitted.report.counts.spelling_certificate_paths > 0);
        assert!(emitted.report.counts.spelling_certificate_uses > 0);
        assert!(emitted.report.counts.composite_structural_entries > 0);
        let yaml = fixture.load_words_yaml();
        let mut words: Vec<_> = yaml.words.into_iter().map(|w| w.word).collect();
        for a in ["a", "e", "i", "p", "s", "á", "xy"] {
            for b in ["a", "e", "i", "p", "s", "á", "xy"] {
                words.push(format!("{a}{b}"));
                for c in ["a", "e", "i", "p", "s", "á", "xy"] {
                    words.push(format!("{a}{b}{c}"));
                }
            }
        }
        words.sort_unstable();
        words.dedup();
        let morpher = pg_parse::Morpher::new(&g, usize::MAX);
        let mut analyzer = pg_foma::composite::compile_analyzer(&g).unwrap();
        let mut compared = 0;
        for (index, word) in words.iter().enumerate() {
            let row = pg_foma::differential::measure_word(
                &g,
                &morpher,
                Some(&mut analyzer),
                index,
                word,
                &pg_foma::compose_budget::ApplyBudget::unbounded(),
                None,
            )
            .unwrap();
            if row.hc_status == "invalid_shape" {
                assert!(row.incomplete);
                assert!(row.confirmed.is_empty());
                continue;
            }
            assert!(!row.incomplete, "{}: {word}: {row:?}", fixture.label());
            assert_eq!(
                row.confirmed_multiset_equal,
                Some(true),
                "{}: {word}: {row:?}",
                fixture.label()
            );
            compared += 1;
        }
        assert!(compared > 100);
    }
}

#[test]
fn copied_spelling_construction_refuses_without_an_artifact_at_a_work_boundary() {
    let fixture = discover()
        .into_iter()
        .find(|f| f.label() == PREFIX)
        .unwrap();
    let g = fixture.load_grammar();
    let emitted = pg_foma::emit::emit_with_structural_limits(
        &g,
        pg_foma::emit::StructuralClosureLimits {
            work_limit: 0,
            ..Default::default()
        },
    );
    assert!(emitted.lexc_source.is_empty());
    assert!(!matches!(emitted.report.tier, FomaTier::Full));
    assert!(emitted.report.enum_budget_exceeded.is_some());
    assert_eq!(emitted.report.counts.composite_structural_entries, 0);
}

#[test]
fn aliases_feature_completion_and_duplicate_definition_controls_are_observable() {
    let fixtures = discover();
    let base = fixtures.iter().find(|f| f.label() == PREFIX).unwrap();
    let base_words = base.load_words_yaml();
    let alias_deleted = fixtures
        .iter()
        .find(|f| f.label() == format!("{PREFIX}-alias-deleted"))
        .unwrap()
        .load_words_yaml();
    let completed = fixtures
        .iter()
        .find(|f| f.label() == format!("{PREFIX}-completed"))
        .unwrap()
        .load_words_yaml();
    assert!(!base_words
        .words
        .iter()
        .find(|w| w.word == "á")
        .unwrap()
        .parses
        .is_empty());
    assert!(
        alias_deleted
            .words
            .iter()
            .find(|w| w.word == "á")
            .unwrap()
            .expect_skip
    );
    assert!(!base_words
        .words
        .iter()
        .find(|w| w.word == "i")
        .unwrap()
        .parses
        .is_empty());
    assert!(
        completed
            .words
            .iter()
            .find(|w| w.word == "i")
            .unwrap()
            .expect_fail
    );
    assert!(
        base_words
            .words
            .iter()
            .find(|w| w.word == "pass")
            .unwrap()
            .parses
            .len()
            > 1
    );
    let xml = base.load_grammar_xml();
    let duplicate = r#"<SegmentDefinition id="duplicate"><Representations><Representation>e</Representation></Representations><FeatureValue feature="f" symbolValues="fP"/><FeatureValue feature="g" symbolValues="gM"/></SegmentDefinition>"#;
    assert!(pg_grammar::load(&xml.replace(
        "</SegmentDefinitions>",
        &format!("{duplicate}</SegmentDefinitions>")
    ))
    .is_err());
}
