//! Dictionary packs through the public `analyze_text` entry point only.

use pg_analyzer::{
    analyze_text, AnalyzeOptions, CheckStatus, DictionaryIdentity, DictionaryPack, EvidenceKind,
    LanguagePack, LimitReason, OccurrenceOutcome, ProfileInput, ResourceKind, TextAnalysis,
    TextInput, TextRequest,
};
use pg_parse::Morpher;

const GRAMMAR: &str = include_str!("../../pg-cli/tests/data/trace-family.xml");
const LDML: &str =
    "<ldml><characters><exemplarCharacters>[a-z]</exemplarCharacters></characters></ldml>";
const AFF: &str = "SET UTF-8\nTRY esianrtolcdugmphbyfvkwzx\n";
const DIC: &str = "4\nthe\nword\nkibes\nlist\n";

fn english() -> DictionaryPack {
    DictionaryPack::new(
        DictionaryIdentity {
            language: "en".into(),
            source_id: "en_US".into(),
            source_sha256: "cd".repeat(32),
            version: "2020.12".into(),
            licence: "MIT".into(),
        },
        AFF,
        DIC,
    )
    .unwrap()
}

fn run(
    grammars: &[&str],
    dictionaries: &[DictionaryPack],
    text: &str,
    options: AnalyzeOptions,
) -> TextAnalysis {
    run_with(LDML, 5_000_000, grammars, dictionaries, text, options)
}

fn run_with(
    ldml: &str,
    work_cap: usize,
    grammars: &[&str],
    dictionaries: &[DictionaryPack],
    text: &str,
    options: AnalyzeOptions,
) -> TextAnalysis {
    let grammar = pg_grammar::load(GRAMMAR).unwrap();
    let morpher = Morpher::new(&grammar, work_cap);
    let packs: Vec<LanguagePack<'_>> = grammars
        .iter()
        .map(|language| LanguagePack {
            language: (*language).into(),
            source_id: format!("{language}.xml"),
            source_sha256: "ab".repeat(32),
            morpher: &morpher,
        })
        .collect();
    let texts = [TextInput {
        id: "t".into(),
        text,
        styles: &[],
    }];
    analyze_text(&TextRequest {
        profile: ProfileInput::Ldml {
            identity: "qaa.ldml",
            xml: ldml,
        },
        packs: &packs,
        dictionaries,
        texts: &texts,
        options,
    })
    .unwrap()
}

#[test]
fn acceptance_is_valid_dictionary_evidence_never_a_parse() {
    let result = run(&[], &[english()], "the", AnalyzeOptions::default());
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Valid);
    assert_eq!(occurrence.valid_languages, ["en"]);
    let check = &occurrence.checks[0];
    assert_eq!(check.status, CheckStatus::Valid);
    assert_eq!(check.evidence_kind, EvidenceKind::DictionaryAcceptance);
    assert_eq!(check.analyses, 0, "no HermitCrab analysis is fabricated");
    assert!(check.suggestions.is_empty());
}

#[test]
fn misspelling_carries_suggestions_but_is_not_valid_and_leaves_text_alone() {
    let result = run(&[], &[english()], "teh", AnalyzeOptions::default());
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.wordform, "teh");
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Misspelled);
    assert!(occurrence.valid_languages.is_empty());
    assert_eq!(occurrence.misspelled_languages, ["en"]);
    let check = &occurrence.checks[0];
    assert_eq!(check.status, CheckStatus::Misspelled);
    assert!(check.suggestions.contains(&"the".to_owned()));
    let stats = &result.corpus;
    assert_eq!(stats.valid.occurrences, 0);
    assert_eq!(
        (stats.misspelled.occurrences, stats.misspelled.wordforms),
        (1, 1)
    );
    let en = &stats.languages["en"];
    assert_eq!(en.valid.occurrences, 0);
    assert_eq!(en.misspelled.occurrences, 1);
}

#[test]
fn a_miss_with_no_suggestions_is_no_analysis_never_misspelled() {
    let result = run(&[], &[english()], "qqqqqqqqqqqq", AnalyzeOptions::default());
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.checks[0].status, CheckStatus::NoAnalysis);
    assert_eq!(occurrence.outcome, OccurrenceOutcome::NoAnalysis);
    assert!(occurrence.misspelled_languages.is_empty());
    assert_eq!(result.corpus.misspelled.occurrences, 0);
    assert_eq!(result.corpus.languages["en"].no_analysis.occurrences, 1);
}

