use super::super::*;
use crate::common::WsForm;
use crate::feature::{
    ClosedFeature, FeatureStructure, FeatureSystem, FeatureValue, FeatureValueKind,
    FeatureValueSymbol,
};
use crate::lexicon::{Allomorph, LexEntry, Lexicon, Msa, Sense};
use crate::morphology::{MorphType, Morphology, ParserParameters, PartOfSpeech};
use crate::phonology::{Phoneme, Phonology};

fn ws(ws: &str, form: &str) -> WsForm {
    WsForm {
        ws: ws.to_string(),
        form: form.to_string(),
    }
}

/// A small but structurally rich snapshot: one closed feature, one phoneme, one POS, one lex entry with a stem MSA and a sense referencing it.
fn sample_snapshot() -> Snapshot {
    let noun_pos_guid = "11111111-1111-1111-1111-111111111111".to_string();
    let number_feature_guid = "22222222-2222-2222-2222-222222222222".to_string();
    let sg_guid = "33333333-3333-3333-3333-333333333333".to_string();
    let pl_guid = "44444444-4444-4444-4444-444444444444".to_string();
    let phoneme_guid = "55555555-5555-5555-5555-555555555555".to_string();
    let entry_guid = "66666666-6666-6666-6666-666666666666".to_string();
    let allomorph_guid = "77777777-7777-7777-7777-777777777777".to_string();
    let msa_guid = "88888888-8888-8888-8888-888888888888".to_string();
    let sense_guid = "99999999-9999-9999-9999-999999999999".to_string();

    let project = Project {
        name: "Test Project".to_string(),
        vernacular_writing_systems: vec!["sen".to_string()],
        analysis_writing_systems: vec!["en".to_string()],
        exemplar_characters: Vec::new(),
    };

    let feature_systems = FeatureSystems {
        phonological: FeatureSystem::default(),
        morphosyntactic: FeatureSystem {
            closed_features: vec![ClosedFeature {
                guid: number_feature_guid.clone(),
                name: "Number".to_string(),
                abbreviation: "num".to_string(),
                values: vec![
                    FeatureValueSymbol {
                        guid: sg_guid.clone(),
                        name: "singular".to_string(),
                        abbreviation: "sg".to_string(),
                    },
                    FeatureValueSymbol {
                        guid: pl_guid.clone(),
                        name: "plural".to_string(),
                        abbreviation: "pl".to_string(),
                    },
                ],
            }],
            complex_features: vec![],
        },
    };

    let phonology = Phonology {
        phonemes: vec![Phoneme {
            guid: phoneme_guid,
            name: "a".to_string(),
            representations: vec![ws("sen", "a")],
            features: None,
            basic_ipa_symbol: Some("a".to_string()),
        }],
        ..Phonology::default()
    };

    let morphology = Morphology {
        parts_of_speech: vec![PartOfSpeech {
            guid: noun_pos_guid.clone(),
            name: "Noun".to_string(),
            abbreviation: "n".to_string(),
            children: vec![],
            inflection_classes: vec![],
            default_inflection_class: None,
            inflectable_features: vec![number_feature_guid.clone()],
            stem_names: vec![],
            affix_slots: vec![],
            affix_templates: vec![],
        }],
        compound_rules: vec![],
        adhoc_prohibitions: vec![],
        exception_features: vec![],
        lex_entry_infl_types: vec![],
        parser_parameters: ParserParameters::default(),
    };

    let lexicon = Lexicon {
        entries: vec![LexEntry {
            guid: entry_guid,
            citation_form: vec![ws("sen", "kanga")],
            lexeme_morph_type: MorphType::Stem,
            allomorphs: vec![Allomorph {
                guid: allomorph_guid,
                morph_type: MorphType::Stem,
                is_abstract: false,
                forms: vec![ws("sen", "kanga")],
                environments: vec![],
                positions: vec![],
                stem_name: None,
                inflection_classes: vec![],
                ms_env_features: None,
                ms_env_part_of_speech: None,
                process: None,
            }],
            msas: vec![Msa::Stem {
                guid: msa_guid.clone(),
                part_of_speech: Some(noun_pos_guid),
                inflection_class: None,
                features: Some(FeatureStructure {
                    values: vec![FeatureValue {
                        feature: number_feature_guid,
                        value: FeatureValueKind::Closed { value: sg_guid },
                    }],
                }),
                exception_features: vec![],
                from_parts_of_speech: vec![],
                slots: vec![],
            }],
            senses: vec![Sense {
                guid: sense_guid,
                gloss: vec![ws("en", "dog")],
                definition: vec![],
                msa: Some(msa_guid),
            }],
            entry_refs: vec![],
        }],
    };

    Snapshot::new(project, feature_systems, phonology, morphology, lexicon)
}

