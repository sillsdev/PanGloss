use pg_lexicon::*;
use std::sync::Arc;

const XML: &str = r#"<HermitCrabInput><Language><Name>RuntimeTest</Name><PartsOfSpeech><PartOfSpeech id="posN"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech><CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions><SegmentDefinition id="a"><Representations><Representation>a</Representation></Representations></SegmentDefinition><SegmentDefinition id="b"><Representations><Representation>b</Representation></Representations></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable><Strata><Stratum characterDefinitionTable="t"><Name>S</Name><LexicalEntries><LexicalEntry id="official-a" partOfSpeech="posN"><Allomorphs><Allomorph id="aa"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#;

fn runtime(source: &str) -> SuppliedLexiconRuntime {
    SuppliedLexiconRuntime::new(Arc::new(pg_grammar::load(source).unwrap()), source).unwrap()
}

fn entry(signature: SignatureId, stem: &str) -> SuppliedEntry {
    let date = LexicalDate::parse("2026-07-22 12:00:00.123").unwrap();
    SuppliedEntry {
        id: EntryId::from_bytes([7; 16]),
        stem: stem.into(),
        gloss: String::new(),
        signatures: vec![signature],
        date_created: date.clone(),
        date_modified: date,
        authority: EntryAuthority::Supplied,
        state: ValidationState::Active,
    }
}

fn document(rt: &SuppliedLexiconRuntime, stem: &str) -> LexiconDocument {
    let signature = rt.catalog().signatures()[0].clone();
    LexiconDocument {
        schema_version: LEXICON_SCHEMA_VERSION,
        grammar_name: "RuntimeTest".into(),
        source_grammar_fingerprint: rt.source_fingerprint().into(),
        gloss_language: None,
        signatures: vec![signature.clone()],
        entries: vec![entry(signature.id, stem)],
    }
}

#[test]
fn versioned_document_round_trips_and_preserves_schema() {
    let rt = runtime(XML);
    let report = rt.import_document(document(&rt, "b")).unwrap();
    assert!(report.exact_match);
    let exported = rt.export_document();
    assert_eq!(exported.schema_version, 1);
    assert_eq!(exported.grammar_name, "RuntimeTest");
    assert_eq!(exported.entries.len(), 1);
    assert_eq!(
        serde_json::from_str::<LexiconDocument>(&serde_json::to_string(&exported).unwrap())
            .unwrap(),
        exported
    );
    assert!(!rt.parse_word("b").structured.is_empty());
}

#[test]
fn schema_and_grammar_name_are_hard_stops_and_import_is_atomic() {
    let rt = runtime(XML);
    rt.import_document(document(&rt, "b")).unwrap();
    let before = rt.snapshot();
    let mut unsupported = document(&rt, "a");
    unsupported.schema_version += 1;
    assert_eq!(
        rt.import_document(unsupported).unwrap_err().code,
        "unsupported_schema"
    );
    let mut wrong_name = document(&rt, "a");
    wrong_name.grammar_name = "Other".into();
    assert_eq!(
        rt.import_document(wrong_name).unwrap_err().code,
        "grammar_name_mismatch"
    );
    assert!(Arc::ptr_eq(&before, &rt.snapshot()));
}

#[test]
fn deterministic_fingerprint_and_changed_build_reconcile() {
    assert_eq!(
        grammar_source_fingerprint(XML),
        grammar_source_fingerprint(XML)
    );
    assert_ne!(
        grammar_source_fingerprint(XML),
        grammar_source_fingerprint(&(XML.to_owned() + " "))
    );
    let old = runtime(XML);
    let doc = document(&old, "b");
    let changed_source = XML.replace("<Name>Noun</Name>", "<Name>Renamed noun</Name>");
    let changed = runtime(&changed_source);
    let report = changed.import_document(doc).unwrap();
    assert!(!report.exact_match);
    assert!(report.compatible_migration);
    assert!(!changed.parse_word("b").structured.is_empty());
}

