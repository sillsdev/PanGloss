//! Tests call only the public `analyze_text` entry point.

use pg_analyzer::{
    analyze_text, AnalyzeOptions, CheckStatus, LanguagePack, LimitReason, OccurrenceOutcome,
    ProfileInput, ReadingKind, ResourceKind, TextAnalysis, TextInput, TextRequest,
};
use pg_parse::Morpher;

const GRAMMAR: &str = include_str!("../../pg-cli/tests/data/trace-family.xml");
const LDML: &str =
    "<ldml><characters><exemplarCharacters>[a-z]</exemplarCharacters></characters></ldml>";

fn run(
    packs: &[(&str, &Morpher<'_>)],
    texts: &[(&str, &str)],
    options: AnalyzeOptions,
) -> TextAnalysis {
    let packs: Vec<LanguagePack<'_>> = packs
        .iter()
        .map(|(language, morpher)| LanguagePack {
            language: (*language).into(),
            source_id: format!("{language}.xml"),
            source_sha256: "ab".repeat(32),
            morpher,
        })
        .collect();
    let texts: Vec<TextInput<'_>> = texts
        .iter()
        .map(|(id, text)| TextInput {
            id: (*id).into(),
            text,
            styles: &[],
        })
        .collect();
    analyze_text(&TextRequest {
        profile: ProfileInput::Ldml {
            identity: "qaa.ldml",
            xml: LDML,
        },
        packs: &packs,
        dictionaries: &[],
        texts: &texts,
        options,
    })
    .unwrap()
}

#[test]
fn occurrences_carry_wordform_span_evidence_and_resource_identities() {
    let grammar = pg_grammar::load(GRAMMAR).unwrap();
    let morpher = Morpher::new(&grammar, 5_000_000);
    let result = run(
        &[("xx", &morpher)],
        &[("t1", "kibes, qqqq 16th")],
        AnalyzeOptions::default(),
    );
    let occurrences = &result.texts[0].occurrences;
    let words: Vec<_> = occurrences.iter().map(|o| o.wordform.as_str()).collect();
    assert_eq!(words, ["kibes", "qqqq", "16th"].map(str::to_owned).to_vec());
    assert_eq!(occurrences[0].span.utf8, [0, 5]);
    assert_eq!(occurrences[1].span.code_points, [7, 11]);
    assert_eq!(occurrences[2].outcome, OccurrenceOutcome::NotAWord);
    assert!(occurrences[2].checks.is_empty());
    assert_eq!(occurrences[0].outcome, OccurrenceOutcome::Valid);
    assert_eq!(occurrences[0].valid_languages, ["xx"]);
    assert!(occurrences[0].checks[0].analyses >= 1);
    assert_eq!(occurrences[1].outcome, OccurrenceOutcome::Incomplete);
    assert_eq!(occurrences[1].checks[0].status, CheckStatus::Incomplete);
    assert_eq!(occurrences[1].checks[0].limits, [LimitReason::InvalidShape]);
    assert_eq!(result.resources[0].kind, ResourceKind::SegmentationProfile);
    assert_eq!(result.resources[0].source_id, "qaa.ldml");
    assert_eq!(result.resources[1].language.as_deref(), Some("xx"));
    assert_eq!(occurrences[0].checks[0].resource, 1);
}

#[test]
fn a_finished_miss_is_no_analysis_but_an_interrupted_search_is_incomplete() {
    let grammar = pg_grammar::load(GRAMMAR).unwrap();
    let roomy = Morpher::new(&grammar, 5_000_000);
    let starved = Morpher::new(&grammar, 1);
    let miss = run(
        &[("xx", &roomy)],
        &[("t", "zozozo")],
        AnalyzeOptions::default(),
    );
    assert_eq!(
        miss.texts[0].occurrences[0].outcome,
        OccurrenceOutcome::NoAnalysis
    );
    let capped = run(
        &[("xx", &starved)],
        &[("t", "kibes")],
        AnalyzeOptions::default(),
    );
    let occurrence = &capped.texts[0].occurrences[0];
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Incomplete);
    assert!(occurrence.checks[0].limits.contains(&LimitReason::Capped));
    assert_eq!(capped.corpus.no_analysis.occurrences, 0);
    assert_eq!(capped.corpus.incomplete.occurrences, 1);
}

#[test]
fn occurrences_and_wordforms_are_counted_apart_per_text_and_per_corpus() {
    let grammar = pg_grammar::load(GRAMMAR).unwrap();
    let morpher = Morpher::new(&grammar, 5_000_000);
    let result = run(
        &[("xx", &morpher)],
        &[("a", "kibes kibes vem zozozo"), ("b", "kibes zozozo kib")],
        AnalyzeOptions::default(),
    );
    let a = &result.texts[0].statistics;
    assert_eq!((a.total.occurrences, a.total.wordforms), (4, 3));
    assert_eq!((a.valid.occurrences, a.valid.wordforms), (3, 2));
    assert_eq!((a.no_analysis.occurrences, a.no_analysis.wordforms), (1, 1));
    let corpus = &result.corpus;
    assert_eq!((corpus.total.occurrences, corpus.total.wordforms), (7, 4));
    assert_eq!(
        (corpus.valid.occurrences, corpus.valid.wordforms),
        (5, 3),
        "kibes, vem and kib"
    );
    let xx = &corpus.languages["xx"].valid;
    assert_eq!((xx.occurrences, xx.wordforms), (5, 3));
}

#[test]
fn a_form_valid_in_two_packs_is_ambiguous_and_counted_once_per_language() {
    let grammar = pg_grammar::load(GRAMMAR).unwrap();
    let morpher = Morpher::new(&grammar, 5_000_000);
    let result = run(
        &[("xx", &morpher), ("yy", &morpher)],
        &[("t", "kibes")],
        AnalyzeOptions::default(),
    );
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.valid_languages, ["xx", "yy"]);
    let stats = &result.corpus;
    assert_eq!(
        (stats.valid.occurrences, stats.ambiguous.occurrences),
        (1, 1)
    );
    assert_eq!(stats.languages["xx"].valid.occurrences, 1);
    assert_eq!(stats.languages["yy"].valid.occurrences, 1);
}

