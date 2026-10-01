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

/// The retry is now gated behind `analyze_word_opts`'s `guess_fallback` (`analyze_word` itself hardcodes `false`); this exercises it via the explicit opt-in to prove guess runs only after the official-and-supplied union misses.
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

/// A synthetic grammar carrying a genuine lexical-PATTERN root (`[Any]*`), unlike `toy_fixture`'s ordinary literal roots: only a real pattern root lets the guesser produce a non-empty (not just `guessed: true` but empty) analysis.
const GUESS_PATTERN_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PgLexiconGuessDefaultProbe</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
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

/// Gate: `analyze_word` must return an empty, not-`guessed` result for a word only analyzable by guessing, never silently retrying — `hc_parse_word`/`hc_parse_batch` route through exactly this method, and their wire format has no `guessed` field.
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

    // Not gone, only relocated: the explicit opt-in on the same runtime/word finds a real guessed analysis via the pattern root.
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

#[test]
fn rich_generation_rejects_forged_supplied_payload() {
    let (_grammar, runtime) = setup();
    let signature = runtime.catalog().signatures()[0].id.clone();
    runtime
        .add(AddRequest {
            stem: "milu".into(),
            gloss: String::new(),
            signatures: vec![signature],
            expected_revision: None,
        })
        .unwrap();
    let outcome = runtime.analyze_word("milu", None);
    let mut supplied = outcome
        .structured
        .into_iter()
        .find(|a| a.supplied_root.is_some())
        .expect("supplied analysis");
    supplied.supplied_root.as_mut().unwrap().lexical_spelling = "panu".into();
    let root_index = supplied.root_morpheme_index as usize;
    supplied.morpheme_roots[root_index] = supplied.supplied_root.clone();
    let error = runtime
        .generate_analysis(pg_lexicon::GenerationRequest {
            revision: outcome.revision,
            analysis: supplied,
        })
        .unwrap_err();
    assert_eq!(error.code, "invalid_supplied_root");
}

fn supplied_generation_request(runtime: &SuppliedLexiconRuntime) -> pg_lexicon::GenerationRequest {
    let signature = runtime.catalog().signatures()[0].id.clone();
    runtime
        .add(AddRequest {
            stem: "milu".into(),
            gloss: String::new(),
            signatures: vec![signature],
            expected_revision: None,
        })
        .unwrap();
    let outcome = runtime.analyze_word("milu", None);
    pg_lexicon::GenerationRequest {
        revision: outcome.revision,
        analysis: outcome
            .structured
            .into_iter()
            .find(|a| a.supplied_root.is_some())
            .unwrap(),
    }
}

#[test]
fn rich_generation_preserves_homograph_identities_and_affix_payloads() {
    let (_grammar, runtime) = setup();
    let signature = runtime.catalog().signatures()[0].id.clone();
    for _ in 0..2 {
        runtime
            .add(AddRequest {
                stem: "milu".into(),
                gloss: String::new(),
                signatures: vec![signature.clone()],
                expected_revision: None,
            })
            .unwrap();
    }
    let mut regenerated = Vec::new();
    for word in ["milusi", "miluta"] {
        let outcome = runtime.analyze_word(word, None);
        for analysis in outcome
            .structured
            .into_iter()
            .filter(|a| a.supplied_root.is_some())
        {
            assert_eq!(analysis.morpheme_ids.len(), 2, "root plus plural affix");
            let request = pg_lexicon::GenerationRequest {
                revision: outcome.revision.clone(),
                analysis: analysis.clone(),
            };
            let json = serde_json::to_value(&request).unwrap();
            for field in [
                "synFs",
                "mpr",
                "morphOccurrences",
                "guessedString",
                "morphemeRoots",
            ] {
                assert!(
                    json["analysis"].get(field).is_some(),
                    "missing full analysis field {field}"
                );
            }
            let roundtrip: pg_lexicon::GenerationRequest = serde_json::from_value(json).unwrap();
            assert_eq!(roundtrip.analysis, analysis);
            let generated = runtime.generate_analysis(roundtrip).unwrap();
            assert_eq!(generated.words, vec![word.to_string()]);
            assert_eq!(generated.analysis, analysis);
            assert_eq!(generated.revision, outcome.revision);
            assert_eq!(
                generated.completion,
                pg_lexicon::GenerationCompletion::NotAssessed
            );
            regenerated.push(generated.analysis.supplied_root.unwrap().realization_id);
        }
    }
    assert_eq!(
        regenerated.len(),
        2,
        "two distinct supplied homographs must regenerate"
    );
    assert_ne!(regenerated[0], regenerated[1]);
}

