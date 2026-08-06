//! `hc_parse_word` / `hc_parse_batch`: the two result-producing entry points. Both encode
//! through the same `crate::buffer` writer (`hc_parse_word` is the `word_count == 1` case),
//! and both wrap their **entire** body in `catch_unwind` — including, for `hc_parse_batch`,
//! the call-scoped unified-analysis Rayon pool, so a panic raised inside a worker thread and
//! propagated out through `par_iter`/`.install()` is caught here, not left to unwind across
//! the `extern "C"` frame.
//!
//! ## Guessed analyses must never be wire-indistinguishable from confirmed ones
//! These two pass `guess_fallback: false` into `GrammarHandle::analyze_word`/`analyze_words`
//! (which forward it to `pg_lexicon::SuppliedLexiconRuntime::analyze_word_opts`), so a total
//! miss stays a total miss here — zero analyses, never an unmarked guess. Callers that want
//! guessing use `hc_parse_word_opts`/`hc_parse_batch_opts` below, whose wire format can mark
//! it honestly. See `crate::buffer`'s `write_word` for the belt-and-braces encoder-side guard
//! that makes this format unable to emit a guessed analysis even if a future change to this
//! file passed `true`.
//!
//! ## `hc_parse_word_opts` / `hc_parse_batch_opts` (ABI v3)
//! Additive siblings of the two entry points above, giving the Guesser (`guessRoot`/
//! `LexicalGuess`) engine logic an FFI surface for the first time. These two route through the
//! grammar handle's plain `pg_parse::Morpher` (`gh.morpher` — the SAME engine `pg-cli`'s
//! `pangloss batch/parse --guess` uses), **not** the supplied-lexicon/foma union
//! `hc_parse_word`/`hc_parse_batch` use, so `guess_root == 0` reproduces `Morpher::parse_word`
//! byte-for-byte (the default, pre-existing behavior) and the CLI/FFI/library paths are all
//! provably the same computation (see `tests/parse_opts_gate.rs`). They encode through
//! `crate::buffer::encode_single_guess`/`crate::buffer::encode_batch_guess` — a distinct
//! wire format/magic from `hc_parse_word`/`hc_parse_batch`'s, carrying the `guessed` bit
//! `ParseOutcome`/`WordAnalysis` already track — so a guessed analysis is never wire-
//! indistinguishable from a confirmed one. `hc_parse_word`/`hc_parse_batch` themselves, and
//! their existing wire format, are completely untouched by this addition (per ABI discipline —
//! see `crate::HC_ABI_VERSION`'s doc, every new entry point gets its own new symbol rather than
//! changing an existing one's contract).

use crate::error::{
    write_buf, write_empty_buf, HcResultBuf, HC_ERR_INVALID_ARG, HC_ERR_NULL_ARG, HC_ERR_PANIC,
    HC_ERR_UTF8, HC_OK,
};
use crate::grammar::HcGrammarHandle;

/// A borrowed UTF-8 string passed into `hc_parse_batch` (`HcStr`): a pointer + byte
/// length, no ownership transfer, no terminator assumed (not nul-terminated — `len` is exact).
#[repr(C)]
pub struct HcStr {
    pub ptr: *const u8,
    pub len: usize,
}

/// `hc_parse_word(HcGrammarHandle, const uint8_t* word_utf8, size_t len, HcResultBuf* out)`.
/// Parses one word on the caller's own thread (no rayon involvement — safe to call concurrently
/// from many host threads against the same handle, since the grammar handle is `Send + Sync`).
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
        // `guess_fallback: false`, since this entry point's wire format has no `guessed` bit.
        let outcome = unified_to_parse(gh.analyze_word(word, false));
        Ok(crate::buffer::encode_single(&outcome))
    });
    finish(result, out)
}

fn unified_to_parse(unified: pg_lexicon::UnifiedAnalysis) -> pg_parse::ParseOutcome {
    pg_parse::ParseOutcome {
        analyses: unified.analyses,
        structured: unified.structured,
        capped: unified.capped,
        invalid_shape: unified.invalid_shape,
        steps: 0,
        timed_out: unified.timed_out,
        guessed: unified.guessed,
        candidates_generated: unified.candidates_generated,
    }
}

