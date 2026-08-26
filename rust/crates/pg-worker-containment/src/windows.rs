use crate::{
    compare_environment_keys, ChildTermination, ContainedStdio, ContainmentError, DirectChildExit,
    ExecutionLimits, FinalEvidence, LaunchOptions, MemoryLimitEvidence,
};
use std::ffi::{c_void, OsStr, OsString};
use std::fs::File;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, IntoRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::System::JobObjects::{
    CreateJobObjectW, JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
    JobObjectLimitViolationInformation2, JobObjectNotificationLimitInformation2,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
    JOBOBJECT_ASSOCIATE_COMPLETION_PORT, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOBOBJECT_LIMIT_VIOLATION_INFORMATION_2,
    JOBOBJECT_NOTIFICATION_LIMIT_INFORMATION_2, JOB_OBJECT_LIMIT_JOB_MEMORY,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, UpdateProcThreadAttribute, WaitForSingleObject,
    CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST, STARTF_USESTDHANDLES,
    STARTUPINFOEXW,
};
use windows_sys::Win32::System::IO::{CreateIoCompletionPort, GetQueuedCompletionStatus};

const JOB_OBJECT_MSG_NOTIFICATION_LIMIT: u32 = 11;
const MAX_NOTIFICATION_RESERVE_BYTES: usize = 64 * 1024 * 1024;

struct PipePair {
    read: OwnedHandle,
    write: OwnedHandle,
}

/// Owns every native handle for one contained worker attempt.
pub(crate) struct ContainedWorkerProcess {
    job: OwnedHandle,
    completion_port: OwnedHandle,
    process: OwnedHandle,
    process_id: u32,
    stdio: Option<ContainedStdio>,
    direct_exit: Option<DirectChildExit>,
    memory_evidence: Option<MemoryLimitEvidence>,
    peak_memory_charge_bytes: u64,
}

impl ContainedWorkerProcess {
    pub(crate) fn spawn(
        executable: &Path,
        args: &[OsString],
        options: &LaunchOptions,
        limits: ExecutionLimits,
    ) -> Result<Self, ContainmentError> {
        let memory_limit_bytes = limits.max_committed_memory_bytes();
        let memory_limit: usize =
            memory_limit_bytes
                .try_into()
                .map_err(|_| ContainmentError::Unavailable {
                    detail: "configured job memory limit does not fit this Windows target"
                        .to_string(),
                })?;

        let (job, completion_port) = create_configured_job(memory_limit)?;
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        let stdin = create_pipe(&security)?;
        let stdout = create_pipe(&security)?;
        let stderr = create_pipe(&security)?;

        // Only the three child endpoints may cross the process boundary.
        set_not_inheritable(stdin.write.raw_handle())?;
        set_not_inheritable(stdout.read.raw_handle())?;
        set_not_inheritable(stderr.read.raw_handle())?;

        let mut command_line = command_line(executable, args)?;
        let application_name = wide_path(executable)?;
        let current_directory = options.current_dir_path().map(wide_path).transpose()?;
        let environment = environment_block(options)?;
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = stdin.read.raw_handle();
        startup.StartupInfo.hStdOutput = stdout.write.raw_handle();
        startup.StartupInfo.hStdError = stderr.write.raw_handle();

        let mut attribute_size = 0usize;
        // SAFETY: the null list is the documented sizing probe; all other arguments are valid
        // pointers to local scalar storage and the required attribute count is two.
        unsafe {
            let _ = InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut attribute_size);
        }
        if attribute_size == 0 {
            return Err(win32_unavailable("sizing process attribute list"));
        }
        let words = (attribute_size + size_of::<usize>() - 1) / size_of::<usize>();
        let mut attribute_storage = vec![0usize; words];
        startup.lpAttributeList = attribute_storage.as_mut_ptr().cast();
        // SAFETY: `attribute_storage` is live, sufficiently sized, and aligned for the opaque
        // attribute list; the list is initialized exactly once before its updates.
        unsafe {
            if InitializeProcThreadAttributeList(startup.lpAttributeList, 2, 0, &mut attribute_size)
                == 0
            {
                return Err(win32_unavailable("initializing process attribute list"));
            }
        }
        let child_handles = [
            stdin.read.raw_handle(),
            stdout.write.raw_handle(),
            stderr.write.raw_handle(),
        ];
        let job_handle = job.raw_handle();
        // SAFETY: the attribute list and both value arrays remain live through CreateProcessW;
        // byte sizes match the pointed-to HANDLE values and no attribute is updated twice.
        unsafe {
            if UpdateProcThreadAttribute(
                startup.lpAttributeList,
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                (&job_handle as *const HANDLE).cast(),
                size_of::<HANDLE>(),
                null_mut(),
                null(),
            ) == 0
            {
                DeleteProcThreadAttributeList(startup.lpAttributeList);
                return Err(win32_unavailable("assigning process job-list attribute"));
            }
            if UpdateProcThreadAttribute(
                startup.lpAttributeList,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                child_handles.as_ptr().cast(),
                size_of::<HANDLE>() * child_handles.len(),
                null_mut(),
                null(),
            ) == 0
            {
                DeleteProcThreadAttributeList(startup.lpAttributeList);
                return Err(win32_unavailable("assigning inherited-handle attribute"));
            }
        }

