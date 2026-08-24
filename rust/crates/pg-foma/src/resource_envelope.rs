//! Closed, versioned resource profiles for one compile attempt.
//!
//! The product surface deliberately names profiles rather than accepting a raw closure
//! counter.  The snapshot is a value: once selected, no process environment is consulted.

use std::fmt;
use std::str::FromStr;

use crate::characterization::{CharacterizationResult, ClosureTerminal};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceEnvelopeId {
    ManagedV1,
    TunedSurfaceWork10kV1,
}

/// Construction-cap policy for one developer or managed compile attempt.
///
/// This is deliberately separate from [`ResourceEnvelopeId`]: the envelope remains the shipped
/// identity for watchdog, transport, and completion evidence while this policy only projects the
/// internal deterministic construction caps used by the emitter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompileSizeMode {
    Managed,
    #[cfg(feature = "developer-tools")]
    DeveloperStress,
}

impl Default for CompileSizeMode {
    fn default() -> Self {
        Self::Managed
    }
}

impl ResourceEnvelopeId {
    pub const fn all() -> &'static [Self] {
        &[Self::ManagedV1, Self::TunedSurfaceWork10kV1]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ManagedV1 => "managed-v1",
            Self::TunedSurfaceWork10kV1 => "tuned-surface-work-10k-v1",
        }
    }
}

