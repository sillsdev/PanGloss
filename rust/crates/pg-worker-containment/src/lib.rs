#![deny(unsafe_op_in_unsafe_fn)]
#![doc = "OS-enforced process-tree containment for selected compile workers."]

use std::cmp::Ordering;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(all(not(windows), not(target_os = "linux")))]
mod unsupported;

/// Whether `process_rss_bytes` can measure anything on this target; a caller enforcing a memory limit must refuse when false.
pub const PROCESS_RSS_SUPPORTED: bool = cfg!(any(windows, target_os = "linux"));

/// Resident set size of `pid` in bytes, or None when it cannot be read (process gone, access denied, unsupported OS).
pub fn process_rss_bytes(pid: u32) -> Option<u64> {
    #[cfg(windows)]
    {
        windows::process_rss_bytes(pid)
    }
    #[cfg(target_os = "linux")]
    {
        linux::process_rss_bytes(pid)
    }
    #[cfg(all(not(windows), not(target_os = "linux")))]
    {
        unsupported::process_rss_bytes(pid)
    }
}

/// Resident set size of the current process in bytes, or None when it cannot be read.
pub fn current_process_rss_bytes() -> Option<u64> {
    process_rss_bytes(std::process::id())
}

/// Finite limits for one worker attempt. The serialized-payload limit is carried here so the
/// containment seam remains the single caller-facing execution configuration; the Windows
/// adapter enforces the committed-memory and lifecycle dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionLimits {
    max_serialized_fst_bytes: u64,
    max_committed_memory_bytes: u64,
    max_wall_time: Duration,
}

impl ExecutionLimits {
    /// Creates a limit set. Every dimension is required to be non-zero.
    pub const fn try_new(
        max_serialized_fst_bytes: u64,
        max_committed_memory_bytes: u64,
        max_wall_time: Duration,
    ) -> Result<Self, ExecutionLimitError> {
        if max_serialized_fst_bytes == 0 {
            return Err(ExecutionLimitError::ZeroSerializedFstBytes);
        }
        if max_committed_memory_bytes == 0 {
            return Err(ExecutionLimitError::ZeroCommittedMemoryBytes);
        }
        if max_wall_time.is_zero() {
            return Err(ExecutionLimitError::ZeroWallTime);
        }
        Ok(Self {
            max_serialized_fst_bytes,
            max_committed_memory_bytes,
            max_wall_time,
        })
    }

    pub const fn max_serialized_fst_bytes(self) -> u64 {
        self.max_serialized_fst_bytes
    }

    pub const fn max_committed_memory_bytes(self) -> u64 {
        self.max_committed_memory_bytes
    }

    pub const fn max_wall_time(self) -> Duration {
        self.max_wall_time
    }

    /// Child-side limits for a selected payload, preserving the supervisor's configured
    /// serialized-payload authority while using the ratified external defaults for other axes.
    pub fn for_selected_payload(
        max_serialized_fst_bytes: u64,
    ) -> Result<Self, ExecutionLimitError> {
        Self::try_new(
            max_serialized_fst_bytes,
            Self::default().max_committed_memory_bytes,
            Self::default().max_wall_time,
        )
    }
}

/// Ratified finite defaults: 1 GiB serialized payload, 10 GiB committed memory, and 10 minutes.
pub const DEFAULT_EXECUTION_LIMITS: ExecutionLimits = ExecutionLimits {
    max_serialized_fst_bytes: 1024 * 1024 * 1024,
    max_committed_memory_bytes: 10 * 1024 * 1024 * 1024,
    max_wall_time: Duration::from_secs(10 * 60),
};

impl Default for ExecutionLimits {
    fn default() -> Self {
        DEFAULT_EXECUTION_LIMITS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionLimitError {
    ZeroSerializedFstBytes,
    ZeroCommittedMemoryBytes,
    ZeroWallTime,
}

impl std::fmt::Display for ExecutionLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let field = match self {
            Self::ZeroSerializedFstBytes => "max_serialized_fst_bytes",
            Self::ZeroCommittedMemoryBytes => "max_committed_memory_bytes",
            Self::ZeroWallTime => "max_wall_time",
        };
        write!(f, "{field} must be positive")
    }
}

impl std::error::Error for ExecutionLimitError {}

