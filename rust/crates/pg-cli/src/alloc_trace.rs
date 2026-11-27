//! `alloc-trace` feature (default off): a counting `#[global_allocator]` ground-truth wrapper.
//! See docs/research/word-memory-trace.md for what this is checked against and why.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

const NB: usize = 130;
static HIST: [AtomicUsize; NB] = [const { AtomicUsize::new(0) }; NB];
static HBYTES: [AtomicUsize; NB] = [const { AtomicUsize::new(0) }; NB];
static SNAP: [AtomicUsize; NB] = [const { AtomicUsize::new(0) }; NB];
static SNAPB: [AtomicUsize; NB] = [const { AtomicUsize::new(0) }; NB];
static SNAP_LIVE: AtomicUsize = AtomicUsize::new(0);
static SNAP_RSS: AtomicUsize = AtomicUsize::new(0);
static BUSY: AtomicUsize = AtomicUsize::new(0);

fn bucket(size: usize) -> usize {
    if size <= 512 {
        size.div_ceil(8)
    } else {
        65 + (usize::BITS - size.leading_zeros()) as usize
    }
}

fn rss_kb() -> usize {
    use std::io::Read;
    let mut buf = [0u8; 128];
    let Ok(mut f) = std::fs::File::open("/proc/self/statm") else {
        return 0;
    };
    let n = f.read(&mut buf).unwrap_or(0);
    let s = std::str::from_utf8(&buf[..n]).unwrap_or("");
    s.split_whitespace()
        .nth(1)
        .and_then(|v| v.parse::<usize>().ok())
        .map_or(0, |p| p * 4)
}

static PK_TODAY: AtomicUsize = AtomicUsize::new(0);
static PK_TODAY_SL: AtomicUsize = AtomicUsize::new(0);
static PK_TODAY_SA: AtomicUsize = AtomicUsize::new(0);
static PK_ARENA: AtomicUsize = AtomicUsize::new(0);
static PK_ARENA_SL: AtomicUsize = AtomicUsize::new(0);
static PK_ARENA_SA: AtomicUsize = AtomicUsize::new(0);
static PK_ARENA_TODAY: AtomicUsize = AtomicUsize::new(0);

fn bump(delta: usize) {
    let live = LIVE.fetch_add(delta, Ordering::Relaxed) + delta;
    PEAK.fetch_max(live, Ordering::Relaxed);
    use pg_rules::trail::probe::{EXT_LIVE, STEP_ALLOC, STEP_LIVE};
    EXT_LIVE.store(live, Ordering::Relaxed);
    let sl = STEP_LIVE.load(Ordering::Relaxed);
    let sa = STEP_ALLOC.load(Ordering::Relaxed);
    let today = live + 8 * sl;
    if today > PK_TODAY.load(Ordering::Relaxed) {
        PK_TODAY.store(today, Ordering::Relaxed);
        PK_TODAY_SL.store(sl, Ordering::Relaxed);
        PK_TODAY_SA.store(sa, Ordering::Relaxed);
    }
    let arena = (live + 16 * sa).saturating_sub(40 * sl);
    if arena > PK_ARENA.load(Ordering::Relaxed) {
        PK_ARENA.store(arena, Ordering::Relaxed);
        PK_ARENA_SL.store(sl, Ordering::Relaxed);
        PK_ARENA_SA.store(sa, Ordering::Relaxed);
        PK_ARENA_TODAY.store(today, Ordering::Relaxed);
    }
    if live > SNAP_LIVE.load(Ordering::Relaxed) + (2 << 20) && BUSY.swap(1, Ordering::Relaxed) == 0
    {
        SNAP_LIVE.store(live, Ordering::Relaxed);
        for i in 0..NB {
            SNAP[i].store(HIST[i].load(Ordering::Relaxed), Ordering::Relaxed);
            SNAPB[i].store(HBYTES[i].load(Ordering::Relaxed), Ordering::Relaxed);
        }
        SNAP_RSS.store(rss_kb(), Ordering::Relaxed);
        BUSY.store(0, Ordering::Relaxed);
    }
}

