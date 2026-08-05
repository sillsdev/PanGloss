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
//! [`Snapshot::from_json`] rejects any document whose `format`/`version` don't match what this
//! build understands, with a specific [`SnapshotError`] variant — never a generic parse failure.
//!
//! # Determinism
//!
//! [`Snapshot::to_json`] always pretty-prints with two-space indentation and struct fields in
//! their Rust declaration order (serde's default, unmodified). Every collection in this format
//! is a plain `Vec` — **construction order is preserved on the wire**; nothing is sorted or
//! reordered by this crate. Where the source data has a meaningful order (affix template slots,
//! rule application order, allomorph disjunctive-ordering, ...) that order is the snapshot's
//! order; where the source data is an unordered LCM collection, the snapshot's order is simply
//! whatever order the producer (`pg-fwdata`) encountered it in, which is itself deterministic
//! across imports of the same `.fwdata` file (`docs/fwdata-import-plan.md` §5.3). Two
//! [`Snapshot`]s built with the same field values in the same order always serialize to
//! byte-identical JSON.
//!
//! # Validation
//!
//! [`Snapshot::validate`] is a light, warning-only check for dangling GUID references (see the
//! [`validate`] module doc for exactly what is and isn't checked). It never rejects a snapshot —
//! real FieldWorks projects contain stale references, and this pipeline must tolerate them
//! (`docs/fwdata-import-plan.md` §1).
#![forbid(unsafe_code)]

pub mod common;
pub mod feature;
pub mod lexicon;
pub mod morphology;
pub mod phonology;
pub mod project;
pub mod validate;
mod warning;

pub use common::{Guid, WsForm};
pub use feature::{
    ClosedFeature, ComplexFeature, FeatureStructure, FeatureSystem, FeatureSystems, FeatureValue,
    FeatureValueKind, FeatureValueSymbol,
};
pub use lexicon::{AffixProcess, Allomorph, EntryRef, LexEntry, Lexicon, Msa, RuleMapping, Sense};
pub use morphology::{
    AdhocProhibition, Adjacency, AffixSlot, AffixTemplate, CompoundConstituentRequirement,
    CompoundOutcome, CompoundRule, CompoundRuleMaxApplications, ExceptionFeature, InflectionClass,
    LexEntryInflType, MorphType, Morphology, ParserParameters, PartOfSpeech, StemName,
};
pub use phonology::{
    BoundaryMarker, Environment, FeatureConstraint, MetathesisRule, NaturalClass, PhonContext,
    Phoneme, PhonologicalRule, Phonology, RewriteRhs, RewriteRule, RuleDirection,
};
pub use project::Project;
pub use warning::Warning;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The `"format"` tag every valid snapshot document carries.
pub const FORMAT_TAG: &str = "pangloss-project";
/// The envelope version this build of `pg-snapshot` reads and writes.
pub const FORMAT_VERSION: u32 = 1;

/// Errors from [`Snapshot::from_json`].
#[derive(Debug, Error)]
pub enum SnapshotError {
    /// The document isn't well-formed JSON, or doesn't match the [`Snapshot`] shape at all
    /// (missing/mistyped required fields).
    #[error("invalid snapshot JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The document parsed, but its `"format"` tag isn't the one this crate understands.
    #[error("unrecognized snapshot format {found:?}; expected {FORMAT_TAG:?}")]
    UnknownFormat { found: String },
    /// The document parsed and its format tag matched, but its `"version"` is one this build
    /// doesn't know how to read.
    #[error("unsupported snapshot version {found}; this build supports version {FORMAT_VERSION}")]
    UnsupportedVersion { found: u32 },
}

/// A complete PanGloss project snapshot: the versioned envelope plus every section described in
/// `docs/fwdata-import-plan.md` §3 / `docs/snapshot-format.md`.
///
/// Field declaration order below is exactly the order [`Snapshot::to_json`] emits keys in
/// (serde_json's struct default), and is deliberately envelope-first: `format`/`version` are the
/// first two keys of every emitted document.
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
}