/// Native proof that the aggregate worker-tree memory boundary fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryLimitEvidence {
    WindowsObservedJobMemoryLimitViolation {
        notification_limit_bytes: u64,
        peak_job_memory_used_bytes: u64,
    },
    LinuxCgroupV2MemoryLimitViolation {
        effective_memory_max_bytes: u64,
        memory_peak_bytes: u64,
        oom_kill_count_delta: std::num::NonZeroU64,
        max_event_count_delta: std::num::NonZeroU64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildTermination {
    Exited(u32),
    Signaled(i32),
}

impl MemoryLimitEvidence {
    pub const fn peak_memory_charge_bytes(self) -> u64 {
        match self {
            Self::WindowsObservedJobMemoryLimitViolation {
                peak_job_memory_used_bytes,
                ..
            } => peak_job_memory_used_bytes,
            Self::LinuxCgroupV2MemoryLimitViolation {
                memory_peak_bytes, ..
            } => memory_peak_bytes,
        }
    }
}

/// Final native evidence captured while the job handle is still owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinalEvidence {
    pub memory_limit: Option<MemoryLimitEvidence>,
    pub peak_memory_charge_bytes: u64,
}

/// Launch options preserving ordinary Windows environment and current-directory semantics.
#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    environment: Vec<(OsString, Option<OsString>)>,
    clear_environment: bool,
    current_dir: Option<PathBuf>,
}

impl LaunchOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Override an inherited variable, or add it when no inherited value exists.
    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.set_environment(key.into(), Some(value.into()));
        self
    }

    /// Remove a variable from the inherited environment.
    pub fn env_remove(mut self, key: impl Into<OsString>) -> Self {
        self.set_environment(key.into(), None);
        self
    }

    /// Start with an empty environment, then apply subsequent [`Self::env`] calls.
    pub fn env_clear(mut self) -> Self {
        self.clear_environment = true;
        self
    }

    pub fn current_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(path.into());
        self
    }

    pub(crate) fn environment(&self) -> &[(OsString, Option<OsString>)] {
        &self.environment
    }

    pub(crate) fn clear_environment(&self) -> bool {
        self.clear_environment
    }

    pub(crate) fn current_dir_path(&self) -> Option<&Path> {
        self.current_dir.as_deref()
    }

    fn set_environment(&mut self, key: OsString, value: Option<OsString>) {
        if let Some((_, old_value)) = self
            .environment
            .iter_mut()
            .find(|(old_key, _)| compare_environment_keys(old_key, &key) == Ordering::Equal)
        {
            *old_value = value;
        } else {
            self.environment.push((key, value));
        }
    }
}

#[cfg(windows)]
fn compare_environment_keys(left: &std::ffi::OsStr, right: &std::ffi::OsStr) -> Ordering {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Globalization::{CompareStringOrdinal, CSTR_EQUAL, CSTR_GREATER_THAN};

    let left = left.encode_wide().collect::<Vec<_>>();
    let right = right.encode_wide().collect::<Vec<_>>();
    // SAFETY: both pointers and their exact UTF-16 lengths remain live for this synchronous call.
    let result = unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            left.len()
                .try_into()
                .expect("environment key length fits i32"),
            right.as_ptr(),
            right
                .len()
                .try_into()
                .expect("environment key length fits i32"),
            1,
        )
    };
    match result {
        CSTR_EQUAL => Ordering::Equal,
        CSTR_GREATER_THAN => Ordering::Greater,
        _ => Ordering::Less,
    }
}

#[cfg(target_os = "linux")]
fn compare_environment_keys(left: &std::ffi::OsStr, right: &std::ffi::OsStr) -> Ordering {
    use std::os::unix::ffi::OsStrExt;

    left.as_bytes().cmp(right.as_bytes())
}

#[cfg(all(not(windows), not(target_os = "linux")))]
fn compare_environment_keys(left: &std::ffi::OsStr, right: &std::ffi::OsStr) -> Ordering {
    left.to_string_lossy()
        .to_ascii_uppercase()
        .cmp(&right.to_string_lossy().to_ascii_uppercase())
}

/// The three parent-owned pipe endpoints. They are ordinary files so callers can use the
/// standard `Read`/`Write` implementations without seeing native handles.
pub struct ContainedStdio {
    pub stdin: std::fs::File,
    pub stdout: std::fs::File,
    pub stderr: std::fs::File,
}

