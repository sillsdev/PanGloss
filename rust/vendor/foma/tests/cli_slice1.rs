//! Integration tests for the foma CLI covering slice-1 iface behavior whose
//! only observable effect is process/stdout state (not assertable in-process).
//!
//! iface_quit only ever calls exit(0) (after destroying defined nets and the
//! stack), so it has no in-process observable path — it is pinned here by
//! spawning the built `foma -q` binary, feeding it "quit", and asserting a
//! clean exit.

use std::io::Write;
use std::process::{Command, Stdio};

// iface_quit: "quit" destroys all defined nets + the stack and exit(0)s; the
// process terminates with status 0 and never returns to the REPL.
// [spec:foma:sem:iface.iface-quit-fn/test]
// [spec:foma:sem:foma.iface-quit-fn/test]
#[test]
fn quit_command_exits_zero() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_foma"))
        .arg("-q")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn foma binary");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"quit\n")
        .expect("failed to write to foma stdin");
    let status = child.wait().expect("failed to wait on foma");
    assert!(
        status.success(),
        "foma -q with `quit` should exit 0, got {status:?}"
    );
}
