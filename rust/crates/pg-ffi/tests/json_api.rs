use pangloss_ffi::*;
use serde_json::{json, Value};
use std::ffi::c_void;

const XML: &str = r#"<HermitCrabInput><Language><Name>FfiJson</Name><PartsOfSpeech><PartOfSpeech id="posN"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech><CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions><SegmentDefinition id="a"><Representations><Representation>a</Representation></Representations></SegmentDefinition><SegmentDefinition id="b"><Representations><Representation>b</Representation></Representations></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable><Strata><Stratum characterDefinitionTable="t"><Name>S</Name><LexicalEntries><LexicalEntry id="official-a" partOfSpeech="posN"><Allomorphs><Allomorph id="aa"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#;

fn load() -> *mut c_void {
    let mut handle = std::ptr::null_mut();
    let mut error = HcError::EMPTY;
    assert_eq!(
        unsafe { hc_grammar_load(XML.as_ptr(), XML.len(), &mut handle, &mut error) },
        HC_OK
    );
    handle
}

unsafe fn call(
    f: unsafe extern "C" fn(HcGrammarHandle, *const u8, usize, *mut HcResultBuf) -> i32,
    handle: HcGrammarHandle,
    request: &Value,
) -> Value {
    let bytes = serde_json::to_vec(request).unwrap();
    let mut out = HcResultBuf::EMPTY;
    assert_eq!(
        unsafe { f(handle, bytes.as_ptr(), bytes.len(), &mut out) },
        HC_OK
    );
    assert!(!out.data.is_null());
    let value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    unsafe { hc_buf_free(&mut out) };
    assert!(out.data.is_null());
    value
}

#[test]
fn abi_version_preserves_binary_parse_and_exposes_structured_json_errors() {
    // ABI v3: hc_parse_word's binary wire format and the JSON API are untouched by the guess-opt-in pair.
    assert_eq!(hc_abi_version(), 3);
    let handle = load();
    let mut binary = HcResultBuf::EMPTY;
    assert_eq!(
        unsafe { hc_parse_word(handle, b"a".as_ptr(), 1, &mut binary) },
        HC_OK
    );
    assert!(decode(unsafe { std::slice::from_raw_parts(binary.data, binary.len) }).is_some());
    unsafe { hc_buf_free(&mut binary) };

    let mut out = HcResultBuf::EMPTY;
    assert_eq!(
        unsafe { hc_lexicon_add_json(handle, b"{".as_ptr(), 1, &mut out) },
        HC_OK
    );
    let value: Value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "invalid_json");
    unsafe {
        hc_buf_free(&mut out);
        hc_grammar_free(handle)
    };
}

#[test]
fn invalid_utf8_is_an_enveloped_error_but_null_nonempty_input_is_transport_failure() {
    let handle = load();
    let invalid = [0xff];
    let mut out = HcResultBuf::EMPTY;
    assert_eq!(
        unsafe { hc_lexicon_add_json(handle, invalid.as_ptr(), 1, &mut out) },
        HC_OK
    );
    let value: Value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    assert_eq!(value["error"]["code"], "invalid_utf8");
    unsafe { hc_buf_free(&mut out) };
    assert_eq!(
        unsafe { hc_lexicon_add_json(handle, std::ptr::null(), 1, &mut out) },
        HC_ERR_NULL_ARG
    );
    assert!(out.data.is_null());
    unsafe { hc_grammar_free(handle) };
}

