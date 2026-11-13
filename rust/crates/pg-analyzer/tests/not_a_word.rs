//! "Not a word" tokens (ADR 0016) through the public `analyze_text` entry point only.

use pg_analyzer::{
    analyze_text, usfm_styles, AnalyzeOptions, DictionaryIdentity, DictionaryPack,
    OccurrenceOutcome, ProfileInput, Rule, SplitCharacter, TextAnalysis, TextInput, TextRequest,
};

const LDML: &str = "<ldml><characters><exemplarCharacters>[\\u0027 \\- a b d e g-i k-p r-u w y]</exemplarCharacters><exemplarCharacters type=\"auxiliary\">[c f j q v x z \\u00F1]</exemplarCharacters></characters></ldml>";

fn cebuano() -> DictionaryPack {
    let identity = DictionaryIdentity {
        language: "ceb".into(),
        source_id: "ceb".into(),
        source_sha256: "ef".repeat(32),
        version: "1".into(),
        licence: "MIT".into(),
    };
    let words = ["Ang", "matag", "usa", "kasing", "kamao", "b", "x"];
    DictionaryPack::new(identity, "SET UTF-8\n", "0\n")
        .unwrap()
        .with_word_list(words.map(str::to_owned).to_vec())
}

fn run_with(
    profile: ProfileInput<'_>,
    text: &str,
    usfm: bool,
    options: AnalyzeOptions,
) -> TextAnalysis {
    let styles = if usfm { usfm_styles(text) } else { Vec::new() };
    let dictionaries = [cebuano()];
    let texts = [TextInput {
        id: "t".into(),
        text,
        styles: &styles,
    }];
    analyze_text(&TextRequest {
        profile,
        packs: &[],
        dictionaries: &dictionaries,
        texts: &texts,
        options,
    })
    .unwrap()
}

fn run(text: &str, usfm: bool, options: AnalyzeOptions) -> TextAnalysis {
    let profile = ProfileInput::Ldml {
        identity: "ceb.ldml",
        xml: LDML,
    };
    run_with(profile, text, usfm, options)
}

fn skipped(result: &TextAnalysis) -> Vec<&str> {
    result.texts[0]
        .occurrences
        .iter()
        .filter(|o| o.outcome == OccurrenceOutcome::NotAWord)
        .map(|o| o.wordform.as_str())
        .collect()
}

#[test]
fn ceb_examples_digits_and_foreign_characters_are_not_words_with_the_character_reported() {
    let result = run(
        "16 3:16 16th 3a ' Ang Ñga d\u{00e9}",
        false,
        AnalyzeOptions::default(),
    );
    // `'` is in the main set but is no letter; the digits and the foreign `é` are not valid.
    assert_eq!(
        skipped(&result),
        ["16", "3", "16", "16th", "3a", "'", "d\u{00e9}"]
    );
    let counts = &result.corpus.not_a_word;
    assert_eq!(counts.total.occurrences, 7);
    assert_eq!(counts.by_character["'"].occurrences, 1);
    assert_eq!(counts.by_character["1"].occurrences, 3);
    assert_eq!(counts.by_character["3"].occurrences, 2);
    assert_eq!(counts.by_character["\u{00e9}"].occurrences, 1);
    let info = result.texts[0].occurrences[0].not_a_word.as_ref().unwrap();
    assert_eq!(info.character.as_deref(), Some("1"));
    // The rest are words: `Ñ` folds to `ñ` (auxiliary); nothing else counts them.
    assert_eq!(result.corpus.total.occurrences, 2);
    assert_eq!(result.corpus.misspelled.occurrences, 0);
    assert!(result.texts[0].occurrences[0].checks.is_empty());
    assert_eq!(result.not_a_word_rule, Rule::ValidCharacters);
}

#[test]
fn a_decomposed_letter_matches_its_composed_exemplar() {
    let result = run("n\u{0303}a", false, AnalyzeOptions::default());
    assert!(skipped(&result).is_empty());
}

#[test]
fn without_exemplar_sets_the_digit_or_no_letter_rule_applies_and_the_summary_says_so() {
    let ldml = ProfileInput::Ldml {
        identity: "bare.ldml",
        xml: "<ldml/>",
    };
    let result = run_with(
        ldml,
        "16 3a ' \u{00e9}t\u{00e9}",
        false,
        AnalyzeOptions::default(),
    );
    assert_eq!(skipped(&result), ["16", "3a"]);
    assert_eq!(result.not_a_word_rule, Rule::FallbackDigitOrNoLetter);
    assert_eq!(result.corpus.not_a_word.by_character["1"].occurrences, 1);
}

#[test]
fn a_leading_apostrophe_is_counted_through_the_split_without_it() {
    let result = run("'Ang", false, AnalyzeOptions::default());
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Valid);
    assert_eq!(occurrence.alternative_split, [SplitCharacter::Apostrophe]);
    assert_eq!(
        result.corpus.valid_via_alternative.apostrophe.occurrences,
        1
    );
}

#[test]
fn hyphen_splits_are_counted_as_valid_only_through_an_alternative() {
    let result = run(
        "matag-usa kasing-kasing kasing qqq-kamao",
        false,
        AnalyzeOptions::default(),
    );
    let via = &result.corpus.valid_via_alternative;
    assert_eq!(via.total.occurrences, 2);
    assert_eq!(via.hyphen.occurrences, 2);
    assert_eq!(via.apostrophe.occurrences, 0);
    // `kasing` is analysed directly, so it is not "only through an alternative".
    assert_eq!(result.corpus.valid.occurrences, 3);
    assert_eq!(result.corpus.no_analysis.occurrences, 1);
    assert_eq!(result.corpus.not_a_word.total.occurrences, 0);
}