#[test]
fn no_packs_and_request_limits_are_unattempted_not_failed() {
    let none = run(&[], &[("t", "kibes vem")], AnalyzeOptions::default());
    assert!(none.texts[0]
        .occurrences
        .iter()
        .all(|o| o.outcome == OccurrenceOutcome::Unattempted && o.checks.is_empty()));
    assert_eq!(none.corpus.unattempted.occurrences, 2);
    assert_eq!(none.corpus.no_analysis.occurrences, 0);

    let grammar = pg_grammar::load(GRAMMAR).unwrap();
    let morpher = Morpher::new(&grammar, 5_000_000);
    let limited = run(
        &[("xx", &morpher)],
        &[("t", "kibes vem kib")],
        AnalyzeOptions {
            max_checked_occurrences: Some(1),
            ..AnalyzeOptions::default()
        },
    );
    let outcomes: Vec<_> = limited.texts[0]
        .occurrences
        .iter()
        .map(|o| o.outcome)
        .collect();
    assert_eq!(
        outcomes,
        [
            OccurrenceOutcome::Valid,
            OccurrenceOutcome::Unattempted,
            OccurrenceOutcome::Unattempted
        ]
    );
    let skipped = &limited.texts[0].occurrences[1].checks[0];
    assert_eq!(skipped.status, CheckStatus::Unattempted);
    assert_eq!(
        skipped.unattempted_reason.as_deref(),
        Some("occurrence_limit")
    );
    assert_eq!(limited.corpus.total.occurrences, 3);
}

#[test]
fn result_round_trips_through_json_and_rejects_bad_requests() {
    let grammar = pg_grammar::load(GRAMMAR).unwrap();
    let morpher = Morpher::new(&grammar, 5_000_000);
    let result = run(
        &[("xx", &morpher)],
        &[("t", "kibes zozozo")],
        AnalyzeOptions::default(),
    );
    assert_eq!(result.schema, "pangloss-text-analysis/v1");
    let json = serde_json::to_string(&result).unwrap();
    assert_eq!(serde_json::from_str::<TextAnalysis>(&json).unwrap(), result);

    let pack = |language: &str| LanguagePack {
        language: language.into(),
        source_id: "g.xml".into(),
        source_sha256: "ab".repeat(32),
        morpher: &morpher,
    };
    let packs = [pack("xx"), pack("xx")];
    let error = analyze_text(&TextRequest {
        profile: ProfileInput::Ldml {
            identity: "qaa.ldml",
            xml: LDML,
        },
        packs: &packs,
        dictionaries: &[],
        texts: &[],
        options: AnalyzeOptions::default(),
    })
    .unwrap_err();
    assert!(error.contains("more than one Language Pack"), "{error}");
}