#[test]
fn round_trips_through_json() {
    let snap = sample_snapshot();
    let json = snap.to_json();
    let parsed = Snapshot::from_json(&json).expect("valid snapshot JSON must parse");
    assert_eq!(snap, parsed);
}

#[test]
fn to_json_is_deterministic_across_calls() {
    let snap = sample_snapshot();
    assert_eq!(snap.to_json(), snap.to_json());
}

#[test]
fn grammar_hash_is_stable_across_repeated_calls_on_the_same_snapshot() {
    let snap = sample_snapshot();
    assert_eq!(snap.grammar_hash(), snap.grammar_hash());
}

#[test]
fn grammar_hash_is_stable_across_two_loads_of_the_same_source() {
    let snap_a = sample_snapshot();
    let json = snap_a.to_json();
    let snap_b = Snapshot::from_json(&json).expect("round-trip must parse");
    assert_eq!(snap_a.grammar_hash(), snap_b.grammar_hash());
}

#[test]
fn grammar_hash_differs_for_different_grammars() {
    let snap_a = sample_snapshot();
    let mut snap_b = sample_snapshot();
    snap_b.lexicon.entries[0].citation_form = vec![ws("sen", "different")];
    assert_ne!(snap_a.grammar_hash(), snap_b.grammar_hash());
}

#[test]
fn grammar_hash_is_a_64_char_hex_digest() {
    let snap = sample_snapshot();
    let hash = snap.grammar_hash();
    assert_eq!(hash.len(), 64, "expected a SHA-256 hex digest: {hash:?}");
    assert!(
        hash.chars().all(|c| c.is_ascii_hexdigit()),
        "expected only hex digits: {hash:?}"
    );
}

#[test]
fn grammar_hash_ignores_conversion_provenance_differences() {
    let snap_a = sample_snapshot();
    let mut snap_b = sample_snapshot();
    snap_b
        .conversion_provenance
        .import_issues
        .push(ConversionIssue {
            code: ImportWarningCode::Unregistered("test.issue".to_string()),
            class: IssueClass::AmbiguousSource,
            source: None,
            fatal: false,
            message: "an import diagnostic".to_string(),
        });
    assert_ne!(snap_a.conversion_provenance, snap_b.conversion_provenance);
    assert_eq!(snap_a.grammar_hash(), snap_b.grammar_hash());
}

#[test]
fn grammar_hash_changes_when_project_changes() {
    let snap_a = sample_snapshot();
    let mut snap_b = sample_snapshot();
    snap_b.project.name = "Different Project".to_string();
    assert_ne!(snap_a.grammar_hash(), snap_b.grammar_hash());
}

#[test]
fn grammar_hash_changes_when_feature_systems_changes() {
    let snap_a = sample_snapshot();
    let mut snap_b = sample_snapshot();
    snap_b.feature_systems.morphosyntactic.closed_features[0].name = "Different".to_string();
    assert_ne!(snap_a.grammar_hash(), snap_b.grammar_hash());
}

#[test]
fn grammar_hash_changes_when_phonology_changes() {
    let snap_a = sample_snapshot();
    let mut snap_b = sample_snapshot();
    snap_b.phonology.phonemes[0].name = "different".to_string();
    assert_ne!(snap_a.grammar_hash(), snap_b.grammar_hash());
}

#[test]
fn grammar_hash_changes_when_morphology_changes() {
    let snap_a = sample_snapshot();
    let mut snap_b = sample_snapshot();
    snap_b.morphology.parts_of_speech[0].name = "Different".to_string();
    assert_ne!(snap_a.grammar_hash(), snap_b.grammar_hash());
}

#[test]
fn grammar_hash_changes_when_lexicon_changes() {
    let snap_a = sample_snapshot();
    let mut snap_b = sample_snapshot();
    snap_b.lexicon.entries[0].citation_form = vec![ws("sen", "different")];
    assert_ne!(snap_a.grammar_hash(), snap_b.grammar_hash());
}

#[test]
fn grammar_hash_changes_when_version_changes() {
    let snap_a = sample_snapshot();
    let mut snap_b = sample_snapshot();
    snap_b.version = 2;
    assert_ne!(snap_a.grammar_hash(), snap_b.grammar_hash());
}

#[test]
fn grammar_hash_changes_when_format_changes() {
    let snap_a = sample_snapshot();
    let mut snap_b = sample_snapshot();
    snap_b.format = "different-format".to_string();
    assert_ne!(snap_a.grammar_hash(), snap_b.grammar_hash());
}