        let mut process_info = PROCESS_INFORMATION::default();
        let mut flags = EXTENDED_STARTUPINFO_PRESENT;
        if environment.is_some() {
            flags |= CREATE_UNICODE_ENVIRONMENT;
        }
        let current_directory_ptr = current_directory
            .as_ref()
            .map_or(null(), |value| value.as_ptr());
        let environment_ptr = environment
            .as_ref()
            .map_or(null(), |value| value.as_ptr().cast::<c_void>());
        // SAFETY: all UTF-16 buffers and startup/process structures are mutable/live for the
        // duration of this synchronous call. The explicit inherit list and bInheritHandles=1
        // provide exactly the three child pipe handles.
        let created = unsafe {
            CreateProcessW(
                application_name.as_ptr(),
                command_line.as_mut_ptr(),
                null(),
                null(),
                1,
                flags,
                environment_ptr,
                current_directory_ptr,
                &startup.StartupInfo,
                &mut process_info,
            )
        };
        // SAFETY: initialization succeeded and the attribute list has no further users after
        // CreateProcessW returns.
        unsafe { DeleteProcThreadAttributeList(startup.lpAttributeList) };
        if created == 0 {
            return Err(process_creation_error());
        }

        let process = owned_handle(process_info.hProcess, "worker process")?;
        // The primary thread handle is not needed after launch.
        close_handle(process_info.hThread);
        drop(stdin.read);
        drop(stdout.write);
        drop(stderr.write);
        let stdio = ContainedStdio {
            stdin: file_from_owned(stdin.write),
            stdout: file_from_owned(stdout.read),
            stderr: file_from_owned(stderr.read),
        };
        Ok(Self {
            job,
            completion_port,
            process,
            process_id: process_info.dwProcessId,
            stdio: Some(stdio),
            direct_exit: None,
            memory_evidence: None,
            peak_memory_charge_bytes: 0,
        })
    }

    pub(crate) fn take_stdio(&mut self) -> Option<ContainedStdio> {
        self.stdio.take()
    }

    pub(crate) fn try_wait_direct_child(
        &mut self,
        deadline: Instant,
    ) -> Result<Option<DirectChildExit>, ContainmentError> {
        match self.try_wait_direct_child_raw() {
            Ok(result) => Ok(result),
            Err(error) => Err(combine_initiating_cleanup(error, self.cleanup(deadline))),
        }
    }

    fn try_wait_direct_child_raw(&mut self) -> Result<Option<DirectChildExit>, ContainmentError> {
        if let Some(exit) = self.direct_exit {
            return Ok(Some(exit));
        }
        // The signaled handle distinguishes a legal exit code 259 from STILL_ACTIVE.
        let wait = unsafe { WaitForSingleObject(self.process.raw_handle(), 0) };
        if wait == WAIT_TIMEOUT {
            return Ok(None);
        }
        if wait != WAIT_OBJECT_0 {
            return Err(win32_error("polling worker process"));
        }
        let mut exit_code = 0u32;
        // SAFETY: process is a live owned process handle and exit_code points to writable storage.
        unsafe {
            if GetExitCodeProcess(self.process.raw_handle(), &mut exit_code) == 0 {
                return Err(win32_error("querying worker exit status"));
            }
        }
        let exit = DirectChildExit {
            process_id: self.process_id,
            termination: ChildTermination::Exited(exit_code),
        };
        self.direct_exit = Some(exit);
        Ok(Some(exit))
    }

    pub(crate) fn poll_containment(
        &mut self,
        deadline: Instant,
    ) -> Result<Option<MemoryLimitEvidence>, ContainmentError> {
        match self.poll_containment_raw() {
            Ok(result) => Ok(result),
            Err(error) => Err(combine_initiating_cleanup(error, self.cleanup(deadline))),
        }
    }

    fn poll_containment_raw(&mut self) -> Result<Option<MemoryLimitEvidence>, ContainmentError> {
        let info = self.query_limits()?;
        self.observe_peak(info.PeakJobMemoryUsed as u64);
        self.poll_memory_limit_message()?;
        Ok(self.memory_evidence)
    }

    pub(crate) fn terminate_tree(&mut self, deadline: Instant) -> Result<(), ContainmentError> {
        match self.terminate_tree_raw(deadline) {
            Ok(()) => Ok(()),
            Err(error) => Err(combine_initiating_cleanup(error, self.cleanup(deadline))),
        }
    }

    fn terminate_tree_raw(&mut self, deadline: Instant) -> Result<(), ContainmentError> {
        self.terminate_job_raw()?;
        self.wait_tree_empty_raw(deadline)
    }

    pub(crate) fn wait_tree_empty(&mut self, deadline: Instant) -> Result<(), ContainmentError> {
        match self.wait_tree_empty_raw(deadline) {
            Ok(()) => Ok(()),
            Err(error) => Err(combine_initiating_cleanup(error, self.cleanup(deadline))),
        }
    }

    fn wait_tree_empty_raw(&mut self, deadline: Instant) -> Result<(), ContainmentError> {
        loop {
            let accounting = self.query_accounting()?;
            if accounting.ActiveProcesses == 0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(ContainmentError::DeadlineExceeded {
                    operation: "waiting for worker tree",
                });
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub(crate) fn reap_direct_child(
        &mut self,
        deadline: Instant,
    ) -> Result<DirectChildExit, ContainmentError> {
        match self.reap_direct_child_raw(deadline) {
            Ok(result) => Ok(result),
            Err(error) => Err(combine_initiating_cleanup(error, self.cleanup(deadline))),
        }
    }

    fn reap_direct_child_raw(
        &mut self,
        deadline: Instant,
    ) -> Result<DirectChildExit, ContainmentError> {
        if let Some(exit) = self.direct_exit {
            return Ok(exit);
        }
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ContainmentError::DeadlineExceeded {
                    operation: "reaping direct worker child",
                });
            }
            let timeout = remaining.as_millis().clamp(1, u32::MAX as u128) as u32;
            // SAFETY: process is a valid synchronization handle owned by this object.
            let result = unsafe { WaitForSingleObject(self.process.raw_handle(), timeout) };
            if result == WAIT_TIMEOUT {
                continue;
            }
            if result != WAIT_OBJECT_0 {
                return Err(win32_error("waiting for worker process"));
            }
            return self
                .try_wait_direct_child_raw()?
                .ok_or_else(|| ContainmentError::Failed {
                    detail: "worker process was signaled but has no exit status".to_string(),
                });
        }
    }

    pub(crate) fn final_evidence_and_peak(
        &mut self,
        deadline: Instant,
    ) -> Result<FinalEvidence, ContainmentError> {
        match self.final_evidence_and_peak_raw() {
            Ok(result) => Ok(result),
            Err(error) => Err(combine_initiating_cleanup(error, self.cleanup(deadline))),
        }
    }

    fn final_evidence_and_peak_raw(&mut self) -> Result<FinalEvidence, ContainmentError> {
        let mut failures = Vec::new();
        if self.direct_exit.is_none() {
            failures.push("direct worker child has not been reaped".to_string());
        }
        match self.query_limits() {
            Ok(info) => {
                self.observe_peak(info.PeakJobMemoryUsed as u64);
                if let Err(error) = self.poll_memory_limit_message() {
                    failures.push(error.to_string());
                }
            }
            Err(error) => failures.push(error.to_string()),
        }
        match self.query_accounting() {
            Ok(accounting) if accounting.ActiveProcesses != 0 => failures
                .push("worker job still has active processes during finalization".to_string()),
            Ok(_) => {}
            Err(error) => failures.push(error.to_string()),
        }
        if failures.is_empty() {
            Ok(FinalEvidence {
                memory_limit: self.memory_evidence,
                peak_memory_charge_bytes: self.peak_memory_charge_bytes,
            })
        } else {
            Err(ContainmentError::Failed {
                detail: format!("worker finalization failed: {}", failures.join("; ")),
            })
        }
    }

    fn cleanup(&mut self, deadline: Instant) -> Result<(), ContainmentError> {
        let mut failures = Vec::new();
        if let Err(error) = self.terminate_job_raw() {
            failures.push(error.to_string());
        }
        if let Err(error) = self.wait_tree_empty_raw(deadline) {
            failures.push(error.to_string());
        }
        if self.direct_exit.is_none() {
            if let Err(error) = self.reap_direct_child_raw(deadline) {
                failures.push(error.to_string());
            }
        }
        if let Err(error) = self.final_evidence_and_peak_raw() {
            failures.push(error.to_string());
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(ContainmentError::Failed {
                detail: format!("worker cleanup failed: {}", failures.join("; ")),
            })
        }
    }

    fn terminate_job_raw(&self) -> Result<(), ContainmentError> {
        // SAFETY: the job handle is owned and valid for this object's lifetime.
        unsafe {
            if TerminateJobObject(self.job.raw_handle(), 1) == 0 {
                return Err(win32_error("terminating worker job"));
            }
        }
        Ok(())
    }

    fn query_limits(&self) -> Result<JOBOBJECT_EXTENDED_LIMIT_INFORMATION, ContainmentError> {
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // SAFETY: info is correctly sized writable storage for the selected information class;
        // the job handle is owned and valid.
        unsafe {
            if QueryInformationJobObject(
                self.job.raw_handle(),
                JobObjectExtendedLimitInformation,
                (&mut info as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                null_mut(),
            ) == 0
            {
                return Err(win32_error("querying worker job memory evidence"));
            }
        }
        Ok(info)
    }

    fn query_accounting(&self) -> Result<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, ContainmentError> {
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: info is correctly sized writable storage for the selected information class;
        // the job handle is owned and valid.
        unsafe {
            if QueryInformationJobObject(
                self.job.raw_handle(),
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                null_mut(),
            ) == 0
            {
                return Err(win32_error("querying worker job population"));
            }
        }
        Ok(info)
    }

    fn observe_peak(&mut self, peak: u64) {
        self.peak_memory_charge_bytes = self.peak_memory_charge_bytes.max(peak);
    }

    fn poll_memory_limit_message(&mut self) -> Result<(), ContainmentError> {
        if self.memory_evidence.is_some() {
            return Ok(());
        }
        loop {
            let mut bytes = 0u32;
            let mut key = 0usize;
            let mut overlapped = null_mut();
            // SAFETY: all output pointers refer to live local storage; zero timeout makes this a
            // non-blocking drain of the completion port owned by this process.
            let received = unsafe {
                GetQueuedCompletionStatus(
                    self.completion_port.raw_handle(),
                    &mut bytes,
                    &mut key,
                    &mut overlapped,
                    0,
                )
            };
            if received == 0 {
                // Only WAIT_TIMEOUT denotes an intact but empty evidence channel.
                if unsafe { GetLastError() } == WAIT_TIMEOUT {
                    break;
                }
                return Err(win32_error("polling worker job completion port"));
            }
            if bytes == JOB_OBJECT_MSG_NOTIFICATION_LIMIT {
                break;
            }
        }
        let violation = self.query_limit_violation()?;
        if violation.ViolationLimitFlags & JOB_OBJECT_LIMIT_JOB_MEMORY != 0 {
            self.observe_peak(violation.JobMemory);
            self.memory_evidence = Some(
                MemoryLimitEvidence::WindowsObservedJobMemoryLimitViolation {
                    notification_limit_bytes: notification_limit(
                        self.query_limits()?.JobMemoryLimit,
                    ) as u64,
                    peak_job_memory_used_bytes: self.peak_memory_charge_bytes,
                },
            );
            // SAFETY: the queried violation flag is native proof that the configured aggregate
            // notification threshold fired; all descendants are members of this owned job.
            unsafe {
                if TerminateJobObject(self.job.raw_handle(), 1) == 0 {
                    return Err(win32_error("terminating memory-limited worker job"));
                }
            }
        }
        Ok(())
    }

    fn query_limit_violation(
        &self,
    ) -> Result<JOBOBJECT_LIMIT_VIOLATION_INFORMATION_2, ContainmentError> {
        let mut info = JOBOBJECT_LIMIT_VIOLATION_INFORMATION_2::default();
        // SAFETY: info is correctly sized writable storage and the job handle remains owned.
        unsafe {
            if QueryInformationJobObject(
                self.job.raw_handle(),
                JobObjectLimitViolationInformation2,
                (&mut info as *mut JOBOBJECT_LIMIT_VIOLATION_INFORMATION_2).cast(),
                size_of::<JOBOBJECT_LIMIT_VIOLATION_INFORMATION_2>() as u32,
                null_mut(),
            ) == 0
            {
                return Err(win32_error("querying worker job limit violation"));
            }
        }
        Ok(info)
    }
}