fn hist_add(size: usize) {
    let b = bucket(size);
    HIST[b].fetch_add(1, Ordering::Relaxed);
    HBYTES[b].fetch_add(size, Ordering::Relaxed);
}

fn hist_sub(size: usize) {
    let b = bucket(size);
    HIST[b].fetch_sub(1, Ordering::Relaxed);
    HBYTES[b].fetch_sub(size, Ordering::Relaxed);
}

pub fn report() {
    let m = |a: &AtomicUsize| a.load(Ordering::Relaxed) as f64 / 1048576.0;
    eprintln!(
        "PROJ today_peak_mb={:.1} (steps_live={} steps_alloc_total={}) | arena_peak_mb={:.1} (steps_live={} steps_alloc={} today_at_that_moment_mb={:.1}) | total_steps_alloc_final={}",
        m(&PK_TODAY),
        PK_TODAY_SL.load(Ordering::Relaxed),
        PK_TODAY_SA.load(Ordering::Relaxed),
        m(&PK_ARENA),
        PK_ARENA_SL.load(Ordering::Relaxed),
        PK_ARENA_SA.load(Ordering::Relaxed),
        m(&PK_ARENA_TODAY),
        pg_rules::trail::probe::STEP_ALLOC.load(Ordering::Relaxed),
    );
    pg_rules::trail::probe::ts("end");
    eprintln!(
        "ALLOCPROBE snap_live_mb={} snap_rss_mb={} peak_mb={} end_rss_mb={}",
        SNAP_LIVE.load(Ordering::Relaxed) >> 20,
        SNAP_RSS.load(Ordering::Relaxed) >> 10,
        PEAK.load(Ordering::Relaxed) >> 20,
        rss_kb() >> 10
    );
    for i in 0..NB {
        let n = SNAP[i].load(Ordering::Relaxed);
        let b = SNAPB[i].load(Ordering::Relaxed);
        if b >= 1 << 20 {
            if i <= 64 {
                eprintln!(
                    "ALLOCPROBE size<={} count={} mb={:.1}",
                    i * 8,
                    n,
                    b as f64 / 1048576.0
                );
            } else {
                eprintln!(
                    "ALLOCPROBE size<2^{} count={} mb={:.1}",
                    i - 65,
                    n,
                    b as f64 / 1048576.0
                );
            }
        }
    }
}

fn drop_bytes(delta: usize) {
    LIVE.fetch_sub(delta, Ordering::Relaxed);
}

pub struct CountingAlloc;

thread_local! { static IN_BIG: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

fn big(kind: &str, size: usize) {
    if size < (1 << 20) || std::env::var_os("BIGTRACE").is_none() {
        return;
    }
    if IN_BIG.with(|c| c.replace(true)) {
        return;
    }
    let bt = std::backtrace::Backtrace::force_capture();
    eprintln!("BIG {kind} {size} live={} sl={}\n{bt}", LIVE.load(Ordering::Relaxed),
        pg_rules::trail::probe::STEP_LIVE.load(Ordering::Relaxed));
    IN_BIG.with(|c| c.set(false));
}

// SAFETY: every method delegates the actual allocation work to `System`, which already upholds
// `GlobalAlloc`'s contract; this wrapper only adds non-mutating byte-count bookkeeping around it.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            big("alloc", layout.size());
            hist_add(layout.size());
            bump(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        hist_sub(layout.size());
        drop_bytes(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            hist_sub(layout.size());
            hist_add(new_size);
            big("realloc", new_size);
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
            hist_add(layout.size());
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
    for a in [&PK_TODAY, &PK_TODAY_SL, &PK_TODAY_SA, &PK_ARENA, &PK_ARENA_SL, &PK_ARENA_SA, &PK_ARENA_TODAY] {
        a.store(0, Ordering::Relaxed);
    }
    pg_rules::trail::probe::reset();
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}
