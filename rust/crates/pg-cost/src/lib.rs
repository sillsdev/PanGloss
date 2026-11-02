//! Deterministic parse cost: fixed weights over work units, bytes allocated and registers copied.
//!
//! Cost is a report. No cap, deadline or parse decision reads it, so it cannot change a parse.
//! The weights are constants of the code, not of a grammar: see `docs/design/parse-cost.md`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

pub mod calibrate;

/// Instructions per work unit.
pub const WORK_WEIGHT: u64 = 500;
/// Instructions per requested byte.
pub const BYTE_WEIGHT: u64 = 2;
/// Instructions per FST register copied.
pub const REGISTER_WEIGHT: u64 = 330;

static ENABLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static BYTES: Cell<u64> = const { Cell::new(0) };
    static REGISTERS: Cell<u64> = const { Cell::new(0) };
}

/// The allocator a host must install for bytes to count; it forwards everything to `System`.
pub struct CountingAllocator;

#[inline]
fn add_bytes(size: usize) {
    if ENABLED.load(Ordering::Relaxed) {
        let _ = BYTES.try_with(|bytes| bytes.set(bytes.get().saturating_add(size as u64)));
    }
}

// SAFETY: every method delegates to `System`, which upholds `GlobalAlloc`'s contract; the wrapper
// only adds to a thread-local counter, and that counter has no destructor and never allocates.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        add_bytes(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        add_bytes(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        add_bytes(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
}

/// Counting could not be turned on, so a cost would silently omit allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocatorNotInstalled;

impl fmt::Display for AllocatorNotInstalled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("this build cannot measure parse cost: `CountingAllocator` is not installed")
    }
}

impl std::error::Error for AllocatorNotInstalled {}

/// Turns counting on, proving by a real allocation that bytes reach the counter.
pub fn enable() -> Result<(), AllocatorNotInstalled> {
    ENABLED.store(true, Ordering::Relaxed);
    let before = BYTES.with(Cell::get);
    drop(std::hint::black_box(vec![0u8; 64]));
    if BYTES.with(Cell::get) == before {
        ENABLED.store(false, Ordering::Relaxed);
        return Err(AllocatorNotInstalled);
    }
    Ok(())
}

/// Counts FST registers copied on this thread.
#[inline]
pub fn add_registers(count: usize) {
    if ENABLED.load(Ordering::Relaxed) {
        REGISTERS.with(|registers| registers.set(registers.get().saturating_add(count as u64)));
    }
}

/// The counts behind one word's cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub work: u64,
    pub bytes: u64,
    pub registers: u64,
}

impl Counts {
    /// The word's cost in instruction-sized units under the fixed weights.
    pub fn cost(&self) -> u64 {
        self.work
            .saturating_mul(WORK_WEIGHT)
            .saturating_add(self.bytes.saturating_mul(BYTE_WEIGHT))
            .saturating_add(self.registers.saturating_mul(REGISTER_WEIGHT))
    }
}

/// Measures the bytes and registers of one parse on the current thread.
pub struct Meter {
    bytes: u64,
    registers: u64,
}

impl Meter {
    /// `None` when counting is off, so a caller cannot mistake an unmeasured parse for a cheap one.
    pub fn start() -> Option<Self> {
        ENABLED.load(Ordering::Relaxed).then(|| Self {
            bytes: BYTES.with(Cell::get),
            registers: REGISTERS.with(Cell::get),
        })
    }

    pub fn finish(self, work: usize) -> Counts {
        Counts {
            work: work as u64,
            bytes: BYTES.with(Cell::get) - self.bytes,
            registers: REGISTERS.with(Cell::get) - self.registers,
        }
    }
}

/// CPU time this thread has used, or `None` where the platform cannot report it.
pub fn thread_cpu_time() -> Option<std::time::Duration> {
    platform::thread_cpu_time()
}

#[cfg(unix)]
#[allow(unsafe_code)]
mod platform {
    pub fn thread_cpu_time() -> Option<std::time::Duration> {
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `time` is a valid, writable timespec for the duration of the call.
        let status = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
        (status == 0).then(|| std::time::Duration::new(time.tv_sec as u64, time.tv_nsec as u32))
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod platform {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThread() -> isize;
        fn GetThreadTimes(
            thread: isize,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }

    pub fn thread_cpu_time() -> Option<std::time::Duration> {
        let (mut creation, mut exit, mut kernel, mut user) = Default::default();
        // SAFETY: every pointer is a valid, writable FileTime for the duration of the call, and
        // `GetCurrentThread` returns a pseudo handle that needs no closing.
        let status = unsafe {
            GetThreadTimes(
                GetCurrentThread(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        };
        let ticks = |time: FileTime| (u64::from(time.high) << 32) | u64::from(time.low);
        (status != 0).then(|| std::time::Duration::from_nanos((ticks(kernel) + ticks(user)) * 100))
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    pub fn thread_cpu_time() -> Option<std::time::Duration> {
        None
    }
}

#[cfg(test)]
mod tests;