fn create_configured_job(
    memory_limit: usize,
) -> Result<(OwnedHandle, OwnedHandle), ContainmentError> {
    // SAFETY: null security descriptor and null name request a private unnamed job object.
    let raw = unsafe { CreateJobObjectW(null(), null()) };
    if raw.is_null() {
        return Err(win32_unavailable("creating worker job"));
    }
    // SAFETY: this handle was newly returned by CreateJobObjectW and ownership transfers once.
    let job = unsafe { OwnedHandle::from_raw_handle(raw.cast()) };
    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    info.BasicLimitInformation.LimitFlags =
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_JOB_MEMORY;
    info.JobMemoryLimit = memory_limit;
    // SAFETY: info is initialized and remains live for the synchronous configuration call.
    unsafe {
        if SetInformationJobObject(
            job.raw_handle(),
            JobObjectExtendedLimitInformation,
            (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            return Err(win32_unavailable("configuring worker job limits"));
        }
    }
    let mut notification = JOBOBJECT_NOTIFICATION_LIMIT_INFORMATION_2::default();
    notification.LimitFlags = JOB_OBJECT_LIMIT_JOB_MEMORY;
    // Reserve bounded headroom because a commit rejected at the hard ceiling cannot notify first.
    notification.Anonymous1.JobMemoryLimit = notification_limit(memory_limit) as u64;
    // SAFETY: notification is initialized for the selected information class and remains live
    // for the synchronous call. This channel provides guaranteed delivery of limit evidence.
    unsafe {
        if SetInformationJobObject(
            job.raw_handle(),
            JobObjectNotificationLimitInformation2,
            (&notification as *const JOBOBJECT_NOTIFICATION_LIMIT_INFORMATION_2).cast(),
            size_of::<JOBOBJECT_NOTIFICATION_LIMIT_INFORMATION_2>() as u32,
        ) == 0
        {
            return Err(win32_unavailable(
                "configuring worker job memory notification",
            ));
        }
    }
    let completion_port = create_completion_port()?;
    let association = JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
        CompletionKey: null_mut(),
        CompletionPort: completion_port.raw_handle(),
    };
    // SAFETY: association points to live initialized storage; both handles remain owned after
    // this synchronous call and the completion port is associated before any child is launched.
    unsafe {
        if SetInformationJobObject(
            job.raw_handle(),
            windows_sys::Win32::System::JobObjects::JobObjectAssociateCompletionPortInformation,
            (&association as *const JOBOBJECT_ASSOCIATE_COMPLETION_PORT).cast(),
            size_of::<JOBOBJECT_ASSOCIATE_COMPLETION_PORT>() as u32,
        ) == 0
        {
            return Err(win32_unavailable("associating worker job completion port"));
        }
    }
    Ok((job, completion_port))
}

