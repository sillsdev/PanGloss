use crate::{
    ChildTermination, ContainedStdio, ContainmentError, DirectChildExit, ExecutionLimits,
    FinalEvidence, LaunchOptions, MemoryLimitEvidence,
};
use libc::{c_char, c_int, c_void};
use std::ffi::{CString, OsStr, OsString};
use std::fs;
use std::io::{Read, Write};
use std::mem::size_of;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const CLONE_PIDFD: u64 = 0x0000_1000;
const CLONE_INTO_CGROUP: u64 = 1u64 << 33;
const O_PATH: c_int = 0o10000000;
const O_NOFOLLOW: c_int = 0o00400000;
const AT_REMOVEDIR: c_int = 0x200;
const PR_SET_PDEATHSIG: c_int = 1;
const CHILD_CGROUP_PREFIX: &str = ".pangloss-worker-";
static NEXT_CGROUP_ID: AtomicU64 = AtomicU64::new(1);

#[repr(C)]
struct CloneArgs {
    flags: u64,
    pidfd: u64,
    child_tid: u64,
    parent_tid: u64,
    exit_signal: u64,
    stack: u64,
    stack_size: u64,
    tls: u64,
    set_tid: u64,
    set_tid_size: u64,
    cgroup: u64,
}

struct PipePair {
    read: OwnedFd,
    write: OwnedFd,
}

#[derive(Default, Clone, Copy)]
struct MemoryEvents {
    max: u64,
    oom_kill: u64,
}

struct ExecSpec {
    candidates: Vec<CString>,
    argv_storage: Vec<CString>,
    argv: Vec<*const c_char>,
    env_storage: Vec<CString>,
    envp: Vec<*const c_char>,
    cwd: Option<OwnedFd>,
}

struct DelegatedRoot {
    directory: OwnedFd,
}

struct CgroupMount {
    root: String,
    mountpoint: PathBuf,
}

/// Linux cgroup-v2 containment beneath the explicitly configured delegated root.
pub(crate) struct ContainedWorkerProcess {
    parent_cgroup: OwnedFd,
    cgroup: OwnedFd,
    cgroup_name: CString,
    process_id: libc::pid_t,
    _pidfd: OwnedFd,
    stdio: Option<ContainedStdio>,
    direct_exit: Option<DirectChildExit>,
    memory_evidence: Option<MemoryLimitEvidence>,
    baseline_events: MemoryEvents,
    effective_memory_max_bytes: u64,
    peak_memory_charge_bytes: u64,
    removed: bool,
}