#[test]
fn a_single_letter_in_the_valid_set_is_an_ordinary_word() {
    let result = run("x kamao", false, AnalyzeOptions::default());
    assert!(skipped(&result).is_empty());
    assert_eq!(result.corpus.valid.occurrences, 2);
}

#[test]
fn accept_entries_pass_and_win_over_the_character_rule_and_reject_pattern() {
    let options = AnalyzeOptions {
        accept_words: vec!["16TH".into()],
        accept_patterns: vec![r"^\d+a$".into()],
        reject_pattern: Some("kamao".into()),
        ..AnalyzeOptions::default()
    };
    let result = run("16th 3a 4 kamao usa", false, options);
    // `4` fails the character rule; `kamao` fails the extra reject; the accepted ones pass.
    assert_eq!(skipped(&result), ["4", "kamao"]);
    assert_eq!(result.corpus.not_a_word.reject_pattern.occurrences, 1);
    let accepted = AnalyzeOptions {
        accept_words: vec!["kamao".into()],
        reject_pattern: Some("kamao".into()),
        ..AnalyzeOptions::default()
    };
    assert!(skipped(&run("kamao", false, accepted)).is_empty());
}

#[test]
fn the_character_rule_can_be_switched_off() {
    let off = AnalyzeOptions {
        no_character_rule: true,
        ..AnalyzeOptions::default()
    };
    let result = run("16 kamao", false, off);
    assert!(skipped(&result).is_empty());
    assert_eq!(result.corpus.total.occurrences, 2);
    assert_eq!(result.not_a_word_rule, Rule::Off);
}

#[test]
fn an_invalid_pattern_is_an_error_not_a_silent_default() {
    let options = AnalyzeOptions {
        reject_pattern: Some("(".into()),
        ..AnalyzeOptions::default()
    };
    let dictionaries = [cebuano()];
    let texts = [TextInput {
        id: "t".into(),
        text: "a",
        styles: &[],
    }];
    let result = analyze_text(&TextRequest {
        profile: ProfileInput::Ldml {
            identity: "x",
            xml: LDML,
        },
        packs: &[],
        dictionaries: &dictionaries,
        texts: &texts,
        options,
    });
    assert!(result.unwrap_err().contains("reject"));
}

#[test]
fn the_segmentation_profile_can_carry_the_exceptions() {
    let profile = pg_analyzer::analyze(
        "",
        ProfileInput::Ldml {
            identity: "ceb.ldml",
            xml: LDML,
        },
        &[],
    )
    .unwrap()
    .profile;
    assert_eq!(profile.auxiliary.len(), 8);
    let with_exceptions = pg_analyzer::SegmentationProfile {
        accept_words: vec!["3a".into()],
        reject_pattern: Some("^usa$".into()),
        ..profile
    };
    let json = serde_json::to_string(&with_exceptions).unwrap();
    let result = run_with(
        ProfileInput::Json(&json),
        "3a 16 usa",
        false,
        AnalyzeOptions::default(),
    );
    assert_eq!(skipped(&result), ["16", "usa"]);
}

const USFM: &str =
    "\\id GEN Test\n\\c 1\n\\p\n\\v 1 Ang kamao\n\\v 2-3 matag usa\n\\c 12\n\\v 4a x\n";

#[test]
fn usfm_chapter_and_verse_numbers_and_markers_never_reach_the_word_list() {
    let result = run(USFM, true, AnalyzeOptions::default());
    let words: Vec<_> = result.texts[0]
        .occurrences
        .iter()
        .filter(|o| o.outcome != OccurrenceOutcome::NotAWord)
        .map(|o| o.wordform.as_str())
        .collect();
    assert_eq!(words, ["Ang", "kamao", "matag", "usa", "x"]);
    assert_eq!(skipped(&result), ["1", "1", "2-3", "12", "4a"]);
    assert_eq!(result.corpus.not_a_word.style.occurrences, 5);
    // The style-based exclusion holds even when the filter rejects nothing.
    let off = AnalyzeOptions {
        no_character_rule: true,
        ..AnalyzeOptions::default()
    };
    let result = run(USFM, true, off);
    assert_eq!(skipped(&result), ["1", "1", "2-3", "12", "4a"]);
}

#[test]
fn plain_text_mode_sees_the_markers_as_tokens() {
    let result = run(USFM, false, AnalyzeOptions::default());
    assert!(result.corpus.total.occurrences > 5);
}

#[test]
fn summary_only_keeps_the_statistics_and_drops_the_occurrence_list() {
    let options = AnalyzeOptions {
        summary_only: true,
        ..AnalyzeOptions::default()
    };
    let result = run("16 kamao", false, options);
    assert!(result.texts[0].occurrences.is_empty());
    assert_eq!(result.texts[0].statistics.not_a_word.total.occurrences, 1);
    assert_eq!(
        result.texts[0].statistics.not_a_word.forms[0].wordform,
        "16"
    );
    assert_eq!(result.corpus.valid.occurrences, 1);
}

#[test]
fn not_a_word_does_not_spend_the_checked_occurrence_budget() {
    let options = AnalyzeOptions {
        max_checked_occurrences: Some(1),
        ..AnalyzeOptions::default()
    };
    let result = run("16 3 kamao", false, options);
    assert_eq!(result.corpus.valid.occurrences, 1);
    assert_eq!(result.corpus.unattempted.occurrences, 0);
}
