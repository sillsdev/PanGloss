//! Closed, versioned requests for exact FieldWorks MSA-addressed generation.

use std::fs::File;
use std::io::Read;

use pg_grammar::model::Grammar;
use pg_parse::morpher::{GenMorpheme, Morpher};
use pg_parse::source_identity::{
    resolve_authored_morpheme, resolve_fw_entry_msa, ResolvedMorphemeOwner, SourceIdentityError,
};
use pg_snapshot::FwClass;
use serde::Deserialize;

/// Maximum request document size read from disk, including whitespace.
pub(crate) const MAX_GENERATE_REQUEST_BYTES: usize = 64 * 1024;
/// Maximum number of selected morphemes, including the root.
pub(crate) const MAX_GENERATE_REQUEST_MORPHEMES: usize = 64;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum GenerateRequestError {
    InvalidJson(String),
    RequestTooLarge {
        limit: usize,
    },
    RequestRead {
        path: String,
        detail: String,
    },
    UnsupportedSchemaVersion {
        found: u32,
    },
    TooManyMorphemes {
        limit: usize,
    },
    NonCanonicalGuid {
        path: String,
        value: String,
    },
    WrongRootMsaClass {
        found: String,
    },
    UnsupportedMsaClass {
        path: String,
        found: String,
    },
    UnsupportedFormSelection {
        path: String,
        form_guid: String,
    },
    AuthoredIdentityUnavailable,
    UnknownEntry {
        entry_guid: String,
    },
    UnknownMsa {
        msa_guid: String,
    },
    MsaNotOwnedByEntry {
        entry_guid: String,
        msa_guid: String,
    },
    AmbiguousIdentity {
        entry_guid: String,
        msa_guid: String,
    },
    RequestPathMissing,
    MixedSelectors,
}