fn notification_limit(memory_limit: usize) -> usize {
    let reserve = (memory_limit / 2).min(MAX_NOTIFICATION_RESERVE_BYTES);
    memory_limit.saturating_sub(reserve).max(1)
}

fn create_completion_port() -> Result<OwnedHandle, ContainmentError> {
    // SAFETY: INVALID_HANDLE_VALUE requests a new completion port; no pointers are involved.
    let raw = unsafe { CreateIoCompletionPort((-1isize) as HANDLE, null_mut(), 0, 1) };
    if raw.is_null() {
        return Err(win32_unavailable("creating worker job completion port"));
    }
    // SAFETY: this handle was newly returned by CreateIoCompletionPort and ownership transfers
    // exactly once.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.cast()) })
}

fn create_pipe(attributes: &SECURITY_ATTRIBUTES) -> Result<PipePair, ContainmentError> {
    let mut read = null_mut();
    let mut write = null_mut();
    // SAFETY: both output pointers and the immutable security attributes point to live storage;
    // CreatePipe initializes each returned handle atomically or reports failure.
    let ok = unsafe { CreatePipe(&mut read, &mut write, attributes, 0) };
    if ok == 0 {
        return Err(win32_unavailable("creating worker stdio pipe"));
    }
    let read = owned_handle(read, "worker pipe read endpoint")?;
    let write = match owned_handle(write, "worker pipe write endpoint") {
        Ok(handle) => handle,
        Err(error) => {
            return Err(error);
        }
    };
    Ok(PipePair { read, write })
}

