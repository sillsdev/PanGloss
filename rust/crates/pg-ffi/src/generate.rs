//! `hc_generate_words` (W7, extending plan §4.2's six entry points to seven): the FFI counterpart
//! to C# `Morpher.GenerateWords(WordAnalysis)` (Morpher.cs:659-679).
//!
//! Exposes only the `WordAnalysis`-consuming overload, not the direct 3-arg
//! `GenerateWords(LexEntry, IEnumerable<Morpheme>, FeatureStruct)` overload — the direct overload's
//! `realizationalFS` parameter is an arbitrary syntactic `FeatureStruct`, and this ABI has no wire
//! encoding for one yet (every existing entry point only ever produces/consumes numeric ids and
//! UTF-8 strings). The `WordAnalysis` overload's `realizationalFS` is always empty (C# `new
//! FeatureStruct()`, Morpher.cs:666), so it never needs one: the natural, self-contained FFI
//! surface for a native host that already has a `WordAnalysis` in hand (e.g. from a prior
//! `hc_parse_word`/`hc_parse_batch` call's numeric `structured` output) and wants to regenerate
//! surface forms from it — round-tripping analysis into generation without ever touching a raw
//! `FeatureStruct`. A future revision can add a `hc_generate_words_direct` entry point alongside a
//! syntactic-FS wire format if a caller needs the direct overload's extra generality.

use crate::error::{HcResultBuf, HC_ERR_NULL_ARG};
use crate::grammar::HcGrammarHandle;
use crate::parse::finish;
use pg_parse::WordAnalysis;

/// `hc_generate_words(HcGrammarHandle, const uint32_t* morpheme_ids, size_t morpheme_count,
/// int32_t root_morpheme_index, HcResultBuf* out)` (W7). `morpheme_ids` is the grammar-tier
/// `MorphemeId` ordinal sequence exactly as `hc_parse_word`/`hc_parse_batch` already emit it (see
/// `buffer` module docs' `morpheme_ids` field) — typically round-tripped straight from a prior
/// parse's decoded `structured` analysis, unchanged. `root_morpheme_index` is that same analysis's
/// root index into the sequence (`-1` or out-of-range yields zero words, matching
/// `Morpher::generate_words_from_analysis`'s own defensive empty-result handling — see that
/// method's doc for why this differs from C#'s unchecked array-index cast).
///
/// Returns/leaves `*out` under the same contract as `crate::parse::hc_parse_word`: `HC_OK` (0) on
/// success (an `hc_generate_words`-specific buffer, see `buffer::encode_generated_words`), otherwise
/// a nonzero `HC_ERR_*` code with `*out` left as `HcResultBuf::EMPTY`. The caller should call
/// `hc_buf_free(out)` unconditionally, exactly as for the other result-producing entry points.
///
/// # Safety
/// `handle` must be a still-valid handle from `hc_grammar_load` (or null, an error not UB).
/// `morpheme_ids` must point to `morpheme_count` readable `u32`s (or be null iff `morpheme_count ==
/// 0`). `out` must be a valid pointer to `HcResultBuf` storage for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn hc_generate_words(
    handle: HcGrammarHandle,
    morpheme_ids: *const u32,
    morpheme_count: usize,
    root_morpheme_index: i32,
    out: *mut HcResultBuf,
) -> i32 {
    let result = std::panic::catch_unwind(|| -> Result<Vec<u8>, i32> {
        // SAFETY: `handle` validity is this function's documented precondition.
        let gh = unsafe { crate::grammar::borrow(handle) }.ok_or(HC_ERR_NULL_ARG)?;
        if morpheme_count > 0 && morpheme_ids.is_null() {
            return Err(HC_ERR_NULL_ARG);
        }
        // SAFETY: `morpheme_ids`/`morpheme_count` validity is this function's documented
        // precondition; the null+zero-length case never dereferences the pointer.
        let ids: &[u32] = if morpheme_count == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(morpheme_ids, morpheme_count) }
        };
        let wa = WordAnalysis {
            morpheme_ids: ids.to_vec(),
            morph_occurrences: Vec::new(),
            root_morpheme_index,
            pos_id: None,
            syn_fs: Default::default(),
            mpr: Default::default(),
            guessed: false,
            guessed_string: None,
            provenance: pg_parse::AnalysisProvenance::Grammar,
            supplied_root: None,
            morpheme_roots: vec![None; ids.len()],
        };
        let words = gh.morpher.generate_words_from_analysis(&wa);
        Ok(crate::buffer::encode_generated_words(&words))
    });
    finish(result, out)
}

#[cfg(test)]
mod tests;
