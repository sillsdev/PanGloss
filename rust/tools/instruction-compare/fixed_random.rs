// LD_PRELOAD shim: getrandom/getentropy return a fixed stream, so std HashMap RandomState gets the same keys every run and instruction counts stop varying with them.
#![allow(unsafe_code)]
use std::sync::atomic::{AtomicU64, Ordering};

static STATE: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);

fn fill(buf: *mut u8, len: usize) {
    for i in 0..len {
        let mut x = STATE.load(Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        STATE.store(x, Ordering::Relaxed);
        unsafe { *buf.add(i) = (x >> 24) as u8 };
    }
}

#[no_mangle]
pub extern "C" fn getrandom(buf: *mut u8, len: usize, _flags: u32) -> isize {
    fill(buf, len);
    len as isize
}

#[no_mangle]
pub extern "C" fn getentropy(buf: *mut u8, len: usize) -> i32 {
    fill(buf, len);
    0
}