const LDML_JOINING: &str = "<ldml><characters><exemplarCharacters>[a-z \\u0027 \\-]</exemplarCharacters></characters></ldml>";

fn run_joining(texts: &[(&str, &str)], options: AnalyzeOptions) -> TextAnalysis {
    let grammar = pg_grammar::load(GRAMMAR).unwrap();
    let morpher = Morpher::new(&grammar, 5_000_000);
    let packs = [LanguagePack {
        language: "xx".into(),
        source_id: "xx.xml".into(),
        source_sha256: "ab".repeat(32),
        morpher: &morpher,
    }];
    let texts: Vec<TextInput<'_>> = texts
        .iter()
        .map(|(id, text)| TextInput {
            id: (*id).into(),
            text,
            styles: &[],
        })
        .collect();
    analyze_text(&TextRequest {
        profile: ProfileInput::Ldml {
            identity: "qaa.ldml",
            xml: LDML_JOINING,
        },
        packs: &packs,
        dictionaries: &[],
        texts: &texts,
        options,
    })
    .unwrap()
}

fn parts(reading: &pg_analyzer::Reading) -> Vec<&str> {
    reading.parts.iter().map(|p| p.wordform.as_str()).collect()
}

#[test]
fn any_analysed_alternative_makes_the_occurrence_valid_in_the_order_the_issue_gives() {
    let result = run_joining(&[("t", "kib-vem")], AnalyzeOptions::default());
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.wordform, "kib-vem");
    assert_eq!(occurrence.checks[0].status, CheckStatus::Incomplete);
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Valid);
    assert_eq!(occurrence.valid_languages, ["xx"]);
    let readings = &occurrence.readings;
    assert_eq!(readings[0].kind, ReadingKind::Primary);
    assert_eq!(parts(&readings[0]), ["kib-vem"]);
    assert!(!readings[0].analysed);
    assert_eq!(
        parts(&readings[1]),
        ["kib", "vem"],
        "all parts analysed next"
    );
    assert!(readings[1].analysed);
    assert_eq!(readings[1].kind, ReadingKind::Alternative);
    assert_eq!(parts(&readings[2]), ["kib-", "vem"], "the rest last");
    assert!(!readings[2].analysed);
    let spans: Vec<_> = readings[1]
        .parts
        .iter()
        .map(|p| p.span.code_points)
        .collect();
    assert_eq!(spans, [[0, 3], [4, 7]]);
    assert_eq!(readings[1].parts[1].span.utf8, [4, 7]);
    assert_eq!(readings[1].parts[0].valid_languages, ["xx"]);
}

#[test]
fn missed_alternatives_are_not_counted_and_counts_do_not_grow_with_alternatives() {
    let result = run_joining(
        &[("t", "kib-vem kib-vem kib-zozozo zozozo")],
        AnalyzeOptions::default(),
    );
    let stats = &result.texts[0].statistics;
    assert_eq!((stats.total.occurrences, stats.total.wordforms), (4, 3));
    assert_eq!((stats.valid.occurrences, stats.valid.wordforms), (2, 1));
    assert_eq!(
        (stats.no_analysis.occurrences, stats.no_analysis.wordforms),
        (1, 1)
    );
    assert_eq!(
        (stats.incomplete.occurrences, stats.incomplete.wordforms),
        (1, 1),
        "the joined form has a shape the grammar rejects"
    );
    assert_eq!(stats.languages["xx"].valid.occurrences, 2);
    let miss = &result.texts[0].occurrences[2];
    assert_eq!(miss.outcome, OccurrenceOutcome::Incomplete);
    assert!(miss.readings.len() > 1);
}

#[test]
fn an_occurrence_without_dual_role_characters_has_only_its_primary_reading() {
    let result = run_joining(&[("t", "kibes")], AnalyzeOptions::default());
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.readings.len(), 1);
    assert_eq!(occurrence.readings[0].kind, ReadingKind::Primary);
    assert!(occurrence.readings[0].analysed);
    assert!(occurrence.limits.is_empty());
}