#[test]
fn duplicate_entries_conflicting_mappings_and_missing_mappings_reject_atomically() {
    let rt = runtime(XML);
    let good = document(&rt, "b");
    rt.import_document(good.clone()).unwrap();
    let before = rt.snapshot();

    let mut duplicate = good.clone();
    duplicate.entries.push(duplicate.entries[0].clone());
    assert_eq!(
        rt.import_document(duplicate).unwrap_err().code,
        "duplicate_entry_id"
    );

    let mut conflict = good.clone();
    let mut second = conflict.signatures[0].clone();
    second.pos.as_mut().unwrap().label = "conflict".into();
    conflict.signatures.push(second);
    assert_eq!(
        rt.import_document(conflict).unwrap_err().code,
        "conflicting_signature_mapping"
    );

    let mut missing = good;
    missing.signatures.clear();
    assert_eq!(
        rt.import_document(missing).unwrap_err().code,
        "missing_signature_mapping"
    );
    assert!(Arc::ptr_eq(&before, &rt.snapshot()));
}

#[test]
fn incompatible_entries_are_retained_inactive_and_valid_entries_publish_together() {
    let rt = runtime(XML);
    let mut doc = document(&rt, "b");
    let mut invalid = doc.entries[0].clone();
    invalid.id = EntryId::from_bytes([8; 16]);
    invalid.stem = "z".into();
    doc.entries.push(invalid);
    let report = rt.import_document(doc).unwrap();
    assert_eq!(report.inactive_entries.len(), 1);
    let snapshot = rt.snapshot();
    assert_eq!(snapshot.entries().len(), 2);
    assert!(snapshot
        .entries()
        .iter()
        .any(|entry| matches!(entry.state, ValidationState::Inactive { .. })));
    assert!(!rt.parse_word("b").structured.is_empty());
}

#[test]
fn missing_current_signature_is_inactive_and_retains_last_known_labels() {
    let old = runtime(XML);
    let old_doc = document(&old, "b");
    let old_label = old_doc.signatures[0].pos.as_ref().unwrap().label.clone();
    let incompatible_source = XML
        .replace("id=\"posN\"", "id=\"posOther\"")
        .replace("partOfSpeech=\"posN\"", "partOfSpeech=\"posOther\"")
        .replace("<Name>Noun</Name>", "<Name>Other class</Name>");
    let changed = runtime(&incompatible_source);
    let report = changed.import_document(old_doc).unwrap();
    assert_eq!(report.inactive_entries.len(), 1);
    let exported = changed.export_document();
    assert_eq!(
        exported.signatures[0].pos.as_ref().unwrap().label,
        old_label
    );
    assert!(matches!(
        exported.entries[0].state,
        ValidationState::Inactive { .. }
    ));
}

#[test]
fn compatible_signatures_refresh_readable_labels() {
    let old = runtime(XML);
    let doc = document(&old, "b");
    let renamed_source = XML.replace("<Name>Noun</Name>", "<Name>Current noun label</Name>");
    let changed = runtime(&renamed_source);
    changed.import_document(doc).unwrap();
    assert_eq!(
        changed.export_document().signatures[0]
            .pos
            .as_ref()
            .unwrap()
            .label,
        "Current noun label"
    );
}

#[test]
fn promotion_supersedes_by_shared_128_bits_and_complete_override_restores_supplied_authority() {
    let id = EntryId::from_bytes([7; 16]);
    let official_id = id.to_dotnet_guid_string().unwrap();
    let promoted_source = XML.replace("official-a", &official_id);
    let rt = runtime(&promoted_source);
    let mut doc = document(&rt, "b");
    let report = rt.import_document(doc.clone()).unwrap();
    assert_eq!(report.superseded_entries, vec![id.clone()]);
    assert!(matches!(
        rt.export_document().entries[0].state,
        ValidationState::Superseded { .. }
    ));
    assert!(rt.parse_word("b").structured.is_empty());

    doc.entries[0].authority = EntryAuthority::SuppliedOverride {
        official_entry_id: official_id,
        note: Some("keep supplied record".into()),
    };
    rt.import_document(doc).unwrap();
    assert!(matches!(
        rt.export_document().entries[0].state,
        ValidationState::Active
    ));
    assert!(rt.parse_word("a").structured.is_empty());
    assert!(!rt.parse_word("b").structured.is_empty());
}

