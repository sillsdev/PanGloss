//! Why-not reports on the snapshot (.fwdata) route carry the FieldWorks targets an author can edit: MoForm, environment and prohibition guids, the environment text and every member morph.

use pg_grammar_model::model::Grammar;
use pg_parse::why_not::{
    expand_guid, why_not, Candidate, ExpectedMorph, FailureKind, FirstFailure, GuidRef,
    WhyNotError, WhyNotOutcome, WhyNotRequest,
};
use pg_parse::Morpher;

const PROJECT: &str = include_str!("data/why-not-fieldworks-guids/project.fwdata");
const GUID: &str = "00000000-0000-0000-0000-";

fn grammar() -> Grammar {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.fwdata");
    std::fs::write(&path, PROJECT).unwrap();
    let (snapshot, _report) = pg_fwdata::import_file(&path).unwrap();
    pg_grammar::compile_project(&snapshot).unwrap().0
}

fn key_of_msa(g: &Grammar, msa: &str) -> String {
    g.morphemes
        .iter()
        .find(|m| m.source_msa_guid.as_deref() == Some(&format!("{GUID}{msa}")))
        .unwrap_or_else(|| panic!("no morpheme for msa {msa}"))
        .xml_key
        .clone()
}

fn failure(g: &Grammar, word: &str, msas: &[&str]) -> FirstFailure {
    let morpher = Morpher::new(g, 5_000_000);
    let positions = msas
        .iter()
        .map(|m| vec![ExpectedMorph::new(key_of_msa(g, m))])
        .collect();
    let request = WhyNotRequest {
        word,
        candidates: vec![Candidate { positions }],
        root_index: None,
    };
    let report = why_not(g, &morpher, &request).unwrap();
    match &report.results[0].outcome {
        WhyNotOutcome::Rejected(f) => {
            assert!(f.kind.is_answered(), "{:?}", f.kind);
            (**f).clone()
        }
        other => panic!("{other:?}"),
    }
}

fn form(n: &str) -> Option<String> {
    Some(format!("{GUID}{n}"))
}

#[test]
fn every_failed_environment_of_an_allomorph_is_reported_with_its_guid_text_and_index() {
    let g = grammar();
    let f = failure(&g, "da", &["000000000102"]);
    assert_eq!(f.kind, FailureKind::EnvironmentUnmet);
    assert_eq!(f.object.as_ref().unwrap().guid, form("000000000101"));
    assert_eq!(f.morphs[0].form_guid, form("000000000101"));
    let seen: Vec<_> = f
        .environments
        .iter()
        .map(|e| (e.index, e.guid.clone(), e.text.clone()))
        .collect();
    assert_eq!(
        seen,
        [
            (Some(0), form("000000000017"), Some("/[V]_".to_string())),
            (Some(1), form("000000000030"), Some("/t_".to_string())),
        ]
    );
    assert!(
        f.plain.contains("/[V]_") && f.plain.contains("/t_"),
        "{}",
        f.plain
    );
    let env = f.environment.as_ref().unwrap();
    assert_eq!(
        (env.guid.as_deref(), env.text.as_deref(), env.index),
        (None, None, None)
    );
    let guid = f.morphs[0].guid.as_ref().unwrap();
    assert_eq!(guid.entry_guid, form("000000000100"));
    assert_eq!(guid.msa_guid, form("000000000102"));
}

#[test]
fn the_second_allomorph_of_an_entry_reports_its_own_form_and_environment() {
    let g = grammar();
    let f = failure(&g, "dat", &["000000000102"]);
    assert_eq!(f.kind, FailureKind::EnvironmentUnmet);
    assert_eq!(f.object.as_ref().unwrap().guid, form("000000000111"));
    assert_eq!(f.morphs[0].form_guid, form("000000000111"));
    let env = f.environment.as_ref().unwrap();
    assert_eq!(env.guid, form("000000000017"));
    assert_eq!(env.text.as_deref(), Some("/[V]_"));
    assert_eq!(env.index, Some(0));
    assert!(f.environments.is_empty());
    assert!(f.plain.contains("(environment /[V]_)"), "{}", f.plain);
}

fn prohibition(word: &str, msas: &[&str], rule: &str, forms: [&str; 2]) {
    let g = grammar();
    let f = failure(&g, word, msas);
    assert_eq!(f.kind, FailureKind::AdhocProhibition, "{word}");
    assert_eq!(f.object.as_ref().unwrap().guid, form(rule), "{word}");
    let seen: Vec<_> = f.members.iter().map(|m| m.form_guid.clone()).collect();
    assert_eq!(seen, [form(forms[0]), form(forms[1])], "{word}");
    assert!(f.members.iter().all(|m| m.guid.is_some()));
}

