//! Closed, versioned resource profiles for one compile attempt.
//!
//! The product surface deliberately names profiles rather than accepting a raw closure
//! counter.  The snapshot is a value: once selected, no process environment is consulted.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceEnvelopeId {
    ManagedV1,
    TunedSurfaceWork10kV1,
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
pub struct WatchdogEnvelope {
    pub wall_timeout_ms: u64,
    pub rss_limit_mb: u64,
    pub rss_sample_interval_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunicationEnvelope {
    pub max_request_bytes: u64,
    pub max_result_bytes: u64,
    pub max_captured_stderr_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumerationEnvelope {
    pub composite_entry_cap: usize,
    pub pair_probe_cap: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendEnvelope {
    pub tuned_surface_closure_work_cap: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceEnvelope {
    pub schema_version: u32,
    pub id: ResourceEnvelopeId,
    pub worker_protocol_version: u32,
    pub watchdog: WatchdogEnvelope,
    pub communication: CommunicationEnvelope,
    pub compose: ComposeEnvelope,
    pub enumeration: EnumerationEnvelope,
    pub backend: BackendEnvelope,
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
            // Protocol v1 is part of the envelope schema and remains available to wasm builds,
            // where the native worker module is intentionally absent.
            worker_protocol_version: 1,
            watchdog: WatchdogEnvelope {
                wall_timeout_ms: 120_000,
                rss_limit_mb: 4_096,
                rss_sample_interval_ms: 200,
            },
            communication: CommunicationEnvelope {
                max_request_bytes: 4 * 1024 * 1024,
                max_result_bytes: 16 * 1024 * 1024,
                max_captured_stderr_bytes: 4 * 1024 * 1024,
            },
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttemptId(String);

impl AttemptId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompileEnvelopeRequest {
    pub envelope_id: ResourceEnvelopeId,
    pub retry_of: Option<AttemptId>,
}

impl Default for CompileEnvelopeRequest {
    fn default() -> Self {
        Self {
            envelope_id: ResourceEnvelopeId::ManagedV1,
            retry_of: None,
        }
    }
}

impl CompileEnvelopeRequest {
    pub fn explicit_retry(prior: AttemptId, envelope_id: ResourceEnvelopeId) -> Self {
        Self {
            envelope_id,
            retry_of: Some(prior),
        }
    }

    pub fn attempt_count(&self) -> usize {
        1
    }
}