#[test]
fn a_capped_occurrence_is_incomplete_not_dropped() {
    let options = AnalyzeOptions {
        max_alternatives: Some(1),
        ..AnalyzeOptions::default()
    };
    let result = run_joining(&[("t", "kib-vem-kib")], options);
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.readings.len(), 2, "primary and one alternative");
    assert_eq!(occurrence.limits, [LimitReason::AlternativesCapped]);
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Incomplete);
    let stats = &result.corpus;
    assert_eq!(
        (stats.total.occurrences, stats.incomplete.occurrences),
        (1, 1)
    );
    assert_eq!(stats.no_analysis.occurrences, 0);

    let roomy = run_joining(&[("t", "kib-vem-kib")], AnalyzeOptions::default());
    let found = &roomy.texts[0].occurrences[0];
    assert_eq!(found.outcome, OccurrenceOutcome::Valid);
    assert!(found.limits.is_empty());
}

#[test]
fn a_valid_occurrence_keeps_the_cap_limit_it_hit() {
    let options = AnalyzeOptions {
        max_alternatives: Some(2),
        ..AnalyzeOptions::default()
    };
    let result = run_joining(&[("t", "kib-vem-kib-vem-kib-vem")], options);
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.limits, [LimitReason::AlternativesCapped]);
    assert!(occurrence.readings.len() <= 3);
    assert_eq!(result.corpus.total.occurrences, 1);
}

#[test]
fn unattempted_occurrences_generate_no_alternatives() {
    let options = AnalyzeOptions {
        max_checked_occurrences: Some(0),
        ..AnalyzeOptions::default()
    };
    let result = run_joining(&[("t", "kib-vem")], options);
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Unattempted);
    assert_eq!(occurrence.readings.len(), 1);
}

#[test]
fn an_edge_hyphen_gives_a_reading_without_it() {
    let result = run_joining(&[("t", "-kib")], AnalyzeOptions::default());
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.wordform, "-kib");
    let words: Vec<_> = occurrence.readings.iter().map(parts).collect();
    assert!(words.contains(&vec!["kib"]), "{words:?}");
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Valid);
}

fn run_grammars(
    grammars: &[(&str, String)],
    ldml: &str,
    text: &str,
    options: AnalyzeOptions,
) -> TextAnalysis {
    let loaded: Vec<_> = grammars
        .iter()
        .map(|(_, xml)| pg_grammar::load(xml).unwrap())
        .collect();
    let morphers: Vec<_> = loaded.iter().map(|g| Morpher::new(g, 5_000_000)).collect();
    let packs: Vec<LanguagePack<'_>> = grammars
        .iter()
        .zip(&morphers)
        .map(|((language, _), morpher)| LanguagePack {
            language: (*language).into(),
            source_id: format!("{language}.xml"),
            source_sha256: "ab".repeat(32),
            morpher,
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
        dictionaries: &[],
        texts: &texts,
        options,
    })
    .unwrap()
}

#[test]
fn per_language_counts_come_from_valid_languages_when_only_an_alternative_is_valid() {
    let result = run_joining(&[("t", "kib-vem")], AnalyzeOptions::default());
    let stats = &result.corpus;
    assert_eq!(stats.languages["xx"].valid.occurrences, 1);
    assert_eq!(stats.languages["xx"].valid.wordforms, 1);
    assert_eq!(
        (stats.valid.occurrences, stats.no_analysis.occurrences),
        (1, 0)
    );
    assert_eq!(stats.incomplete.occurrences, 0);
}

#[test]
fn a_mixed_language_alternative_is_analysed_but_never_ambiguous() {
    let xx = GRAMMAR.replace(
        "<PhoneticShape>vem</PhoneticShape>",
        "<PhoneticShape>vez</PhoneticShape>",
    );
    let yy = GRAMMAR.replace(
        "<PhoneticShape>kib</PhoneticShape>",
        "<PhoneticShape>kiz</PhoneticShape>",
    );
    let result = run_grammars(
        &[("xx", xx), ("yy", yy)],
        LDML_JOINING,
        "kib-vem",
        AnalyzeOptions::default(),
    );
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Valid);
    assert_eq!(occurrence.valid_languages, ["xx", "yy"]);
    let mixed = &occurrence.readings[1];
    assert_eq!(parts(mixed), ["kib", "vem"]);
    assert_eq!(mixed.parts[0].valid_languages, ["xx"]);
    assert_eq!(mixed.parts[1].valid_languages, ["yy"]);
    let stats = &result.corpus;
    assert_eq!(stats.ambiguous.occurrences, 0);
    assert_eq!(stats.languages["xx"].valid.occurrences, 1);
    assert_eq!(stats.languages["yy"].valid.occurrences, 1);
}