fn set_not_inheritable(handle: HANDLE) -> Result<(), ContainmentError> {
    // SAFETY: handle was returned by CreatePipe and remains owned by the caller.
    unsafe {
        if SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) == 0 {
            return Err(win32_error("marking parent pipe endpoint non-inheritable"));
        }
    }
    Ok(())
}

fn owned_handle(raw: HANDLE, what: &str) -> Result<OwnedHandle, ContainmentError> {
    if raw.is_null() {
        return Err(win32_error(what));
    }
    // SAFETY: this raw handle is newly returned by Win32 and ownership is transferred exactly
    // once to OwnedHandle; every error path before this point leaves no wrapper to double-close.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.cast()) })
}

fn file_from_owned(handle: OwnedHandle) -> File {
    let raw = handle.into_raw_handle();
    // SAFETY: the OwnedHandle is consumed, transferring its one live pipe handle to File exactly
    // once; no owner remains after into_raw_handle.
    unsafe { File::from_raw_handle(raw) }
}

fn close_handle(raw: HANDLE) {
    if !raw.is_null() {
        // SAFETY: this handle is the temporary primary-thread handle returned by CreateProcessW;
        // no other owner is created for it.
        unsafe {
            let _ = CloseHandle(raw);
        }
    }
}

fn win32_error(context: &str) -> ContainmentError {
    // SAFETY: GetLastError reads the calling thread's Win32 error slot and takes no pointers.
    let code = unsafe { GetLastError() };
    ContainmentError::Failed {
        detail: format!("{context} (Win32 error {code})"),
    }
}

