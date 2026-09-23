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
    let rc =
        unsafe { crate::grammar::hc_grammar_load(xml.as_ptr(), xml.len(), &mut handle, &mut err) };
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