#[test]
fn catalog_crud_search_export_import_and_provenance_flow() {
    let handle = load();
    let catalog = unsafe { call(hc_lexicon_catalog_json, handle, &json!({})) };
    let signature = catalog["value"]["signatures"][0]["id"].as_str().unwrap();
    let language = unsafe {
        call(
            hc_lexicon_set_gloss_language_json,
            handle,
            &json!({"glossLanguage":"en"}),
        )
    };
    let revision = language["value"]["revision"].as_str().unwrap();
    let added = unsafe {
        call(
            hc_lexicon_add_json,
            handle,
            &json!({"stem":"b","gloss":"bee","signatures":[signature],"expectedRevision":revision}),
        )
    };
    let id = added["value"]["value"]["id"].as_str().unwrap();
    let revision = added["value"]["revision"].as_str().unwrap();
    assert_eq!(
        unsafe { call(hc_lexicon_get_json, handle, &json!({"id":id})) }["value"]["stem"],
        "b"
    );
    assert_eq!(
        unsafe { call(hc_lexicon_list_json, handle, &json!({})) }["value"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        unsafe { call(hc_lexicon_search_json, handle, &json!({"query":"bee"})) }["value"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let updated = unsafe {
        call(
            hc_lexicon_update_json,
            handle,
            &json!({"id":id,"stem":"b","gloss":"letter bee","signatures":[signature],"expectedRevision":revision}),
        )
    };
    assert_eq!(updated["value"]["changed"], true);
    let authority = unsafe {
        call(
            hc_lexicon_set_authority_json,
            handle,
            &json!({"id":id,"authority":"supplied","expectedRevision":updated["value"]["revision"]}),
        )
    };
    assert_eq!(authority["value"]["changed"], false);
    let analysis = unsafe { call(hc_analyze_word_json, handle, &json!({"word":"b"})) };
    assert_eq!(
        analysis["value"]["structured"][0]["provenance"]["kind"],
        "supplied"
    );
    assert_eq!(
        analysis["value"]["structured"][0]["provenance"]["entryId"],
        id
    );
    let exported = unsafe { call(hc_lexicon_export_json, handle, &json!({})) };
    assert_eq!(
        unsafe { call(hc_lexicon_remove_json, handle, &json!({"id":id})) }["value"]["changed"],
        true
    );
    let imported = unsafe {
        call(
            hc_lexicon_import_json,
            handle,
            &json!({"document": exported["value"]}),
        )
    };
    assert_eq!(imported["value"]["changed"], true);
    assert_eq!(
        unsafe { call(hc_lexicon_clear_json, handle, &json!({})) }["value"]["value"],
        1
    );
    unsafe { hc_grammar_free(handle) };
}

#[test]
fn classification_and_guide_handles_have_explicit_lifetimes() {
    let handle = load();
    let matrix = unsafe { call(hc_classification_matrix_json, handle, &json!({"stem":"b"})) };
    let bytes = serde_json::to_vec(&matrix["value"]).unwrap();
    let mut guide = std::ptr::null_mut();
    let mut out = HcResultBuf::EMPTY;
    assert_eq!(
        unsafe {
            hc_classification_guide_new_json(bytes.as_ptr(), bytes.len(), &mut guide, &mut out)
        },
        HC_OK
    );
    assert!(!guide.is_null());
    unsafe { hc_buf_free(&mut out) };
    let remaining =
        unsafe { call_guide(hc_classification_guide_remaining_json, guide, &json!({})) };
    assert!(remaining["value"].is_array());
    assert!(
        unsafe { call_guide(hc_classification_guide_next_json, guide, &json!({})) }["ok"]
            .as_bool()
            .unwrap()
    );
    assert!(
        unsafe { call_guide(hc_classification_guide_useful_json, guide, &json!({})) }["value"]
            .is_array()
    );
    assert!(
        unsafe { call_guide(hc_classification_guide_selection_json, guide, &json!({})) }["value"]
            .is_object()
    );
    assert_eq!(
        unsafe {
            call_guide(
                hc_classification_guide_answer_json,
                guide,
                &json!({"formId":"missing","judgment":"yes"}),
            )
        }["error"]["code"],
        "unknown_form"
    );
    assert_eq!(
        unsafe { call_guide(hc_classification_guide_undo_json, guide, &json!({})) }["value"],
        false
    );
    unsafe {
        hc_classification_guide_free(guide);
        hc_grammar_free(handle)
    };
}

#[test]
fn json_reads_and_mutations_can_share_a_live_handle() {
    let handle = load() as usize;
    let mut threads = Vec::new();
    for _ in 0..8 {
        threads.push(std::thread::spawn(move || {
            let h = handle as HcGrammarHandle;
            for _ in 0..20 {
                let value = unsafe { call(hc_lexicon_list_json, h, &json!({})) };
                assert_eq!(value["ok"], true);
            }
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
    unsafe { hc_grammar_free(handle as HcGrammarHandle) };
}

#[test]
fn shared_binding_fixture_normalizes_native_json_contract() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tools/fixtures/supplied-lexicon-binding.json"
    ))
    .unwrap();
    let handle = load_handle_from_xml(fixture["grammarXml"].as_str().unwrap());
    let catalog = unsafe { call(hc_lexicon_catalog_json, handle, &json!({})) };
    let signature = catalog["value"]["signatures"][0]["id"].as_str().unwrap();
    let invalid = unsafe {
        call(
            hc_lexicon_add_json,
            handle,
            &json!({"stem":"","gloss":"","signatures":[signature]}),
        )
    };
    let gloss = unsafe {
        call(
            hc_lexicon_set_gloss_language_json,
            handle,
            &json!({"glossLanguage":"en"}),
        )
    };
    let added = unsafe {
        call(
            hc_lexicon_add_json,
            handle,
            &json!({"stem":"b","gloss":"bee","signatures":[signature],"expectedRevision":gloss["value"]["revision"]}),
        )
    };
    let id = added["value"]["value"]["id"].as_str().unwrap();
    let add_revision = added["value"]["revision"].clone();
    let get = unsafe { call(hc_lexicon_get_json, handle, &json!({"id":id})) };
    let list = unsafe { call(hc_lexicon_list_json, handle, &json!({})) };
    let search = unsafe {
        call(
            hc_lexicon_search_json,
            handle,
            &json!({"query":"bee","signature":signature,"state":"active","pos":"posN"}),
        )
    };
    let conflict = unsafe {
        call(
            hc_lexicon_update_json,
            handle,
            &json!({"id":id,"stem":"b","gloss":"letter bee","signatures":[signature],"expectedRevision":"rev_0"}),
        )
    };
    let updated = unsafe {
        call(
            hc_lexicon_update_json,
            handle,
            &json!({"id":id,"stem":"b","gloss":"letter bee","signatures":[signature],"expectedRevision":add_revision}),
        )
    };
    let authority = unsafe {
        call(
            hc_lexicon_set_authority_json,
            handle,
            &json!({"id":id,"authority":"supplied","expectedRevision":updated["value"]["revision"]}),
        )
    };
    let exported = unsafe { call(hc_lexicon_export_json, handle, &json!({})) };
    let matrix = unsafe { call(hc_classification_matrix_json, handle, &json!({"stem":"b"})) };
    let mut guide_matrix = matrix["value"].clone();
    guide_matrix["forms"] = json!([{"id":"form-1","surface":"bs","predictions":[{"signatureId":signature,"derivations":[[{"id":"rule-pl","label":"plural"}]]}]}]);
    let matrix_bytes = serde_json::to_vec(&guide_matrix).unwrap();
    let mut guide = std::ptr::null_mut();
    let mut guide_out = HcResultBuf::EMPTY;
    assert_eq!(
        unsafe {
            hc_classification_guide_new_json(
                matrix_bytes.as_ptr(),
                matrix_bytes.len(),
                &mut guide,
                &mut guide_out,
            )
        },
        HC_OK
    );
    unsafe { hc_buf_free(&mut guide_out) };
    let guide_remaining =
        unsafe { call_guide(hc_classification_guide_remaining_json, guide, &json!({})) };
    let guide_next = unsafe { call_guide(hc_classification_guide_next_json, guide, &json!({})) };
    let guide_useful =
        unsafe { call_guide(hc_classification_guide_useful_json, guide, &json!({})) };
    let guide_selection =
        unsafe { call_guide(hc_classification_guide_selection_json, guide, &json!({})) };
    let guide_answer = unsafe {
        call_guide(
            hc_classification_guide_answer_json,
            guide,
            &json!({"formId":"form-1","judgment":"yes"}),
        )
    };
    let guide_after_answer =
        unsafe { call_guide(hc_classification_guide_remaining_json, guide, &json!({})) };
    let guide_undo = unsafe { call_guide(hc_classification_guide_undo_json, guide, &json!({})) };
    let guide_error = unsafe {
        call_guide(
            hc_classification_guide_answer_json,
            guide,
            &json!({"formId":"missing","judgment":"yes"}),
        )
    };
    unsafe { hc_classification_guide_free(guide) };
    let supplied_analysis = unsafe { call(hc_analyze_word_json, handle, &json!({"word":"b"})) };
    let grammar_analysis = unsafe { call(hc_analyze_word_json, handle, &json!({"word":"a"})) };
    let removed = unsafe {
        call(
            hc_lexicon_remove_json,
            handle,
            &json!({"id":id,"expectedRevision":authority["value"]["revision"]}),
        )
    };
    let imported = unsafe {
        call(
            hc_lexicon_import_json,
            handle,
            &json!({"document":exported["value"]}),
        )
    };
    let after_import = unsafe { call(hc_lexicon_list_json, handle, &json!({})) };
    let cleared = unsafe {
        call(
            hc_lexicon_clear_json,
            handle,
            &json!({"expectedRevision":imported["value"]["revision"]}),
        )
    };
    let after_clear = unsafe { call(hc_lexicon_list_json, handle, &json!({})) };
    let restored = unsafe {
        call(
            hc_lexicon_import_json,
            handle,
            &json!({"document":exported["value"]}),
        )
    };
    let after_restore = unsafe { call(hc_lexicon_list_json, handle, &json!({})) };

    let case_handle = load_handle_from_xml(fixture["grammarXml"].as_str().unwrap());
    let case_catalog = unsafe { call(hc_lexicon_catalog_json, case_handle, &json!({})) };
    let case_signature = case_catalog["value"]["signatures"][0]["id"]
        .as_str()
        .unwrap();
    let case_added = unsafe {
        call(
            hc_lexicon_add_json,
            case_handle,
            &json!({"stem":"B","gloss":"","signatures":[case_signature]}),
        )
    };
    let case_id = case_added["value"]["value"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("uppercase add failed: {case_added}"));
    let case_get = unsafe { call(hc_lexicon_get_json, case_handle, &json!({"id":case_id})) };
    let case_list = unsafe { call(hc_lexicon_list_json, case_handle, &json!({})) };
    let case_search = unsafe { call(hc_lexicon_search_json, case_handle, &json!({"query":"B"})) };
    let case_export = unsafe { call(hc_lexicon_export_json, case_handle, &json!({})) };
    let case_analysis = unsafe { call(hc_analyze_word_json, case_handle, &json!({"word":"B"})) };
    let transcript = json!({
        "catalog": catalog["value"], "invalidAdd": invalid["error"], "setGlossLanguage": gloss["value"],
        "add": added["value"], "get": get["value"], "list": list["value"], "search": search["value"],
        "revisionConflict": conflict["error"], "update": updated["value"], "setAuthority": authority["value"],
        "export": exported["value"], "classificationMatrix": matrix["value"],
        "guide": {"remaining":guide_remaining["value"],"next":guide_next["value"],"useful":guide_useful["value"],"selection":guide_selection["value"],"answer":guide_answer["value"],"afterAnswer":guide_after_answer["value"],"undo":guide_undo["value"],"invalidAnswer":guide_error["error"]},
        "analysis":{"supplied":supplied_analysis["value"],"grammar":grammar_analysis["value"]},
        "remove":removed["value"], "import":imported["value"], "afterImport":after_import["value"],
        "clear":cleared["value"], "afterClear":after_clear["value"], "restore":restored["value"], "afterRestore":after_restore["value"],
        "authoredCase":{"add":case_added["value"],"get":case_get["value"],"list":case_list["value"],"search":case_search["value"],"export":case_export["value"],"analysis":case_analysis["value"]}
    });
    let normalized = normalize_binding(transcript, signature);
    let expected =
        expand_fixture_refs(fixture["expectedTranscript"].clone(), &fixture["fragments"]);
    assert_eq!(normalized, expected);
    unsafe {
        hc_grammar_free(case_handle);
        hc_grammar_free(handle);
    };
}

fn normalize_binding(value: Value, signature: &str) -> Value {
    fn walk(value: Value, signature: &str, key: Option<&str>) -> Value {
        match value {
            Value::String(text) if text == signature => json!("$signature"),
            Value::String(text) if text.starts_with("pgl_") => json!("$entry"),
            Value::String(_) if matches!(key, Some("dateCreated" | "dateModified")) => {
                json!("$date")
            }
            Value::String(_)
                if matches!(key, Some("grammarFingerprint" | "sourceGrammarFingerprint")) =>
            {
                json!("$grammarFingerprint")
            }
            Value::String(_) if matches!(key, Some("buildFingerprint")) => {
                json!("$buildFingerprint")
            }
            Value::Array(items) => Value::Array(
                items
                    .into_iter()
                    .map(|v| walk(v, signature, None))
                    .collect(),
            ),
            Value::Object(items) => Value::Object(
                items
                    .into_iter()
                    .map(|(k, v)| {
                        let normalized_key = if k == signature {
                            "$signature".into()
                        } else {
                            k
                        };
                        let normalized_value = walk(v, signature, Some(&normalized_key));
                        (normalized_key, normalized_value)
                    })
                    .collect(),
            ),
            other => other,
        }
    }
    walk(value, signature, None)
}

fn expand_fixture_refs(value: Value, fragments: &Value) -> Value {
    match value {
        Value::Object(ref map) if map.len() == 1 && map.contains_key("$ref") => {
            let name = map["$ref"].as_str().unwrap();
            expand_fixture_refs(fragments[name].clone(), fragments)
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|v| expand_fixture_refs(v, fragments))
                .collect(),
        ),
        Value::Object(items) => Value::Object(
            items
                .into_iter()
                .map(|(k, v)| (k, expand_fixture_refs(v, fragments)))
                .collect(),
        ),
        other => other,
    }
}

fn load_handle_from_xml(xml: &str) -> HcGrammarHandle {
    let mut handle = std::ptr::null_mut();
    let mut error = HcError::EMPTY;
    assert_eq!(
        unsafe { hc_grammar_load(xml.as_ptr(), xml.len(), &mut handle, &mut error) },
        HC_OK
    );
    handle
}

unsafe fn call_guide(
    f: unsafe extern "C" fn(HcClassificationGuideHandle, *const u8, usize, *mut HcResultBuf) -> i32,
    guide: HcClassificationGuideHandle,
    request: &Value,
) -> Value {
    let bytes = serde_json::to_vec(request).unwrap();
    let mut out = HcResultBuf::EMPTY;
    assert_eq!(
        unsafe { f(guide, bytes.as_ptr(), bytes.len(), &mut out) },
        HC_OK
    );
    let value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    unsafe { hc_buf_free(&mut out) };
    value
}