#[test]
fn a_prohibition_carries_its_own_guid_and_every_member() {
    prohibition(
        "tada",
        &["000000000202", "000000000102"],
        "0000000009a1",
        ["000000000201", "000000000101"],
    );
}

#[test]
fn a_prohibition_with_the_same_primary_and_another_member_is_told_apart() {
    prohibition(
        "tadat",
        &["000000000202", "000000000102"],
        "0000000009a2",
        ["000000000201", "000000000111"],
    );
}

#[test]
fn a_prohibition_with_the_same_member_and_another_primary_is_told_apart() {
    prohibition(
        "tda",
        &["000000000202", "000000000102"],
        "0000000009a4",
        ["000000000211", "000000000101"],
    );
}

#[test]
fn a_guid_names_every_morpheme_sharing_its_entry_and_msa() {
    let mut g = grammar();
    let guid = GuidRef {
        entry_guid: form("000000000100"),
        msa_guid: form("000000000102"),
    };
    let one = expand_guid(&g, &guid, None).unwrap();
    assert_eq!(one.len(), 1);
    let mut twin = g
        .morphemes
        .iter()
        .find(|m| m.xml_key == one[0].morpheme)
        .unwrap()
        .clone();
    twin.xml_key = "twin".into();
    g.morphemes.push(twin);
    let both = expand_guid(&g, &guid, None).unwrap();
    let keys: Vec<_> = both.iter().map(|c| c.morpheme.as_str()).collect();
    assert_eq!(keys, [one[0].morpheme.as_str(), "twin"]);
    assert!(both.iter().all(|c| c.guid.as_ref() == Some(&guid)));
    assert!(matches!(
        expand_guid(&g, &guid, Some("any")),
        Err(WhyNotError::AmbiguousGuidAllomorph { morphemes: 2, .. })
    ));
}

/// A sentence for an author must name morphs by form text and gloss, never by a guid or a `<formguid>#n` key.
fn assert_no_raw_ids(plain: &str) {
    assert!(
        !plain.contains("00000000-0000") && !plain.contains('#'),
        "{plain}"
    );
}

#[test]
fn an_environment_sentence_names_the_allomorph_by_form_and_gloss() {
    let g = grammar();
    for (word, form) in [("da", "da"), ("dat", "dat")] {
        let f = failure(&g, word, &["000000000102"]);
        assert_eq!(f.kind, FailureKind::EnvironmentUnmet);
        assert_no_raw_ids(&f.plain);
        assert!(
            f.plain.starts_with(&format!("{form} 'run' is not allowed")),
            "{}",
            f.plain
        );
        assert!(f.object.as_ref().unwrap().guid.is_some());
    }
}

#[test]
fn a_morph_without_a_gloss_is_named_by_its_form_alone() {
    let g = grammar();
    let f = failure(&g, "da", &["000000000103"]);
    assert_eq!(f.kind, FailureKind::EnvironmentUnmet);
    assert_no_raw_ids(&f.plain);
    assert!(f.plain.starts_with("da is not allowed"), "{}", f.plain);
}

#[test]
fn a_prohibition_sentence_names_both_morphs_by_form_and_gloss() {
    let g = grammar();
    for (word, msas, sentence) in [
        (
            "tada",
            ["000000000202", "000000000102"],
            "ta- 'PA' forbids da 'run'",
        ),
        (
            "tadat",
            ["000000000202", "000000000102"],
            "ta- 'PA' forbids dat 'run'",
        ),
        (
            "tda",
            ["000000000202", "000000000102"],
            "t- 'PA' forbids da 'run'",
        ),
    ] {
        let f = failure(&g, word, &msas);
        assert_eq!(f.kind, FailureKind::AdhocProhibition, "{word}");
        assert_eq!(f.plain, sentence, "{word}");
        assert_eq!(f.position, Some(0), "{word}");
        assert!(f.object.as_ref().unwrap().guid.is_some());
    }
}

#[test]
fn a_no_morph_match_sentence_quotes_no_raw_id() {
    let g = grammar();
    let f = failure(&g, "ad", &["000000000102"]);
    assert_eq!(f.kind, FailureKind::NoMorphMatch);
    assert_no_raw_ids(&f.plain);
}
