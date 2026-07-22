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
typedef struct { int32_t code; int32_t _pad; HcResultBuf message; } HcError;
typedef struct { const uint8_t *ptr; size_t len; } HcStr;

enum {
    HC_OK = 0,
    HC_ERR_UTF8 = 1,
    HC_ERR_GRAMMAR_LOAD = 2,
    HC_ERR_NULL_ARG = 3,
    HC_ERR_PANIC = 4,
    HC_ERR_INVALID_ARG = 5
};

int32_t hc_abi_version(void); /* 2: binary parse ABI unchanged; JSON API added. */
int32_t hc_grammar_load(const uint8_t *xml_utf8, size_t len, HcGrammarHandle *out, HcError *error);
void hc_grammar_free(HcGrammarHandle handle);
int32_t hc_parse_word(HcGrammarHandle handle, const uint8_t *word_utf8, size_t len, HcResultBuf *out);
int32_t hc_parse_batch(HcGrammarHandle handle, const HcStr *words, size_t count, int32_t max_threads, HcResultBuf *out);
int32_t hc_generate_words(HcGrammarHandle handle, const uint32_t *morpheme_ids, size_t morpheme_count, int32_t root_morpheme_index, HcResultBuf *out);
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
