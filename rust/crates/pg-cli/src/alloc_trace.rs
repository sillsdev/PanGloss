//! `alloc-trace` feature (default off): a counting `#[global_allocator]` ground-truth wrapper.
//! See docs/research/word-memory-trace.md for what this is checked against and why.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn bump(delta: usize) {
    let live = LIVE.fetch_add(delta, Ordering::Relaxed) + delta;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

fn drop_bytes(delta: usize) {
    LIVE.fetch_sub(delta, Ordering::Relaxed);
}

pub struct CountingAlloc;

// SAFETY: every method delegates the actual allocation work to `System`, which already upholds
// `GlobalAlloc`'s contract; this wrapper only adds non-mutating byte-count bookkeeping around it.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            bump(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        drop_bytes(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            if new_size >= layout.size() {
                bump(new_size - layout.size());
            } else {
                drop_bytes(layout.size() - new_size);
            }
        }
        new_ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            bump(layout.size());
        }
        ptr
    }
}

/// Current live (not-yet-deallocated) byte count.
pub fn live_bytes() -> usize {
    LIVE.load(Ordering::Relaxed)
}

/// Peak live byte count since the last `reset_peak` (or process start).
pub fn peak_bytes() -> usize {
    PEAK.load(Ordering::Relaxed)
}

/// Rebase the peak to the current live count, so the next word's peak reflects only new growth.
pub fn reset_peak() {
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}
