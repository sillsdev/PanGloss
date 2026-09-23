use super::{
    decode_wait_status, parse_cgroup_event_value, parse_memory_events, select_cgroup2_mount,
};
use std::path::Path;

#[test]
fn cgroup_event_parser_accepts_numeric_populated_zero() {
    assert_eq!(
        parse_cgroup_event_value("populated 0\nfrozen 0\n", "populated"),
        Ok(0)
    );
}

#[test]
fn cgroup_event_parser_rejects_malformed_record() {
    assert!(parse_cgroup_event_value("populated\n", "populated").is_err());
    assert!(parse_cgroup_event_value("populated 0 extra\n", "populated").is_err());
    assert!(parse_cgroup_event_value("populated nope\n", "populated").is_err());
    assert!(parse_cgroup_event_value("populated 0\nfrozen nope\n", "populated").is_err());
}

#[test]
fn cgroup_event_parser_rejects_duplicate_requested_record() {
    assert!(parse_cgroup_event_value("populated 0\npopulated 1\n", "populated").is_err());
}

#[test]
fn cgroup_event_parser_rejects_missing_requested_record() {
    assert!(parse_cgroup_event_value("frozen 0\n", "populated").is_err());
}

#[test]
fn wait_status_parser_rejects_non_terminal_status() {
    // 0x7f is WIFSTOPPED, the actual non-terminal encoding; 0x37 is a signalled exit, which decodes fine.
    assert!(decode_wait_status(0x7f).is_err());
    assert!(decode_wait_status(0x37).is_ok());
}

#[test]
fn memory_events_parser_rejects_duplicate_required_records() {
    for text in [
        "max 1\nmax 2\noom_kill 1\n",
        "max 1\noom_kill 1\noom_kill 2\n",
    ] {
        assert!(parse_memory_events(text).is_err(), "accepted: {text:?}");
    }
}

#[test]
fn memory_events_parser_rejects_truncated_malformed_and_overflowing_required_values() {
    for text in [
        "max\noom_kill 1\n",
        "max 1 trailing\noom_kill 1\n",
        "max nope\noom_kill 1\n",
        "max 1\noom_kill 18446744073709551616\n",
    ] {
        assert!(parse_memory_events(text).is_err(), "accepted: {text:?}");
    }
}

#[test]
fn memory_events_parser_rejects_malformed_and_overflowing_unknown_values() {
    for text in [
        "max 1\noom_kill 1\nunknown nope\n",
        "max 1\noom_kill 1\nunknown 18446744073709551616\n",
    ] {
        assert!(parse_memory_events(text).is_err(), "accepted: {text:?}");
    }
}

#[test]
fn memory_events_parser_rejects_missing_required_records() {
    for text in ["max 1\n", "oom_kill 1\n", "unknown 1\n"] {
        assert!(parse_memory_events(text).is_err(), "accepted: {text:?}");
    }
}

#[test]
fn cgroup_mount_selection_prefers_the_most_specific_covering_root() {
    let text = concat!(
        "36 25 0:32 / /sys/fs/cgroup rw - cgroup2 cgroup rw,memory\n",
        "37 25 0:32 /tenant /sys/fs/cgroup/tenant rw - cgroup2 cgroup rw,memory\n",
    );
    let selected =
        select_cgroup2_mount(text, Path::new("/tenant/build")).expect("specific cgroup mount");
    assert_eq!(selected.root, "/tenant");
    assert_eq!(selected.mountpoint, Path::new("/sys/fs/cgroup/tenant"));
}

#[test]
fn cgroup_mount_selection_rejects_a_tied_most_specific_root() {
    let text = concat!(
        "36 25 0:32 /tenant /sys/fs/cgroup/a rw - cgroup2 cgroup rw,memory\n",
        "37 25 0:33 /tenant /sys/fs/cgroup/b rw - cgroup2 cgroup rw,memory\n",
    );
    let error = match select_cgroup2_mount(text, Path::new("/tenant/build")) {
        Err(error) => error,
        Ok(_) => panic!("tied cgroup mounts must be unavailable"),
    };
    assert!(error.to_string().contains("ambiguous"));
}

#[test]
fn cgroup_mount_selection_decodes_mountinfo_root_and_mountpoint() {
    let text = "36 25 0:32 /tenant\\040space /sys/fs/cgroup\\040v2 rw - cgroup2 cgroup rw,memory\n";
    let selected =
        select_cgroup2_mount(text, Path::new("/tenant space/build")).expect("decoded cgroup mount");
    assert_eq!(selected.root, "/tenant space");
    assert_eq!(selected.mountpoint, Path::new("/sys/fs/cgroup v2"));
}
