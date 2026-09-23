use super::*;
#[cfg(target_os = "linux")]
use std::ffi::OsStr;

#[test]
fn limit_configuration_rejects_zero_dimensions() {
    assert_eq!(
        ExecutionLimits::try_new(0, 1, Duration::from_secs(1)),
        Err(ExecutionLimitError::ZeroSerializedFstBytes)
    );
    assert_eq!(
        ExecutionLimits::try_new(1, 0, Duration::from_secs(1)),
        Err(ExecutionLimitError::ZeroCommittedMemoryBytes)
    );
    assert_eq!(
        ExecutionLimits::try_new(1, 1, Duration::ZERO),
        Err(ExecutionLimitError::ZeroWallTime)
    );
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn current_process_rss_is_positive() {
    let rss = current_process_rss_bytes();
    assert!(
        matches!(rss, Some(bytes) if bytes > 0),
        "current process RSS: {rss:?}"
    );
}

#[test]
fn nonexistent_process_rss_is_none() {
    assert_eq!(process_rss_bytes(u32::MAX), None);
}

#[cfg(windows)]
#[test]
fn environment_overrides_are_case_insensitive() {
    let options = LaunchOptions::new().env("Path", "one").env("PATH", "two");
    assert_eq!(options.environment().len(), 1);
    assert_eq!(options.environment()[0].1, Some(OsString::from("two")));
}

#[cfg(target_os = "linux")]
#[test]
fn environment_overrides_preserve_case_distinct_keys_on_linux() {
    let options = LaunchOptions::new().env("Path", "one").env("PATH", "two");
    assert_eq!(options.environment().len(), 2);
    assert_eq!(
        options
            .environment()
            .iter()
            .find(|(key, _)| key == "Path")
            .and_then(|(_, value)| value.as_deref()),
        Some(OsStr::new("one"))
    );
    assert_eq!(
        options
            .environment()
            .iter()
            .find(|(key, _)| key == "PATH")
            .and_then(|(_, value)| value.as_deref()),
        Some(OsStr::new("two"))
    );
}

#[cfg(windows)]
#[test]
fn environment_overrides_use_windows_unicode_case_mapping() {
    let options = LaunchOptions::new().env("PÄTH", "one").env("päth", "two");
    assert_eq!(options.environment().len(), 1);
    assert_eq!(options.environment()[0].1, Some(OsString::from("two")));
}
