//! Grammar handle: load, free, and the self-referential `Morpher` construction (plan §4.2).
//!
//! The handle is immutable and `Send + Sync` once built (compile-time-checked below): one
//! `hc_grammar_load` can back any number of concurrent `hc_parse_word` callers, or one internally
//! parallel `hc_parse_batch` call. **Safety contract the caller must uphold** (undocumentable in
//! the C signature itself, so stated here and in the crate docs): do not call `hc_grammar_free`
//! on a handle while any `hc_parse_word`/`hc_parse_batch` call using it is still in flight on
//! another thread. This is the same "read-only while shared, exclusive to free" contract as any
//! `Arc`-free C API; a future revision could enforce it with a refcount if FieldWorks needs it.

use hc_grammar::model::Grammar;
use hc_parse::Morpher;

use crate::error::{
    clear_error, set_error, HcError, HC_ERR_GRAMMAR_LOAD, HC_ERR_NULL_ARG, HC_ERR_UTF8, HC_OK,
};

/// Opaque handle type in the C ABI (`typedef void* HcGrammarHandle;`). Actually a
/// `Box<GrammarHandle>` leaked via [`Box::into_raw`]; reclaimed by [`hc_grammar_free`].
pub type HcGrammarHandle = *mut std::ffi::c_void;

/// Matches the Indonesian parity-gate configuration already established by the M6/M7 work
/// (`--step-cap 500000 --memo=on`; see MEMORY `rust-parity-facts`): a bounded budget by default
/// so a native host cannot trigger the unbounded-memory runaway documented there (a prior session
/// hit 55GB+ on an unmemoized/uncapped Indonesian word) merely by loading a grammar and parsing.
/// Plan §4.2's ABI has no step-cap/memo parameter yet in `hc_grammar_load` — a candidate addition
/// for the #448 budgets port (M10) if a host ever needs a different budget.
///
/// `pub` so tests can build a plain in-process `hc_parse::Morpher` with the identical
/// configuration the FFI handle uses internally — the FFI-vs-in-process parity test needs both
/// sides run under the same budget/memo settings, or a mismatch there (not an encoding bug) could
/// masquerade as one.
pub const DEFAULT_STEP_CAP: usize = 500_000;
pub const DEFAULT_MEMO: bool = true;

/// The boxed, self-owning grammar + pre-built `Morpher`.
///
/// `Morpher<'g>` borrows the `Grammar` it parses against; to store both in one heap allocation
/// behind an opaque C pointer, `grammar` is boxed first (a stable heap address that does not move
/// even if this outer struct is moved — only the `Box`'s pointer moves), and `morpher` holds a
/// `'static`-lifetime-erased reference into it, constructed once in [`GrammarHandle::new`] and
/// never re-pointed. This is sound because:
/// - the boxed `Grammar` is never mutated or moved out of this struct after construction, so its
///   heap address is stable for the handle's whole lifetime;
/// - `morpher` (declared first, so it drops first — see field order below) never outlives
///   `grammar`, since both live and die inside the same `Box<GrammarHandle>`; and
/// - `HcGrammarHandle` never exposes the `Grammar` reference or the `Morpher` separately, only
///   this struct as a single opaque unit.
pub(crate) struct GrammarHandle {
    /// Declared before `grammar` so it drops first. Neither type's `Drop` impl (there isn't one)
    /// dereferences the other, so this ordering isn't load-bearing for *this* struct today — but
    /// it is the correct discipline for a self-referential struct regardless, and keeps this
    /// sound if either type ever gains a `Drop` impl later.
    pub(crate) morpher: Morpher<'static>,
    /// Never read directly after construction — it exists purely to *own* the allocation
    /// `morpher` points into (dropping it is what actually frees the grammar). That's a real
    /// use the `dead_code` lint can't see, hence the explicit allow.
    #[allow(dead_code)]
    grammar: Box<Grammar>,
}

impl GrammarHandle {
    fn new(grammar: Grammar) -> Box<GrammarHandle> {
        let grammar = Box::new(grammar);
        // SAFETY: `grammar` is heap-allocated and, per the struct docs above, never moved or
        // mutated again for the lifetime of the `GrammarHandle` being constructed — only this
        // function ever had `&mut` access to it, and that access ends here. The transmuted
        // lifetime therefore only asserts something true: the referent outlives every use of
        // `morpher`, because both are dropped together (as fields of the same struct) and
        // `morpher` (declared first) drops before `grammar` is freed.
        let grammar_ref: &'static Grammar = unsafe { &*(grammar.as_ref() as *const Grammar) };
        let morpher = Morpher::new(grammar_ref, DEFAULT_STEP_CAP).with_memo(DEFAULT_MEMO);
        Box::new(GrammarHandle { morpher, grammar })
    }
}

