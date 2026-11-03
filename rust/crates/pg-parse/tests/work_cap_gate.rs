//! Synthetic search-work containment: one work cap bounds a word, and ordinary words never see it.

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
fn a_word_capped_on_work_alone_reports_cap_and_keeps_rule_attempt_stats() {
    let grammar = suffix_grammar();
    let morpher = Morpher::new(&grammar, 8);
    let (outcome, rows) = morpher.parse_word_with_stats("sagn", &ParseOptions::default());
    assert!(outcome.capped);
    assert!(!outcome.timed_out);
    assert_eq!(outcome.work_steps, 8);
    let attempts: u64 = rows
        .iter()
        .filter(|row| row.kind == ObjectKind::MorphRule && row.direction == Direction::Analysis)
        .map(|row| row.counters.attempts)
        .sum();
    assert!(attempts > 0, "the stats rows still record rule attempts");
    assert!(attempts < outcome.work_steps as u64);
}

#[test]
fn ordinary_words_keep_their_analyses_and_work_when_the_bound_does_not_fire() {
    let grammar = suffix_grammar();
    let bounded = Morpher::new(&grammar, 10_000_000);
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
        assert_eq!(actual.work_steps, full.work_steps);
    }
    let another = bounded.parse_word("sag");
    assert!(
        !another.capped,
        "the previous word cannot consume this word's allowance"
    );
}

/// `k` homophonous one-shot suffix rules, so the unwind is combinatorial and analysis is nearly all of the work.
fn homophonous_grammar(k: usize) -> pg_grammar_model::model::Grammar {
    let mut rules = String::new();
    let mut ids = Vec::new();
    for i in 0..k {
        rules.push_str(&format!(
            r#"<MorphologicalRule id="mrN{i}"><Name>n_suffix_{i}</Name><MorphemeId>N{i}</MorphemeId>
              <MorphologicalSubrules><MorphologicalSubrule id="subN{i}">
                <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+n</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule></MorphologicalSubrules>
            </MorphologicalRule>"#
        ));
        ids.push(format!("mrN{i}"));
    }
    let lexicon = r#"<LexicalEntry id="root" partOfSpeech="posV"><MorphemeId>ROOT</MorphemeId>
      <Allomorphs><Allomorph id="rootAllo"><PhoneticShape>sag</PhoneticShape></Allomorph></Allomorphs>
    </LexicalEntry>"#;
    csharp_port_common::build_grammar_custom_lexicon(&rules, &ids.join(" "), lexicon)
}

/// Analysis uses the whole cap and the reserve sits on top of it: a candidate found before analysis hit the cap is confirmed only because the reserve exists, so the word is `CAP`, keeps that analysis, and spends more than the cap but never more than `cap + cap / 16`.
#[test]
fn a_candidate_found_before_the_cap_is_confirmed_in_the_reserve() {
    let grammar = homophonous_grammar(4);
    let mut confirmed_in_reserve = 0;
    for cap in (0..60).map(|step| 200 + step * 100) {
        let outcome = Morpher::new(&grammar, cap).parse_word("sagnnnn");
        assert!(
            outcome.work_steps <= cap + cap / 16,
            "cap {cap}: {} work steps exceed the bound",
            outcome.work_steps
        );
        if outcome.capped && !outcome.analyses.is_empty() && outcome.work_steps > cap {
            confirmed_in_reserve += 1;
        }
    }
    assert!(
        confirmed_in_reserve > 0,
        "no cap left analysis stopped and a root confirmed beyond the cap"
    );
}

/// A word whose total work is within the cap never stops: at `cap = work` its status, analyses and work equal the unbounded run. This is the proof the reserve was designed to give, checked on every conformance word that finishes quickly.
#[test]
fn a_word_run_at_a_cap_equal_to_its_work_is_unchanged() {
    use pg_conformance_fixtures::discover;
    let (mut checked, mut at_boundary) = (0usize, 0usize);
    for fixture in discover() {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        let unbounded = Morpher::new(&grammar, 20_000_000);
        for entry in fixture.load_words_yaml().words {
            if !entry.adapter_visible() {
                continue;
            }
            let free = unbounded.parse_word(&entry.word);
            if free.capped || free.timed_out || free.invalid_shape {
                continue;
            }
            let bounded = Morpher::new(&grammar, free.work_steps).parse_word(&entry.word);
            let label = format!("{}: {}", fixture.label(), entry.word);
            assert_eq!(bounded.capped, free.capped, "{label}");
            assert_eq!(bounded.signature(), free.signature(), "{label}");
            assert_eq!(bounded.work_steps, free.work_steps, "{label}");
            checked += 1;
            at_boundary += usize::from(free.work_steps >= 16);
        }
    }
    assert!(checked > 100, "only {checked} words were checked");
    assert!(at_boundary > 100, "only {at_boundary} words had a reserve");
}
#[test]
fn an_oversized_surface_stops_before_allocating_a_search_shape() {
    let grammar = suffix_grammar();
    let outcome = Morpher::new(&grammar, 32).parse_word(&"s".repeat(4096));
    assert!(outcome.capped);
    assert!(!outcome.invalid_shape);
    assert_eq!(outcome.work_steps, 32);
    assert!(outcome.analyses.is_empty());
}

#[test]
fn invalid_surface_reports_the_work_spent_on_segmentation() {
    let grammar = suffix_grammar();
    let outcome = Morpher::new(&grammar, 10_000_000).parse_word("!");
    assert!(outcome.invalid_shape);
    assert!(!outcome.capped && !outcome.timed_out);
    assert_eq!(outcome.work_steps, 1);
}

/// Generation has its own limit: the work cap given to `Morpher::new` is for parsing, and a caller that sets neither pays for neither.
#[test]
fn generation_reads_its_own_cap_not_the_parse_work_cap() {
    use pg_featstruct::FeatureStruct;
    use pg_parse::GenerateWordsStopReason;
    let grammar = suffix_grammar();
    let root = csharp_port_common::lex_entry_id(&grammar, "ROOT");

    let tight = Morpher::new(&grammar, 10_000_000).with_generation_cap(1);
    let outcome = tight.generate_words_bounded(root, &[], FeatureStruct::EMPTY);
    assert_eq!(
        outcome.stop_reason,
        Some(GenerateWordsStopReason::WorkLimitReached { limit: 1 })
    );

    let roomy = Morpher::new(&grammar, 1).with_generation_cap(10_000_000);
    let outcome = roomy.generate_words_bounded(root, &[], FeatureStruct::EMPTY);
    assert_eq!(outcome.stop_reason, None);
    assert!(!outcome.surfaces.is_empty());
    assert!(
        roomy.parse_word("sag").capped,
        "the parse cap of 1 still binds parsing"
    );

    let default = Morpher::new(&grammar, 1);
    assert_eq!(
        default
            .generate_words_bounded(root, &[], FeatureStruct::EMPTY)
            .stop_reason,
        Some(GenerateWordsStopReason::WorkLimitReached { limit: 1 }),
        "with no explicit generation cap it equals the work cap"
    );
}