fn win32_unavailable(context: &str) -> ContainmentError {
    // SAFETY: GetLastError reads the calling thread's Win32 error slot and takes no pointers.
    let code = unsafe { GetLastError() };
    ContainmentError::Unavailable {
        detail: format!("{context} (Win32 error {code})"),
    }
}

fn process_creation_error() -> ContainmentError {
    // ERROR_ACCESS_DENIED is the documented failure for an incompatible host-job nesting
    // arrangement; it must fail closed as unavailable rather than suggest an unmanaged retry.
    // SAFETY: GetLastError reads the calling thread's Win32 error slot and takes no pointers.
    let code = unsafe { GetLastError() };
    if code == 5 {
        ContainmentError::Unavailable {
            detail: format!("creating contained worker process (Win32 error {code})"),
        }
    } else {
        ContainmentError::Failed {
            detail: format!("creating contained worker process (Win32 error {code})"),
        }
    }
}

fn combine_initiating_cleanup(
    initiating: ContainmentError,
    cleanup: Result<(), ContainmentError>,
) -> ContainmentError {
    match cleanup {
        Ok(()) => initiating,
        Err(error) => ContainmentError::Failed {
            detail: format!("{initiating}; cleanup failed: {error}"),
        },
    }
}

fn wide_path(path: &Path) -> Result<Vec<u16>, ContainmentError> {
    wide_os(path.as_os_str(), "path")
}