impl std::fmt::Debug for ContainedStdio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainedStdio").finish_non_exhaustive()
    }
}

/// Direct-child status with the process identity retained for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectChildExit {
    pub process_id: u32,
    pub termination: ChildTermination,
}

impl DirectChildExit {
    pub const fn success(self) -> bool {
        matches!(self.termination, ChildTermination::Exited(0))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainmentError {
    Unavailable { detail: String },
    Failed { detail: String },
    DeadlineExceeded { operation: &'static str },
}

impl std::fmt::Display for ContainmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable { detail } => write!(f, "containment unavailable: {detail}"),
            Self::Failed { detail } => write!(f, "containment failed: {detail}"),
            Self::DeadlineExceeded { operation } => write!(f, "{operation} exceeded its deadline"),
        }
    }
}

impl std::error::Error for ContainmentError {}

pub type SpawnError = ContainmentError;

/// A worker process whose complete descendant tree is owned by the platform containment object.
///
/// The concrete implementation is target-specific; no native handle is exposed. Callers must
/// retain this value until direct status, tree-drain status, and final evidence are captured.
pub struct ContainedWorkerProcess {
    #[cfg(windows)]
    inner: windows::ContainedWorkerProcess,
    #[cfg(target_os = "linux")]
    inner: linux::ContainedWorkerProcess,
    #[cfg(all(not(windows), not(target_os = "linux")))]
    inner: unsupported::ContainedWorkerProcess,
}

impl ContainedWorkerProcess {
    /// Launches exactly the executable at `executable`; this API intentionally performs no
    /// `PATH` search.
    pub fn spawn(
        executable: &Path,
        args: &[OsString],
        options: &LaunchOptions,
        limits: ExecutionLimits,
    ) -> Result<Self, SpawnError> {
        Ok(Self {
            inner: platform_spawn(executable, args, options, limits)?,
        })
    }

    pub fn take_stdio(&mut self) -> Option<ContainedStdio> {
        self.inner.take_stdio()
    }

    pub fn try_wait_direct_child(
        &mut self,
        deadline: Instant,
    ) -> Result<Option<DirectChildExit>, ContainmentError> {
        self.inner.try_wait_direct_child(deadline)
    }

    pub fn poll_containment(
        &mut self,
        deadline: Instant,
    ) -> Result<Option<MemoryLimitEvidence>, ContainmentError> {
        self.inner.poll_containment(deadline)
    }

    pub fn terminate_tree(&mut self, deadline: Instant) -> Result<(), ContainmentError> {
        self.inner.terminate_tree(deadline)
    }

    pub fn wait_tree_empty(&mut self, deadline: Instant) -> Result<(), ContainmentError> {
        self.inner.wait_tree_empty(deadline)
    }

    pub fn reap_direct_child(
        &mut self,
        deadline: Instant,
    ) -> Result<DirectChildExit, ContainmentError> {
        self.inner.reap_direct_child(deadline)
    }

    pub fn final_evidence_and_peak(
        &mut self,
        deadline: Instant,
    ) -> Result<FinalEvidence, ContainmentError> {
        self.inner.final_evidence_and_peak(deadline)
    }
}

#[cfg(windows)]
fn platform_spawn(
    executable: &Path,
    args: &[OsString],
    options: &LaunchOptions,
    limits: ExecutionLimits,
) -> Result<windows::ContainedWorkerProcess, SpawnError> {
    windows::ContainedWorkerProcess::spawn(executable, args, options, limits)
}

#[cfg(target_os = "linux")]
fn platform_spawn(
    executable: &Path,
    args: &[OsString],
    options: &LaunchOptions,
    limits: ExecutionLimits,
) -> Result<linux::ContainedWorkerProcess, SpawnError> {
    linux::ContainedWorkerProcess::spawn(executable, args, options, limits)
}

#[cfg(all(not(windows), not(target_os = "linux")))]
fn platform_spawn(
    executable: &Path,
    args: &[OsString],
    options: &LaunchOptions,
    limits: ExecutionLimits,
) -> Result<unsupported::ContainedWorkerProcess, SpawnError> {
    unsupported::ContainedWorkerProcess::spawn(executable, args, options, limits)
}

#[cfg(test)]
mod tests;
