use super::*;
#[test]
fn named_analysis_policies_pin_host_contracts() {
    assert_eq!(
        AnalysisPolicy::browser_default(),
        AnalysisPolicy { step_cap: 100_000 }
    );
    assert_eq!(
        AnalysisPolicy::native_abi_v1(),
        AnalysisPolicy { step_cap: 500_000 }
    );
}
#[test]
fn inactive_override_never_suppresses_official_identity() {
    let date = crate::LexicalDate::parse("2026-07-22 00:00:00.000").unwrap();
    let entry = crate::SuppliedEntry {
        id: crate::EntryId::from_bytes([1; 16]),
        stem: "x".into(),
        gloss: String::new(),
        signatures: vec![],
        date_created: date.clone(),
        date_modified: date,
        authority: EntryAuthority::SuppliedOverride {
            official_entry_id: "official".into(),
            note: None,
        },
        state: ValidationState::Inactive {
            diagnostics: vec!["missing signature".into()],
        },
    };
    assert!(active_override_ids(&[entry]).is_empty());
}
