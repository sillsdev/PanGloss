use super::*;

fn decoded(value: Vec<u16>) -> String {
    let units = &value[..value.len() - 1];
    String::from_utf16(units).expect("test command line is valid UTF-16")
}

#[test]
fn command_line_matches_windows_argv_quoting_rules() {
    let line = command_line(
        Path::new(r"C:\Program Files\worker.exe"),
        &[
            OsString::from("plain"),
            OsString::from("space value"),
            OsString::from("say\"hi"),
            OsString::from("trailing path\\"),
        ],
    )
    .expect("valid command line");

    assert_eq!(
        decoded(line),
        r#""C:\Program Files\worker.exe" plain "space value" say\"hi "trailing path\\""#
    );
}

#[test]
fn notification_reserves_bounded_headroom_below_the_hard_cap() {
    assert_eq!(notification_limit(128 * 1024 * 1024), 64 * 1024 * 1024);
    assert_eq!(
        notification_limit(10 * 1024 * 1024 * 1024),
        10 * 1024 * 1024 * 1024 - MAX_NOTIFICATION_RESERVE_BYTES
    );
    assert_eq!(notification_limit(1), 1);
}
