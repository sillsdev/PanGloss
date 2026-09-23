//! Optional license declaration: the manifest may declare `open`, `commercial`, or a namespaced
//! license class plus license identifier/text/reference and publisher metadata. Licensing is
//! declaration and provenance only — it does not license or restrict FieldWorks analysis, and
//! unknown namespaced declarations stay round-trippable. Nothing in this crate ever reads
//! `LicenseDeclaration` to gate a read or an analysis — see `crate::format::read_pack`'s doc for
//! where that hard rule is enforced.

use serde::{Deserialize, Serialize};

/// The declared license classification. `Namespaced` keeps an arbitrary forward-compatible
/// namespace string round-trippable without this schema step needing to enumerate
/// every possible license family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "class", rename_all = "snake_case")]
pub enum LicenseClass {
    Open,
    Commercial,
    Namespaced { namespace: String },
}

/// A pack's optional license declaration. Every field beyond `class` is optional — license
/// identifier/text/reference and publisher metadata MAY accompany a
/// declared class, but none of it MUST.
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
mod tests;