impl Snapshot {
    /// Build a new snapshot with the current [`FORMAT_TAG`]/[`FORMAT_VERSION`] envelope
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
        }
    }

    /// Parse a snapshot from JSON text, rejecting any document whose envelope doesn't match
    /// this crate's [`FORMAT_TAG`]/[`FORMAT_VERSION`].
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
    /// section). Serialization of a well-formed [`Snapshot`] cannot fail in practice (every
    /// field is a plain data type with a total `Serialize` impl); this returns a `String`
    /// directly rather than a `Result` for caller convenience, panicking only if serde_json
    /// itself reports an error (e.g. a `NaN` float, which this format never produces — there
    /// are no floats in this schema at all).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("Snapshot serialization is infallible")
    }

    /// Light structural validation: GUID cross-references that don't resolve within this same
    /// snapshot, reported as [`Warning`]s (a stable short code alongside human-readable prose).
    /// Never fails/panics; an empty `Vec` means nothing suspicious was found (not that the
    /// snapshot is semantically complete — see the [`validate`] module doc for scope).
    pub fn validate(&self) -> Vec<Warning> {
        validate::validate(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::WsForm;
    use crate::feature::{
        ClosedFeature, FeatureStructure, FeatureSystem, FeatureValue, FeatureValueKind,
        FeatureValueSymbol,
    };
    use crate::lexicon::{Allomorph, LexEntry, Lexicon, Msa, Sense};
    use crate::morphology::{MorphType, Morphology, ParserParameters, PartOfSpeech};
    use crate::phonology::{Phoneme, Phonology};

    fn ws(ws: &str, form: &str) -> WsForm {
        WsForm {
            ws: ws.to_string(),
            form: form.to_string(),
        }
    }

    /// A small but structurally rich snapshot: one closed feature, one phoneme, one POS, one
    /// lex entry with a stem MSA and a sense referencing it. Used by the round-trip and
    /// validation tests below.
    fn sample_snapshot() -> Snapshot {
        let noun_pos_guid = "11111111-1111-1111-1111-111111111111".to_string();
        let number_feature_guid = "22222222-2222-2222-2222-222222222222".to_string();
        let sg_guid = "33333333-3333-3333-3333-333333333333".to_string();
        let pl_guid = "44444444-4444-4444-4444-444444444444".to_string();
        let phoneme_guid = "55555555-5555-5555-5555-555555555555".to_string();
        let entry_guid = "66666666-6666-6666-6666-666666666666".to_string();
        let allomorph_guid = "77777777-7777-7777-7777-777777777777".to_string();
        let msa_guid = "88888888-8888-8888-8888-888888888888".to_string();
        let sense_guid = "99999999-9999-9999-9999-999999999999".to_string();

        let project = Project {
            name: "Test Project".to_string(),
            vernacular_writing_systems: vec!["sen".to_string()],
            analysis_writing_systems: vec!["en".to_string()],
        };

        let feature_systems = FeatureSystems {
            phonological: FeatureSystem::default(),
            morphosyntactic: FeatureSystem {
                closed_features: vec![ClosedFeature {
                    guid: number_feature_guid.clone(),
                    name: "Number".to_string(),
                    abbreviation: "num".to_string(),
                    values: vec![
                        FeatureValueSymbol {
                            guid: sg_guid.clone(),
                            name: "singular".to_string(),
                            abbreviation: "sg".to_string(),
                        },
                        FeatureValueSymbol {
                            guid: pl_guid.clone(),
                            name: "plural".to_string(),
                            abbreviation: "pl".to_string(),
                        },
                    ],
                }],
                complex_features: vec![],
            },
        };

        let phonology = Phonology {
            phonemes: vec![Phoneme {
                guid: phoneme_guid,
                name: "a".to_string(),
                representations: vec![ws("sen", "a")],
                features: None,
                basic_ipa_symbol: Some("a".to_string()),
            }],
            ..Phonology::default()
        };

        let morphology = Morphology {
            parts_of_speech: vec![PartOfSpeech {
                guid: noun_pos_guid.clone(),
                name: "Noun".to_string(),
                abbreviation: "n".to_string(),
                children: vec![],
                inflection_classes: vec![],
                default_inflection_class: None,
                inflectable_features: vec![number_feature_guid.clone()],
                stem_names: vec![],
                affix_slots: vec![],
                affix_templates: vec![],
            }],
            compound_rules: vec![],
            adhoc_prohibitions: vec![],
            exception_features: vec![],
            lex_entry_infl_types: vec![],
            parser_parameters: ParserParameters::default(),
        };

        let lexicon = Lexicon {
            entries: vec![LexEntry {
                guid: entry_guid,
                citation_form: vec![ws("sen", "kanga")],
                lexeme_morph_type: MorphType::Stem,
                allomorphs: vec![Allomorph {
                    guid: allomorph_guid,
                    morph_type: MorphType::Stem,
                    is_abstract: false,
                    forms: vec![ws("sen", "kanga")],
                    environments: vec![],
                    positions: vec![],
                    stem_name: None,
                    inflection_classes: vec![],
                    ms_env_features: None,
                    ms_env_part_of_speech: None,
                    process: None,
                }],
                msas: vec![Msa::Stem {
                    guid: msa_guid.clone(),
                    part_of_speech: Some(noun_pos_guid),
                    inflection_class: None,
                    features: Some(FeatureStructure {
                        values: vec![FeatureValue {
                            feature: number_feature_guid,
                            value: FeatureValueKind::Closed { value: sg_guid },
                        }],
                    }),
                    exception_features: vec![],
                    from_parts_of_speech: vec![],
                    slots: vec![],
                }],
                senses: vec![Sense {
                    guid: sense_guid,
                    gloss: vec![ws("en", "dog")],
                    definition: vec![],
                    msa: Some(msa_guid),
                }],
                entry_refs: vec![],
            }],
        };

        Snapshot::new(project, feature_systems, phonology, morphology, lexicon)
    }

    #[test]
    fn round_trips_through_json() {
        let snap = sample_snapshot();
        let json = snap.to_json();
        let parsed = Snapshot::from_json(&json).expect("valid snapshot JSON must parse");
        assert_eq!(snap, parsed);
    }

    #[test]
    fn to_json_is_deterministic_across_calls() {
        let snap = sample_snapshot();
        assert_eq!(snap.to_json(), snap.to_json());
    }

    #[test]
    fn to_json_emits_envelope_first() {
        let snap = sample_snapshot();
        let json = snap.to_json();
        let format_pos = json.find("\"format\"").unwrap();
        let version_pos = json.find("\"version\"").unwrap();
        let project_pos = json.find("\"project\"").unwrap();
        assert!(format_pos < version_pos);
        assert!(version_pos < project_pos);
    }

    #[test]
    fn sample_snapshot_validates_clean() {
        let snap = sample_snapshot();
        let warnings = snap.validate();
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    #[test]
    fn from_json_rejects_wrong_format_tag() {
        let mut snap = sample_snapshot();
        snap.format = "some-other-format".to_string();
        let json = snap.to_json();
        let err = Snapshot::from_json(&json).unwrap_err();
        assert!(
            matches!(err, SnapshotError::UnknownFormat { found } if found == "some-other-format")
        );
    }

    #[test]
    fn from_json_rejects_unsupported_version() {
        let mut snap = sample_snapshot();
        snap.version = 999;
        let json = snap.to_json();
        let err = Snapshot::from_json(&json).unwrap_err();
        assert!(matches!(
            err,
            SnapshotError::UnsupportedVersion { found: 999 }
        ));
    }

    #[test]
    fn from_json_rejects_malformed_json() {
        let err = Snapshot::from_json("{ not json").unwrap_err();
        assert!(matches!(err, SnapshotError::Json(_)));
    }

    #[test]
    fn validate_reports_dangling_sense_msa_reference_as_warning_not_error() {
        let mut snap = sample_snapshot();
        // Point the sense at a nonexistent MSA guid — mirrors the plan's motivating example of
        // a stale FieldWorks reference (§1): must produce a warning, and `from_json` must still
        // succeed on the round-tripped JSON (dangling refs are never a hard error).
        snap.lexicon.entries[0].senses[0].msa =
            Some("00000000-0000-0000-0000-000000000000".to_string());
        let json = snap.to_json();
        let reparsed = Snapshot::from_json(&json).expect("dangling refs must not block parsing");
        let warnings = reparsed.validate();
        assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
        assert!(warnings[0].contains("00000000-0000-0000-0000-000000000000"));
    }

    #[test]
    fn validate_reports_dangling_environment_reference() {
        let mut snap = sample_snapshot();
        snap.lexicon.entries[0].allomorphs[0]
            .environments
            .push("dangling-env-guid".to_string());
        let warnings = snap.validate();
        assert!(warnings.iter().any(|w| w.contains("dangling-env-guid")));
    }

    #[test]
    fn validate_reports_dangling_exception_feature_reference() {
        let mut snap = sample_snapshot();
        if let Msa::Stem {
            exception_features, ..
        } = &mut snap.lexicon.entries[0].msas[0]
        {
            exception_features.push("no-such-exception-feature".to_string());
        } else {
            panic!("expected the sample entry's MSA to be Msa::Stem");
        }
        let warnings = snap.validate();
        assert!(warnings
            .iter()
            .any(|w| w.contains("no-such-exception-feature")));
    }

    #[test]
    fn validate_accepts_exception_feature_present_in_registry() {
        let mut snap = sample_snapshot();
        snap.morphology
            .exception_features
            .push(crate::morphology::ExceptionFeature {
                guid: "latinate-guid".to_string(),
                name: "Latinate".to_string(),
                abbreviation: "lat".to_string(),
            });
        if let Msa::Stem {
            exception_features, ..
        } = &mut snap.lexicon.entries[0].msas[0]
        {
            exception_features.push("latinate-guid".to_string());
        } else {
            panic!("expected the sample entry's MSA to be Msa::Stem");
        }
        let warnings = snap.validate();
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    #[test]
    fn validate_reports_dangling_adhoc_prohibition_reference() {
        let mut snap = sample_snapshot();
        snap.morphology
            .adhoc_prohibitions
            .push(crate::morphology::AdhocProhibition::Morpheme {
                guid: "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".to_string(),
                disabled: false,
                primary: "does-not-exist".to_string(),
                others: vec!["also-missing".to_string()],
                adjacency: crate::morphology::Adjacency::Anywhere,
            });
        let warnings = snap.validate();
        assert!(warnings.iter().any(|w| w.contains("does-not-exist")));
        assert!(warnings.iter().any(|w| w.contains("also-missing")));
    }

    // --- warning codes -----------------------------------------------------------------------

    #[test]
    fn validate_dangling_reference_carries_the_expected_code() {
        let mut snap = sample_snapshot();
        snap.lexicon.entries[0].allomorphs[0]
            .environments
            .push("dangling-env-guid".to_string());
        let warnings = snap.validate();
        let hit = warnings
            .iter()
            .find(|w| w.contains("dangling-env-guid"))
            .expect("the dangling-environment warning must be present");
        assert_eq!(hit.code, "snapshot.dangling-reference");
    }

    /// Two structurally different situations -- a plain dangling cross-reference vs. a sense's
    /// MSA reference that resolves fine globally but not within its own owning entry -- must get
    /// different codes.
    #[test]
    fn validate_out_of_scope_reference_gets_a_different_code_than_dangling_reference() {
        let mut snap = sample_snapshot();
        snap.lexicon.entries[0].senses[0].msa =
            Some("00000000-0000-0000-0000-000000000000".to_string());
        snap.lexicon.entries[0].allomorphs[0]
            .environments
            .push("dangling-env-guid".to_string());
        let warnings = snap.validate();
        let scope_warning = warnings
            .iter()
            .find(|w| w.contains("does not resolve within this entry"))
            .expect("the out-of-scope sense/msa warning must be present");
        let dangling_warning = warnings
            .iter()
            .find(|w| w.contains("dangling-env-guid"))
            .expect("the dangling-environment warning must be present");
        assert_ne!(scope_warning.code, dangling_warning.code);
        assert_eq!(scope_warning.code, "snapshot.reference-out-of-scope");
        assert_eq!(dangling_warning.code, "snapshot.dangling-reference");
    }

    /// The same check, fired with two different dangling guids (so the interpolated `message`
    /// text necessarily differs), must still produce the identical `code` both times -- the code
    /// depends on which situation was detected, never on the specific text of the message (task
    /// 3.8 requirement (a)).
    #[test]
    fn validate_dangling_reference_code_is_stable_regardless_of_message_text() {
        let mut snap_a = sample_snapshot();
        snap_a.lexicon.entries[0].allomorphs[0]
            .environments
            .push("guid-aaa".to_string());
        let mut snap_b = sample_snapshot();
        snap_b.lexicon.entries[0].allomorphs[0]
            .environments
            .push("guid-bbb".to_string());

        let warnings_a = snap_a.validate();
        let warnings_b = snap_b.validate();
        assert_eq!(warnings_a.len(), 1);
        assert_eq!(warnings_b.len(), 1);
        assert_ne!(warnings_a[0].message, warnings_b[0].message);
        assert_eq!(warnings_a[0].code, warnings_b[0].code);
    }

    /// Existing prose is unchanged by this task -- same wording
    /// `validate_reports_dangling_sense_msa_reference_as_warning_not_error` already asserted a
    /// substring of before codes existed, now pinned exactly.
    #[test]
    fn validate_warning_prose_is_unchanged_at_representative_sites() {
        let mut snap = sample_snapshot();
        let entry_guid = snap.lexicon.entries[0].guid.clone();
        let sense_guid = snap.lexicon.entries[0].senses[0].guid.clone();
        let dangling_msa = "00000000-0000-0000-0000-000000000000".to_string();
        snap.lexicon.entries[0].senses[0].msa = Some(dangling_msa.clone());
        let warnings = snap.validate();
        assert_eq!(warnings.len(), 1);
        let expected = format!(
            "lex entry {entry_guid:?} sense {sense_guid:?}: msa {dangling_msa:?} does not \
             resolve within this entry"
        );
        assert_eq!(warnings[0].message, expected);
    }
}
