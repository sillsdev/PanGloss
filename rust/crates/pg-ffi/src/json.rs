//! Length-delimited JSON boundary for the supplied lexicon API.

use crate::error::{write_buf, write_empty_buf, HcResultBuf, HC_ERR_NULL_ARG, HC_OK};
use crate::grammar::{self, HcGrammarHandle};
use pg_lexicon::{
    AddRequest, ClassificationGuide, ClassificationMatrix, ClassificationRequest, EntryId,
    ExpectedRevision, ImportRequest, Judgment, RemoveRequest, SearchRequest, SetAuthorityRequest,
    SetGlossLanguageRequest, StructuredError, UpdateRequest,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;

pub type HcClassificationGuideHandle = *mut std::ffi::c_void;

struct GuideHandle {
    guide: Mutex<ClassificationGuide>,
    #[cfg(test)]
    force_next_panic: std::sync::atomic::AtomicBool,
}

impl GuideHandle {
    fn new(guide: ClassificationGuide) -> Self {
        Self {
            guide: Mutex::new(guide),
            #[cfg(test)]
            force_next_panic: std::sync::atomic::AtomicBool::new(false),
        }
    }

    #[cfg(test)]
    fn new_for_test(guide: ClassificationGuide) -> Self {
        Self::new(guide)
    }

    #[cfg(test)]
    fn force_next_panic_for_test(&self) {
        self.force_next_panic
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn panic_if_requested(&self) {
        #[cfg(test)]
        if self
            .force_next_panic
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            panic!("forced classification guide panic under mutex");
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum Envelope<T: Serialize> {
    Ok { ok: bool, value: T },
    Err { ok: bool, error: StructuredError },
}

fn error(code: &str, message: impl Into<String>, details: Value) -> StructuredError {
    StructuredError {
        code: code.into(),
        message: message.into(),
        details,
    }
}

fn encode<T: Serialize>(result: Result<T, StructuredError>) -> Vec<u8> {
    match result {
        Ok(value) => serde_json::to_vec(&Envelope::Ok { ok: true, value }),
        Err(error) => serde_json::to_vec(&Envelope::<Value>::Err { ok: false, error }),
    }
    .expect("JSON envelopes are serializable")
}

unsafe fn json_call<R, T, F>(
    request: *const u8,
    len: usize,
    out: *mut HcResultBuf,
    operation: F,
) -> i32
where
    R: DeserializeOwned,
    T: Serialize,
    F: FnOnce(R) -> Result<T, StructuredError>,
{
    if out.is_null() {
        return HC_ERR_NULL_ARG;
    }
    if len > 0 && request.is_null() {
        write_empty_buf(out);
        return HC_ERR_NULL_ARG;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let bytes = if len == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(request, len) }
        };
        let text = std::str::from_utf8(bytes).map_err(|e| {
            error(
                "invalid_utf8",
                "request is not valid UTF-8",
                json!({"error":e.to_string()}),
            )
        })?;
        let request = serde_json::from_str(text).map_err(|e| {
            error(
                "invalid_json",
                "request is not valid JSON",
                json!({"error":e.to_string()}),
            )
        })?;
        operation(request)
    }));
    let bytes = match result {
        Ok(value) => encode(value),
        Err(payload) => encode::<Value>(Err(error(
            "panic",
            "native operation panicked",
            json!({"message":crate::error::panic_message(payload)}),
        ))),
    };
    write_buf(out, bytes);
    HC_OK
}

unsafe fn grammar_call<R, T, F>(
    handle: HcGrammarHandle,
    request: *const u8,
    len: usize,
    out: *mut HcResultBuf,
    operation: F,
) -> i32
where
    R: DeserializeOwned,
    T: Serialize,
    F: FnOnce(&grammar::GrammarHandle, R) -> Result<T, StructuredError>,
{
    unsafe {
        json_call(request, len, out, |request| {
            let handle = grammar::borrow(handle)
                .ok_or_else(|| error("null_handle", "grammar handle is null", Value::Null))?;
            operation(handle, request)
        })
    }
}