#[test]
fn missing_conversion_provenance_deserializes_as_schema_version_zero_unknown() {
    let snap = sample_snapshot();
    let mut json_value: serde_json::Value = serde_json::from_str(&snap.to_json()).unwrap();
    json_value
        .as_object_mut()
        .unwrap()
        .remove("conversionProvenance");
    let json_without_provenance = serde_json::to_string(&json_value).unwrap();
    let reparsed = Snapshot::from_json(&json_without_provenance)
        .expect("must parse without conversionProvenance");
    assert_eq!(reparsed.conversion_provenance.schema_version, 0);
    assert_eq!(
        reparsed.conversion_provenance.source_inventory_status,
        SourceInventoryStatus::Unknown
    );
}

/// Pins the one-time digest migration: an old-style document hashes as the SHA-256 of its own bytes.
#[test]
fn old_style_json_without_provenance_hashes_as_the_semantic_projection() {
    use sha2::{Digest, Sha256};
    let snap = sample_snapshot();
    let projection_json = serde_json::to_string_pretty(&GrammarHashInput::from(&snap))
        .expect("GrammarHashInput serialization is infallible");
    assert!(!projection_json.contains("conversionProvenance"));
    let reparsed = Snapshot::from_json(&projection_json)
        .expect("old-style JSON (no provenance) must still parse");
    let mut hasher = Sha256::new();
    hasher.update(projection_json.as_bytes());
    let expected_hash: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(reparsed.grammar_hash(), expected_hash);
}

#[test]
fn snapshot_loaded_from_provenance_lacking_json_hashes_stably_after_reserialization() {
    let snap = sample_snapshot();
    let projection_json = serde_json::to_string_pretty(&GrammarHashInput::from(&snap))
        .expect("GrammarHashInput serialization is infallible");
    let reparsed = Snapshot::from_json(&projection_json)
        .expect("old-style JSON (no provenance) must still parse");
    let hash_before = reparsed.grammar_hash();
    let reparsed_again = Snapshot::from_json(&reparsed.to_json())
        .expect("must re-parse this crate's own to_json output");
    assert_eq!(hash_before, reparsed_again.grammar_hash());
}

#[test]
fn to_json_emits_envelope_first() {
    let snap = sample_snapshot();
    let json = snap.to_json();
    let format_pos = json.find("\"format\"").unwrap();
    let version_pos = json.find("\"version\"").unwrap();
    let project_pos = json.find("\"project\"").unwrap();
    assert!(format_pos < version_pos);
    assert!(version_pos < project_pos);
}

#[test]
fn sample_snapshot_validates_clean() {
    let snap = sample_snapshot();
    let warnings = snap.validate();
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
}

#[test]
fn from_json_rejects_wrong_format_tag() {
    let mut snap = sample_snapshot();
    snap.format = "some-other-format".to_string();
    let json = snap.to_json();
    let err = Snapshot::from_json(&json).unwrap_err();
    assert!(matches!(err, SnapshotError::UnknownFormat { found } if found == "some-other-format"));
}

#[test]
fn from_json_rejects_unsupported_version() {
    let mut snap = sample_snapshot();
    snap.version = 999;
    let json = snap.to_json();
    let err = Snapshot::from_json(&json).unwrap_err();
    assert!(matches!(
        err,
        SnapshotError::UnsupportedVersion { found: 999 }
    ));
}

#[test]
fn from_json_rejects_malformed_json() {
    let err = Snapshot::from_json("{ not json").unwrap_err();
    assert!(matches!(err, SnapshotError::Json(_)));
}

#[test]
fn validate_reports_dangling_sense_msa_reference_as_warning_not_error() {
    let mut snap = sample_snapshot();
    // Points the sense at a nonexistent MSA guid (a stale FieldWorks reference): must warn, and `from_json` must still succeed on the round-tripped JSON.
    snap.lexicon.entries[0].senses[0].msa =
        Some("00000000-0000-0000-0000-000000000000".to_string());
    let json = snap.to_json();
    let reparsed = Snapshot::from_json(&json).expect("dangling refs must not block parsing");
    let warnings = reparsed.validate();
    assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
    assert!(warnings[0].contains("00000000-0000-0000-0000-000000000000"));
}

#[test]
fn validate_reports_dangling_environment_reference() {
    let mut snap = sample_snapshot();
    snap.lexicon.entries[0].allomorphs[0]
        .environments
        .push("dangling-env-guid".to_string());
    let warnings = snap.validate();
    assert!(warnings.iter().any(|w| w.contains("dangling-env-guid")));
}

#[test]
fn validate_reports_dangling_exception_feature_reference() {
    let mut snap = sample_snapshot();
    if let Msa::Stem {
        exception_features, ..
    } = &mut snap.lexicon.entries[0].msas[0]
    {
        exception_features.push("no-such-exception-feature".to_string());
    } else {
        panic!("expected the sample entry's MSA to be Msa::Stem");
    }
    let warnings = snap.validate();
    assert!(warnings
        .iter()
        .any(|w| w.contains("no-such-exception-feature")));
}

