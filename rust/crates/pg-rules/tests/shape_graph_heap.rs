//! The shape graph's real heap bytes and peak, read from the allocator, stay inside its own accounting.
#![cfg(all(target_os = "linux", target_env = "gnu"))]

use pg_fst::work::WorkBudget;
use pg_grammar_model::model::{AllomorphId, MRuleId};
use pg_rules::shape_graph::ShapeGraph;
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
    nodes: usize,
    rows: usize,
}

/// Record `keys` shapes' rows for three rules each, two with two output edges and one empty, and read the graph's accounted bytes, real bytes and real peak. The shapes are built as they are recorded and held only by the graph, so their heap counts as the graph's.
fn fill(work: WorkBudget, keys: u32) -> (Measured, ShapeGraph) {
    let before = live();
    PEAK.with(|peak| peak.set(before));
    let graph = ShapeGraph::new(work);
    let mut peak_accounted = 0;
    for key in 0..keys {
        let input = shape(key);
        let edges = [
            (Some(AllomorphId(0)), shape(key + 1_000_000)),
            (Some(AllomorphId(1)), shape(key + 2_000_000)),
        ];
        for rule in 0..3 {
            let edges: &[_] = if rule == 2 { &[] } else { &edges };
            if graph
                .record(MRuleId(rule), true, &input, edges, 100)
                .is_none()
            {
                break;
            }
        }
        peak_accounted = peak_accounted.max(graph.bytes());
    }
    let measured = Measured {
        accounted: graph.bytes(),
        real: (live() - before) as usize,
        peak: (PEAK.with(Cell::get) - before) as usize,
        nodes: graph.nodes(),
        rows: graph.rows(),
    };
    println!(
        "{} nodes, {} rows: accounted {}, real {}, real peak {}",
        measured.nodes, measured.rows, measured.accounted, measured.real, measured.peak
    );
    assert!(peak_accounted >= measured.accounted);
    (measured, graph)
}

/// The accounting covers what the allocator holds, and does not overstate it by much.
fn check(m: &Measured) {
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
fn a_filled_graph_tracks_the_allocator_at_several_sizes() {
    for keys in [3, 40, 700, 4_000] {
        let (m, _graph) = fill(WorkBudget::new(usize::MAX), keys);
        assert_eq!(m.rows, keys as usize * 3, "nothing is refused or evicted");
        check(&m);
    }
}

#[test]
fn the_real_peak_stays_inside_the_accounting_through_every_resize() {
    for keys in [5, 100, 3_000] {
        let (m, _graph) = fill(WorkBudget::new(usize::MAX), keys);
        assert!(
            m.peak <= m.accounted,
            "real peak {} passed accounted {}",
            m.peak,
            m.accounted
        );
    }
}

#[test]
fn a_graph_under_a_word_allowance_stays_in_its_share_then_drops_itself() {
    let allowance = 8 * 400 * 1024;
    let work = WorkBudget::new(usize::MAX).with_word_cap(Some(allowance));
    let (m, graph) = fill(work.clone(), 6_000);
    assert!(graph.is_dead(), "the fill must reach the share");
    assert_eq!(m.accounted, 0, "a dropped graph holds nothing");
    assert!(
        m.real < 64 * 1024,
        "real bytes left after the drop: {}",
        m.real
    );
    drop(graph);
    assert!(
        work.try_charge_optional(allowance),
        "dropping releases the charge"
    );
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
