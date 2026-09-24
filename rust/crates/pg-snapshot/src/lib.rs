//! `pg-snapshot`: PanGloss's owned, versioned JSON project-snapshot format.
//!
//! This is the interchange contract between `pg-fwdata` (which reads FieldWorks `.fwdata`
//! project files) and `pg_grammar::compile` (which turns a snapshot into a runnable `Grammar`) —
//! see `docs/fwdata-import-plan.md` §2-§3 for the overall architecture and §6 for how this crate
//! (T1) fits into the task breakdown. It is deliberately **not** a mirror of FieldWorks/LCM
//! class names: fields are named for what they mean in this pipeline, and every cross-reference
//! is a FieldWorks GUID (lowercase-hyphenated string) rather than the session-scoped `Hvo`
//! integers the legacy HC-XML export used (those drift across FieldWorks sessions and are
//! therefore unusable as a durable interchange key).
//!
//! Every section's fields are documented at the LCM-property level in `docs/snapshot-format.md`
//! (authored alongside this crate) and, closer to the metal, on each field below with a
//! `HCLoader.cs` line reference for how FieldWorks' own HermitCrab exporter reads it.
//!
//! # Envelope and versioning
//!
//! Every snapshot JSON document is wrapped in a versioned envelope:
//! ```json
//! { "format": "pangloss-project", "version": 1, "project": { ... }, ... }
//! ```
//! A document whose `format`/`version` don't match what this build understands is refused with a
//! specific `SnapshotError` variant — never a generic parse failure, pinned by
//! `from_json_rejects_wrong_format_tag`/`from_json_rejects_unsupported_version`.
//!
//! # Determinism
//!
//! `Snapshot::to_json` always pretty-prints with two-space indentation and struct fields in
//! their Rust declaration order (serde's default, unmodified). Every collection in this format
//! is a plain `Vec` — **construction order is preserved on the wire**; nothing is sorted or
//! reordered by this crate. Where the source data has a meaningful order (affix template slots,
//! rule application order, allomorph disjunctive-ordering, ...) that order is the snapshot's
//! order; where the source data is an unordered LCM collection, the snapshot's order is simply
//! whatever order the producer (`pg-fwdata`) encountered it in, which is itself deterministic
//! across imports of the same `.fwdata` file (`docs/fwdata-import-plan.md` §5.3). Two
//! `Snapshot`s built with the same field values in the same order always serialize to
//! byte-identical JSON.
//!
//! # Validation
//!
//! `Snapshot::validate` is a light, warning-only check for dangling GUID references (see the
//! `validate` module doc for exactly what is and isn't checked), returning `Vec<Warning>` rather
//! than a `Result` — real FieldWorks projects contain stale references, and this pipeline must
//! tolerate them (`docs/fwdata-import-plan.md` §1).
#![forbid(unsafe_code)]

pub mod common;
pub mod conversion;
pub mod feature;
pub mod fieldworks_paths;
pub mod lexicon;
pub mod morphology;
pub mod phonology;
pub mod project;
pub mod validate;
mod warning;
pub mod warning_metadata;

pub use common::{Guid, WsForm};
pub use conversion::{
    ConversionInventory, ConversionIssue, ConversionProvenance, InventoryDelta, InventoryIdentity,
    InventoryKey, InventoryKind, IssueClass, ProvenanceError, RawSourceCensus, SelectionRecorder,
    SourceInventoryStatus, SourceRef, CONVERSION_PROVENANCE_SCHEMA_VERSION,
};
pub use feature::{
    ClosedFeature, ComplexFeature, FeatureStructure, FeatureSystem, FeatureSystems, FeatureValue,
    FeatureValueKind, FeatureValueSymbol,
};
pub use lexicon::{AffixProcess, Allomorph, EntryRef, LexEntry, Lexicon, Msa, RuleMapping, Sense};
pub use morphology::{
    ActiveParser, AdhocProhibition, Adjacency, AffixSlot, AffixTemplate,
    CompoundConstituentRequirement, CompoundOutcome, CompoundRule, CompoundRuleMaxApplications,
    ExceptionFeature, InflectionClass, LexEntryInflType, MorphType, Morphology, ParserParameters,
    PartOfSpeech, StemName, XAmpleParameters,
};
pub use phonology::{
    BoundaryMarker, Environment, FeatureConstraint, MetathesisRule, NaturalClass, PhonContext,
    Phoneme, PhonologicalRule, Phonology, RewriteRhs, RewriteRule, RuleDirection,
};
pub use project::Project;
pub use warning::{canonical_guid, Audience, FwClass, FwObjectRef, ImportWarningCode, Warning};
pub use warning_metadata::{import_warning_metadata, ImportWarningMetadata};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The `"format"` tag every valid snapshot document carries.
pub const FORMAT_TAG: &str = "pangloss-project";
/// The envelope version this build of `pg-snapshot` reads and writes.
pub const FORMAT_VERSION: u32 = 1;

