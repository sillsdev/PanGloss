use super::*;

#[test]
fn panic_is_contained_in_a_structured_nonempty_envelope() {
    let request = b"{}";
    let mut out = HcResultBuf::EMPTY;
    let code = unsafe {
        json_call::<Empty, Value, _>(request.as_ptr(), request.len(), &mut out, |_| {
            panic!("test panic")
        })
    };
    assert_eq!(code, HC_OK);
    let bytes = unsafe { std::slice::from_raw_parts(out.data, out.len) };
    let value: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "panic");
    unsafe { crate::parse::hc_buf_free(&mut out) };
}

#[test]
fn poisoned_guide_mutex_recovers_for_the_same_ffi_handle() {
    let matrix = ClassificationMatrix {
        stem: "x".into(),
        candidates: Vec::new(),
        forms: Vec::new(),
        exhaustive: true,
        truncation_reason: None,
    };
    let guide = Box::new(GuideHandle::new_for_test(ClassificationGuide::new(matrix)));
    guide.force_next_panic_for_test();
    let raw = Box::into_raw(guide).cast();
    let request = b"{}";
    let mut out = HcResultBuf::EMPTY;
    assert_eq!(
        unsafe {
            hc_classification_guide_remaining_json(raw, request.as_ptr(), request.len(), &mut out)
        },
        HC_OK
    );
    let first: Value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    assert_eq!(first["error"]["code"], "panic");
    unsafe { crate::hc_buf_free(&mut out) };

    assert_eq!(
        unsafe {
            hc_classification_guide_remaining_json(raw, request.as_ptr(), request.len(), &mut out)
        },
        HC_OK
    );
    let second: Value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    assert_eq!(second["ok"], true);
    unsafe {
        crate::hc_buf_free(&mut out);
        hc_classification_guide_free(raw)
    };
}
