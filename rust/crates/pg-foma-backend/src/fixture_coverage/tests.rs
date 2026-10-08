use super::*;
use pg_conformance_fixtures::require_fixture;

fn nonrecursive_fixture() -> (FixtureRef, Grammar, WordsYaml) {
    let fixture = require_fixture("edge-cases", "compounding-non-recursive");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    let words = fixture.load_words_yaml();
    (fixture, grammar, words)
}

#[test]
fn variant_tag_must_match_the_owning_predicates_observed_variant() {
    let (fixture, grammar, mut words) = nonrecursive_fixture();
    let positive = words
        .words
        .iter_mut()
        .find(|w| w.word == "fasubel")
        .unwrap();
    positive
        .exercises
        .push("compounding.non-recursive".to_string());
    positive.exercises.push("compounding.recursive".to_string());
    let report = coverage_for_grammar(&fixture.label(), &grammar, &words);
    assert!(report
        .passing_constructs
        .contains("compounding.non-recursive"));
    assert!(!report.passing_constructs.contains("compounding.recursive"));
    assert_eq!(report.invalid_variant_tags.len(), 1);
}

#[test]
fn skipped_invisible_and_mismatching_words_supply_no_coverage() {
    let (fixture, grammar, mut words) = nonrecursive_fixture();
    let positive = words
        .words
        .iter()
        .find(|w| w.word == "fasubel")
        .unwrap()
        .clone();
    words.words = vec![positive.clone()];
    let tag = "compounding.non-recursive";
    assert!(coverage_for_grammar(&fixture.label(), &grammar, &words)
        .passing_constructs
        .contains(tag));
    words.words[0].parses[0].signature = "WRONG".to_string();
    assert!(coverage_for_grammar(&fixture.label(), &grammar, &words)
        .passing_constructs
        .is_empty());
    words.words[0] = positive.clone();
    words.words[0].expect_skip = true;
    assert!(coverage_for_grammar(&fixture.label(), &grammar, &words)
        .passing_constructs
        .is_empty());
    words.words[0] = positive.clone();
    words.words[0].parses[0].guess = true;
    assert!(coverage_for_grammar(&fixture.label(), &grammar, &words)
        .passing_constructs
        .is_empty());
    words.words[0] = positive.clone();
    words.words[0].word = "#".to_string();
    words.words[0].parses.clear();
    assert!(coverage_for_grammar(&fixture.label(), &grammar, &words)
        .passing_constructs
        .is_empty());
    words.words[0] = positive;
    words.expect_crash = true;
    assert!(coverage_for_grammar(&fixture.label(), &grammar, &words)
        .passing_constructs
        .is_empty());
    words.expect_crash = false;
    words.budget_ms = Some(1);
    assert!(coverage_for_grammar(&fixture.label(), &grammar, &words)
        .passing_constructs
        .is_empty());
}

#[test]
fn a_broken_fixture_grammar_is_a_named_load_failure() {
    let (mut fixture, _, _) = nonrecursive_fixture();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../.tmp")
        .join(format!("coverage-load-failure-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("grammar.xml"), "<HermitCrabInput>").unwrap();
    std::fs::copy(fixture.words_yaml_path(), dir.join("words.yaml")).unwrap();
    fixture.dir = dir.clone();
    fixture.name = "broken-coverage-grammar".to_string();
    let report = passing_covered_constructs(&[fixture.clone()]);
    std::fs::remove_dir_all(dir).unwrap();
    assert!(report.passing_constructs.is_empty());
    assert_eq!(report.load_failures.len(), 1);
    assert_eq!(report.load_failures[0].fixture, fixture.label());
    assert!(!report.load_failures[0].error.is_empty());
}

#[test]
fn upstream_tag_extension_requires_the_original_passing_word_tag() {
    let fixture = require_fixture("edge-cases", "iterative-epenthesis-cascade");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    let mut words = fixture.load_words_yaml();
    let tag = "epenthesis.structural-route";
    assert!(coverage_for_grammar(&fixture.label(), &grammar, &words)
        .passing_constructs
        .contains(tag));
    for word in &mut words.words {
        word.exercises.clear();
        for parse in &mut word.parses {
            parse.exercises.clear();
        }
    }
    assert!(!coverage_for_grammar(&fixture.label(), &grammar, &words)
        .passing_constructs
        .contains(tag));
}
