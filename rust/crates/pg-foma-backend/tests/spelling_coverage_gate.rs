//! Spelling certificates through admission, actual emission, and confirmed identities.

use pg_conformance_fixtures::discover;
use pg_foma::emit::{emit_tuned_surface, FomaTier};

const PREFIX: &str = "staging:edge-cases/spelling-coverage-copy-slots";

const COMPOUND: &str = "staging:edge-cases/spelling-coverage-compound";

/// A proved compound fixture and the token inventory its exhaustive neighbors are drawn from.
struct Proved {
    suffix: &'static str,
    tokens: &'static [&'static str],
    max_len: usize,
}

const PROVED: &[Proved] = &[
    Proved {
        suffix: "-repeated-root",
        tokens: &["a", "\u{e1}", "xy", "e", "p"],
        max_len: 3,
    },
    Proved {
        suffix: "-allomorphs",
        tokens: &["a", "\u{e1}", "xy", "p", "e"],
        max_len: 3,
    },
    Proved {
        suffix: "-orders",
        tokens: &["a", "p", "s", "xy", "\u{e1}"],
        max_len: 3,
    },
    Proved {
        suffix: "-orders-pos",
        tokens: &["a", "p", "s", "xy", "\u{e1}"],
        max_len: 3,
    },
    Proved {
        suffix: "-affixes",
        tokens: &["a", "xy", "s", "i", "\u{e1}"],
        max_len: 3,
    },
    Proved {
        suffix: "-reversed-table",
        tokens: &["a", "\u{e1}", "xy", "p"],
        max_len: 4,
    },
    Proved {
        suffix: "-alias-deleted",
        tokens: &["a", "\u{e1}", "xy", "p", "e"],
        max_len: 3,
    },
    Proved {
        suffix: "-completed",
        tokens: &["a", "\u{e1}", "xy", "i", "p"],
        max_len: 3,
    },
    Proved {
        suffix: "-boundary",
        tokens: &["a", "xy", "\u{e1}", "^0"],
        max_len: 4,
    },
    Proved {
        suffix: "-boundary-explicit",
        tokens: &["a", "xy", "\u{e1}", "-"],
        max_len: 4,
    },
    Proved {
        suffix: "-template",
        tokens: &["a", "xy", "s", "i"],
        max_len: 4,
    },
];

fn fixture_named(label: &str) -> pg_conformance_fixtures::FixtureRef {
    discover()
        .into_iter()
        .find(|f| f.label() == label)
        .unwrap_or_else(|| panic!("fixture {label} is missing"))
}

/// The declared words plus every token string up to `max_len`, so each declared word's neighbors are measured too.
fn neighbor_words(
    declared: impl IntoIterator<Item = String>,
    tokens: &[&str],
    max_len: usize,
) -> Vec<String> {
    let mut words: Vec<String> = declared.into_iter().collect();
    let mut frontier = vec![String::new()];
    for _ in 0..max_len {
        let mut next = Vec::new();
        for base in &frontier {
            for token in tokens {
                next.push(format!("{base}{token}"));
            }
        }
        words.extend(next.iter().cloned());
        frontier = next;
    }
    words.sort_unstable();
    words.dedup();
    words
}

fn assert_refused_everywhere(g: &pg_grammar::model::Grammar, what: &str) {
    let emitted = emit_tuned_surface(g);
    assert!(
        emitted.lexc_source.is_empty(),
        "{what}: emitted despite the refusal: {:?}",
        emitted.report
    );
    let Err(pg_foma::analyzer::FomaError::CapabilityRefused(refusals)) =
        pg_foma::analyzer::compile_proposer(g)
    else {
        panic!("{what}: must be refused at normal compilation")
    };
    assert!(
        refusals.iter().any(|d| d.refusal
            == pg_health::capability::CapabilityRefusal::ProposalRecall(
                pg_health::capability::ProposalRecallGap::AmbiguousSegmentSpelling
            )),
        "{what}: {refusals:?}"
    );
}

