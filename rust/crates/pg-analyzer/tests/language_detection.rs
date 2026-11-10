//! The language-detection proposer through the public `analyze_text` entry point only.

use pg_analyzer::{
    analyze_text, AnalyzeOptions, DictionaryIdentity, DictionaryPack, LanguageProposal,
    OccurrenceOutcome, ProfileInput, ProposalStatus, TextAnalysis, TextInput, TextRequest,
};

const LDML: &str =
    "<ldml><characters><exemplarCharacters>[a-z]</exemplarCharacters></characters></ldml>";
const AFF: &str = "SET UTF-8\n";
const EN: &str = "the\nthere\ntheir\nother\nmother\nweather\nthing\nthink\nwith\nthat\nthis\nhello\nwater\nstring\nbring";
const ES: &str =
    "casa\ncosa\nperro\ngato\nsenora\nnosotros\npueblo\ncon\nque\nlos\nlas\nagua\nlibro\nnuevo";

fn pack(language: &str, words: &str) -> DictionaryPack {
    let dic = format!("{}\n{words}\n", words.lines().count());
    DictionaryPack::new(
        DictionaryIdentity {
            language: language.into(),
            source_id: format!("{language}-test"),
            source_sha256: "ab".repeat(32),
            version: "1".into(),
            licence: "MIT".into(),
        },
        AFF,
        &dic,
    )
    .unwrap()
}

fn run(dictionaries: &[DictionaryPack], text: &str) -> TextAnalysis {
    run_with(dictionaries, text, AnalyzeOptions::default())
}

fn run_with(dictionaries: &[DictionaryPack], text: &str, options: AnalyzeOptions) -> TextAnalysis {
    let texts = [TextInput {
        id: "t".into(),
        text,
        styles: &[],
    }];
    analyze_text(&TextRequest {
        profile: ProfileInput::Ldml {
            identity: "qaa.ldml",
            xml: LDML,
        },
        packs: &[],
        dictionaries,
        texts: &texts,
        options,
    })
    .unwrap()
}

fn proposal<'a>(analysis: &'a TextAnalysis, language: &str) -> Option<&'a LanguageProposal> {
    analysis
        .detection
        .proposals
        .iter()
        .find(|p| p.language == language)
}

#[test]
fn enough_dictionary_evidence_proposes_the_language_with_counts() {
    let result = run(
        &[pack("en", EN), pack("es", ES)],
        "the water with that thing hello the string",
    );
    let en = proposal(&result, "en").expect("English is proposed");
    assert_eq!(en.status, ProposalStatus::Proposed);
    assert_eq!(en.occurrences.occurrences, 8);
    assert_eq!(en.occurrences.wordforms, 7);
    assert_eq!(en.dictionary.occurrences, 8);
    assert_eq!(en.ngram.occurrences, 0);
    assert!(proposal(&result, "es").is_none());
    assert!(result.detection.sufficient);
    // The proposal is never a verdict on an Occurrence.
    assert_eq!(result.texts[0].occurrences[0].valid_languages, ["en"]);
    assert_eq!(result.texts[0].detection, result.detection);
}

#[test]
fn short_input_stays_ambiguous() {
    let result = run(&[pack("en", EN), pack("es", ES)], "the water");
    let en = proposal(&result, "en").expect("the evidence is still shown");
    assert_eq!(en.status, ProposalStatus::Ambiguous);
    assert_eq!(en.occurrences.occurrences, 2);
    assert!(!result.detection.sufficient);
}

#[test]
fn words_valid_in_two_languages_stay_ambiguous_between_them() {
    let shared = "con\nque\nlos\nlas\nagua\nlibro";
    let result = run(
        &[pack("en", &format!("{EN}\n{shared}")), pack("es", ES)],
        "con que los las agua libro con que",
    );
    for language in ["en", "es"] {
        let p = proposal(&result, language).unwrap();
        assert_eq!(p.status, ProposalStatus::Ambiguous, "{language}");
        assert_eq!(p.shared.occurrences, 8);
    }
    assert!(!result.detection.sufficient);
}

#[test]
fn ngram_evidence_covers_forms_no_dictionary_lists_but_only_with_a_clear_winner() {
    let result = run(
        &[pack("en", EN), pack("es", ES)],
        "hellothere weatherthing motherwater otherthing perrito pueblos gatos casas",
    );
    let en = proposal(&result, "en").unwrap();
    let es = proposal(&result, "es").unwrap();
    assert!(en.ngram.occurrences >= 3, "{en:?}");
    assert!(es.ngram.occurrences >= 3, "{es:?}");
    assert_eq!(en.dictionary.occurrences, 0);
}

