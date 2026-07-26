#include "pangloss.h"

#if defined(__cplusplus)
#error "C smoke must compile as C"
#endif

typedef char assert_result_buf_layout[(sizeof(HcResultBuf) == 3 * sizeof(size_t)) ? 1 : -1];
typedef char assert_error_layout[(offsetof(HcError, message) == 8) ? 1 : -1];
typedef char assert_str_layout[(sizeof(HcStr) == 2 * sizeof(size_t)) ? 1 : -1];

void pangloss_header_c_smoke(void) {
    HcGrammarHandle handle = NULL;
    HcError error = {0};
    (void)hc_abi_version();
    (void)hc_grammar_load(NULL, 0, &handle, &error);
    hc_buf_free(&error.message);
    hc_grammar_free(handle);

    (void)&hc_parse_word; (void)&hc_parse_batch; (void)&hc_generate_words;
    (void)&hc_parse_word_opts; (void)&hc_parse_batch_opts;
    (void)&hc_lexicon_catalog_json; (void)&hc_lexicon_add_json;
    (void)&hc_lexicon_get_json; (void)&hc_lexicon_list_json;
    (void)&hc_lexicon_search_json; (void)&hc_lexicon_update_json;
    (void)&hc_lexicon_remove_json; (void)&hc_lexicon_clear_json;
    (void)&hc_lexicon_set_gloss_language_json; (void)&hc_lexicon_set_authority_json;
    (void)&hc_lexicon_import_json; (void)&hc_lexicon_export_json;
    (void)&hc_classification_matrix_json; (void)&hc_analyze_word_json;
    (void)&hc_classification_guide_new_json; (void)&hc_classification_guide_answer_json;
    (void)&hc_classification_guide_undo_json; (void)&hc_classification_guide_remaining_json;
    (void)&hc_classification_guide_next_json; (void)&hc_classification_guide_useful_json;
    (void)&hc_classification_guide_selection_json; (void)&hc_classification_guide_free;
}

int main(void) {
    pangloss_header_c_smoke();
    return 0;
}
