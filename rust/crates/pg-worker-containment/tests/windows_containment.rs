#![cfg(windows)]

use pg_worker_containment::{
    ChildTermination, ContainedWorkerProcess, ContainmentError, ExecutionLimits, LaunchOptions,
    MemoryLimitEvidence,
};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::Read;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows_sys::Win32::System::JobObjects::IsProcessInJob;
use windows_sys::Win32::System::Threading::GetCurrentProcess;

static ENVIRONMENT_LOCK: Mutex<()> = Mutex::new(());

fn child_executable() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_containment_test_child"))
}

fn limits(memory_bytes: u64) -> ExecutionLimits {
    ExecutionLimits::try_new(1024, memory_bytes, Duration::from_secs(10)).expect("valid limits")
}

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "pg containment {label} Unicode 漢字 {} {nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create test directory");
    path
}

fn encoded(value: &OsStr) -> String {
    value
        .encode_wide()
        .map(|unit| format!("{unit:04x}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn spawn_reader(mut stream: File) -> (std::thread::JoinHandle<()>, std::sync::mpsc::Receiver<Vec<u8>>) {
    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).expect("read pipe");
        sender.send(bytes).expect("return pipe bytes");
    });
    (handle, receiver)
}

fn finish_reader(
    handle: std::thread::JoinHandle<()>,
    receiver: std::sync::mpsc::Receiver<Vec<u8>>,
) -> Vec<u8> {
    let bytes = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("pipe reached EOF within cleanup deadline");
    handle.join().expect("pipe reader finished");
    bytes
}

