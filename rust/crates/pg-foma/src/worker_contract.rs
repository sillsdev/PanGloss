//! Always-built worker wire/default constants shared by the native worker and resource profiles.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkerProtocolLimits {
    pub(crate) max_request_bytes: u64,
    pub(crate) max_result_bytes: u64,
    pub(crate) max_captured_stderr_bytes: u64,
}

pub(crate) const PROTOCOL_VERSION: u32 = 9;

pub(crate) const PROTOCOL_LIMITS: WorkerProtocolLimits = WorkerProtocolLimits {
    max_request_bytes: 4 * 1024 * 1024,
    max_result_bytes: 16 * 1024 * 1024,
    max_captured_stderr_bytes: 4 * 1024 * 1024,
};
