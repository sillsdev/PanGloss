//! Parses `phonology-mutations.yaml` (`machine/conformance/PROTOCOL.md` section 10): the small,
//! versioned manifest of scripted LibLCM phoneme-inventory mutations checked in beside a
//! FieldWorks witness project, and each case's expectations relative to that unmodified baseline.
//!
//! Version 1 is the only recognized shape: `remove_phoneme`/`remove_all_phonemes` operations and
//! `same_as_base` the only defined `expect` relation, mirroring
//! `tools/xample-projector/build.ps1`'s own `ConvertFrom-PhonologyMutationsYaml` (that PowerShell
//! parser is this manifest's first, already-proven reader; this is the same shape, in Rust, per
//! that file's own "the Rust side owns YAML later" note). A manifest naming anything else is a
//! refusal, never a best-effort guess -- the same discipline `crate::reader` applies to the
//! helper's JSON responses.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};

const SUPPORTED_MANIFEST_VERSION: u64 = 1;

/// One parsed, version-checked `phonology-mutations.yaml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhonologyMutations {
    pub base_sha256: String,
    pub cases: Vec<MutationCase>,
}

impl PhonologyMutations {
    pub fn case(&self, id: &str) -> Option<&MutationCase> {
        self.cases.iter().find(|c| c.id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationCase {
    pub id: String,
    pub operations: Vec<MutationOperation>,
    pub expect: MutationExpectation,
}

impl MutationCase {
    /// The wire-format request `tools/xample-projector`'s `mutate` subcommand expects (README.md's
    /// `mutate` contract) -- built here, not re-derived at each call site, so a manifest case and
    /// the request sent for it cannot drift apart.
    pub fn to_request_json(&self, base_sha256: &str) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 1,
            "caseId": self.id,
            "baseSha256": base_sha256,
            "operations": self.operations.iter().map(MutationOperation::to_wire_json).collect::<Vec<_>>(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationOperation {
    RemovePhoneme {
        guid: String,
        assert_representations: Vec<String>,
        require_unreferenced: bool,
    },
    RemoveAllPhonemes {
        require_unreferenced: bool,
    },
}

impl MutationOperation {
    fn to_wire_json(&self) -> serde_json::Value {
        match self {
            MutationOperation::RemovePhoneme {
                guid,
                assert_representations,
                require_unreferenced,
            } => {
                serde_json::json!({
                    "op": "remove_phoneme",
                    "guid": guid,
                    "assertRepresentations": assert_representations,
                    "requireUnreferenced": require_unreferenced,
                })
            }
            MutationOperation::RemoveAllPhonemes {
                require_unreferenced,
            } => serde_json::json!({
                "op": "remove_all_phonemes",
                "requireUnreferenced": require_unreferenced,
            }),
        }
    }
}

/// v1's only defined value for `expect.xample_projection`/`expect.hc_analyses`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectRelation {
    SameAsBase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationExpectation {
    pub xample_projection: ExpectRelation,
    pub hc_analyses: ExpectRelation,
    pub inferred_segments: Vec<String>,
}

#[derive(Debug)]
pub enum FixtureError {
    Malformed(serde_yaml::Error),
    UnsupportedVersion {
        found: u64,
    },
    UnknownOperation {
        case_id: String,
        found: String,
    },
    UnsupportedExpectRelation {
        case_id: String,
        field: &'static str,
        found: String,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Sha256Mismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FixtureError::Malformed(e) => write!(f, "malformed phonology-mutations.yaml: {e}"),
            FixtureError::UnsupportedVersion { found } => write!(
                f,
                "phonology-mutations.yaml declares version {found}, but this reader only \
                 understands version {SUPPORTED_MANIFEST_VERSION}"
            ),
            FixtureError::UnknownOperation { case_id, found } => write!(
                f,
                "case {case_id:?}: operation {found:?} is not in the v1 vocabulary (remove_phoneme, \
                 remove_all_phonemes)"
            ),
            FixtureError::UnsupportedExpectRelation { case_id, field, found } => write!(
                f,
                "case {case_id:?}: expect.{field} is {found:?}, which is outside the v1 \
                 vocabulary (only \"same_as_base\" is defined)"
            ),
            FixtureError::Io { path, source } => {
                write!(f, "{}: {source}", path.display())
            }
            FixtureError::Sha256Mismatch { path, expected, actual } => write!(
                f,
                "{}: sha256 {actual} does not match phonology-mutations.yaml's declared \
                 base_sha256 {expected}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for FixtureError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    version: u64,
    base_sha256: String,
    cases: Vec<RawCase>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCase {
    id: String,
    // Raw YAML values, not a derived enum: `serde_yaml`'s deserialize_enum wants its own `!tag` syntax, never a plain single-key mapping.
    operations: Vec<serde_yaml::Value>,
    expect: RawExpect,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemovePhonemeBody {
    guid: String,
    assert_representations: Vec<String>,
    require_unreferenced: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveAllPhonemesBody {
    require_unreferenced: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawExpect {
    xample_projection: String,
    hc_analyses: String,
    inferred_segments: Vec<String>,
}

/// Parse and version-check one `phonology-mutations.yaml` document. Does not touch the filesystem —
/// see [`verify_base_sha256`] for checking the declared digest against a real project file.
pub fn parse_manifest(yaml_text: &str) -> Result<PhonologyMutations, FixtureError> {
    let raw: RawManifest = serde_yaml::from_str(yaml_text).map_err(FixtureError::Malformed)?;
    if raw.version != SUPPORTED_MANIFEST_VERSION {
        return Err(FixtureError::UnsupportedVersion { found: raw.version });
    }
    let mut cases = Vec::with_capacity(raw.cases.len());
    for raw_case in raw.cases {
        let mut operations = Vec::with_capacity(raw_case.operations.len());
        for raw_op in raw_case.operations {
            operations.push(convert_operation(&raw_case.id, raw_op)?);
        }
        let expect = convert_expect(&raw_case.id, raw_case.expect)?;
        cases.push(MutationCase {
            id: raw_case.id,
            operations,
            expect,
        });
    }
    Ok(PhonologyMutations {
        base_sha256: raw.base_sha256,
        cases,
    })
}

// One `operations[]` entry: a single-key mapping, dispatched on that key by hand (see `RawCase.operations`).
fn convert_operation(
    case_id: &str,
    value: serde_yaml::Value,
) -> Result<MutationOperation, FixtureError> {
    let mapping = value.as_mapping().filter(|m| m.len() == 1).ok_or_else(|| {
        FixtureError::UnknownOperation {
            case_id: case_id.to_string(),
            found: format!("{value:?}"),
        }
    })?;
    let (key, body) = mapping.iter().next().expect("checked len == 1 above");
    let key_str = key.as_str().ok_or_else(|| FixtureError::UnknownOperation {
        case_id: case_id.to_string(),
        found: format!("{key:?}"),
    })?;
    match key_str {
        "remove_phoneme" => {
            let body: RemovePhonemeBody =
                serde_yaml::from_value(body.clone()).map_err(FixtureError::Malformed)?;
            Ok(MutationOperation::RemovePhoneme {
                guid: body.guid,
                assert_representations: body.assert_representations,
                require_unreferenced: body.require_unreferenced,
            })
        }
        "remove_all_phonemes" => {
            let body: RemoveAllPhonemesBody =
                serde_yaml::from_value(body.clone()).map_err(FixtureError::Malformed)?;
            Ok(MutationOperation::RemoveAllPhonemes {
                require_unreferenced: body.require_unreferenced,
            })
        }
        other => Err(FixtureError::UnknownOperation {
            case_id: case_id.to_string(),
            found: other.to_string(),
        }),
    }
}

fn convert_expect(case_id: &str, raw: RawExpect) -> Result<MutationExpectation, FixtureError> {
    Ok(MutationExpectation {
        xample_projection: parse_relation(case_id, "xample_projection", &raw.xample_projection)?,
        hc_analyses: parse_relation(case_id, "hc_analyses", &raw.hc_analyses)?,
        inferred_segments: raw.inferred_segments,
    })
}

fn parse_relation(
    case_id: &str,
    field: &'static str,
    value: &str,
) -> Result<ExpectRelation, FixtureError> {
    match value {
        "same_as_base" => Ok(ExpectRelation::SameAsBase),
        other => Err(FixtureError::UnsupportedExpectRelation {
            case_id: case_id.to_string(),
            field,
            found: other.to_string(),
        }),
    }
}

/// Hashes `project_path` and compares it (case-insensitively) to `manifest.base_sha256` --
/// verifying the manifest still describes the project checked in beside it, per this crate's
/// CLAUDE.md rule that a stale manifest must fail loudly rather than be silently trusted.
pub fn verify_base_sha256(
    manifest: &PhonologyMutations,
    project_path: &Path,
) -> Result<(), FixtureError> {
    let bytes = std::fs::read(project_path).map_err(|source| FixtureError::Io {
        path: project_path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual = format!("{:x}", hasher.finalize());
    if actual.eq_ignore_ascii_case(&manifest.base_sha256) {
        Ok(())
    } else {
        Err(FixtureError::Sha256Mismatch {
            path: project_path.to_path_buf(),
            expected: manifest.base_sha256.clone(),
            actual,
        })
    }
}

#[cfg(test)]
mod tests;
