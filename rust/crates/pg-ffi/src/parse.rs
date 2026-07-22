//! `hc_parse_word` / `hc_parse_batch` (plan §4.2): the two result-producing entry points. Both
//! encode through the same [`crate::buffer`] writer (`hc_parse_word` is the `word_count == 1`
//! case), and both wrap their **entire** body in `catch_unwind` (plan §8 layer 7) — including,
//! for `hc_parse_batch`, the call into `pg_parse::hc_parse_batch`'s rayon scoped pool, so a panic
//! raised inside a worker thread and propagated out through `par_iter`/`.install()` is caught
//! here, not left to unwind across the `extern "C"` frame.

use crate::error::{
    write_buf, write_empty_buf, HcResultBuf, HC_ERR_INVALID_ARG, HC_ERR_NULL_ARG, HC_ERR_PANIC,
    HC_ERR_UTF8, HC_OK,
};
use crate::grammar::HcGrammarHandle;

/// A borrowed UTF-8 string passed into `hc_parse_batch` (plan §4.2's `HcStr`): a pointer + byte
/// length, no ownership transfer, no terminator assumed (not nul-terminated — `len` is exact).
#[repr(C)]
pub struct HcStr {
    pub ptr: *const u8,
    pub len: usize,
}

/// `hc_parse_word(HcGrammarHandle, const uint8_t* word_utf8, size_t len, HcResultBuf* out)`
/// (plan §4.2). Parses one word on the caller's own thread (no rayon involvement — safe to call
/// concurrently from many host threads against the same handle, per plan §4.2's "grammar handle
/// is Send + Sync" contract).
///
/// Returns `HC_OK` (0) on success, otherwise a nonzero `HC_ERR_*` code; `*out` is always left in
/// a valid, freeable state (either the encoded result, or `HcResultBuf::EMPTY` on any error path
/// — see `error` module docs), so the caller should call `hc_buf_free(out)` unconditionally.
///
/// # Safety
/// `handle` must be a still-valid handle from `hc_grammar_load` (or null, which is an error, not
/// UB). `word_utf8` must point to `len` readable bytes (or be null iff `len == 0`). `out` must be
/// a valid pointer to `HcResultBuf` storage for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn hc_parse_word(
    handle: HcGrammarHandle,
    word_utf8: *const u8,
    len: usize,
    out: *mut HcResultBuf,
) -> i32 {
    let result = std::panic::catch_unwind(|| -> Result<Vec<u8>, i32> {
        // SAFETY: `handle` validity is this function's documented precondition.
        let gh = unsafe { crate::grammar::borrow(handle) }.ok_or(HC_ERR_NULL_ARG)?;
        if len > 0 && word_utf8.is_null() {
            return Err(HC_ERR_NULL_ARG);
        }
        // SAFETY: `word_utf8`/`len` validity is this function's documented precondition; the
        // null+zero-length case never dereferences the pointer.
        let bytes: &[u8] = if len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(word_utf8, len) }
        };
        let word = std::str::from_utf8(bytes).map_err(|_| HC_ERR_UTF8)?;
        let unified = gh.runtime.analyze_word(word, None);
        let outcome = pg_parse::ParseOutcome {
            analyses: unified.analyses,
            structured: unified.structured,
            capped: unified.capped,
            invalid_shape: unified.invalid_shape,
            steps: 0,
            timed_out: unified.timed_out,
            guessed: unified.guessed,
            candidates_generated: unified.candidates_generated,
        };
        Ok(crate::buffer::encode_single(&outcome))
    });
    finish(result, out)
}

