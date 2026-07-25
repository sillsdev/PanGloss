//! Optional license declaration (R2A: "optional license declaration"; `make-wasm-analysis-only`
//! design.md: "the manifest may declare `open`, `commercial`, or a namespaced license class plus
//! license identifier/text/reference and publisher metadata" — "Licensing is declaration and
//! provenance only... it does not license or restrict FieldWorks analysis" and tasks.md 1.4: "keep
//! unknown namespaced declarations round-trippable"). Nothing in this crate ever reads
//! [`LicenseDeclaration`] to gate a read or an analysis — see `crate::format::read_pack`'s doc for
//! where that hard rule is enforced.

use serde::{Deserialize, Serialize};

/// The declared license classification. `Namespaced` keeps an arbitrary forward-compatible
/// namespace string round-trippable (tasks.md 1.4) without this schema step needing to enumerate
/// every possible license family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "class", rename_all = "snake_case")]
pub enum LicenseClass {
    Open,
    Commercial,
    Namespaced { namespace: String },
}

/// A pack's optional license declaration. Every field beyond `class` is optional — design.md
/// names "license identifier/text/reference and publisher metadata" as what MAY accompany a
/// declared class, not what MUST.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LicenseDeclaration {
    pub class: LicenseClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_or_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
}

#[cfg(test)]
mod tests {
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
}
