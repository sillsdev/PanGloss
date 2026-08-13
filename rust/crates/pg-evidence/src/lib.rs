//! Shared, backend-independent identity for PanGloss evidence artifacts.
#![forbid(unsafe_code)]

use serde::{Deserialize, Deserializer, Serialize};

/// Version of the shared evidence context wire contract.
pub const EVIDENCE_CONTEXT_VERSION: u32 = 1;

/// The source system's namespace for a construct identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstructIdKind {
    /// An identifier assigned by FieldWorks and stable across imports.
    FieldworksGuid,
    /// An identifier authored by a source format or PanGloss configuration.
    AuthoredId,
    /// An identifier assigned by the PanGloss compiler rather than source content.
    CompilerAssigned,
}

/// Errors returned when constructing a typed evidence identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityError {
    /// A component was empty or contained only whitespace.
    EmptyComponent,
    /// An observation definition version must be greater than zero.
    InvalidDefinitionVersion,
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyComponent => write!(f, "identity components must not be empty"),
            Self::InvalidDefinitionVersion => {
                write!(
                    f,
                    "observation definition version must be greater than zero"
                )
            }
        }
    }
}

impl std::error::Error for IdentityError {}

/// A stable source identity for a construct that can produce evidence.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ConstructKey {
    namespace: String,
    kind: String,
    id: String,
    id_kind: ConstructIdKind,
}

impl ConstructKey {
    /// Build a construct identity, rejecting ambiguous empty components.
    pub fn new(
        namespace: impl Into<String>,
        kind: impl Into<String>,
        id: impl Into<String>,
        id_kind: ConstructIdKind,
    ) -> Result<Self, IdentityError> {
        let namespace = namespace.into();
        let kind = kind.into();
        let id = id.into();
        if is_blank(&namespace) || is_blank(&kind) || is_blank(&id) {
            return Err(IdentityError::EmptyComponent);
        }
        Ok(Self {
            namespace,
            kind,
            id,
            id_kind,
        })
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn id_kind(&self) -> ConstructIdKind {
        self.id_kind
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConstructKeyWire {
    namespace: String,
    kind: String,
    id: String,
    id_kind: ConstructIdKind,
}

impl<'de> Deserialize<'de> for ConstructKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ConstructKeyWire::deserialize(deserializer)?;
        Self::new(wire.namespace, wire.kind, wire.id, wire.id_kind)
            .map_err(serde::de::Error::custom)
    }
}

/// A stable identity for one aggregate observation of a construct.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ObservationKey {
    construct: ConstructKey,
    operation: String,
    metric: String,
    definition_version: u32,
    aggregation: String,
}

impl ObservationKey {
    /// Build an observation identity with a positive definition version.
    pub fn new(
        construct: ConstructKey,
        operation: impl Into<String>,
        metric: impl Into<String>,
        definition_version: u32,
        aggregation: impl Into<String>,
    ) -> Result<Self, IdentityError> {
        let operation = operation.into();
        let metric = metric.into();
        let aggregation = aggregation.into();
        if is_blank(&operation) || is_blank(&metric) || is_blank(&aggregation) {
            return Err(IdentityError::EmptyComponent);
        }
        if definition_version == 0 {
            return Err(IdentityError::InvalidDefinitionVersion);
        }
        Ok(Self {
            construct,
            operation,
            metric,
            definition_version,
            aggregation,
        })
    }

    pub fn construct(&self) -> &ConstructKey {
        &self.construct
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn metric(&self) -> &str {
        &self.metric
    }

    pub fn definition_version(&self) -> u32 {
        self.definition_version
    }

    pub fn aggregation(&self) -> &str {
        &self.aggregation
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationKeyWire {
    construct: ConstructKey,
    operation: String,
    metric: String,
    definition_version: u32,
    aggregation: String,
}

impl<'de> Deserialize<'de> for ObservationKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ObservationKeyWire::deserialize(deserializer)?;
        Self::new(
            wire.construct,
            wire.operation,
            wire.metric,
            wire.definition_version,
            wire.aggregation,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// Provenance shared by health, profiling, and assessment evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceContext {
    #[serde(default = "default_context_version")]
    pub context_version: u32,
    pub lang_project_guid: Option<String>,
    pub source_sha256: Option<String>,
    pub model_fingerprint: Option<String>,
    pub importer_version: Option<String>,
    pub compiler_version: Option<String>,
    pub pipeline: Option<String>,
    pub backend: Option<String>,
    pub profile: Option<String>,
    pub config_digest: Option<String>,
}

impl Default for EvidenceContext {
    fn default() -> Self {
        Self {
            context_version: EVIDENCE_CONTEXT_VERSION,
            lang_project_guid: None,
            source_sha256: None,
            model_fingerprint: None,
            importer_version: None,
            compiler_version: None,
            pipeline: None,
            backend: None,
            profile: None,
            config_digest: None,
        }
    }
}

fn default_context_version() -> u32 {
    EVIDENCE_CONTEXT_VERSION
}

fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}