#[test]
fn vernacular_parse_stays_visible_when_a_dictionary_also_accepts() {
    let result = run(&["xx"], &[english()], "kibes", AnalyzeOptions::default());
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.valid_languages, ["xx", "en"]);
    let kinds: Vec<_> = occurrence.checks.iter().map(|c| c.evidence_kind).collect();
    assert_eq!(
        kinds,
        [
            EvidenceKind::LanguagePackAnalysis,
            EvidenceKind::DictionaryAcceptance
        ]
    );
    assert!(occurrence.checks[0].analyses > 0);
    let stats = &result.corpus;
    assert_eq!(stats.ambiguous.occurrences, 1);
    assert_eq!(stats.languages["xx"].valid.occurrences, 1);
    assert_eq!(stats.languages["en"].valid.occurrences, 1);
    assert_eq!(stats.languages["en"].ambiguous.occurrences, 1);
}

/// English whose only near neighbour of "vem" is "vet" (the grammar parses "vem" as `xx`).
fn english_with_vet(language: &str) -> DictionaryPack {
    DictionaryPack::new(
        DictionaryIdentity {
            language: language.into(),
            source_id: format!("{language}_vet"),
            source_sha256: "cd".repeat(32),
            version: "1".into(),
            licence: "MIT".into(),
        },
        AFF,
        "2\nvet\nthe\n",
    )
    .unwrap()
}

#[test]
fn valid_vernacular_with_an_english_misspelling_candidate_keeps_both_facts() {
    let setup = run(
        &[],
        &[english_with_vet("en")],
        "vem",
        AnalyzeOptions::default(),
    );
    let guard = &setup.texts[0].occurrences[0];
    assert_eq!(guard.outcome, OccurrenceOutcome::Misspelled);
    assert!(guard.checks[0].suggestions.contains(&"vet".to_owned()));

    let result = run(
        &["xx"],
        &[english_with_vet("en")],
        "vem",
        AnalyzeOptions::default(),
    );
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Valid);
    assert_eq!(occurrence.valid_languages, ["xx"]);
    assert_eq!(occurrence.misspelled_languages, ["en"]);
    let stats = &result.corpus;
    assert_eq!(stats.valid.occurrences, 1);
    assert_eq!(
        stats.misspelled.occurrences, 0,
        "the partition counts it valid"
    );
    let en = &stats.languages["en"];
    assert_eq!((en.misspelled.occurrences, en.misspelled.wordforms), (1, 1));
    assert_eq!(en.valid.occurrences, 0);
    let xx = &stats.languages["xx"];
    assert_eq!((xx.valid.occurrences, xx.valid.wordforms), (1, 1));
    assert_eq!(xx.misspelled.occurrences, 0);
}

#[test]
fn for_one_language_valid_beats_misspelled() {
    let result = run(
        &["xx"],
        &[english_with_vet("xx")],
        "vem",
        AnalyzeOptions::default(),
    );
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Valid);
    assert_eq!(occurrence.valid_languages, ["xx"]);
    assert!(occurrence.misspelled_languages.is_empty());
    let xx = &result.corpus.languages["xx"];
    assert_eq!(xx.valid.occurrences, 1);
    assert_eq!(xx.misspelled.occurrences, 0);
    assert_eq!(result.corpus.ambiguous.occurrences, 0, "one language, once");
}

#[test]
fn a_suggestion_search_over_the_bound_is_incomplete_never_silently_empty() {
    let options = AnalyzeOptions {
        max_suggestion_chars: Some(2),
        ..AnalyzeOptions::default()
    };
    let result = run(&[], &[english()], "teh", options);
    let occurrence = &result.texts[0].occurrences[0];
    let check = &occurrence.checks[0];
    assert_eq!(check.status, CheckStatus::Incomplete);
    assert_eq!(check.limits, [LimitReason::SuggestionsCapped]);
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Incomplete);
    assert_eq!(result.corpus.no_analysis.occurrences, 0);
    assert_eq!(result.corpus.misspelled.occurrences, 0);
}

#[test]
fn misspelled_languages_is_optional_when_reading_older_json() {
    let result = run(&[], &[english()], "the", AnalyzeOptions::default());
    let mut value = serde_json::to_value(&result).unwrap();
    value["texts"][0]["occurrences"][0]
        .as_object_mut()
        .unwrap()
        .remove("misspelled_languages");
    let parsed: TextAnalysis = serde_json::from_value(value).unwrap();
    assert!(parsed.texts[0].occurrences[0]
        .misspelled_languages
        .is_empty());
}

