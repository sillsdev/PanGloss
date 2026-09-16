//! `HC_WS_STATS=1` (Windows only, off by default): real OS working set via `K32GetProcessMemoryInfo`, see `docs/research/memory-measurement-repair.md`.

use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// One `K32GetProcessMemoryInfo` reading: `peak_working_set_bytes` is the process-lifetime running maximum, `current_working_set_bytes` is the instant's resident set.
pub struct WorkingSet {
    pub peak_working_set_bytes: u64,
    pub current_working_set_bytes: u64,
}

/// `None` only if the Win32 call itself reports failure -- surfaced, never silently zeroed.
pub fn snapshot() -> Option<WorkingSet> {
    let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    // SAFETY: `GetCurrentProcess` returns a valid pseudo-handle that needs no cleanup; `counters` is
    // a stack-allocated, correctly-`cb`-sized `PROCESS_MEMORY_COUNTERS` that the call fills in
    // place, matching the documented `K32GetProcessMemoryInfo` contract.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    if ok == 0 {
        return None;
    }
    Some(WorkingSet {
        peak_working_set_bytes: counters.PeakWorkingSetSize as u64,
        current_working_set_bytes: counters.WorkingSetSize as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reports_nonzero_and_peak_at_least_current() {
        // Touch some heap so the working set is not degenerate before reading it.
        let _live: Vec<u8> = vec![0u8; 8 * 1024 * 1024];
        let ws = snapshot().expect("K32GetProcessMemoryInfo must succeed for our own process");
        assert!(ws.current_working_set_bytes > 0);
        assert!(ws.peak_working_set_bytes >= ws.current_working_set_bytes);
    }
}
