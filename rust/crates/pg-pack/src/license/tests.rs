use super::*;

#[test]
fn commercial_declaration_round_trips() {
    let decl = LicenseDeclaration {
        class: LicenseClass::Commercial,
        identifier: Some("synthetic-license-0001".to_string()),
        text_or_reference: Some("https://example.invalid/synthetic-license".to_string()),
        publisher: Some("Synthetic Publisher".to_string()),
    };
    let json = serde_json::to_string(&decl).unwrap();
    let parsed: LicenseDeclaration = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, decl);
}

#[test]
fn unknown_namespaced_declaration_round_trips() {
    let decl = LicenseDeclaration {
        class: LicenseClass::Namespaced {
            namespace: "synthetic.future-license-family".to_string(),
        },
        identifier: None,
        text_or_reference: None,
        publisher: None,
    };
    let json = serde_json::to_string(&decl).unwrap();
    let parsed: LicenseDeclaration = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, decl);
    assert!(json.contains("synthetic.future-license-family"));
}

#[test]
fn open_declaration_omits_absent_optional_fields() {
    let decl = LicenseDeclaration {
        class: LicenseClass::Open,
        identifier: None,
        text_or_reference: None,
        publisher: None,
    };
    let json = serde_json::to_string(&decl).unwrap();
    assert!(!json.contains("identifier"));
    assert!(!json.contains("publisher"));
}