impl ContainedWorkerProcess {
    pub(crate) fn spawn(
        executable: &Path,
        args: &[OsString],
        options: &LaunchOptions,
        limits: ExecutionLimits,
    ) -> Result<Self, ContainmentError> {
        let cgroup = create_cgroup(limits.max_committed_memory_bytes())?;
        let mut guard = SpawnGuard::new(cgroup);
        let baseline_events = match read_events(guard.child_fd()) {
            Ok(events) => events,
            Err(error) => {
                return Err(guard.fail(error));
            }
        };
        let mut spec = match prepare_exec(executable, args, options) {
            Ok(spec) => spec,
            Err(error) => {
                return Err(guard.fail(error));
            }
        };
        let stdin = match pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                return Err(guard.fail(error));
            }
        };
        let stdout = match pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                return Err(guard.fail(error));
            }
        };
        let stderr = match pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                return Err(guard.fail(error));
            }
        };
        let error_pipe = match pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                return Err(guard.fail(error));
            }
        };
        // SAFETY: getpid has no pointer arguments and is safe to call in the launching parent.
        let parent_pid = unsafe { libc::getpid() };
        let mut pidfd: c_int = -1;
        let clone_args = CloneArgs {
            flags: CLONE_INTO_CGROUP | CLONE_PIDFD,
            pidfd: (&mut pidfd as *mut c_int) as u64,
            child_tid: 0,
            parent_tid: 0,
            exit_signal: libc::SIGCHLD as u64,
            stack: 0,
            stack_size: 0,
            tls: 0,
            set_tid: 0,
            set_tid_size: 0,
            cgroup: guard.child_fd() as u64,
        };
        // SAFETY: clone_args points to live, repr(C) storage and all referenced fds remain open
        // until the synchronous syscall returns; the kernel copies the structure immediately.
        let result = unsafe {
            libc::syscall(
                libc::SYS_clone3,
                &clone_args as *const CloneArgs,
                size_of::<CloneArgs>(),
            )
        };
        if result == 0 {
            // Allocation or locking here could deadlock before exec.
            // SAFETY: clone3 preserves the live allocations/fds; child_exec uses only prebuilt
            // pointers and async-signal-safe raw operations.
            unsafe {
                child_exec(
                    &mut spec,
                    stdin.read.as_raw_fd(),
                    stdout.write.as_raw_fd(),
                    stderr.write.as_raw_fd(),
                    [
                        stdin.write.as_raw_fd(),
                        stdout.read.as_raw_fd(),
                        stderr.read.as_raw_fd(),
                        error_pipe.read.as_raw_fd(),
                    ],
                    parent_pid,
                    error_pipe.write.as_raw_fd(),
                )
            }
        }
        if result < 0 {
            let error = std::io::Error::last_os_error();
            let initiating = unavailable(format!(
                "clone3 with cgroup placement failed: {error}"
            ));
            return Err(guard.fail(initiating));
        }
        let process_id = result as libc::pid_t;
        guard.record_process(process_id);
        let pidfd = if pidfd >= 0 {
            // SAFETY: clone3 returned this as a new owned pidfd, and no other owner exists.
            unsafe { OwnedFd::from_raw_fd(pidfd) }
        } else {
            let initiating = unavailable("clone3 did not return a pidfd");
            return Err(guard.fail(initiating));
        };
        drop(stdin.read);
        drop(stdout.write);
        drop(stderr.write);
        drop(error_pipe.write);
        // SAFETY: into_raw_fd transfers sole ownership of the live read end to File.
        let mut error_file = unsafe { std::fs::File::from_raw_fd(error_pipe.read.into_raw_fd()) };
        let mut launch_error = Vec::new();
        if let Err(error) = error_file.read_to_end(&mut launch_error) {
            let initiating = failed(format!("reading child launch status: {error}"));
            return Err(guard.fail(initiating));
        }
        if !launch_error.is_empty() {
            let errno = launch_error
                .get(..size_of::<c_int>())
                .and_then(|bytes| bytes.try_into().ok())
                .map(c_int::from_ne_bytes)
                .unwrap_or(libc::EIO);
            let initiating = failed(format!(
                "worker failed before exec: {}",
                std::io::Error::from_raw_os_error(errno)
            ));
            return Err(guard.fail(initiating));
        }
        let stdio = ContainedStdio {
            // SAFETY: each into_raw_fd transfers sole ownership of its live parent endpoint.
            stdin: unsafe { std::fs::File::from_raw_fd(stdin.write.into_raw_fd()) },
            // SAFETY: each into_raw_fd transfers sole ownership of its live parent endpoint.
            stdout: unsafe { std::fs::File::from_raw_fd(stdout.read.into_raw_fd()) },
            // SAFETY: each into_raw_fd transfers sole ownership of its live parent endpoint.
            stderr: unsafe { std::fs::File::from_raw_fd(stderr.read.into_raw_fd()) },
        };
        let cgroup = guard.disarm();
        let effective_memory_max_bytes = cgroup.effective_memory_max_bytes;
        let parent_cgroup = cgroup.parent;
        let child_cgroup = cgroup.child;
        let cgroup_name = cgroup.name;
        Ok(Self {
            parent_cgroup,
            cgroup: child_cgroup,
            cgroup_name,
            process_id,
            _pidfd: pidfd,
            stdio: Some(stdio),
            direct_exit: None,
            memory_evidence: None,
            baseline_events,
            effective_memory_max_bytes,
            peak_memory_charge_bytes: 0,
            removed: false,
        })
    }

    pub(crate) fn take_stdio(&mut self) -> Option<ContainedStdio> {
        self.stdio.take()
    }

    pub(crate) fn try_wait_direct_child(
        &mut self,
    ) -> Result<Option<DirectChildExit>, ContainmentError> {
        if let Some(exit) = self.direct_exit {
            return Ok(Some(exit));
        }
        let mut status = 0;
        loop {
            // SAFETY: process_id is the live direct child and status points to writable storage.
            let result = unsafe { libc::waitpid(self.process_id, &mut status, libc::WNOHANG) };
            if result == 0 {
                return Ok(None);
            }
            if result == self.process_id {
                return Ok(Some(self.record_exit(status)?));
            }
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(failed(format!("polling direct child: {error}")));
        }
    }

    pub(crate) fn poll_containment(
        &mut self,
    ) -> Result<Option<MemoryLimitEvidence>, ContainmentError> {
        self.observe_memory_peak()?;
        let events = read_events(self.cgroup.as_raw_fd())?;
        self.latch_memory_evidence(events)?;
        Ok(self.memory_evidence)
    }

    pub(crate) fn terminate_tree(&mut self, deadline: Instant) -> Result<(), ContainmentError> {
        if let Err(error) = write_at(self.cgroup.as_raw_fd(), "cgroup.kill", b"1") {
            return Err(combine_initiating_cleanup(error, self.cleanup(deadline)));
        }
        if let Err(error) = self.wait_tree_empty(deadline) {
            return Err(combine_initiating_cleanup(error, self.cleanup(deadline)));
        }
        Ok(())
    }

    pub(crate) fn wait_tree_empty(&mut self, deadline: Instant) -> Result<(), ContainmentError> {
        loop {
            let populated = read_cgroup_event(self.cgroup.as_raw_fd(), "populated")?;
            if populated == 0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(ContainmentError::DeadlineExceeded {
                    operation: "waiting for worker cgroup",
                });
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub(crate) fn reap_direct_child(
        &mut self,
        deadline: Instant,
    ) -> Result<DirectChildExit, ContainmentError> {
        if let Some(exit) = self.direct_exit {
            return Ok(exit);
        }
        loop {
            if let Some(exit) = self.try_wait_direct_child()? {
                return Ok(exit);
            }
            if Instant::now() >= deadline {
                return Err(ContainmentError::DeadlineExceeded {
                    operation: "reaping direct worker child",
                });
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub(crate) fn final_evidence_and_peak(&mut self) -> Result<FinalEvidence, ContainmentError> {
        let mut failures = Vec::new();
        if let Err(error) = self.observe_memory_peak() {
            failures.push(error.to_string());
        }
        match read_events(self.cgroup.as_raw_fd()) {
            Ok(events) => {
                if let Err(error) = self.latch_memory_evidence(events) {
                    failures.push(error.to_string());
                }
            }
            Err(error) => failures.push(error.to_string()),
        }
        match read_cgroup_event(self.cgroup.as_raw_fd(), "populated") {
            Ok(0) => {}
            Ok(_) => failures.push("worker cgroup is still populated during finalization".into()),
            Err(error) => failures.push(error.to_string()),
        }
        if let Err(error) = self.remove_cgroup() {
            failures.push(error.to_string());
        }
        if failures.is_empty() {
            Ok(FinalEvidence {
                memory_limit: self.memory_evidence,
                peak_memory_charge_bytes: self.peak_memory_charge_bytes,
            })
        } else {
            Err(failed(format!(
                "worker finalization failed: {}",
                failures.join("; ")
            )))
        }
    }

    fn cleanup(&mut self, deadline: Instant) -> Result<(), ContainmentError> {
        let report = {
            let mut context = CleanupContext {
                parent: self.parent_cgroup.as_raw_fd(),
                child: self.cgroup.as_raw_fd(),
                name: &self.cgroup_name,
                process_id: Some(self.process_id),
                direct_exit: self.direct_exit,
                baseline_events: Some(self.baseline_events),
                effective_memory_max_bytes: Some(self.effective_memory_max_bytes),
                memory_evidence: self.memory_evidence,
                peak_memory_charge_bytes: self.peak_memory_charge_bytes,
                removed: self.removed,
            };
            context.run(deadline)
        };
        self.direct_exit = report.direct_exit;
        self.memory_evidence = report.memory_evidence;
        self.peak_memory_charge_bytes = report.peak_memory_charge_bytes;
        self.removed = report.removed;
        report.result
    }

    fn record_exit(&mut self, status: c_int) -> Result<DirectChildExit, ContainmentError> {
        let termination = decode_wait_status(status)?;
        let exit = DirectChildExit {
            process_id: self.process_id as u32,
            termination,
        };
        self.direct_exit = Some(exit);
        Ok(exit)
    }

    fn observe_memory_peak(&mut self) -> Result<(), ContainmentError> {
        self.peak_memory_charge_bytes = self
            .peak_memory_charge_bytes
            .max(read_u64_at(self.cgroup.as_raw_fd(), "memory.peak")?);
        Ok(())
    }

    fn latch_memory_evidence(&mut self, events: MemoryEvents) -> Result<(), ContainmentError> {
        latch_memory_evidence_at(
            self.cgroup.as_raw_fd(),
            self.baseline_events,
            self.effective_memory_max_bytes,
            self.peak_memory_charge_bytes,
            &mut self.memory_evidence,
            events,
        )
    }

    fn remove_cgroup(&mut self) -> Result<(), ContainmentError> {
        if self.removed {
            return Ok(());
        }
        // SAFETY: parent_cgroup is an owned directory fd and cgroup_name is a live NUL-terminated
        // name; unlinkat performs no ownership transfer of either pointer.
        let result = unsafe {
            libc::unlinkat(
                self.parent_cgroup.as_raw_fd(),
                self.cgroup_name.as_ptr(),
                AT_REMOVEDIR,
            )
        };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            return Err(failed(format!("removing worker cgroup: {error}")));
        }
        self.removed = true;
        Ok(())
    }
}

impl Drop for ContainedWorkerProcess {
    fn drop(&mut self) {
        if !self.removed {
            let _ = self.cleanup(Instant::now() + Duration::from_secs(5));
        }
    }
}

struct CreatedCgroup {
    parent: OwnedFd,
    child: OwnedFd,
    name: CString,
    effective_memory_max_bytes: u64,
}

struct SpawnGuard {
    cgroup: Option<CreatedCgroup>,
    process_id: Option<libc::pid_t>,
    // Explicit failures consume the emergency cleanup attempt; Drop handles only unwinds.
    cleanup_claimed: bool,
}

impl SpawnGuard {
    fn new(cgroup: CreatedCgroup) -> Self {
        Self {
            cgroup: Some(cgroup),
            process_id: None,
            cleanup_claimed: false,
        }
    }

    fn child_fd(&self) -> RawFd {
        self.cgroup
            .as_ref()
            .expect("spawn guard retains cgroup")
            .child
            .as_raw_fd()
    }

    fn record_process(&mut self, process_id: libc::pid_t) {
        self.process_id = Some(process_id);
    }

    fn fail(&mut self, initiating: ContainmentError) -> ContainmentError {
        self.cleanup_claimed = true;
        combine_initiating_cleanup(initiating, self.cleanup())
    }

    fn disarm(mut self) -> CreatedCgroup {
        self.cgroup.take().expect("spawn guard retains cgroup")
    }

    fn cleanup(&mut self) -> Result<(), ContainmentError> {
        let Some(cgroup) = self.cgroup.as_ref() else {
            return Ok(());
        };
        let result = if let Some(process_id) = self.process_id {
            let report = CleanupContext {
                parent: cgroup.parent.as_raw_fd(),
                child: cgroup.child.as_raw_fd(),
                name: &cgroup.name,
                process_id: Some(process_id),
                direct_exit: None,
                baseline_events: None,
                effective_memory_max_bytes: None,
                memory_evidence: None,
                peak_memory_charge_bytes: 0,
                removed: false,
            }
            .run(Instant::now() + Duration::from_secs(5));
            if report.removed {
                self.cgroup.take();
            }
            report.result
        } else {
            let result = remove_named_cgroup(cgroup.parent.as_raw_fd(), &cgroup.name);
            if result.is_ok() {
                self.cgroup.take();
            }
            result
        };
        result
    }
}

impl Drop for SpawnGuard {
    fn drop(&mut self) {
        if !self.cleanup_claimed {
            let _ = self.cleanup();
        }
    }
}

struct CleanupReport {
    result: Result<(), ContainmentError>,
    direct_exit: Option<DirectChildExit>,
    memory_evidence: Option<MemoryLimitEvidence>,
    peak_memory_charge_bytes: u64,
    removed: bool,
}

struct CleanupContext<'a> {
    parent: RawFd,
    child: RawFd,
    name: &'a CString,
    process_id: Option<libc::pid_t>,
    direct_exit: Option<DirectChildExit>,
    baseline_events: Option<MemoryEvents>,
    effective_memory_max_bytes: Option<u64>,
    memory_evidence: Option<MemoryLimitEvidence>,
    peak_memory_charge_bytes: u64,
    removed: bool,
}

impl CleanupContext<'_> {
    fn run(mut self, deadline: Instant) -> CleanupReport {
        let mut failures = Vec::new();
        if self.process_id.is_some() {
            if let Err(error) = write_at(self.child, "cgroup.kill", b"1") {
                failures.push(error.to_string());
            }
            if let Err(error) = wait_cgroup_empty(self.child, deadline) {
                failures.push(error.to_string());
            }
            if self.direct_exit.is_none() {
                match reap_process(self.process_id.expect("process id is present"), deadline) {
                    Ok(status) => match decode_wait_status(status) {
                        Ok(termination) => {
                            self.direct_exit = Some(DirectChildExit {
                                process_id: self.process_id.expect("process id is present") as u32,
                                termination,
                            });
                        }
                        Err(error) => failures.push(error.to_string()),
                    },
                    Err(error) => failures.push(error.to_string()),
                }
            }
            match read_u64_at(self.child, "memory.peak") {
                Ok(peak) => self.peak_memory_charge_bytes = self.peak_memory_charge_bytes.max(peak),
                Err(error) => failures.push(error.to_string()),
            }
            match read_events(self.child) {
                Ok(events) => {
                    if let (Some(baseline), Some(effective)) =
                        (self.baseline_events, self.effective_memory_max_bytes)
                    {
                        if let Err(error) = latch_memory_evidence_at(
                            self.child,
                            baseline,
                            effective,
                            self.peak_memory_charge_bytes,
                            &mut self.memory_evidence,
                            events,
                        ) {
                            failures.push(error.to_string());
                        }
                    }
                }
                Err(error) => failures.push(error.to_string()),
            }
        }
        if let Err(error) = remove_named_cgroup(self.parent, self.name) {
            failures.push(error.to_string());
        } else {
            self.removed = true;
        }
        let result = if failures.is_empty() {
            Ok(())
        } else {
            Err(failed(format!("worker cleanup failed: {}", failures.join("; "))))
        };
        CleanupReport {
            result,
            direct_exit: self.direct_exit,
            memory_evidence: self.memory_evidence,
            peak_memory_charge_bytes: self.peak_memory_charge_bytes,
            removed: self.removed,
        }
    }
}

fn create_cgroup(memory_limit: u64) -> Result<CreatedCgroup, ContainmentError> {
    let delegated = DelegatedRoot::resolve()?;
    let parent = delegated.directory;
    let mut id = NEXT_CGROUP_ID.fetch_add(1, Ordering::Relaxed);
    let (name, child) = loop {
        let candidate = CString::new(format!(
            "{CHILD_CGROUP_PREFIX}{}-{}",
            // SAFETY: getpid has no pointer arguments and is safe to call in the parent.
            unsafe { libc::getpid() },
            id
        ))
        .map_err(|_| failed("generated cgroup name contains NUL"))?;
        // SAFETY: parent fd is owned and candidate remains live and NUL-terminated for the call.
        let made = unsafe { libc::mkdirat(parent.as_raw_fd(), candidate.as_ptr(), 0o700) };
        if made == 0 {
            let fd = match open_at(
                parent.as_raw_fd(),
                &candidate,
                libc::O_RDONLY | libc::O_DIRECTORY,
            ) {
                Ok(fd) => fd,
                Err(error) => {
                    return Err(combine_initiating_cleanup(
                        error,
                        remove_named_cgroup(parent.as_raw_fd(), &candidate),
                    ));
                }
            };
            break (candidate, fd);
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EEXIST) {
            id = id.wrapping_add(1);
            continue;
        }
        return Err(unavailable(format!(
            "creating delegated worker cgroup: {error}"
        )));
    };
    let effective = match configure_child_cgroup(child.as_raw_fd(), memory_limit) {
        Ok(effective) => effective,
        Err(error) => {
            return Err(combine_initiating_cleanup(
                error,
                remove_named_cgroup(parent.as_raw_fd(), &name),
            ));
        }
    };
    Ok(CreatedCgroup {
        parent,
        child,
        name,
        effective_memory_max_bytes: effective,
    })
}

impl DelegatedRoot {
    fn resolve() -> Result<Self, ContainmentError> {
        let configured = std::env::var_os("PANGLOSS_CGROUP_DELEGATED_ROOT")
            .ok_or_else(|| unavailable("PANGLOSS_CGROUP_DELEGATED_ROOT is required"))?;
        let hierarchy_path = parse_canonical_hierarchy_path(&configured)?;
        let current = read_unified_membership()?;
        require_strict_descendant(&current, &hierarchy_path)?;
        let mount = most_specific_covering_mount(&hierarchy_path)?;
        let directory = open_mapped_root(&mount, &hierarchy_path)?;
        require_empty_root_and_memory_delegation(directory.as_raw_fd())?;
        Ok(Self {
            directory,
        })
    }
}

fn parse_canonical_hierarchy_path(value: &OsStr) -> Result<PathBuf, ContainmentError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes[0] != b'/' || (bytes.len() > 1 && bytes.ends_with(b"/")) {
        return Err(unavailable(
            "PANGLOSS_CGROUP_DELEGATED_ROOT must be an absolute canonical hierarchy path",
        ));
    }
    if bytes.windows(2).any(|pair| pair == b"//") {
        return Err(unavailable(
            "PANGLOSS_CGROUP_DELEGATED_ROOT contains duplicate separators",
        ));
    }
    for component in bytes.split(|byte| *byte == b'/').filter(|part| !part.is_empty()) {
        if component == b"." || component == b".." || component.contains(&0) {
            return Err(unavailable(
                "PANGLOSS_CGROUP_DELEGATED_ROOT contains an invalid component",
            ));
        }
    }
    Ok(PathBuf::from(value))
}

