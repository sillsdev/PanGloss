//! GrammarHandle construction must remain inside the C ABI panic boundary.

use std::ffi::c_void;

use pangloss_ffi::{hc_buf_free, hc_grammar_load, HcError, HC_ERR_PANIC};

const MINIMAL_XML: &str = r#"<HermitCrabInput><Language><Name>LoadBoundary</Name><PartsOfSpeech><PartOfSpeech id="posN"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech><CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions/></CharacterDefinitionTable><Strata><Stratum characterDefinitionTable="t"><Name>S</Name><LexicalEntries/></Stratum></Strata></Language></HermitCrabInput>"#;

fn assert_load_returns_error(xml: &str) {
    let mut handle: *mut c_void = std::ptr::null_mut();
    let mut error = HcError::EMPTY;
    let code = unsafe { hc_grammar_load(xml.as_ptr(), xml.len(), &mut handle, &mut error) };

    assert_eq!(
        code, HC_ERR_PANIC,
        "construction failure must be caught at the ABI boundary"
    );
    assert!(handle.is_null(), "failed load published a handle");
    unsafe { hc_buf_free(&mut error.message) };
}

#[test]
fn missing_language_name_does_not_unwind_across_c_abi() {
    assert_load_returns_error(&MINIMAL_XML.replace("<Name>LoadBoundary</Name>", ""));
}

#[test]
fn blank_language_name_does_not_unwind_across_c_abi() {
    assert_load_returns_error(&MINIMAL_XML.replace("LoadBoundary", "  \n  "));
}