/// Each case changes the proved compound in exactly one way the proof must not cover; the mutated grammar must load and still be refused by both the capability envelope and direct emission.
#[test]
fn compound_shapes_outside_the_proof_keep_the_typed_spelling_refusal() {
    let base = fixture_named(&format!("{COMPOUND}-repeated-root"));
    let xml = base.load_grammar_xml();
    let copy_pair =
        "<CopyFromInput index=\"h0\" />\n                  <CopyFromInput index=\"n0\" />";
    assert!(xml.contains(copy_pair));
    let cases: &[(&str, String)] = &[
        (
            "a literal segment between the stems",
            xml.replace(
                copy_pair,
                "<CopyFromInput index=\"h0\" /><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index=\"n0\" />",
            ),
        ),
        (
            "the head copied twice",
            xml.replace(
                copy_pair,
                "<CopyFromInput index=\"h0\" /><CopyFromInput index=\"h0\" /><CopyFromInput index=\"n0\" />",
            ),
        ),
        (
            "the non-head stem dropped",
            xml.replace(copy_pair, "<CopyFromInput index=\"h0\" />"),
        ),
        (
            "a self-feeding compound",
            xml.replace(
                "<CompoundingRule id=\"cr1\">",
                "<CompoundingRule id=\"cr1\" multipleApplication=\"2\">",
            ),
        ),
    ];
    for (what, mutated) in cases {
        let g = pg_grammar::load(mutated).unwrap_or_else(|e| panic!("{what}: {e:?}"));
        assert_refused_everywhere(&g, what);
    }
    let restricted = fixture_named(&format!("{COMPOUND}-orders")).load_grammar_xml();
    let adds_mpr = "<MorphologicalRule id=\"ADD\" requiredPartsOfSpeech=\"v\"><Name>ADD</Name><MorphologicalSubrules><MorphologicalSubrule id=\"ADD0\"><MorphologicalInput><PhoneticSequence id=\"ADDS\"><OptionalSegmentSequence min=\"1\" max=\"-1\"><SimpleContext naturalClass=\"any\" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput MPRFeatures=\"m2\"><CopyFromInput index=\"ADDS\"/><InsertSegments><PhoneticShape>i</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>ADD</MorphemeId></MorphologicalRule>";
    let g = pg_grammar::load(
        &restricted
            .replace(
                "</MorphologicalRuleDefinitions>",
                &format!("{adds_mpr}</MorphologicalRuleDefinitions>"),
            )
            .replace(
                "morphologicalRules=\"cr1\"",
                "morphologicalRules=\"cr1 ADD\"",
            ),
    )
    .expect("the restricted compound with an MPR-adding rule loads");
    assert_refused_everywhere(
        &g,
        "an MPR-restricted compound whose stems can gain MPR features before it applies",
    );
    let asymmetric = fixture_named(&format!("{COMPOUND}-orders-asymmetric")).load_grammar();
    assert_refused_everywhere(
        &asymmetric,
        "a non-head-first output whose stems are not licensed as the opposite slot",
    );
    for (shape, what) in [
        ("lead", "a boundary written before the head stem"),
        ("trail", "a boundary written after the non-head stem"),
        ("double", "two boundaries written between the stems"),
    ] {
        let g = fixture_named(&format!("{COMPOUND}-boundary-{shape}")).load_grammar();
        assert_refused_everywhere(&g, what);
    }
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
fn proved_compound_routes_are_created_consumed_and_recover_every_identity() {
    for proved in PROVED {
        let label = format!("{COMPOUND}{}", proved.suffix);
        let fixture = fixture_named(&label);
        let g = fixture.load_grammar();
        let subrules: usize = g
            .mrules
            .iter()
            .map(|r| match r {
                pg_grammar::model::MorphRuleDef::Compounding(c) => c.subrules.len(),
                _ => 0,
            })
            .sum();
        let emitted = emit_tuned_surface(&g);
        assert!(
            matches!(emitted.report.tier, FomaTier::Full),
            "{label}: {:?}",
            emitted.report
        );
        assert!(!emitted.lexc_source.is_empty(), "{label}");
        let counts = &emitted.report.counts;
        assert!(counts.spelling_certificate_paths > 0, "{label}");
        assert_eq!(counts.compound_certificate_subrules, subrules, "{label}");
        assert!(
            counts.compound_certificate_uses >= 1,
            "{label}: the proof was created but no compound chain consumed it"
        );
        let yaml = fixture.load_words_yaml();
        let declared: Vec<_> = yaml.words.iter().collect();
        let words = neighbor_words(
            declared.iter().map(|w| w.word.clone()),
            proved.tokens,
            proved.max_len,
        );
        let morpher = pg_parse::Morpher::new(&g, usize::MAX);
        let mut analyzer = pg_foma::composite::compile_analyzer(&g).unwrap();
        let (mut compared, mut compound_words) = (0, 0);
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
                assert!(row.incomplete, "{label}: {word}");
                assert!(row.confirmed.is_empty(), "{label}: {word}");
                continue;
            }
            assert_eq!(row.hc_status, "complete", "{label}: {word}");
            assert_eq!(row.fst_status, "complete", "{label}: {word}");
            assert!(!row.incomplete, "{label}: {word}: {row:?}");
            assert_eq!(
                row.proposal_misses.as_deref(),
                Some(&[][..]),
                "{label}: {word}: a missing pre-confirmation identity"
            );
            assert_eq!(
                row.spurious_confirmed.as_deref(),
                Some(&[][..]),
                "{label}: {word}: a surviving extra"
            );
            assert_eq!(
                row.confirmed_multiset_equal,
                Some(true),
                "{label}: {word}: {row:?}"
            );
            if let Some(entry) = declared.iter().find(|w| w.word == *word) {
                let hc: usize = row.oracle.iter().map(|m| m.count).sum();
                assert_eq!(
                    hc,
                    entry.parses.len(),
                    "{label}: {word}: HC occurrence count differs from the C# signature count"
                );
                compound_words += entry
                    .parses
                    .iter()
                    .filter(|p| p.rules.iter().any(|r| r.starts_with("cr")))
                    .count();
            }
            compared += 1;
        }
        assert!(compared >= 60, "{label}: only {compared} words compared");
        assert!(
            compound_words >= 2,
            "{label}: only {compound_words} declared compound parses were exercised"
        );
    }
}

