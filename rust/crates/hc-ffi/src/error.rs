//! Error codes and the two C-visible buffer types (plan §4.2).
//!
//! `HcResultBuf` is a generic owned-byte-buffer triple `(data, len, cap)` reused for two
//! purposes: parse results (`hc_parse_word`/`hc_parse_batch`) and, embedded in [`HcError`], a
//! grammar-load error message. Reusing one type means `hc_buf_free` — the only free function the
//! ABI declares (plan §4.2) — is sufficient to release either; there is no second free function
//! for `HcError::message`. A zeroed `HcResultBuf` (`data == null`, `len == 0`, `cap == 0`) is
//! always a valid, freeable "empty buffer" state — both the encoder (on error paths) and
//! `hc_buf_free` (idempotent on an already-freed/zeroed buffer) maintain that invariant, so a
//! caller may unconditionally call `hc_buf_free` on any `HcResultBuf`/`HcError::message` it was
//! handed, whether or not the call that produced it succeeded.

/// `hc_abi_version()` / entry-point return codes (plan §4.2). `0` is always success; every other
/// value is an error and, for `hc_grammar_load`, is accompanied by a human-readable message in
/// the caller-supplied `HcError`. `hc_parse_word`/`hc_parse_batch` have no `HcError*` parameter
/// in the ABI, so their failures are the bare code only — see each function's doc comment.
pub const HC_OK: i32 = 0;
/// `xml_utf8`/`word_utf8`/an `HcStr.ptr` was not valid UTF-8.
pub const HC_ERR_UTF8: i32 = 1;
/// `hc_grammar::load` rejected the grammar (XML parse error, unsupported construct, or semantic
/// error — see the message in `HcError`).
pub const HC_ERR_GRAMMAR_LOAD: i32 = 2;
/// A required pointer argument was null (or a non-null-length buffer had a null pointer).
pub const HC_ERR_NULL_ARG: i32 = 3;
/// A panic was caught at the FFI boundary (plan §8 layer 7) — the process is otherwise healthy
/// and the grammar handle (if any) remains valid for further calls.
pub const HC_ERR_PANIC: i32 = 4;
/// An argument was structurally invalid in a way that isn't a null-pointer or UTF-8 problem
/// (currently: a negative `max_threads`).
pub const HC_ERR_INVALID_ARG: i32 = 5;

/// A generic owned byte buffer handed across the FFI boundary. See module docs for the dual use
/// as both a parse-result buffer and an error-message buffer, and the "always freeable, even
/// zeroed" invariant.
#[repr(C)]
pub struct HcResultBuf {
    pub data: *mut u8,
    pub len: usize,
    pub cap: usize,
}

impl HcResultBuf {
    /// The zeroed "empty/no buffer" state — always valid to hand to `hc_buf_free`.
    pub const EMPTY: HcResultBuf = HcResultBuf {
        data: std::ptr::null_mut(),
        len: 0,
        cap: 0,
    };
}

/// `hc_grammar_load`'s out-parameter: an error code plus an owned UTF-8 message buffer (empty on
/// success). Free `message` via `hc_buf_free(&mut err.message)`.
#[repr(C)]
pub struct HcError {
    pub code: i32,
    _pad: i32,
    pub message: HcResultBuf,
}

impl HcError {
    pub const EMPTY: HcError = HcError {
        code: HC_OK,
        _pad: 0,
        message: HcResultBuf::EMPTY,
    };
}

/// Move `bytes` into a leaked `(ptr, len, cap)` triple suitable for handing across the FFI
/// boundary; the receiver must eventually pass it to [`crate::hc_buf_free`] (directly, or nested
/// inside an `HcError`) to reclaim it.
pub(crate) fn leak_buf(bytes: Vec<u8>) -> HcResultBuf {
    let mut v = bytes;
    let data = v.as_mut_ptr();
    let len = v.len();
    let cap = v.capacity();
    std::mem::forget(v);
    HcResultBuf { data, len, cap }
}

/// Write `HcResultBuf::EMPTY` into `*out` if `out` is non-null. Used on every error path so the
/// out-parameter is always left in the "valid, freeable, empty" state (module docs) rather than
/// uninitialized.
pub(crate) fn write_empty_buf(out: *mut HcResultBuf) {
    if out.is_null() {
        return;
    }
    // SAFETY: caller-supplied `out` is documented (plan §4.2) as a valid out-pointer for the
    // duration of the call; non-null just checked. Writing a plain-old-data struct never reads
    // the previous contents.
    unsafe {
        *out = HcResultBuf::EMPTY;
    }
}

/// Write `bytes` into `*out` if `out` is non-null (dropping `bytes` otherwise — the caller passed
/// a null out-pointer, which is a caller error already reflected in the returned status code).
pub(crate) fn write_buf(out: *mut HcResultBuf, bytes: Vec<u8>) {
    if out.is_null() {
        return;
    }
    let buf = leak_buf(bytes);
    // SAFETY: see `write_empty_buf`.
    unsafe {
        *out = buf;
    }
}

/// Reset `*err` to "no error" if `err` is non-null. Does **not** free any previous message the
/// caller may not have freed yet — callers own exactly one `HcError` per `hc_grammar_load` call
/// and are expected to free/inspect it before the next call on the same `HcError` storage.
pub(crate) fn clear_error(err: *mut HcError) {
    if err.is_null() {
        return;
    }
    // SAFETY: see `write_empty_buf`.
    unsafe {
        *err = HcError::EMPTY;
    }
}

/// Write `code`/`msg` into `*err` if `err` is non-null.
pub(crate) fn set_error(err: *mut HcError, code: i32, msg: &str) {
    if err.is_null() {
        return;
    }
    let buf = leak_buf(msg.as_bytes().to_vec());
    // SAFETY: see `write_empty_buf`.
    unsafe {
        *err = HcError {
            code,
            _pad: 0,
            message: buf,
        };
    }
}

/// Render a caught `catch_unwind` payload as a display string (`Box<dyn Any + Send>` only ever
/// holds a `&'static str` or `String` for panics raised via `panic!`/`.expect`/`.unwrap`, which is
/// everything this codebase raises — anything else prints a fixed fallback rather than failing).
pub(crate) fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}