fn without_verbatim_prefix(path: &Path) -> &Path {
    path.as_os_str()
        .to_str()
        .and_then(|value| value.strip_prefix(r"\\?\"))
        .map(Path::new)
        .unwrap_or(path)
}

fn current_process_is_in_job() -> bool {
    let mut in_job = 0;
    // SAFETY: the pseudo-handle is valid and the output pointer is live for this call.
    let ok = unsafe { IsProcessInJob(GetCurrentProcess(), std::ptr::null_mut(), &mut in_job) };
    assert_ne!(ok, 0, "IsProcessInJob failed: {}", std::io::Error::last_os_error());
    in_job != 0
}

fn run_child(args: &[OsString], options: &LaunchOptions) -> (ChildTermination, String, String) {
    let mut process =
        ContainedWorkerProcess::spawn(child_executable(), args, options, limits(256 << 20))
            .expect("contained child launch");
    let stdio = process.take_stdio().expect("stdio once");
    drop(stdio.stdin);
    let (stdout_handle, stdout_receiver) = spawn_reader(stdio.stdout);
    let (stderr_handle, stderr_receiver) = spawn_reader(stdio.stderr);
    let deadline = Instant::now() + Duration::from_secs(10);
    let exit = process.reap_direct_child(deadline).expect("reap child");
    process.wait_tree_empty(deadline).expect("empty tree");
    assert!(
        process
            .poll_containment(deadline)
            .expect("final poll")
            .is_none()
    );
    (
        exit.termination,
        String::from_utf8(finish_reader(stdout_handle, stdout_receiver)).expect("UTF-8 stdout"),
        String::from_utf8(finish_reader(stderr_handle, stderr_receiver)).expect("UTF-8 stderr"),
    )
}

#[test]
fn launches_native_child_with_exact_argv_unicode_quotes_and_backslashes() {
    let values = [
        OsString::new(),
        OsString::from("spaces and\ttabs"),
        OsString::from("say\"hello"),
        OsString::from(r"trailing path\"),
        OsString::from("slashes\\\\\"quote"),
        OsString::from("Unicode ✓漢字"),
    ];
    let args = std::iter::once(OsString::from("argv"))
        .chain(values.iter().cloned())
        .collect::<Vec<_>>();
    let (exit, stdout, stderr) = run_child(&args, &LaunchOptions::default());
    assert_eq!(exit, ChildTermination::Exited(0), "{stderr}");
    let actual = stdout.lines().collect::<Vec<_>>();
    let expected = values
        .iter()
        .map(|value| format!("UTF16:{}", encoded(value)))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn launch_options_preserve_clear_override_remove_and_case_insensitive_environment() {
    let _guard = ENVIRONMENT_LOCK.lock().expect("environment lock");
    let inherited = format!("PG_PÄTH_INHERITED_{}", std::process::id());
    let inherited_lower = inherited.to_ascii_lowercase().replace('Ä', "ä");
    std::env::set_var(&inherited, "inherited value ✓");
    let args = [OsString::from("environment"), OsString::from(&inherited)];
    let (_, stdout, _) = run_child(&args, &LaunchOptions::default());
    assert_eq!(stdout.trim(), format!("UTF16:{}", encoded(OsStr::new("inherited value ✓"))));

    let options = LaunchOptions::new()
        .env(&inherited_lower, "first")
        .env(inherited.to_ascii_uppercase(), "second value 漢")
        .env_remove("PATH");
    let args = [
        OsString::from("environment"),
        OsString::from(&inherited_lower),
        OsString::from("Path"),
    ];
    let (_, stdout, _) = run_child(&args, &options);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines[0], format!("UTF16:{}", encoded(OsStr::new("second value 漢"))));
    assert_eq!(lines[1], "ABSENT");

    let options = LaunchOptions::new().env_clear().env("Only", "value");
    let args = [
        OsString::from("environment"),
        OsString::from("Only"),
        OsString::from(&inherited),
    ];
    let (_, stdout, _) = run_child(&args, &options);
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["UTF16:0076,0061,006c,0075,0065", "ABSENT"]);
    std::env::remove_var(&inherited);
}

#[test]
fn launch_options_set_child_current_directory_with_spaces_and_unicode() {
    let directory = temporary_directory("cwd");
    let args = [OsString::from("cwd")];
    let (exit, stdout, stderr) = run_child(&args, &LaunchOptions::new().current_dir(&directory));
    assert_eq!(exit, ChildTermination::Exited(0), "{stderr}");
    let expected = fs::canonicalize(&directory).expect("canonical test directory");
    assert_eq!(
        stdout.trim(),
        format!("UTF16:{}", encoded(without_verbatim_prefix(&expected).as_os_str()))
    );
    fs::remove_dir(&directory).expect("remove test directory");
}

#[test]
fn child_observes_job_membership_before_first_user_action() {
    let args = [OsString::from("job")];
    let (exit, stdout, stderr) = run_child(&args, &LaunchOptions::default());
    assert_eq!(exit, ChildTermination::Exited(0), "{stderr}");
    assert_eq!(stdout.trim(), "IN_JOB=1");
}

#[test]
fn managed_host_job_allows_nested_atomic_containment() {
    if !current_process_is_in_job() {
        return;
    }
    let args = [OsString::from("job")];
    let (exit, stdout, stderr) = run_child(&args, &LaunchOptions::default());
    assert_eq!(exit, ChildTermination::Exited(0), "{stderr}");
    assert_eq!(stdout.trim(), "IN_JOB=1");
}

#[test]
fn direct_child_exit_code_259_is_not_mistaken_for_a_live_process() {
    let args = [OsString::from("exit"), OsString::from("259")];
    let (exit, _, _) = run_child(&args, &LaunchOptions::default());
    assert_eq!(exit, ChildTermination::Exited(259));
}

#[test]
fn missing_executable_returns_typed_launch_failure_without_fallback() {
    let missing = temporary_directory("missing").join("does-not-exist.exe");
    let result = ContainedWorkerProcess::spawn(
        &missing,
        &[],
        &LaunchOptions::default(),
        limits(64 << 20),
    );
    assert!(matches!(result, Err(ContainmentError::Failed { .. })));
    fs::remove_dir(missing.parent().expect("missing parent")).expect("remove test directory");
}

#[test]
fn termination_kills_descendant_tree_and_closes_both_pipes_within_deadline() {
    let directory = temporary_directory("tree");
    let ready = directory.join("ready");
    let late = directory.join("late");
    let args = [
        OsString::from("spawn-holder"),
        ready.as_os_str().to_os_string(),
        late.as_os_str().to_os_string(),
    ];
    let mut process = ContainedWorkerProcess::spawn(
        child_executable(),
        &args,
        &LaunchOptions::default(),
        limits(256 << 20),
    )
    .expect("contained launch");
    let stdio = process.take_stdio().expect("stdio once");
    drop(stdio.stdin);
    let (stdout_handle, stdout_receiver) = spawn_reader(stdio.stdout);
    let (stderr_handle, stderr_receiver) = spawn_reader(stdio.stderr);
    let ready_deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() && Instant::now() < ready_deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ready.exists(), "descendant never became ready");
    let cleanup_deadline = Instant::now() + Duration::from_secs(5);
    process.terminate_tree(cleanup_deadline).expect("tree kill");
    process.reap_direct_child(cleanup_deadline).expect("direct reap");
    process.wait_tree_empty(cleanup_deadline).expect("tree empty");
    let stdout = finish_reader(stdout_handle, stdout_receiver);
    let stderr = finish_reader(stderr_handle, stderr_receiver);
    assert!(stdout.windows(6).any(|bytes| bytes == b"holder"));
    assert!(stderr.windows(6).any(|bytes| bytes == b"holder"));
    assert!(!late.exists(), "killed descendant wrote delayed sentinel");
    fs::remove_dir_all(&directory).expect("remove test directory");
}

#[test]
fn aggregate_descendant_memory_limit_latches_native_evidence_and_kills_tree() {
    for attempt in 0..3 {
        let directory = temporary_directory(&format!("memory-{attempt}"));
        let ready = directory.join("ready");
        let args = [
            OsString::from("spawn-allocators"),
            ready.as_os_str().to_os_string(),
        ];
        let memory_limit = 128 << 20;
        let mut process = ContainedWorkerProcess::spawn(
            child_executable(),
            &args,
            &LaunchOptions::default(),
            limits(memory_limit),
        )
        .expect("contained launch");
        let stdio = process.take_stdio().expect("stdio once");
        drop(stdio.stdin);
        let (stdout_handle, stdout_receiver) = spawn_reader(stdio.stdout);
        let (stderr_handle, stderr_receiver) = spawn_reader(stdio.stderr);
        let deadline = Instant::now() + Duration::from_secs(10);
        let evidence = loop {
            if let Some(evidence) = process
                .poll_containment(deadline)
                .expect("poll containment")
            {
                break evidence;
            }
            assert!(Instant::now() < deadline, "memory evidence did not fire");
            std::thread::sleep(Duration::from_millis(2));
        };
        let MemoryLimitEvidence::WindowsObservedJobMemoryLimitViolation {
            notification_limit_bytes,
            peak_job_memory_used_bytes,
        } = evidence
        else {
            panic!("Windows adapter returned non-Windows evidence");
        };
        assert_eq!(notification_limit_bytes, 64 << 20);
        assert!(peak_job_memory_used_bytes >= notification_limit_bytes);
        process.reap_direct_child(deadline).expect("direct reap");
        process.wait_tree_empty(deadline).expect("tree empty");
        let final_evidence = process
            .final_evidence_and_peak(deadline)
            .expect("final evidence");
        assert_eq!(final_evidence.memory_limit, Some(evidence));
        assert!(final_evidence.peak_memory_charge_bytes > 0);
        let _stdout = finish_reader(stdout_handle, stdout_receiver);
        let _stderr = finish_reader(stderr_handle, stderr_receiver);
        fs::remove_dir_all(&directory).expect("remove test directory");
    }
}