/// The nine transcribed C# rows of the alias self-compound: four accepted occurrences (the bare control and three self-compounds, one identity each) and five negatives, all complete with equal multiplicities.
#[test]
fn the_nine_row_alias_self_compound_replays_with_exact_multiplicities() {
    let fixture = fixture_named(&format!("{COMPOUND}-repeated-root"));
    let g = fixture.load_grammar();
    let yaml = fixture.load_words_yaml();
    assert_eq!(yaml.words.len(), 9);
    let morpher = pg_parse::Morpher::new(&g, usize::MAX);
    let mut analyzer = pg_foma::composite::compile_analyzer(&g).unwrap();
    let mut accepted = 0;
    for (index, entry) in yaml.words.iter().enumerate() {
        let row = pg_foma::differential::measure_word(
            &g,
            &morpher,
            Some(&mut analyzer),
            index,
            &entry.word,
            &pg_foma::compose_budget::ApplyBudget::unbounded(),
            None,
        )
        .unwrap();
        assert!(!row.incomplete, "{}: {row:?}", entry.word);
        assert_eq!(row.confirmed_multiset_equal, Some(true), "{}", entry.word);
        assert_eq!(
            row.proposal_misses.as_deref(),
            Some(&[][..]),
            "{}",
            entry.word
        );
        assert_eq!(
            row.spurious_confirmed.as_deref(),
            Some(&[][..]),
            "{}",
            entry.word
        );
        let hc: usize = row.oracle.iter().map(|m| m.count).sum();
        let fst: usize = row.confirmed.iter().map(|m| m.count).sum();
        assert_eq!(hc, entry.parses.len(), "{}", entry.word);
        assert_eq!(fst, hc, "{}", entry.word);
        accepted += hc;
    }
    assert_eq!(accepted, 4);
}

/// At a zero compound depth bound the grammar yields no proposer and the refusal names the bound.
#[test]
fn compound_construction_refuses_without_an_artifact_at_the_compound_depth_bound() {
    use pg_foma::construction::{FstCompileConfig, StrategyRefusal};
    let g = fixture_named(&format!("{COMPOUND}-affixes")).load_grammar();
    let mut config = FstCompileConfig::default();
    assert!(pg_foma::analyzer::compile_proposer_with_config(&g, &config)
        .0
        .is_ok());
    config.bounds.compound_depth = Some(0);
    let (result, profile) = pg_foma::analyzer::compile_proposer_with_config(&g, &config);
    assert!(
        result.is_err(),
        "a stopped construction produced an artifact"
    );
    let attempt = &profile.construction.attempts[0];
    assert_eq!(attempt.refusal, Some(StrategyRefusal::LimitExceeded));
    let bound = attempt
        .bound_refusal
        .as_ref()
        .expect("the emitter names the bound");
    assert_eq!(bound.limit, 0);
    assert!(bound.value > bound.limit);
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