#[test]
fn ngram_never_decides_a_very_short_form() {
    let result = run(&[pack("en", EN), pack("es", ES)], "xq zz ab");
    assert!(result.detection.proposals.is_empty());
}

#[test]
fn no_dictionary_packs_means_no_proposals() {
    let result = run(&[], "the water with that thing hello the string");
    assert!(result.detection.proposals.is_empty());
    assert!(!result.detection.sufficient);
}

#[test]
fn four_explained_occurrences_stay_ambiguous_and_five_are_proposed() {
    let packs = [pack("en", EN), pack("es", ES)];
    let four = run(&packs, "the water with that");
    assert_eq!(
        proposal(&four, "en").unwrap().status,
        ProposalStatus::Ambiguous
    );
    let five = run(&packs, "the water with that hello");
    assert_eq!(
        proposal(&five, "en").unwrap().status,
        ProposalStatus::Proposed
    );
}

#[test]
fn an_ngram_tie_between_languages_gives_no_evidence() {
    let result = run(
        &[pack("en", EN), pack("de", EN)],
        "hellothere weatherthing motherwater otherthing waterthing",
    );
    assert!(
        result.detection.proposals.is_empty(),
        "{:?}",
        result.detection
    );
}

#[test]
fn an_occurrence_valid_only_through_an_alternative_reading_still_votes() {
    let joining =
        "<ldml><characters><exemplarCharacters>[a-z \\-]</exemplarCharacters></characters></ldml>";
    let dictionaries = [pack("en", "the\nwater\nhello\nthing")];
    let text = "the-water hello-thing the-thing water-hello hello-water";
    let texts = [TextInput {
        id: "t".into(),
        text,
        styles: &[],
    }];
    let result = analyze_text(&TextRequest {
        profile: ProfileInput::Ldml {
            identity: "qaa.ldml",
            xml: joining,
        },
        packs: &[],
        dictionaries: &dictionaries,
        texts: &texts,
        options: AnalyzeOptions::default(),
    })
    .unwrap();
    let first = &result.texts[0].occurrences[0];
    assert_eq!(
        first.wordform, "the-water",
        "the primary split joins at the hyphen"
    );
    assert_eq!(
        first.valid_languages,
        ["en"],
        "valid through the split reading"
    );
    let en = proposal(&result, "en").expect("the alternative readings vote");
    assert_eq!(en.dictionary.occurrences, 5);
    assert_eq!(en.status, ProposalStatus::Proposed);
}

#[test]
fn a_misspelled_occurrence_still_casts_an_ngram_vote_for_its_language() {
    let result = run(
        &[pack("en", EN), pack("es", ES)],
        "wather wather wather wather wather",
    );
    let outcomes: Vec<_> = result.texts[0]
        .occurrences
        .iter()
        .map(|o| o.outcome)
        .collect();
    assert!(
        outcomes.iter().all(|o| *o == OccurrenceOutcome::Misspelled),
        "{outcomes:?}"
    );
    let en = proposal(&result, "en").expect("a misspelling is still evidence of its language");
    assert_eq!(en.ngram.occurrences, 5);
    assert_eq!(en.dictionary.occurrences, 0);
    assert_eq!(en.status, ProposalStatus::Proposed);
}

#[test]
fn incomplete_occurrences_cast_no_ngram_vote() {
    let text = "hellothere weatherthing motherwater otherthing waterthing";
    let packs = [pack("en", EN), pack("es", ES)];
    let finished = run(&packs, text);
    assert_eq!(
        proposal(&finished, "en").unwrap().ngram.occurrences,
        5,
        "the control votes"
    );
    let capped = run_with(
        &packs,
        text,
        AnalyzeOptions {
            max_suggestion_chars: Some(2),
            ..AnalyzeOptions::default()
        },
    );
    assert!(capped.texts[0]
        .occurrences
        .iter()
        .all(|o| o.outcome == OccurrenceOutcome::Incomplete));
    assert!(
        capped.detection.proposals.is_empty(),
        "{:?}",
        capped.detection
    );
}

#[test]
fn unattempted_occurrences_cast_no_ngram_vote() {
    let result = run_with(
        &[pack("en", EN), pack("es", ES)],
        "hellothere weatherthing motherwater otherthing waterthing",
        AnalyzeOptions {
            max_checked_occurrences: Some(0),
            ..AnalyzeOptions::default()
        },
    );
    assert!(result.texts[0]
        .occurrences
        .iter()
        .all(|o| o.outcome == OccurrenceOutcome::Unattempted));
    assert!(result.detection.proposals.is_empty());
}
