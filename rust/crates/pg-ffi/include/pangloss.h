#ifndef PANGLOSS_H
#define PANGLOSS_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void *HcGrammarHandle;
typedef void *HcClassificationGuideHandle;
typedef struct { uint8_t *data; size_t len; size_t cap; } HcResultBuf;

int32_t hc_abi_version(void); /* 2: binary parse ABI unchanged; JSON API added. */
void hc_buf_free(HcResultBuf *buf);

/* All request strings are length-delimited UTF-8 and need not be NUL-terminated. JSON calls
 * return a PanGloss-owned UTF-8 envelope: {"ok":true,"value":...} or
 * {"ok":false,"error":{"code":...,"message":...,"details":...}}. Free every nonempty
 * output with hc_buf_free. Empty output is reserved for a transport error. */
#define HC_GRAMMAR_JSON_FN(name) int32_t name(HcGrammarHandle, const uint8_t *, size_t, HcResultBuf *)
HC_GRAMMAR_JSON_FN(hc_lexicon_catalog_json);
HC_GRAMMAR_JSON_FN(hc_lexicon_add_json);
HC_GRAMMAR_JSON_FN(hc_lexicon_get_json);
HC_GRAMMAR_JSON_FN(hc_lexicon_list_json);
HC_GRAMMAR_JSON_FN(hc_lexicon_search_json);
HC_GRAMMAR_JSON_FN(hc_lexicon_update_json);
HC_GRAMMAR_JSON_FN(hc_lexicon_remove_json);
HC_GRAMMAR_JSON_FN(hc_lexicon_clear_json);
HC_GRAMMAR_JSON_FN(hc_lexicon_set_gloss_language_json);
HC_GRAMMAR_JSON_FN(hc_lexicon_set_authority_json);
HC_GRAMMAR_JSON_FN(hc_lexicon_import_json);
HC_GRAMMAR_JSON_FN(hc_lexicon_export_json);
HC_GRAMMAR_JSON_FN(hc_classification_matrix_json);
HC_GRAMMAR_JSON_FN(hc_analyze_word_json);
#undef HC_GRAMMAR_JSON_FN

int32_t hc_classification_guide_new_json(const uint8_t *, size_t, HcClassificationGuideHandle *, HcResultBuf *);
#define HC_GUID_JSON_FN(name) int32_t name(HcClassificationGuideHandle, const uint8_t *, size_t, HcResultBuf *)
HC_GUID_JSON_FN(hc_classification_guide_answer_json);
HC_GUID_JSON_FN(hc_classification_guide_undo_json);
HC_GUID_JSON_FN(hc_classification_guide_remaining_json);
HC_GUID_JSON_FN(hc_classification_guide_next_json);
HC_GUID_JSON_FN(hc_classification_guide_useful_json);
HC_GUID_JSON_FN(hc_classification_guide_selection_json);
#undef HC_GUID_JSON_FN
void hc_classification_guide_free(HcClassificationGuideHandle);

#ifdef __cplusplus
}
#endif
#endif
