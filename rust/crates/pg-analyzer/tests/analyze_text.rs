//! Tests call only the public `analyze_text` entry point.

use pg_analyzer::{
    analyze_text, AnalyzeOptions, CheckStatus, LanguagePack, LimitReason, OccurrenceOutcome,
    ProfileInput, ResourceKind, TextAnalysis, TextInput, TextRequest,
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
        texts: &[],
        options: AnalyzeOptions::default(),
    })
    .unwrap_err();
    assert!(error.contains("more than one Language Pack"), "{error}");
}
