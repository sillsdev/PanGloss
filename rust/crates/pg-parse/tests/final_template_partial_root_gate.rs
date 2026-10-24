/// End-to-end #553 partial-root and partial-rule conformance coverage.
use pg_conformance_fixtures::{replay_fixture_runs, require_fixture};
use pg_parse::{result_multiset, Morpher, ParseOptions};

fn assert_all_fixture_runs(name: &str) {
    let fixture = require_fixture("edge-cases", name);
    let words = fixture.load_words_yaml();
    let runs = replay_fixture_runs(&fixture, &words);
    assert_eq!(runs.len(), 1 + words.variants.len());
    for run in &runs {
        assert!(
            run.replay.mismatches.is_empty(),
            "{}: {:?}",
            run.name,
            run.replay.mismatches
        );
        let expected_checked = if run.is_base {
            words.words.len()
        } else {
            let variant = words
                .variants
                .iter()
                .find(|variant| run.name.ends_with(&format!("#variant={}", variant.name)))
                .expect("every non-base run names a declared variant");
            words.for_variant(variant).words.len()
        };
        assert_eq!(
            run.replay.checked, expected_checked,
            "{}: every declared status/identity row must be checked",
            run.name
        );
    }
}

#[test]
fn machine_480_partial_root_fixture_matches_every_status_and_identity_row() {
    assert_all_fixture_runs("final-template-partial-root");
}

#[test]
fn partial_entry_without_partial_rules_skips_its_template_battery() {
    let fixture = require_fixture("edge-cases", "final-template-partial-root");
    let grammar = fixture.load_grammar();
    let morpher = Morpher::new(&grammar, usize::MAX);
    let (outcome, _, prune_rows) =
        morpher.parse_word_with_stats_and_prunes("pakfaga", &ParseOptions::default());

    assert!(!outcome.capped, "pakfaga must finish uncapped");
    assert!(!outcome.invalid_shape, "pakfaga must segment");
    assert!(
        outcome.analyses.is_empty(),
        "a partial root cannot license an ordinary rule when its template battery is skipped"
    );
    let final_templates_skipped: u64 = prune_rows
        .iter()
        .map(|row| row.counters.final_templates_skipped)
        .sum();
    let template_batteries_skipped: u64 = prune_rows
        .iter()
        .map(|row| row.counters.template_batteries_skipped)
        .sum();
    assert_eq!(
        final_templates_skipped, 0,
        "the partial-root path skips the whole battery, not one final template"
    );
    assert!(
        template_batteries_skipped > 0,
        "must observe the actual partial-root battery-prune effect, got {prune_rows:?}"
    );
}

#[test]
fn partial_rule_fixture_remains_the_prune_control() {
    assert_all_fixture_runs("final-template-partial-discriminators");

    let fixture = require_fixture("edge-cases", "final-template-partial-discriminators");
    let grammar = fixture.load_grammar();
    let morpher = Morpher::new(&grammar, usize::MAX);
    let positive = morpher.parse_word("pilfbvb");
    assert_eq!(
        result_multiset(&positive.analyses),
        ["PIL+FINTAGB+GATE2PARTIAL|pilfbvb".to_string()]
    );
}