#[derive(Deserialize)]
struct Empty {}
#[derive(Deserialize)]
struct IdRequest {
    id: EntryId,
}
#[derive(Deserialize)]
struct WordRequest {
    word: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogResponse {
    signatures: Vec<pg_lexicon::ClassSignature>,
    revision: pg_lexicon::Revision,
}

macro_rules! grammar_api {
    ($name:ident, $req:ty, $body:expr) => {
        /// Executes one JSON operation.
        ///
        /// # Safety
        /// The handle must be live; input must address `len` readable bytes (or be null only
        /// when `len == 0`); `out` must address writable result-buffer storage.
        #[no_mangle]
        pub unsafe extern "C" fn $name(
            handle: HcGrammarHandle,
            request: *const u8,
            len: usize,
            out: *mut HcResultBuf,
        ) -> i32 {
            unsafe { grammar_call::<$req, _, _>(handle, request, len, out, $body) }
        }
    };
}

grammar_api!(hc_lexicon_catalog_json, Empty, |h, _| {
    let s = h.runtime.snapshot();
    Ok(CatalogResponse {
        signatures: h.runtime.catalog().signatures().to_vec(),
        revision: s.revision().clone(),
    })
});
grammar_api!(hc_lexicon_add_json, AddRequest, |h, r| h.runtime.add(r));
grammar_api!(hc_lexicon_update_json, UpdateRequest, |h, r| h
    .runtime
    .update(r));
grammar_api!(hc_lexicon_remove_json, RemoveRequest, |h, r| h
    .runtime
    .remove(r));
grammar_api!(hc_lexicon_clear_json, ExpectedRevision, |h, r| h
    .runtime
    .clear(r));
grammar_api!(
    hc_lexicon_set_gloss_language_json,
    SetGlossLanguageRequest,
    |h, r| h.runtime.set_gloss_language(r)
);
grammar_api!(
    hc_lexicon_set_authority_json,
    SetAuthorityRequest,
    |h, r| h.runtime.set_authority(r)
);
grammar_api!(hc_lexicon_export_json, Empty, |h, _| Ok(h
    .runtime
    .export_document()));
grammar_api!(hc_lexicon_import_json, ImportRequest, |h, r| h
    .runtime
    .import(r));
grammar_api!(hc_lexicon_list_json, Empty, |h, _| Ok(h.runtime.list()));
grammar_api!(hc_lexicon_get_json, IdRequest, |h, r| h
    .runtime
    .get(&r.id)
    .ok_or_else(|| error(
        "entry_not_found",
        "entry not found",
        Value::Null
    )));
grammar_api!(hc_lexicon_search_json, SearchRequest, |h, r| {
    Ok(h.runtime.search(&r))
});
grammar_api!(
    hc_classification_matrix_json,
    ClassificationRequest,
    |h, r| pg_lexicon::classify(&h.grammar, h.runtime.catalog(), r)
);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisJson {
    analyses: Vec<(String, String)>,
    structured: Vec<AnalysisItem>,
    capped: bool,
    invalid_shape: bool,
    timed_out: bool,
    guessed: bool,
    candidates_generated: usize,
    revision: pg_lexicon::Revision,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisItem {
    morpheme_ids: Vec<u32>,
    root_morpheme_index: i32,
    pos_id: Option<u32>,
    guessed: bool,
    provenance: pg_parse::AnalysisProvenance,
    supplied_root: Option<pg_parse::SuppliedRoot>,
}
grammar_api!(hc_analyze_word_json, WordRequest, |h, r| {
    // `guess_fallback: true` -- unlike `hc_parse_word`/`hc_parse_batch`'s binary wire format, this
    // JSON envelope already carries `guessed` honestly at both the word level (`AnalysisJson`) and
    // per analysis (`AnalysisItem`), so it explicitly opts back into the retry that
    // `pg_lexicon::SuppliedLexiconRuntime::analyze_word` now defaults off, preserving this
    // endpoint's pre-existing observable behavior rather than silently losing the capability as a
    // side effect of that default flip (see `pg-lexicon`'s `analysis` module doc).
    let a = h.analyze_word(&r.word, true);
    let structured = a
        .structured
        .iter()
        .map(|x| AnalysisItem {
            morpheme_ids: x.morpheme_ids.clone(),
            root_morpheme_index: x.root_morpheme_index,
            pos_id: x.pos_id,
            guessed: x.guessed,
            provenance: x.provenance.clone(),
            supplied_root: x.supplied_root.clone(),
        })
        .collect();
    Ok(AnalysisJson {
        analyses: a.analyses,
        structured,
        capped: a.capped,
        invalid_shape: a.invalid_shape,
        timed_out: a.timed_out,
        guessed: a.guessed,
        candidates_generated: a.candidates_generated,
        revision: a.revision,
    })
});

#[no_mangle]
/// Creates an opaque classification-guide handle from a matrix JSON document.
///
/// # Safety
/// Input must address `len` readable bytes; both output pointers must be valid writable storage.
pub unsafe extern "C" fn hc_classification_guide_new_json(
    request: *const u8,
    len: usize,
    guide_out: *mut HcClassificationGuideHandle,
    out: *mut HcResultBuf,
) -> i32 {
    if guide_out.is_null() {
        if !out.is_null() {
            write_empty_buf(out)
        };
        return HC_ERR_NULL_ARG;
    }
    *guide_out = std::ptr::null_mut();
    unsafe {
        json_call::<ClassificationMatrix, _, _>(request, len, out, |matrix| {
            let guide = Box::new(GuideHandle::new(ClassificationGuide::new(matrix)));
            *guide_out = Box::into_raw(guide).cast();
            Ok(json!({"created":true}))
        })
    }
}
unsafe fn guide_call<R, T, F>(
    handle: HcClassificationGuideHandle,
    request: *const u8,
    len: usize,
    out: *mut HcResultBuf,
    f: F,
) -> i32
where
    R: DeserializeOwned,
    T: Serialize,
    F: FnOnce(&mut ClassificationGuide, R) -> Result<T, StructuredError>,
{
    unsafe {
        json_call(request, len, out, |r| {
            if handle.is_null() {
                return Err(error(
                    "null_handle",
                    "classification guide handle is null",
                    Value::Null,
                ));
            }
            let h = &*(handle as *const GuideHandle);
            let mut g = h
                .guide
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            h.panic_if_requested();
            f(&mut g, r)
        })
    }
}
macro_rules! guide_api {
    ($name:ident,$req:ty,$body:expr) => {
        /// Executes one classification-guide JSON operation.
        ///
        /// # Safety
        /// The guide handle must be live; input must address `len` readable bytes (or be null
        /// only when `len == 0`); `out` must address writable result-buffer storage.
        #[no_mangle]
        pub unsafe extern "C" fn $name(
            handle: HcClassificationGuideHandle,
            request: *const u8,
            len: usize,
            out: *mut HcResultBuf,
        ) -> i32 {
            unsafe { guide_call::<$req, _, _>(handle, request, len, out, $body) }
        }
    };
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnswerRequest {
    form_id: String,
    judgment: Judgment,
}
guide_api!(
    hc_classification_guide_answer_json,
    AnswerRequest,
    |g, r| {
        g.answer(&r.form_id, r.judgment)?;
        Ok(json!({"answered":true}))
    }
);
guide_api!(
    hc_classification_guide_undo_json,
    Empty,
    |g, _| Ok(g.undo())
);
guide_api!(hc_classification_guide_remaining_json, Empty, |g, _| Ok(
    g.remaining_signatures()
));
guide_api!(hc_classification_guide_next_json, Empty, |g, _| Ok(
    g.next_form()
));
guide_api!(hc_classification_guide_useful_json, Empty, |g, _| Ok(
    g.all_useful_forms()
));
guide_api!(hc_classification_guide_selection_json, Empty, |g, _| Ok(
    g.final_selection()
));

#[no_mangle]
/// Frees a classification-guide handle. A null handle is a no-op.
///
/// # Safety
/// A non-null handle must have been returned by `hc_classification_guide_new_json`, must not have
/// been freed, and must not be in use by another thread.
pub unsafe extern "C" fn hc_classification_guide_free(handle: HcClassificationGuideHandle) {
    let _ = std::panic::catch_unwind(|| {
        if !handle.is_null() {
            unsafe { drop(Box::from_raw(handle as *mut GuideHandle)) }
        }
    });
}

#[cfg(test)]
mod tests {
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
                hc_classification_guide_remaining_json(
                    raw,
                    request.as_ptr(),
                    request.len(),
                    &mut out,
                )
            },
            HC_OK
        );
        let first: Value =
            serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) })
                .unwrap();
        assert_eq!(first["error"]["code"], "panic");
        unsafe { crate::hc_buf_free(&mut out) };

        assert_eq!(
            unsafe {
                hc_classification_guide_remaining_json(
                    raw,
                    request.as_ptr(),
                    request.len(),
                    &mut out,
                )
            },
            HC_OK
        );
        let second: Value =
            serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) })
                .unwrap();
        assert_eq!(second["ok"], true);
        unsafe {
            crate::hc_buf_free(&mut out);
            hc_classification_guide_free(raw)
        };
    }
}
