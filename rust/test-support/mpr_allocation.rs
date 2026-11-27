use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static LARGEST: Cell<Option<usize>> = const { Cell::new(None) };
}

struct AllocationProbe;

#[global_allocator]
static ALLOCATOR: AllocationProbe = AllocationProbe;

fn record(size: usize) {
    let _ = LARGEST.try_with(|largest| {
        if let Some(previous) = largest.get() {
            largest.set(Some(previous.max(size)));
        }
    });
}

// SAFETY: all allocation operations forward their original pointers and layouts to System.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for AllocationProbe {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(ptr, layout, size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

pub fn measured<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            LARGEST.with(|largest| largest.set(None));
        }
    }
    LARGEST.with(|largest| {
        assert!(largest.get().is_none());
        largest.set(Some(0));
    });
    let _reset = Reset;
    let result = operation();
    let largest = LARGEST.with(|largest| largest.get().unwrap());
    (result, largest)
}

pub fn request(id: u32) -> serde_json::Value {
    serde_json::json!({"revision":"rev_1", "analysis": {
        "morphemeIds":[0], "morphOccurrences":[], "rootMorphemeIndex":0,
        "posId":null, "synFs":{"entries":[]},
        "mpr":{"version":1,"ids":[id]}, "guessed":false,
        "guessedString":null, "provenance":{"kind":"grammar"},
        "suppliedRoot":null, "morphemeRoots":[]
    }})
}

pub fn request_at(id: u32, location: usize) -> serde_json::Value {
    let mut value = request(0);
    let mpr = serde_json::json!({"version":1,"ids":[id]});
    let root = serde_json::json!({
        "entryId":"entry", "realizationId":"root", "lexicalSpelling":"pala",
        "gloss":"", "synFs":{"entries":[]}, "mpr":mpr,
        "stratum":0, "authority":{"kind":"supplied"}
    });
    match location {
        0 => value["analysis"]["mpr"] = mpr,
        1 => value["analysis"]["suppliedRoot"] = root,
        2 => value["analysis"]["morphemeRoots"] = serde_json::json!([root]),
        _ => panic!("unknown MPR payload location"),
    }
    value
}