fn read_unified_membership() -> Result<PathBuf, ContainmentError> {
    fs::read_to_string("/proc/self/cgroup")
        .map_err(|error| unavailable(format!("reading /proc/self/cgroup: {error}")))?
        .lines()
        .find_map(|line| {
            let mut fields = line.splitn(3, ':');
            (fields.next() == Some("0") && fields.next() == Some(""))
                .then(|| fields.next().map(PathBuf::from))
                .flatten()
        })
        .ok_or_else(|| unavailable("the process is not in a unified cgroup"))
}

fn require_strict_descendant(current: &Path, root: &Path) -> Result<(), ContainmentError> {
    let current = current.to_string_lossy();
    let root = root.to_string_lossy();
    let prefix = if root == "/" {
        "/".to_owned()
    } else {
        format!("{root}/")
    };
    if current == root || !current.starts_with(&prefix) {
        return Err(unavailable(
            "supervisor membership is not a strict descendant of the delegated root",
        ));
    }
    Ok(())
}

fn most_specific_covering_mount(path: &Path) -> Result<CgroupMount, ContainmentError> {
    let mut matches = Vec::new();
    for line in fs::read_to_string("/proc/self/mountinfo")
        .map_err(|error| unavailable(format!("reading /proc/self/mountinfo: {error}")))?
        .lines()
    {
        let Some(mount) = parse_cgroup2_mount(line) else { continue };
        if hierarchy_is_under(path, Path::new(&mount.root)) {
            matches.push(mount);
        }
    }
    let Some(longest) = matches
        .iter()
        .map(|mount| component_count(Path::new(&mount.root)))
        .max()
    else {
        return Err(unavailable("no visible cgroup-v2 mount covers the delegated root"));
    };
    let selected = matches
        .into_iter()
        .filter(|mount| component_count(Path::new(&mount.root)) == longest)
        .collect::<Vec<_>>();
    if selected.len() != 1 {
        return Err(unavailable("cgroup-v2 mount mapping is ambiguous"));
    }
    Ok(selected.into_iter().next().expect("one selected mount"))
}