/// `hc_parse_batch(HcGrammarHandle, const HcStr* words, size_t n, int32_t max_threads,
/// HcResultBuf* out)`. `max_threads == 0` means Rayon's available-core default.
/// A requested-size call-scoped pool parallelizes unified runtime analysis across words. Do not
/// call this concurrently with itself or with `hc_parse_word` calls that would oversubscribe the
/// host beyond what the caller intends (the grammar handle itself is safe to share; this is a
/// performance note, not a soundness one).
///
/// Returns/leaves `*out` under the same contract as `hc_parse_word`.
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
        // `guess_fallback: false`, same reasoning as `hc_parse_word`'s own call above.
        let outcomes: Vec<_> = gh
            .analyze_words(&rust_words, max_threads as usize, false)
            .map_err(|_| HC_ERR_INVALID_ARG)?
            .into_iter()
            .map(|(outcome, elapsed)| pg_parse::BatchWordOutcome {
                outcome: unified_to_parse(outcome),
                elapsed,
            })
            .collect();
        Ok(crate::buffer::encode_batch(&outcomes))
    });
    finish(result, out)
}

/// `hc_parse_word_opts(HcGrammarHandle, const uint8_t* word_utf8, size_t len, int32_t guess_root,
/// HcResultBuf* out)` (see this module's own doc). `guess_root == 0`
/// reproduces `Morpher::parse_word`/`hc_parse_word`'s pre-existing analysis set byte-for-byte
/// (guess off, the default); nonzero enables the P11 lexical-pattern guesser on a total
/// normal-lexicon miss. Parses one word on the caller's own thread, exactly like
/// `hc_parse_word`. Encodes through `crate::buffer::encode_single_guess` (see module doc).
///
/// Returns/leaves `*out` under the same contract as `hc_parse_word`.
///
/// # Safety
/// Same preconditions as `hc_parse_word`.
#[no_mangle]
pub unsafe extern "C" fn hc_parse_word_opts(
    handle: HcGrammarHandle,
    word_utf8: *const u8,
    len: usize,
    guess_root: i32,
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
        let opts = pg_parse::ParseOptions::default().with_guess_root(guess_root != 0);
        let outcome = gh.morpher.parse_word_opts(word, &opts);
        Ok(crate::buffer::encode_single_guess(&outcome))
    });
    finish(result, out)
}

/// `hc_parse_batch_opts(HcGrammarHandle, const HcStr* words, size_t n, int32_t max_threads,
/// int32_t guess_root, HcResultBuf* out)` (see this module's own doc).
/// `guess_root == 0` reproduces the pre-existing guess-off analysis set byte-for-byte, per word;
/// nonzero enables the guesser for every word in the batch. Internally parallel (rayon) via
/// `parse_batch_with_opts` — a smaller, additive sibling of `pg_parse::hc_parse_batch` that
/// threads a `ParseOptions` through (that free function has none). Encodes through
/// `crate::buffer::encode_batch_guess`.
///
/// Returns/leaves `*out` under the same contract as `hc_parse_batch`.
///
/// # Safety
/// Same preconditions as `hc_parse_batch`.
#[no_mangle]
pub unsafe extern "C" fn hc_parse_batch_opts(
    handle: HcGrammarHandle,
    words: *const HcStr,
    n: usize,
    max_threads: i32,
    guess_root: i32,
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
        let opts = pg_parse::ParseOptions::default().with_guess_root(guess_root != 0);
        let outcomes = parse_batch_with_opts(&gh.morpher, &rust_words, max_threads as usize, &opts);
        Ok(crate::buffer::encode_batch_guess(&outcomes))
    });
    finish(result, out)
}

/// `--guess`'s own parallel batch dispatch, since `hc_parse_batch` has no `ParseOptions` to express "guess on"; deliberately simpler than that function's dispatch scheduling, order-preserving via rayon's indexed `par_iter().map().collect()`.
fn parse_batch_with_opts(
    morpher: &pg_parse::Morpher,
    words: &[String],
    max_threads: usize,
    opts: &pg_parse::ParseOptions,
) -> Vec<pg_parse::BatchWordOutcome> {
    use rayon::prelude::*;
    if words.is_empty() {
        return Vec::new();
    }
    let mut pool_builder = rayon::ThreadPoolBuilder::new().stack_size(1 << 30);
    if max_threads > 0 {
        pool_builder = pool_builder.num_threads(max_threads);
    }
    let pool = pool_builder
        .build()
        .expect("build rayon pool for hc_parse_batch_opts");
    pool.install(|| {
        words
            .par_iter()
            .map(|word| {
                pg_parse::batch::test_panic_if_requested(word);
                let start = std::time::Instant::now();
                let outcome = morpher.parse_word_opts(word, opts);
                let elapsed = start.elapsed();
                pg_parse::BatchWordOutcome { outcome, elapsed }
            })
            .collect()
    })
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

/// `hc_buf_free(HcResultBuf*)`. Frees a buffer produced by `hc_parse_word`,
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
