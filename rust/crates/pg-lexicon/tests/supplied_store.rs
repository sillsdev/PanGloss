use pg_lexicon::*;

struct IDs(u8);
impl IdSource for IDs {
    fn next_128(&mut self) -> Result<[u8; 16], StructuredError> {
        let b = self.0;
        self.0 += 1;
        Ok([b; 16])
    }
}
struct Times(Vec<&'static str>);
impl Clock for Times {
    fn now(&mut self) -> LexicalDate {
        LexicalDate::parse(self.0.remove(0)).unwrap()
    }
}

fn store() -> SuppliedLexiconStore<IDs, Times> {
    SuppliedLexiconStore::new(
        IDs(0),
        Times(vec![
            "2026-07-22 12:00:00.123",
            "2026-07-22 12:01:00.456",
            "2026-07-22 12:02:00.789",
            "2026-07-22 12:03:00.000",
        ]),
    )
}
fn add(
    st: &mut SuppliedLexiconStore<IDs, Times>,
    stem: &str,
    gloss: &str,
) -> Result<MutationResult<SuppliedEntry>, StructuredError> {
    st.add(AddRequest {
        stem: stem.into(),
        gloss: gloss.into(),
        signatures: vec![SignatureId::parse(&format!("sig_{}", "0".repeat(64))).unwrap()],
        expected_revision: None,
    })
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
        "03020100-0504-0706-0809-0a0b0c0d0e0f"
    );
    assert_eq!(
        EntryId::from_dotnet_guid_string("03020100-0504-0706-0809-0a0b0c0d0e0f").unwrap(),
        id
    );
    assert!(serde_json::from_str::<EntryId>(r#""pgl_bad""#).is_err());
    assert!(serde_json::from_str::<LexicalDate>(r#""2026-02-29 12:00:00.123""#).is_err());
    assert!(LexicalDate::parse("2024-02-29 23:59:59.999").is_ok());
    assert!(LexicalDate::parse("2024-01-01 24:00:00.000").is_err());
}

#[test]
fn production_sources_generate_valid_values() {
    let mut ids = OsIdSource;
    assert_ne!(ids.next_128().unwrap(), ids.next_128().unwrap());
    let mut clock = UtcClock;
    assert_eq!(clock.now().as_str().len(), 23);
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
    assert_eq!(a.value.date_created, a.value.date_modified);
    let conflict = st
        .remove(RemoveRequest {
            id: a.value.id.clone(),
            expected_revision: Some(r0),
        })
        .unwrap_err();
    assert_eq!(conflict.code, "revision_conflict");
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
            state: None
        })
        .len(),
        1
    );
    st.set_authority(SetAuthorityRequest {
        id: a.id.clone(),
        authority: EntryAuthority::SuppliedOverride {
            official_entry_id: "official".into(),
            note: None,
        },
        expected_revision: None,
    })
    .unwrap();
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