/// `hc_parse_batch(HcGrammarHandle, const HcStr* words, size_t n, int32_t max_threads,
/// HcResultBuf* out)` (plan §4.2). `max_threads == 0` means "all cores" (forwarded to
/// `pg_parse::hc_parse_batch`, which treats `0` as rayon's default). Internally parallel — do not
/// call this concurrently with itself or with `hc_parse_word` calls that would oversubscribe the
/// host beyond what the caller intends (the grammar handle itself is safe to share; this is a
/// performance note, not a soundness one).
///
/// Returns/leaves `*out` under the same contract as [`hc_parse_word`].
///
/// # Safety
/// `handle` must be a still-valid handle from `hc_grammar_load`. `words` must point to `n` valid
/// `HcStr` entries (or be null iff `n == 0`); each entry's `ptr` must point to `len` readable
/// bytes (or be null iff `len == 0`). `out` must be a valid pointer to `HcResultBuf` storage.
#[no_mangle]
pub unsafe extern "C" fn hc_parse_batch(
    handle: HcGrammarHandle,
    words: *const HcStr,
    n: usize,
    max_threads: i32,
    out: *mut HcResultBuf,
) -> i32 {
    let result = std::panic::catch_unwind(|| -> Result<Vec<u8>, i32> {
        // SAFETY: `handle` validity is this function's documented precondition.
        let gh = unsafe { crate::grammar::borrow(handle) }.ok_or(HC_ERR_NULL_ARG)?;
        if n > 0 && words.is_null() {
            return Err(HC_ERR_NULL_ARG);
        }
        if max_threads < 0 {
            return Err(HC_ERR_INVALID_ARG);
        }
        let mut rust_words: Vec<String> = Vec::with_capacity(n);
        for i in 0..n {
            // SAFETY: `words` points to `n` valid entries (documented precondition); `i < n`.
            let hs = unsafe { &*words.add(i) };
            if hs.len > 0 && hs.ptr.is_null() {
                return Err(HC_ERR_NULL_ARG);
            }
            // SAFETY: each entry's `(ptr, len)` validity is this function's documented
            // precondition; the null+zero-length case never dereferences the pointer.
            let bytes: &[u8] = if hs.len == 0 {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(hs.ptr, hs.len) }
            };
            let s = std::str::from_utf8(bytes).map_err(|_| HC_ERR_UTF8)?;
            rust_words.push(s.to_string());
        }
        // The one call that can panic on a rayon worker thread (test-panic-hook or, in
        // principle, any latent engine bug) — deliberately still inside this same
        // `catch_unwind`, not factored out, so the abort-safety test exercises exactly this path.
        let outcomes = pg_parse::hc_parse_batch(&gh.morpher, &rust_words, max_threads as usize);
        Ok(crate::buffer::encode_batch(&outcomes))
    });
    finish(result, out)
}

/// Shared tail for every result-producing entry point (`hc_parse_word`/`hc_parse_batch`/
/// `hc_generate_words`): turn a `catch_unwind` result into a return code, always leaving `*out` in
/// a valid, freeable state. `pub(crate)` so `generate` can reuse it rather than duplicating the
/// three-way match.
pub(crate) fn finish(
    result: std::thread::Result<Result<Vec<u8>, i32>>,
    out: *mut HcResultBuf,
) -> i32 {
    match result {
        Ok(Ok(bytes)) => {
            write_buf(out, bytes);
            HC_OK
        }
        Ok(Err(code)) => {
            write_empty_buf(out);
            code
        }
        Err(_payload) => {
            write_empty_buf(out);
            HC_ERR_PANIC
        }
    }
}

/// `hc_buf_free(HcResultBuf*)` (plan §4.2). Frees a buffer produced by `hc_parse_word`,
/// `hc_parse_batch`, or embedded in an `HcError::message`. Idempotent: a null pointer, or a
/// buffer already zeroed (`data == null`), is a no-op — safe to call more than once or on a
/// buffer that was never populated (e.g. the `HcError::message` of a *successful*
/// `hc_grammar_load` call, which is `HcResultBuf::EMPTY`).
///
/// # Safety
/// If `buf` is non-null, `*buf` must be a value this crate produced (via `hc_parse_word`,
/// `hc_parse_batch`, or `hc_grammar_load`'s `HcError::message`) and not already freed.
#[no_mangle]
pub unsafe extern "C" fn hc_buf_free(buf: *mut HcResultBuf) {
    let result = std::panic::catch_unwind(|| {
        if buf.is_null() {
            return;
        }
        // SAFETY: precondition documented above.
        let b = unsafe { &mut *buf };
        if !b.data.is_null() {
            // SAFETY: `(data, len, cap)` were produced by `Vec::into_raw_parts`-equivalent
            // leaking in `crate::error::leak_buf` and not yet freed (precondition).
            drop(unsafe { Vec::from_raw_parts(b.data, b.len, b.cap) });
        }
        *b = HcResultBuf::EMPTY;
    });
    if let Err(payload) = result {
        eprintln!(
            "hc_buf_free: caught panic: {}",
            crate::error::panic_message(payload)
        );
    }
}
