//! C ABI boundary for the .NET host (plan §4.2). This is the ONE crate permitted `unsafe` (the
//! FFI itself, `#[no_mangle] pub unsafe extern "C" fn`s dereferencing caller-supplied raw
//! pointers) — every other crate in this workspace is `#![forbid(unsafe_code)]`. `extern "C"`,
//! UTF-8 in/out, `x86_64-pc-windows-msvc`.
//!
//! ## The entry points (plan §4.2; W7 adds the seventh; ABI v3 adds the guess-opt-in pair)
//! - [`hc_abi_version`] — version probe, no grammar needed.
//! - [`grammar::hc_grammar_load`] / [`grammar::hc_grammar_free`] — load/free a compiled grammar.
//! - [`parse::hc_parse_word`] — parse one word on the caller's thread.
//! - [`parse::hc_parse_batch`] — parse many words, internally parallel (rayon).
//! - [`parse::hc_parse_word_opts`] / [`parse::hc_parse_batch_opts`] (HC-rust port gap G3,
//!   `docs/hermitcrab-rust-port-audit.md` sec 2/3 item 1) — additive `guess_root`-parameterized
//!   siblings of the two entry points above, routing through the plain `pg_parse::Morpher` (the
//!   same engine `pg-cli`'s `--guess` flag uses) rather than the supplied-lexicon/foma union;
//!   encode through a distinct wire format (`buffer::encode_single_guess`/`encode_batch_guess`)
//!   carrying the `guessed` bit so a guessed analysis is never wire-indistinguishable from a
//!   confirmed one. See `parse` module's own doc for the full contract.
//! - [`parse::hc_buf_free`] — free a result/error-message buffer.
//! - [`generate::hc_generate_words`] — generate surface forms from a `WordAnalysis`-shaped
//!   morpheme sequence (C# `Morpher.GenerateWords(WordAnalysis)`, W7).
//!
//! ## No panic crosses the boundary (plan §8 layer 7)
//! **Every** entry point's entire body is wrapped in `std::panic::catch_unwind`, converting any
//! caught panic into `HC_ERR_PANIC` (see [`error`]) rather than letting an unwind reach the
//! `extern "C"` frame (undefined behavior, not just an ugly error). This holds for
//! `hc_parse_batch` too, including panics raised on a rayon worker thread and propagated out
//! through `par_iter`/`.install()` — see `tests/abort_safety.rs` for the test that proves it on
//! that exact path, not just the single-word one.
//!
//! `panic = "unwind"` in the workspace's `[profile.release]` (`../../Cargo.toml`) is a hard
//! requirement for this to work at all — `"abort"` would make every `catch_unwind` here silently
//! void. That setting is deliberately left alone by this crate; see the workspace `Cargo.toml`'s
//! own comment on it.
//!
//! ## Grammar handle
//! Immutable and `Send + Sync` once built (compile-time-checked in [`grammar`]): one
//! `hc_grammar_load` can back any number of concurrent `hc_parse_word` callers (the FieldWorks
//! parallelize-across-words pattern) or one internally-parallel `hc_parse_batch` call. See
//! [`grammar`]'s module docs for the one safety contract the C signature can't express: don't
//! free a handle while a call using it is still in flight.
//!
//! ## Wire format
//! See [`buffer`]'s module docs for the exact `HcResultBuf` byte layout.

pub mod buffer;
pub mod error;
pub mod generate;
pub mod grammar;
pub mod json;
pub mod parse;

pub use buffer::{
    decode, decode_generated_words, decode_guess, encode_batch, encode_batch_guess,
    encode_generated_words, encode_single, encode_single_guess, DecodedAnalysis,
    DecodedAnalysisGuess, DecodedWord, DecodedWordGuess,
};
pub use error::{
    HcError, HcResultBuf, HC_ERR_GRAMMAR_LOAD, HC_ERR_INVALID_ARG, HC_ERR_NULL_ARG, HC_ERR_PANIC,
    HC_ERR_UTF8, HC_OK,
};
pub use generate::hc_generate_words;
pub use grammar::{
    hc_grammar_free, hc_grammar_load, HcGrammarHandle, DEFAULT_MEMO, DEFAULT_STEP_CAP,
};
pub use json::*;
pub use parse::{
    hc_buf_free, hc_parse_batch, hc_parse_batch_opts, hc_parse_word, hc_parse_word_opts, HcStr,
};

/// ABI version — bump on any struct layout or semantic change so the managed side can detect a
/// mismatched native DLL and fall back to the managed engine (plan §4.2).
///
/// - `2`: binary parse ABI unchanged; JSON API added.
/// - `3` (HC-rust port gap G3, `docs/hermitcrab-rust-port-audit.md` sec 2/3 item 1): purely
///   additive — `hc_parse_word`/`hc_parse_batch` and their existing wire format are completely
///   untouched. Adds `hc_parse_word_opts`/`hc_parse_batch_opts` (a `guess_root: i32` opt-in
///   surface for the P11 lexical-pattern guesser) plus their own distinct wire format/magic
///   (`buffer::encode_single_guess`/`encode_batch_guess`, decoded by `buffer::decode_guess`)
///   carrying a `guessed` bit per word and per analysis.
pub const HC_ABI_VERSION: i32 = 3;

/// `hc_abi_version()` — the managed loader checks this before trusting the DLL.
#[no_mangle]
pub extern "C" fn hc_abi_version() -> i32 {
    HC_ABI_VERSION
}

#[cfg(test)]
mod tests {
    #[test]
    fn abi_version_is_exposed() {
        assert_eq!(super::hc_abi_version(), super::HC_ABI_VERSION);
    }
}
