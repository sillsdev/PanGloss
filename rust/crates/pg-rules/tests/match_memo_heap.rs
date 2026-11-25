//! The match memo's real heap peak, read from the allocator, stays under its cap and tracks its own accounting.
#![cfg(all(target_os = "linux", target_env = "gnu"))]

use pg_fst::work::WorkBudget;
use pg_grammar_model::model::AllomorphId;
use pg_rules::morph::match_memo::{MatchMemo, DEFAULT_CAP_BYTES};
use pg_shape::{Shape, ShapeBuilder};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

extern "C" {
    fn malloc_usable_size(ptr: *mut std::ffi::c_void) -> usize;
}

thread_local! {
    static LIVE: Cell<isize> = const { Cell::new(0) };
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

/// Counts the allocator's real chunk sizes (usable bytes plus the 8-byte header) on this thread, with the peak.
struct Real;

fn chunk(ptr: *mut u8) -> isize {
    if ptr.is_null() {
        0
    } else {
        (unsafe { malloc_usable_size(ptr.cast()) } + 8) as isize
    }
}

fn add(delta: isize) {
    let _ = LIVE.try_with(|live| {
        live.set(live.get() + delta);
        let _ = PEAK.try_with(|peak| peak.set(peak.get().max(live.get())));
    });
}

unsafe impl GlobalAlloc for Real {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        add(chunk(ptr));
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        add(-chunk(ptr));
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old = chunk(ptr);
        let new = System.realloc(ptr, layout, new_size);
        add(chunk(new) - old);
        new
    }
}

#[global_allocator]
static ALLOCATOR: Real = Real;

fn live() -> isize {
    LIVE.with(Cell::get)
}

/// A shape of 6 to 41 nodes with a four-lane feature matrix, varied by `seed`.
fn shape(seed: u32) -> Shape {
    let mut builder = ShapeBuilder::with_features(4);
    for node in 0..(6 + seed % 36) {
        builder.push_segment_with_lanes(
            seed.wrapping_mul(31).wrapping_add(node) % 97,
            &[seed as u64, node as u64, 0, 1],
        );
    }
    builder.finish()
}

struct Measured {
    accounted: usize,
    real: usize,
    peak: usize,
    entries: usize,
}

/// Fill a memo of this cap with `keys` matches and read its accounted bytes, real bytes and real peak.
fn fill(cap: usize, keys: u32) -> Measured {
    let before = live();
    PEAK.with(|peak| peak.set(before));
    let memo = MatchMemo::with_cap(WorkBudget::new(usize::MAX), cap);
    let mut peak_accounted = 0;
    for key in 0..keys {
        let input = shape(key);
        let outputs = [shape(key + 1_000_000), shape(key + 2_000_000)];
        for _ in 0..2 {
            memo.insert(AllomorphId(key % 7), true, &input, &outputs, false, 100);
        }
        peak_accounted = peak_accounted.max(memo.bytes());
    }
    assert!(
        peak_accounted <= cap,
        "accounted {peak_accounted} passed the cap {cap}"
    );
    let measured = Measured {
        accounted: memo.bytes(),
        real: (live() - before) as usize,
        peak: (PEAK.with(Cell::get) - before) as usize,
        entries: memo.len(),
    };
    println!(
        "cap {cap}: {} entries, accounted {}, real {}, real peak {}",
        measured.entries, measured.accounted, measured.real, measured.peak
    );
    measured
}

fn check(m: &Measured, cap: usize) {
    assert!(m.peak <= cap, "real peak {} passed the cap {cap}", m.peak);
    assert!(
        m.real <= m.accounted,
        "under-counted: real {} over accounted {}",
        m.real,
        m.accounted
    );
    assert!(
        m.accounted * 10 <= m.real * 16,
        "accounted {} is over 1.6x real {}",
        m.accounted,
        m.real
    );
}

#[test]
fn a_filled_memo_stays_under_the_default_cap_and_tracks_the_allocator() {
    let m = fill(DEFAULT_CAP_BYTES, 2_000);
    assert_eq!(m.entries, 2_000, "the fill must not evict");
    check(&m, DEFAULT_CAP_BYTES);
}

#[test]
fn an_evicting_memo_stays_under_a_small_cap_through_every_resize() {
    for cap in [200 * 1024, 1024 * 1024] {
        let m = fill(cap, 6_000);
        assert!(m.entries < 6_000, "the fill must evict at cap {cap}");
        assert!(
            m.entries > 10,
            "cap {cap} left room for only {} entries",
            m.entries
        );
        check(&m, cap);
    }
}

#[test]
fn an_evicting_memo_stays_under_the_default_cap() {
    let m = fill(DEFAULT_CAP_BYTES, 16_000);
    assert!(m.entries < 16_000, "the fill must evict at the default cap");
    check(&m, DEFAULT_CAP_BYTES);
}

#[test]
fn shape_heap_bytes_covers_the_allocators_chunks_within_a_step_per_column() {
    use pg_shape::{CdBits, CdSet};
    for seed in 0..64u32 {
        let before = live();
        let shape = {
            let mut builder = ShapeBuilder::with_features(3);
            for node in 0..(1 + seed % 9) {
                builder.push_segment_with_lanes(node, &[1, 2, u64::from(seed)]);
            }
            for node in 0..(seed % 4) {
                let ids = if node % 2 == 0 {
                    vec![3, 70 + seed]
                } else {
                    vec![5]
                };
                builder.push_segment_with_lanes_and_set(
                    &[0, 0, 0],
                    CdSet::Members(CdBits::from_ids(ids)),
                );
            }
            if seed % 3 == 0 {
                builder.set_generated_full(1, &[2, 3]);
            }
            if seed % 5 == 0 {
                builder.set_unresolved_variables(1, &[4]);
            }
            builder.finish()
        };
        let real = (live() - before) as usize;
        let model = shape.heap_bytes();
        assert!(
            real <= model && model <= real + 16 * 8,
            "seed {seed}: model {model}, real {real}"
        );
    }
}
