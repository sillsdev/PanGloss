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
            root_morpheme_index,
            pos_id: None,
            syn_fs: Default::default(),
            mpr: Default::default(),
            guessed: false,
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
mod tests {
    use super::*;
    use crate::buffer::decode_generated_words;
    use crate::error::HC_OK;
    use crate::grammar::hc_grammar_free;

    /// A minimal one-stratum grammar exercising `hc_generate_words` end-to-end through the real FFI boundary, independent of `csharp_port_common`'s shared test lexicon (this crate has no `pg-parse` dev-dependency).
    const GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>FfiGenerate</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>t1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cBdry"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrEd">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
            <MorphologicalSubrules><MorphologicalSubrule id="subEd">
              <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+b</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eR" partOfSpeech="posV"><MorphemeId>R</MorphemeId>
            <Allomorphs><Allomorph id="aR"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    unsafe fn load() -> HcGrammarHandle {
        let mut handle: HcGrammarHandle = std::ptr::null_mut();
        let mut err = crate::error::HcError::EMPTY;
        let xml = GRAMMAR_XML.as_bytes();
        let rc = unsafe {
            crate::grammar::hc_grammar_load(xml.as_ptr(), xml.len(), &mut handle, &mut err)
        };
        assert_eq!(rc, HC_OK, "grammar load failed");
        handle
    }

    #[test]
    fn generates_root_plus_suffix() {
        // `load_stratum` visits `MorphologicalRuleDefinitions` before `LexicalEntries`, so "PAST" is morpheme 0 and "R" is morpheme 1.
        unsafe {
            let handle = load();
            let ids: [u32; 2] = [1, 0]; // [root "R", other "PAST"]
            let mut out = HcResultBuf::EMPTY;
            let rc = hc_generate_words(handle, ids.as_ptr(), ids.len(), 0, &mut out);
            assert_eq!(rc, HC_OK, "hc_generate_words failed");
            let bytes = std::slice::from_raw_parts(out.data, out.len);
            let words = decode_generated_words(bytes).expect("decodes");
            assert_eq!(words, vec!["ab".to_string()]); // "a" + "+b", boundary stripped
            crate::parse::hc_buf_free(&mut out);
            hc_grammar_free(handle);
        }
    }

    #[test]
    fn null_handle_is_null_arg_error() {
        unsafe {
            let ids: [u32; 1] = [0];
            let mut out = HcResultBuf::EMPTY;
            let rc = hc_generate_words(std::ptr::null_mut(), ids.as_ptr(), 1, 0, &mut out);
            assert_eq!(rc, HC_ERR_NULL_ARG);
            assert!(out.data.is_null());
        }
    }

    #[test]
    fn out_of_range_root_index_yields_empty_not_panic() {
        unsafe {
            let handle = load();
            let ids: [u32; 2] = [0, 1];
            let mut out = HcResultBuf::EMPTY;
            let rc = hc_generate_words(handle, ids.as_ptr(), ids.len(), 99, &mut out);
            assert_eq!(rc, HC_OK);
            let bytes = std::slice::from_raw_parts(out.data, out.len);
            let words = decode_generated_words(bytes).expect("decodes");
            assert!(words.is_empty());
            crate::parse::hc_buf_free(&mut out);
            hc_grammar_free(handle);
        }
    }
}