fn hierarchy_is_under(path: &Path, root: &Path) -> bool {
    if root == Path::new("/") {
        return path.is_absolute();
    }
    path == root || path.strip_prefix(root).is_ok()
}

fn component_count(path: &Path) -> usize {
    path.components().filter(|component| matches!(component, std::path::Component::Normal(_))).count()
}

fn open_mapped_root(mount: &CgroupMount, hierarchy: &Path) -> Result<OwnedFd, ContainmentError> {
    let mountpoint = cstring_os(mount.mountpoint.as_os_str())?;
    // SAFETY: mountpoint is a live NUL-terminated path and the returned directory fd is newly owned.
    let mount_fd = unsafe {
        libc::open(
            mountpoint.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if mount_fd < 0 {
        return Err(unavailable(format!("opening cgroup-v2 mount: {}", std::io::Error::last_os_error())));
    }
    // SAFETY: mount returned a fresh directory descriptor with no Rust owner; this transfer
    // gives exactly one OwnedFd ownership of it.
    let mut current = unsafe { OwnedFd::from_raw_fd(mount_fd) };
    let root = Path::new(&mount.root);
    let suffix = if root == Path::new("/") {
        hierarchy.strip_prefix("/").unwrap_or(hierarchy)
    } else {
        hierarchy.strip_prefix(root).map_err(|_| unavailable("mount root mapping escaped"))?
    };
    for component in suffix.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(unavailable("delegated root mapping contains traversal"));
        };
        let name = cstring_os(component)?;
        // SAFETY: current is an owned directory fd and name remains live for this openat call.
        let fd = unsafe {
            libc::openat(
                current.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(unavailable(format!("opening mapped delegated root: {}", std::io::Error::last_os_error())));
        }
        // SAFETY: openat returned a fresh descriptor and current is replaced only after ownership transfer.
        current = unsafe { OwnedFd::from_raw_fd(fd) };
    }
    Ok(current)
}

fn require_empty_root_and_memory_delegation(root: RawFd) -> Result<(), ContainmentError> {
    if read_text_at(root, "cgroup.type")?.trim() != "domain" {
        return Err(unavailable("configured delegated root is not a normal domain cgroup"));
    }
    if !read_text_at(root, "cgroup.procs")?.trim().is_empty() {
        return Err(unavailable("configured delegated root cgroup.procs is not empty"));
    }
    if !read_text_at(root, "cgroup.subtree_control")?
        .split_whitespace()
        .any(|value| value == "memory")
    {
        return Err(unavailable("configured delegated root does not delegate memory"));
    }
    Ok(())
}

fn configure_child_cgroup(child: RawFd, requested: u64) -> Result<u64, ContainmentError> {
    if read_text_at(child, "cgroup.type")?.trim() != "domain" {
        return Err(unavailable(
            "generated worker cgroup is not a domain cgroup",
        ));
    }
    for surface in [
        "memory.max",
        "memory.oom.group",
        "cgroup.events",
        "memory.events",
        "memory.peak",
    ] {
        let _ = open_at(child, &cstring(surface)?, libc::O_RDONLY)?;
    }
    let _ = open_at(child, &cstring("cgroup.procs")?, libc::O_WRONLY)?;
    let _ = open_at(child, &cstring("cgroup.kill")?, libc::O_WRONLY)?;
    write_at(child, "memory.max", requested.to_string().as_bytes())?;
    write_at(child, "memory.oom.group", b"1")?;
    if let Some(swap) = open_optional_at(child, &cstring("memory.swap.max")?, libc::O_WRONLY)? {
        // SAFETY: into_raw_fd transfers sole ownership of the opened swap control file.
        let mut file = unsafe { std::fs::File::from_raw_fd(swap.into_raw_fd()) };
        file.write_all(b"0").map_err(|error| {
            unavailable(format!("writing cgroup surface memory.swap.max: {error}"))
        })?;
    }
    let effective = read_u64_at(child, "memory.max")?;
    if effective == 0 || effective > requested {
        return Err(unavailable(format!(
            "kernel effective memory.max {effective} is outside requested positive cap {requested}"
        )));
    }
    Ok(effective)
}

fn remove_named_cgroup(parent: RawFd, name: &CString) -> Result<(), ContainmentError> {
    // SAFETY: cgroup parent fd and name are owned/live and the kernel reads the name immediately.
    let result = unsafe {
        libc::unlinkat(parent, name.as_ptr(), AT_REMOVEDIR)
    };
    if result != 0 {
        return Err(failed(format!(
            "removing worker cgroup: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

fn combine_initiating_cleanup(
    initiating: ContainmentError,
    cleanup: Result<(), ContainmentError>,
) -> ContainmentError {
    match cleanup {
        Ok(()) => initiating,
        Err(error) => failed(format!("{initiating}; cleanup failed: {error}")),
    }
}

fn latch_memory_evidence_at(
    child: RawFd,
    baseline: MemoryEvents,
    effective_memory_max_bytes: u64,
    peak_memory_charge_bytes: u64,
    memory_evidence: &mut Option<MemoryLimitEvidence>,
    events: MemoryEvents,
) -> Result<(), ContainmentError> {
    if memory_evidence.is_some() {
        return Ok(());
    }
    let max_delta = events.max.saturating_sub(baseline.max);
    let oom_delta = events.oom_kill.saturating_sub(baseline.oom_kill);
    let (Some(max_delta), Some(oom_delta)) = (
        std::num::NonZeroU64::new(max_delta),
        std::num::NonZeroU64::new(oom_delta),
    ) else {
        return Ok(());
    };
    *memory_evidence = Some(MemoryLimitEvidence::LinuxCgroupV2MemoryLimitViolation {
        effective_memory_max_bytes,
        memory_peak_bytes: peak_memory_charge_bytes,
        oom_kill_count_delta: oom_delta,
        max_event_count_delta: max_delta,
    });
    write_at(child, "cgroup.kill", b"1")
}

fn wait_cgroup_empty(cgroup: RawFd, deadline: Instant) -> Result<(), ContainmentError> {
    loop {
        if read_cgroup_event(cgroup, "populated")? == 0 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(ContainmentError::DeadlineExceeded {
                operation: "waiting for failed worker cgroup",
            });
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn parse_cgroup2_mount(line: &str) -> Option<CgroupMount> {
    let (left, right) = line.split_once(" - ")?;
    let left = left.split_whitespace().collect::<Vec<_>>();
    let right = right.split_whitespace().collect::<Vec<_>>();
    if right.first().copied() != Some("cgroup2") || left.len() < 5 {
        return None;
    }
    Some(CgroupMount {
        root: decode_mountinfo_field(left[3]),
        mountpoint: PathBuf::from(decode_mountinfo_field(left[4])),
    })
}

fn decode_mountinfo_field(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let code = chars.by_ref().take(3).collect::<String>();
            match code.as_str() {
                "040" => decoded.push(' '),
                "011" => decoded.push('\t'),
                "012" => decoded.push('\n'),
                "134" => decoded.push('\\'),
                _ => {
                    decoded.push('\\');
                    decoded.push_str(&code);
                }
            }
        } else {
            decoded.push(ch);
        }
    }
    decoded
}

fn prepare_exec(
    executable: &Path,
    args: &[OsString],
    options: &LaunchOptions,
) -> Result<ExecSpec, ContainmentError> {
    let mut argv_storage = Vec::with_capacity(args.len() + 1);
    argv_storage.push(cstring_os(executable.as_os_str())?);
    for arg in args {
        argv_storage.push(cstring_os(arg.as_os_str())?);
    }
    let argv = argv_storage
        .iter()
        .map(|value| value.as_ptr())
        .chain([std::ptr::null()])
        .collect();
    let environment = prepare_environment(options)?;
    let envp = environment
        .iter()
        .map(|value| value.as_ptr())
        .chain([std::ptr::null()])
        .collect();
    let cwd = options
        .current_dir_path()
        .map(open_current_dir)
        .transpose()?;
    Ok(ExecSpec {
        candidates: vec![cstring_os(executable.as_os_str())?],
        argv_storage,
        argv,
        env_storage: environment,
        envp,
        cwd,
    })
}

fn open_current_dir(path: &Path) -> Result<OwnedFd, ContainmentError> {
    let path = cstring_os(path.as_os_str())?;
    // SAFETY: path is live NUL-terminated storage; caller paths intentionally follow symlinks.
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(unavailable(format!(
            "opening worker current directory: {}",
            std::io::Error::last_os_error()
        )));
    }
    // SAFETY: open returned a fresh descriptor with no Rust owner.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn prepare_environment(options: &LaunchOptions) -> Result<Vec<CString>, ContainmentError> {
    let mut values = if options.clear_environment() {
        Vec::new()
    } else {
        std::env::vars_os().collect::<Vec<_>>()
    };
    for (key, value) in options.environment() {
        values.retain(|(old_key, _)| old_key.as_bytes() != key.as_bytes());
        if let Some(value) = value {
            values.push((key.clone(), value.clone()));
        }
    }
    values
        .into_iter()
        .map(|(key, value)| {
            let mut bytes = key.as_bytes().to_vec();
            bytes.push(b'=');
            bytes.extend_from_slice(value.as_bytes());
            CString::new(bytes).map_err(|_| failed("environment contains an interior NUL"))
        })
        .collect()
}

// SAFETY: The clone3 child supplies prebuilt strings and valid inherited fds; the body uses only
// async-signal-safe operations until execve or _exit.
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn child_exec(
    spec: &mut ExecSpec,
    stdin: RawFd,
    stdout: RawFd,
    stderr: RawFd,
    parent_fds: [RawFd; 4],
    parent_pid: libc::pid_t,
    launch_error: RawFd,
) -> ! {
    if libc::prctl(PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
        report_child_error(launch_error, *libc::__errno_location());
    }
    if libc::getppid() != parent_pid {
        report_child_error(launch_error, libc::ESRCH);
    }
    for fd in parent_fds {
        libc::close(fd);
    }
    if libc::dup2(stdin, libc::STDIN_FILENO) < 0
        || libc::dup2(stdout, libc::STDOUT_FILENO) < 0
        || libc::dup2(stderr, libc::STDERR_FILENO) < 0
    {
        report_child_error(launch_error, libc::EIO);
    }
    for fd in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
            report_child_error(launch_error, *libc::__errno_location());
        }
    }
    for fd in [stdin, stdout, stderr] {
        if fd > libc::STDERR_FILENO {
            libc::close(fd);
        }
    }
    if let Some(cwd) = &spec.cwd {
        if libc::fchdir(cwd.as_raw_fd()) != 0 {
            report_child_error(launch_error, *libc::__errno_location());
        }
    }
    let envp = spec.envp.as_ptr();
    for candidate in &spec.candidates {
        libc::execve(candidate.as_ptr(), spec.argv.as_ptr(), envp);
    }
    report_child_error(launch_error, *libc::__errno_location());
}

// SAFETY: `launch_error` is the live child-only writable end of the CLOEXEC status pipe; the body
// uses only async-signal-safe operations until _exit.
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn report_child_error(launch_error: RawFd, errno: c_int) -> ! {
    let bytes = errno.to_ne_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        let result = libc::write(
            launch_error,
            bytes[offset..].as_ptr().cast::<c_void>(),
            bytes.len() - offset,
        );
        if result > 0 {
            offset += result as usize;
            continue;
        }
        if result < 0 && *libc::__errno_location() == libc::EINTR {
            continue;
        }
        break;
    }
    libc::_exit(127);
}

fn pipe() -> Result<PipePair, ContainmentError> {
    let mut fds = [0; 2];
    // SAFETY: fds points to two writable c_int slots and pipe2 initializes them atomically.
    let result = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
    if result != 0 {
        return Err(unavailable(format!(
            "creating worker stdio pipe: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(PipePair {
        // SAFETY: pipe2 returned fresh descriptors with no existing Rust owners.
        read: unsafe { OwnedFd::from_raw_fd(fds[0]) },
        // SAFETY: pipe2 returned fresh descriptors with no existing Rust owners.
        write: unsafe { OwnedFd::from_raw_fd(fds[1]) },
    })
}

fn open_at(dir: RawFd, path: &CString, flags: c_int) -> Result<OwnedFd, ContainmentError> {
    // SAFETY: dir is an owned directory fd and path is live NUL-terminated storage.
    let fd = unsafe { libc::openat(dir, path.as_ptr(), flags | O_NOFOLLOW | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(unavailable(format!(
            "opening cgroup surface: {}",
            std::io::Error::last_os_error()
        )));
    }
    // SAFETY: openat returned a fresh descriptor transferred to this OwnedFd.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn open_optional_at(
    dir: RawFd,
    path: &CString,
    flags: c_int,
) -> Result<Option<OwnedFd>, ContainmentError> {
    // SAFETY: dir is an owned directory fd and path is live NUL-terminated storage.
    let fd = unsafe { libc::openat(dir, path.as_ptr(), flags | O_NOFOLLOW | libc::O_CLOEXEC) };
    if fd >= 0 {
        // SAFETY: openat returned a fresh descriptor transferred to this OwnedFd.
        return Ok(Some(unsafe { OwnedFd::from_raw_fd(fd) }));
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ENOENT) {
        Ok(None)
    } else {
        Err(unavailable(format!("opening cgroup surface: {error}")))
    }
}

fn read_text_at(dir: RawFd, name: &str) -> Result<String, ContainmentError> {
    let fd = open_at(dir, &cstring(name)?, libc::O_RDONLY)?;
    // SAFETY: into_raw_fd transfers sole ownership of the opened cgroup surface to File.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd.into_raw_fd()) };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| failed(format!("reading cgroup surface {name}: {error}")))?;
    String::from_utf8(bytes).map_err(|_| failed(format!("cgroup surface {name} is not UTF-8")))
}

fn read_u64_at(dir: RawFd, name: &str) -> Result<u64, ContainmentError> {
    let value = read_text_at(dir, name)?;
    value
        .trim()
        .parse()
        .map_err(|_| failed(format!("cgroup surface {name} is not numeric")))
}

fn read_cgroup_event(dir: RawFd, wanted: &str) -> Result<u64, ContainmentError> {
    parse_cgroup_event_value(&read_text_at(dir, "cgroup.events")?, wanted)
        .map_err(|detail| failed(format!("cgroup.events {detail} for {wanted}")))
}

fn parse_cgroup_event_value(text: &str, wanted: &str) -> Result<u64, &'static str> {
    let mut value = None;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else {
            return Err("contains a malformed record");
        };
        let Some(raw_value) = fields.next() else {
            return Err("contains a truncated record");
        };
        if fields.next().is_some() {
            return Err("contains an extra field");
        }
        let numeric = raw_value
            .parse()
            .map_err(|_| "record value is not numeric")?;
        if name == wanted {
            if value.is_some() {
                return Err("contains a duplicate requested record");
            }
            value = Some(numeric);
        }
    }
    value.ok_or("has no requested record")
}

fn read_events(dir: RawFd) -> Result<MemoryEvents, ContainmentError> {
    let mut events = MemoryEvents::default();
    let mut saw_max = false;
    let mut saw_oom_kill = false;
    for line in read_text_at(dir, "memory.events")?.lines() {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else { continue };
        let Some(value) = fields.next() else {
            return Err(failed("memory.events contains a truncated record"));
        };
        if fields.next().is_some() {
            return Err(failed("memory.events contains an invalid record"));
        }
        match name {
            "max" => {
                if saw_max {
                    return Err(failed("memory.events contains duplicate max records"));
                }
                events.max = value
                    .parse()
                    .map_err(|_| failed("memory.events max is not numeric"))?;
                saw_max = true;
            }
            "oom_kill" => {
                if saw_oom_kill {
                    return Err(failed("memory.events contains duplicate oom_kill records"));
                }
                events.oom_kill = value
                    .parse()
                    .map_err(|_| failed("memory.events oom_kill is not numeric"))?;
                saw_oom_kill = true;
            }
            _ => {}
        }
    }
    if !saw_max || !saw_oom_kill {
        return Err(failed("memory.events lacks required max and oom_kill records"));
    }
    Ok(events)
}

fn decode_wait_status(status: c_int) -> Result<ChildTermination, ContainmentError> {
    if libc::WIFEXITED(status) {
        Ok(ChildTermination::Exited(libc::WEXITSTATUS(status) as u32))
    } else if libc::WIFSIGNALED(status) {
        Ok(ChildTermination::Signaled(libc::WTERMSIG(status) as i32))
    } else {
        Err(failed("waitpid returned a non-terminal child status"))
    }
}

fn reap_process(process_id: libc::pid_t, deadline: Instant) -> Result<c_int, ContainmentError> {
    let mut status = 0;
    loop {
        // SAFETY: process_id is the owned direct child and status points to writable storage.
        let result = unsafe { libc::waitpid(process_id, &mut status, libc::WNOHANG) };
        if result == process_id {
            return Ok(status);
        }
        if result == 0 {
            if Instant::now() >= deadline {
                return Err(ContainmentError::DeadlineExceeded {
                    operation: "reaping worker child",
                });
            }
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EINTR) {
            if Instant::now() >= deadline {
                return Err(ContainmentError::DeadlineExceeded {
                    operation: "reaping worker child",
                });
            }
            continue;
        }
        return Err(failed(format!("reaping worker child: {error}")));
    }
}

fn write_at(dir: RawFd, name: &str, value: &[u8]) -> Result<(), ContainmentError> {
    let fd = open_at(dir, &cstring(name)?, libc::O_WRONLY)?;
    // SAFETY: into_raw_fd transfers sole ownership of the opened cgroup surface to File.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd.into_raw_fd()) };
    file.write_all(value)
        .map_err(|error| unavailable(format!("writing cgroup surface {name}: {error}")))
}

fn cstring(value: &str) -> Result<CString, ContainmentError> {
    CString::new(value.as_bytes()).map_err(|_| failed("cgroup surface name contains NUL"))
}

fn cstring_os(value: &OsStr) -> Result<CString, ContainmentError> {
    CString::new(value.as_bytes()).map_err(|_| failed("path, argument, or key contains NUL"))
}

fn unavailable(detail: impl Into<String>) -> ContainmentError {
    ContainmentError::Unavailable {
        detail: detail.into(),
    }
}

fn failed(detail: impl Into<String>) -> ContainmentError {
    ContainmentError::Failed {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_wait_status, parse_cgroup_event_value};

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
        assert!(decode_wait_status(0x37).is_err());
    }
}