impl fmt::Display for ResourceEnvelopeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ResourceEnvelopeId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "managed-v1" => Ok(Self::ManagedV1),
            "tuned-surface-work-10k-v1" => Ok(Self::TunedSurfaceWork10kV1),
            _ => Err(format!("unknown resource envelope id {value:?}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchdogEnvelope {
    pub wall_timeout_ms: u64,
    pub rss_limit_mb: u64,
    pub rss_sample_interval_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommunicationEnvelope {
    pub max_request_bytes: u64,
    pub max_result_bytes: u64,
    pub max_captured_stderr_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposeEnvelope {
    pub state_cap: usize,
    pub arc_cap: usize,
    pub tuple_cap: usize,
    pub group_cap: usize,
    pub line_cap: usize,
    pub compound_pair_cap: usize,
    pub chain_depth_cap: Option<usize>,
    pub ordering_multiplicity_cap: Option<usize>,
}

impl ComposeEnvelope {
    /// Projects this envelope's caps onto a `ComposeBudget`, the deterministic compile-time compose limit.
    pub(crate) fn to_compose_budget(self) -> crate::compose_budget::ComposeBudget {
        let mut budget = crate::compose_budget::ComposeBudget::with_caps(
            self.state_cap,
            self.arc_cap,
            self.tuple_cap,
            self.group_cap,
            self.line_cap,
            None,
        );
        if let Some(cap) = self.chain_depth_cap {
            budget = budget.with_chain_depth_cap(cap);
        }
        if let Some(cap) = self.ordering_multiplicity_cap {
            budget = budget.with_ordering_multiplicity_cap(cap);
        }
        budget
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumerationEnvelope {
    pub composite_entry_cap: usize,
    pub pair_probe_cap: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendEnvelope {
    pub tuned_surface_closure_work_cap: usize,
    pub tuned_surface_closure_depth_cap: usize,
    pub tuned_surface_compound_chain_depth_cap: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ResourceEnvelope {
    schema_version: u32,
    id: ResourceEnvelopeId,
    worker_protocol_version: u32,
    watchdog: WatchdogEnvelope,
    communication: CommunicationEnvelope,
    compose: ComposeEnvelope,
    enumeration: EnumerationEnvelope,
    backend: BackendEnvelope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompileLimits {
    pub compose: ComposeEnvelope,
    pub enumeration: EnumerationEnvelope,
    pub backend: BackendEnvelope,
}

impl ResourceEnvelope {
    pub const fn id(&self) -> ResourceEnvelopeId {
        self.id
    }
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }
    pub const fn worker_protocol_version(&self) -> u32 {
        self.worker_protocol_version
    }
    pub const fn watchdog(&self) -> WatchdogEnvelope {
        self.watchdog
    }
    pub const fn communication(&self) -> CommunicationEnvelope {
        self.communication
    }
    pub const fn compose(&self) -> ComposeEnvelope {
        self.compose
    }
    pub const fn enumeration(&self) -> EnumerationEnvelope {
        self.enumeration
    }
    pub const fn backend(&self) -> BackendEnvelope {
        self.backend
    }

    /// Projects the internal construction limits for one attempt without changing the shipped
    /// envelope identity or any external containment limit.
    pub const fn compile_limits(&self, mode: CompileSizeMode) -> CompileLimits {
        match mode {
            CompileSizeMode::Managed => CompileLimits {
                compose: self.compose,
                enumeration: self.enumeration,
                backend: self.backend,
            },
            #[cfg(feature = "developer-tools")]
            CompileSizeMode::DeveloperStress => CompileLimits {
                compose: ComposeEnvelope {
                    state_cap: usize::MAX,
                    arc_cap: usize::MAX,
                    tuple_cap: usize::MAX,
                    group_cap: usize::MAX,
                    line_cap: usize::MAX,
                    compound_pair_cap: usize::MAX,
                    chain_depth_cap: Some(crate::compose_budget::CHAIN_DEPTH_ABSOLUTE_CEILING),
                    ordering_multiplicity_cap: None,
                },
                enumeration: EnumerationEnvelope {
                    composite_entry_cap: usize::MAX,
                    pair_probe_cap: usize::MAX,
                },
                backend: BackendEnvelope {
                    tuned_surface_closure_work_cap: usize::MAX,
                    tuned_surface_closure_depth_cap:
                        crate::compose_budget::CHAIN_DEPTH_ABSOLUTE_CEILING,
                    tuned_surface_compound_chain_depth_cap:
                        crate::compose_budget::CHAIN_DEPTH_ABSOLUTE_CEILING,
                },
            },
        }
    }
}

fn shipped_watchdog() -> WatchdogEnvelope {
    WatchdogEnvelope {
        wall_timeout_ms: crate::worker_contract::DEFAULT_WALL_TIMEOUT_MS,
        rss_limit_mb: crate::worker_contract::DEFAULT_RSS_LIMIT_MB,
        rss_sample_interval_ms: crate::worker_contract::DEFAULT_RSS_SAMPLE_INTERVAL_MS,
    }
}

fn shipped_communication() -> CommunicationEnvelope {
    CommunicationEnvelope {
        max_request_bytes: crate::worker_contract::V1_LIMITS.max_request_bytes,
        max_result_bytes: crate::worker_contract::V1_LIMITS.max_result_bytes,
        max_captured_stderr_bytes: crate::worker_contract::V1_LIMITS.max_captured_stderr_bytes,
    }
}

const fn shipped_worker_protocol_version() -> u32 {
    crate::worker_contract::PROTOCOL_VERSION
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceEnvelopeWire {
    schema_version: u32,
    id: ResourceEnvelopeId,
    worker_protocol_version: u32,
    watchdog: WatchdogEnvelope,
    communication: CommunicationEnvelope,
    compose: ComposeEnvelope,
    enumeration: EnumerationEnvelope,
    backend: BackendEnvelope,
}

impl<'de> Deserialize<'de> for ResourceEnvelope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ResourceEnvelopeWire::deserialize(deserializer)?;
        let expected = Self::for_id(wire.id);
        let actual = Self {
            schema_version: wire.schema_version,
            id: wire.id,
            worker_protocol_version: wire.worker_protocol_version,
            watchdog: wire.watchdog,
            communication: wire.communication,
            compose: wire.compose,
            enumeration: wire.enumeration,
            backend: wire.backend,
        };
        if actual != expected {
            return Err(D::Error::custom(
                "resource envelope is not an exact shipped profile",
            ));
        }
        Ok(actual)
    }
}

impl ResourceEnvelope {
    pub fn for_id(id: ResourceEnvelopeId) -> Self {
        let compose = ComposeEnvelope {
            state_cap: crate::compose_budget::DEFAULT_STATE_BUDGET,
            arc_cap: crate::compose_budget::DEFAULT_ARC_BUDGET,
            tuple_cap: crate::compose_budget::DEFAULT_TUPLE_BUDGET,
            group_cap: crate::compose_budget::DEFAULT_GROUP_BUDGET,
            line_cap: crate::compose_budget::DEFAULT_LINE_BUDGET,
            compound_pair_cap: crate::compose_budget::DEFAULT_COMPOUND_PAIR_BUDGET,
            chain_depth_cap: None,
            ordering_multiplicity_cap: Some(
                crate::compose_budget::DEFAULT_ORDERING_MULTIPLICITY_BUDGET,
            ),
        };
        Self {
            schema_version: 1,
            id,
            worker_protocol_version: shipped_worker_protocol_version(),
            watchdog: shipped_watchdog(),
            communication: shipped_communication(),
            compose,
            enumeration: EnumerationEnvelope {
                composite_entry_cap: crate::morphotactics::DEFAULT_ENTRY_BUDGET,
                pair_probe_cap: crate::morphotactics::DEFAULT_PROBE_BUDGET,
            },
            backend: BackendEnvelope {
                tuned_surface_closure_work_cap: match id {
                    ResourceEnvelopeId::ManagedV1 => 3_000,
                    ResourceEnvelopeId::TunedSurfaceWork10kV1 => 10_000,
                },
                tuned_surface_closure_depth_cap: 64,
                tuned_surface_compound_chain_depth_cap: 200,
            },
        }
    }

    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("ResourceEnvelope serialization is infallible")
    }

    pub fn digest(&self) -> String {
        format!("{:x}", Sha256::digest(self.canonical_json().as_bytes()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct AttemptId(String);

impl<'de> Deserialize<'de> for AttemptId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl AttemptId {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("attempt id must not be empty".to_string())
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileEnvelopeRequest {
    envelope_id: ResourceEnvelopeId,
    size_mode: CompileSizeMode,
    attempt_id: AttemptId,
    retry_of: Option<AttemptId>,
    prior_closure: Option<CharacterizationResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryAuthorization {
    attempt_id: AttemptId,
    envelope_id: ResourceEnvelopeId,
    prior_closure: CharacterizationResult,
}

fn fresh_attempt_id() -> Result<AttemptId, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| format!("could not create attempt id: {error}"))?;
    let mut encoded = String::with_capacity("attempt-".len() + bytes.len() * 2);
    encoded.push_str("attempt-");
    for byte in bytes {
        encoded.push_str(&format!("{byte:02x}"));
    }
    AttemptId::new(encoded)
}

impl CompileEnvelopeRequest {
    pub fn try_new(envelope_id: ResourceEnvelopeId) -> Result<Self, String> {
        Self::try_new_with_mode(envelope_id, CompileSizeMode::Managed)
    }

    pub fn try_new_with_mode(
        envelope_id: ResourceEnvelopeId,
        size_mode: CompileSizeMode,
    ) -> Result<Self, String> {
        Ok(Self {
            envelope_id,
            size_mode,
            attempt_id: fresh_attempt_id()?,
            retry_of: None,
            prior_closure: None,
        })
    }

    pub fn retry_from(
        authorization: &RetryAuthorization,
        envelope_id: ResourceEnvelopeId,
    ) -> Result<Self, String> {
        if authorization.envelope_id == envelope_id {
            return Err("a retry must select a different resource envelope".to_string());
        }
        if matches!(
            authorization.prior_closure.terminal,
            ClosureTerminal::Complete
        ) {
            return Err("a complete attempt cannot authorize a retry".to_string());
        }
        Ok(Self {
            envelope_id,
            size_mode: CompileSizeMode::Managed,
            attempt_id: fresh_attempt_id()?,
            retry_of: Some(authorization.attempt_id.clone()),
            prior_closure: Some(authorization.prior_closure.clone()),
        })
    }

    pub const fn envelope_id(&self) -> ResourceEnvelopeId {
        self.envelope_id
    }

    pub const fn size_mode(&self) -> CompileSizeMode {
        self.size_mode
    }

    pub fn attempt_id(&self) -> &AttemptId {
        &self.attempt_id
    }

    pub fn retry_of(&self) -> Option<&AttemptId> {
        self.retry_of.as_ref()
    }

    pub fn prior_closure(&self) -> Option<&CharacterizationResult> {
        self.prior_closure.as_ref()
    }

    pub(crate) fn from_worker_wire(
        envelope_id: ResourceEnvelopeId,
        envelope_digest: &str,
        attempt_id: impl Into<String>,
        size_mode: CompileSizeMode,
    ) -> Result<Self, String> {
        let expected = ResourceEnvelope::for_id(envelope_id);
        if envelope_digest != expected.digest() {
            return Err(format!(
                "resource envelope digest does not match shipped {envelope_id}"
            ));
        }
        Ok(Self {
            envelope_id,
            size_mode,
            attempt_id: AttemptId::new(attempt_id)?,
            retry_of: None,
            prior_closure: None,
        })
    }
}

impl RetryAuthorization {
    pub(crate) fn from_terminal_failure(
        request: &CompileEnvelopeRequest,
        closure: &CharacterizationResult,
    ) -> Option<Self> {
        let retryable = matches!(
            closure.terminal,
            ClosureTerminal::Incomplete(
                crate::characterization::ClosureStopReason::WorkBudgetReached
                    | crate::characterization::ClosureStopReason::DepthBudgetReached
                    | crate::characterization::ClosureStopReason::EnumerationBudgetReached
                    | crate::characterization::ClosureStopReason::ResourceBudgetReached
            )
        );
        if retryable {
            Some(Self {
                attempt_id: request.attempt_id.clone(),
                envelope_id: request.envelope_id,
                prior_closure: closure.clone(),
            })
        } else {
            None
        }
    }
}