#[test]
fn word_list_membership_is_evidence_and_absence_rejects_nothing() {
    let pack = english().with_word_list(["zork"]);
    let result = run(&[], &[pack], "zork zorp the", AnalyzeOptions::default());
    let o = &result.texts[0].occurrences;
    assert_eq!(o[0].outcome, OccurrenceOutcome::Valid);
    assert_eq!(
        o[0].checks[0].evidence_kind,
        EvidenceKind::WordListMembership
    );
    assert_ne!(o[1].outcome, OccurrenceOutcome::Valid);
    assert_eq!(
        o[2].checks[0].evidence_kind,
        EvidenceKind::DictionaryAcceptance
    );
    let bare = run(&[], &[english()], "zork", AnalyzeOptions::default());
    assert_ne!(
        bare.texts[0].occurrences[0].outcome,
        OccurrenceOutcome::Valid
    );
}

#[test]
fn identities_record_source_version_hash_and_licence() {
    let result = run(&["xx"], &[english()], "the", AnalyzeOptions::default());
    let r = &result.resources;
    assert_eq!(r.len(), 3);
    assert_eq!(r[2].kind, ResourceKind::DictionaryPack);
    assert_eq!(r[2].source_id, "en_US");
    assert_eq!(r[2].source_sha256, "cd".repeat(32));
    assert_eq!(r[2].version.as_deref(), Some("2020.12"));
    assert_eq!(r[2].licence.as_deref(), Some("MIT"));
    assert_eq!(r[2].language.as_deref(), Some("en"));
    let check = &result.texts[0].occurrences[0].checks[1];
    assert_eq!(check.resource, 2);
}

#[test]
fn occurrence_limit_leaves_dictionary_checks_unattempted() {
    let result = run(
        &[],
        &[english()],
        "the word",
        AnalyzeOptions {
            max_checked_occurrences: Some(1),
            ..AnalyzeOptions::default()
        },
    );
    let skipped = &result.texts[0].occurrences[1];
    assert_eq!(skipped.checks[0].status, CheckStatus::Unattempted);
    assert_eq!(skipped.outcome, OccurrenceOutcome::Unattempted);
}

#[test]
fn result_with_dictionary_evidence_round_trips_through_json() {
    let result = run(
        &["xx"],
        &[english()],
        "teh kibes",
        AnalyzeOptions::default(),
    );
    let json = serde_json::to_string(&result).unwrap();
    assert_eq!(serde_json::from_str::<TextAnalysis>(&json).unwrap(), result);
}

#[test]
fn malformed_hunspell_data_is_refused() {
    let identity = DictionaryIdentity {
        language: "en".into(),
        source_id: "x".into(),
        source_sha256: "cd".repeat(32),
        version: "1".into(),
        licence: "MIT".into(),
    };
    assert!(DictionaryPack::new(identity, AFF, "not a count\nword\n").is_err());
}

#[test]
fn an_unfinished_grammar_search_outranks_a_misspelling() {
    let result = run_with(
        LDML,
        1,
        &["xx"],
        &[english_with_vet("en")],
        "vem",
        AnalyzeOptions::default(),
    );
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.checks[0].status, CheckStatus::Incomplete);
    assert_eq!(occurrence.checks[1].status, CheckStatus::Misspelled);
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Incomplete);
    assert_eq!(result.corpus.misspelled.occurrences, 0);
    assert_eq!(result.corpus.incomplete.occurrences, 1);
}

const LDML_JOINING: &str = "<ldml><characters><exemplarCharacters>[a-z \\u0027 \\-]</exemplarCharacters></characters></ldml>";

#[test]
fn an_alternative_only_win_hides_primary_misspellings_and_misses() {
    let dictionary = || english_with_vet("en");
    let setup = run_with(
        LDML_JOINING,
        5_000_000,
        &[],
        &[dictionary()],
        "kib-vem",
        AnalyzeOptions::default(),
    );
    let guard = &setup.texts[0].occurrences[0];
    assert_eq!(
        guard.checks[0].status,
        CheckStatus::Misspelled,
        "dictionary flags the primary"
    );
    let result = run_with(
        LDML_JOINING,
        5_000_000,
        &["xx"],
        &[dictionary()],
        "kib-vem",
        AnalyzeOptions::default(),
    );
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Valid);
    assert_eq!(occurrence.valid_languages, ["xx"]);
    assert!(occurrence.misspelled_languages.is_empty());
    let stats = &result.corpus;
    assert_eq!(stats.languages["xx"].valid.occurrences, 1);
    assert_eq!(stats.languages["xx"].no_analysis.occurrences, 0);
    let en = &stats.languages["en"];
    assert_eq!(en.misspelled.occurrences, 0);
    assert_eq!(
        en.no_analysis.occurrences, 0,
        "the primary's miss is ignored"
    );
    assert_eq!(en.valid.occurrences, 0);
}