// Compile-time proof the handle can be shared across host threads, per plan §4.2 ("immutable and
// Send + Sync"). If a future field addition ever breaks this, it fails to compile right here
// instead of silently becoming a data race reachable only under concurrent FFI load.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<GrammarHandle>();
};

/// `hc_grammar_load(const uint8_t* xml_utf8, size_t len, HcGrammarHandle* out, HcError* err)`
/// (plan §4.2). Entire body wrapped in `catch_unwind` (plan §8 layer 7) — no panic from XML
/// parsing, grammar compilation, or lint checking crosses this boundary.
///
/// # Safety
/// `xml_utf8` must point to `len` readable bytes (or `len == 0`, in which case it may be null);
/// `out` and `err` must each be a valid pointer to storage for their respective out-types, valid
/// for the duration of the call. On success, `*out` receives a handle that must eventually be
/// passed to [`hc_grammar_free`] exactly once.
#[no_mangle]
pub unsafe extern "C" fn hc_grammar_load(
    xml_utf8: *const u8,
    len: usize,
    out: *mut HcGrammarHandle,
    err: *mut HcError,
) -> i32 {
    let result = std::panic::catch_unwind(|| -> Result<Grammar, (i32, String)> {
        if out.is_null() {
            return Err((HC_ERR_NULL_ARG, "hc_grammar_load: out is null".to_string()));
        }
        if len > 0 && xml_utf8.is_null() {
            return Err((
                HC_ERR_NULL_ARG,
                "hc_grammar_load: xml_utf8 is null but len > 0".to_string(),
            ));
        }
        // SAFETY: `xml_utf8`/`len` validity is this function's documented precondition; the
        // null+zero-length case is handled above without dereferencing anything.
        let bytes: &[u8] = if len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(xml_utf8, len) }
        };
        let xml = std::str::from_utf8(bytes).map_err(|e| {
            (
                HC_ERR_UTF8,
                format!("hc_grammar_load: invalid UTF-8 in grammar xml: {e}"),
            )
        })?;
        hc_grammar::load(xml).map_err(|e| (HC_ERR_GRAMMAR_LOAD, format!("hc_grammar_load: {e}")))
    });

    match result {
        Ok(Ok(grammar)) => {
            let handle = GrammarHandle::new(grammar);
            // SAFETY: `out` non-null already checked above.
            unsafe {
                *out = Box::into_raw(handle) as HcGrammarHandle;
            }
            clear_error(err);
            HC_OK
        }
        Ok(Err((code, msg))) => {
            set_error(err, code, &msg);
            code
        }
        Err(payload) => {
            let msg = crate::error::panic_message(payload);
            let full = format!("hc_grammar_load: caught panic: {msg}");
            set_error(err, crate::error::HC_ERR_PANIC, &full);
            crate::error::HC_ERR_PANIC
        }
    }
}

/// `hc_grammar_free(HcGrammarHandle)` (plan §4.2). Reclaims the handle built by
/// [`hc_grammar_load`]. A null handle is a no-op. Wrapped in `catch_unwind` per plan §8 layer 7
/// even though this path is not expected to panic — every entry point gets the same treatment,
/// not just the ones judged risky, since that judgment is exactly what a regression would
/// invalidate.
///
/// # Safety
/// `handle` must be either null or a value previously returned via `*out` by a successful
/// `hc_grammar_load` call, not yet freed, and not in concurrent use by any in-flight
/// `hc_parse_word`/`hc_parse_batch` call (see module docs).
#[no_mangle]
pub unsafe extern "C" fn hc_grammar_free(handle: HcGrammarHandle) {
    let result = std::panic::catch_unwind(|| {
        if handle.is_null() {
            return;
        }
        // SAFETY: per this function's documented precondition, `handle` is a still-valid,
        // not-yet-freed value produced by `Box::into_raw` in `hc_grammar_load`.
        unsafe {
            drop(Box::from_raw(handle as *mut GrammarHandle));
        }
    });
    if let Err(payload) = result {
        // No error channel on this signature (plan §4.2: `void` return) — the panic is still
        // caught (never crosses the boundary), just not reportable beyond a diagnostic.
        eprintln!(
            "hc_grammar_free: caught panic: {}",
            crate::error::panic_message(payload)
        );
    }
}

/// Borrow the `GrammarHandle` behind an opaque `HcGrammarHandle`, or `None` if null.
///
/// # Safety
/// `handle` must be either null or a value returned by a successful `hc_grammar_load` and not
/// yet freed via `hc_grammar_free`.
pub(crate) unsafe fn borrow<'a>(handle: HcGrammarHandle) -> Option<&'a GrammarHandle> {
    if handle.is_null() {
        None
    } else {
        // SAFETY: precondition documented above.
        Some(unsafe { &*(handle as *const GrammarHandle) })
    }
}
