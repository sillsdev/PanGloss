use pg_lexicon::*;

struct IDs(u8);
impl IdSource for IDs {
    fn next_128(&mut self) -> Result<[u8; 16], StructuredError> {
        let b = self.0;
        self.0 += 1;
        Ok([b; 16])
    }
}
struct FailIDs;
impl IdSource for FailIDs {
    fn next_128(&mut self) -> Result<[u8; 16], StructuredError> {
        Err(StructuredError {
            code: "entropy_failure".into(),
            message: "no entropy".into(),
            details: serde_json::json!({"source":"test"}),
        })
    }
}
struct Times(Vec<&'static str>);
impl Clock for Times {
    fn now(&mut self) -> LexicalDate {
        LexicalDate::parse(self.0.remove(0)).unwrap()
    }
}

fn store() -> SuppliedLexiconStore<IDs, Times> {
    let c = catalog();
    SuppliedLexiconStore::new(
        IDs(0),
        Times(vec![
            "2026-07-22 12:00:00.123",
            "2026-07-22 12:01:00.456",
            "2026-07-22 12:02:00.789",
            "2026-07-22 12:03:00.000",
        ]),
        &c,
        |_| Ok(()),
    )
}
fn default_signature() -> SignatureId {
    catalog().signatures()[0].id.clone()
}
fn add(
    st: &mut SuppliedLexiconStore<IDs, Times>,
    stem: &str,
    gloss: &str,
) -> Result<MutationResult<SuppliedEntry>, StructuredError> {
    st.add(AddRequest {
        stem: stem.into(),
        gloss: gloss.into(),
        signatures: vec![default_signature()],
        expected_revision: None,
    })
}
fn grammar() -> pg_grammar::model::Grammar {
    let x = r#"<HermitCrabInput><Language><Name>T</Name><PartsOfSpeech><PartOfSpeech id="n"><Name>n</Name></PartOfSpeech><PartOfSpeech id="v"><Name>v</Name></PartOfSpeech></PartsOfSpeech><CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions><SegmentDefinition id="a"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable><Strata><Stratum characterDefinitionTable="t"><Name>S</Name><LexicalEntries><LexicalEntry id="en" partOfSpeech="n"><Allomorphs><Allomorph id="an"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry><LexicalEntry id="ev" partOfSpeech="v"><Allomorphs><Allomorph id="av"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#;
    pg_grammar::load(x).unwrap()
}
fn catalog() -> ClassCatalog {
    ClassCatalog::from_grammar(&grammar()).unwrap()
}

#[test]
fn id_guid_and_date_vectors() {
    let id = EntryId::from_bytes([0; 16]);
    assert_eq!(id.as_str(), "pgl_AAAAAAAAAAAAAAAAAAAAAA");
    assert_eq!(id.to_dotnet_guid_bytes().unwrap(), [0; 16]);
    let bytes = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
    assert_eq!(
        EntryId::from_bytes(bytes).to_dotnet_guid_bytes().unwrap(),
        [3, 2, 1, 0, 5, 4, 7, 6, 8, 9, 10, 11, 12, 13, 14, 15]
    );
    assert_eq!(
        LexicalDate::parse("2026-07-22 12:00:00.123")
            .unwrap()
            .as_str(),
        "2026-07-22 12:00:00.123"
    );
    let id = EntryId::from_bytes(bytes);
    assert_eq!(
        id.to_dotnet_guid_string().unwrap(),
        "00010203-0405-0607-0809-0a0b0c0d0e0f"
    );
    assert_eq!(
        EntryId::from_dotnet_guid_string("00010203-0405-0607-0809-0a0b0c0d0e0f").unwrap(),
        id
    );
    assert!(serde_json::from_str::<EntryId>(r#""pgl_bad""#).is_err());
    assert!(EntryId::parse("pgl_AAAAAAAAAAAAAAAAAAAAAB").is_err());
    assert!(EntryId::from_dotnet_guid_string("000102030405-0607-0809-0a0b0c0d0e0f").is_err());
    assert!(serde_json::from_str::<LexicalDate>(r#""2026-02-29 12:00:00.123""#).is_err());
    assert!(LexicalDate::parse("2024-02-29 23:59:59.999").is_ok());
    assert!(LexicalDate::parse("2024-01-01 24:00:00.000").is_err());
}

#[test]
fn production_sources_generate_valid_values() {
    let mut ids = OsIdSource;
    assert_eq!(ids.next_128().unwrap().len(), 16);
    let mut clock = UtcClock;
    assert_eq!(clock.now().as_str().len(), 23);
}

#[test]
fn id_source_failure_is_atomic() {
    let c = catalog();
    let mut st =
        SuppliedLexiconStore::new(FailIDs, Times(vec!["2026-07-22 12:00:00.123"]), &c, |_| {
            Ok(())
        });
    let revision = st.revision().clone();
    let sig = c.signatures()[0].id.clone();
    assert_eq!(
        st.add(AddRequest {
            stem: "a".into(),
            gloss: "".into(),
            signatures: vec![sig],
            expected_revision: None
        })
        .unwrap_err()
        .code,
        "entropy_failure"
    );
    assert_eq!(st.revision(), &revision);
    assert!(st.list().is_empty());
}

#[test]
fn catalog_and_shape_validation_precede_id_allocation_and_pos_search_is_exact() {
    let c = catalog();
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let validator_calls = calls.clone();
    let validator_grammar = std::sync::Arc::new(grammar());
    let mut st = SuppliedLexiconStore::new(
        IDs(0),
        Times(vec!["2026-07-22 12:00:00.123"]),
        &c,
        move |stem| {
            validator_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            validate_shape(&validator_grammar, stem)
        },
    );
    let unknown = SignatureId::parse(&format!("sig_{}", "f".repeat(64))).unwrap();
    let unknown_error = st
        .add(AddRequest {
            stem: "a".into(),
            gloss: "".into(),
            signatures: vec![unknown.clone()],
            expected_revision: None,
        })
        .unwrap_err();
    assert_eq!(unknown_error.code, "unknown_signature");
    assert_eq!(unknown_error.details["signatureIds"][0], unknown.as_str());
    let n = c
        .signatures()
        .iter()
        .find(|s| s.pos.as_ref().unwrap().id == "n")
        .unwrap()
        .id
        .clone();
    assert_eq!(
        st.add(AddRequest {
            stem: "z".into(),
            gloss: "".into(),
            signatures: vec![n.clone()],
            expected_revision: None
        })
        .unwrap_err()
        .code,
        "invalid_shape"
    );
    let e = st
        .add(AddRequest {
            stem: "a".into(),
            gloss: "".into(),
            signatures: vec![n],
            expected_revision: None,
        })
        .unwrap()
        .value;
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 3);
    assert_eq!(e.id, EntryId::from_bytes([0; 16]));
    assert_eq!(
        st.search(&SearchRequest {
            query: "".into(),
            signature: None,
            state: Some(ValidationStateKind::Active),
            pos: Some("n".into())
        })
        .len(),
        1
    );
    assert!(st
        .search(&SearchRequest {
            query: "".into(),
            signature: None,
            state: None,
            pos: Some("v".into())
        })
        .is_empty());
}

#[test]
fn validation_ids_revisions_noops_and_conflicts_are_atomic() {
    let mut st = store();
    let r0 = st.revision().clone();
    assert_eq!(
        add(&mut st, "a", "x").unwrap_err().code,
        "gloss_language_required"
    );
    assert_eq!(st.revision(), &r0);
    st.set_gloss_language(SetGlossLanguageRequest {
        gloss_language: Some("en".into()),
        expected_revision: None,
    })
    .unwrap();
    let a = add(&mut st, "a", "x").unwrap();
    assert_eq!(a.value.id, EntryId::from_bytes([0; 16]));
    assert_eq!(a.value.date_created, a.value.date_modified);
    let conflict = st
        .remove(RemoveRequest {
            id: a.value.id.clone(),
            expected_revision: Some(r0),
        })
        .unwrap_err();
    assert_eq!(conflict.code, "revision_conflict");
    assert_eq!(
        conflict.details["current"],
        serde_json::to_value(st.revision()).unwrap()
    );
    assert!(st.get(&a.value.id).is_some());
    let before = st.revision().clone();
    let no = st
        .update(UpdateRequest {
            id: a.value.id.clone(),
            stem: "a".into(),
            gloss: "x".into(),
            signatures: a.value.signatures.clone(),
            expected_revision: None,
        })
        .unwrap();
    assert!(!no.changed);
    assert_eq!(st.revision(), &before);
}

#[test]
fn signature_reorder_is_noop_and_gloss_only_preserves_identity() {
    let mut st = store();
    st.set_gloss_language(SetGlossLanguageRequest {
        gloss_language: Some("en".into()),
        expected_revision: None,
    })
    .unwrap();
    let mut signatures: Vec<_> = catalog()
        .signatures()
        .iter()
        .map(|s| s.id.clone())
        .collect();
    signatures.sort();
    let s0 = signatures[0].clone();
    let s1 = signatures[1].clone();
    let added = st
        .add(AddRequest {
            stem: "a".into(),
            gloss: "x".into(),
            signatures: vec![s1.clone(), s0.clone(), s1.clone()],
            expected_revision: None,
        })
        .unwrap()
        .value;
    assert_eq!(added.signatures, vec![s0.clone(), s1.clone()]);
    let no = st
        .update(UpdateRequest {
            id: added.id.clone(),
            stem: "a".into(),
            gloss: "x".into(),
            signatures: added.signatures.iter().rev().cloned().collect(),
            expected_revision: None,
        })
        .unwrap();
    assert!(!no.changed);
    let changed = st
        .update(UpdateRequest {
            id: added.id.clone(),
            stem: "a".into(),
            gloss: "y".into(),
            signatures: added.signatures.clone(),
            expected_revision: None,
        })
        .unwrap();
    assert_eq!(changed.value.id, added.id);
    assert_eq!(changed.value.date_created, added.date_created);
    assert_ne!(changed.value.date_modified, added.date_modified);
}

#[test]
fn crud_homographs_search_authority_and_clear() {
    let mut st = store();
    st.set_gloss_language(SetGlossLanguageRequest {
        gloss_language: Some("en".into()),
        expected_revision: None,
    })
    .unwrap();
    let a = add(&mut st, "bank", "shore").unwrap().value;
    let b = add(&mut st, "bank", "money").unwrap().value;
    assert_ne!(a.id, b.id);
    assert_eq!(st.list().len(), 2);
    assert_eq!(
        st.search(&SearchRequest {
            query: "money".into(),
            signature: None,
            state: None,
            pos: None
        })
        .len(),
        1
    );
    let authority_change = st
        .set_authority(SetAuthorityRequest {
            id: a.id.clone(),
            authority: EntryAuthority::SuppliedOverride {
                official_entry_id: "official".into(),
                note: None,
            },
            expected_revision: None,
        })
        .unwrap();
    assert_eq!(authority_change.value.date_created, a.date_created);
    assert_ne!(authority_change.value.date_modified, a.date_modified);
    let rev = st.revision().clone();
    assert_eq!(
        st.set_gloss_language(SetGlossLanguageRequest {
            gloss_language: None,
            expected_revision: None
        })
        .unwrap_err()
        .code,
        "gloss_language_required"
    );
    assert_eq!(st.revision(), &rev);
    assert!(matches!(
        st.get(&a.id).unwrap().authority,
        EntryAuthority::SuppliedOverride { .. }
    ));
    assert!(
        st.remove(RemoveRequest {
            id: b.id,
            expected_revision: None
        })
        .unwrap()
        .changed
    );
    assert_eq!(st.clear(ExpectedRevision::default()).unwrap().value, 1);
    assert!(st.list().is_empty());
}
