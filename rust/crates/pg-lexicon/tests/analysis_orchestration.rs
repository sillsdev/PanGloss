#[path = "toy_fixture.rs"]
mod fixture;

use pg_lexicon::{
    AddRequest, AnalysisCache, EntryAuthority, OfficialOutcome, SetAuthorityRequest,
    SetGlossLanguageRequest, SuppliedLexiconRuntime,
};
use pg_parse::{AnalysisProvenance, Morpher};
use std::sync::Arc;

struct ZeroId;
impl pg_lexicon::IdSource for ZeroId {
    fn next_128(&mut self) -> Result<[u8; 16], pg_lexicon::StructuredError> {
        Ok([0; 16])
    }
}
struct FixedClock;
impl pg_lexicon::Clock for FixedClock {
    fn now(&mut self) -> pg_lexicon::LexicalDate {
        pg_lexicon::LexicalDate::parse("2026-07-22 00:00:00.000").unwrap()
    }
}

fn setup() -> (Arc<pg_grammar::model::Grammar>, SuppliedLexiconRuntime) {
    let grammar = Arc::new(pg_grammar::load(fixture::TOY_XML).unwrap());
    let runtime = SuppliedLexiconRuntime::new(grammar.clone(), fixture::TOY_XML).unwrap();
    (grammar, runtime)
}

fn official(grammar: &pg_grammar::model::Grammar, word: &str) -> OfficialOutcome {
    let outcome = Morpher::new(grammar, usize::MAX).parse_word(word);
    OfficialOutcome {
        analyses: outcome.analyses,
        structured: outcome.structured,
        candidates_generated: outcome.candidates_generated,
    }
}

#[test]
fn confirmed_official_and_supplied_paths_union_without_guessing_or_case_folding() {
    let (grammar, runtime) = setup();
    let signature = runtime.catalog().signatures()[0].id.clone();
    runtime
        .add(AddRequest {
            stem: "milu".into(),
            gloss: String::new(),
            signatures: vec![signature],
            expected_revision: None,
        })
        .unwrap();
    let outcome = runtime.analyze_word("milu", Some(official(&grammar, "milu")));
    assert!(outcome
        .structured
        .iter()
        .any(|a| matches!(a.provenance, AnalysisProvenance::Grammar)));
    assert!(outcome
        .structured
        .iter()
        .any(|a| matches!(a.provenance, AnalysisProvenance::Supplied { .. })));
    assert!(!outcome.guessed);

    let upper = runtime.analyze_word("MILU", Some(official(&grammar, "MILU")));
    assert!(
        upper.invalid_shape || upper.structured.is_empty(),
        "analysis must preserve authored case, not lowercase input"
    );
}

#[test]
fn guess_runs_only_after_the_total_official_and_supplied_union_misses() {
    let (grammar, runtime) = setup();
    let missing = runtime.analyze_word("panu", Some(official(&grammar, "panu")));
    assert!(missing.guessed);
    assert!(missing
        .structured
        .iter()
        .all(|a| matches!(a.provenance, AnalysisProvenance::Guessed)));

    let signature = runtime.catalog().signatures()[0].id.clone();
    runtime
        .add(AddRequest {
            stem: "panu".into(),
            gloss: String::new(),
            signatures: vec![signature],
            expected_revision: None,
        })
        .unwrap();
    let supplied = runtime.analyze_word("panu", Some(official(&grammar, "panu")));
    assert!(!supplied.guessed);
    assert!(supplied
        .structured
        .iter()
        .any(|a| matches!(a.provenance, AnalysisProvenance::Supplied { .. })));
}

#[test]
fn explicit_override_suppresses_the_matching_official_and_keeps_override_provenance() {
    let official_id = "00000000-0000-0000-0000-000000000000";
    let xml = fixture::TOY_XML.replace("id=\"eHouse\"", &format!("id=\"{official_id}\""));
    let grammar = Arc::new(pg_grammar::load(&xml).unwrap());
    let runtime =
        SuppliedLexiconRuntime::with_sources(grammar.clone(), &xml, ZeroId, FixedClock).unwrap();
    let signature = runtime.catalog().signatures()[0].id.clone();
    let added = runtime
        .add(AddRequest {
            stem: "milu".into(),
            gloss: String::new(),
            signatures: vec![signature],
            expected_revision: None,
        })
        .unwrap();
    runtime
        .set_authority(SetAuthorityRequest {
            id: added.value.id,
            authority: EntryAuthority::SuppliedOverride {
                official_entry_id: official_id.into(),
                note: None,
            },
            expected_revision: None,
        })
        .unwrap();
    let outcome = runtime.analyze_word("milu", Some(official(&grammar, "milu")));
    assert!(!outcome
        .structured
        .iter()
        .any(|a| matches!(a.provenance, AnalysisProvenance::Grammar)));
    assert!(outcome.structured.iter().any(|a| matches!(a.provenance, AnalysisProvenance::SuppliedOverride { ref overridden_grammar_entry_id, .. } if overridden_grammar_entry_id == official_id)));
}

#[test]
fn cache_is_revision_and_exact_spelling_aware_and_metadata_edits_evict_old_entries() {
    let (grammar, runtime) = setup();
    let mut cache = AnalysisCache::default();
    let first = runtime.analyze_word("milu", Some(official(&grammar, "milu")));
    let old_revision = first.revision.clone();
    cache.insert(first, "milu".into());
    assert!(cache.get(&old_revision, "milu").is_some());
    assert!(cache.get(&old_revision, "Milu").is_none());

    runtime
        .set_gloss_language(SetGlossLanguageRequest {
            gloss_language: Some("en".into()),
            expected_revision: None,
        })
        .unwrap();
    let current = runtime.snapshot().revision().clone();
    assert!(cache.get(&current, "milu").is_none());
    cache.insert(
        runtime.analyze_word("milu", Some(official(&grammar, "milu"))),
        "milu".into(),
    );
    assert_eq!(cache.len(), 1);
    assert!(cache.get(&old_revision, "milu").is_none());
}
