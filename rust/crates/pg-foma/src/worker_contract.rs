//! Always-built worker wire/default constants shared by the native worker and resource profiles.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkerProtocolLimits {
    pub(crate) max_request_bytes: u64,
    pub(crate) max_result_bytes: u64,
    pub(crate) max_captured_stderr_bytes: u64,
    pub(crate) max_wall_timeout_ms: u64,
    pub(crate) max_rss_limit_mb: u64,
    pub(crate) min_rss_sample_interval_ms: u64,
}

pub(crate) const PROTOCOL_VERSION: u32 = 2;

pub(crate) const PROTOCOL_LIMITS: WorkerProtocolLimits = WorkerProtocolLimits {
    max_request_bytes: 4 * 1024 * 1024,
    max_result_bytes: 16 * 1024 * 1024,
    max_captured_stderr_bytes: 4 * 1024 * 1024,
    max_wall_timeout_ms: 24 * 60 * 60 * 1000,
    max_rss_limit_mb: 256 * 1024,
    min_rss_sample_interval_ms: 10,
};

pub(crate) const DEFAULT_WALL_TIMEOUT_MS: u64 = 120_000;
pub(crate) const DEFAULT_RSS_LIMIT_MB: u64 = 4_096;
pub(crate) const DEFAULT_RSS_SAMPLE_INTERVAL_MS: u64 = 200;