#[test]
fn rich_generation_rejects_all_changed_canonical_root_fields_before_engine_entry() {
    let (_grammar, runtime) = setup();
    let request = supplied_generation_request(&runtime);
    for field in 0..8 {
        let mut changed = request.clone();
        let root = changed.analysis.supplied_root.as_mut().unwrap();
        match field {
            0 => root.entry_id.push('x'),
            1 => root.realization_id.push('x'),
            2 => root.lexical_spelling = "panu".into(),
            3 => root.gloss = "forged".into(),
            4 => root.mpr.0 ^= 1,
            5 => root.stratum.0 = u8::MAX,
            6 => {
                root.authority = pg_parse::RootAuthority::SuppliedOverride {
                    official_entry_id: "forged".into(),
                }
            }
            7 => root.syn_fs = Default::default(),
            _ => unreachable!(),
        }
        assert_ne!(
            changed.analysis.supplied_root,
            request.analysis.supplied_root
        );
        let index = changed.analysis.root_morpheme_index as usize;
        changed.analysis.morpheme_roots[index] = changed.analysis.supplied_root.clone();
        let error = runtime.generate_analysis(changed).unwrap_err();
        assert_eq!(error.code, "invalid_supplied_root", "changed field {field}");
    }
}

#[test]
fn rich_generation_rejects_stale_and_removed_roots() {
    let (_grammar, runtime) = setup();
    let mut request = supplied_generation_request(&runtime);
    let entry_id =
        pg_lexicon::EntryId::parse(&request.analysis.supplied_root.as_ref().unwrap().entry_id)
            .unwrap();
    let removed = runtime
        .remove(pg_lexicon::RemoveRequest {
            id: entry_id,
            expected_revision: Some(request.revision.clone()),
        })
        .unwrap();
    assert_eq!(
        runtime.generate_analysis(request.clone()).unwrap_err().code,
        "revision_conflict"
    );
    request.revision = removed.revision;
    assert_eq!(
        runtime.generate_analysis(request).unwrap_err().code,
        "invalid_supplied_root"
    );
}

#[test]
fn rich_generation_rejects_inconsistent_slots_head_and_provenance() {
    let (_grammar, runtime) = setup();
    let request = supplied_generation_request(&runtime);
    for case in 0..5 {
        let mut changed = request.clone();
        match case {
            0 => changed.analysis.root_morpheme_index = -1,
            1 => {
                changed.analysis.morpheme_roots.clear();
            }
            2 => changed.analysis.supplied_root = None,
            3 => changed.analysis.morpheme_ids[0] = 0,
            4 => changed.analysis.provenance = AnalysisProvenance::Grammar,
            _ => unreachable!(),
        }
        assert_eq!(
            runtime.generate_analysis(changed).unwrap_err().code,
            "invalid_analysis",
            "case {case}"
        );
    }
    let mut missing = request.clone();
    missing.analysis.supplied_root = None;
    missing.analysis.morpheme_roots[0] = None;
    assert_eq!(
        runtime.generate_analysis(missing).unwrap_err().code,
        "invalid_analysis"
    );
    let mut guessed = request;
    guessed.analysis.guessed = true;
    guessed.analysis.provenance = AnalysisProvenance::Guessed;
    assert_eq!(
        runtime.generate_analysis(guessed).unwrap_err().code,
        "unsupported_guessed_generation"
    );
}

#[test]
fn rich_generation_authored_root_control_keeps_existing_generation_semantics() {
    let (grammar, runtime) = setup();
    let outcome = runtime.analyze_word("milusi", None);
    assert!(!outcome.structured.is_empty());
    for analysis in outcome.structured {
        assert_eq!(analysis.provenance, AnalysisProvenance::Grammar);
        let expected = Morpher::new(&grammar, usize::MAX).generate_words_from_analysis(&analysis);
        assert!(!expected.is_empty());
        let generated = runtime
            .generate_analysis(pg_lexicon::GenerationRequest {
                revision: outcome.revision.clone(),
                analysis: analysis.clone(),
            })
            .unwrap();
        assert_eq!(generated.words, expected);
        assert_eq!(generated.analysis, analysis);
    }
}