impl std::fmt::Display for GenerateRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(detail) => write!(f, "InvalidGenerateRequest: {detail}"),
            Self::RequestTooLarge { limit } => {
                write!(f, "GenerateRequestTooLarge: request exceeds {limit} bytes")
            }
            Self::RequestRead { path, detail } => {
                write!(f, "GenerateRequestRead: cannot read {path}: {detail}")
            }
            Self::UnsupportedSchemaVersion { found } => write!(
                f,
                "UnsupportedGenerateRequestVersion: schemaVersion {found}; supported version is 1"
            ),
            Self::TooManyMorphemes { limit } => write!(
                f,
                "GenerateRequestTooManyMorphemes: request exceeds {limit} selected morphemes"
            ),
            Self::NonCanonicalGuid { path, value } => write!(
                f,
                "GenerateRequestNonCanonicalGuid: {path} must be a lowercase hyphenated GUID, got {value:?}"
            ),
            Self::WrongRootMsaClass { found } => write!(
                f,
                "GenerateRequestWrongRootMsaClass: root.msaClass must be MoStemMsa, got {found}"
            ),
            Self::UnsupportedMsaClass { path, found } => write!(
                f,
                "GenerateRequestUnsupportedMsaClass: {path}.msaClass {found:?} is unsupported"
            ),
            Self::UnsupportedFormSelection { path, form_guid } => write!(
                f,
                "UnsupportedFormSelection: {path}={form_guid}; v1 selects an MSA and cannot constrain synthesis to a source form"
            ),
            Self::AuthoredIdentityUnavailable => write!(
                f,
                "AuthoredIdentityUnavailable: this grammar has no FieldWorks entry/MSA identities for request mode; use positional <MorphemeId> selectors with HC XML, or load an imported FieldWorks snapshot"
            ),
            Self::UnknownEntry { entry_guid } => {
                write!(f, "GenerateRequestUnknownEntry: no compiled source entry {entry_guid}")
            }
            Self::UnknownMsa { msa_guid } => {
                write!(f, "GenerateRequestUnknownMsa: no compiled source MSA {msa_guid}")
            }
            Self::MsaNotOwnedByEntry {
                entry_guid,
                msa_guid,
            } => write!(
                f,
                "GenerateRequestMsaNotOwnedByEntry: MSA {msa_guid} is not owned by entry {entry_guid}"
            ),
            Self::AmbiguousIdentity {
                entry_guid,
                msa_guid,
            } => write!(
                f,
                "GenerateRequestAmbiguousIdentity: entry {entry_guid} and MSA {msa_guid} resolve to multiple compiled morphemes"
            ),
            Self::RequestPathMissing => {
                write!(f, "GenerateRequestPathMissing: --request requires a JSON file path")
            }
            Self::MixedSelectors => write!(
                f,
                "GenerateRequestMixedSelectors: use either positional <MorphemeId> selectors or --request <request.json>, not both"
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerateRequestV1 {
    schema_version: u32,
    root: AuthoredMorphemeRef,
    morphemes: Vec<AuthoredMorphemeRef>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthoredMorphemeRef {
    entry_guid: String,
    msa_guid: String,
    msa_class: AuthoredMsaClass,
    #[serde(default)]
    form_guid: Option<String>,
}

#[derive(Debug)]
enum AuthoredMsaClass {
    Stem,
    Inflectional,
    Derivational,
    UnclassifiedAffix,
    Unsupported(String),
}

impl AuthoredMsaClass {
    fn from_wire(value: String) -> Self {
        match value.as_str() {
            "MoStemMsa" => Self::Stem,
            "MoInflAffMsa" => Self::Inflectional,
            "MoDerivAffMsa" => Self::Derivational,
            "MoUnclassifiedAffixMsa" => Self::UnclassifiedAffix,
            _ => Self::Unsupported(value),
        }
    }

    fn class(&self) -> Option<FwClass> {
        match self {
            Self::Stem => Some(FwClass::MoStemMsa),
            Self::Inflectional => Some(FwClass::MoInflAffMsa),
            Self::Derivational => Some(FwClass::MoDerivAffMsa),
            Self::UnclassifiedAffix => Some(FwClass::MoUnclassifiedAffixMsa),
            Self::Unsupported(_) => None,
        }
    }

    fn wire_name(&self) -> &str {
        match self {
            Self::Stem => "MoStemMsa",
            Self::Inflectional => "MoInflAffMsa",
            Self::Derivational => "MoDerivAffMsa",
            Self::UnclassifiedAffix => "MoUnclassifiedAffixMsa",
            Self::Unsupported(value) => value,
        }
    }
}

impl<'de> serde::Deserialize<'de> for AuthoredMsaClass {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from_wire)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedMorphemeRef {
    pub(crate) entry_guid: String,
    pub(crate) msa_guid: String,
    pub(crate) msa_class: FwClass,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedGenerateRequest {
    pub(crate) root: ValidatedMorphemeRef,
    pub(crate) morphemes: Vec<ValidatedMorphemeRef>,
}

pub(crate) fn parse_request_json(
    bytes: &[u8],
) -> Result<ValidatedGenerateRequest, GenerateRequestError> {
    if bytes.len() > MAX_GENERATE_REQUEST_BYTES {
        return Err(GenerateRequestError::RequestTooLarge {
            limit: MAX_GENERATE_REQUEST_BYTES,
        });
    }
    let request: GenerateRequestV1 = serde_json::from_slice(bytes)
        .map_err(|error| GenerateRequestError::InvalidJson(error.to_string()))?;
    if request.schema_version != 1 {
        return Err(GenerateRequestError::UnsupportedSchemaVersion {
            found: request.schema_version,
        });
    }
    if request.morphemes.len().saturating_add(1) > MAX_GENERATE_REQUEST_MORPHEMES {
        return Err(GenerateRequestError::TooManyMorphemes {
            limit: MAX_GENERATE_REQUEST_MORPHEMES,
        });
    }

    let root = validate_morpheme_ref(request.root, "root")?;
    let morphemes = request
        .morphemes
        .into_iter()
        .enumerate()
        .map(|(index, item)| validate_morpheme_ref(item, &format!("morphemes[{index}]")))
        .collect::<Result<Vec<_>, _>>()?;
    if root.msa_class != FwClass::MoStemMsa {
        return Err(GenerateRequestError::WrongRootMsaClass {
            found: format_msa_class(root.msa_class),
        });
    }

    Ok(ValidatedGenerateRequest { root, morphemes })
}

fn validate_morpheme_ref(
    item: AuthoredMorphemeRef,
    path: &str,
) -> Result<ValidatedMorphemeRef, GenerateRequestError> {
    let Some(msa_class) = item.msa_class.class() else {
        return Err(GenerateRequestError::UnsupportedMsaClass {
            path: path.into(),
            found: item.msa_class.wire_name().into(),
        });
    };
    for (field, value) in [("entryGuid", &item.entry_guid), ("msaGuid", &item.msa_guid)] {
        if !is_canonical_guid(value) {
            return Err(GenerateRequestError::NonCanonicalGuid {
                path: format!("{path}.{field}"),
                value: value.clone(),
            });
        }
    }
    if let Some(form_guid) = item.form_guid.as_deref() {
        if !is_canonical_guid(form_guid) {
            return Err(GenerateRequestError::NonCanonicalGuid {
                path: format!("{path}.formGuid"),
                value: form_guid.into(),
            });
        }
        return Err(GenerateRequestError::UnsupportedFormSelection {
            path: format!("{path}.formGuid"),
            form_guid: form_guid.into(),
        });
    }
    Ok(ValidatedMorphemeRef {
        entry_guid: item.entry_guid,
        msa_guid: item.msa_guid,
        msa_class,
    })
}

fn is_canonical_guid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(byte),
        })
}

