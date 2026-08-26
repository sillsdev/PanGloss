#![cfg(target_os = "linux")]

use pg_worker_containment::{
    ChildTermination, ContainedWorkerProcess, ContainmentError, ExecutionLimits, LaunchOptions,
    MemoryLimitEvidence,
};
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static ENVIRONMENT_LOCK: Mutex<()> = Mutex::new(());

fn child_executable() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_containment_test_child"))
}

fn limits(memory_bytes: u64) -> ExecutionLimits {
    ExecutionLimits::try_new(1024, memory_bytes, Duration::from_secs(10)).expect("valid limits")
}

fn required_capability() -> bool {
    matches!(
        std::env::var("PANGLOSS_CGROUP_TEST_REQUIRED")
            .ok()
            .as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "pangloss-cgroup-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create test directory");
    path
}

fn encoded(value: &OsStr) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn spawn_reader(
    mut stream: File,
) -> (
    std::thread::JoinHandle<()>,
    std::sync::mpsc::Receiver<Vec<u8>>,
) {
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
        .recv_timeout(Duration::from_secs(3))
        .expect("pipe reached EOF within cleanup deadline");
    handle.join().expect("pipe reader finished");
    bytes
}

fn spawn_or_skip(
    args: &[OsString],
    options: &LaunchOptions,
    memory_bytes: u64,
) -> Option<ContainedWorkerProcess> {
    assert_configured_root_ready();
    match ContainedWorkerProcess::spawn(child_executable(), args, options, limits(memory_bytes)) {
        Ok(process) => Some(process),
        Err(ContainmentError::Unavailable { detail }) if !required_capability() => {
            eprintln!("SKIP: Linux cgroup containment unavailable: {detail}");
            None
        }
        Err(error) => panic!("Linux containment gate failed: {error}"),
    }
}

fn wait_for_file(path: &Path, deadline: Instant) {
    while !path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        path.exists(),
        "fixture did not become ready: {}",
        path.display()
    );
}

fn run_clean_child(
    args: &[OsString],
    options: &LaunchOptions,
) -> Option<(ChildTermination, String, String)> {
    let mut process = spawn_or_skip(args, options, 256 << 20)?;
    let stdio = process.take_stdio().expect("stdio once");
    drop(stdio.stdin);
    let (stdout_handle, stdout_receiver) = spawn_reader(stdio.stdout);
    let (stderr_handle, stderr_receiver) = spawn_reader(stdio.stderr);
    let deadline = Instant::now() + Duration::from_secs(10);
    let exit = process.reap_direct_child(deadline).expect("reap child");
    process.wait_tree_empty(deadline).expect("empty tree");
    assert!(process.poll_containment().expect("final poll").is_none());
    let evidence = process.final_evidence_and_peak().expect("final evidence");
    assert!(evidence.memory_limit.is_none());
    Some((
        exit.termination,
        String::from_utf8(finish_reader(stdout_handle, stdout_receiver)).expect("UTF-8 stdout"),
        String::from_utf8(finish_reader(stderr_handle, stderr_receiver)).expect("UTF-8 stderr"),
    ))
}

#[test]
fn missing_executable_returns_typed_spawn_failure_without_fallback() {
    let directory = temporary_directory("missing");
    let missing = directory.join("does-not-exist");
    let snapshot = configured_worker_root_snapshot();
    let result =
        ContainedWorkerProcess::spawn(&missing, &[], &LaunchOptions::default(), limits(64 << 20));
    match result {
        Err(ContainmentError::Failed { .. }) => {}
        Err(ContainmentError::Unavailable { detail }) if !required_capability() => {
            eprintln!("SKIP: Linux cgroup containment unavailable: {detail}");
            fs::remove_dir(&directory).expect("remove missing-executable test directory");
            return;
        }
        Err(error) => panic!("missing executable returned the wrong spawn error: {error}"),
        Ok(_) => panic!("missing executable must fail during contained spawn, not fall back"),
    }
    if let Some((mapped, before)) = snapshot {
        let after = direct_worker_children(&mapped).expect("read delegated root children");
        assert_eq!(
            after, before,
            "missing executable left a worker cgroup residue"
        );
    }
    fs::remove_dir(&directory).expect("remove missing-executable test directory");
}