#[test]
fn validate_accepts_exception_feature_present_in_registry() {
    let mut snap = sample_snapshot();
    snap.morphology
        .exception_features
        .push(crate::morphology::ExceptionFeature {
            guid: "latinate-guid".to_string(),
            name: "Latinate".to_string(),
            abbreviation: "lat".to_string(),
        });
    if let Msa::Stem {
        exception_features, ..
    } = &mut snap.lexicon.entries[0].msas[0]
    {
        exception_features.push("latinate-guid".to_string());
    } else {
        panic!("expected the sample entry's MSA to be Msa::Stem");
    }
    let warnings = snap.validate();
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
}

#[test]
fn validate_reports_dangling_adhoc_prohibition_reference() {
    let mut snap = sample_snapshot();
    snap.morphology
        .adhoc_prohibitions
        .push(crate::morphology::AdhocProhibition::Morpheme {
            guid: "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".to_string(),
            disabled: false,
            primary: "does-not-exist".to_string(),
            others: vec!["also-missing".to_string()],
            adjacency: crate::morphology::Adjacency::Anywhere,
        });
    let warnings = snap.validate();
    assert!(warnings.iter().any(|w| w.contains("does-not-exist")));
    assert!(warnings.iter().any(|w| w.contains("also-missing")));
}

// --- warning codes -----------------------------------------------------------------------

#[test]
fn validate_dangling_reference_carries_the_expected_code() {
    let mut snap = sample_snapshot();
    snap.lexicon.entries[0].allomorphs[0]
        .environments
        .push("dangling-env-guid".to_string());
    let warnings = snap.validate();
    let hit = warnings
        .iter()
        .find(|w| w.contains("dangling-env-guid"))
        .expect("the dangling-environment warning must be present");
    assert_eq!(hit.code, "snapshot.dangling-reference");
}

/// Two structurally different situations -- a dangling cross-reference vs. a sense's MSA resolving fine globally but not within its own entry -- must get different codes.
#[test]
fn validate_out_of_scope_reference_gets_a_different_code_than_dangling_reference() {
    let mut snap = sample_snapshot();
    snap.lexicon.entries[0].senses[0].msa =
        Some("00000000-0000-0000-0000-000000000000".to_string());
    snap.lexicon.entries[0].allomorphs[0]
        .environments
        .push("dangling-env-guid".to_string());
    let warnings = snap.validate();
    let scope_warning = warnings
        .iter()
        .find(|w| w.contains("does not resolve within this entry"))
        .expect("the out-of-scope sense/msa warning must be present");
    let dangling_warning = warnings
        .iter()
        .find(|w| w.contains("dangling-env-guid"))
        .expect("the dangling-environment warning must be present");
    assert_ne!(scope_warning.code, dangling_warning.code);
    assert_eq!(scope_warning.code, "snapshot.reference-out-of-scope");
    assert_eq!(dangling_warning.code, "snapshot.dangling-reference");
}

/// The same check, fired with two different dangling guids (different interpolated `message` text), must still produce the identical `code` both times.
#[test]
fn validate_dangling_reference_code_is_stable_regardless_of_message_text() {
    let mut snap_a = sample_snapshot();
    snap_a.lexicon.entries[0].allomorphs[0]
        .environments
        .push("guid-aaa".to_string());
    let mut snap_b = sample_snapshot();
    snap_b.lexicon.entries[0].allomorphs[0]
        .environments
        .push("guid-bbb".to_string());

    let warnings_a = snap_a.validate();
    let warnings_b = snap_b.validate();
    assert_eq!(warnings_a.len(), 1);
    assert_eq!(warnings_b.len(), 1);
    assert_ne!(warnings_a[0].message, warnings_b[0].message);
    assert_eq!(warnings_a[0].code, warnings_b[0].code);
}

/// Pins the same warning prose that `validate_reports_dangling_sense_msa_reference_as_warning_not_error` already asserts as a substring, now exactly.
#[test]
fn validate_warning_prose_is_unchanged_at_representative_sites() {
    let mut snap = sample_snapshot();
    let entry_guid = snap.lexicon.entries[0].guid.clone();
    let sense_guid = snap.lexicon.entries[0].senses[0].guid.clone();
    let dangling_msa = "00000000-0000-0000-0000-000000000000".to_string();
    snap.lexicon.entries[0].senses[0].msa = Some(dangling_msa.clone());
    let warnings = snap.validate();
    assert_eq!(warnings.len(), 1);
    let expected = format!(
        "lex entry {entry_guid:?} sense {sense_guid:?}: msa {dangling_msa:?} does not \
             resolve within this entry"
    );
    assert_eq!(warnings[0].message, expected);
}