#[test]
fn rich_generation_rejects_inactive_and_superseded_roots_and_accepts_override() {
    let (_grammar, runtime) = setup();
    let mut request = supplied_generation_request(&runtime);
    let mut document = runtime.export_document();
    document.entries[0].stem = "q".into();
    let reconciled = runtime.import_document(document).unwrap();
    assert_eq!(reconciled.inactive_entries.len(), 1);
    request.revision = reconciled.revision;
    assert_eq!(
        runtime.generate_analysis(request).unwrap_err().code,
        "invalid_supplied_root"
    );

    let base = Arc::new(pg_grammar::load(fixture::TOY_XML).unwrap());
    let unpromoted =
        SuppliedLexiconRuntime::with_sources(base, fixture::TOY_XML, ZeroId, FixedClock).unwrap();
    let mut before_promotion = supplied_generation_request(&unpromoted);
    let official_id = "00000000-0000-0000-0000-000000000000";
    let xml = fixture::TOY_XML.replace("id=\"eHouse\"", &format!("id=\"{official_id}\""));
    let promoted_grammar = Arc::new(pg_grammar::load(&xml).unwrap());
    let promoted =
        SuppliedLexiconRuntime::with_sources(promoted_grammar, &xml, ZeroId, FixedClock).unwrap();
    let added = promoted
        .add(AddRequest {
            stem: "milu".into(),
            gloss: String::new(),
            signatures: vec![promoted.catalog().signatures()[0].id.clone()],
            expected_revision: None,
        })
        .unwrap();
    assert!(matches!(
        added.value.state,
        pg_lexicon::ValidationState::Superseded { .. }
    ));
    before_promotion.revision = added.revision;
    assert_eq!(
        promoted
            .generate_analysis(before_promotion.clone())
            .unwrap_err()
            .code,
        "invalid_supplied_root"
    );
    let changed = promoted
        .set_authority(SetAuthorityRequest {
            id: added.value.id,
            authority: EntryAuthority::SuppliedOverride {
                official_entry_id: official_id.into(),
                note: None,
            },
            expected_revision: None,
        })
        .unwrap();
    before_promotion.revision = changed.revision;
    assert_eq!(
        promoted
            .generate_analysis(before_promotion)
            .unwrap_err()
            .code,
        "invalid_supplied_root"
    );
    let fresh = promoted.analyze_word("milu", None);
    let analysis = fresh
        .structured
        .into_iter()
        .find(|a| a.supplied_root.is_some())
        .unwrap();
    assert!(matches!(
        analysis.provenance,
        AnalysisProvenance::SuppliedOverride { .. }
    ));
    let generated = promoted
        .generate_analysis(pg_lexicon::GenerationRequest {
            revision: fresh.revision,
            analysis: analysis.clone(),
        })
        .unwrap();
    assert_eq!(generated.analysis, analysis);
    assert_eq!(generated.words, vec!["milu".to_string()]);
}

#[test]
fn rich_generation_preserves_supplied_compound_non_heads_with_grammar_head() {
    let rule = r#"<CompoundingRule id="mrC"><Name>compound</Name><CompoundingSubrules><CompoundingSubrule><HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput><NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput><MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput></CompoundingSubrule></CompoundingSubrules></CompoundingRule>"#;
    let xml = fixture::TOY_XML
        .replace(
            "morphologicalRules=\"mrPl\"",
            "morphologicalRules=\"mrPl mrC\"",
        )
        .replace(
            "<MorphologicalRuleDefinitions>",
            &format!("<MorphologicalRuleDefinitions>{rule}"),
        );
    let grammar = Arc::new(pg_grammar::load(&xml).unwrap());
    let runtime = SuppliedLexiconRuntime::new(grammar, &xml).unwrap();
    runtime
        .add(AddRequest {
            stem: "panu".into(),
            gloss: String::new(),
            signatures: vec![runtime.catalog().signatures()[0].id.clone()],
            expected_revision: None,
        })
        .unwrap();
    let outcome = runtime.analyze_word("milupanu", None);
    let analysis = outcome
        .structured
        .into_iter()
        .find(|a| {
            a.provenance == AnalysisProvenance::Grammar
                && a.morpheme_roots.iter().any(Option::is_some)
        })
        .expect("grammar-head/supplied-nonhead compound");
    assert_eq!(analysis.supplied_root, None);
    let generated = runtime
        .generate_analysis(pg_lexicon::GenerationRequest {
            revision: outcome.revision,
            analysis: analysis.clone(),
        })
        .unwrap();
    assert_eq!(generated.analysis, analysis);
    assert!(generated.words.contains(&"milupanu".to_string()));
}