struct EnvironmentGuard {
    key: &'static str,
    original: Option<OsString>,
}

impl EnvironmentGuard {
    fn set(key: &'static str, value: Option<&OsStr>) -> Self {
        let original = std::env::var_os(key);
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
        Self { key, original }
    }
}

impl Drop for EnvironmentGuard {
    fn drop(&mut self) {
        match self.original.as_deref() {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

fn canonical_hierarchy_path(value: &OsStr) -> Option<String> {
    let value = value.to_str()?;
    if value == "/" {
        return Some(value.to_string());
    }
    if !value.starts_with('/')
        || value.ends_with('/')
        || value.contains("//")
        || value
            .split('/')
            .skip(1)
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return None;
    }
    Some(value.to_string())
}

fn configured_delegated_root() -> Option<String> {
    canonical_hierarchy_path(std::env::var_os("PANGLOSS_CGROUP_DELEGATED_ROOT")?.as_os_str())
}

fn unified_membership(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()?.lines().find_map(|line| {
        let relative = line.strip_prefix("0::")?.trim();
        Some(if relative.is_empty() {
            "/".to_string()
        } else if relative.starts_with('/') {
            relative.to_string()
        } else {
            format!("/{relative}")
        })
    })
}

fn unescape_mountinfo(value: &str) -> String {
    value
        .replace(r"\040", " ")
        .replace(r"\011", "\t")
        .replace(r"\012", "\n")
        .replace(r"\134", "\\")
}

fn hierarchy_components(path: &str) -> impl Iterator<Item = &str> {
    path.trim_matches('/')
        .split('/')
        .filter(|component| !component.is_empty())
}

fn hierarchy_contains(ancestor: &str, descendant: &str) -> bool {
    ancestor == "/"
        || ancestor == descendant
        || descendant
            .strip_prefix(ancestor.trim_end_matches('/'))
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn map_hierarchy_to_cgroup_mount(hierarchy: &str) -> Option<PathBuf> {
    let mountinfo = fs::read_to_string("/proc/self/mountinfo").ok()?;
    let mut candidates = mountinfo.lines().filter_map(|line| {
        let (left, right) = line.split_once(" - ")?;
        if right.split_whitespace().next()? != "cgroup2" {
            return None;
        }
        let fields = left.split_whitespace().collect::<Vec<_>>();
        let mount_root = unescape_mountinfo(fields.get(3)?);
        let mount_point = PathBuf::from(unescape_mountinfo(fields.get(4)?));
        hierarchy_contains(&mount_root, hierarchy).then_some((
            hierarchy_components(&mount_root).count(),
            mount_root,
            mount_point,
        ))
    });
    let first = candidates.next()?;
    let (_, mount_root, mount_point) = candidates.fold(first, |best, candidate| {
        if candidate.0 > best.0 {
            candidate
        } else {
            best
        }
    });
    let suffix = hierarchy
        .strip_prefix(mount_root.trim_end_matches('/'))
        .unwrap_or("")
        .trim_start_matches('/');
    Some(if suffix.is_empty() {
        mount_point
    } else {
        mount_point.join(suffix)
    })
}

fn current_unified_membership() -> Option<String> {
    unified_membership(Path::new("/proc/self/cgroup"))
}

fn parent_hierarchy(path: &str) -> Option<String> {
    let parent = Path::new(path).parent()?.to_str()?;
    Some(if parent.is_empty() {
        "/".to_string()
    } else {
        parent.to_string()
    })
}

fn direct_worker_children(root: &Path) -> std::io::Result<BTreeSet<OsString>> {
    let mut children = BTreeSet::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        if name
            .as_os_str()
            .to_string_lossy()
            .starts_with(".pangloss-worker-")
        {
            children.insert(name);
        }
    }
    Ok(children)
}

fn configured_worker_root_snapshot() -> Option<(PathBuf, BTreeSet<OsString>)> {
    let root = configured_delegated_root()?;
    let mapped = map_hierarchy_to_cgroup_mount(&root)?;
    let children = direct_worker_children(&mapped).expect("read delegated root children");
    Some((mapped, children))
}

fn current_parent_worker_root_snapshot() -> Option<(PathBuf, BTreeSet<OsString>)> {
    let current = current_unified_membership()?;
    let root = parent_hierarchy(&current)?;
    let mapped = map_hierarchy_to_cgroup_mount(&root)?;
    let children = direct_worker_children(&mapped).expect("read current parent children");
    Some((mapped, children))
}

fn assert_configured_root_ready() {
    let Some(raw_root) = std::env::var_os("PANGLOSS_CGROUP_DELEGATED_ROOT") else {
        return;
    };
    let root = canonical_hierarchy_path(&raw_root).expect("configured delegated root is canonical");
    let current = current_unified_membership().expect("read current unified cgroup membership");
    assert!(
        current != root && hierarchy_contains(&root, &current),
        "current membership {current:?} must be a strict descendant of delegated root {root:?}"
    );
    let mapped_root = map_hierarchy_to_cgroup_mount(&root)
        .expect("map configured delegated root through a visible cgroup2 mount");
    assert!(
        fs::read_to_string(mapped_root.join("cgroup.procs"))
            .expect("read delegated root cgroup.procs")
            .trim()
            .is_empty(),
        "configured delegated root must be empty"
    );
}

fn assert_unavailable_spawn(context: &str) {
    match ContainedWorkerProcess::spawn(
        child_executable(),
        &[OsString::from("cgroup")],
        &LaunchOptions::default(),
        limits(256 << 20),
    ) {
        Err(ContainmentError::Unavailable { detail }) => {
            assert!(!detail.trim().is_empty(), "{context} returned empty detail")
        }
        Err(error) => panic!("{context} returned the wrong error: {error}"),
        Ok(_) => panic!("{context} unexpectedly spawned a worker"),
    }
}

#[test]
fn cgroup_capability_unavailable_is_skip_unless_required() {
    let args = [OsString::from("cgroup")];
    let Some((exit, _stdout, _stderr)) = run_clean_child(&args, &LaunchOptions::default()) else {
        return;
    };
    assert_eq!(exit, ChildTermination::Exited(0));
}

#[test]
fn absent_delegated_root_is_a_typed_unavailable_error() {
    let _lock = ENVIRONMENT_LOCK.lock().expect("environment lock");
    let _environment = EnvironmentGuard::set("PANGLOSS_CGROUP_DELEGATED_ROOT", None);
    assert_unavailable_spawn("absent delegated root");
}

#[test]
fn malformed_delegated_roots_fail_without_worker_residue() {
    let _lock = ENVIRONMENT_LOCK.lock().expect("environment lock");
    for raw_root in ["", "relative", "/.", "/..", "/foo//bar", "/foo/"] {
        let snapshot = current_parent_worker_root_snapshot();
        let _environment =
            EnvironmentGuard::set("PANGLOSS_CGROUP_DELEGATED_ROOT", Some(OsStr::new(raw_root)));
        assert_unavailable_spawn(&format!("malformed delegated root {raw_root:?}"));
        if let Some((mapped, before)) = snapshot {
            let after = direct_worker_children(&mapped).expect("read current parent children");
            assert_eq!(
                after, before,
                "malformed root {raw_root:?} left worker cgroup residue"
            );
        }
    }
}

#[test]
fn delegated_root_outside_current_membership_is_unavailable() {
    let _lock = ENVIRONMENT_LOCK.lock().expect("environment lock");
    let _environment = EnvironmentGuard::set(
        "PANGLOSS_CGROUP_DELEGATED_ROOT",
        Some(OsStr::new("/.pangloss-definitely-not-an-ancestor")),
    );
    assert_unavailable_spawn("non-ancestor delegated root");
}

#[test]
fn populated_delegated_root_is_unavailable() {
    let _lock = ENVIRONMENT_LOCK.lock().expect("environment lock");
    let current = current_unified_membership().expect("read current unified cgroup membership");
    let _environment =
        EnvironmentGuard::set("PANGLOSS_CGROUP_DELEGATED_ROOT", Some(OsStr::new(&current)));
    assert_unavailable_spawn("populated delegated root");
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
    let Some((exit, stdout, stderr)) = run_clean_child(&args, &LaunchOptions::default()) else {
        return;
    };
    assert_eq!(exit, ChildTermination::Exited(0), "{stderr}");
    let actual = stdout.lines().collect::<Vec<_>>();
    let expected = values
        .iter()
        .map(|value| format!("BYTES:{}", encoded(value)))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn launch_options_preserve_clear_override_remove_and_environment() {
    let _guard = ENVIRONMENT_LOCK.lock().expect("environment lock");
    let upper = format!("PANGLOSS_CGROUP_CASE_{}", std::process::id());
    let lower = upper.to_ascii_lowercase();
    std::env::set_var(&upper, "upper inherited ✓");
    std::env::set_var(&lower, "lower inherited 漢");
    let args = [
        OsString::from("environment"),
        OsString::from(&upper),
        OsString::from(&lower),
    ];
    let Some(result) = run_clean_child(&args, &LaunchOptions::default()) else {
        std::env::remove_var(&upper);
        std::env::remove_var(&lower);
        return;
    };
    assert_eq!(
        result.1.lines().collect::<Vec<_>>(),
        [
            format!("BYTES:{}", encoded(OsStr::new("upper inherited ✓"))),
            format!("BYTES:{}", encoded(OsStr::new("lower inherited 漢"))),
        ]
    );

    let options = LaunchOptions::new()
        .env(&upper, "upper override ✓")
        .env_remove(&lower);
    let args = [
        OsString::from("environment"),
        OsString::from(&upper),
        OsString::from(&lower),
    ];
    let Some(result) = run_clean_child(&args, &options) else {
        std::env::remove_var(&upper);
        std::env::remove_var(&lower);
        return;
    };
    assert_eq!(
        result.1.lines().collect::<Vec<_>>(),
        [
            format!("BYTES:{}", encoded(OsStr::new("upper override ✓"))),
            "ABSENT".to_string()
        ]
    );

    let options = LaunchOptions::new().env_clear().env("Only", "value");
    let args = [
        OsString::from("environment"),
        OsString::from("Only"),
        OsString::from(&upper),
        OsString::from(&lower),
    ];
    let Some(result) = run_clean_child(&args, &options) else {
        std::env::remove_var(&upper);
        std::env::remove_var(&lower);
        return;
    };
    assert_eq!(
        result.1.lines().collect::<Vec<_>>(),
        ["BYTES:76,61,6c,75,65", "ABSENT", "ABSENT"]
    );
    std::env::remove_var(&upper);
    std::env::remove_var(&lower);
}

#[test]
fn launch_options_set_child_current_directory_with_spaces_and_unicode() {
    let directory = temporary_directory("cwd Unicode 漢字");
    let args = [OsString::from("cwd")];
    let Some((exit, stdout, stderr)) =
        run_clean_child(&args, &LaunchOptions::new().current_dir(&directory))
    else {
        return;
    };
    assert_eq!(exit, ChildTermination::Exited(0), "{stderr}");
    let expected = fs::canonicalize(&directory).expect("canonical test directory");
    assert_eq!(
        stdout.trim(),
        format!("BYTES:{}", encoded(expected.as_os_str()))
    );
    fs::remove_dir(&directory).expect("remove test directory");
}

#[test]
fn stdin_stdout_and_stderr_remain_connected_to_the_contained_child() {
    let args = [OsString::from("stdio")];
    let Some(mut process) = spawn_or_skip(&args, &LaunchOptions::default(), 256 << 20) else {
        return;
    };
    let mut stdio = process.take_stdio().expect("stdio once");
    stdio
        .stdin
        .write_all(b"stdin survives containment\n")
        .expect("write child stdin");
    drop(stdio.stdin);
    let (stdout_handle, stdout_receiver) = spawn_reader(stdio.stdout);
    let (stderr_handle, stderr_receiver) = spawn_reader(stdio.stderr);
    let deadline = Instant::now() + Duration::from_secs(10);
    let exit = process.reap_direct_child(deadline).expect("reap child");
    process.wait_tree_empty(deadline).expect("empty tree");
    assert!(exit.success());
    assert_eq!(
        finish_reader(stdout_handle, stdout_receiver),
        b"stdin survives containment\n"
    );
    assert_eq!(
        finish_reader(stderr_handle, stderr_receiver),
        b"stderr marker\n"
    );
    assert!(process
        .final_evidence_and_peak()
        .expect("final evidence")
        .memory_limit
        .is_none());
}

#[test]
fn child_starts_in_configured_delegated_root_on_its_first_action() {
    let Some(root) = configured_delegated_root() else {
        eprintln!("SKIP: PANGLOSS_CGROUP_DELEGATED_ROOT is not configured");
        return;
    };
    let current = current_unified_membership().expect("read current unified cgroup membership");
    assert!(
        current != root && hierarchy_contains(&root, &current),
        "current membership {current:?} must be a strict descendant of delegated root {root:?}"
    );
    let mapped_root =
        map_hierarchy_to_cgroup_mount(&root).expect("map configured delegated root mount");
    assert!(
        fs::read_to_string(mapped_root.join("cgroup.procs"))
            .expect("read delegated root cgroup.procs")
            .trim()
            .is_empty(),
        "configured delegated root must be empty before launch"
    );
    let args = [OsString::from("cgroup")];
    let Some((exit, stdout, stderr)) = run_clean_child(&args, &LaunchOptions::default()) else {
        return;
    };
    assert_eq!(exit, ChildTermination::Exited(0), "{stderr}");
    let child = stdout
        .strip_prefix("CGROUP:0::")
        .expect("fixture emitted unified cgroup membership")
        .trim();
    assert_ne!(child, current, "child must not stay in supervisor cgroup");
    assert!(
        hierarchy_contains(&root, child),
        "child cgroup {child:?} escaped delegated root {root:?}"
    );
    assert_eq!(
        parent_hierarchy(child).expect("worker cgroup has a parent"),
        root,
        "worker must be a direct child of delegated root"
    );
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
    let Some(mut process) = spawn_or_skip(&args, &LaunchOptions::default(), 256 << 20) else {
        return;
    };
    let stdio = process.take_stdio().expect("stdio once");
    drop(stdio.stdin);
    let (stdout_handle, stdout_receiver) = spawn_reader(stdio.stdout);
    let (stderr_handle, stderr_receiver) = spawn_reader(stdio.stderr);
    wait_for_file(&ready, Instant::now() + Duration::from_secs(5));
    let deadline = Instant::now() + Duration::from_secs(5);
    process.terminate_tree(deadline).expect("tree kill");
    process.reap_direct_child(deadline).expect("direct reap");
    process.wait_tree_empty(deadline).expect("tree empty");
    let stdout = finish_reader(stdout_handle, stdout_receiver);
    let stderr = finish_reader(stderr_handle, stderr_receiver);
    assert!(stdout.windows(6).any(|bytes| bytes == b"holder"));
    assert!(stderr.windows(6).any(|bytes| bytes == b"holder"));
    assert!(process
        .final_evidence_and_peak()
        .expect("final evidence")
        .memory_limit
        .is_none());
    assert!(!late.exists(), "killed descendant wrote delayed sentinel");
    fs::remove_dir_all(&directory).expect("remove test directory");
}

#[test]
fn aggregate_descendant_memory_limit_latches_hierarchical_event_evidence() {
    let directory = temporary_directory("memory");
    let ready = directory.join("ready");
    let args = [
        OsString::from("spawn-allocators"),
        ready.as_os_str().to_os_string(),
        OsString::from((64 * 1024 * 1024).to_string()),
    ];
    let memory_limit = 96 * 1024 * 1024;
    let Some(mut process) = spawn_or_skip(&args, &LaunchOptions::default(), memory_limit) else {
        return;
    };
    let stdio = process.take_stdio().expect("stdio once");
    drop(stdio.stdin);
    let (stdout_handle, stdout_receiver) = spawn_reader(stdio.stdout);
    let (stderr_handle, stderr_receiver) = spawn_reader(stdio.stderr);
    wait_for_file(&ready, Instant::now() + Duration::from_secs(5));
    let deadline = Instant::now() + Duration::from_secs(15);
    let evidence = loop {
        if let Some(evidence) = process.poll_containment().expect("poll containment") {
            break evidence;
        }
        assert!(Instant::now() < deadline, "memory evidence did not fire");
        std::thread::sleep(Duration::from_millis(5));
    };
    let MemoryLimitEvidence::LinuxCgroupV2MemoryLimitViolation {
        effective_memory_max_bytes,
        memory_peak_bytes,
        oom_kill_count_delta,
        max_event_count_delta,
    } = evidence
    else {
        panic!("Linux adapter returned non-Linux evidence");
    };
    assert!(effective_memory_max_bytes > 0);
    assert!(effective_memory_max_bytes <= memory_limit);
    assert!(memory_peak_bytes > 0);
    assert!(oom_kill_count_delta.get() > 0);
    assert!(max_event_count_delta.get() > 0);
    process.reap_direct_child(deadline).expect("direct reap");
    process.wait_tree_empty(deadline).expect("tree empty");
    let final_evidence = process.final_evidence_and_peak().expect("final evidence");
    assert!(final_evidence.memory_limit.is_some());
    assert!(final_evidence.peak_memory_charge_bytes > 0);
    let _ = finish_reader(stdout_handle, stdout_receiver);
    let _ = finish_reader(stderr_handle, stderr_receiver);
    fs::remove_dir_all(&directory).expect("remove test directory");
}

#[test]
fn direct_child_crash_cleans_up_its_living_descendant_without_memory_evidence() {
    let directory = temporary_directory("crash");
    let sentinel = directory.join("sentinel");
    let ready = directory.join("ready");
    let args = [
        OsString::from("spawn-crash"),
        sentinel.as_os_str().to_os_string(),
        ready.as_os_str().to_os_string(),
    ];
    let Some(mut process) = spawn_or_skip(&args, &LaunchOptions::default(), 256 << 20) else {
        return;
    };
    let stdio = process.take_stdio().expect("stdio once");
    drop(stdio.stdin);
    let (stdout_handle, stdout_receiver) = spawn_reader(stdio.stdout);
    let (stderr_handle, stderr_receiver) = spawn_reader(stdio.stderr);
    wait_for_file(&ready, Instant::now() + Duration::from_secs(5));
    let deadline = Instant::now() + Duration::from_secs(5);
    let exit = process.reap_direct_child(deadline).expect("reap crash");
    assert!(!exit.success());
    process
        .terminate_tree(deadline)
        .expect("kill living descendant");
    process.wait_tree_empty(deadline).expect("tree empty");
    let final_evidence = process.final_evidence_and_peak().expect("final evidence");
    assert!(final_evidence.memory_limit.is_none());
    let _ = finish_reader(stdout_handle, stdout_receiver);
    let _ = finish_reader(stderr_handle, stderr_receiver);
    std::thread::sleep(Duration::from_millis(1800));
    assert!(!sentinel.exists(), "descendant survived direct-child crash");
    fs::remove_dir_all(&directory).expect("remove test directory");
}

#[test]
fn concurrent_fork_fanout_during_termination_leaves_no_surviving_descendants() {
    let directory = temporary_directory("fanout-race");
    let survivors = directory.join("survivors");
    let ready = directory.join("ready");
    let args = [
        OsString::from("spawn-race"),
        survivors.as_os_str().to_os_string(),
        ready.as_os_str().to_os_string(),
        OsString::from("24"),
        OsString::from("10"),
    ];
    let Some(mut process) = spawn_or_skip(&args, &LaunchOptions::default(), 256 << 20) else {
        return;
    };
    let stdio = process.take_stdio().expect("stdio once");
    drop(stdio.stdin);
    let (stdout_handle, stdout_receiver) = spawn_reader(stdio.stdout);
    let (stderr_handle, stderr_receiver) = spawn_reader(stdio.stderr);
    wait_for_file(&ready, Instant::now() + Duration::from_secs(5));
    let deadline = Instant::now() + Duration::from_secs(5);
    process.terminate_tree(deadline).expect("kill fanout");
    process
        .reap_direct_child(deadline)
        .expect("reap race parent");
    process.wait_tree_empty(deadline).expect("tree empty");
    let stdout = finish_reader(stdout_handle, stdout_receiver);
    let stderr = finish_reader(stderr_handle, stderr_receiver);
    assert!(stdout.windows(5).any(|bytes| bytes == b"race-"));
    assert!(stderr.windows(5).any(|bytes| bytes == b"race-"));
    std::thread::sleep(Duration::from_millis(2300));
    let survivors = fs::read_dir(&survivors)
        .expect("survivor directory")
        .next()
        .is_none();
    assert!(survivors, "a descendant escaped the cgroup kill");
    fs::remove_dir_all(&directory).expect("remove test directory");
}

#[test]
fn ordinary_abort_and_timeout_have_no_memory_limit_evidence() {
    let args = [OsString::from("abort")];
    let Some(mut process) = spawn_or_skip(&args, &LaunchOptions::default(), 256 << 20) else {
        return;
    };
    let stdio = process.take_stdio().expect("stdio once");
    drop(stdio.stdin);
    let (stdout_handle, stdout_receiver) = spawn_reader(stdio.stdout);
    let (stderr_handle, stderr_receiver) = spawn_reader(stdio.stderr);
    let deadline = Instant::now() + Duration::from_secs(5);
    let exit = process.reap_direct_child(deadline).expect("reap abort");
    assert!(matches!(exit.termination, ChildTermination::Signaled(_)));
    process.wait_tree_empty(deadline).expect("empty tree");
    assert!(process
        .final_evidence_and_peak()
        .expect("final evidence")
        .memory_limit
        .is_none());
    let _ = finish_reader(stdout_handle, stdout_receiver);
    let _ = finish_reader(stderr_handle, stderr_receiver);

    let directory = temporary_directory("timeout");
    let ready = directory.join("ready");
    let late = directory.join("late");
    let args = [
        OsString::from("spawn-holder"),
        ready.as_os_str().to_os_string(),
        late.as_os_str().to_os_string(),
    ];
    let Some(mut process) = spawn_or_skip(&args, &LaunchOptions::default(), 256 << 20) else {
        return;
    };
    let stdio = process.take_stdio().expect("stdio once");
    drop(stdio.stdin);
    let (stdout_handle, stdout_receiver) = spawn_reader(stdio.stdout);
    let (stderr_handle, stderr_receiver) = spawn_reader(stdio.stderr);
    wait_for_file(&ready, Instant::now() + Duration::from_secs(5));
    let deadline = Instant::now() + Duration::from_secs(5);
    process.terminate_tree(deadline).expect("timeout kill");
    process.reap_direct_child(deadline).expect("reap timeout");
    process
        .wait_tree_empty(deadline)
        .expect("empty timeout tree");
    assert!(process
        .final_evidence_and_peak()
        .expect("final evidence")
        .memory_limit
        .is_none());
    let _ = finish_reader(stdout_handle, stdout_receiver);
    let _ = finish_reader(stderr_handle, stderr_receiver);
    assert!(!late.exists());
    fs::remove_dir_all(&directory).expect("remove test directory");
}

#[test]
fn completed_child_cgroup_is_removed_after_tree_cleanup() {
    let args = [OsString::from("cgroup")];
    let Some(mut process) = spawn_or_skip(&args, &LaunchOptions::default(), 256 << 20) else {
        return;
    };
    let stdio = process.take_stdio().expect("stdio once");
    drop(stdio.stdin);
    let (stdout_handle, stdout_receiver) = spawn_reader(stdio.stdout);
    let (stderr_handle, stderr_receiver) = spawn_reader(stdio.stderr);
    let deadline = Instant::now() + Duration::from_secs(10);
    process.reap_direct_child(deadline).expect("reap child");
    process.wait_tree_empty(deadline).expect("empty tree");
    let stdout = String::from_utf8(finish_reader(stdout_handle, stdout_receiver)).expect("stdout");
    let _ = finish_reader(stderr_handle, stderr_receiver);
    let relative = stdout
        .strip_prefix("CGROUP:0::")
        .expect("fixture emitted cgroup")
        .trim();
    process.final_evidence_and_peak().expect("final evidence");
    let path = map_hierarchy_to_cgroup_mount(relative)
        .expect("map completed worker cgroup through a visible mount");
    assert!(
        !path.exists(),
        "worker cgroup removal was not confirmed before final evidence returned: {}",
        path.display()
    );
    drop(process);
}
