//! User-added lexicon entries ("add to dictionary") — the serde JSON model persisted client-side
//! (browser IndexedDB; see the add-to-dictionary design doc,
//! `PanGloss-demo/docs/superpowers/specs/2026-07-14-add-to-dictionary-and-realize-inference-design.md`,
//! Sub-project 1). This engine crate defines the shape; PanGloss-demo only stores/loads the blob,
//! keyed by grammar id.
//!
//! Class assignment is stored as [`crate::classes::ClassCandidate::key`] (a POS-label +
//! sorted-MPR-names string), NOT as a raw `MprId` bitset — `MprId`s can be renumbered whenever a
//! FieldWorks project is reconverted to a fresh `*-hc.xml`; the MPR feature *names*
//! (`Grammar::mpr_names`) survive that regeneration, integer ids do not.

use serde::{Deserialize, Serialize};

/// One user-added dictionary entry: the vernacular shape + gloss + chosen inflection class the
/// "add to dictionary" flow collects, before [`crate::augment::augment_xml`] splices a fresh
/// `<LexicalEntry>` for it into a copy of the grammar's own XML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserLexEntry {
    /// Caller-assigned (the demo uses `crypto.randomUUID()`); becomes the `user:<id>` marker
    /// [`crate::augment::augment_xml`] writes into the cloned entry's `<Property name="ID">` value
    /// so the demo can recognize user-added morphemes in tooltips.
    pub id: String,
    /// The vernacular surface form as typed — segmented against the grammar's surface-stratum char
    /// table by [`crate::classes::validate_shape`] before this entry is ever created.
    pub shape: String,
    /// The user's English gloss.
    pub gloss: String,
    /// The [`crate::classes::ClassCandidate::key`] the user picked for this entry's inflection
    /// class.
    pub class_key: String,
    /// ISO timestamp, caller-supplied — this crate never reads the clock.
    pub added_at: String,
}

/// The whole user lexicon for one grammar: persisted as one JSON blob (browser IndexedDB, keyed by
/// grammar id) and re-applied via [`crate::augment::augment_xml`] on every grammar (re)load.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserLexicon {
    pub entries: Vec<UserLexEntry>,
}
