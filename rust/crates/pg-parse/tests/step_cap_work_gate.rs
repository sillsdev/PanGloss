//! Synthetic search-work containment and compatibility of the analysis-attempt diagnostic.

#[path = "csharp_port_common/mod.rs"]
mod csharp_port_common;

use pg_parse::{Morpher, ParseOptions};
use pg_rules::stats::{Direction, ObjectKind};

fn suffix_grammar() -> pg_grammar_model::model::Grammar {
    let rule = r#"<MorphologicalRule id="suffix"><Name>Suffix</Name><MorphemeId>SUFFIX</MorphemeId>
      <MorphologicalSubrules><MorphologicalSubrule id="suffixAllo">
        <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
        <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+n</PhoneticShape></InsertSegments></MorphologicalOutput>
      </MorphologicalSubrule></MorphologicalSubrules>
    </MorphologicalRule>"#;
    let lexicon = r#"<LexicalEntry id="root" partOfSpeech="posV"><MorphemeId>ROOT</MorphemeId>
      <Allomorphs><Allomorph id="rootAllo"><PhoneticShape>sag</PhoneticShape></Allomorph></Allomorphs>
    </LexicalEntry>"#;
    csharp_port_common::build_grammar_custom_lexicon(rule, "suffix", lexicon)
}

#[test]
fn parse_reports_the_shared_work_bound_and_keeps_rule_attempt_stats() {
    let grammar = suffix_grammar();
    let morpher = Morpher::new(&grammar, 100_000).with_work_cap(8);
    let (outcome, rows) = morpher.parse_word_with_stats("sagn", &ParseOptions::default());
    assert!(outcome.capped);
    assert!(!outcome.timed_out);
    assert_eq!(outcome.work_steps, 8);
    let attempts: u64 = rows
        .iter()
        .filter(|row| row.kind == ObjectKind::MorphRule && row.direction == Direction::Analysis)
        .map(|row| row.counters.attempts)
        .sum();
    assert_eq!(outcome.steps as u64, attempts);
    assert!(outcome.steps < outcome.work_steps);
}

#[test]
fn ordinary_words_keep_their_analyses_and_attempt_counts_when_the_bound_does_not_fire() {
    let grammar = suffix_grammar();
    let bounded = Morpher::new(&grammar, 100_000);
    let unbounded = Morpher::new(&grammar, usize::MAX);
    for word in ["sag", "sagn"] {
        let full = unbounded.parse_word(word);
        let actual = bounded.parse_word(word);
        assert!(!full.capped && !actual.capped);
        assert!(
            !full.analyses.is_empty(),
            "control must reach confirmation for {word}"
        );
        let mut expected_analyses = full.analyses;
        let mut actual_analyses = actual.analyses;
        expected_analyses.sort();
        actual_analyses.sort();
        assert_eq!(actual_analyses, expected_analyses);
        assert_eq!(
            full.steps, 1,
            "release baseline analysis attempts for {word}"
        );
        assert_eq!(actual.steps, full.steps);
        assert_eq!(actual.work_steps, full.work_steps);
        assert!(actual.work_steps > actual.steps);
    }
    let another = bounded.parse_word("sag");
    assert!(
        !another.capped,
        "the previous word cannot consume this word's allowance"
    );
}

#[test]
fn an_oversized_surface_stops_before_allocating_a_search_shape() {
    let grammar = suffix_grammar();
    let outcome = Morpher::new(&grammar, 100_000)
        .with_work_cap(32)
        .parse_word(&"s".repeat(4096));
    assert!(outcome.capped);
    assert!(!outcome.invalid_shape);
    assert_eq!(outcome.work_steps, 32);
    assert_eq!(outcome.steps, 0);
    assert!(outcome.analyses.is_empty());
}

#[test]
fn invalid_surface_reports_the_work_spent_on_segmentation() {
    let grammar = suffix_grammar();
    let outcome = Morpher::new(&grammar, 100_000).parse_word("!");
    assert!(outcome.invalid_shape);
    assert!(!outcome.capped && !outcome.timed_out);
    assert_eq!(outcome.work_steps, 1);
    assert_eq!(outcome.steps, 0);
}

#[test]
fn an_analysis_attempt_cap_still_confirms_prior_candidates_with_ample_work() {
    let grammar = suffix_grammar();
    let baseline = Morpher::new(&grammar, usize::MAX).parse_word("sagn");
    let actual = Morpher::new(&grammar, 1)
        .with_work_cap(100_000)
        .parse_word("sagn");
    assert!(actual.capped);
    assert!(!actual.timed_out);
    assert_eq!(actual.steps, 1);
    assert!(actual.work_steps > actual.steps && actual.work_steps < 100_000);
    assert!(
        !actual.analyses.is_empty(),
        "analysis CAP cannot discard completed confirmation"
    );
    assert_eq!(actual.signature(), baseline.signature());
}