#[test]
fn two_languages_that_each_analyse_a_reading_alone_are_ambiguous() {
    let result = run_grammars(
        &[("xx", GRAMMAR.to_owned()), ("yy", GRAMMAR.to_owned())],
        LDML_JOINING,
        "kib-vem",
        AnalyzeOptions::default(),
    );
    assert_eq!(result.corpus.ambiguous.occurrences, 1);
}

fn hyphenated_grammar() -> String {
    GRAMMAR
        .replace(
            "<SegmentDefinitions>",
            "<SegmentDefinitions><SegmentDefinition id=\"chy\"><Representations><Representation>-</Representation></Representations></SegmentDefinition>",
        )
        .replace("<PhoneticShape>vem</PhoneticShape>", "<PhoneticShape>ve-m</PhoneticShape>")
}

#[test]
fn alternatives_are_skipped_for_an_analysed_primary_unless_all_readings_is_set() {
    let run_with = |all_readings| {
        run_grammars(
            &[("xx", hyphenated_grammar())],
            LDML_JOINING,
            "ve-m",
            AnalyzeOptions {
                all_readings,
                ..AnalyzeOptions::default()
            },
        )
    };
    let skipped = run_with(false);
    let occurrence = &skipped.texts[0].occurrences[0];
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Valid);
    assert_eq!(occurrence.readings.len(), 1);
    let all = run_with(true);
    let full = &all.texts[0].occurrences[0];
    assert_eq!(full.outcome, OccurrenceOutcome::Valid);
    assert!(full.readings.len() > 1);
    assert_eq!(all.corpus, skipped.corpus);
}

#[test]
fn readings_are_distinct_and_never_repeat_the_primary_split() {
    let result = run_joining(&[("t", "kib--vem")], AnalyzeOptions::default());
    let words: Vec<Vec<&str>> = result.texts[0].occurrences[0]
        .readings
        .iter()
        .map(parts)
        .collect();
    let mut unique = words.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), words.len(), "{words:?}");
    assert_eq!(words.iter().filter(|w| **w == ["kib--vem"]).count(), 1);
}

#[test]
fn a_curly_quote_the_profile_makes_word_forming_gives_split_readings() {
    let ldml = "<ldml><characters><exemplarCharacters>[a-z \\u2019]</exemplarCharacters></characters></ldml>";
    let result = run_grammars(
        &[("xx", GRAMMAR.to_owned())],
        ldml,
        "kib\u{2019}vem",
        AnalyzeOptions::default(),
    );
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.wordform, "kib\u{2019}vem");
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Valid);
    let words: Vec<_> = occurrence.readings.iter().map(parts).collect();
    assert!(words.contains(&vec!["kib\u{2019}", "vem"]), "{words:?}");
    assert!(words.contains(&vec!["kib", "vem"]), "{words:?}");
}

#[test]
fn zero_alternatives_reports_every_dual_role_occurrence_capped_not_dropped() {
    let options = AnalyzeOptions {
        max_alternatives: Some(0),
        ..AnalyzeOptions::default()
    };
    let result = run_joining(&[("t", "kib-vem")], options);
    let occurrence = &result.texts[0].occurrences[0];
    assert_eq!(occurrence.readings.len(), 1);
    assert_eq!(occurrence.limits, [LimitReason::AlternativesCapped]);
    assert_eq!(occurrence.outcome, OccurrenceOutcome::Incomplete);
}

#[test]
fn part_spans_are_exact_in_every_unit_for_multibyte_text() {
    let options = AnalyzeOptions {
        no_character_rule: true,
        ..AnalyzeOptions::default()
    };
    let result = run_joining(&[("t", "kib 𐐀é-é")], options);
    let occurrence = &result.texts[0].occurrences[1];
    assert_eq!(occurrence.wordform, "𐐀é-é");
    let split = occurrence
        .readings
        .iter()
        .find(|r| parts(r) == ["𐐀é", "é"])
        .expect("split reading");
    assert_eq!(split.parts[0].span.code_points, [4, 6]);
    assert_eq!(split.parts[0].span.utf8, [4, 10]);
    assert_eq!(split.parts[0].span.utf16, [4, 7]);
    assert_eq!(split.parts[1].span.code_points, [7, 8]);
    assert_eq!(split.parts[1].span.utf8, [11, 13]);
    assert_eq!(split.parts[1].span.utf16, [8, 9]);
}
