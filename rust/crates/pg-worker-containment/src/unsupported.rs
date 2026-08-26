use crate::{
    ContainedStdio, ContainmentError, DirectChildExit, ExecutionLimits, FinalEvidence,
    LaunchOptions, MemoryLimitEvidence,
};
use std::ffi::OsString;
use std::path::Path;
use std::time::Instant;

/// Fails closed on targets whose containment adapter is unavailable.
pub(crate) struct ContainedWorkerProcess;

impl ContainedWorkerProcess {
    pub(crate) fn spawn(
        _executable: &Path,
        _args: &[OsString],
        _options: &LaunchOptions,
        _limits: ExecutionLimits,
    ) -> Result<Self, ContainmentError> {
        Err(ContainmentError::Unavailable {
            detail: "no platform containment adapter is available for this target".to_string(),
        })
    }

    pub(crate) fn take_stdio(&mut self) -> Option<ContainedStdio> {
        None
    }

    pub(crate) fn try_wait_direct_child(
        &mut self,
    ) -> Result<Option<DirectChildExit>, ContainmentError> {
        Err(unavailable())
    }

    pub(crate) fn poll_containment(
        &mut self,
    ) -> Result<Option<MemoryLimitEvidence>, ContainmentError> {
        Err(unavailable())
    }

    pub(crate) fn terminate_tree(&mut self, _deadline: Instant) -> Result<(), ContainmentError> {
        Err(unavailable())
    }

    pub(crate) fn wait_tree_empty(&mut self, _deadline: Instant) -> Result<(), ContainmentError> {
        Err(unavailable())
    }

    pub(crate) fn reap_direct_child(
        &mut self,
        _deadline: Instant,
    ) -> Result<DirectChildExit, ContainmentError> {
        Err(unavailable())
    }

    pub(crate) fn final_evidence_and_peak(&mut self) -> Result<FinalEvidence, ContainmentError> {
        Err(unavailable())
    }
}

fn unavailable() -> ContainmentError {
    ContainmentError::Unavailable {
        detail: "no platform containment adapter is available for this target".to_string(),
    }
}