fn wide_os(value: &OsStr, what: &str) -> Result<Vec<u16>, ContainmentError> {
    if value.encode_wide().any(|unit| unit == 0) {
        return Err(ContainmentError::Failed {
            detail: format!("{what} contains an embedded NUL"),
        });
    }
    let mut wide: Vec<u16> = value.encode_wide().collect();
    wide.push(0);
    Ok(wide)
}

fn command_line(executable: &Path, args: &[OsString]) -> Result<Vec<u16>, ContainmentError> {
    let mut values = Vec::with_capacity(args.len() + 1);
    values.push(executable.as_os_str().to_os_string());
    values.extend(args.iter().cloned());
    let mut line = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            line.push(' ' as u16);
        }
        line.extend(quote_windows_arg(value)?);
    }
    line.push(0);
    Ok(line)
}

fn quote_windows_arg(value: &OsStr) -> Result<Vec<u16>, ContainmentError> {
    let units: Vec<u16> = value.encode_wide().collect();
    if units.iter().any(|&unit| unit == 0) {
        return Err(ContainmentError::Failed {
            detail: "worker argument contains an embedded NUL".to_string(),
        });
    }
    let outer_quotes = units.is_empty()
        || units
            .iter()
            .any(|&unit| unit == ' ' as u16 || unit == '\t' as u16);
    let mut quoted = Vec::with_capacity(units.len() + 2);
    if outer_quotes {
        quoted.push('"' as u16);
    }
    let mut backslashes = 0usize;
    for unit in units {
        if unit == '\\' as u16 {
            backslashes += 1;
        } else if unit == '"' as u16 {
            quoted.extend(std::iter::repeat_n('\\' as u16, backslashes * 2 + 1));
            quoted.push('"' as u16);
            backslashes = 0;
        } else {
            quoted.extend(std::iter::repeat_n('\\' as u16, backslashes));
            quoted.push(unit);
            backslashes = 0;
        }
    }
    let trailing_backslashes = if outer_quotes {
        backslashes * 2
    } else {
        backslashes
    };
    quoted.extend(std::iter::repeat_n('\\' as u16, trailing_backslashes));
    if outer_quotes {
        quoted.push('"' as u16);
    }
    Ok(quoted)
}

fn environment_block(options: &LaunchOptions) -> Result<Option<Vec<u16>>, ContainmentError> {
    if !options.clear_environment() && options.environment().is_empty() {
        return Ok(None);
    }
    let mut values: Vec<(OsString, OsString)> = if options.clear_environment() {
        Vec::new()
    } else {
        std::env::vars_os().collect()
    };
    for (key, value) in options.environment() {
        if let Some((_, existing)) = values
            .iter_mut()
            .find(|(existing_key, _)| compare_environment_keys(existing_key, key).is_eq())
        {
            if let Some(value) = value {
                *existing = value.clone();
            } else {
                *existing = OsString::new();
            }
        } else if let Some(value) = value {
            values.push((key.clone(), value.clone()));
        }
    }
    values.retain(|(key, _)| {
        !options.environment().iter().any(|(removed, value)| {
            value.is_none() && compare_environment_keys(removed, key).is_eq()
        })
    });
    values.sort_by(|left, right| compare_environment_keys(&left.0, &right.0));
    let mut block = Vec::new();
    for (key, value) in values {
        let key = wide_os(&key, "environment key")?;
        let value = wide_os(&value, "environment value")?;
        block.extend_from_slice(&key[..key.len() - 1]);
        block.push('=' as u16);
        block.extend_from_slice(&value);
    }
    if block.last().copied() != Some(0) {
        block.push(0);
    }
    block.push(0);
    Ok(Some(block))
}

trait RawHandleExt {
    fn raw_handle(&self) -> HANDLE;
}

impl RawHandleExt for OwnedHandle {
    fn raw_handle(&self) -> HANDLE {
        std::os::windows::io::AsRawHandle::as_raw_handle(self).cast()
    }
}

#[cfg(test)]
mod tests {
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
}
