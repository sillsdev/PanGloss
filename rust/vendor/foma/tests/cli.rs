//! Integration tests for the three CLI binaries (`foma`, `flookup`,
//! `cgflookup`), spawning the built executables and asserting on their
//! stdout/stderr bytes. Expected values were derived from the ported binaries
//! (cross-checked against upstream foma semantics) while writing these tests;
//! the tests never invoke any external foma at runtime.
//!
//! Binary paths come from Cargo's `CARGO_BIN_EXE_<name>` env vars. Small
//! transducer files are built by our own `foma` binary (`regex …; save stack`)
//! so the lookup tools have deterministic, self-contained inputs.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

fn foma() -> Command {
    Command::new(env!("CARGO_BIN_EXE_foma"))
}
fn flookup() -> Command {
    Command::new(env!("CARGO_BIN_EXE_flookup"))
}
fn cgflookup() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cgflookup"))
}

/// Spawn `cmd`, write `input` to its stdin (then close it → EOF), and collect
/// (stdout, stderr, exit status).
fn run(cmd: &mut Command, input: &[u8]) -> (Vec<u8>, Vec<u8>, ExitStatus) {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn binary");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input)
        .expect("failed to write to child stdin");
    let out = child.wait_with_output().expect("failed to wait on child");
    (out.stdout, out.stderr, out.status)
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A unique, per-process temporary path (files are removed on drop by callers).
fn temp_path(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    p.push(format!(
        "foma_cli_test_{}_{}_{}.foma",
        tag,
        std::process::id(),
        n
    ));
    p
}

/// Build a saved-stack `.foma` file from the given regexes (pushed in order,
/// so the last regex ends up on top of the stack), using our own foma binary.
fn build_stack(tag: &str, regexes: &[&str]) -> PathBuf {
    let path = temp_path(tag);
    let mut script = String::new();
    for r in regexes {
        script.push_str(&format!("regex {};\n", r));
    }
    script.push_str(&format!("save stack {}\n", path.display()));
    let (_o, _e, st) = run(foma().arg("-q"), script.as_bytes());
    assert!(st.success(), "foma failed while building stack file");
    assert!(
        path.exists(),
        "stack file {} was not created",
        path.display()
    );
    path
}

fn s(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

// ─────────────────────────────── foma binary ───────────────────────────────

// main-fn: `-v` prints "argv[0] MAJOR.MINOR.BUILD STATUS\n" and exit(0). argv[0]
// is the spawned path, so we assert the version suffix.
// [spec:foma:sem:foma.main-fn/test]
#[test]
fn foma_v_prints_version_and_exits() {
    let (out, _err, st) = run(foma().arg("-v"), b"");
    assert!(st.success());
    assert!(
        s(&out).ends_with(" 0.10.0alpha\n"),
        "version line was {:?}",
        s(&out)
    );
}

// main-fn: `-h` calls print_help() and exit(0).
// print-help-fn: usage string + "Options:\n" + one tab-aligned line per option
// (-e/-f/-l/-p/-q/-r/-s/-v) — the CLI option "command list".
// [spec:foma:sem:foma.main-fn/test]
// [spec:foma:sem:foma.print-help-fn/test]
#[test]
fn foma_h_prints_help() {
    let (out, _err, st) = run(foma().arg("-h"), b"");
    assert!(st.success());
    let text = s(&out);
    assert!(text.starts_with("Usage: foma "));
    assert!(text.contains("Options:\n"));
    for opt in [
        "-e \"command\"",
        "-f scriptfile",
        "-l scriptfile",
        "-p\t",
        "-q\t",
        "-r\t",
        "-s\t",
        "-v\t",
    ] {
        assert!(text.contains(opt), "help missing {:?}", opt);
    }
}

// main-fn: `-q` sets g_verbose = 0, suppressing the startup banner; `-e` runs a
// command immediately at startup. With no other input the REPL then hits EOF and
// exits 0. The banner-suppressed run of `quit` produces no output at all.
// [spec:foma:sem:foma.main-fn/test]
#[test]
fn foma_q_suppresses_banner_and_e_executes() {
    // -q + quit: banner suppressed, quit exits before the EOF newline → empty.
    let (out, _err, st) = run(foma().arg("-q"), b"quit\n");
    assert!(st.success());
    assert!(
        out.is_empty(),
        "quiet quit should emit nothing, got {:?}",
        s(&out)
    );

    // Default (no -q) prints the multi-line disclaimer banner first.
    let (out2, _e2, st2) = run(&mut foma(), b"quit\n");
    assert!(st2.success());
    assert!(
        s(&out2).starts_with("Foma, version 0.10.0"),
        "expected banner, got {:?}",
        s(&out2)
    );

    // -e executes the given command at startup (regex a b c; leaves it on the
    // stack); the following EOF prints "\n" and exits 0.
    let (out3, _e3, st3) = run(foma().args(["-q", "-e", "regex a b c;"]), b"print words\n");
    assert!(st3.success());
    assert_eq!(out3, b"abc\n\n");
}

// main-fn: `-f scriptfile` reads the whole file, runs it, and exit(0) — WITHOUT
// entering the REPL, so there is no trailing EOF newline. This also pins the
// fix that flex-style scanning stops at the file_to_mem NUL terminator (no
// spurious "***Unknown command" for the trailing '\0').
// [spec:foma:sem:foma.main-fn/test]
#[test]
fn foma_f_runs_script_and_quits() {
    let script = temp_path("script");
    std::fs::write(&script, b"regex a b c;\nprint words\n").unwrap();
    let (out, _err, st) = run(foma().args(["-q", "-f"]).arg(&script), b"");
    let _ = std::fs::remove_file(&script);
    assert!(st.success());
    assert_eq!(out, b"abc\n", "got {:?}", s(&out));
}

// rl-gets-fn: the REPL reads piped stdin line by line and returns NULL at EOF;
// main-fn then prints "\n" and exit(0) at the main prompt. Byte-exact `print
// words` output ("abc") is followed by that EOF newline.
// [spec:foma:sem:foma.rl-gets-fn/test]
// [spec:foma:sem:foma.main-fn/test]
#[test]
fn foma_reads_piped_stdin_until_eof() {
    let (out, _err, st) = run(foma().arg("-q"), b"regex a b c;\nprint words\n");
    assert!(st.success());
    // "abc\n" from print words, then "\n" from the EOF handler.
    assert_eq!(out, b"abc\n\n", "got {:?}", s(&out));
}

// main-fn: piped multi-command session exercising the interface.l dispatch:
// apply down/up, define + push, undefine, stack ops (print size, clear stack),
// echo, and multi-line `regex …;` continuation. Each output is byte-exact.
// [spec:foma:sem:foma.main-fn/test]
#[test]
fn foma_piped_commands_byte_exact() {
    // apply down: a:b lower-applied to "a" yields "b".
    let (out, _e, st) = run(foma().arg("-q"), b"regex a:b;\napply down a\n");
    assert!(st.success());
    assert_eq!(out, b"b\n\n", "apply down: {:?}", s(&out));

    // apply up: a:b upper-applied to "b" yields "a".
    let (out, _e, _) = run(foma().arg("-q"), b"regex a:b;\napply up b\n");
    assert_eq!(out, b"a\n\n", "apply up: {:?}", s(&out));

    // multi-line regex continuation: the body spans a newline until the `;`.
    let (out, _e, _) = run(foma().arg("-q"), b"regex a b\nc d;\nprint words\n");
    assert_eq!(out, b"abcd\n\n", "multiline regex: {:?}", s(&out));

    // define + push + print words, then undefine makes the name unknown.
    let (out, _e, _) = run(
        foma().arg("-q"),
        b"define foo a b;\npush foo\nprint words\nundefine foo\npush foo\n",
    );
    assert_eq!(
        out,
        b"ab\n'foo' is not a defined symbol.\n\n".to_vec(),
        "define/undefine: {:?}",
        s(&out)
    );

    // stack ops: print size on a single-arc net, then clear stack empties it.
    let (out, _e, _) = run(
        foma().arg("-q"),
        b"regex a;\nprint size\nclear stack\nprint size\n",
    );
    assert_eq!(
        out,
        b"202 bytes. 2 states, 1 arc, 1 path.\nNot enough networks on stack. Operation requires at least 1.\n\n".to_vec(),
        "stack ops: {:?}",
        s(&out)
    );

    // echo prints its argument verbatim plus a newline.
    let (out, _e, _) = run(foma().arg("-q"), b"echo hello world\n");
    assert_eq!(out, b"hello world\n\n", "echo: {:?}", s(&out));

    // apply-mode block: bare "apply down" enters apply mode, each following line
    // is a word (a→b, then "???" is impossible here) until "END;" returns to the
    // main prompt where "print size" runs again.
    let (out, _e, _) = run(
        foma().arg("-q"),
        b"regex a:b;\napply down\na\nEND;\nprint size\n",
    );
    assert_eq!(
        out,
        b"b\n228 bytes. 2 states, 1 arc, 1 path.\n\n".to_vec(),
        "apply-mode block: {:?}",
        s(&out)
    );
}

// main-fn: the `source` command reads and runs another script, announcing
// "Opening file '<f>'." first.
// [spec:foma:sem:foma.main-fn/test]
#[test]
fn foma_source_command() {
    let script = temp_path("source");
    std::fs::write(&script, b"regex x y z;\nprint words\n").unwrap();
    let cmd = format!("source {}\n", script.display());
    let (out, _err, st) = run(foma().arg("-q"), cmd.as_bytes());
    let _ = std::fs::remove_file(&script);
    assert!(st.success());
    let text = s(&out);
    assert!(text.starts_with("Opening file '"), "got {:?}", text);
    assert!(
        text.contains("xyz\n"),
        "source did not run script: {:?}",
        text
    );
}

// ─────────────────────────────── flookup ───────────────────────────────────

// main-fn + get-next-line-fn + handle-line-fn + app-print-fn + applyer-fn +
// lookup-chain: default up-application of words from stdin through a single net.
// Each stdin line is read by get_next_line; handle_line walks the one-node chain
// applying `applyer` (apply_up by default); app_print echoes "line<TAB>result"
// and "+?" for a miss; main prints the wordseparator ("\n") after each word.
// [spec:foma:sem:flookup.main-fn/test]
// [spec:foma:sem:flookup.get-next-line-fn/test]
// [spec:foma:sem:flookup.handle-line-fn/test]
// [spec:foma:sem:flookup.app-print-fn/test]
// [spec:foma:sem:flookup.applyer-fn/test]
// [spec:foma:def:flookup.lookup-chain/test]
#[test]
fn flookup_single_net_up() {
    let net = build_stack("fl_ab", &["a:b"]);
    let (out, _err, st) = run(flookup().arg(&net), b"b\nz\n");
    let _ = std::fs::remove_file(&net);
    assert!(st.success());
    // "b" → "a" (hit, tab-separated, echoed input); "z" → "+?" (miss).
    assert_eq!(out, b"b\ta\n\nz\t+?\n\n", "got {:?}", s(&out));
}

// applyer-fn: `-i` repoints the applyer to apply_down (direction DIR_DOWN), so
// the same a:b net now maps "a" (upper) → "b" (lower).
// [spec:foma:sem:flookup.applyer-fn/test]
// [spec:foma:sem:flookup.main-fn/test]
#[test]
fn flookup_inverse_down() {
    let net = build_stack("fl_i", &["a:b"]);
    let (out, _err, st) = run(flookup().args(["-i"]).arg(&net), b"a\n");
    let _ = std::fs::remove_file(&net);
    assert!(st.success());
    assert_eq!(out, b"a\tb\n\n", "got {:?}", s(&out));
}

// handle-line-fn + lookup-chain: two disjoint transducers (a:b, c:d) exercise
// the difference between the two chain-traversal modes. Cascade (default)
// composes them: neither "b" nor "d" survives the composition, so both miss.
// Alternates (-a, priority union) tries each net in turn: "b"→"a" (net a:b),
// "d"→"c" (net c:d). This pins both branches of handle_line and the multi-net
// chain construction (append vs prepend).
// [spec:foma:sem:flookup.handle-line-fn/test]
// [spec:foma:def:flookup.lookup-chain/test]
// [spec:foma:sem:flookup.main-fn/test]
#[test]
fn flookup_cascade_vs_alternates() {
    let net = build_stack("fl_two", &["a:b", "c:d"]);

    // Cascade (default): composition of a:b and c:d is empty for these inputs.
    let (out, _e, st) = run(flookup().arg(&net), b"b\nd\n");
    assert!(st.success());
    assert_eq!(out, b"b\t+?\n\nd\t+?\n\n", "cascade: {:?}", s(&out));

    // Alternates: first net that yields a result wins per word.
    let (out2, _e2, st2) = run(flookup().args(["-a"]).arg(&net), b"b\nd\n");
    assert!(st2.success());
    assert_eq!(out2, b"b\ta\n\nd\tc\n\n", "alternates: {:?}", s(&out2));

    let _ = std::fs::remove_file(&net);
}

// main-fn: `-v` prints the version banner and exit 0; a missing file operand and
// unknown options print the usage string to stderr and exit(EXIT_FAILURE).
// [spec:foma:sem:flookup.main-fn/test]
#[test]
fn flookup_version_and_usage_errors() {
    let (out, _err, st) = run(flookup().arg("-v"), b"");
    assert!(st.success());
    assert_eq!(out, b"flookup 1.03 (foma library version 0.10.0alpha)\n");

    // Missing file operand → usage on stderr, exit failure.
    let (_o, err, st) = run(&mut flookup(), b"");
    assert!(!st.success());
    assert!(
        s(&err).starts_with("Usage: flookup "),
        "stderr {:?}",
        s(&err)
    );

    // `-h` prints usage + help to stdout and exits 0.
    let (out, _e, st) = run(flookup().arg("-h"), b"");
    assert!(st.success());
    assert!(s(&out).starts_with("Usage: flookup "));
}

// ─────────────────────────────── cgflookup ─────────────────────────────────

// main-fn + get-next-line-fn + handle-line-fn + app-print-fn + applyer-fn +
// lookup-chain: CG cohort output. handle_line prints the cohort header
// `"<word>"` immediately before the first reading; each reading is TAB-indented;
// a word with no analyses prints only the bare header (main's fallback). The
// word separator defaults to empty (unlike flookup).
// [spec:foma:sem:cgflookup.main-fn/test]
// [spec:foma:sem:cgflookup.get-next-line-fn/test]
// [spec:foma:sem:cgflookup.handle-line-fn/test]
// [spec:foma:sem:cgflookup.app-print-fn/test]
// [spec:foma:sem:cgflookup.applyer-fn/test]
// [spec:foma:def:cgflookup.lookup-chain/test]
#[test]
fn cgflookup_cohort_output() {
    let net = build_stack("cg_ab", &["a:b"]);
    let (out, _err, st) = run(cgflookup().arg(&net), b"b\nz\n");
    let _ = std::fs::remove_file(&net);
    assert!(st.success());
    // "b": header `"<b>"` then reading "\ta"; "z": bare header only.
    assert_eq!(out, b"\"<b>\"\n\ta\n\"<z>\"\n", "got {:?}", s(&out));
}

// app-print-fn: `-u` marks readings of capitalized wordforms — when the first
// char of the INPUT line is uppercase, the reading is suffixed " <*>". An
// accepting net for the literal string "Abc" (an identity path) reproduces the
// input on apply_up so the reading equals the (uppercase) input.
// [spec:foma:sem:cgflookup.app-print-fn/test]
// [spec:foma:sem:cgflookup.main-fn/test]
#[test]
fn cgflookup_uppercase_marking() {
    let up = build_stack("cg_up", &["{Abc}"]);
    let low = build_stack("cg_low", &["{abc}"]);

    // Uppercase input → reading tagged with " <*>".
    let (out, _e, st) = run(cgflookup().args(["-u"]).arg(&up), b"Abc\n");
    assert!(st.success());
    assert_eq!(out, b"\"<Abc>\"\n\tAbc <*>\n", "uppercase: {:?}", s(&out));

    // Lowercase input → no tag, even with -u.
    let (out2, _e2, _) = run(cgflookup().args(["-u"]).arg(&low), b"abc\n");
    assert_eq!(out2, b"\"<abc>\"\n\tabc\n", "lowercase: {:?}", s(&out2));

    // Without -u the uppercase input is not tagged.
    let (out3, _e3, _) = run(cgflookup().arg(&up), b"Abc\n");
    assert_eq!(out3, b"\"<Abc>\"\n\tAbc\n", "no -u: {:?}", s(&out3));

    let _ = std::fs::remove_file(&up);
    let _ = std::fs::remove_file(&low);
}

// main-fn: `-v` prints the version and exits 0; missing file operand → usage to
// stderr and exit(EXIT_FAILURE).
// [spec:foma:sem:cgflookup.main-fn/test]
#[test]
fn cgflookup_version_and_usage_error() {
    let (out, _err, st) = run(cgflookup().arg("-v"), b"");
    assert!(st.success());
    assert_eq!(out, b"cgflookup 1.03 (foma library version 0.10.0alpha)\n");

    let (_o, err, st) = run(&mut cgflookup(), b"");
    assert!(!st.success());
    assert!(
        s(&err).starts_with("Usage: cgflookup "),
        "stderr {:?}",
        s(&err)
    );
}

// -x (advertised in the usage text; disables echo, a no-op here since cgflookup
// never echoes) is accepted rather than erroring out to usage as C did.
// [spec:foma:sem:cgflookup.main-fn+1/test]
#[test]
fn cgflookup_dash_x_is_accepted() {
    let net = build_stack("cg_x", &["a:b"]);
    let (out, _err, st) = run(cgflookup().args(["-x"]).arg(&net), b"b\n");
    let _ = std::fs::remove_file(&net);
    assert!(
        st.success(),
        "cgflookup -x should be accepted, not an error"
    );
    assert_eq!(out, b"\"<b>\"\n\ta\n", "got {:?}", s(&out));
}

/* ------------------------------------------------------------------ */
/* flookup UDP server mode                                             */
/* ------------------------------------------------------------------ */

// [spec:foma:sem:flookup.server-init-fn/test]
// [spec:foma:sem:flookup.app-print-fn+1/test]
#[test]
fn flookup_server_mode_binds_and_answers_over_udp() {
    use std::net::UdpSocket;
    use std::time::Duration;

    let stack = build_stack("srv", &["a:b"]);
    // Dodge port contention: derive a high port from the pid.
    let port = 20000 + (std::process::id() % 20000) as u16;
    let mut child = flookup()
        .arg("-S")
        .arg("-A")
        .arg("127.0.0.1")
        .arg("-P")
        .arg(port.to_string())
        .arg(&stack)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn flookup server");

    let sock = UdpSocket::bind("127.0.0.1:0").expect("client bind");
    sock.set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let server = format!("127.0.0.1:{}", port);

    // The server prints its banner then blocks in recvfrom; retry-send until a
    // reply arrives (covers startup latency without reading the child's pipe).
    let mut reply = Vec::new();
    let mut got = 0usize;
    for _ in 0..20 {
        sock.send_to(b"b\n", &server).expect("send");
        let mut buf = [0u8; 65536];
        if let Ok((n, _)) = sock.recv_from(&mut buf) {
            got = n;
            reply.extend_from_slice(&buf[..n]);
            break;
        }
    }
    assert!(got > 0, "no UDP reply from flookup server");
    // apply up of surface "b" through a:b yields lexical "a":
    // serverstring accumulates "b\ta\n" + the blank separator line.
    let text = String::from_utf8_lossy(&reply);
    assert!(
        text.contains("b\ta\n"),
        "unexpected server reply: {:?}",
        text
    );

    // A miss uses the "+?" marker, the same as stdin mode. (C emitted the
    // reverse "?+" in server mode.)
    sock.send_to(b"zzz\n", &server).expect("send miss");
    let mut buf = [0u8; 65536];
    let (n, _) = sock.recv_from(&mut buf).expect("miss reply");
    let miss = String::from_utf8_lossy(&buf[..n]).into_owned();
    assert!(
        miss.contains("+?") && !miss.contains("?+"),
        "miss reply should carry +?: {:?}",
        miss
    );

    child.kill().ok();
    child.wait().ok();
    std::fs::remove_file(&stack).ok();
}
