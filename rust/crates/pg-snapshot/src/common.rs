//! Shared primitive types used across every snapshot section.

use serde::{Deserialize, Serialize};

/// A FieldWorks GUID, always rendered lowercase-hyphenated (the canonical `Guid.ToString()`
/// format, e.g. `"3c6ce6a1-3d3b-4c3c-9e3e-9b5a6d9d0f1a"`). Every cross-reference within a
/// `crate::Snapshot` is one of these — never an `Hvo` (FieldWorks' in-session integer id,
/// which is *not* stable across loads and therefore unsuitable as a durable interchange key;
/// see `docs/fwdata-import-plan.md` §3).
///
/// A plain type alias (not a newtype) so serde renders it as a bare JSON string with no extra
/// wrapping, and so callers can compare/hash it against `String` literals without ceremony.
pub type Guid = String;

/// A single writing-system-tagged string value — the common shape for every LCM `MultiUnicode`/
/// `MultiString` field this format carries (phoneme/boundary graphemes, allomorph forms). `ws`
/// is the writing system tag (ICU locale id, e.g. `"en"`, `"sen-Latn"`), not a GUID: FieldWorks
/// writing systems are identified by tag, not by object GUID, in the `.fwdata` XML itself
/// (`AUni ws="..."` / `AStr ws="..."` attributes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsForm {
    pub ws: String,
    pub form: String,
}