#[test]
fn incomplete_override_and_duplicate_signature_references_reject_without_publish() {
    let rt = runtime(XML);
    let good = document(&rt, "b");
    rt.import_document(good.clone()).unwrap();
    let before = rt.snapshot();

    let mut duplicate_signature = good.clone();
    let repeated = duplicate_signature.entries[0].signatures[0].clone();
    duplicate_signature.entries[0].signatures.push(repeated);
    assert_eq!(
        rt.import_document(duplicate_signature).unwrap_err().code,
        "duplicate_signature_reference"
    );

    let mut incomplete = good;
    incomplete.entries[0].authority = EntryAuthority::SuppliedOverride {
        official_entry_id: "not-the-promoted-entry".into(),
        note: None,
    };
    assert_eq!(
        rt.import_document(incomplete).unwrap_err().code,
        "invalid_override"
    );
    assert!(Arc::ptr_eq(&before, &rt.snapshot()));
}

#[test]
fn concurrent_readers_observe_complete_old_or_new_snapshots() {
    let grammar = Arc::new(pg_grammar::load(XML).unwrap());
    let rt = Arc::new(SuppliedLexiconRuntime::new(grammar.clone(), XML).unwrap());
    let populated = document(&rt, "b");
    let mut empty = populated.clone();
    empty.entries.clear();
    empty.signatures.clear();

    let writer_rt = rt.clone();
    let writer = std::thread::spawn(move || {
        for i in 0..100 {
            writer_rt
                .import_document(if i % 2 == 0 {
                    populated.clone()
                } else {
                    empty.clone()
                })
                .unwrap();
        }
    });
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let reader_rt = rt.clone();
            let grammar = grammar.clone();
            std::thread::spawn(move || {
                for _ in 0..100 {
                    let snapshot = reader_rt.snapshot();
                    let parsed =
                        pg_parse::Morpher::new_with_overlay(&grammar, 100_000, snapshot.overlay())
                            .parse_word("b");
                    assert_eq!(parsed.structured.is_empty(), snapshot.entries().is_empty());
                }
            })
        })
        .collect();
    writer.join().unwrap();
    for reader in readers {
        reader.join().unwrap();
    }
}

struct IDs;
impl IdSource for IDs {
    fn next_128(&mut self) -> Result<[u8; 16], StructuredError> {
        Ok([9; 16])
    }
}
struct Times;
impl Clock for Times {
    fn now(&mut self) -> LexicalDate {
        LexicalDate::parse("2026-07-22 13:00:00.000").unwrap()
    }
}

#[test]
fn ordinary_mutations_validate_before_atomic_snapshot_publication() {
    let grammar = Arc::new(pg_grammar::load(XML).unwrap());
    let rt = SuppliedLexiconRuntime::with_sources(grammar, XML, IDs, Times).unwrap();
    let before = rt.snapshot();
    let signature = rt.catalog().signatures()[0].id.clone();
    assert_eq!(
        rt.add(AddRequest {
            stem: "z".into(),
            gloss: String::new(),
            signatures: vec![signature.clone()],
            expected_revision: None,
        })
        .unwrap_err()
        .code,
        "invalid_shape"
    );
    assert!(Arc::ptr_eq(&before, &rt.snapshot()));

    let added = rt
        .add(AddRequest {
            stem: "b".into(),
            gloss: String::new(),
            signatures: vec![signature],
            expected_revision: None,
        })
        .unwrap();
    assert!(added.changed);
    assert!(!rt.parse_word("b").structured.is_empty());
    assert!(
        rt.remove(RemoveRequest {
            id: added.value.id,
            expected_revision: Some(added.revision),
        })
        .unwrap()
        .changed
    );
    assert!(rt.parse_word("b").structured.is_empty());
}
