#[path = "support/toy_fixture.rs"]
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
fn duplicate_confirmed_records_dedup_but_supplied_homographs_remain_distinct() {
    let (grammar, runtime) = setup();
    let signature = runtime.catalog().signatures()[0].id.clone();
    runtime
        .add(AddRequest {
            stem: "milu".into(),
            gloss: String::new(),
            signatures: vec![signature.clone()],
            expected_revision: None,
        })
        .unwrap();
    runtime
        .add(AddRequest {
            stem: "milu".into(),
            gloss: String::new(),
            signatures: vec![signature],
            expected_revision: None,
        })
        .unwrap();
    let mut proposed = official(&grammar, "milu");
    proposed.analyses.push(proposed.analyses[0].clone());
    proposed.structured.push(proposed.structured[0].clone());
    let outcome = runtime.analyze_word("milu", Some(proposed));
    assert_eq!(
        outcome
            .structured
            .iter()
            .filter(|a| matches!(a.provenance, AnalysisProvenance::Grammar))
            .count(),
        1
    );
    assert_eq!(
        outcome
            .structured
            .iter()
            .filter(|a| matches!(a.provenance, AnalysisProvenance::Supplied { .. }))
            .count(),
        2
    );
}

#[test]
fn proposer_rejection_of_a_real_grammar_root_cannot_be_reintroduced_by_guess_retry() {
    let (_grammar, runtime) = setup();
    let rejected = OfficialOutcome {
        analyses: vec![],
        structured: vec![],
        candidates_generated: 0,
    };
    let outcome = runtime.analyze_word("milu", Some(rejected));
    assert!(!outcome
        .structured
        .iter()
        .any(|a| matches!(a.provenance, AnalysisProvenance::Grammar)));
}

/// The unconditional retry used to live directly in `analyze_word`; it is now gated behind
/// `guess_fallback` on `analyze_word_opts` (`analyze_word` itself hardcodes `false` — see
/// `guess_retry_defaults_off_on_the_plain_analyze_word_entry_point` below). This test exercises the
/// retry logic itself via the explicit opt-in, so it still proves "guess runs only after the total
/// official-and-supplied union misses," just through the method that can actually produce it.
#[test]
fn guess_runs_only_after_the_total_official_and_supplied_union_misses() {
    let (grammar, runtime) = setup();
    let missing = runtime.analyze_word_opts("panu", Some(official(&grammar, "panu")), true);
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
    let supplied = runtime.analyze_word_opts("panu", Some(official(&grammar, "panu")), true);
    assert!(!supplied.guessed);
    assert!(supplied
        .structured
        .iter()
        .any(|a| matches!(a.provenance, AnalysisProvenance::Supplied { .. })));
}

/// A minimal synthetic grammar carrying a genuine lexical-PATTERN root (`[Any]*`, the same
/// iterative-shape recipe `pg-ffi`'s own `parse_opts_gate.rs::GRAMMAR_XML` uses for
/// `FfiGuessOptsProbe`) rather than `toy_fixture`'s ordinary literal roots. This matters: the
/// guesser (`pg_parse::guess::lexical_guess`) only ever fabricates an analysis from a lexical
/// pattern (`Morpher::lexical_patterns`, `RootAllomorphDef::is_pattern`) -- `toy_fixture::TOY_XML`
/// has none, so a guess against it always comes back with `guessed: true` but an EMPTY structured
/// set (the word-level flag just means "the guess branch ran and still found nothing"). Only a
/// grammar with a real pattern root can produce a genuinely non-empty guessed analysis, which is
/// what "the capability is not lost, only relocated" needs to prove.
const GUESS_PATTERN_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PgLexiconGuessDefaultProbe</Name>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="ePattern">
            <MorphemeId>PATTERN</MorphemeId>
            <Allomorphs><Allomorph id="aPattern"><PhoneticShape>[Any]*</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pattern</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// Gate: the `pg-lexicon` guesser retry defaults to OFF. `analyze_word` (the plain, pre-existing
/// entry point every caller used before `analyze_word_opts` existed) must return an EMPTY, NOT
/// `guessed` result for a word that is only analyzable by guessing — it must not silently retry
/// through the guesser the way it used to unconditionally. This is the fix for the FFI-boundary
/// overclaim: `hc_parse_word`/`hc_parse_batch` route through exactly this method, and their wire
/// format has no `guessed` field, so a guess must never come back from it at all.
#[test]
fn guess_retry_defaults_off_on_the_plain_analyze_word_entry_point() {
    let grammar = Arc::new(pg_grammar::load(GUESS_PATTERN_XML).unwrap());
    let runtime = SuppliedLexiconRuntime::new(grammar, GUESS_PATTERN_XML).unwrap();

    let missing = runtime.analyze_word("gag", None);
    assert!(
        missing.structured.is_empty(),
        "guess retry must not fire through the default entry point: {:?}",
        missing.structured
    );
    assert!(!missing.guessed);

    // The capability is not gone, only relocated: the explicit opt-in on the very same runtime,
    // same word, finds a REAL (non-vacuous) guessed analysis via the pattern root.
    let opted_in = runtime.analyze_word_opts("gag", None, true);
    assert!(opted_in.guessed);
    assert!(
        !opted_in.structured.is_empty(),
        "the pattern root must produce a real guessed analysis when opted in"
    );
    assert!(opted_in
        .structured
        .iter()
        .all(|a| matches!(a.provenance, AnalysisProvenance::Guessed)));
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