fn format_msa_class(class: FwClass) -> String {
    match class {
        FwClass::MoStemMsa => "MoStemMsa",
        FwClass::MoInflAffMsa => "MoInflAffMsa",
        FwClass::MoDerivAffMsa => "MoDerivAffMsa",
        FwClass::MoUnclassifiedAffixMsa => "MoUnclassifiedAffixMsa",
        _ => "unsupported",
    }
    .into()
}

pub(crate) fn read_request_file(
    path: &str,
) -> Result<ValidatedGenerateRequest, GenerateRequestError> {
    let mut file = File::open(path).map_err(|error| GenerateRequestError::RequestRead {
        path: path.into(),
        detail: error.to_string(),
    })?;
    let mut bytes = Vec::with_capacity(MAX_GENERATE_REQUEST_BYTES + 1);
    file.by_ref()
        .take((MAX_GENERATE_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| GenerateRequestError::RequestRead {
            path: path.into(),
            detail: error.to_string(),
        })?;
    parse_request_json(&bytes)
}

pub(crate) fn generate_request_words(
    grammar: &Grammar,
    request: &ValidatedGenerateRequest,
) -> Result<Vec<String>, GenerateRequestError> {
    let root = resolve_fw_entry_msa(
        grammar,
        &request.root.entry_guid,
        &request.root.msa_guid,
        request.root.msa_class,
    )
    .map_err(|error| map_identity_error(error, &request.root))?;
    let others = request
        .morphemes
        .iter()
        .map(|item| {
            resolve_authored_morpheme(grammar, &item.entry_guid, &item.msa_guid, item.msa_class)
                .map(|resolved| match resolved.owner {
                    ResolvedMorphemeOwner::LexicalEntry(entry) => GenMorpheme::NonHead(entry),
                    ResolvedMorphemeOwner::MorphologicalRule(rule) => GenMorpheme::Rule(rule),
                })
                .map_err(|error| map_identity_error(error, item))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let morpher = Morpher::new(grammar, crate::DEFAULT_STEP_CAP.as_morpher_cap());
    Ok(morpher.generate_words(root, &others, pg_featstruct::FeatureStruct::EMPTY))
}

fn map_identity_error(
    error: SourceIdentityError,
    item: &ValidatedMorphemeRef,
) -> GenerateRequestError {
    match error {
        SourceIdentityError::EmptyLexicon | SourceIdentityError::AuthoredIdentityUnavailable => {
            GenerateRequestError::AuthoredIdentityUnavailable
        }
        SourceIdentityError::UnknownEntry => GenerateRequestError::UnknownEntry {
            entry_guid: item.entry_guid.clone(),
        },
        SourceIdentityError::UnknownMsa => GenerateRequestError::UnknownMsa {
            msa_guid: item.msa_guid.clone(),
        },
        SourceIdentityError::MsaNotOwnedByEntry => GenerateRequestError::MsaNotOwnedByEntry {
            entry_guid: item.entry_guid.clone(),
            msa_guid: item.msa_guid.clone(),
        },
        SourceIdentityError::AmbiguousIdentity => GenerateRequestError::AmbiguousIdentity {
            entry_guid: item.entry_guid.clone(),
            msa_guid: item.msa_guid.clone(),
        },
        SourceIdentityError::UnsupportedMsaClass => GenerateRequestError::UnsupportedMsaClass {
            path: "request".into(),
            found: format_msa_class(item.msa_class),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = "00000000-0000-0000-0000-000000000030";
    const MSA: &str = "00000000-0000-0000-0000-000000000032";

    fn request(root_class: &str) -> String {
        format!(
            r#"{{"schemaVersion":1,"root":{{"entryGuid":"{ENTRY}","msaGuid":"{MSA}","msaClass":"{root_class}"}},"morphemes":[]}}"#
        )
    }

    #[test]
    fn generate_request_rejects_unknown_fields_at_every_object_level() {
        let json =
            request("MoStemMsa").replace("\"morphemes\":[]", "\"morphemes\":[],\"fallback\":true");
        assert!(matches!(
            parse_request_json(json.as_bytes()),
            Err(GenerateRequestError::InvalidJson(_))
        ));

        let json = request("MoStemMsa").replace("\"msaGuid\":", "\"extra\":1,\"msaGuid\":");
        assert!(matches!(
            parse_request_json(json.as_bytes()),
            Err(GenerateRequestError::InvalidJson(_))
        ));
    }

    #[test]
    fn generate_request_refuses_unsupported_versions_and_msa_classes() {
        let json = request("MoStemMsa").replace("schemaVersion\":1", "schemaVersion\":2");
        assert_eq!(
            parse_request_json(json.as_bytes()),
            Err(GenerateRequestError::UnsupportedSchemaVersion { found: 2 })
        );
        assert!(matches!(
            parse_request_json(request("MoCompoundRule").as_bytes()),
            Err(GenerateRequestError::UnsupportedMsaClass { .. })
        ));
    }

    #[test]
    fn generate_request_refuses_noncanonical_guids_and_wrong_root_class() {
        let invalid = request("MoStemMsa").replace(ENTRY, "00000000-0000-0000-0000-00000000003A");
        assert!(matches!(
            parse_request_json(invalid.as_bytes()),
            Err(GenerateRequestError::NonCanonicalGuid { .. })
        ));
        assert!(matches!(
            parse_request_json(request("MoInflAffMsa").as_bytes()),
            Err(GenerateRequestError::WrongRootMsaClass { .. })
        ));
    }

    #[test]
    fn generate_request_refuses_every_non_null_form_selector() {
        let root_form = request("MoStemMsa").replace(
            "\"msaClass\":\"MoStemMsa\"",
            "\"msaClass\":\"MoStemMsa\",\"formGuid\":\"00000000-0000-0000-0000-000000000031\"",
        );
        assert!(matches!(
            parse_request_json(root_form.as_bytes()),
            Err(GenerateRequestError::UnsupportedFormSelection { path, .. })
                if path == "root.formGuid"
        ));

        let item_form = request("MoStemMsa").replace(
            "\"morphemes\":[]",
            "\"morphemes\":[{\"entryGuid\":\"00000000-0000-0000-0000-000000000050\",\"msaGuid\":\"00000000-0000-0000-0000-000000000052\",\"msaClass\":\"MoInflAffMsa\",\"formGuid\":\"00000000-0000-0000-0000-000000000051\"}]",
        );
        assert!(matches!(
            parse_request_json(item_form.as_bytes()),
            Err(GenerateRequestError::UnsupportedFormSelection { path, .. })
                if path == "morphemes[0].formGuid"
        ));
    }

    #[test]
    fn generate_request_enforces_finite_document_and_chain_limits() {
        assert_eq!(
            parse_request_json(&vec![b' '; MAX_GENERATE_REQUEST_BYTES + 1]),
            Err(GenerateRequestError::RequestTooLarge {
                limit: MAX_GENERATE_REQUEST_BYTES
            })
        );
        let extra = format!(
            "{{\"entryGuid\":\"{ENTRY}\",\"msaGuid\":\"{MSA}\",\"msaClass\":\"MoStemMsa\"}}"
        );
        let json = format!(
            "{{\"schemaVersion\":1,\"root\":{{\"entryGuid\":\"{ENTRY}\",\"msaGuid\":\"{MSA}\",\"msaClass\":\"MoStemMsa\"}},\"morphemes\":[{}]}}",
            std::iter::repeat_n(extra, MAX_GENERATE_REQUEST_MORPHEMES)
                .collect::<Vec<_>>()
                .join(",")
        );
        assert_eq!(
            parse_request_json(json.as_bytes()),
            Err(GenerateRequestError::TooManyMorphemes {
                limit: MAX_GENERATE_REQUEST_MORPHEMES
            })
        );
    }

    #[test]
    fn authored_generation_refusal_explains_xml_identity_recovery() {
        let grammar = pg_grammar::load(crate::tests::MINI_GRAMMAR_XML)
            .expect("committed HC XML unit fixture loads");
        let request = parse_request_json(request("MoStemMsa").as_bytes()).expect("valid DTO");
        let error = generate_request_words(&grammar, &request).unwrap_err();
        assert_eq!(error, GenerateRequestError::AuthoredIdentityUnavailable);
        assert!(error.to_string().contains("positional <MorphemeId>"));
    }
}