/// Errors from `Snapshot::from_json`.
#[derive(Debug, Error)]
pub enum SnapshotError {
    /// The document isn't well-formed JSON, or doesn't match the `Snapshot` shape (missing/mistyped required fields).
    #[error("invalid snapshot JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The document parsed, but its `"format"` tag isn't the one this crate understands.
    #[error("unrecognized snapshot format {found:?}; expected {FORMAT_TAG:?}")]
    UnknownFormat { found: String },
    /// The document parsed and matched the format tag, but its `"version"` is one this build doesn't know how to read.
    #[error("unsupported snapshot version {found}; this build supports version {FORMAT_VERSION}")]
    UnsupportedVersion { found: u32 },
}

/// A complete PanGloss project snapshot: the versioned envelope plus every section described in
/// `docs/fwdata-import-plan.md` §3 / `docs/snapshot-format.md`.
///
/// Field declaration order below is exactly the order `Snapshot::to_json` emits keys in
/// (serde_json's struct default), and is deliberately envelope-first: `format`/`version` are the
/// first two keys of every emitted document. `conversion_provenance` is last and is excluded from
/// [`Snapshot::grammar_hash`]'s semantic projection — see that method's doc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub format: String,
    pub version: u32,
    pub project: Project,
    pub feature_systems: FeatureSystems,
    pub phonology: Phonology,
    pub morphology: Morphology,
    pub lexicon: Lexicon,
    #[serde(default)]
    pub conversion_provenance: ConversionProvenance,
}

impl Snapshot {
    /// Build a new snapshot with the current `FORMAT_TAG`/`FORMAT_VERSION` envelope
    /// already filled in — the normal way to construct one (rather than setting `format`/
    /// `version` by hand).
    pub fn new(
        project: Project,
        feature_systems: FeatureSystems,
        phonology: Phonology,
        morphology: Morphology,
        lexicon: Lexicon,
    ) -> Self {
        Snapshot {
            format: FORMAT_TAG.to_string(),
            version: FORMAT_VERSION,
            project,
            feature_systems,
            phonology,
            morphology,
            lexicon,
            conversion_provenance: ConversionProvenance::synthetic(),
        }
    }

    /// Parse a snapshot from JSON text, rejecting any document whose envelope doesn't match
    /// this crate's `FORMAT_TAG`/`FORMAT_VERSION`.
    pub fn from_json(json: &str) -> Result<Self, SnapshotError> {
        let snap: Snapshot = serde_json::from_str(json)?;
        if snap.format != FORMAT_TAG {
            return Err(SnapshotError::UnknownFormat { found: snap.format });
        }
        if snap.version != FORMAT_VERSION {
            return Err(SnapshotError::UnsupportedVersion {
                found: snap.version,
            });
        }
        Ok(snap)
    }

    /// Serialize to deterministic, pretty-printed JSON (see the module doc's "Determinism"
    /// section). Serialization of a well-formed `Snapshot` cannot fail in practice (every
    /// field is a plain data type with a total `Serialize` impl); this returns a `String`
    /// directly rather than a `Result` for caller convenience, panicking only if serde_json
    /// itself reports an error (e.g. a `NaN` float, which this format never produces — there
    /// are no floats in this schema at all).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("Snapshot serialization is infallible")
    }

    /// A stable hex digest (SHA-256) of this snapshot's semantic fields: the stats cache's
    /// grammar-change detector. Hashes every field except [`Snapshot::conversion_provenance`], so
    /// an import that changes only its diagnostics (a re-import with the same source graph, a
    /// new [`ConversionIssue`]) never invalidates the cache the way editing `project`,
    /// `feature_systems`, `phonology`, `morphology`, or `lexicon` does.
    pub fn grammar_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let json = serde_json::to_string_pretty(&GrammarHashInput::from(self))
            .expect("GrammarHashInput serialization is infallible");
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// Light structural validation: GUID cross-references that don't resolve within this same
    /// snapshot, reported as `Warning`s (a stable short code alongside human-readable prose).
    /// Never fails/panics; an empty `Vec` means nothing suspicious was found (not that the
    /// snapshot is semantically complete — see the `validate` module doc for scope).
    pub fn validate(&self) -> Vec<Warning> {
        validate::validate(self)
    }
}

/// `Snapshot`'s exhaustively-destructured semantic-field projection for [`Snapshot::grammar_hash`].
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GrammarHashInput<'a> {
    format: &'a str,
    version: u32,
    project: &'a Project,
    feature_systems: &'a FeatureSystems,
    phonology: &'a Phonology,
    morphology: &'a Morphology,
    lexicon: &'a Lexicon,
}

impl<'a> From<&'a Snapshot> for GrammarHashInput<'a> {
    fn from(snapshot: &'a Snapshot) -> Self {
        let Snapshot {
            format,
            version,
            project,
            feature_systems,
            phonology,
            morphology,
            lexicon,
            conversion_provenance: _,
        } = snapshot;
        GrammarHashInput {
            format: format.as_str(),
            version: *version,
            project,
            feature_systems,
            phonology,
            morphology,
            lexicon,
        }
    }
}

#[cfg(test)]
mod tests;
